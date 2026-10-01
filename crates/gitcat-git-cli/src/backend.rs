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
        conflict_expected_state, entry_at_stage, merge_tree_preflight_unavailable,
        parse_merge_tree_paths, summarize_paths, supports_merge_tree_preflight,
    },
    credentials::{GitCredentialSource, HostCredential},
    limits::*,
    operation::ensure_operation,
    parse::{
        DETAIL_FORMAT, LOG_FORMAT, ParsedStatus, REF_FORMAT, STASH_GRAPH_FORMAT, StashCommit,
        StashGraph, parse_changed_files, parse_commit_details, parse_file_diff, parse_git_version,
        parse_line_stats, parse_log, parse_name_status, parse_refs, parse_search_hits,
        parse_stash_graph, parse_status,
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

    async fn is_untracked_file(&self, path: &Path, relative: &str) -> ApiResult<bool> {
        if !path.join(relative).is_file() {
            return Ok(false);
        }
        let args = vec![
            "ls-files".into(),
            "-z".into(),
            "--".into(),
            OsString::from(relative),
        ];
        let listed = self.read(Some(path), args).await?;
        Ok(listed.stdout.iter().all(|byte| *byte == 0))
    }

    async fn untracked_diff(&self, path: &Path, request: &DiffRequest) -> ApiResult<FileDiff> {
        let mut args = os_args(&[
            "diff",
            "--no-index",
            "--patch",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
        ]);
        args.push(format!("--unified={}", unified_context(request)).into());
        if request.ignore_whitespace {
            args.push("--ignore-all-space".into());
        }
        args.push("--".into());
        args.push("/dev/null".into());
        args.push(request.path.as_str().into());
        let mut options = GitRunOptions::read_only(request.max_bytes.clamp(1, MAX_DIFF_BYTES));
        options.allow_stdout_truncation = true;
        options.allow_failure = true;
        options.timeout = Some(Duration::from_secs(60));
        let output = self
            .runner
            .run(Some(path), &args, None, CancellationToken::new(), options)
            .await?;
        if !matches!(output.status.code(), Some(0 | 1)) {
            return Err(self.runner.failure_error(&output));
        }
        parse_file_diff(&output.stdout, &request.path, output.stdout_truncated)
    }

    async fn inspect_repository(&self, path: &Path) -> ApiResult<RepositoryInfo> {
        let selected = dunce::canonicalize(path).map_err(|error| {
            ApiError::new(
                ErrorCode::InvalidPath,
                "Repository path could not be resolved",
            )
            .with_details(format!("{}: {error}", path.display()))
        })?;
        if !selected.is_dir() {
            return Err(ApiError::new(
                ErrorCode::InvalidPath,
                "Repository path must be a directory",
            ));
        }

        let bare = self
            .read(
                Some(&selected),
                os_args(&["rev-parse", "--is-bare-repository"]),
            )
            .await?
            .stdout_lossy()
            .trim()
            == "true";
        if bare {
            return Err(ApiError::new(
                ErrorCode::UnsupportedOperation,
                "Bare repositories are not supported by the desktop worktree API",
            ));
        }
        let root = if bare {
            selected.clone()
        } else {
            let output = self
                .read(Some(&selected), os_args(&["rev-parse", "--show-toplevel"]))
                .await?;
            dunce::canonicalize(output.stdout_lossy().trim()).map_err(|error| {
                ApiError::new(
                    ErrorCode::InvalidRepository,
                    "Repository root could not be resolved",
                )
                .with_details(error.to_string())
            })?
        };

        let git_dir_output = self
            .read(
                Some(&selected),
                os_args(&["rev-parse", "--absolute-git-dir"]),
            )
            .await?;
        let git_dir = canonical_or_absolute(&selected, git_dir_output.stdout_lossy().trim())?;
        let common_output = self
            .read(
                Some(&selected),
                os_args(&["rev-parse", "--path-format=absolute", "--git-common-dir"]),
            )
            .await?;
        let common_dir = canonical_or_absolute(&selected, common_output.stdout_lossy().trim())?;

        let format_output = self
            .read_allow_failure(
                Some(&selected),
                os_args(&["rev-parse", "--show-object-format"]),
            )
            .await?;
        let object_format = if format_output.success() {
            match format_output.stdout_lossy().trim() {
                "sha1" => ObjectFormat::Sha1,
                "sha256" => ObjectFormat::Sha256,
                _ => ObjectFormat::Unknown,
            }
        } else {
            ObjectFormat::Sha1
        };
        let name = root
            .file_name()
            .unwrap_or(root.as_os_str())
            .to_string_lossy()
            .into_owned();
        Ok(RepositoryInfo {
            root: root.to_string_lossy().into_owned(),
            git_dir: git_dir.to_string_lossy().into_owned(),
            common_dir: common_dir.to_string_lossy().into_owned(),
            name,
            is_bare: bare,
            object_format,
        })
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

    async fn line_stats(&self, path: &Path, cached: bool) -> ApiResult<HashMap<String, LineStats>> {
        let mut args = os_args(&[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "-M",
            "--numstat",
            "-z",
        ]);
        if cached {
            args.push("--cached".into());
        }
        let output = self.read_allow_failure(Some(path), args).await?;
        if !output.success() {
            return Ok(HashMap::new());
        }
        parse_line_stats(&output.stdout)
    }

    // Untracked files never reach `git diff --numstat`, so their additions come
    // from counting the lines on disk. Large or binary files stay unmeasured.
    async fn untracked_line_stats(
        &self,
        path: &Path,
        paths: Vec<String>,
    ) -> ApiResult<HashMap<String, LineStats>> {
        if paths.is_empty() {
            return Ok(HashMap::new());
        }
        let root = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let mut stats = HashMap::new();
            for relative in paths.into_iter().take(MAX_UNTRACKED_STAT_FILES) {
                let file = root.join(&relative);
                let Ok(metadata) = fs::metadata(&file) else {
                    continue;
                };
                if !metadata.is_file() || metadata.len() > MAX_UNTRACKED_STAT_BYTES {
                    continue;
                }
                let Ok(bytes) = fs::read(&file) else {
                    continue;
                };
                if bytes.contains(&0) {
                    continue;
                }
                let lines = bytes.iter().filter(|byte| **byte == b'\n').count() as u64
                    + u64::from(!bytes.is_empty() && !bytes.ends_with(b"\n"));
                stats.insert(
                    relative,
                    LineStats {
                        additions: lines,
                        deletions: 0,
                    },
                );
            }
            stats
        })
        .await
        .map_err(blocking_line_stats_task_error)
    }

    async fn apply_line_stats(&self, path: &Path, status: &mut WorktreeStatus) -> ApiResult<()> {
        if status.entries.is_empty() {
            return Ok(());
        }
        let untracked: Vec<String> = status
            .entries
            .iter()
            .filter(|entry| {
                matches!(entry.worktree, Some(ChangeKind::Untracked)) && !entry.submodule
            })
            .map(|entry| entry.path.clone())
            .collect();
        let (worktree, index, new_files) = tokio::try_join!(
            self.line_stats(path, false),
            self.line_stats(path, true),
            self.untracked_line_stats(path, untracked)
        )?;
        for entry in &mut status.entries {
            entry.index_stats = index.get(&entry.path).copied();
            entry.worktree_stats = if matches!(entry.worktree, Some(ChangeKind::Untracked)) {
                new_files.get(&entry.path).copied()
            } else {
                worktree.get(&entry.path).copied()
            };
        }
        Ok(())
    }

    async fn refs_output(&self, path: &Path) -> ApiResult<GitCommandOutput> {
        let mut args = os_args(&["for-each-ref"]);
        args.push(format!("--format={REF_FORMAT}").into());
        args.extend(os_args(&["refs/heads", "refs/remotes", "refs/tags"]));
        self.read(Some(path), args).await
    }

    async fn generation_and_refs(&self, path: &Path) -> ApiResult<(String, Vec<u8>, ParsedStatus)> {
        let (status, refs) = tokio::try_join!(self.status_output(path), self.refs_output(path))?;
        let parsed = parse_status(&status.stdout)?;
        let mut hasher = DefaultHasher::new();
        status.stdout.hash(&mut hasher);
        refs.stdout.hash(&mut hasher);
        Ok((format!("{:016x}", hasher.finish()), refs.stdout, parsed))
    }

    async fn history_inputs(&self, path: &Path) -> ApiResult<HistoryInputs> {
        let (status, refs, head, stash_output) = tokio::try_join!(
            self.status_output(path),
            self.refs_output(path),
            self.read_allow_failure(
                Some(path),
                os_args(&["rev-parse", "--verify", "--end-of-options", "HEAD"]),
            ),
            self.stash_graph_output(path),
        )?;
        let parsed_status = parse_status(&status.stdout)?;
        let has_wip = !parsed_status.status.clean;
        let wip_head_oid = if has_wip {
            match parsed_status.head {
                HeadState::Branch { oid, .. } | HeadState::Detached { oid } => Some(oid),
                HeadState::Unborn { .. } => None,
            }
        } else {
            None
        };
        let refs = refs.stdout;
        let mut hasher = DefaultHasher::new();
        refs.hash(&mut hasher);
        head.status.code().hash(&mut hasher);
        head.stdout.hash(&mut hasher);
        has_wip.hash(&mut hasher);
        stash_output.hash(&mut hasher);
        Ok(HistoryInputs {
            generation: format!("{:016x}", hasher.finish()),
            refs_output: refs,
            layout: GraphLayoutContext { wip_head_oid },
            stashes: parse_stash_graph(&stash_output)?,
        })
    }

    async fn head_oid(&self, path: &Path) -> ApiResult<Option<String>> {
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

    async fn short_oid(&self, path: &Path, oid: &str) -> ApiResult<String> {
        let output = self
            .read(
                Some(path),
                vec![
                    "rev-parse".into(),
                    "--short".into(),
                    "--verify".into(),
                    "--end-of-options".into(),
                    oid.into(),
                ],
            )
            .await?;
        Ok(output.stdout_lossy().trim().to_owned())
    }

    async fn default_stash_message(&self, path: &Path) -> ApiResult<Option<String>> {
        let output = self
            .read_allow_failure(
                Some(path),
                os_args(&["symbolic-ref", "--quiet", "--short", "HEAD"]),
            )
            .await?;
        if !output.success() {
            return Ok(None);
        }
        let branch = output.stdout_lossy().trim().to_owned();
        Ok((!branch.is_empty()).then(|| format!("WIP on {branch}")))
    }

    async fn resolve_commit(&self, path: &Path, revision: &str) -> ApiResult<String> {
        if revision.is_empty() || revision.contains(['\0', '\n', '\r']) {
            return Err(ApiError::new(
                ErrorCode::InvalidRevision,
                "Git revision is empty or malformed",
            ));
        }
        let expression = format!("{revision}^{{commit}}");
        let output = self
            .read(
                Some(path),
                vec![
                    "rev-parse".into(),
                    "--verify".into(),
                    "--end-of-options".into(),
                    expression.into(),
                ],
            )
            .await
            .map_err(|error| {
                if matches!(
                    error.code,
                    ErrorCode::GitCommandFailed | ErrorCode::InvalidRevision
                ) {
                    ApiError::new(
                        ErrorCode::InvalidRevision,
                        "Git revision could not be resolved",
                    )
                } else {
                    error
                }
            })?;
        let oid = output.stdout_lossy().trim().to_owned();
        if !is_full_oid(&oid) {
            return Err(ApiError::new(
                ErrorCode::InvalidRevision,
                "Git did not resolve the revision to a full object ID",
            ));
        }
        Ok(oid)
    }

    async fn commit_parent_oids(&self, path: &Path, oid: &str) -> ApiResult<Vec<String>> {
        let output = self
            .read(
                Some(path),
                vec![
                    "show".into(),
                    "-s".into(),
                    "--format=%P".into(),
                    oid.into(),
                    "--".into(),
                ],
            )
            .await?;
        Ok(output
            .stdout_lossy()
            .split_whitespace()
            .map(str::to_owned)
            .collect())
    }

    async fn is_ancestor(&self, path: &Path, ancestor: &str, descendant: &str) -> ApiResult<bool> {
        let output = self
            .read_allow_failure(
                Some(path),
                vec![
                    "merge-base".into(),
                    "--is-ancestor".into(),
                    ancestor.into(),
                    descendant.into(),
                ],
            )
            .await?;
        Ok(output.success())
    }

    async fn range_has_merge(&self, path: &Path, base: &str, head: &str) -> ApiResult<bool> {
        // Merges strictly after `base` would be linearized by a plain rebase,
        // so callers reject rewording when any exist.
        let output = self
            .read(
                Some(path),
                vec![
                    "rev-list".into(),
                    "--merges".into(),
                    "--max-count=1".into(),
                    format!("{base}..{head}").into(),
                    "--".into(),
                ],
            )
            .await?;
        Ok(!output.stdout_lossy().trim().is_empty())
    }

    async fn read_commit_author(&self, path: &Path, oid: &str) -> ApiResult<CommitAuthor> {
        let output = self
            .read(
                Some(path),
                vec![
                    "show".into(),
                    "-s".into(),
                    "--date=raw".into(),
                    "--format=%an%x00%ae%x00%ad".into(),
                    oid.into(),
                    "--".into(),
                ],
            )
            .await?;
        let raw = output.stdout_lossy();
        let mut parts = raw.trim_end_matches(['\n', '\r']).splitn(3, '\0');
        Ok(CommitAuthor {
            name: parts.next().unwrap_or_default().to_owned(),
            email: parts.next().unwrap_or_default().to_owned(),
            date: parts.next().unwrap_or_default().to_owned(),
        })
    }

    /// Rebuilds `oid` with `message`, preserving its tree, parents and
    /// authorship, and returns the new (dangling) commit object id.
    async fn rebuild_commit_message(
        &self,
        path: &Path,
        oid: &str,
        message: &str,
    ) -> ApiResult<String> {
        let tree = self
            .read(
                Some(path),
                vec![
                    "rev-parse".into(),
                    "--verify".into(),
                    "--end-of-options".into(),
                    format!("{oid}^{{tree}}").into(),
                ],
            )
            .await?
            .stdout_lossy()
            .trim()
            .to_owned();
        if !is_full_oid(&tree) {
            return Err(ApiError::new(
                ErrorCode::GitCommandFailed,
                "Git did not resolve the commit tree",
            ));
        }
        let parents = self.commit_parent_oids(path, oid).await?;
        let author = self.read_commit_author(path, oid).await?;

        let mut args: Vec<OsString> = vec!["commit-tree".into(), tree.into()];
        for parent in parents {
            args.push("-p".into());
            args.push(parent.into());
        }
        args.push("-F".into());
        args.push("-".into());

        let mut options = GitRunOptions::mutation(READ_OUTPUT_CAP);
        options.extra_env = author.into_env();
        let output = self
            .runner
            .run(
                Some(path),
                &args,
                Some(message.as_bytes()),
                CancellationToken::new(),
                options,
            )
            .await?;
        let new_oid = output.stdout_lossy().trim().to_owned();
        if !is_full_oid(&new_oid) {
            return Err(ApiError::new(
                ErrorCode::GitCommandFailed,
                "Git did not return a rewritten commit object",
            ));
        }
        Ok(new_oid)
    }

    async fn remotes(&self, path: &Path) -> ApiResult<Vec<RemoteInfo>> {
        let output = self.read(Some(path), os_args(&["remote"])).await?;
        let mut remotes = Vec::new();
        for name in output
            .stdout_lossy()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            validate_remote_name(name)?;
            let fetch = self
                .read(
                    Some(path),
                    vec!["remote".into(), "get-url".into(), name.into()],
                )
                .await?;
            let fetch_url = fetch.stdout_lossy().trim().to_owned();
            validate_remote_url(&fetch_url)?;
            let mut push_options = GitRunOptions::read_only(READ_OUTPUT_CAP);
            push_options.allow_failure = true;
            let push = self
                .runner
                .run(
                    Some(path),
                    &[
                        "remote".into(),
                        "get-url".into(),
                        "--push".into(),
                        name.into(),
                    ],
                    None,
                    CancellationToken::new(),
                    push_options,
                )
                .await?;
            let push_url = if push.success() {
                push.stdout_lossy().trim().to_owned()
            } else {
                fetch_url.clone()
            };
            validate_remote_url(&push_url)?;
            let fetch_url = redact_sensitive(&fetch_url);
            let push_url = redact_sensitive(&push_url);
            // Fetch names where the repository lives; push may point at a fork
            // remote that shares no host, so it is only the fallback.
            let url = parse_remote_url(&fetch_url).or_else(|| parse_remote_url(&push_url));
            let forge = url.as_ref().map(detect_forge).unwrap_or_default();
            let web_url = url.as_ref().and_then(|parts| forge_web_url(parts, forge));
            remotes.push(RemoteInfo {
                name: name.to_owned(),
                fetch_url,
                push_url,
                url,
                forge,
                web_url,
            });
        }
        Ok(remotes)
    }

    /// Checks the selection and hands back the remotes it was checked against,
    /// so a caller that also needs their URLs does not ask Git twice.
    async fn validate_remote_selection(
        &self,
        path: &Path,
        remote: Option<&str>,
    ) -> ApiResult<Vec<RemoteInfo>> {
        let remotes = self.remotes(path).await?;
        if let Some(remote) = remote {
            validate_remote_name(remote)?;
            if !remotes.iter().any(|entry| entry.name == remote) {
                return Err(ApiError::new(
                    ErrorCode::InvalidSettings,
                    "Selected remote does not exist",
                ));
            }
        } else if remotes.is_empty() {
            return Err(ApiError::new(
                ErrorCode::UpstreamMissing,
                "Repository has no configured remote",
            ));
        }
        Ok(remotes)
    }

    /// Options for a network command, carrying a token when it is unambiguous
    /// which host the command will contact.
    ///
    /// A command that reaches several hosts -- a push or pull with no explicit
    /// remote in a repository whose remotes sit on different services -- is
    /// left to the credential helpers Git already has: installing one clears
    /// them for the whole command, and a token for one host is no use to
    /// another. A fetch never lands here with more than one host, because it
    /// runs one command per remote.
    async fn network_options(&self, remotes: &[RemoteInfo], remote: Option<&str>) -> GitRunOptions {
        let mut options = GitRunOptions::network(NETWORK_OUTPUT_CAP);
        let Some(source) = self.credentials.as_ref() else {
            return options;
        };
        let hosts: BTreeSet<&str> = remotes
            .iter()
            .filter(|entry| remote.is_none_or(|name| entry.name == name))
            .filter_map(|entry| password_host(entry.url.as_ref()))
            .collect();
        let mut hosts = hosts.into_iter();
        let (Some(host), None) = (hosts.next(), hosts.next()) else {
            return options;
        };

        if let Some(token) = source.token_for(host).await {
            options.credential = Some(HostCredential {
                host: host.to_owned(),
                token,
            });
        }
        options
    }

    /// Options for a clone, whose host comes from the URL rather than from a
    /// configured remote.
    async fn clone_options(&self, url: &str) -> GitRunOptions {
        let mut options = GitRunOptions::network(NETWORK_OUTPUT_CAP);
        let Some(source) = self.credentials.as_ref() else {
            return options;
        };
        let parsed = parse_remote_url(url);
        let Some(host) = password_host(parsed.as_ref()) else {
            return options;
        };

        if let Some(token) = source.token_for(host).await {
            options.credential = Some(HostCredential {
                host: host.to_owned(),
                token,
            });
        }
        options
    }

    async fn mutation_result(
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

    /// Puts a `README.md` in an empty repository and answers with its path.
    ///
    /// A file that is already there is left exactly as it is: this seeds a
    /// starting point for a repository that has none, it never overwrites what
    /// the user wrote. The heading is the repository folder's own name, which
    /// is what the hosting services fill a new README with.
    async fn write_initial_readme(&self, path: &Path) -> ApiResult<String> {
        let root = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let target = root.join(INITIAL_README);
            if target.exists() {
                return Ok(INITIAL_README.to_owned());
            }
            let name = root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Repository");
            fs::write(&target, format!("# {name}\n")).map_err(|error| {
                ApiError::new(ErrorCode::Io, "The initial README could not be written")
                    .with_details(error.to_string())
            })?;
            Ok(INITIAL_README.to_owned())
        })
        .await
        .map_err(|error| {
            ApiError::new(
                ErrorCode::Internal,
                "Writing the initial README did not finish",
            )
            .with_details(error.to_string())
        })?
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

    /// The commit a push would send, and the commit its destination already
    /// has -- when both are knowable without contacting the remote.
    ///
    /// Gittyup answers this locally before spending a network round trip, and
    /// so does this: the destination's remote-tracking ref already names a
    /// commit, and a push that would send the one the branch is on has nothing
    /// to do. The tracking ref can be stale, which is the price of not asking
    /// the remote; that is the same data the ahead/behind counters are drawn
    /// from, so a user acting on what the window shows gets a consistent
    /// answer.
    ///
    /// `None` means the shape is one this cannot answer for -- a detached
    /// HEAD, a `push.default` that sends more than one branch, no destination
    /// to derive. Those go to Git unchanged.
    async fn push_destination_state(
        &self,
        path: &Path,
        options: &PushOptions,
    ) -> ApiResult<Option<(String, String)>> {
        // A named branch is both the source and the destination name; only a
        // push that names none is about whatever HEAD is on.
        let branch = match &options.branch {
            Some(branch) => {
                self.validate_branch_name(branch).await?;
                branch.clone()
            }
            None => {
                let head = self
                    .read_allow_failure(
                        Some(path),
                        os_args(&["symbolic-ref", "--quiet", "--short", "HEAD"]),
                    )
                    .await?;
                if !head.success() {
                    return Ok(None);
                }
                let head = head.stdout_lossy().trim().to_owned();
                if head.is_empty() {
                    return Ok(None);
                }
                head
            }
        };

        let config = |key: String| async move {
            let output = self
                .read_allow_failure(Some(path), os_args(&["config", "--get", key.as_str()]))
                .await?;
            let value = output.stdout_lossy().trim().to_owned();
            ApiResult::Ok(if output.success() && !value.is_empty() {
                Some(value)
            } else {
                None
            })
        };

        // `matching` and `nothing` do not describe one branch going to one
        // place, so nothing here can stand in for the command.
        let mode = config("push.default".to_owned())
            .await?
            .unwrap_or_else(|| "simple".to_owned());
        if options.branch.is_none()
            && !matches!(
                mode.as_str(),
                "simple" | "current" | "upstream" | "tracking"
            )
        {
            return Ok(None);
        }

        // The remote the push actually goes to. An explicitly chosen one wins,
        // which is what keeps an upstream on a different remote out of the
        // comparison.
        let remote = match &options.remote {
            Some(remote) => remote.clone(),
            None => {
                let configured = match config(format!("branch.{branch}.pushRemote")).await? {
                    Some(value) => Some(value),
                    None => match config("remote.pushDefault".to_owned()).await? {
                        Some(value) => Some(value),
                        None => config(format!("branch.{branch}.remote")).await?,
                    },
                };
                match configured {
                    Some(remote) => remote,
                    None => return Ok(None),
                }
            }
        };

        let destination = match &options.branch {
            Some(branch) => branch.clone(),
            None if matches!(mode.as_str(), "upstream" | "tracking") => {
                // Only the branch's own upstream names the destination here,
                // and only on the remote that upstream lives on.
                if config(format!("branch.{branch}.remote")).await?.as_deref() != Some(&remote) {
                    return Ok(None);
                }
                match config(format!("branch.{branch}.merge")).await? {
                    Some(merge) => merge
                        .strip_prefix("refs/heads/")
                        .unwrap_or(&merge)
                        .to_owned(),
                    None => return Ok(None),
                }
            }
            None => branch.clone(),
        };

        let local = self
            .rev_parse_commit(path, &format!("refs/heads/{branch}"))
            .await?;
        let tracking = self
            .rev_parse_commit(path, &format!("refs/remotes/{remote}/{destination}"))
            .await?;
        Ok(match (local, tracking) {
            (Some(local), Some(tracking)) => Some((local, tracking)),
            _ => None,
        })
    }

    /// The commit a ref points at, or `None` when the ref does not resolve.
    async fn rev_parse_commit(&self, path: &Path, reference: &str) -> ApiResult<Option<String>> {
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

    /// Runs a network mutation, and gives a refused credential exactly one
    /// second chance.
    ///
    /// The token GitCat holds can die between commands -- revoked on the
    /// service's page, or superseded by a sign-in elsewhere -- and the user has
    /// no way to tell that apart from "the push failed". A rejection is worth
    /// one renewal: if the source can produce a different token, the same
    /// command runs again with it. One retry, and only when the token actually
    /// changed, so a service that keeps refusing is reported rather than asked
    /// twice.
    async fn mutate_network(
        &self,
        path: &Path,
        args: Vec<OsString>,
        cancellation: CancellationToken,
        options: GitRunOptions,
    ) -> ApiResult<MutationResult> {
        let result = self
            .mutate_with(
                path,
                args.clone(),
                None,
                cancellation.clone(),
                options.clone(),
            )
            .await;
        let (Some(credential), Some(source)) = (
            renewal_candidate(&result, &options),
            self.credentials.as_ref(),
        ) else {
            return result;
        };
        let renewed = source.renew_rejected(&credential.host).await;
        let Some(token) = renewed_token(credential, renewed) else {
            return result;
        };
        let mut retry = options.clone();
        retry.credential = Some(HostCredential {
            host: credential.host.clone(),
            token,
        });
        self.mutate_with(path, args, None, cancellation, retry)
            .await
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
        let output = self
            .runner
            .run(Some(path), &args, stdin, cancellation, options)
            .await?;
        if output.success() {
            return self.mutation_result(path, before_oid).await;
        }
        let result = self.mutation_result(path, before_oid).await?;
        let failure = self.runner.failure_error(&output);
        if result.needs_user_action && failure.code == ErrorCode::ConflictsPresent {
            Ok(result)
        } else {
            Err(failure)
        }
    }

    async fn changed_files_for_commit(
        &self,
        path: &Path,
        oid: &str,
        parents: &[String],
        parent_index: usize,
    ) -> ApiResult<Vec<ChangedFile>> {
        let (name_args, stat_args) = if parents.is_empty() {
            if parent_index != 0 {
                return Err(ApiError::new(
                    ErrorCode::InvalidRevision,
                    "Root commit has no selected parent",
                ));
            }
            (
                vec![
                    "diff-tree".into(),
                    "--root".into(),
                    "--no-commit-id".into(),
                    "-r".into(),
                    "-M".into(),
                    "--name-status".into(),
                    "-z".into(),
                    oid.into(),
                ],
                vec![
                    "diff-tree".into(),
                    "--root".into(),
                    "--no-commit-id".into(),
                    "-r".into(),
                    "-M".into(),
                    "--numstat".into(),
                    "-z".into(),
                    oid.into(),
                ],
            )
        } else {
            let parent = parents.get(parent_index).ok_or_else(|| {
                ApiError::new(
                    ErrorCode::InvalidRevision,
                    "Commit parent index is out of range",
                )
            })?;
            (
                vec![
                    "diff".into(),
                    "--no-ext-diff".into(),
                    "--no-textconv".into(),
                    "-M".into(),
                    "--name-status".into(),
                    "-z".into(),
                    parent.into(),
                    oid.into(),
                    "--".into(),
                ],
                vec![
                    "diff".into(),
                    "--no-ext-diff".into(),
                    "--no-textconv".into(),
                    "-M".into(),
                    "--numstat".into(),
                    "-z".into(),
                    parent.into(),
                    oid.into(),
                    "--".into(),
                ],
            )
        };
        let (names, stats) = tokio::try_join!(
            self.read(Some(path), name_args),
            self.read(Some(path), stat_args)
        )?;
        parse_changed_files(&names.stdout, &stats.stdout)
    }

    async fn stash_graph_output(&self, path: &Path) -> ApiResult<Vec<u8>> {
        let mut args = os_args(&["stash", "list"]);
        args.push(format!("--format={STASH_GRAPH_FORMAT}").into());
        Ok(self.read(Some(path), args).await?.stdout)
    }

    async fn stash_graph(&self, path: &Path) -> ApiResult<StashGraph> {
        parse_stash_graph(&self.stash_graph_output(path).await?)
    }

    async fn history_revision(&self, path: &Path, scope: &HistoryScope) -> ApiResult<OsString> {
        match scope {
            HistoryScope::CurrentBranch => Ok("HEAD".into()),
            HistoryScope::AllRefs => Ok("--all".into()),
            HistoryScope::Ref(reference) => Ok(self.resolve_commit(path, reference).await?.into()),
        }
    }

    /// The revisions the all-refs graph walks. Only the namespaces the graph
    /// decorates are named: `--all` would also walk refs other tools park
    /// under their own prefixes, and one of those pointing at a pruned object
    /// fails the whole walk with "bad object". Stashes are added separately.
    async fn all_refs_revisions(&self, path: &Path) -> ApiResult<Vec<OsString>> {
        let mut revisions = os_args(&["--branches", "--remotes", "--tags"]);
        if self.head_oid(path).await?.is_some() {
            revisions.push("HEAD".into());
        }
        Ok(revisions)
    }

    async fn visible_history_revision(
        &self,
        path: &Path,
        scope: &HistoryScope,
        stashes: &StashGraph,
    ) -> ApiResult<OsString> {
        let revision = self.history_revision(path, scope).await?;
        let HistoryScope::Ref(_) = scope else {
            return Ok(revision);
        };
        let oid = revision.to_string_lossy();
        Ok(stash_for_oid(stashes, &oid)
            .map_or_else(|| revision.clone(), |(visible_oid, _)| visible_oid.into()))
    }

    async fn search_inputs(
        &self,
        path: &Path,
        scope: &HistoryScope,
        stashes: &StashGraph,
    ) -> ApiResult<SearchInputs> {
        match scope {
            HistoryScope::AllRefs => Ok(SearchInputs {
                normal_revisions: self.all_refs_revisions(path).await?,
                virtual_stashes: stashes
                    .commits
                    .iter()
                    .map(|(oid, stash)| (oid.clone(), stash.clone()))
                    .collect(),
            }),
            HistoryScope::CurrentBranch => Ok(SearchInputs {
                normal_revisions: vec!["HEAD".into()],
                virtual_stashes: Vec::new(),
            }),
            HistoryScope::Ref(reference) => {
                let oid = self.resolve_commit(path, reference).await?;
                if let Some((visible_oid, stash)) = stash_for_oid(stashes, &oid) {
                    Ok(SearchInputs {
                        normal_revisions: stash
                            .base_oid
                            .iter()
                            .cloned()
                            .map(OsString::from)
                            .collect(),
                        virtual_stashes: vec![(visible_oid.clone(), stash.clone())],
                    })
                } else {
                    Ok(SearchInputs {
                        normal_revisions: vec![oid.into()],
                        virtual_stashes: Vec::new(),
                    })
                }
            }
        }
    }

    async fn virtual_stash_search_hits(
        &self,
        path: &Path,
        needle: &str,
        candidates: Vec<(String, StashCommit)>,
    ) -> ApiResult<Vec<CommitSearchHit>> {
        let needle_lower = needle.to_lowercase();
        let mut matching = candidates
            .into_iter()
            .filter(|(_, stash)| stash.label.to_lowercase().contains(&needle_lower))
            .collect::<HashMap<_, _>>();
        if matching.is_empty() {
            return Ok(Vec::new());
        }

        let mut tips = matching.keys().cloned().collect::<Vec<_>>();
        tips.sort_unstable();
        let mut args = os_args(&[
            "log",
            "--no-walk=sorted",
            "--no-show-signature",
            "--encoding=UTF-8",
        ]);
        args.push(format!("--format={LOG_FORMAT}").into());
        args.extend(tips.into_iter().map(OsString::from));
        args.push("--".into());
        let output = self.read(Some(path), args).await?;

        Ok(parse_log(&output.stdout)?
            .into_iter()
            .filter_map(|commit| {
                matching.remove(&commit.oid).map(|stash| CommitSearchHit {
                    oid: commit.oid,
                    subject: stash.label,
                    body_excerpt: None,
                    matched_subject: true,
                    matched_body: false,
                })
            })
            .collect())
    }
}

