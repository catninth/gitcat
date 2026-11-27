use std::{
    collections::{BTreeSet, HashMap, hash_map::DefaultHasher},
    ffi::OsString,
    fmt, fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use gitcat_contracts::*;
use gitcat_core::{
    GitBackend, GraphLayoutContext, detect_forge, forge_web_url, layout_commits_with_context,
    parse_remote_url,
};
use tokio_util::sync::CancellationToken;

use crate::{
    conflict::{
        GITLINK_MODE, conflict_expected_state, entry_at_stage, merge_tree_preflight_unavailable,
        parse_merge_tree_paths, summarize_paths, supports_merge_tree_preflight,
    },
    credentials::{GitCredentialSource, HostCredential},
    limits::*,
    line_patch::{PatchDirection, build_line_patch},
    operation::{ensure_operation, operation_message_file, operation_state_from_git_dir},
    parse::{
        DETAIL_FORMAT, DetectedRename, LOG_FORMAT, ParsedStatus, REF_FORMAT, STASH_GRAPH_FORMAT,
        StashCommit, StashGraph, merge_worktree_renames, parse_changed_files, parse_commit_details,
        parse_file_diff, parse_git_version, parse_line_stats, parse_log, parse_name_status,
        parse_numstat_renames, parse_refs, parse_search_hits, parse_stash_graph, parse_status,
    },
    runner::{GitCommandOutput, GitRunOptions, GitRunner, os_args, redact_sensitive},
    validate::{
        is_full_oid, sparse_patterns, validate_mainline_parent, validate_message, validate_paths,
        validate_relative_path, validate_remote_name, validate_remote_url,
    },
};

/// The file an empty repository is seeded with so its first commit has
/// something in it. The hosting services and GitKraken all start here too, and
/// a README is the one file whose absence is always a gap.
const INITIAL_README: &str = "README.md";

mod branches;
mod commits;
mod conflicts;
mod history;
mod operations;
mod remotes;
mod renames;
mod repository;
mod stash;
mod worktree;

#[derive(Debug, Clone)]
struct CommitAuthor {
    name: String,
    email: String,
    date: String,
}

#[derive(Debug)]
struct HistoryInputs {
    generation: String,
    refs_output: Vec<u8>,
    layout: GraphLayoutContext,
    stashes: StashGraph,
}

#[derive(Debug)]
struct SearchInputs {
    normal_revisions: Vec<OsString>,
    virtual_stashes: Vec<(String, StashCommit)>,
}

impl CommitAuthor {
    fn into_env(self) -> Vec<(OsString, OsString)> {
        vec![
            ("GIT_AUTHOR_NAME".into(), self.name.into()),
            ("GIT_AUTHOR_EMAIL".into(), self.email.into()),
            ("GIT_AUTHOR_DATE".into(), self.date.into()),
        ]
    }
}

#[derive(Clone, Default)]
pub struct GitCliBackend {
    pub(crate) runner: GitRunner,
    /// Where a token for a hosting service comes from. Absent by default, which
    /// leaves every network command to the user's own credential helpers.
    credentials: Option<Arc<dyn GitCredentialSource>>,
}

// The credential source is deliberately opaque: nothing about it belongs in a
// debug rendering of the backend.
impl fmt::Debug for GitCliBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCliBackend")
            .field("runner", &self.runner)
            .field("credentials", &self.credentials.is_some())
            .finish()
    }
}

