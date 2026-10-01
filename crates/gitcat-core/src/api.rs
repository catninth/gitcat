use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gitcat_contracts::*;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::GitBackend;

#[derive(Clone)]
struct RepositoryEntry {
    path: PathBuf,
    common_dir: PathBuf,
    mutation_lock: Arc<Mutex<()>>,
    closing: Arc<AtomicBool>,
}

const MAX_HISTORY_PAGE_SIZE: usize = 500;
const MAX_HISTORY_LANES: usize = 512;
const MAX_SEARCH_RESULTS: usize = 10_000;
const MAX_SEARCH_QUERY_CHARS: usize = 256;
const MAX_DIFF_CONTEXT_LINES: u16 = 100;
const MAX_DIFF_BYTES: usize = 128 * 1024 * 1024;

/// Transport-neutral facade used by the future UI adapter.
///
/// Repository paths never cross this boundary after a repository is opened.
/// Mutating operations for one repository share one asynchronous lock, while
/// reads and operations on different repositories may proceed concurrently.
pub struct CoreApi {
    backend: Arc<dyn GitBackend>,
    repositories: RwLock<HashMap<RepositoryId, RepositoryEntry>>,
}

impl CoreApi {
    pub fn new(backend: Arc<dyn GitBackend>) -> Self {
        Self {
            backend,
            repositories: RwLock::new(HashMap::new()),
        }
    }

    pub async fn probe(&self) -> ApiResult<GitVersion> {
        self.backend.probe().await
    }

    pub async fn open(&self, path: impl AsRef<Path>) -> ApiResult<(RepositoryId, RepositoryInfo)> {
        let info = self.backend.open_repository(path.as_ref()).await?;
        self.register(info).await
    }

    pub async fn open_repository(
        &self,
        path: impl AsRef<Path>,
    ) -> ApiResult<(RepositoryId, RepositoryInfo)> {
        self.open(path).await
    }

    pub async fn init(
        &self,
        path: impl AsRef<Path>,
        default_branch: &str,
    ) -> ApiResult<(RepositoryId, RepositoryInfo)> {
        let info = self
            .backend
            .init_repository(path.as_ref(), default_branch)
            .await?;
        self.register(info).await
    }

    pub async fn init_repository(
        &self,
        path: impl AsRef<Path>,
        default_branch: &str,
    ) -> ApiResult<(RepositoryId, RepositoryInfo)> {
        self.init(path, default_branch).await
    }

    pub async fn clone(
        &self,
        options: &CloneOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<(RepositoryId, RepositoryInfo)> {
        let info = self.backend.clone_repository(options, cancellation).await?;
        self.register(info).await
    }

    pub async fn clone_repository(
        &self,
        options: &CloneOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<(RepositoryId, RepositoryInfo)> {
        self.clone(options, cancellation).await
    }

    pub async fn close(&self, repository_id: &RepositoryId) -> ApiResult<()> {
        let entry = self.repository_entry(repository_id).await?;
        if entry.closing.swap(true, Ordering::AcqRel) {
            return Err(repository_closed(repository_id));
        }
        let _mutation_guard = entry.mutation_lock.lock().await;

        let removed = self.repositories.write().await.remove(repository_id);
        if removed.is_none() {
            return Err(repository_closed(repository_id));
        }

        Ok(())
    }

    pub async fn close_repository(&self, repository_id: &RepositoryId) -> ApiResult<()> {
        self.close(repository_id).await
    }

    pub async fn snapshot(&self, repository_id: &RepositoryId) -> ApiResult<RepositorySnapshot> {
        let path = self.repository_path(repository_id).await?;
        self.backend.snapshot(&path).await
    }

    pub async fn history(
        &self,
        repository_id: &RepositoryId,
        query: &HistoryQuery,
    ) -> ApiResult<HistoryPage> {
        validate_history_query(query)?;
        let path = self.repository_path(repository_id).await?;
        self.backend.history(&path, query).await
    }

    pub async fn search(
        &self,
        repository_id: &RepositoryId,
        query: &CommitSearchQuery,
    ) -> ApiResult<CommitSearchResult> {
        validate_search_query(query)?;
        let path = self.repository_path(repository_id).await?;
        self.backend.search_commits(&path, query).await
    }

    pub async fn search_commits(
        &self,
        repository_id: &RepositoryId,
        query: &CommitSearchQuery,
    ) -> ApiResult<CommitSearchResult> {
        self.search(repository_id, query).await
    }

    pub async fn details(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
        parent_index: usize,
    ) -> ApiResult<CommitDetails> {
        let path = self.repository_path(repository_id).await?;
        self.backend.commit_details(&path, oid, parent_index).await
    }

    pub async fn commit_details(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
        parent_index: usize,
    ) -> ApiResult<CommitDetails> {
        self.details(repository_id, oid, parent_index).await
    }

    pub async fn diff(
        &self,
        repository_id: &RepositoryId,
        request: &DiffRequest,
    ) -> ApiResult<FileDiff> {
        validate_diff_request(request)?;
        let path = self.repository_path(repository_id).await?;
        self.backend.diff(&path, request).await
    }

    pub async fn conflict_preflight(
        &self,
        repository_id: &RepositoryId,
        target: &str,
    ) -> ApiResult<ConflictPreflightResult> {
        let path = self.repository_path(repository_id).await?;
        self.backend.conflict_preflight(&path, target).await
    }

    pub async fn conflict_details(
        &self,
        repository_id: &RepositoryId,
        path: &str,
    ) -> ApiResult<ConflictFileDetails> {
        let repository_path = self.repository_path(repository_id).await?;
        self.backend.conflict_details(&repository_path, path).await
    }

    pub async fn stage(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.stage_paths(&path, paths).await
        })
        .await
    }