#[async_trait]
impl GitBackend for GitCliBackend {
    async fn probe(&self) -> ApiResult<GitVersion> {
        let output = self.read(None, os_args(&["version"])).await?;
        let (major, minor, patch, raw) = parse_git_version(&output.stdout)?;
        if major < 2 || (major == 2 && minor < 31) {
            return Err(ApiError::new(
                ErrorCode::UnsupportedGitVersion,
                "Git 2.31 or newer is required",
            )
            .with_details(raw));
        }
        Ok(GitVersion {
            major,
            minor,
            patch,
            raw,
        })
    }

    async fn open_repository(&self, path: &Path) -> ApiResult<RepositoryInfo> {
        self.inspect_repository(path).await
    }

    async fn init_repository(
        &self,
        path: &Path,
        default_branch: &str,
    ) -> ApiResult<RepositoryInfo> {
        self.validate_branch_name(default_branch).await?;
        let args = vec![
            "init".into(),
            format!("--initial-branch={default_branch}").into(),
            "--".into(),
            path.as_os_str().to_owned(),
        ];
        self.runner
            .run(
                None,
                &args,
                None,
                CancellationToken::new(),
                GitRunOptions::mutation(READ_OUTPUT_CAP),
            )
            .await?;
        self.inspect_repository(path).await
    }

