use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use async_trait::async_trait;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use super::*;

struct MockBackend {
    calls: StdMutex<Vec<(&'static str, PathBuf)>>,
    active_mutations: AtomicUsize,
    max_active_mutations: AtomicUsize,
    mutation_delay: Duration,
    fetch_saw_cancelled: AtomicBool,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self {
            calls: StdMutex::new(Vec::new()),
            active_mutations: AtomicUsize::new(0),
            max_active_mutations: AtomicUsize::new(0),
            mutation_delay: Duration::from_millis(30),
            fetch_saw_cancelled: AtomicBool::new(false),
        }
    }
}

impl MockBackend {
    fn record(&self, name: &'static str, path: &Path) {
        self.calls
            .lock()
            .expect("call log lock poisoned")
            .push((name, path.to_path_buf()));
    }

    async fn mutation(&self, name: &'static str, path: &Path) -> ApiResult<MutationResult> {
        self.record(name, path);
        let active = self.active_mutations.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active_mutations
            .fetch_max(active, Ordering::SeqCst);
        tokio::time::sleep(self.mutation_delay).await;
        self.active_mutations.fetch_sub(1, Ordering::SeqCst);
        Ok(mutation_result())
    }
}

#[async_trait]
impl GitBackend for MockBackend {
    async fn probe(&self) -> ApiResult<GitVersion> {
        Ok(GitVersion {
            major: 2,
            minor: 50,
            patch: 0,
            raw: "git version 2.50.0".into(),
        })
    }

    async fn open_repository(&self, path: &Path) -> ApiResult<RepositoryInfo> {
        self.record("open_repository", path);
        Ok(repository_info(path))
    }

    async fn init_repository(
        &self,
        path: &Path,
        _default_branch: &str,
    ) -> ApiResult<RepositoryInfo> {
        self.record("init_repository", path);
        Ok(repository_info(path))
    }

