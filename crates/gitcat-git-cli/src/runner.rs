use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::Duration,
};

use gitcat_contracts::{ApiError, ApiResult, ErrorCode};

use crate::credentials::{HostCredential, helper_args, helper_env};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
    sync::mpsc,
};
use tokio_util::sync::CancellationToken;

const DEFAULT_OUTPUT_CAP: usize = 16 * 1024 * 1024;
const ABSOLUTE_OUTPUT_CAP: usize = 128 * 1024 * 1024;
const MAX_INPUT_BYTES: usize = 1024 * 1024;
const STDERR_DETAILS_CAP: usize = 8 * 1024;

struct ProcessTree {
    #[cfg(windows)]
    job: Option<windows_job::Job>,
}

impl ProcessTree {
    fn attach(child: &Child) -> io::Result<Self> {
        #[cfg(windows)]
        {
            Ok(Self {
                job: windows_job::Job::assign(child)?,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = child;
            Ok(Self {})
        }
    }

    fn terminate(&self, child: &mut Child) {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            job.terminate();
        }
        let _ = child.start_kill();
    }
}

#[cfg(windows)]
mod windows_job {
    use std::{ffi::c_void, io, mem::size_of, ptr};

    use tokio::process::Child;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject,
        },
    };

    pub(super) struct Job(HANDLE);

    // The handle is uniquely owned and Windows job APIs are thread-safe.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Job {
        pub(super) fn assign(child: &Child) -> io::Result<Option<Self>> {
            let Some(process) = child.raw_handle() else {
                return Ok(None);
            };
            // SAFETY: every pointer passed here is either null as permitted by
            // the API or points to a live value for the duration of the call.
            unsafe {
                let handle = CreateJobObjectW(ptr::null(), ptr::null());
                if handle.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let job = Self(handle);
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    (&raw const limits).cast::<c_void>(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                ) == 0
                {
                    return Err(io::Error::last_os_error());
                }
                if AssignProcessToJobObject(handle, process as HANDLE) == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Some(job))
            }
        }

        pub(super) fn terminate(&self) {
            // SAFETY: self owns a valid job handle until Drop.
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // KILL_ON_JOB_CLOSE also removes any helper that outlived git.exe.
            // SAFETY: the owned handle is closed exactly once here.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GitRunner {
    executable: PathBuf,
    default_output_cap: usize,
    default_timeout: Option<Duration>,
}

#[derive(Debug, Clone)]
pub(crate) struct GitRunOptions {
    pub read_only: bool,
    pub output_cap: usize,
    pub timeout: Option<Duration>,
    pub allow_failure: bool,
    pub allow_stdout_truncation: bool,
    /// Extra process environment applied after the sanitized baseline. Used to
    /// preserve authorship (`GIT_AUTHOR_*`) when rebuilding commit objects.
    pub extra_env: Vec<(OsString, OsString)>,
    /// Token to offer Git for this one command. Set only for network commands,
    /// and only when the host it belongs to is the host being contacted.
    pub credential: Option<HostCredential>,
    /// Unambiguous HTTPS/HTTP host, even when no credential is stored yet.
    /// Recovery must target the failed command, not the tab open later.
    pub remote_host: Option<String>,
}

impl GitRunOptions {
    pub fn read_only(output_cap: usize) -> Self {
        Self {
            read_only: true,
            output_cap,
            timeout: Some(Duration::from_secs(30)),
            allow_failure: false,
            allow_stdout_truncation: false,
            extra_env: Vec::new(),
            credential: None,
            remote_host: None,
        }
    }

    pub fn mutation(output_cap: usize) -> Self {
        Self {
            read_only: false,
            output_cap,
            timeout: Some(Duration::from_secs(120)),
            allow_failure: false,
            allow_stdout_truncation: false,
            extra_env: Vec::new(),
            credential: None,
            remote_host: None,
        }
    }