    async fn clone_repository(
        &self,
        options: &CloneOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<RepositoryInfo> {
        validate_remote_url(&options.url)?;
        let destination = PathBuf::from(&options.destination);
        if destination.as_os_str().is_empty() {
            return Err(ApiError::new(
                ErrorCode::InvalidPath,
                "Clone destination is empty",
            ));
        }
        let mut args = os_args(&["clone", "--progress"]);
        if let Some(branch) = &options.branch {
            self.validate_branch_name(branch).await?;
            args.push("--branch".into());
            args.push(branch.into());
        }
        if let Some(depth) = options.depth {
            if depth == 0 {
                return Err(ApiError::new(
                    ErrorCode::InvalidSettings,
                    "Clone depth must be greater than zero",
                ));
            }
            args.push(format!("--depth={depth}").into());
        }
        if options.filter_blob_none {
            args.push("--filter=blob:none".into());
        }
        let sparse = options
            .sparse_paths
            .as_deref()
            .map(sparse_patterns)
            .transpose()?;
        if sparse.is_some() {
            args.push("--sparse".into());
        }
        args.push("--".into());
        args.push(options.url.as_str().into());
        args.push(destination.as_os_str().to_owned());
        let run = self.clone_options(&options.url).await;
        self.runner
            .run(None, &args, None, cancellation.clone(), run)
            .await?;
        // The clone itself only sets the sparse root up; which directories
        // join it is a second command, and the patterns go in on stdin so a
        // path that looks like an option cannot become one.
        if let Some(patterns) = sparse.filter(|patterns| !patterns.is_empty()) {
            self.runner
                .run(
                    Some(&destination),
                    &os_args(&["sparse-checkout", "set", "--stdin"]),
                    Some(patterns.join("\n").as_bytes()),
                    cancellation,
                    GitRunOptions::mutation(READ_OUTPUT_CAP),
                )
                .await?;
        }
        self.inspect_repository(&destination).await
    }

    async fn snapshot(&self, path: &Path) -> ApiResult<RepositorySnapshot> {
        let (generation, refs_output, mut parsed_status) = self.generation_and_refs(path).await?;
        parsed_status.status.stash_count = self
            .stash_graph(path)
            .await?
            .commits
            .len()
            .try_into()
            .unwrap_or(u32::MAX);
        self.apply_line_stats(path, &mut parsed_status.status)
            .await?;
        let refs = parse_refs(&refs_output)?;
        let mut local_branches = Vec::new();
        let mut remote_branches = Vec::new();
        let mut symbolic_remote_targets = Vec::new();
        let mut tags = Vec::new();
        for parsed_ref in refs {
            match parsed_ref.label.kind {
                RefKind::LocalBranch => {
                    if let Some(branch) = parsed_ref.branch {
                        local_branches.push(branch);
                    }
                }
                RefKind::RemoteBranch if parsed_ref.symbolic_target.is_none() => {
                    if let Some(branch) = parsed_ref.branch {
                        remote_branches.push(branch);
                    }
                }
                RefKind::Tag => tags.push(parsed_ref.label),
                RefKind::RemoteBranch => {
                    if let Some(target) = parsed_ref.symbolic_target {
                        symbolic_remote_targets.push((parsed_ref.label.full_name, target));
                    }
                }
            }
        }
        local_branches.sort_by(|left, right| left.name.cmp(&right.name));
        remote_branches.sort_by(|left, right| left.name.cmp(&right.name));
        tags.sort_by(|left, right| left.name.cmp(&right.name));
        symbolic_remote_targets.sort_by(|left, right| left.0.cmp(&right.0));
        let remote_exists = |name: &str| remote_branches.iter().any(|branch| branch.name == name);
        let default_conflict_target = local_branches
            .iter()
            .find(|branch| branch.is_head)
            .and_then(|branch| branch.upstream.clone())
            .filter(|upstream| remote_exists(upstream))
            .or_else(|| {
                let resolve_target = |target: &str| {
                    target
                        .strip_prefix("refs/remotes/")
                        .filter(|target| remote_exists(target))
                        .map(str::to_owned)
                };
                let origin_default = symbolic_remote_targets
                    .iter()
                    .find(|(symbolic, _)| symbolic == "refs/remotes/origin/HEAD")
                    .and_then(|(_, target)| resolve_target(target));
                origin_default.or_else(|| {
                    symbolic_remote_targets
                        .iter()
                        .find_map(|(_, target)| resolve_target(target))
                })
            });

        let info = self.inspect_repository(path).await?;
        let shallow = self
            .read_allow_failure(
                Some(path),
                os_args(&["rev-parse", "--is-shallow-repository"]),
            )
            .await?
            .stdout_lossy()
            .trim()
            == "true";
        let partial_clone = self
            .read_allow_failure(
                Some(path),
                os_args(&["config", "--get-regexp", r"^remote\..*\.promisor$"]),
            )
            .await?
            .success();
        let sparse_checkout = self
            .read_allow_failure(
                Some(path),
                os_args(&["config", "--bool", "core.sparseCheckout"]),
            )
            .await?
            .stdout_lossy()
            .trim()
            == "true";
        let (operation_state, operation_progress, raw_operation_source) =
            self.operation_status(path).await?;
        let name_side = |value: String| {
            let branch = local_branches
                .iter()
                .chain(remote_branches.iter())
                .find(|branch| branch.oid == value);
            match branch {
                Some(branch) => branch.name.clone(),
                None => short_operation_oid(&value),
            }
        };
        let operation_source = raw_operation_source.map(|source| OperationSource {
            incoming: name_side(source.incoming),
            onto: source.onto.map(name_side),
            incoming_oid: source.incoming_oid,
        });
        let remotes = self.remotes(path).await?;
        Ok(RepositorySnapshot {
            generation,
            head: parsed_status.head,
            operation_state,
            operation_progress,
            operation_source,
            status: parsed_status.status,
            local_branches,
            remote_branches,
            default_conflict_target,
            tags,
            remotes,
            capabilities: RepositoryCapabilities {
                shallow,
                partial_clone,
                sparse_checkout,
                worktree: info.git_dir != info.common_dir,
            },
        })
    }

    async fn history(&self, path: &Path, query: &HistoryQuery) -> ApiResult<HistoryPage> {
        let HistoryInputs {
            generation,
            refs_output,
            layout,
            stashes,
        } = self.history_inputs(path).await?;
        if let Some(cursor) = &query.cursor {
            if cursor.generation != generation {
                return Err(ApiError::new(
                    ErrorCode::StaleSnapshot,
                    "Repository changed while commit history was paged",
                ));
            }
        }
        if matches!(query.scope, HistoryScope::CurrentBranch)
            && self.head_oid(path).await?.is_none()
        {
            return Ok(HistoryPage {
                generation,
                commits: Vec::new(),
                next_cursor: None,
                has_more: false,
            });
        }
        let limit = query.limit.clamp(1, MAX_HISTORY_PAGE);
        let offset = query.cursor.as_ref().map_or(0, |cursor| cursor.offset);
        if offset > 10_000_000 {
            return Err(ApiError::new(
                ErrorCode::InvalidSettings,
                "History cursor offset is outside the supported range",
            ));
        }
        let mut args = os_args(&[
            "log",
            "--topo-order",
            "--date-order",
            "--no-show-signature",
            "--encoding=UTF-8",
        ]);
        args.push(format!("--format={LOG_FORMAT}").into());
        args.push(format!("--skip={offset}").into());
        args.push(format!("--max-count={}", limit + 1).into());
        if matches!(query.scope, HistoryScope::AllRefs) {
            args.extend(self.all_refs_revisions(path).await?);
            let mut stash_tips: Vec<&String> = stashes.commits.keys().collect();
            stash_tips.sort_unstable();
            args.extend(stash_tips.into_iter().map(OsString::from));
        } else {
            args.push(
                self.visible_history_revision(path, &query.scope, &stashes)
                    .await?,
            );
        }
        args.push("--".into());
        let output = self.read(Some(path), args).await?;
        let inputs_after = self.history_inputs(path).await?;
        if inputs_after.generation != generation {
            return Err(ApiError::new(
                ErrorCode::StaleSnapshot,
                "Repository changed while commit history was read",
            ));
        }
        let mut commits = parse_log(&output.stdout)?;
        let has_more = commits.len() > limit;
        commits.truncate(limit);
        let walked = commits.len();
        apply_stash_view(&mut commits, &stashes);

        let mut labels: HashMap<String, Vec<RefLabel>> = HashMap::new();
        for parsed_ref in parse_refs(&refs_output)? {
            if matches!(parsed_ref.label.kind, RefKind::RemoteBranch)
                && parsed_ref.symbolic_target.is_some()
            {
                continue;
            }
            labels
                .entry(parsed_ref.oid)
                .or_default()
                .push(parsed_ref.label);
        }
        for commit in &mut commits {
            commit.decorations = labels.remove(&commit.oid).unwrap_or_default();
        }
        let mut lanes = query
            .cursor
            .as_ref()
            .map(|cursor| cursor.lanes.clone())
            .unwrap_or_default();
        layout_commits_with_context(&mut commits, &mut lanes, &layout);
        let next_cursor = has_more.then(|| HistoryCursor {
            generation: generation.clone(),
            offset: offset + walked,
            lanes,
        });
        Ok(HistoryPage {
            generation,
            commits,
            next_cursor,
            has_more,
        })
    }

    async fn search_commits(
        &self,
        path: &Path,
        query: &CommitSearchQuery,
    ) -> ApiResult<CommitSearchResult> {
        let needle = query.query.trim();
        if needle.is_empty() {
            return Ok(CommitSearchResult {
                total: 0,
                truncated: false,
                hits: Vec::new(),
            });
        }
        if needle.chars().count() > 256 || needle.contains('\0') {
            return Err(ApiError::new(
                ErrorCode::InvalidSettings,
                "Commit search query must contain at most 256 characters",
            ));
        }
        if matches!(query.scope, HistoryScope::CurrentBranch)
            && self.head_oid(path).await?.is_none()
        {
            return Ok(CommitSearchResult {
                total: 0,
                truncated: false,
                hits: Vec::new(),
            });
        }
        let limit = query.limit.clamp(1, MAX_SEARCH_RESULTS);
        let stashes = self.stash_graph(path).await?;
        let SearchInputs {
            normal_revisions,
            virtual_stashes,
        } = self.search_inputs(path, &query.scope, &stashes).await?;
        let mut virtual_hits = self
            .virtual_stash_search_hits(path, needle, virtual_stashes)
            .await?;
        let virtual_total = virtual_hits.len();
        let normal_limit = limit.saturating_sub(virtual_total.min(limit));
        let grep = format!("--grep={needle}");
        let (normal_total, mut normal_hits) = if normal_revisions.is_empty() {
            (0, Vec::new())
        } else {
            let mut count_args = os_args(&[
                "rev-list",
                "--count",
                "--fixed-strings",
                "--regexp-ignore-case",
            ]);
            count_args.push(grep.clone().into());
            count_args.extend(normal_revisions.iter().cloned());
            count_args.push("--".into());
            let count_output = self.read(Some(path), count_args).await?;
            let total = count_output
                .stdout_lossy()
                .trim()
                .parse::<usize>()
                .map_err(|_| {
                    ApiError::new(
                        ErrorCode::GitCommandFailed,
                        "Git returned an invalid search count",
                    )
                })?;

            if normal_limit == 0 {
                (total, Vec::new())
            } else {
                // Fetch enough additional raw matches to replace any stash
                // plumbing reachable through an unusual extra ref.
                let raw_limit = normal_limit
                    .saturating_add(stashes.hidden.len())
                    .saturating_add(stashes.commits.len());
                let mut log_args = os_args(&[
                    "log",
                    "--topo-order",
                    "--date-order",
                    "--fixed-strings",
                    "--regexp-ignore-case",
                    "--encoding=UTF-8",
                ]);
                log_args.push(grep.into());
                log_args.push(format!("--format={LOG_FORMAT}").into());
                log_args.push(format!("--max-count={raw_limit}").into());
                log_args.extend(normal_revisions.iter().cloned());
                log_args.push("--".into());
                let output = self.read(Some(path), log_args).await?;
                let mut hits = parse_search_hits(&output.stdout, needle)?;
                let raw_hit_count = hits.len();
                hits.retain(|hit| {
                    !stashes.hidden.contains(&hit.oid) && !stashes.commits.contains_key(&hit.oid)
                });
                let visible_total = total.saturating_sub(raw_hit_count - hits.len());
                hits.truncate(normal_limit);
                (visible_total, hits)
            }
        };

        // Virtual stash labels are not stored in their index-parent commit
        // objects, so Git cannot place them inside a single `--grep` traversal.
        // Keep virtual rows first in commit-date order, then preserve Git's
        // original topo/date order for every ordinary commit hit.
        let total = virtual_total.saturating_add(normal_total);
        virtual_hits.truncate(limit);
        virtual_hits.append(&mut normal_hits);
        Ok(CommitSearchResult {
            total,
            truncated: total > virtual_hits.len(),
            hits: virtual_hits,
        })
    }

    async fn commit_details(
        &self,
        path: &Path,
        oid: &str,
        parent_index: usize,
    ) -> ApiResult<CommitDetails> {
        let oid = self.resolve_commit(path, oid).await?;
        let stashes = self.stash_graph(path).await?;
        let stash = stash_for_oid(&stashes, &oid)
            .map(|(visible_oid, stash)| (visible_oid.clone(), stash.clone()));
        let show_oid = stash
            .as_ref()
            .map_or_else(|| oid.as_str(), |(_, stash)| stash.stash_oid.as_str());
        let mut args = os_args(&["show", "-s", "--no-show-signature", "--encoding=UTF-8"]);
        args.push(format!("--format={DETAIL_FORMAT}").into());
        args.push(show_oid.into());
        args.push("--".into());
        let output = self.read(Some(path), args).await?;
        let mut details = parse_commit_details(&output.stdout)?.details;
        let (diff_oid, diff_parents) = if let Some((visible_oid, stash)) = stash {
            details.oid = visible_oid.clone();
            details.short_oid = self.short_oid(path, &visible_oid).await?;
            details.subject = stash.label;
            details.parent_oids = stash.base_oid.iter().cloned().collect();
            (stash.stash_oid, details.parent_oids.clone())
        } else {
            (oid.clone(), details.parent_oids.clone())
        };
        let files = self
            .changed_files_for_commit(path, &diff_oid, &diff_parents, parent_index)
            .await?;
        details.stats = DiffStats {
            files: files.len().try_into().unwrap_or(u32::MAX),
            additions: files.iter().filter_map(|file| file.additions).sum(),
            deletions: files.iter().filter_map(|file| file.deletions).sum(),
        };
        details.files = files;
        Ok(details)
    }

    async fn diff(&self, path: &Path, request: &DiffRequest) -> ApiResult<FileDiff> {
        validate_relative_path(&request.path)?;
        if matches!(
            request.target,
            DiffTarget::Worktree | DiffTarget::HeadToWorktree
        ) && self.is_untracked_file(path, &request.path).await?
        {
            return self.untracked_diff(path, request).await;
        }
        let mut args = match &request.target {
            DiffTarget::Worktree => os_args(&["diff"]),
            DiffTarget::Staged => os_args(&["diff", "--cached"]),
            DiffTarget::HeadToWorktree => {
                if let Some(head) = self.head_oid(path).await? {
                    vec!["diff".into(), head.into()]
                } else {
                    os_args(&["diff", "--cached"])
                }
            }
            DiffTarget::Commit { oid, parent_index } => {
                let oid = self.resolve_commit(path, oid).await?;
                let stashes = self.stash_graph(path).await?;
                let stash = stash_for_oid(&stashes, &oid).map(|(_, stash)| stash.clone());
                if let Some(stash) = stash {
                    if *parent_index != 0 {
                        return Err(ApiError::new(
                            ErrorCode::InvalidRevision,
                            "Commit parent index is out of range",
                        ));
                    }
                    if let Some(base_oid) = stash.base_oid {
                        vec!["diff".into(), base_oid.into(), stash.stash_oid.into()]
                    } else {
                        vec![
                            "show".into(),
                            "--format=".into(),
                            "--root".into(),
                            stash.stash_oid.into(),
                        ]
                    }
                } else {
                    let details = self.commit_details(path, &oid, *parent_index).await?;
                    if details.parent_oids.is_empty() {
                        vec![
                            "show".into(),
                            "--format=".into(),
                            "--root".into(),
                            oid.into(),
                        ]
                    } else {
                        vec![
                            "diff".into(),
                            details.parent_oids[*parent_index].clone().into(),
                            oid.into(),
                        ]
                    }
                }
            }
            DiffTarget::Between { base_oid, head_oid } => vec![
                "diff".into(),
                self.resolve_commit(path, base_oid).await?.into(),
                self.resolve_commit(path, head_oid).await?.into(),
            ],
        };
        let mut name_args = args.clone();
        name_args.extend(os_args(&[
            "--name-status",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
            "--",
        ]));
        name_args.push(request.path.as_str().into());
        let names = self.read(Some(path), name_args).await?;
        let selected = parse_name_status(&names.stdout)?;
        // An unmerged path is reported once per stage, so the same path landing
        // here twice is expected. Only a foreign path means the request widened
        // past the single file the pane asked for.
        if selected.iter().any(|file| file.new_path != request.path) {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "Diff request must select exactly one changed file",
            ));
        }
        // Git answers an unmerged worktree path with a combined diff the patch
        // parser cannot read. Ask for the stage-2 side instead, so the pane
        // shows the conflicted working copy against "ours".
        if matches!(request.target, DiffTarget::Worktree)
            && selected
                .iter()
                .any(|file| file.status == ChangeKind::Unmerged)
        {
            args.push("--ours".into());
        }
        args.extend(os_args(&[
            "--patch",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
        ]));
        args.push(format!("--unified={}", unified_context(request)).into());
        if request.ignore_whitespace {
            args.push("--ignore-all-space".into());
        }
        args.push("--".into());
        args.push(request.path.as_str().into());
        let mut options = GitRunOptions::read_only(request.max_bytes.clamp(1, MAX_DIFF_BYTES));
        options.allow_stdout_truncation = true;
        options.timeout = Some(Duration::from_secs(60));
        let output = self
            .runner
            .run(Some(path), &args, None, CancellationToken::new(), options)
            .await?;
        parse_file_diff(&output.stdout, &request.path, output.stdout_truncated)
    }

    async fn conflict_preflight(
        &self,
        path: &Path,
        target: &str,
    ) -> ApiResult<ConflictPreflightResult> {
        // Resolve caller input before feature detection or merge simulation. The
        // merge-tree command receives only full object IDs, never an option-like ref.
        let target_oid = self.resolve_commit(path, target).await?;
        let version = self.probe().await?;
        if !supports_merge_tree_preflight(&version) {
            return Ok(ConflictPreflightResult {
                target: target.to_owned(),
                target_oid,
                state: ConflictPreflightState::Unavailable,
                conflicting_paths: Vec::new(),
                unavailable_reason: Some(
                    "Conflict preflight requires Git 2.38 or newer".to_owned(),
                ),
            });
        }

        let head_oid = self.resolve_commit(path, "HEAD").await?;
        let output = self
            .read_allow_failure(
                Some(path),
                vec![
                    "merge-tree".into(),
                    "--write-tree".into(),
                    "--name-only".into(),
                    "-z".into(),
                    "--no-messages".into(),
                    head_oid.into(),
                    target_oid.as_str().into(),
                ],
            )
            .await?;

        let exit_code = output.status.code();
        if !matches!(exit_code, Some(0 | 1)) {
            if merge_tree_preflight_unavailable(&output) {
                return Ok(ConflictPreflightResult {
                    target: target.to_owned(),
                    target_oid,
                    state: ConflictPreflightState::Unavailable,
                    conflicting_paths: Vec::new(),
                    unavailable_reason: Some(
                        "This Git build does not support conflict preflight".to_owned(),
                    ),
                });
            }
            return Err(self.runner.failure_error(&output));
        }

        let conflicting_paths = parse_merge_tree_paths(&output.stdout)?;
        let state = match exit_code {
            Some(0) if conflicting_paths.is_empty() => ConflictPreflightState::Clean,
            Some(1) if !conflicting_paths.is_empty() => ConflictPreflightState::Conflicting,
            _ => {
                return Err(ApiError::new(
                    ErrorCode::GitCommandFailed,
                    "Git conflict preflight returned an inconsistent result",
                ));
            }
        };

        Ok(ConflictPreflightResult {
            target: target.to_owned(),
            target_oid,
            state,
            conflicting_paths,
            unavailable_reason: None,
        })
    }

    async fn conflict_details(
        &self,
        path: &Path,
        conflict_path: &str,
    ) -> ApiResult<ConflictFileDetails> {
        self.ensure_conflicted_path(path, conflict_path).await?;
        let entries = self.unmerged_index_entries(path, conflict_path).await?;
        let base = match entry_at_stage(&entries, 1) {
            Some(entry) => Some(self.conflict_index_version(path, entry).await?),
            None => None,
        };
        let ours = match entry_at_stage(&entries, 2) {
            Some(entry) => Some(self.conflict_index_version(path, entry).await?),
            None => None,
        };
        let theirs = match entry_at_stage(&entries, 3) {
            Some(entry) => Some(self.conflict_index_version(path, entry).await?),
            None => None,
        };
        let worktree = self
            .inspect_conflict_result_async(path, conflict_path)
            .await?;

        Ok(ConflictFileDetails {
            path: conflict_path.to_owned(),
            expected_state: conflict_expected_state(&entries, worktree.identity),
            base,
            ours,
            theirs,
            result: worktree.content,
        })
    }

    async fn stage_paths(&self, path: &Path, paths: &[String]) -> ApiResult<MutationResult> {
        if paths.is_empty() {
            return self.mutation_result(path, self.head_oid(path).await?).await;
        }
        validate_paths(paths)?;
        let mut args = os_args(&["add", "--"]);
        args.extend(paths.iter().map(OsString::from));
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    async fn unstage_paths(&self, path: &Path, paths: &[String]) -> ApiResult<MutationResult> {
        if paths.is_empty() {
            return self.mutation_result(path, self.head_oid(path).await?).await;
        }
        validate_paths(paths)?;
        let mut args = if self.head_oid(path).await?.is_some() {
            os_args(&["restore", "--staged", "--"])
        } else {
            os_args(&["rm", "--cached", "--force", "--ignore-unmatch", "--"])
        };
        args.extend(paths.iter().map(OsString::from));
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    async fn discard_paths(&self, path: &Path, paths: &[String]) -> ApiResult<MutationResult> {
        if paths.is_empty() {
            return self.mutation_result(path, self.head_oid(path).await?).await;
        }
        validate_paths(paths)?;
        let before = self.head_oid(path).await?;

        let mut ls_args = os_args(&["ls-files", "-z", "--"]);
        ls_args.extend(paths.iter().map(OsString::from));
        let listed = self.read(Some(path), ls_args).await?;
        let listed_text = listed.stdout_lossy();
        let tracked: std::collections::HashSet<&str> = listed_text
            .split('\0')
            .filter(|entry| !entry.is_empty())
            .collect();
        let tracked_paths: Vec<&String> = paths
            .iter()
            .filter(|p| tracked.contains(p.as_str()))
            .collect();
        let untracked_paths: Vec<&String> = paths
            .iter()
            .filter(|p| !tracked.contains(p.as_str()))
            .collect();

        if !tracked_paths.is_empty() {
            let mut args = if self.head_oid(path).await?.is_some() {
                os_args(&["restore", "--staged", "--worktree", "--source=HEAD", "--"])
            } else {
                os_args(&["rm", "--cached", "--force", "--ignore-unmatch", "--"])
            };
            args.extend(tracked_paths.iter().map(|p| OsString::from(p.as_str())));
            let output = self
                .runner
                .run(Some(path), &args, None, CancellationToken::new(), {
                    let mut options = GitRunOptions::mutation(READ_OUTPUT_CAP);
                    options.allow_failure = true;
                    options
                })
                .await?;
            if !output.success() {
                return Err(self.runner.failure_error(&output));
            }
        }

        if !untracked_paths.is_empty() {
            let mut args = os_args(&["clean", "--force", "-d", "--"]);
            args.extend(untracked_paths.iter().map(|p| OsString::from(p.as_str())));
            let output = self
                .runner
                .run(Some(path), &args, None, CancellationToken::new(), {
                    let mut options = GitRunOptions::mutation(READ_OUTPUT_CAP);
                    options.allow_failure = true;
                    options
                })
                .await?;
            if !output.success() {
                return Err(self.runner.failure_error(&output));
            }
        }

        self.mutation_result(path, before).await
    }

    async fn stash_paths(
        &self,
        path: &Path,
        paths: &[String],
        message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        validate_paths(paths)?;
        if paths.is_empty() {
            return self.mutation_result(path, self.head_oid(path).await?).await;
        }
        let mut add_args = os_args(&["add", "-A", "--"]);
        add_args.extend(paths.iter().map(OsString::from));
        self.mutate(path, add_args, None, CancellationToken::new(), false)
            .await?;

        let mut args = os_args(&["stash", "push"]);
        let message = match message {
            Some(message) => Some(message.to_owned()),
            None => self.default_stash_message(path).await?,
        };
        if let Some(message) = message {
            if message.len() > MAX_COMMIT_MESSAGE_BYTES || message.contains('\0') {
                return Err(ApiError::new(
                    ErrorCode::InvalidSettings,
                    "Stash message is too large",
                ));
            }
            args.push("--message".into());
            args.push(message.into());
        }
        args.push("--".into());
        args.extend(paths.iter().map(OsString::from));
        let result = self
            .mutate(path, args, None, CancellationToken::new(), false)
            .await;
        if result.is_err() {
            let mut restore_args = os_args(&["restore", "--staged", "--"]);
            restore_args.extend(paths.iter().map(OsString::from));
            let _ = self.read_allow_failure(Some(path), restore_args).await;
        }
        result
    }

    async fn append_gitignore(
        &self,
        path: &Path,
        patterns: &[String],
    ) -> ApiResult<MutationResult> {
        let before = self.head_oid(path).await?;
        let cleaned: Vec<String> = patterns
            .iter()
            .map(|pattern| pattern.trim().to_owned())
            .filter(|pattern| !pattern.is_empty())
            .collect();
        for pattern in &cleaned {
            if pattern.contains('\0') || pattern.contains('\n') {
                return Err(ApiError::new(
                    ErrorCode::InvalidRequest,
                    "Ignore pattern contains an invalid character",
                ));
            }
        }
        if cleaned.is_empty() {
            return self.mutation_result(path, before).await;
        }

        let gitignore = path.join(".gitignore");
        let existing = std::fs::read_to_string(&gitignore).unwrap_or_default();
        let present: std::collections::HashSet<&str> =
            existing.lines().map(|line| line.trim()).collect();
        let additions: Vec<&String> = cleaned
            .iter()
            .filter(|pattern| !present.contains(pattern.as_str()))
            .collect();
        if additions.is_empty() {
            return self.mutation_result(path, before).await;
        }

        let mut next = existing;
        if !next.is_empty() && !next.ends_with('\n') {
            next.push('\n');
        }
        for pattern in additions {
            next.push_str(pattern);
            next.push('\n');
        }
        std::fs::write(&gitignore, next).map_err(|error| {
            ApiError::new(ErrorCode::Internal, "Could not update .gitignore")
                .with_details(error.to_string())
        })?;
        self.mutation_result(path, before).await
    }

    async fn create_patch(&self, path: &Path, paths: &[String], staged: bool) -> ApiResult<String> {
        validate_paths(paths)?;
        let mut args = if staged {
            os_args(&["diff", "--cached", "--"])
        } else {
            os_args(&["diff", "--"])
        };
        args.extend(paths.iter().map(OsString::from));
        let output = self.read(Some(path), args).await?;
        Ok(output.stdout_lossy())
    }

    async fn resolve_conflict(
        &self,
        path: &Path,
        conflict_path: &str,
        resolution: ConflictResolution,
        expected_state: &ConflictExpectedState,
    ) -> ApiResult<MutationResult> {
        let entries = self
            .ensure_expected_conflict(path, conflict_path, expected_state)
            .await?;

        let checkout_side = match resolution {
            ConflictResolution::Ours => Some(("--ours", 2, "ours")),
            ConflictResolution::Theirs => Some(("--theirs", 3, "theirs")),
            ConflictResolution::MarkResolved | ConflictResolution::Delete => None,
        };

        if let Some((checkout_flag, stage, side_name)) = checkout_side {
            if entry_at_stage(&entries, stage).is_none() {
                return Err(ApiError::new(
                    ErrorCode::UnsupportedOperation,
                    format!("The {side_name} side has no file content for this conflict"),
                )
                .with_details(format!(
                    "{conflict_path}: the selected index stage is absent; use the explicit delete resolution if deletion is intended."
                )));
            }

            let mut args = os_args(&["--literal-pathspecs", "checkout", checkout_flag, "--"]);
            args.push(conflict_path.into());
            self.mutate(path, args, None, CancellationToken::new(), false)
                .await?;
        }

        if resolution == ConflictResolution::Delete {
            let mut args = os_args(&[
                "--literal-pathspecs",
                "rm",
                "--force",
                "--ignore-unmatch",
                "--",
            ]);
            args.push(conflict_path.into());
            return self
                .mutate(path, args, None, CancellationToken::new(), false)
                .await;
        }

        self.stage_conflict_path(path, conflict_path).await
    }

    async fn save_conflict_result(
        &self,
        path: &Path,
        conflict_path: &str,
        text: &str,
        line_ending: ConflictLineEndingPolicy,
        expected_state: &ConflictExpectedState,
    ) -> ApiResult<MutationResult> {
        let entries = self
            .ensure_expected_conflict(path, conflict_path, expected_state)
            .await?;
        let desired_mode = [2, 3, 1]
            .into_iter()
            .filter_map(|stage| entry_at_stage(&entries, stage))
            .find(|entry| matches!(entry.mode.as_str(), "100644" | "100755"))
            .map(|entry| entry.mode.as_str())
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::UnsupportedOperation,
                    "Edited text cannot resolve a non-regular-file conflict",
                )
            })?;
        if !matches!(
            expected_state.result.kind,
            ConflictWorktreeKind::Regular | ConflictWorktreeKind::Missing
        ) {
            return Err(ApiError::new(
                ErrorCode::UnsupportedOperation,
                "Edited text cannot resolve a non-regular-file conflict",
            ));
        }
        self.write_conflict_result_async(
            path,
            conflict_path,
            text,
            line_ending,
            &expected_state.result,
            desired_mode,
        )
        .await?;
        self.stage_conflict_path(path, conflict_path).await
    }

    async fn resolve_conflicts(
        &self,
        path: &Path,
        conflict_paths: &[String],
        resolution: ConflictResolution,
    ) -> ApiResult<MutationResult> {
        validate_paths(conflict_paths)?;
        let conflicted = self.conflicted_paths(path).await?;
        let mut targets = if conflict_paths.is_empty() {
            conflicted.clone()
        } else {
            conflict_paths.to_vec()
        };
        targets.sort();
        targets.dedup();
        if targets.is_empty() {
            return self.mutation_result(path, self.head_oid(path).await?).await;
        }

        let unconflicted: Vec<&String> = targets
            .iter()
            .filter(|target| !conflicted.contains(target))
            .collect();
        if !unconflicted.is_empty() {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "Selected paths are not currently conflicted",
            )
            .with_details(summarize_paths(&unconflicted)));
        }

        let checkout_side = match resolution {
            ConflictResolution::Ours => Some(("--ours", 2u8, "ours")),
            ConflictResolution::Theirs => Some(("--theirs", 3u8, "theirs")),
            ConflictResolution::MarkResolved | ConflictResolution::Delete => None,
        };

        if let Some((checkout_flag, stage, side_name)) = checkout_side {
            let stages = self.unmerged_stages(path, &targets).await?;
            let missing: Vec<&String> = targets
                .iter()
                .filter(|target| {
                    stages.get(*target).copied().unwrap_or_default() & (1 << stage) == 0
                })
                .collect();
            if !missing.is_empty() {
                return Err(ApiError::new(
                    ErrorCode::UnsupportedOperation,
                    format!("The {side_name} side has no file content for some conflicts"),
                )
                .with_details(format!(
                    "{}: resolve these individually, or use the explicit delete resolution if deletion is intended.",
                    summarize_paths(&missing)
                )));
            }
            let before_oid = self.head_oid(path).await?;
            self.run_path_chunks(
                path,
                &["--literal-pathspecs", "checkout", checkout_flag, "--"],
                &targets,
            )
            .await?;
            self.run_path_chunks(path, &["--literal-pathspecs", "add", "--"], &targets)
                .await?;
            return self.mutation_result(path, before_oid).await;
        }

        let before_oid = self.head_oid(path).await?;
        let prefix: &[&str] = if resolution == ConflictResolution::Delete {
            &[
                "--literal-pathspecs",
                "rm",
                "--force",
                "--ignore-unmatch",
                "--",
            ]
        } else {
            &["--literal-pathspecs", "add", "--all", "--"]
        };
        self.run_path_chunks(path, prefix, &targets).await?;
        self.mutation_result(path, before_oid).await
    }

    async fn auto_resolve_conflicts(&self, path: &Path) -> ApiResult<MutationResult> {
        // `rerere` only reuses a repository-local resolution recorded for the
        // exact conflict preimage. It never chooses current or incoming content.
        self.mutate(
            path,
            os_args(&[
                "-c",
                "rerere.enabled=true",
                "-c",
                "rerere.autoupdate=true",
                "rerere",
                "--rerere-autoupdate",
            ]),
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    async fn create_commit(
        &self,
        path: &Path,
        options: &CommitOptions,
    ) -> ApiResult<MutationResult> {
        validate_message(&options.message)?;
        let mut args = os_args(&["commit", "-F", "-"]);
        if options.amend {
            args.push("--amend".into());
        }
        if options.signoff {
            args.push("--signoff".into());
        }
        self.mutate(
            path,
            args,
            Some(options.message.as_bytes()),
            CancellationToken::new(),
            false,
        )
        .await
    }

    async fn create_initial_commit(&self, path: &Path, message: &str) -> ApiResult<MutationResult> {
        validate_message(message)?;
        if self.head_oid(path).await?.is_some() {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "The repository already has a commit",
            ));
        }
        let (_, _, parsed) = self.generation_and_refs(path).await?;
        // Whatever the user staged is the commit they meant to make; the README
        // is only for the repository that has nothing staged at all.
        let staged = parsed
            .status
            .entries
            .iter()
            .any(|entry| entry.index.is_some() && !entry.conflicted);
        if !staged {
            let readme = self.write_initial_readme(path).await?;
            let mut args = os_args(&["add", "--"]);
            args.push(OsString::from(readme));
            self.mutate(path, args, None, CancellationToken::new(), false)
                .await?;
        }
        self.mutate(
            path,
            os_args(&["commit", "-F", "-"]),
            Some(message.as_bytes()),
            CancellationToken::new(),
            false,
        )
        .await
    }

    async fn reword_commit(
        &self,
        path: &Path,
        oid: &str,
        message: &str,
    ) -> ApiResult<MutationResult> {
        validate_message(message)?;
        let target = self.resolve_commit(path, oid).await?;

        // Rewriting is only defined while no merge/rebase/cherry-pick is in flight.
        if self.operation_state(path).await? != RepositoryOperationState::Normal {
            return Err(ApiError::new(
                ErrorCode::OperationInProgress,
                "Finish the in-progress Git operation before editing a commit message",
            ));
        }

        let head = self.head_oid(path).await?.ok_or_else(|| {
            ApiError::new(
                ErrorCode::InvalidRequest,
                "The repository has no commit to edit",
            )
        })?;

        // Amending HEAD needs no replay and never touches the working tree.
        if target == head {
            return self
                .mutate(
                    path,
                    os_args(&["commit", "--amend", "--only", "-F", "-"]),
                    Some(message.as_bytes()),
                    CancellationToken::new(),
                    false,
                )
                .await;
        }

        // Older commits are reworded by rebuilding the commit object and
        // replaying its descendants onto it. The tree is byte-identical, so the
        // replay never conflicts; these guards keep it from silently losing
        // history it cannot faithfully reproduce.
        if !self.is_ancestor(path, &target, &head).await? {
            return Err(ApiError::new(
                ErrorCode::UnsupportedOperation,
                "Only commits reachable from the current branch can be edited",
            ));
        }
        if self.range_has_merge(path, &target, &head).await? {
            return Err(ApiError::new(
                ErrorCode::UnsupportedOperation,
                "Cannot edit this message because a later commit is a merge",
            ));
        }

        let rebuilt = self.rebuild_commit_message(path, &target, message).await?;
        let mut args = os_args(&["rebase", "--autostash", "--onto"]);
        args.push(rebuilt.into());
        args.push(target.into());
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    async fn create_branch(
        &self,
        path: &Path,
        name: &str,
        start_oid: &str,
        checkout: bool,
    ) -> ApiResult<MutationResult> {
        self.validate_branch_name(name).await?;
        let oid = self.resolve_commit(path, start_oid).await?;
        let args = if checkout {
            vec!["switch".into(), "-c".into(), name.into(), oid.into()]
        } else {
            vec!["branch".into(), "--".into(), name.into(), oid.into()]
        };
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    async fn checkout_branch(&self, path: &Path, name: &str) -> ApiResult<MutationResult> {
        self.validate_branch_name(name).await?;
        self.mutate(
            path,
            vec!["switch".into(), name.into()],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    async fn rename_branch(
        &self,
        path: &Path,
        old_name: &str,
        new_name: &str,
    ) -> ApiResult<MutationResult> {
        self.validate_branch_name(old_name).await?;
        self.validate_branch_name(new_name).await?;
        self.mutate(
            path,
            vec![
                "branch".into(),
                "-m".into(),
                old_name.into(),
                new_name.into(),
            ],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    async fn delete_branch(
        &self,
        path: &Path,
        name: &str,
        force: bool,
        confirmed: bool,
    ) -> ApiResult<MutationResult> {
        self.validate_branch_name(name).await?;
        if force && !confirmed {
            return Err(confirmation_required(
                "Force-deleting a branch requires confirmation",
            ));
        }
        let mode = if force { "-D" } else { "-d" };
        self.mutate(
            path,
            vec!["branch".into(), mode.into(), "--".into(), name.into()],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    async fn set_upstream(
        &self,
        path: &Path,
        branch: &str,
        upstream: &str,
    ) -> ApiResult<MutationResult> {
        self.validate_branch_name(branch).await?;
        self.validate_branch_name(upstream).await?;
        let upstream_ref = format!("refs/remotes/{upstream}");
        let verify = self
            .read_allow_failure(
                Some(path),
                vec![
                    "show-ref".into(),
                    "--verify".into(),
                    "--quiet".into(),
                    upstream_ref.into(),
                ],
            )
            .await?;
        if !verify.success() {
            return Err(ApiError::new(
                ErrorCode::InvalidRefName,
                "Selected upstream branch does not exist",
            ));
        }
        self.mutate(
            path,
            vec![
                "branch".into(),
                format!("--set-upstream-to={upstream}").into(),
                "--".into(),
                branch.into(),
            ],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    async fn merge_branch(&self, path: &Path, branch: &str) -> ApiResult<MutationResult> {
        self.validate_branch_name(branch).await?;
        let oid = self.resolve_commit(path, branch).await?;
        self.mutate(
            path,
            vec!["merge".into(), "--no-edit".into(), oid.into()],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    async fn add_remote(&self, path: &Path, name: &str, url: &str) -> ApiResult<MutationResult> {
        validate_remote_name(name)?;
        validate_remote_url(url)?;
        self.mutate(
            path,
            vec!["remote".into(), "add".into(), name.into(), url.into()],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    async fn fetch(
        &self,
        path: &Path,
        options: &FetchOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        let remotes = self
            .validate_remote_selection(path, options.remote.as_deref())
            .await?;
        // Every remote is fetched by its own command rather than through
        // `fetch --all`, so each one reaches a single host and can carry the
        // token issued for that host.
        let targets: Vec<String> = match &options.remote {
            Some(remote) => vec![remote.clone()],
            None => remotes.iter().map(|entry| entry.name.clone()).collect(),
        };
        let mut result = None;
        for target in targets {
            let mut args = os_args(&["fetch", "--progress"]);
            if options.prune {
                args.push("--prune".into());
            }
            if options.tags {
                args.push("--tags".into());
            }
            args.push("--".into());
            args.push(target.as_str().into());
            let run = self.network_options(&remotes, Some(target.as_str())).await;
            result = Some(
                self.mutate_network(path, args, cancellation.clone(), run)
                    .await?,
            );
        }
        result.ok_or_else(|| {
            ApiError::new(
                ErrorCode::UpstreamMissing,
                "Repository has no configured remote",
            )
        })
    }

    async fn pull(
        &self,
        path: &Path,
        options: &PullOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        let remotes = self
            .validate_remote_selection(path, options.remote.as_deref())
            .await?;
        if options.branch.is_some() && options.remote.is_none() {
            return Err(ApiError::new(
                ErrorCode::InvalidSettings,
                "Pull branch requires an explicit remote",
            ));
        }
        let mut args = os_args(&["pull", "--progress"]);
        match options.mode {
            PullMode::Merge => {
                args.push("--no-rebase".into());
                args.push("--ff".into());
            }
            PullMode::FastForwardOnly => {
                args.push("--ff-only".into());
                args.push("--no-rebase".into());
            }
            PullMode::Rebase => args.push("--rebase".into()),
        }
        if options.prune {
            args.push("--prune".into());
        }
        if options.autostash {
            args.push("--autostash".into());
        }
        if let Some(remote) = &options.remote {
            args.push("--".into());
            args.push(remote.into());
            if let Some(branch) = &options.branch {
                self.validate_branch_name(branch).await?;
                args.push(branch.into());
            }
        }
        let run = self
            .network_options(&remotes, options.remote.as_deref())
            .await;
        self.mutate_network(path, args, cancellation, run).await
    }

    async fn push(
        &self,
        path: &Path,
        options: &PushOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        if options.branch.is_some() && options.remote.is_none() {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "Pushing an explicit branch requires an explicit remote",
            ));
        }
        let remotes = self
            .validate_remote_selection(path, options.remote.as_deref())
            .await?;
        if options.set_upstream && (options.remote.is_none() || options.branch.is_none()) {
            return Err(ApiError::new(
                ErrorCode::InvalidSettings,
                "Setting upstream requires an explicit remote and branch",
            ));
        }
        // Setting the upstream is a change to the repository's own
        // configuration, so it runs even when the commits are already there.
        // A forced push is an order to make the remote match this branch, and
        // the local view of the remote is exactly what may be out of date, so
        // it is never answered from the tracking ref either.
        if !options.set_upstream
            && options.force == PushForce::None
            && let Some((local, tracking)) = self.push_destination_state(path, options).await?
            && local == tracking
        {
            let before_oid = self.head_oid(path).await?;
            let mut result = self.mutation_result(path, before_oid).await?;
            result.notice = Some("Everything up-to-date".to_owned());
            return Ok(result);
        }
        let mut args = os_args(&["push", "--porcelain", "--progress"]);
        match options.force {
            PushForce::None => {}
            // `--force-if-includes` is what makes the lease mean what the user
            // read: without it a background fetch that moved the tracking ref
            // satisfies the lease on work this branch never saw.
            PushForce::WithLease => {
                args.push("--force-with-lease".into());
                args.push("--force-if-includes".into());
            }
            PushForce::Force => args.push("--force".into()),
        }
        if options.set_upstream {
            args.push("--set-upstream".into());
        }
        if let Some(remote) = &options.remote {
            args.push("--".into());
            args.push(remote.into());
            if let Some(branch) = &options.branch {
                self.validate_branch_name(branch).await?;
                args.push(branch.into());
            }
        }
        let run = self
            .network_options(&remotes, options.remote.as_deref())
            .await;
        self.mutate_network(path, args, cancellation, run).await
    }

    async fn checkout_commit(&self, path: &Path, oid: &str) -> ApiResult<MutationResult> {
        let oid = self.resolve_commit(path, oid).await?;
        self.mutate(
            path,
            vec!["switch".into(), "--detach".into(), oid.into()],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    async fn create_tag(
        &self,
        path: &Path,
        name: &str,
        oid: &str,
        message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        self.validate_tag_name(name).await?;
        let oid = self.resolve_commit(path, oid).await?;
        let (args, stdin) = if let Some(message) = message {
            validate_message(message)?;
            (
                vec![
                    "tag".into(),
                    "-a".into(),
                    "-F".into(),
                    "-".into(),
                    name.into(),
                    oid.into(),
                ],
                Some(message.as_bytes()),
            )
        } else {
            (
                vec!["tag".into(), "--".into(), name.into(), oid.into()],
                None,
            )
        };
        self.mutate(path, args, stdin, CancellationToken::new(), false)
            .await
    }

    async fn cherry_pick(
        &self,
        path: &Path,
        oid: &str,
        mainline_parent: Option<u32>,
    ) -> ApiResult<MutationResult> {
        let oid = self.resolve_commit(path, oid).await?;
        let parents = self.commit_parent_oids(path, &oid).await?;
        validate_mainline_parent(parents.len(), mainline_parent)?;
        let mut args = os_args(&["cherry-pick"]);
        if let Some(parent) = mainline_parent {
            args.extend(["-m".into(), parent.to_string().into()]);
        }
        args.push(oid.into());
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    async fn revert_commit(
        &self,
        path: &Path,
        oid: &str,
        mainline_parent: Option<u32>,
    ) -> ApiResult<MutationResult> {
        let oid = self.resolve_commit(path, oid).await?;
        let parents = self.commit_parent_oids(path, &oid).await?;
        validate_mainline_parent(parents.len(), mainline_parent)?;
        let mut args = os_args(&["revert", "--no-edit"]);
        if let Some(parent) = mainline_parent {
            args.extend(["-m".into(), parent.to_string().into()]);
        }
        args.push(oid.into());
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    async fn reset_to_commit(
        &self,
        path: &Path,
        oid: &str,
        mode: ResetMode,
        confirmed: bool,
    ) -> ApiResult<MutationResult> {
        if mode == ResetMode::Hard && !confirmed {
            return Err(confirmation_required("Hard reset requires confirmation"));
        }
        let snapshot = self.snapshot(path).await?;
        if snapshot.operation_state != RepositoryOperationState::Normal {
            return Err(ApiError::new(
                ErrorCode::OperationInProgress,
                "Finish or abort the current Git operation before resetting",
            ));
        }
        if !matches!(snapshot.head, HeadState::Branch { .. }) {
            return Err(ApiError::new(
                ErrorCode::UnsupportedOperation,
                "Reset is only available while a local branch is checked out",
            ));
        }
        let oid = self.resolve_commit(path, oid).await?;
        let flag = match mode {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
        };
        self.mutate(
            path,
            vec!["reset".into(), flag.into(), oid.into()],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    async fn commit_action_availability(
        &self,
        path: &Path,
        oid: &str,
    ) -> ApiResult<Vec<CommitActionAvailability>> {
        let oid = self.resolve_commit(path, oid).await?;
        let (status_output, operation_state) =
            tokio::try_join!(self.status_output(path), self.operation_state(path))?;
        let parsed_status = parse_status(&status_output.stdout)?;
        let operation_busy = operation_state != RepositoryOperationState::Normal;
        let dirty = !parsed_status.status.clean;
        let reset_unavailable = !matches!(parsed_status.head, HeadState::Branch { .. });
        let head_oid = match &parsed_status.head {
            HeadState::Branch { oid, .. } | HeadState::Detached { oid } => Some(oid.as_str()),
            HeadState::Unborn { .. } => None,
        };
        let target_in_head_history = if let Some(head_oid) = head_oid {
            Some(self.is_ancestor(path, &oid, head_oid).await?)
        } else {
            None
        };
        let cherry_pick_unavailable = match target_in_head_history {
            Some(true) => Some("Commit is already in the current HEAD history"),
            Some(false) => None,
            None => Some("Check out a commit before cherry-picking"),
        };
        let revert_unavailable = match target_in_head_history {
            Some(true) => None,
            Some(false) => Some("Commit is not in the current HEAD history"),
            None => Some("Check out a commit before reverting"),
        };
        // Rewording replays the descendants of the target, so it needs the same
        // guards `reword_commit` itself enforces: reachable from HEAD and no
        // merge in the replayed range.
        let reword_unavailable = match (head_oid, target_in_head_history) {
            (Some(head_oid), Some(true)) => {
                if head_oid == oid.as_str() {
                    None
                } else if self.range_has_merge(path, &oid, head_oid).await? {
                    Some("A later commit is a merge, so this message cannot be edited")
                } else {
                    None
                }
            }
            (_, Some(false)) => {
                Some("Only commits reachable from the current branch can be edited")
            }
            _ => Some("Check out a commit before editing its message"),
        };
        let action = |kind, requires_clean, requires_confirmation, unavailable: Option<&str>| {
            let disabled_reason = if operation_busy {
                Some("Finish or abort the current Git operation first".to_owned())
            } else if kind == CommitActionKind::Reset && reset_unavailable {
                Some("Check out a local branch before resetting".to_owned())
            } else if let Some(reason) = unavailable {
                Some(reason.to_owned())
            } else if requires_clean && dirty {
                Some("Working tree must be clean for this action".to_owned())
            } else {
                None
            };
            CommitActionAvailability {
                kind,
                enabled: disabled_reason.is_none(),
                disabled_reason,
                requires_confirmation,
            }
        };
        Ok(vec![
            action(CommitActionKind::Checkout, true, false, None),
            action(CommitActionKind::CreateBranch, false, false, None),
            action(
                CommitActionKind::CherryPick,
                true,
                false,
                cherry_pick_unavailable,
            ),
            action(CommitActionKind::Revert, true, false, revert_unavailable),
            action(CommitActionKind::Reset, false, true, None),
            action(CommitActionKind::CreateTag, false, false, None),
            action(CommitActionKind::Reword, false, false, reword_unavailable),
            CommitActionAvailability {
                kind: CommitActionKind::CopySha,
                enabled: true,
                disabled_reason: None,
                requires_confirmation: false,
            },
        ])
    }

    async fn continue_operation(
        &self,
        path: &Path,
        operation: ContinueOperation,
    ) -> ApiResult<MutationResult> {
        ensure_operation(self.operation_state(path).await?, operation)?;
        let args = match operation {
            ContinueOperation::Merge => os_args(&["merge", "--continue"]),
            ContinueOperation::Rebase => os_args(&["rebase", "--continue"]),
            ContinueOperation::CherryPick => os_args(&["cherry-pick", "--continue"]),
            ContinueOperation::Revert => os_args(&["revert", "--continue"]),
        };
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    async fn abort_operation(
        &self,
        path: &Path,
        operation: ContinueOperation,
    ) -> ApiResult<MutationResult> {
        ensure_operation(self.operation_state(path).await?, operation)?;
        let args = match operation {
            ContinueOperation::Merge => os_args(&["merge", "--abort"]),
            ContinueOperation::Rebase => os_args(&["rebase", "--abort"]),
            ContinueOperation::CherryPick => os_args(&["cherry-pick", "--abort"]),
            ContinueOperation::Revert => os_args(&["revert", "--abort"]),
        };
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    async fn skip_operation(
        &self,
        path: &Path,
        operation: ContinueOperation,
    ) -> ApiResult<MutationResult> {
        ensure_operation(self.operation_state(path).await?, operation)?;
        let args = match operation {
            ContinueOperation::Merge => {
                return Err(ApiError::new(
                    ErrorCode::UnsupportedOperation,
                    "A merge cannot skip a commit; resolve the conflicts or abort the merge",
                ));
            }
            ContinueOperation::Rebase => os_args(&["rebase", "--skip"]),
            ContinueOperation::CherryPick => os_args(&["cherry-pick", "--skip"]),
            ContinueOperation::Revert => os_args(&["revert", "--skip"]),
        };
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    async fn stash_list(&self, path: &Path) -> ApiResult<Vec<StashEntry>> {
        Ok(self.stash_graph(path).await?.entries())
    }

    async fn stash_push(
        &self,
        path: &Path,
        message: Option<&str>,
        include_untracked: bool,
    ) -> ApiResult<MutationResult> {
        if include_untracked {
            self.mutate(
                path,
                os_args(&["add", "-A", "--"]),
                None,
                CancellationToken::new(),
                false,
            )
            .await?;
        }
        let mut args = os_args(&["stash", "push"]);
        let message = match message {
            Some(message) => Some(message.to_owned()),
            None => self.default_stash_message(path).await?,
        };
        if let Some(message) = message {
            if message.len() > MAX_COMMIT_MESSAGE_BYTES || message.contains('\0') {
                return Err(ApiError::new(
                    ErrorCode::InvalidSettings,
                    "Stash message is too large",
                ));
            }
            args.push("--message".into());
            args.push(message.into());
        }
        let result = self
            .mutate(path, args, None, CancellationToken::new(), false)
            .await;
        if include_untracked && result.is_err() {
            let _ = self
                .read_allow_failure(Some(path), os_args(&["restore", "--staged", "--", "."]))
                .await;
        }
        result
    }

    async fn stash_apply(&self, path: &Path, oid: &str, pop: bool) -> ApiResult<MutationResult> {
        let index = resolve_stash_index(self.stash_list(path).await?, oid)?;
        let action = if pop { "pop" } else { "apply" };
        self.mutate(
            path,
            vec![
                "stash".into(),
                action.into(),
                format!("stash@{{{index}}}").into(),
            ],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    async fn stash_drop(
        &self,
        path: &Path,
        oid: &str,
        confirmed: bool,
    ) -> ApiResult<MutationResult> {
        if !confirmed {
            return Err(confirmation_required(
                "Dropping a stash requires confirmation",
            ));
        }
        let index = resolve_stash_index(self.stash_list(path).await?, oid)?;
        self.mutate(
            path,
            vec![
                "stash".into(),
                "drop".into(),
                format!("stash@{{{index}}}").into(),
            ],
            None,
            CancellationToken::new(),
            false,
        )
        .await
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
#[path = "backend/tests.rs"]
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