impl GitCliBackend {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            runner: GitRunner::new(executable),
            credentials: None,
        }
    }

    /// Lets network commands authenticate with a token GitCat holds, instead of
    /// relying on the user having signed in to a system credential manager.
    pub fn with_credentials(mut self, credentials: Arc<dyn GitCredentialSource>) -> Self {
        self.credentials = Some(credentials);
        self
    }

    pub(crate) async fn read(
        &self,
        path: Option<&Path>,
        args: Vec<OsString>,
    ) -> ApiResult<GitCommandOutput> {
        self.runner
            .run(
                path,
                &args,
                None,
                CancellationToken::new(),
                GitRunOptions::read_only(READ_OUTPUT_CAP),
            )
            .await
    }

    pub(crate) async fn read_allow_failure(
        &self,
        path: Option<&Path>,
        args: Vec<OsString>,
    ) -> ApiResult<GitCommandOutput> {
        let mut options = GitRunOptions::read_only(READ_OUTPUT_CAP);
        options.allow_failure = true;
        self.runner
            .run(path, &args, None, CancellationToken::new(), options)
            .await
    }

    pub(crate) async fn status_output(&self, path: &Path) -> ApiResult<GitCommandOutput> {
        self.read(
            Some(path),
            os_args(&[
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--show-stash",
                "--untracked-files=all",
            ]),
        )
        .await
    }

    pub(crate) async fn head_oid(&self, path: &Path) -> ApiResult<Option<String>> {
        let output = self
            .read_allow_failure(
                Some(path),
                os_args(&["rev-parse", "--verify", "--end-of-options", "HEAD^{commit}"]),
            )
            .await?;
        if output.success() {
            Ok(Some(output.stdout_lossy().trim().to_owned()))
        } else {
            Ok(None)
        }
    }

    pub(crate) async fn mutation_result(
        &self,
        path: &Path,
        before_oid: Option<String>,
    ) -> ApiResult<MutationResult> {
        let after_oid = self.head_oid(path).await?;
        let (generation, _, parsed_status) = self.generation_and_refs(path).await?;
        let conflicts: Vec<_> = parsed_status
            .status
            .entries
            .iter()
            .filter(|entry| entry.conflicted)
            .cloned()
            .collect();
        let needs_user_action = !conflicts.is_empty()
            || self.operation_state(path).await? != RepositoryOperationState::Normal;
        Ok(MutationResult {
            before_oid,
            after_oid,
            generation,
            conflicts,
            needs_user_action,
            notice: None,
        })
    }

    pub(crate) async fn mutate(
        &self,
        path: &Path,
        args: Vec<OsString>,
        stdin: Option<&[u8]>,
        cancellation: CancellationToken,
        network: bool,
    ) -> ApiResult<MutationResult> {
        let options = if network {
            GitRunOptions::network(NETWORK_OUTPUT_CAP)
        } else {
            GitRunOptions::mutation(READ_OUTPUT_CAP)
        };
        self.mutate_with(path, args, stdin, cancellation, options)
            .await
    }

    /// The commit a ref points at, or `None` when the ref does not resolve.
    pub(crate) async fn rev_parse_commit(
        &self,
        path: &Path,
        reference: &str,
    ) -> ApiResult<Option<String>> {
        let output = self
            .read_allow_failure(
                Some(path),
                os_args(&[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    "--end-of-options",
                    &format!("{reference}^{{commit}}"),
                ]),
            )
            .await?;
        let value = output.stdout_lossy().trim().to_owned();
        Ok(if output.success() && !value.is_empty() {
            Some(value)
        } else {
            None
        })
    }

    pub(crate) async fn mutate_with(
        &self,
        path: &Path,
        args: Vec<OsString>,
        stdin: Option<&[u8]>,
        cancellation: CancellationToken,
        mut options: GitRunOptions,
    ) -> ApiResult<MutationResult> {
        let before_oid = self.head_oid(path).await?;
        options.allow_failure = true;
        let remote_host = options.remote_host.clone();
        let output = self
            .runner
            .run(Some(path), &args, stdin, cancellation, options)
            .await?;
        if output.success() {
            return self.mutation_result(path, before_oid).await;
        }
        let result = self.mutation_result(path, before_oid).await?;
        let failure = network_recovery(self.runner.failure_error(&output), remote_host.as_deref());
        if result.needs_user_action && failure.code == ErrorCode::ConflictsPresent {
            Ok(result)
        } else {
            Err(failure)
        }
    }
}

#[async_trait]
impl GitBackend for GitCliBackend {
    async fn list_submodules(&self, path: &Path) -> ApiResult<Vec<SubmoduleInfo>> {
        self.submodules(path).await
    }

    async fn add_submodule(
        &self,
        path: &Path,
        url: &str,
        relative: &str,
    ) -> ApiResult<MutationResult> {
        self.add_submodule_inner(path, url, relative).await
    }

    async fn set_submodule_url(
        &self,
        path: &Path,
        relative: &str,
        url: &str,
    ) -> ApiResult<MutationResult> {
        self.set_submodule_url_inner(path, relative, url).await
    }

