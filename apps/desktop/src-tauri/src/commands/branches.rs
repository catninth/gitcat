//! Local and remote branch management.

use crate::*;

#[tauri::command]
pub(crate) async fn branch_create(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    name: String,
    start_oid: String,
    checkout: bool,
) -> ApiResult<MutationResult> {
    core.create_branch(&repository_id, &name, &start_oid, checkout)
        .await
}

#[tauri::command]
pub(crate) async fn branch_checkout(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    name: String,
) -> ApiResult<MutationResult> {
    core.checkout_branch(&repository_id, &name).await
}

#[tauri::command]
pub(crate) async fn branch_rename(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    old_name: String,
    new_name: String,
) -> ApiResult<MutationResult> {
    core.rename_branch(&repository_id, &old_name, &new_name)
        .await
}

#[tauri::command]
pub(crate) async fn branch_delete(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    name: String,
    force: bool,
    confirmed: bool,
    expected: ExpectedState,
) -> ApiResult<MutationResult> {
    core.delete_branch(&repository_id, &name, force, confirmed, &expected)
        .await
}

#[tauri::command]
pub(crate) async fn remote_branch_delete(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    remote: String,
    branch: String,
    confirmed: bool,
) -> ApiResult<MutationResult> {
    core.delete_remote_branch(&repository_id, &remote, &branch, confirmed)
        .await
}

#[tauri::command]
pub(crate) async fn branch_set_upstream(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    branch: String,
    upstream: String,
) -> ApiResult<MutationResult> {
    core.set_upstream(&repository_id, &branch, &upstream).await
}

#[tauri::command]
pub(crate) async fn branch_merge(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    branch: String,
) -> ApiResult<MutationResult> {
    core.merge_branch(&repository_id, &branch).await
}
