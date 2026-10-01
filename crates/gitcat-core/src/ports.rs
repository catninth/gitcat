use std::path::Path;

use async_trait::async_trait;
use gitcat_contracts::*;
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait GitBackend: Send + Sync {
    async fn probe(&self) -> ApiResult<GitVersion>;
    async fn open_repository(&self, path: &Path) -> ApiResult<RepositoryInfo>;
    async fn init_repository(&self, path: &Path, default_branch: &str)
    -> ApiResult<RepositoryInfo>;
    async fn clone_repository(
        &self,
        options: &CloneOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<RepositoryInfo>;

    async fn snapshot(&self, path: &Path) -> ApiResult<RepositorySnapshot>;
    async fn history(&self, path: &Path, query: &HistoryQuery) -> ApiResult<HistoryPage>;
    async fn search_commits(
        &self,
        path: &Path,
        query: &CommitSearchQuery,
    ) -> ApiResult<CommitSearchResult>;
    async fn commit_details(
        &self,
        path: &Path,
        oid: &str,
        parent_index: usize,
    ) -> ApiResult<CommitDetails>;
    async fn diff(&self, path: &Path, request: &DiffRequest) -> ApiResult<FileDiff>;
    async fn conflict_preflight(
        &self,
        path: &Path,
        target: &str,
    ) -> ApiResult<ConflictPreflightResult>;
    async fn conflict_details(
        &self,
        path: &Path,
        conflict_path: &str,
    ) -> ApiResult<ConflictFileDetails>;

    async fn stage_paths(&self, path: &Path, paths: &[String]) -> ApiResult<MutationResult>;
    async fn unstage_paths(&self, path: &Path, paths: &[String]) -> ApiResult<MutationResult>;
    async fn discard_paths(&self, path: &Path, paths: &[String]) -> ApiResult<MutationResult>;
    async fn apply_diff_lines(
        &self,
        path: &Path,
        request: &LinePatchRequest,
    ) -> ApiResult<MutationResult>;
    async fn stash_paths(
        &self,
        path: &Path,
        paths: &[String],
        message: Option<&str>,
    ) -> ApiResult<MutationResult>;
    async fn append_gitignore(&self, path: &Path, patterns: &[String])
    -> ApiResult<MutationResult>;
    async fn create_patch(&self, path: &Path, paths: &[String], staged: bool) -> ApiResult<String>;
    async fn resolve_conflict(
        &self,
        path: &Path,
        conflict_path: &str,
        resolution: ConflictResolution,
        expected_state: &ConflictExpectedState,
    ) -> ApiResult<MutationResult>;
    async fn save_conflict_result(
        &self,
        path: &Path,
        conflict_path: &str,
        text: &str,
        line_ending: ConflictLineEndingPolicy,
        expected_state: &ConflictExpectedState,
    ) -> ApiResult<MutationResult>;
    async fn resolve_conflicts(
        &self,
        path: &Path,
        conflict_paths: &[String],
        resolution: ConflictResolution,
    ) -> ApiResult<MutationResult>;
    async fn auto_resolve_conflicts(&self, path: &Path) -> ApiResult<MutationResult>;
    async fn create_commit(
        &self,
        path: &Path,
        options: &CommitOptions,
    ) -> ApiResult<MutationResult>;
    /// Makes the first commit of a repository that has none.
    ///
    /// A repository with an unborn HEAD has nothing to show and nothing to
    /// build on, so this is the one commit GitCat offers to compose itself:
    /// where the index is empty it seeds a `README.md` named after the
    /// repository folder, exactly what the hosting services and GitKraken put
    /// there first. Anything already staged is committed as it stands, and
    /// nothing else in the working tree is touched -- the rest stays untracked
    /// for the user to stage in their own order.
    async fn create_initial_commit(&self, path: &Path, message: &str) -> ApiResult<MutationResult>;
    async fn reword_commit(
        &self,
        path: &Path,
        oid: &str,
        message: &str,
    ) -> ApiResult<MutationResult>;

    async fn create_branch(
        &self,
        path: &Path,
        name: &str,
        start_oid: &str,
        checkout: bool,
    ) -> ApiResult<MutationResult>;
    async fn checkout_branch(&self, path: &Path, name: &str) -> ApiResult<MutationResult>;
    async fn rename_branch(
        &self,
        path: &Path,
        old_name: &str,
        new_name: &str,
    ) -> ApiResult<MutationResult>;
    async fn delete_branch(
        &self,
        path: &Path,
        name: &str,
        force: bool,
        confirmed: bool,
    ) -> ApiResult<MutationResult>;
    async fn set_upstream(
        &self,
        path: &Path,
        branch: &str,
        upstream: &str,
    ) -> ApiResult<MutationResult>;
    async fn merge_branch(&self, path: &Path, branch: &str) -> ApiResult<MutationResult>;

    /// Points a new remote name at a URL.
    ///
    /// Nothing is fetched: this exists for a repository that has just been
    /// created on a hosting service, where there is nothing on the other end
    /// to fetch yet.
    async fn add_remote(&self, path: &Path, name: &str, url: &str) -> ApiResult<MutationResult>;

    async fn fetch(
        &self,
        path: &Path,
        options: &FetchOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult>;
    async fn pull(
        &self,
        path: &Path,
        options: &PullOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult>;
    async fn push(
        &self,
        path: &Path,
        options: &PushOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult>;

    async fn checkout_commit(&self, path: &Path, oid: &str) -> ApiResult<MutationResult>;
    async fn create_tag(
        &self,
        path: &Path,
        name: &str,
        oid: &str,
        message: Option<&str>,
    ) -> ApiResult<MutationResult>;
    async fn cherry_pick(
        &self,
        path: &Path,
        oid: &str,
        mainline_parent: Option<u32>,
    ) -> ApiResult<MutationResult>;
    async fn revert_commit(
        &self,
        path: &Path,
        oid: &str,
        mainline_parent: Option<u32>,
    ) -> ApiResult<MutationResult>;
    async fn reset_to_commit(
        &self,
        path: &Path,
        oid: &str,
        mode: ResetMode,
        confirmed: bool,
    ) -> ApiResult<MutationResult>;
    async fn commit_action_availability(
        &self,
        path: &Path,
        oid: &str,
    ) -> ApiResult<Vec<CommitActionAvailability>>;

    async fn continue_operation(
        &self,
        path: &Path,
        operation: ContinueOperation,
    ) -> ApiResult<MutationResult>;
    async fn abort_operation(
        &self,
        path: &Path,
        operation: ContinueOperation,
    ) -> ApiResult<MutationResult>;
    async fn skip_operation(
        &self,
        path: &Path,
        operation: ContinueOperation,
    ) -> ApiResult<MutationResult>;

    async fn stash_list(&self, path: &Path) -> ApiResult<Vec<StashEntry>>;
    async fn stash_push(
        &self,
        path: &Path,
        message: Option<&str>,
        include_untracked: bool,
    ) -> ApiResult<MutationResult>;
    async fn stash_apply(&self, path: &Path, oid: &str, pop: bool) -> ApiResult<MutationResult>;
    async fn stash_drop(
        &self,
        path: &Path,
        oid: &str,
        confirmed: bool,
    ) -> ApiResult<MutationResult>;
}