    async fn update_submodule(&self, path: &Path, relative: &str) -> ApiResult<MutationResult> {
        self.sync_submodule_inner(path, relative, SubmoduleSyncAction::Pull)
            .await
    }

    async fn remove_submodule(
        &self,
        path: &Path,
        relative: &str,
        confirmed: bool,
    ) -> ApiResult<MutationResult> {
        self.remove_submodule_inner(path, relative, confirmed).await
    }

    async fn sync_submodule(
        &self,
        path: &Path,
        relative: &str,
        action: SubmoduleSyncAction,
    ) -> ApiResult<MutationResult> {
        self.sync_submodule_inner(path, relative, action).await
    }

    async fn probe(&self) -> ApiResult<GitVersion> {
        self.probe_inner().await
    }

    async fn open_repository(&self, path: &Path) -> ApiResult<RepositoryInfo> {
        self.inspect_repository(path).await
    }

    async fn init_repository(
        &self,
        path: &Path,
        default_branch: &str,
    ) -> ApiResult<RepositoryInfo> {
        self.init_repository_inner(path, default_branch).await
    }

    async fn clone_repository(
        &self,
        options: &CloneOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<RepositoryInfo> {
        self.clone_repository_inner(options, cancellation).await
    }

    async fn snapshot(&self, path: &Path) -> ApiResult<RepositorySnapshot> {
        self.snapshot_inner(path).await
    }

    async fn history(&self, path: &Path, query: &HistoryQuery) -> ApiResult<HistoryPage> {
        self.history_inner(path, query).await
    }

    async fn search_commits(
        &self,
        path: &Path,
        query: &CommitSearchQuery,
    ) -> ApiResult<CommitSearchResult> {
        self.search_commits_inner(path, query).await
    }

    async fn commit_details(
        &self,
        path: &Path,
        oid: &str,
        parent_index: usize,
    ) -> ApiResult<CommitDetails> {
        self.commit_details_inner(path, oid, parent_index).await
    }

    async fn diff(&self, path: &Path, request: &DiffRequest) -> ApiResult<FileDiff> {
        self.diff_inner(path, request).await
    }

    async fn conflict_preflight(
        &self,
        path: &Path,
        target: &str,
    ) -> ApiResult<ConflictPreflightResult> {
        self.conflict_preflight_inner(path, target).await
    }

    async fn conflict_resolution_warnings(
        &self,
        path: &Path,
        paths: &[String],
    ) -> ApiResult<Vec<ConflictResolutionWarning>> {
        self.conflict_resolution_warnings_inner(path, paths).await
    }

    async fn submodule_conflict_details(
        &self,
        path: &Path,
        conflict_path: &str,
    ) -> ApiResult<SubmoduleConflictDetails> {
        self.describe_submodule_conflict(path, conflict_path).await
    }

    async fn conflict_details(
        &self,
        path: &Path,
        conflict_path: &str,
    ) -> ApiResult<ConflictFileDetails> {
        self.conflict_details_inner(path, conflict_path).await
    }

    async fn stage_paths(&self, path: &Path, paths: &[String]) -> ApiResult<MutationResult> {
        self.stage_paths_inner(path, paths).await
    }

    async fn unstage_paths(&self, path: &Path, paths: &[String]) -> ApiResult<MutationResult> {
        self.unstage_paths_inner(path, paths).await
    }

    async fn discard_paths(&self, path: &Path, paths: &[String]) -> ApiResult<MutationResult> {
        self.discard_paths_inner(path, paths).await
    }

    async fn apply_diff_lines(
        &self,
        path: &Path,
        request: &LinePatchRequest,
    ) -> ApiResult<MutationResult> {
        self.apply_diff_lines_inner(path, request).await
    }

    async fn stash_paths(
        &self,
        path: &Path,
        paths: &[String],
        message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        self.stash_paths_inner(path, paths, message).await
    }

    async fn append_gitignore(
        &self,
        path: &Path,
        patterns: &[String],
    ) -> ApiResult<MutationResult> {
        self.append_gitignore_inner(path, patterns).await
    }

    async fn create_patch(&self, path: &Path, paths: &[String], staged: bool) -> ApiResult<String> {
        self.create_patch_inner(path, paths, staged).await
    }

    async fn resolve_conflict(
        &self,
        path: &Path,
        conflict_path: &str,
        resolution: ConflictResolution,
        expected_state: &ConflictExpectedState,
    ) -> ApiResult<MutationResult> {
        self.resolve_conflict_inner(path, conflict_path, resolution, expected_state)
            .await
    }

    async fn save_conflict_result(
        &self,
        path: &Path,
        conflict_path: &str,
        text: &str,
        line_ending: ConflictLineEndingPolicy,
        expected_state: &ConflictExpectedState,
    ) -> ApiResult<MutationResult> {
        self.save_conflict_result_inner(path, conflict_path, text, line_ending, expected_state)
            .await
    }

    async fn resolve_conflicts(
        &self,
        path: &Path,
        conflict_paths: &[String],
        resolution: ConflictResolution,
    ) -> ApiResult<MutationResult> {
        self.resolve_conflicts_inner(path, conflict_paths, resolution)
            .await
    }

    async fn auto_resolve_conflicts(&self, path: &Path) -> ApiResult<MutationResult> {
        self.auto_resolve_conflicts_inner(path).await
    }

    async fn create_commit(
        &self,
        path: &Path,
        options: &CommitOptions,
    ) -> ApiResult<MutationResult> {
        self.create_commit_inner(path, options).await
    }

    async fn create_initial_commit(&self, path: &Path, message: &str) -> ApiResult<MutationResult> {
        self.create_initial_commit_inner(path, message).await
    }

    async fn reword_commit(
        &self,
        path: &Path,
        oid: &str,
        message: &str,
    ) -> ApiResult<MutationResult> {
        self.reword_commit_inner(path, oid, message).await
    }

    async fn create_branch(
        &self,
        path: &Path,
        name: &str,
        start_oid: &str,
        checkout: bool,
    ) -> ApiResult<MutationResult> {
        self.create_branch_inner(path, name, start_oid, checkout)
            .await
    }

    async fn checkout_branch(&self, path: &Path, name: &str) -> ApiResult<MutationResult> {
        self.checkout_branch_inner(path, name).await
    }

    async fn rename_branch(
        &self,
        path: &Path,
        old_name: &str,
        new_name: &str,
    ) -> ApiResult<MutationResult> {
        self.rename_branch_inner(path, old_name, new_name).await
    }

    async fn delete_branch(
        &self,
        path: &Path,
        name: &str,
        force: bool,
        confirmed: bool,
    ) -> ApiResult<MutationResult> {
        self.delete_branch_inner(path, name, force, confirmed).await
    }

    async fn delete_remote_branch(
        &self,
        path: &Path,
        remote: &str,
        branch: &str,
        confirmed: bool,
    ) -> ApiResult<MutationResult> {
        self.delete_remote_branch_inner(path, remote, branch, confirmed)
            .await
    }

    async fn set_upstream(
        &self,
        path: &Path,
        branch: &str,
        upstream: &str,
    ) -> ApiResult<MutationResult> {
        self.set_upstream_inner(path, branch, upstream).await
    }

    async fn merge_branch(&self, path: &Path, branch: &str) -> ApiResult<MutationResult> {
        self.merge_branch_inner(path, branch).await
    }

    async fn add_remote(&self, path: &Path, name: &str, url: &str) -> ApiResult<MutationResult> {
        self.add_remote_inner(path, name, url).await
    }

    async fn fetch(
        &self,
        path: &Path,
        options: &FetchOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        self.fetch_inner(path, options, cancellation).await
    }

    async fn pull(
        &self,
        path: &Path,
        options: &PullOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        self.pull_inner(path, options, cancellation).await
    }

    async fn pull_with_progress(
        &self,
        path: &Path,
        options: &PullOptions,
        cancellation: CancellationToken,
        progress: PullProgress,
    ) -> ApiResult<MutationResult> {
        self.pull_with_progress_inner(path, options, cancellation, progress)
            .await
    }

    async fn push(
        &self,
        path: &Path,
        options: &PushOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        self.push_inner(path, options, cancellation).await
    }

    async fn checkout_commit(&self, path: &Path, oid: &str) -> ApiResult<MutationResult> {
        self.checkout_commit_inner(path, oid).await
    }

    async fn create_tag(
        &self,
        path: &Path,
        name: &str,
        oid: &str,
        message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        self.create_tag_inner(path, name, oid, message).await
    }

    async fn cherry_pick(
        &self,
        path: &Path,
        oid: &str,
        mainline_parent: Option<u32>,
        commit: bool,
    ) -> ApiResult<MutationResult> {
        self.cherry_pick_inner(path, oid, mainline_parent, commit)
            .await
    }

    async fn revert_commit(
        &self,
        path: &Path,
        oid: &str,
        mainline_parent: Option<u32>,
        commit: bool,
    ) -> ApiResult<MutationResult> {
        self.revert_commit_inner(path, oid, mainline_parent, commit)
            .await
    }

    async fn reset_to_commit(
        &self,
        path: &Path,
        oid: &str,
        mode: ResetMode,
        confirmed: bool,
    ) -> ApiResult<MutationResult> {
        self.reset_to_commit_inner(path, oid, mode, confirmed).await
    }

    async fn commit_action_availability(
        &self,
        path: &Path,
        oid: &str,
    ) -> ApiResult<Vec<CommitActionAvailability>> {
        self.commit_action_availability_inner(path, oid).await
    }

    async fn continue_operation(
        &self,
        path: &Path,
        operation: ContinueOperation,
        message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        self.continue_operation_inner(path, operation, message)
            .await
    }

    async fn abort_operation(
        &self,
        path: &Path,
        operation: ContinueOperation,
    ) -> ApiResult<MutationResult> {
        self.abort_operation_inner(path, operation).await
    }

    async fn skip_operation(
        &self,
        path: &Path,
        operation: ContinueOperation,
    ) -> ApiResult<MutationResult> {
        self.skip_operation_inner(path, operation).await
    }

    async fn stash_list(&self, path: &Path) -> ApiResult<Vec<StashEntry>> {
        self.stash_list_inner(path).await
    }

    async fn stash_push(
        &self,
        path: &Path,
        message: Option<&str>,
        include_untracked: bool,
    ) -> ApiResult<MutationResult> {
        self.stash_push_inner(path, message, include_untracked)
            .await
    }

    async fn stash_apply(&self, path: &Path, oid: &str, pop: bool) -> ApiResult<MutationResult> {
        self.stash_apply_inner(path, oid, pop).await
    }

    async fn stash_drop(
        &self,
        path: &Path,
        oid: &str,
        confirmed: bool,
    ) -> ApiResult<MutationResult> {
        self.stash_drop_inner(path, oid, confirmed).await
    }
}

fn unified_context(request: &DiffRequest) -> u32 {
    if request.whole_file {
        WHOLE_FILE_CONTEXT_LINES
    } else {
        u32::from(request.context_lines.min(100))
    }
}

fn blocking_line_stats_task_error(error: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        ErrorCode::Internal,
        "Untracked line count worker task failed unexpectedly",
    )
    .with_details(error.to_string())
}