    pub fn network(output_cap: usize) -> Self {
        Self {
            read_only: false,
            output_cap,
            timeout: None,
            allow_failure: false,
            allow_stdout_truncation: false,
            extra_env: Vec::new(),
            credential: None,
            remote_host: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct GitCommandOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
}

impl GitCommandOutput {
    pub fn success(&self) -> bool {
        self.status.success()
    }

    pub fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn stderr_lossy_redacted(&self) -> String {
        redact_sensitive(&String::from_utf8_lossy(&self.stderr))
    }
}

impl Default for GitRunner {
    fn default() -> Self {
        Self::new("git")
    }
}

impl GitRunner {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            default_output_cap: DEFAULT_OUTPUT_CAP,
            default_timeout: Some(Duration::from_secs(30)),
        }
    }

    pub async fn run(
        &self,
        cwd: Option<&Path>,
        args: &[OsString],
        stdin: Option<&[u8]>,
        cancellation: CancellationToken,
        mut options: GitRunOptions,
    ) -> ApiResult<GitCommandOutput> {
        options.output_cap = if options.output_cap == 0 {
            self.default_output_cap
        } else {
            options.output_cap
        }
        .clamp(1, ABSOLUTE_OUTPUT_CAP);
        if options.timeout.is_none() && options.read_only {
            options.timeout = self.default_timeout;
        }

        // Installed before the subcommand, because Git reads `-c` overrides
        // only ahead of it.
        let credential_args = options
            .credential
            .as_ref()
            .map(|_| helper_args())
            .unwrap_or_default();

        let mut command = Command::new(&self.executable);
        command
            .arg("--no-pager")
            .arg("-c")
            .arg("color.ui=false")
            .arg("-c")
            .arg("core.quotepath=false")
            .args(credential_args)
            .args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            // C.UTF-8 keeps Git's messages untranslated like plain C, but lets
            // `--regexp-ignore-case` fold non-ASCII letters on Linux, where the
            // C locale only knows ASCII.
            .env("LC_ALL", "C.UTF-8")
            .env("LANG", "C.UTF-8")
            .env("GIT_PAGER", "cat")
            .env("PAGER", "cat")
            .env("TERM", "dumb")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_LITERAL_PATHSPECS", "1")
            .env("GIT_MERGE_AUTOEDIT", "no");

        if options.read_only {
            command.env("GIT_OPTIONAL_LOCKS", "0");
        } else {
            command.env_remove("GIT_OPTIONAL_LOCKS");
        }
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }

        sanitize_git_environment(&mut command);
        // Core operations are non-interactive. Reusing a configured editor can
        // open an invisible process and leave merge/rebase continuations hung.
        command
            .env("GIT_EDITOR", "true")
            .env("GIT_SEQUENCE_EDITOR", "true")
            // Reject external remote helpers, including URLs rewritten through
            // url.<base>.insteadOf. These are the transports GitCat supports.
            .env("GIT_ALLOW_PROTOCOL", "file:git:http:https:ssh");

        // Caller-supplied environment (e.g. preserved authorship) is applied
        // last so it wins over the sanitized baseline. Keys collide only with
        // GIT_AUTHOR_* / GIT_COMMITTER_* which the sanitizer never touches.
        for (key, value) in &options.extra_env {
            command.env(key, value);
        }