    pub async fn stage_paths(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
    ) -> ApiResult<MutationResult> {
        self.stage(repository_id, paths).await
    }

    pub async fn unstage(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.unstage_paths(&path, paths).await
        })
        .await
    }

    pub async fn unstage_paths(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
    ) -> ApiResult<MutationResult> {
        self.unstage(repository_id, paths).await
    }

    pub async fn discard(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.discard_paths(&path, paths).await
        })
        .await
    }

    pub async fn apply_diff_lines(
        &self,
        repository_id: &RepositoryId,
        request: &LinePatchRequest,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.apply_diff_lines(&path, request).await
        })
        .await
    }

    pub async fn stash_file(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
        message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.stash_paths(&path, paths, message).await
        })
        .await
    }

    pub async fn append_gitignore(
        &self,
        repository_id: &RepositoryId,
        patterns: &[String],
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.append_gitignore(&path, patterns).await
        })
        .await
    }

    pub async fn create_patch(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
        staged: bool,
    ) -> ApiResult<String> {
        let path = self.repository_path(repository_id).await?;
        self.backend.create_patch(&path, paths, staged).await
    }

    pub async fn resolve_conflict(
        &self,
        repository_id: &RepositoryId,
        path: &str,
        resolution: ConflictResolution,
        expected_state: &ConflictExpectedState,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, repository_path| async move {
            backend
                .resolve_conflict(&repository_path, path, resolution, expected_state)
                .await
        })
        .await
    }

    pub async fn save_conflict_result(
        &self,
        repository_id: &RepositoryId,
        path: &str,
        text: &str,
        line_ending: ConflictLineEndingPolicy,
        expected_state: &ConflictExpectedState,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, repository_path| async move {
            backend
                .save_conflict_result(&repository_path, path, text, line_ending, expected_state)
                .await
        })
        .await
    }

    pub async fn resolve_conflicts(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
        resolution: ConflictResolution,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, repository_path| async move {
            backend
                .resolve_conflicts(&repository_path, paths, resolution)
                .await
        })
        .await
    }

    pub async fn auto_resolve_conflicts(
        &self,
        repository_id: &RepositoryId,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.auto_resolve_conflicts(&path).await
        })
        .await
    }

    pub async fn commit(
        &self,
        repository_id: &RepositoryId,
        options: &CommitOptions,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.create_commit(&path, options).await
        })
        .await
    }

    pub async fn create_commit(
        &self,
        repository_id: &RepositoryId,
        options: &CommitOptions,
    ) -> ApiResult<MutationResult> {
        self.commit(repository_id, options).await
    }

    /// Composes the first commit for a repository that has none.
    ///
    /// The empty repository is the one case where GitCat can offer a commit
    /// rather than wait for one: see [`GitBackend::create_initial_commit`].
    pub async fn create_initial_commit(
        &self,
        repository_id: &RepositoryId,
        message: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.create_initial_commit(&path, message).await
        })
        .await
    }

    pub async fn reword_commit(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
        message: &str,
        expected: &ExpectedState,
    ) -> ApiResult<MutationResult> {
        self.mutate_expected(repository_id, expected, |backend, path| async move {
            backend.reword_commit(&path, oid, message).await
        })
        .await
    }

    pub async fn create_branch(
        &self,
        repository_id: &RepositoryId,
        name: &str,
        start_oid: &str,
        checkout: bool,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend
                .create_branch(&path, name, start_oid, checkout)
                .await
        })
        .await
    }

    pub async fn checkout_branch(
        &self,
        repository_id: &RepositoryId,
        name: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.checkout_branch(&path, name).await
        })
        .await
    }

    pub async fn rename_branch(
        &self,
        repository_id: &RepositoryId,
        old_name: &str,
        new_name: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.rename_branch(&path, old_name, new_name).await
        })
        .await
    }

    pub async fn delete_branch(
        &self,
        repository_id: &RepositoryId,
        name: &str,
        force: bool,
        confirmed: bool,
        expected: &ExpectedState,
    ) -> ApiResult<MutationResult> {
        if force && !confirmed {
            return Err(ApiError::new(
                ErrorCode::ProtectedOperation,
                "Forced branch deletion requires explicit confirmation",
            ));
        }
        self.mutate_expected(repository_id, expected, |backend, path| async move {
            backend.delete_branch(&path, name, force, confirmed).await
        })
        .await
    }

    pub async fn set_upstream(
        &self,
        repository_id: &RepositoryId,
        branch: &str,
        upstream: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.set_upstream(&path, branch, upstream).await
        })
        .await
    }

    pub async fn merge_branch(
        &self,
        repository_id: &RepositoryId,
        branch: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.merge_branch(&path, branch).await
        })
        .await
    }

    pub async fn add_remote(
        &self,
        repository_id: &RepositoryId,
        name: &str,
        url: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.add_remote(&path, name, url).await
        })
        .await
    }

    pub async fn fetch(
        &self,
        repository_id: &RepositoryId,
        options: &FetchOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.fetch(&path, options, cancellation).await
        })
        .await
    }

    pub async fn pull(
        &self,
        repository_id: &RepositoryId,
        options: &PullOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.pull(&path, options, cancellation).await
        })
        .await
    }

    pub async fn push(
        &self,
        repository_id: &RepositoryId,
        options: &PushOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.push(&path, options, cancellation).await
        })
        .await
    }

    pub async fn checkout_commit(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.checkout_commit(&path, oid).await
        })
        .await
    }

    pub async fn create_tag(
        &self,
        repository_id: &RepositoryId,
        name: &str,
        oid: &str,
        message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.create_tag(&path, name, oid, message).await
        })
        .await
    }

    pub async fn cherry_pick(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
        mainline_parent: Option<u32>,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.cherry_pick(&path, oid, mainline_parent).await
        })
        .await
    }

    pub async fn revert_commit(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
        mainline_parent: Option<u32>,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.revert_commit(&path, oid, mainline_parent).await
        })
        .await
    }

    pub async fn reset_to_commit(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
        mode: ResetMode,
        confirmed: bool,
        expected: &ExpectedState,
    ) -> ApiResult<MutationResult> {
        if matches!(mode, ResetMode::Hard) && !confirmed {
            return Err(ApiError::new(
                ErrorCode::ProtectedOperation,
                "Hard reset requires explicit confirmation",
            ));
        }
        self.mutate_expected(repository_id, expected, |backend, path| async move {
            backend.reset_to_commit(&path, oid, mode, confirmed).await
        })
        .await
    }

    pub async fn commit_action_availability(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
    ) -> ApiResult<Vec<CommitActionAvailability>> {
        let path = self.repository_path(repository_id).await?;
        self.backend.commit_action_availability(&path, oid).await
    }

    pub async fn continue_operation(
        &self,
        repository_id: &RepositoryId,
        operation: ContinueOperation,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.continue_operation(&path, operation).await
        })
        .await
    }

    pub async fn abort_operation(
        &self,
        repository_id: &RepositoryId,
        operation: ContinueOperation,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.abort_operation(&path, operation).await
        })
        .await
    }

    pub async fn skip_operation(
        &self,
        repository_id: &RepositoryId,
        operation: ContinueOperation,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.skip_operation(&path, operation).await
        })
        .await
    }

    pub async fn stash_list(&self, repository_id: &RepositoryId) -> ApiResult<Vec<StashEntry>> {
        let path = self.repository_path(repository_id).await?;
        self.backend.stash_list(&path).await
    }

    pub async fn stash_push(
        &self,
        repository_id: &RepositoryId,
        message: Option<&str>,
        include_untracked: bool,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.stash_push(&path, message, include_untracked).await
        })
        .await
    }

    pub async fn stash_apply(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
        pop: bool,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.stash_apply(&path, oid, pop).await
        })
        .await
    }

    pub async fn stash_drop(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
        confirmed: bool,
        expected: &ExpectedState,
    ) -> ApiResult<MutationResult> {
        if !confirmed {
            return Err(ApiError::new(
                ErrorCode::ProtectedOperation,
                "Dropping a stash requires explicit confirmation",
            ));
        }
        self.mutate_expected(repository_id, expected, |backend, path| async move {
            backend.stash_drop(&path, oid, confirmed).await
        })
        .await
    }

    async fn register(&self, info: RepositoryInfo) -> ApiResult<(RepositoryId, RepositoryInfo)> {
        let canonical_path = canonical_repository_root(&info)?;
        let common_dir = canonical_common_dir(&info, &canonical_path)?;
        let mut repositories = self.repositories.write().await;

        if let Some((repository_id, entry)) = repositories
            .iter()
            .find(|(_, entry)| entry.path == canonical_path)
        {
            if entry.closing.load(Ordering::Acquire) {
                return Err(ApiError::new(
                    ErrorCode::RepositoryBusy,
                    "Repository is currently closing",
                ));
            }
            return Ok((repository_id.clone(), info));
        }

        // Linked worktrees have different roots but mutate the same refs and
        // object database. They must share the common Git directory lock.
        let mutation_lock = repositories
            .values()
            .find(|entry| entry.common_dir == common_dir)
            .map_or_else(
                || Arc::new(Mutex::new(())),
                |entry| entry.mutation_lock.clone(),
            );

        let repository_id = RepositoryId::new();
        repositories.insert(
            repository_id.clone(),
            RepositoryEntry {
                path: canonical_path,
                common_dir,
                mutation_lock,
                closing: Arc::new(AtomicBool::new(false)),
            },
        );

        Ok((repository_id, info))
    }

    async fn repository_entry(&self, repository_id: &RepositoryId) -> ApiResult<RepositoryEntry> {
        let entry = self
            .repositories
            .read()
            .await
            .get(repository_id)
            .cloned()
            .ok_or_else(|| repository_closed(repository_id))?;
        if entry.closing.load(Ordering::Acquire) {
            return Err(repository_closed(repository_id));
        }
        Ok(entry)
    }

    async fn repository_path(&self, repository_id: &RepositoryId) -> ApiResult<PathBuf> {
        Ok(self.repository_entry(repository_id).await?.path)
    }

    /// Worktree root of an open repository. Used by the desktop shell to place a
    /// filesystem watcher on the active repository.
    pub async fn repository_root(&self, repository_id: &RepositoryId) -> ApiResult<PathBuf> {
        self.repository_path(repository_id).await
    }

    async fn mutate<T, F, Fut>(&self, repository_id: &RepositoryId, operation: F) -> ApiResult<T>
    where
        F: FnOnce(Arc<dyn GitBackend>, PathBuf) -> Fut,
        Fut: Future<Output = ApiResult<T>>,
    {
        let entry = self.repository_entry(repository_id).await?;
        let _mutation_guard = entry.mutation_lock.lock().await;

        if entry.closing.load(Ordering::Acquire) {
            return Err(repository_closed(repository_id));
        }

        // Close may win the lock while a mutation is queued. Do not let the
        // queued operation escape through a stale entry after close returns.
        let still_open = self
            .repositories
            .read()
            .await
            .get(repository_id)
            .is_some_and(|current| Arc::ptr_eq(&current.mutation_lock, &entry.mutation_lock));
        if !still_open {
            return Err(repository_closed(repository_id));
        }

        operation(self.backend.clone(), entry.path).await
    }

    async fn mutate_expected<T, F, Fut>(
        &self,
        repository_id: &RepositoryId,
        expected: &ExpectedState,
        operation: F,
    ) -> ApiResult<T>
    where
        F: FnOnce(Arc<dyn GitBackend>, PathBuf) -> Fut,
        Fut: Future<Output = ApiResult<T>>,
    {
        self.mutate(repository_id, |backend, path| async move {
            let snapshot = backend.snapshot(&path).await?;
            validate_expected_state(&snapshot, expected)?;
            operation(backend, path).await
        })
        .await
    }
}