/// An operation side that no ref points at is shown as an abbreviated object id;
/// the panel prints it inline, where a full id would push the branch name out.
fn short_operation_oid(value: &str) -> String {
    let is_oid = value.len() >= 40 && value.chars().all(|character| character.is_ascii_hexdigit());
    if is_oid {
        value.chars().take(8).collect()
    } else {
        value.to_owned()
    }
}

pub(crate) fn canonical_or_absolute(base: &Path, value: &str) -> ApiResult<PathBuf> {
    let path = PathBuf::from(value);
    let path = if path.is_absolute() {
        path
    } else {
        base.join(path)
    };
    dunce::canonicalize(&path).map_err(|error| {
        ApiError::new(
            ErrorCode::InvalidRepository,
            "Repository metadata path could not be resolved",
        )
        .with_details(error.to_string())
    })
}

fn confirmation_required(message: &'static str) -> ApiError {
    ApiError::new(ErrorCode::ProtectedOperation, message)
}

fn stash_for_oid<'a>(stashes: &'a StashGraph, oid: &str) -> Option<(&'a String, &'a StashCommit)> {
    stashes.commits.get_key_value(oid).or_else(|| {
        stashes
            .outer_to_visible
            .get(oid)
            .and_then(|visible_oid| stashes.commits.get_key_value(visible_oid))
    })
}