        // The token reaches the credential helper this way rather than through
        // an argument, which every process on the machine can read.
        if let Some(credential) = &options.credential {
            for (key, value) in helper_env(credential) {
                command.env(key, value);
            }
        }

        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command
            .spawn()
            .map_err(|error| spawn_error(&self.executable, error))?;
        let process_tree = ProcessTree::attach(&child).map_err(|error| {
            let _ = child.start_kill();
            ApiError::new(ErrorCode::Io, "Git process tree could not be isolated")
                .with_details(error.to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            ApiError::new(ErrorCode::Internal, "Git stdout pipe could not be opened")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            ApiError::new(ErrorCode::Internal, "Git stderr pipe could not be opened")
        })?;

        let (cap_tx, mut cap_rx) = mpsc::channel(2);
        let stdout_task = tokio::spawn(read_capped(
            stdout,
            options.output_cap,
            StreamKind::Stdout,
            cap_tx.clone(),
        ));
        let stderr_task = tokio::spawn(read_capped(
            stderr,
            options.output_cap,
            StreamKind::Stderr,
            cap_tx,
        ));

        if let Some(input) = stdin {
            if input.len() > MAX_INPUT_BYTES {
                process_tree.terminate(&mut child);
                let _ = child.wait().await;
                return Err(ApiError::new(
                    ErrorCode::OutputTooLarge,
                    "Git command input exceeded the safety limit",
                ));
            }
            if let Some(mut child_stdin) = child.stdin.take() {
                let write = async {
                    child_stdin.write_all(input).await?;
                    child_stdin.shutdown().await
                };
                tokio::select! {
                    result = write => result.map_err(io_error)?,
                    _ = cancellation.cancelled() => {
                        process_tree.terminate(&mut child);
                        let _ = child.wait().await;
                        return Err(ApiError::new(ErrorCode::Cancelled, "Git operation was cancelled"));
                    }
                }
            }
        }

        let deadline = options
            .timeout
            .map(|timeout| tokio::time::Instant::now() + timeout);
        let timeout_future = async move {
            if let Some(deadline) = deadline {
                tokio::time::sleep_until(deadline).await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        tokio::pin!(timeout_future);
        let mut cap_channel_open = true;
        let status = loop {
            tokio::select! {
                result = child.wait() => break result.map_err(io_error)?,
                _ = cancellation.cancelled() => {
                    process_tree.terminate(&mut child);
                    let _ = child.wait().await;
                    let _ = stdout_task.await;
                    let _ = stderr_task.await;
                    return Err(ApiError::new(ErrorCode::Cancelled, "Git operation was cancelled"));
                }
                _ = &mut timeout_future => {
                    process_tree.terminate(&mut child);
                    let _ = child.wait().await;
                    let _ = stdout_task.await;
                    let _ = stderr_task.await;
                    return Err(ApiError::new(ErrorCode::Timeout, "Git operation timed out"));
                }
                capped = cap_rx.recv(), if cap_channel_open => {
                    match capped {
                        Some(StreamKind::Stdout) if options.allow_stdout_truncation => {
                            // Keep draining without growing memory; original timeout remains active.
                        }
                        Some(_) => {
                            process_tree.terminate(&mut child);
                            let _ = child.wait().await;
                            let _ = stdout_task.await;
                            let _ = stderr_task.await;
                            return Err(ApiError::new(
                                ErrorCode::OutputTooLarge,
                                "Git command output exceeded the safety limit",
                            ));
                        }
                        None => cap_channel_open = false,
                    }
                }
            }
        };

        let stdout = join_reader(stdout_task).await?;
        let stderr = join_reader(stderr_task).await?;
        if stderr.truncated {
            return Err(ApiError::new(
                ErrorCode::OutputTooLarge,
                "Git diagnostic output exceeded the safety limit",
            ));
        }
        let output = GitCommandOutput {
            status,
            stdout: stdout.bytes,
            stderr: stderr.bytes,
            stdout_truncated: stdout.truncated,
        };

        if output.stdout_truncated && !options.allow_stdout_truncation {
            return Err(ApiError::new(
                ErrorCode::OutputTooLarge,
                "Git command output exceeded the safety limit",
            ));
        }
        if !output.status.success() && !options.allow_failure {
            return Err(self.failure_error(&output));
        }
        Ok(output)
    }

    pub fn failure_error(&self, output: &GitCommandOutput) -> ApiError {
        git_failure_error(output)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamKind {
    Stdout,
    Stderr,
}

struct CappedBytes {
    bytes: Vec<u8>,
    truncated: bool,
}

async fn read_capped<R: AsyncRead + Unpin>(
    mut reader: R,
    cap: usize,
    stream: StreamKind,
    cap_tx: mpsc::Sender<StreamKind>,
) -> io::Result<CappedBytes> {
    let mut bytes = Vec::with_capacity(cap.min(64 * 1024));
    let mut chunk = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        let remaining = cap.saturating_sub(bytes.len());
        if remaining > 0 {
            bytes.extend_from_slice(&chunk[..read.min(remaining)]);
        }
        if read > remaining && !truncated {
            truncated = true;
            let _ = cap_tx.try_send(stream);
        }
    }
    Ok(CappedBytes { bytes, truncated })
}

async fn join_reader(
    task: tokio::task::JoinHandle<io::Result<CappedBytes>>,
) -> ApiResult<CappedBytes> {
    task.await
        .map_err(|error| {
            ApiError::new(ErrorCode::Internal, "Git output task failed")
                .with_details(error.to_string())
        })?
        .map_err(io_error)
}

fn sanitize_git_environment(command: &mut Command) {
    const EXACT: &[&str] = &[
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_PREFIX",
        "GIT_SHALLOW_FILE",
        "GIT_REPLACE_REF_BASE",
        "GIT_CONFIG",
        "GIT_CONFIG_SYSTEM",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
        "GIT_EXEC_PATH",
        "GIT_SSH",
        "GIT_SSH_COMMAND",
        "GIT_PROXY_COMMAND",
        "GIT_ASKPASS",
        "SSH_ASKPASS",
        "GIT_EDITOR",
        "GIT_SEQUENCE_EDITOR",
        "GIT_EXTERNAL_DIFF",
        "GIT_CURL_VERBOSE",
        "GIT_ALLOW_PROTOCOL",
    ];
    for key in EXACT {
        command.env_remove(key);
    }
    for (key, _) in std::env::vars_os() {
        let upper = key.to_string_lossy().to_ascii_uppercase();
        if upper.starts_with("GIT_CONFIG_KEY_")
            || upper.starts_with("GIT_CONFIG_VALUE_")
            || upper.starts_with("GIT_TRACE")
        {
            command.env_remove(key);
        }
    }
}

fn spawn_error(executable: &Path, error: io::Error) -> ApiError {
    let code = if error.kind() == io::ErrorKind::NotFound {
        ErrorCode::GitNotFound
    } else {
        ErrorCode::Io
    };
    ApiError::new(code, "Git executable could not be started").with_details(format!(
        "{}: {}",
        executable.display(),
        error
    ))
}

fn io_error(error: io::Error) -> ApiError {
    ApiError::new(ErrorCode::Io, "Git process I/O failed").with_details(error.to_string())
}

fn git_failure_error(output: &GitCommandOutput) -> ApiError {
    let exit = output.status.code().map_or_else(
        || "terminated".to_owned(),
        |code| format!("exit code {code}"),
    );
    classify_failure(
        &output.stderr_lossy_redacted(),
        &redact_sensitive(&output.stdout_lossy()),
        &exit,
    )
}

fn classify_failure(stderr: &str, stdout: &str, exit: &str) -> ApiError {
    let lower = format!("{stderr}\n{stdout}").to_ascii_lowercase();
    let (code, message) = if lower.contains("not a git repository") {
        (
            ErrorCode::InvalidRepository,
            "The selected folder is not a Git repository",
        )
    } else if lower.contains("index.lock") || lower.contains("another git process") {
        (
            ErrorCode::RepositoryBusy,
            "The repository is locked by another Git process",
        )
    } else if lower.contains("failed to remove")
        && (lower.contains("invalid argument")
            || lower.contains("access is denied")
            || lower.contains("permission denied")
            || lower.contains("being used by another process"))
    {
        (
            ErrorCode::RepositoryBusy,
            "Some files could not be removed because another process is using them",
        )
    } else if lower.contains("no tracking information")
        || lower.contains("has no upstream branch")
        || lower.contains("no upstream configured")
    {
        (
            ErrorCode::UpstreamMissing,
            "The branch has no upstream configured",
        )
    } else if lower.contains("authentication failed")
        || lower.contains("could not read username")
        || lower.contains("permission denied (publickey)")
        || lower.contains("terminal prompts disabled")
        || lower.contains("the requested url returned error: 401")
    {
        (
            ErrorCode::AuthenticationRequired,
            "Remote authentication failed",
        )
    } else if lower.contains("stale info") || lower.contains("remote ref updated since checkout") {
        // The lease on the remote did not hold: it moved after the last fetch,
        // or it moved into work this branch never had. Either way something is
        // there that a forced push would have discarded unseen.
        (
            ErrorCode::NonFastForward,
            "The remote moved since the last fetch, so the force push was refused",
        )
    } else if lower.contains("non-fast-forward") || lower.contains("fetch first") {
        (
            ErrorCode::NonFastForward,
            "The remote rejected a non-fast-forward update",
        )
    } else if lower.contains("without 'workflow' scope") {
        (
            ErrorCode::ProtectedOperation,
            "The remote rejected the push because the sign-in may not change workflow files",
        )
    } else if remote_access_denied(&lower) {
        (
            ErrorCode::ProtectedOperation,
            "The remote denied access; check your account and repository permissions",
        )
    } else if lower.contains("remote rejected") || lower.contains("protected branch") {
        (
            ErrorCode::ProtectedOperation,
            "The remote rejected the push",
        )
    } else if lower.contains("would be overwritten") || lower.contains("local changes") {
        (
            ErrorCode::DirtyWorktree,
            "Local changes prevent this Git operation",
        )
    } else if lower.contains("conflict") || lower.contains("unmerged") {
        (
            ErrorCode::ConflictsPresent,
            "Git stopped because conflicts require attention",
        )
    } else if lower.contains("could not resolve host")
        || lower.contains("failed to connect")
        || lower.contains("unable to access")
        || lower.contains("connection timed out")
        || lower.contains("connection reset")
    {
        (ErrorCode::NetworkFailed, "The remote could not be reached")
    } else if lower.contains("bad revision")
        || lower.contains("unknown revision")
        || lower.contains("needed a single revision")
        || lower.contains("ambiguous argument")
    {
        (
            ErrorCode::InvalidRevision,
            "The selected Git revision is invalid",
        )
    } else {
        (ErrorCode::GitCommandFailed, "Git command failed")
    };

    // `push --porcelain` reports why a ref was rejected on stdout, so stderr
    // alone ends at "failed to push some refs" with the reason missing. Stdout
    // goes last because the cap keeps the tail.
    let details = [collapse_progress(stderr), collapse_progress(stdout)]
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let details = tail_chars(&details, STDERR_DETAILS_CAP);
    let details = if details.trim().is_empty() {
        exit.to_owned()
    } else {
        format!("{exit}: {}", details.trim())
    };
    recovery_actions(code, &lower)
        .into_iter()
        .fold(ApiError::new(code, message), |error, (kind, label)| {
            error.with_recovery_action(kind, label)
        })
        .with_details(details)
}

fn remote_access_denied(lower: &str) -> bool {
    lower.contains("the requested url returned error: 403")
        || (lower.contains("remote: permission to ") && lower.contains(" denied to "))
        || lower.contains("remote: write access to repository not granted")
}

/// What to offer doing next about a failure, where the next step follows from
/// the failure itself.
///
/// The output of one Git command is all there is to go on, so an action is
/// offered only where the wording says which command failed: "has no upstream
/// branch" is a push, while a pull's "no tracking information" leaves the user
/// a choice this cannot make for them.
fn recovery_actions(code: ErrorCode, lower: &str) -> Vec<(&'static str, &'static str)> {
    match code {
        // A refused lease is the one place where overwriting is a step the user
        // may legitimately want next -- they already asked to force once -- so
        // it is offered beside the safe answer, never instead of it.
        ErrorCode::NonFastForward
            if lower.contains("stale info")
                || lower.contains("remote ref updated since checkout") =>
        {
            vec![
                ("pull", "Pull, then push again"),
                ("push_force", "Force push, overwriting the remote"),
            ]
        }
        ErrorCode::NonFastForward => vec![("pull", "Pull, then push again")],
        ErrorCode::UpstreamMissing if lower.contains("has no upstream branch") => {
            vec![("push_set_upstream", "Push and set the upstream branch")]
        }
        ErrorCode::AuthenticationRequired => {
            vec![("open_settings", "Check the hosting service connection")]
        }
        ErrorCode::ProtectedOperation if lower.contains("without 'workflow' scope") => {
            vec![("open_settings", "Sign in again to allow workflow files")]
        }
        ErrorCode::ProtectedOperation if remote_access_denied(lower) => {
            vec![("open_settings", "Check the hosting service connection")]
        }
        _ => Vec::new(),
    }
}

pub(crate) fn redact_sensitive(input: &str) -> String {
    let mut result = input.to_owned();
    for scheme in ["https://", "http://", "ssh://"] {
        let mut search_from = 0;
        while let Some(relative_start) = result[search_from..].find(scheme) {
            let credentials_start = search_from + relative_start + scheme.len();
            let tail = &result[credentials_start..];
            let authority_end = tail
                .find(['/', '\\', ' ', '\n', '\r', '\t'])
                .unwrap_or(tail.len());
            let authority = &tail[..authority_end];
            let Some(at) = authority.rfind('@') else {
                search_from = credentials_start + authority_end;
                continue;
            };
            let credentials_end = credentials_start + at;
            result.replace_range(credentials_start..credentials_end, "***");
            search_from = credentials_start + 3 + 1;
        }
    }
    for marker in [
        "token=",
        "access_token=",
        "password=",
        "passwd=",
        "secret=",
        "token:",
        "password:",
        "authorization: bearer ",
        "bearer ",
    ] {
        redact_after_marker(&mut result, marker);
    }
    result
}

fn redact_after_marker(value: &mut String, marker: &str) {
    let mut search_from = 0;
    loop {
        let lower = value.to_ascii_lowercase();
        let Some(relative) = lower[search_from..].find(marker) else {
            break;
        };
        let secret_start = search_from + relative + marker.len();
        let secret_end = value[secret_start..]
            .find(|character: char| {
                character.is_whitespace()
                    || matches!(character, '&' | ';' | ',' | '\'' | '"' | '<' | '>')
            })
            .map_or(value.len(), |relative_end| secret_start + relative_end);
        if secret_start == secret_end {
            search_from = secret_start;
            continue;
        }
        value.replace_range(secret_start..secret_end, "***");
        search_from = secret_start + 3;
    }
}

/// Git draws progress by rewriting one line with carriage returns, so a
/// captured stream holds every intermediate percentage. Only the last segment
/// of a line was ever meant to be read.
fn collapse_progress(input: &str) -> String {
    input
        .split('\n')
        .filter_map(|line| {
            line.rsplit('\r')
                .find(|segment| !segment.trim().is_empty())
                .map(str::trim_end)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tail_chars(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_owned();
    }
    let mut start = input.len() - max_bytes;
    while !input.is_char_boundary(start) {
        start += 1;
    }
    input[start..].to_owned()
}

pub(crate) fn os_args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_failure_keeps_the_porcelain_rejection_reason() {
        // Git sends the progress meter and the summary to stderr, while
        // --porcelain puts the per-ref status on stdout.
        let stderr = concat!(
            "Counting objects:  50% (1/2)\rCounting objects: 100% (2/2), done.\n",
            "error: failed to push some refs to 'https://example.test/repo.git'\n"
        );
        let stdout = concat!(
            "To https://example.test/repo.git\n",
            "!\trefs/heads/main:refs/heads/main\t[remote rejected] ",
            "(refusing to allow an OAuth App to create or update workflow ",
            "'.github/workflows/ci.yml' without 'workflow' scope)\nDone\n"
        );

        let error = classify_failure(stderr, stdout, "exit code 1");

        assert_eq!(error.code, ErrorCode::ProtectedOperation);
        let details = error.details.unwrap_or_default();
        assert!(details.contains("without 'workflow' scope"), "{details}");
        // The progress meter collapses to its final state.
        assert!(details.contains("Counting objects: 100% (2/2), done."));
        assert!(!details.contains("Counting objects:  50%"));
    }

    #[test]
    fn non_fast_forward_still_wins_over_the_generic_rejection() {
        let error = classify_failure(
            "error: failed to push some refs",
            "!\trefs/heads/main:refs/heads/main\t[rejected] (fetch first)",
            "exit code 1",
        );

        assert_eq!(error.code, ErrorCode::NonFastForward);
    }

    #[test]
    fn remote_permission_denials_offer_connection_settings() {
        for stderr in [
            concat!(
                "remote: Permission to ikoliHU/clipcat.git denied to ikoliHU.\n",
                "fatal: unable to access 'https://github.com/ikoliHU/clipcat.git/': ",
                "The requested URL returned error: 403",
            ),
            "remote: Permission to owner/repo.git denied to user.",
            "remote: Write access to repository not granted.",
            "fatal: unable to access 'https://example.test/repo.git/': The requested URL returned error: 403",
        ] {
            let error = classify_failure(stderr, "", "exit code 128");
            // An access denial does not imply an expired credential. Keep it
            // out of the automatic authentication-renewal path.
            assert_eq!(error.code, ErrorCode::ProtectedOperation, "{stderr}");
            assert!(error.message.contains("denied access"));
            assert_eq!(error.recovery_actions.len(), 1);
            assert_eq!(error.recovery_actions[0].kind, "open_settings");
            assert!(error.details.as_deref().unwrap().contains(stderr));
        }
    }

    #[test]
    fn http_authentication_failure_is_distinct_from_network_failure() {
        let error = classify_failure(
            "fatal: unable to access 'https://example.test/repo.git/': The requested URL returned error: 401",
            "",
            "exit code 128",
        );
        assert_eq!(error.code, ErrorCode::AuthenticationRequired);
        assert_eq!(error.recovery_actions[0].kind, "open_settings");

        for reason in [
            "Could not resolve host: example.test",
            "Failed to connect to example.test port 443",
            "The requested URL returned error: 500",
        ] {
            let error = classify_failure(
                &format!("fatal: unable to access 'https://example.test/repo.git/': {reason}"),
                "",
                "exit code 128",
            );
            assert_eq!(error.code, ErrorCode::NetworkFailed, "{reason}");
            assert!(error.recovery_actions.is_empty());
        }
    }

    #[test]
    fn a_rejected_push_offers_the_step_that_unblocks_it() {
        let non_fast_forward = classify_failure(
            "error: failed to push some refs",
            "!	refs/heads/main:refs/heads/main	[rejected] (fetch first)",
            "exit code 1",
        );
        assert_eq!(
            non_fast_forward
                .recovery_actions
                .iter()
                .map(|action| action.kind.as_str())
                .collect::<Vec<_>>(),
            ["pull"]
        );

        let no_upstream = classify_failure(
            "fatal: The current branch work has no upstream branch.",
            "",
            "exit code 128",
        );
        assert_eq!(no_upstream.code, ErrorCode::UpstreamMissing);
        assert_eq!(
            no_upstream
                .recovery_actions
                .iter()
                .map(|action| action.kind.as_str())
                .collect::<Vec<_>>(),
            ["push_set_upstream"]
        );

        // A pull without tracking information is the same code, but setting an
        // upstream is not the only sensible answer, so nothing is offered.
        let pull = classify_failure(
            "There is no tracking information for the current branch.",
            "",
            "exit code 1",
        );
        assert_eq!(pull.code, ErrorCode::UpstreamMissing);
        assert!(pull.recovery_actions.is_empty());
    }

    #[test]
    fn a_missing_workflow_scope_points_at_signing_in_again() {
        let error = classify_failure(
            "error: failed to push some refs",
            "!	refs/heads/main:refs/heads/main	[remote rejected] (refusing to allow an OAuth App to create or update workflow '.github/workflows/ci.yml' without 'workflow' scope)",
            "exit code 1",
        );

        assert_eq!(error.code, ErrorCode::ProtectedOperation);
        let action = error.recovery_actions.first().expect("recovery action");
        assert_eq!(action.kind, "open_settings");
        assert_eq!(action.label, "Sign in again to allow workflow files");

        // A protected branch has no step GitCat can offer.
        let protected = classify_failure(
            "! [remote rejected] main -> main (protected branch hook declined)",
            "",
            "exit code 1",
        );
        assert_eq!(protected.code, ErrorCode::ProtectedOperation);
        assert!(protected.recovery_actions.is_empty());
    }

    #[test]
    fn redacts_url_userinfo() {
        let value = redact_sensitive(
            "fatal: unable to access 'https://alice:secret@example.test/repo': denied",
        );
        assert_eq!(
            value,
            "fatal: unable to access 'https://***@example.test/repo': denied"
        );
    }

    #[test]
    fn keeps_safe_urls() {
        assert_eq!(
            redact_sensitive("https://example.test/repo"),
            "https://example.test/repo"
        );
    }

    #[test]
    fn redacts_tokens_passwords_and_bearer_values() {
        let value =
            redact_sensitive("Authorization: Bearer abc.def token=secret&password=hunter2 next");
        assert_eq!(
            value,
            "Authorization: Bearer *** token=***&password=*** next"
        );
    }
}