fn canonical_repository_root(info: &RepositoryInfo) -> ApiResult<PathBuf> {
    let root = PathBuf::from(&info.root);
    dunce::canonicalize(&root).map_err(|error| {
        ApiError::new(
            ErrorCode::InvalidPath,
            format!("Cannot canonicalize repository root: {}", root.display()),
        )
        .with_details(error.to_string())
    })
}

fn canonical_common_dir(info: &RepositoryInfo, root: &Path) -> ApiResult<PathBuf> {
    let configured = PathBuf::from(&info.common_dir);
    let common_dir = if configured.is_absolute() {
        configured
    } else {
        root.join(configured)
    };
    dunce::canonicalize(&common_dir).map_err(|error| {
        ApiError::new(
            ErrorCode::InvalidPath,
            format!(
                "Cannot canonicalize common Git directory: {}",
                common_dir.display()
            ),
        )
        .with_details(error.to_string())
    })
}

fn validate_history_query(query: &HistoryQuery) -> ApiResult<()> {
    if !(1..=MAX_HISTORY_PAGE_SIZE).contains(&query.limit) {
        return Err(invalid_request("history limit must be between 1 and 500"));
    }
    if let Some(cursor) = &query.cursor {
        if cursor.lanes.heads.len() > MAX_HISTORY_LANES {
            return Err(invalid_request(
                "history cursor contains too many graph lanes",
            ));
        }
        if cursor.generation.len() > 256
            || cursor
                .lanes
                .heads
                .iter()
                .flatten()
                .any(|oid| oid.is_empty() || oid.len() > 128)
        {
            return Err(invalid_request("history cursor is malformed"));
        }
    }
    if let HistoryScope::Ref(name) = &query.scope {
        if name.is_empty() || name.len() > 1024 {
            return Err(invalid_request("history ref scope is malformed"));
        }
    }
    Ok(())
}

