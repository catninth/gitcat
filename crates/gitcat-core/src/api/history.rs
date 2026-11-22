//! History, search, commit details and diffs.

use super::*;

impl CoreApi {
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
}