fn apply_stash_view(commits: &mut Vec<CommitSummary>, stashes: &StashGraph) {
    if stashes.is_empty() {
        return;
    }

    commits.retain(|commit| !stashes.hidden.contains(&commit.oid));
    for commit in commits.iter_mut() {
        let Some(stash) = stashes.commits.get(&commit.oid) else {
            continue;
        };
        commit.parent_oids = stash.base_oid.iter().cloned().collect();
        commit.body_preview = String::new();
        commit.subject = stash.label.clone();
        commit.stash = Some(stash.reference.clone());
    }
}

/// Resolves a stash's *current* `stash@{n}` index by its stable outer commit
/// oid, re-reading the live stash list rather than trusting a caller-supplied
/// index that may have gone stale (shifted or removed) between when the UI
/// displayed the stash and when the action runs.
fn resolve_stash_index(stashes: Vec<StashEntry>, oid: &str) -> ApiResult<usize> {
    stashes
        .into_iter()
        .find(|stash| stash.oid == oid)
        .map(|stash| stash.index)
        .ok_or_else(|| {
            ApiError::new(
                ErrorCode::InvalidRevision,
                "Selected stash no longer exists",
            )
        })
}

#[cfg(test)]
mod tests;

/// The host a password would be sent to, which only exists for the transports
/// that use one. An SSH remote authenticates with a key, so a token is not what
/// it is missing.
/// The credential a failed network command should be retried with, if any.
///
/// Only a refusal is worth renewing: every other failure -- an unreachable
/// host, a rejected non-fast-forward -- would fail identically with a new
/// token. A command that carried no token has nothing to renew either; it was
/// left to the user's own credential helpers.
fn renewal_candidate<'a>(
    result: &ApiResult<MutationResult>,
    options: &'a GitRunOptions,
) -> Option<&'a HostCredential> {
    match result {
        Err(error) if error.code == ErrorCode::AuthenticationRequired => {
            options.credential.as_ref()
        }
        _ => None,
    }
}