fn validate_search_query(query: &CommitSearchQuery) -> ApiResult<()> {
    if query.query.chars().count() > MAX_SEARCH_QUERY_CHARS {
        return Err(invalid_request(
            "commit search query exceeds 256 characters",
        ));
    }
    if !(1..=MAX_SEARCH_RESULTS).contains(&query.limit) {
        return Err(invalid_request(
            "commit search result limit must be between 1 and 10000",
        ));
    }
    if let HistoryScope::Ref(name) = &query.scope {
        if name.is_empty() || name.len() > 1024 {
            return Err(invalid_request("commit search ref scope is malformed"));
        }
    }
    Ok(())
}

fn validate_diff_request(request: &DiffRequest) -> ApiResult<()> {
    if request.context_lines > MAX_DIFF_CONTEXT_LINES {
        return Err(invalid_request(
            "diff context lines must be between 0 and 100",
        ));
    }
    if !(1..=MAX_DIFF_BYTES).contains(&request.max_bytes) {
        return Err(invalid_request(
            "diff byte limit must be between 1 and 134217728",
        ));
    }
    let path = Path::new(&request.path);
    if request.path.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(ApiError::new(
            ErrorCode::InvalidPath,
            "Diff path must be repository-relative",
        ));
    }
    Ok(())
}

