//! Stash list and stash mutations.

use crate::*;

#[tauri::command]
pub(crate) async fn stash_list(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
) -> ApiResult<Vec<StashEntry>> {
    core.stash_list(&repository_id).await
}

#[tauri::command]
pub(crate) async fn stash_push(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    message: Option<String>,
    include_untracked: bool,
) -> ApiResult<MutationResult> {
    core.stash_push(&repository_id, message.as_deref(), include_untracked)
        .await
}

#[tauri::command]
pub(crate) async fn stash_apply(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
    pop: bool,
) -> ApiResult<MutationResult> {
    core.stash_apply(&repository_id, &oid, pop).await
}

#[tauri::command]
pub(crate) async fn stash_drop(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
    confirmed: bool,
    expected: ExpectedState,
) -> ApiResult<MutationResult> {
    core.stash_drop(&repository_id, &oid, confirmed, &expected)
        .await
}