    async fn clone_repository(
        &self,
        options: &CloneOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<RepositoryInfo> {
        if cancellation.is_cancelled() {
            return Err(ApiError::new(ErrorCode::Cancelled, "Clone cancelled"));
        }
        let path = PathBuf::from(&options.destination);
        self.record("clone_repository", &path);
        Ok(repository_info(&path))
    }

    async fn snapshot(&self, path: &Path) -> ApiResult<RepositorySnapshot> {
        self.record("snapshot", path);
        Ok(repository_snapshot())
    }

    async fn history(&self, path: &Path, _query: &HistoryQuery) -> ApiResult<HistoryPage> {
        self.record("history", path);
        Ok(HistoryPage {
            generation: "g1".into(),
            commits: Vec::new(),
            next_cursor: None,
            has_more: false,
        })
    }

    async fn search_commits(
        &self,
        path: &Path,
        _query: &CommitSearchQuery,
    ) -> ApiResult<CommitSearchResult> {
        self.record("search_commits", path);
        Ok(CommitSearchResult {
            total: 0,
            truncated: false,
            hits: Vec::new(),
        })
    }

    async fn commit_details(
        &self,
        path: &Path,
        oid: &str,
        _parent_index: usize,
    ) -> ApiResult<CommitDetails> {
        self.record("commit_details", path);
        Ok(commit_details(oid))
    }

    async fn diff(&self, path: &Path, request: &DiffRequest) -> ApiResult<FileDiff> {
        self.record("diff", path);
        Ok(FileDiff {
            old_path: None,
            new_path: request.path.clone(),
            old_mode: None,
            new_mode: None,
            status: ChangeKind::Modified,
            binary: false,
            stats: DiffStats {
                files: 1,
                additions: 0,
                deletions: 0,
            },
            hunks: Vec::new(),
            truncated: false,
        })
    }

    async fn conflict_preflight(
        &self,
        path: &Path,
        target: &str,
    ) -> ApiResult<ConflictPreflightResult> {
        self.record("conflict_preflight", path);
        Ok(ConflictPreflightResult {
            target: target.to_owned(),
            target_oid: "1111111111111111111111111111111111111111".into(),
            state: ConflictPreflightState::Clean,
            conflicting_paths: Vec::new(),
            unavailable_reason: None,
        })
    }

    async fn conflict_details(
        &self,
        path: &Path,
        conflict_path: &str,
    ) -> ApiResult<ConflictFileDetails> {
        self.record("conflict_details", path);
        Ok(ConflictFileDetails {
            path: conflict_path.to_owned(),
            expected_state: ConflictExpectedState {
                base: None,
                ours: None,
                theirs: None,
                result: ConflictWorktreeIdentity {
                    kind: ConflictWorktreeKind::Missing,
                    size: None,
                    sha256: None,
                    line_ending: None,
                    mode: None,
                },
            },
            base: None,
            ours: None,
            theirs: None,
            result: ConflictFileContent {
                kind: ConflictContentKind::Missing,
                size: None,
                text: None,
                line_ending: None,
            },
        })
    }

    async fn stage_paths(&self, path: &Path, _paths: &[String]) -> ApiResult<MutationResult> {
        self.mutation("stage_paths", path).await
    }

    async fn update_submodule(&self, path: &Path, relative: &str) -> ApiResult<MutationResult> {
        self.mutation("update_submodule", &path.join(relative))
            .await
    }

    async fn unstage_paths(&self, path: &Path, _paths: &[String]) -> ApiResult<MutationResult> {
        self.mutation("unstage_paths", path).await
    }

    async fn discard_paths(&self, path: &Path, _paths: &[String]) -> ApiResult<MutationResult> {
        self.mutation("discard_paths", path).await
    }

    async fn apply_diff_lines(
        &self,
        path: &Path,
        _request: &LinePatchRequest,
    ) -> ApiResult<MutationResult> {
        self.mutation("apply_diff_lines", path).await
    }

    async fn stash_paths(
        &self,
        path: &Path,
        _paths: &[String],
        _message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        self.mutation("stash_paths", path).await
    }

    async fn append_gitignore(
        &self,
        path: &Path,
        _patterns: &[String],
    ) -> ApiResult<MutationResult> {
        self.mutation("append_gitignore", path).await
    }

    async fn create_patch(
        &self,
        path: &Path,
        _paths: &[String],
        _staged: bool,
    ) -> ApiResult<String> {
        self.record("create_patch", path);
        Ok(String::new())
    }

    async fn resolve_conflict(
        &self,
        path: &Path,
        _conflict_path: &str,
        _resolution: ConflictResolution,
        _expected_state: &ConflictExpectedState,
    ) -> ApiResult<MutationResult> {
        self.mutation("resolve_conflict", path).await
    }

    async fn save_conflict_result(
        &self,
        path: &Path,
        _conflict_path: &str,
        _text: &str,
        _line_ending: ConflictLineEndingPolicy,
        _expected_state: &ConflictExpectedState,
    ) -> ApiResult<MutationResult> {
        self.mutation("save_conflict_result", path).await
    }

    async fn resolve_conflicts(
        &self,
        path: &Path,
        _conflict_paths: &[String],
        _resolution: ConflictResolution,
    ) -> ApiResult<MutationResult> {
        self.mutation("resolve_conflicts", path).await
    }

    async fn auto_resolve_conflicts(&self, path: &Path) -> ApiResult<MutationResult> {
        self.mutation("auto_resolve_conflicts", path).await
    }

    async fn create_commit(
        &self,
        path: &Path,
        _options: &CommitOptions,
    ) -> ApiResult<MutationResult> {
        self.mutation("create_commit", path).await
    }

    async fn create_initial_commit(
        &self,
        path: &Path,
        _message: &str,
    ) -> ApiResult<MutationResult> {
        self.mutation("create_initial_commit", path).await
    }

    async fn reword_commit(
        &self,
        path: &Path,
        _oid: &str,
        _message: &str,
    ) -> ApiResult<MutationResult> {
        self.mutation("reword_commit", path).await
    }

    async fn create_branch(
        &self,
        path: &Path,
        _name: &str,
        _start_oid: &str,
        _checkout: bool,
    ) -> ApiResult<MutationResult> {
        self.mutation("create_branch", path).await
    }

    async fn checkout_branch(&self, path: &Path, _name: &str) -> ApiResult<MutationResult> {
        self.mutation("checkout_branch", path).await
    }

    async fn rename_branch(
        &self,
        path: &Path,
        _old_name: &str,
        _new_name: &str,
    ) -> ApiResult<MutationResult> {
        self.mutation("rename_branch", path).await
    }

    async fn delete_branch(
        &self,
        path: &Path,
        _name: &str,
        _force: bool,
        _confirmed: bool,
    ) -> ApiResult<MutationResult> {
        self.mutation("delete_branch", path).await
    }

    async fn delete_remote_branch(
        &self,
        path: &Path,
        _remote: &str,
        _branch: &str,
        _confirmed: bool,
    ) -> ApiResult<MutationResult> {
        self.mutation("delete_remote_branch", path).await
    }

    async fn set_upstream(
        &self,
        path: &Path,
        _branch: &str,
        _upstream: &str,
    ) -> ApiResult<MutationResult> {
        self.mutation("set_upstream", path).await
    }

    async fn merge_branch(&self, path: &Path, _branch: &str) -> ApiResult<MutationResult> {
        self.mutation("merge_branch", path).await
    }

    async fn add_remote(&self, path: &Path, _name: &str, _url: &str) -> ApiResult<MutationResult> {
        self.mutation("add_remote", path).await
    }

    async fn fetch(
        &self,
        path: &Path,
        _options: &FetchOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        let cancelled = cancellation.is_cancelled();
        self.fetch_saw_cancelled.store(cancelled, Ordering::SeqCst);
        if cancelled {
            self.record("fetch", path);
            return Err(ApiError::new(ErrorCode::Cancelled, "Fetch cancelled"));
        }
        self.mutation("fetch", path).await
    }

    async fn pull(
        &self,
        path: &Path,
        _options: &PullOptions,
        _cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        self.mutation("pull", path).await
    }

    async fn push(
        &self,
        path: &Path,
        _options: &PushOptions,
        _cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        self.mutation("push", path).await
    }

    async fn checkout_commit(&self, path: &Path, _oid: &str) -> ApiResult<MutationResult> {
        self.mutation("checkout_commit", path).await
    }

    async fn create_tag(
        &self,
        path: &Path,
        _name: &str,
        _oid: &str,
        _message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        self.mutation("create_tag", path).await
    }

    async fn cherry_pick(
        &self,
        path: &Path,
        _oid: &str,
        _mainline_parent: Option<u32>,
        _commit: bool,
    ) -> ApiResult<MutationResult> {
        self.mutation("cherry_pick", path).await
    }

    async fn revert_commit(
        &self,
        path: &Path,
        _oid: &str,
        _mainline_parent: Option<u32>,
        _commit: bool,
    ) -> ApiResult<MutationResult> {
        self.mutation("revert_commit", path).await
    }

    async fn reset_to_commit(
        &self,
        path: &Path,
        _oid: &str,
        _mode: ResetMode,
        _confirmed: bool,
    ) -> ApiResult<MutationResult> {
        self.mutation("reset_to_commit", path).await
    }

    async fn commit_action_availability(
        &self,
        path: &Path,
        _oid: &str,
    ) -> ApiResult<Vec<CommitActionAvailability>> {
        self.record("commit_action_availability", path);
        Ok(Vec::new())
    }

    async fn continue_operation(
        &self,
        path: &Path,
        _operation: ContinueOperation,
        _message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        self.mutation("continue_operation", path).await
    }

    async fn abort_operation(
        &self,
        path: &Path,
        _operation: ContinueOperation,
    ) -> ApiResult<MutationResult> {
        self.mutation("abort_operation", path).await
    }

    async fn skip_operation(
        &self,
        path: &Path,
        _operation: ContinueOperation,
    ) -> ApiResult<MutationResult> {
        self.mutation("skip_operation", path).await
    }

    async fn stash_list(&self, path: &Path) -> ApiResult<Vec<StashEntry>> {
        self.record("stash_list", path);
        Ok(Vec::new())
    }

    async fn stash_push(
        &self,
        path: &Path,
        _message: Option<&str>,
        _include_untracked: bool,
    ) -> ApiResult<MutationResult> {
        self.mutation("stash_push", path).await
    }

    async fn stash_apply(&self, path: &Path, _oid: &str, _pop: bool) -> ApiResult<MutationResult> {
        self.mutation("stash_apply", path).await
    }

    async fn stash_drop(
        &self,
        path: &Path,
        _oid: &str,
        _confirmed: bool,
    ) -> ApiResult<MutationResult> {
        self.mutation("stash_drop", path).await
    }
}

#[tokio::test]
async fn open_deduplicates_canonical_roots_and_close_rejects_stale_ids() {
    let temp = TempDir::new().expect("temp repository");
    let backend = Arc::new(MockBackend::default());
    let api = CoreApi::new(backend.clone());

    let (first_id, _) = api.open(temp.path()).await.expect("first open");
    let equivalent_path = temp.path().join(".");
    let (second_id, _) = api.open(&equivalent_path).await.expect("second open");
    assert_eq!(first_id, second_id);

    api.snapshot(&first_id).await.expect("open snapshot");
    let canonical = dunce::canonicalize(temp.path()).expect("canonical temp path");
    let snapshot_path = backend
        .calls
        .lock()
        .expect("call log lock poisoned")
        .iter()
        .find_map(|(name, path)| (*name == "snapshot").then(|| path.clone()))
        .expect("snapshot call");
    assert_eq!(snapshot_path, canonical);

    api.close(&first_id).await.expect("close repository");
    let snapshot_error = api.snapshot(&first_id).await.expect_err("closed snapshot");
    assert_eq!(snapshot_error.code, ErrorCode::RepositoryClosed);
    let close_error = api.close(&first_id).await.expect_err("second close");
    assert_eq!(close_error.code, ErrorCode::RepositoryClosed);
}

#[tokio::test]
async fn unknown_ids_fail_before_backend_dispatch() {
    let backend = Arc::new(MockBackend::default());
    let api = CoreApi::new(backend.clone());
    let unknown = RepositoryId::new();

    let read_error = api.snapshot(&unknown).await.expect_err("unknown snapshot");
    let write_error = api
        .stage(&unknown, &["file.txt".into()])
        .await
        .expect_err("unknown stage");

    assert_eq!(read_error.code, ErrorCode::RepositoryClosed);
    assert_eq!(write_error.code, ErrorCode::RepositoryClosed);
    assert!(
        backend
            .calls
            .lock()
            .expect("call log lock poisoned")
            .is_empty()
    );
}

#[test]
fn rejects_unbounded_ui_queries_before_backend_dispatch() {
    let oversized_history = HistoryQuery {
        limit: MAX_HISTORY_PAGE_SIZE + 1,
        ..HistoryQuery::default()
    };
    assert_eq!(
        validate_history_query(&oversized_history).unwrap_err().code,
        ErrorCode::InvalidRequest
    );

    let oversized_search = CommitSearchQuery {
        query: "x".into(),
        scope: HistoryScope::AllRefs,
        limit: MAX_SEARCH_RESULTS + 1,
    };
    assert_eq!(
        validate_search_query(&oversized_search).unwrap_err().code,
        ErrorCode::InvalidRequest
    );

    let unsafe_diff = DiffRequest {
        target: DiffTarget::Worktree,
        path: "../outside".into(),
        context_lines: 3,
        ignore_whitespace: false,
        max_bytes: 1024,
        whole_file: false,
        old_path: None,
    };
    assert_eq!(
        validate_diff_request(&unsafe_diff).unwrap_err().code,
        ErrorCode::InvalidPath
    );
}

#[test]
fn expected_state_detects_stale_head_or_generation() {
    let snapshot = repository_snapshot();
    let stale = ExpectedState {
        head_oid: None,
        generation: "old".into(),
    };
    assert_eq!(
        validate_expected_state(&snapshot, &stale).unwrap_err().code,
        ErrorCode::StaleSnapshot
    );

    let current = ExpectedState {
        head_oid: None,
        generation: snapshot.generation.clone(),
    };
    validate_expected_state(&snapshot, &current).unwrap();
}

#[tokio::test]
async fn mutations_serialize_per_repository_but_not_globally() {
    let first_temp = TempDir::new().expect("first temp repository");
    let second_temp = TempDir::new().expect("second temp repository");
    let backend = Arc::new(MockBackend::default());
    let api = CoreApi::new(backend.clone());
    let (first_id, _) = api.open(first_temp.path()).await.expect("open first");
    let (second_id, _) = api.open(second_temp.path()).await.expect("open second");
    let paths = vec!["file.txt".to_owned()];
    let commit_options = CommitOptions {
        message: "test".into(),
        amend: false,
        signoff: false,
    };

    let (stage_result, commit_result) = tokio::join!(
        api.stage(&first_id, &paths),
        api.commit(&first_id, &commit_options)
    );
    stage_result.expect("stage");
    commit_result.expect("commit");
    assert_eq!(backend.max_active_mutations.load(Ordering::SeqCst), 1);

    backend.max_active_mutations.store(0, Ordering::SeqCst);
    let (first_result, second_result) =
        tokio::join!(api.stage(&first_id, &paths), api.stage(&second_id, &paths));
    first_result.expect("first repository stage");
    second_result.expect("second repository stage");
    assert_eq!(backend.max_active_mutations.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn submodule_mutation_serializes_with_the_child_tab() {
    let temp = TempDir::new().expect("parent repository");
    let child = temp.path().join("child");
    std::fs::create_dir(&child).unwrap();
    let backend = Arc::new(MockBackend::default());
    let api = CoreApi::new(backend.clone());
    let (parent_id, _) = api.open(temp.path()).await.unwrap();
    let (child_id, _) = api.open(&child).await.unwrap();
    let paths = vec!["file.txt".to_owned()];
    let (update, stage) = tokio::join!(
        api.update_submodule(&parent_id, "child"),
        api.stage(&child_id, &paths),
    );
    update.unwrap();
    stage.unwrap();
    assert_eq!(backend.max_active_mutations.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn linked_worktree_submodule_locks_follow_common_directory_ancestry() {
    let temp = TempDir::new().unwrap();
    let parent = temp.path().join("parent");
    let child = temp.path().join("zzz-linked-child");
    let nested = parent.join("A/B");
    let child_common = parent.join(".git/modules/A");
    let nested_common = child_common.join("modules/B");
    std::fs::create_dir_all(&nested_common).unwrap();
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::create_dir_all(&child).unwrap();
    let api = Arc::new(CoreApi::new(Arc::new(MockBackend::default())));
    let (parent_id, _) = api.register(repository_info(&parent)).await.unwrap();
    let mut child_info = repository_info(&child);
    child_info.common_dir = child_common.to_string_lossy().into_owned();
    let (child_id, _) = api.register(child_info).await.unwrap();
    let mut nested_info = repository_info(&nested);
    nested_info.common_dir = nested_common.to_string_lossy().into_owned();
    let (nested_id, _) = api.register(nested_info).await.unwrap();
    let nested_lock = api
        .repository_entry(&nested_id)
        .await
        .unwrap()
        .mutation_lock;
    let nested_guard = nested_lock.lock().await;
    let parent_api = api.clone();
    let parent_task =
        tokio::spawn(async move { parent_api.update_submodule(&parent_id, "A").await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    let child_api = api.clone();
    let child_task = tokio::spawn(async move { child_api.update_submodule(&child_id, "B").await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    drop(nested_guard);
    tokio::time::timeout(Duration::from_secs(2), async {
        parent_task.await.unwrap().unwrap();
        child_task.await.unwrap().unwrap();
    })
    .await
    .expect("ancestor and nested operations must not deadlock through linked worktrees");
}

#[tokio::test]
async fn fetch_forwards_cancellation_token() {
    let temp = TempDir::new().expect("temp repository");
    let backend = Arc::new(MockBackend::default());
    let api = CoreApi::new(backend.clone());
    let (repository_id, _) = api.open(temp.path()).await.expect("open repository");
    let cancellation = CancellationToken::new();
    cancellation.cancel();

    let error = api
        .fetch(
            &repository_id,
            &FetchOptions {
                remote: None,
                prune: false,
                tags: false,
            },
            cancellation,
        )
        .await
        .expect_err("cancelled fetch");

    assert_eq!(error.code, ErrorCode::Cancelled);
    assert!(backend.fetch_saw_cancelled.load(Ordering::SeqCst));
}

fn repository_info(path: &Path) -> RepositoryInfo {
    std::fs::create_dir_all(path.join(".git")).unwrap();
    RepositoryInfo {
        root: path.to_string_lossy().into_owned(),
        git_dir: path.join(".git").to_string_lossy().into_owned(),
        common_dir: path.join(".git").to_string_lossy().into_owned(),
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        is_bare: false,
        object_format: ObjectFormat::Sha1,
    }
}

fn repository_snapshot() -> RepositorySnapshot {
    RepositorySnapshot {
        generation: "g1".into(),
        head: HeadState::Unborn {
            intended_branch: "main".into(),
        },
        operation_state: RepositoryOperationState::Normal,
        operation_progress: None,
        operation_source: None,
        operation_message: None,
        operation_autostash: false,
        status: WorktreeStatus::default(),
        local_branches: Vec::new(),
        remote_branches: Vec::new(),
        default_conflict_target: None,
        tags: Vec::new(),
        remotes: Vec::new(),
        capabilities: RepositoryCapabilities::default(),
    }
}

fn commit_details(oid: &str) -> CommitDetails {
    let identity = Identity {
        name: "Test".into(),
        email: "test@example.com".into(),
    };
    let time = CommitTime {
        seconds: 0,
        offset_minutes: 0,
    };
    CommitDetails {
        oid: oid.into(),
        short_oid: oid.chars().take(7).collect(),
        tree_oid: "tree".into(),
        parent_oids: Vec::new(),
        author: identity.clone(),
        committer: identity,
        authored_at: time.clone(),
        committed_at: time,
        subject: "subject".into(),
        body: String::new(),
        stats: DiffStats {
            files: 0,
            additions: 0,
            deletions: 0,
        },
        files: Vec::new(),
    }
}

fn mutation_result() -> MutationResult {
    MutationResult {
        before_oid: Some("before".into()),
        after_oid: Some("after".into()),
        generation: "g2".into(),
        conflicts: Vec::new(),
        needs_user_action: false,
        notice: None,
    }
}