fn invalid_request(message: impl Into<String>) -> ApiError {
    ApiError::new(ErrorCode::InvalidRequest, message)
}

fn validate_expected_state(
    snapshot: &RepositorySnapshot,
    expected: &ExpectedState,
) -> ApiResult<()> {
    let current_head = match &snapshot.head {
        HeadState::Branch { oid, .. } | HeadState::Detached { oid } => Some(oid.as_str()),
        HeadState::Unborn { .. } => None,
    };
    if snapshot.generation != expected.generation || current_head != expected.head_oid.as_deref() {
        return Err(ApiError::new(
            ErrorCode::StaleSnapshot,
            "Repository changed after the operation was prepared",
        ));
    }
    Ok(())
}

fn repository_closed(repository_id: &RepositoryId) -> ApiError {
    ApiError::new(ErrorCode::RepositoryClosed, "Repository is closed")
        .with_details(format!("repository_id={}", repository_id.0))
}

#[cfg(test)]
mod tests {
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

        async fn add_remote(
            &self,
            path: &Path,
            _name: &str,
            _url: &str,
        ) -> ApiResult<MutationResult> {
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
        ) -> ApiResult<MutationResult> {
            self.mutation("cherry_pick", path).await
        }

        async fn revert_commit(
            &self,
            path: &Path,
            _oid: &str,
            _mainline_parent: Option<u32>,
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

        async fn stash_apply(
            &self,
            path: &Path,
            _oid: &str,
            _pop: bool,
        ) -> ApiResult<MutationResult> {
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
}
