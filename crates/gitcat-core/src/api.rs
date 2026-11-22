use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gitcat_contracts::*;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::GitBackend;

mod branches;
mod commits;
mod conflicts;
mod history;
mod remotes;
mod stash;
mod submodules;
mod worktree;

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
    // Submodule operations take a shared guard so a new child tab cannot join
    // halfway through an operation and escape its parent/child mutation locks.
    registration_gate: RwLock<()>,
}

impl CoreApi {
    pub fn new(backend: Arc<dyn GitBackend>) -> Self {
        Self {
            backend,
            repositories: RwLock::new(HashMap::new()),
            registration_gate: RwLock::new(()),
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

    async fn register(&self, info: RepositoryInfo) -> ApiResult<(RepositoryId, RepositoryInfo)> {
        let _registration_guard = self.registration_gate.write().await;
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

    /// Include external metadata directories used by submodules and worktrees.
    pub async fn repository_watch_paths(
        &self,
        repository_id: &RepositoryId,
    ) -> ApiResult<Vec<PathBuf>> {
        let root = self.repository_path(repository_id).await?;
        let info = self.backend.open_repository(&root).await?;
        let mut paths = vec![root.clone()];
        for directory in [&info.git_dir, &info.common_dir] {
            let path = PathBuf::from(directory);
            let path = if path.is_absolute() {
                path
            } else {
                root.join(path)
            };
            let path = dunce::canonicalize(&path).map_err(|error| {
                ApiError::new(ErrorCode::Io, "Git metadata directory is unavailable")
                    .with_details(error.to_string())
            })?;
            if !paths.iter().any(|watched| path.starts_with(watched)) {
                paths.push(path);
            }
        }
        Ok(paths)
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

    /// Submodule operations also mutate child repositories. Lock any open child
    /// tabs (and their linked worktrees) in stable ancestor-first order.
    async fn mutate_submodules<T, F, Fut>(
        &self,
        repository_id: &RepositoryId,
        operation: F,
    ) -> ApiResult<T>
    where
        F: FnOnce(Arc<dyn GitBackend>, PathBuf) -> Fut,
        Fut: Future<Output = ApiResult<T>>,
    {
        let _registration_guard = self.registration_gate.read().await;
        let parent_entry = self.repository_entry(repository_id).await?;
        let parent_lock = parent_entry.mutation_lock;
        let modules_dir = parent_entry.common_dir.join("modules");
        self.mutate(repository_id, |backend, root| async move {
            let mut descendants: Vec<_> = self
                .repositories
                .read()
                .await
                .values()
                .filter(|entry| {
                    entry.path != root
                        && (entry.path.starts_with(&root)
                            || entry.common_dir.starts_with(&modules_dir))
                        && !Arc::ptr_eq(&entry.mutation_lock, &parent_lock)
                })
                // Linked worktrees can sit anywhere on disk. Their common Git
                // directories preserve module ancestry, so a parent always
                // locks a child before a grandchild, just as the child does.
                .map(|entry| (entry.common_dir.clone(), entry.mutation_lock.clone()))
                .collect();
            descendants.sort_by(|a, b| a.0.cmp(&b.0));
            let mut locks: Vec<Arc<Mutex<()>>> = Vec::new();
            for (_, lock) in descendants {
                if !locks.iter().any(|existing| Arc::ptr_eq(existing, &lock)) {
                    locks.push(lock);
                }
            }
            let mut guards = Vec::new();
            for lock in locks {
                guards.push(lock.lock_owned().await);
            }
            operation(backend, root).await
        })
        .await
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
mod tests;
