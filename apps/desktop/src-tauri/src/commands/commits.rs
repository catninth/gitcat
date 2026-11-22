//! Commits and the operations Git can stop in.

use crate::*;

#[tauri::command]
pub(crate) async fn create_commit(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    options: CommitOptions,
) -> ApiResult<MutationResult> {
    core.commit(&repository_id, &options).await
}

#[tauri::command]
pub(crate) async fn commit_initial(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    message: String,
) -> ApiResult<MutationResult> {
    core.create_initial_commit(&repository_id, &message).await
}

#[tauri::command]
pub(crate) async fn commit_reword(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
    message: String,
    expected: ExpectedState,
) -> ApiResult<MutationResult> {
    core.reword_commit(&repository_id, &oid, &message, &expected)
        .await
}

#[tauri::command]
pub(crate) async fn commit_checkout(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
) -> ApiResult<MutationResult> {
    core.checkout_commit(&repository_id, &oid).await
}

#[tauri::command]
pub(crate) async fn tag_create(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    name: String,
    oid: String,
    message: Option<String>,
) -> ApiResult<MutationResult> {
    core.create_tag(&repository_id, &name, &oid, message.as_deref())
        .await
}

#[tauri::command]
pub(crate) async fn commit_cherry_pick(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
    mainline_parent: Option<u32>,
    commit: Option<bool>,
) -> ApiResult<MutationResult> {
    core.cherry_pick(
        &repository_id,
        &oid,
        mainline_parent,
        commit.unwrap_or(true),
    )
    .await
}

#[tauri::command]
pub(crate) async fn commit_revert(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
    mainline_parent: Option<u32>,
    commit: Option<bool>,
) -> ApiResult<MutationResult> {
    core.revert_commit(
        &repository_id,
        &oid,
        mainline_parent,
        commit.unwrap_or(true),
    )
    .await
}

#[tauri::command]
pub(crate) async fn commit_reset(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
    mode: ResetMode,
    confirmed: bool,
    expected: ExpectedState,
) -> ApiResult<MutationResult> {
    core.reset_to_commit(&repository_id, &oid, mode, confirmed, &expected)
        .await
}

#[tauri::command]
pub(crate) async fn commit_action_availability(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
) -> ApiResult<Vec<CommitActionAvailability>> {
    core.commit_action_availability(&repository_id, &oid).await
}

#[tauri::command]
pub(crate) async fn operation_continue(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    operation: ContinueOperation,
    message: Option<String>,
) -> ApiResult<MutationResult> {
    core.continue_operation(&repository_id, operation, message.as_deref())
        .await
}

#[tauri::command]
pub(crate) async fn operation_abort(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    operation: ContinueOperation,
) -> ApiResult<MutationResult> {
    core.abort_operation(&repository_id, operation).await
}

#[tauri::command]
pub(crate) async fn operation_skip(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    operation: ContinueOperation,
) -> ApiResult<MutationResult> {
    core.skip_operation(&repository_id, operation).await
}