/// The start of a `fetch` or `pull`, with every ref outside the namespaces
/// GitCat shows hidden from the connectivity check that follows the download.
/// That check otherwise treats all local refs as already present, and a single
/// ref another tool parked under its own prefix -- a Codex checkpoint whose
/// object was pruned -- fails the whole command with "bad object". Hiding a ref
/// only makes the check walk further; it never skips an object Git needs.
pub(crate) fn download_args(subcommand: &str) -> Vec<OsString> {
    os_args(&[
        "-c",
        "fetch.hideRefs=refs/",
        "-c",
        "fetch.hideRefs=!refs/heads/",
        "-c",
        "fetch.hideRefs=!refs/remotes/",
        "-c",
        "fetch.hideRefs=!refs/tags/",
        subcommand,
        "--progress",
    ])
}

/// Offer the registered sign-in only for the failed command's known host.
fn network_recovery(mut error: ApiError, host: Option<&str>) -> ApiError {
    // Only GitHub.com has a registered device-flow application. A protected
    // branch has no connection recovery action, so it never offers a sign-in.
    if host.is_some_and(|host| host.eq_ignore_ascii_case("github.com")) {
        for action in &mut error.recovery_actions {
            if action.kind == "open_settings" {
                action.kind = "sign_in_github".into();
                action.label = "Sign in to GitHub again".into();
            }
        }
    }
    error
}

/// The token to try again with.
///
/// Getting the same token back means the renewal changed nothing: the service
/// dropped a credential that is still the current one, and running the command
/// a second time would spend another round trip on the same answer.
fn renewed_token(previous: &HostCredential, renewed: Option<String>) -> Option<String> {
    renewed.filter(|token| token != &previous.token)
}

fn password_host(url: Option<&RemoteUrlParts>) -> Option<&str> {
    let url = url?;
    matches!(url.scheme, RemoteUrlScheme::Https | RemoteUrlScheme::Http)
        .then(|| url.host.as_str())
        .filter(|host| !host.is_empty())
}
