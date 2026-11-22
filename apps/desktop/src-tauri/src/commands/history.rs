//! History pages, commit search, commit details and file diffs.

use crate::*;

#[tauri::command]
pub(crate) async fn history_page(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    query: HistoryQuery,
) -> ApiResult<HistoryPage> {
    core.history(&repository_id, &query).await
}

#[tauri::command]
pub(crate) async fn history_search(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    query: CommitSearchQuery,
) -> ApiResult<CommitSearchResult> {
    core.search(&repository_id, &query).await
}

#[tauri::command]
pub(crate) async fn commit_details(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
    parent_index: usize,
) -> ApiResult<CommitDetails> {
    core.details(&repository_id, &oid, parent_index).await
}

#[tauri::command]
pub(crate) async fn file_diff(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    request: DiffRequest,
) -> ApiResult<FileDiff> {
    core.diff(&repository_id, &request).await
}
