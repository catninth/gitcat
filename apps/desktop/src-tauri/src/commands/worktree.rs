//! Staging, discarding and line-level working tree changes.

use crate::*;

#[tauri::command]
pub(crate) async fn paths_stage(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    paths: Vec<String>,
) -> ApiResult<MutationResult> {
    core.stage(&repository_id, &paths).await
}

#[tauri::command]
pub(crate) async fn paths_unstage(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    paths: Vec<String>,
) -> ApiResult<MutationResult> {
    core.unstage(&repository_id, &paths).await
}

#[tauri::command]
pub(crate) async fn paths_discard(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    paths: Vec<String>,
) -> ApiResult<MutationResult> {
    core.discard(&repository_id, &paths).await
}

#[tauri::command]
pub(crate) async fn diff_lines_apply(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    request: LinePatchRequest,
) -> ApiResult<MutationResult> {
    core.apply_diff_lines(&repository_id, &request).await
}

#[tauri::command]
pub(crate) async fn path_stash(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    paths: Vec<String>,
    message: Option<String>,
) -> ApiResult<MutationResult> {
    core.stash_file(&repository_id, &paths, message.as_deref())
        .await
}

#[tauri::command]
pub(crate) async fn gitignore_append(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    patterns: Vec<String>,
) -> ApiResult<MutationResult> {
    core.append_gitignore(&repository_id, &patterns).await
}

#[tauri::command]
pub(crate) async fn file_patch_save(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    paths: Vec<String>,
    staged: bool,
    destination: String,
) -> ApiResult<()> {
    let patch = core.create_patch(&repository_id, &paths, staged).await?;
    std::fs::write(&destination, patch).map_err(|error| {
        ApiError::new(ErrorCode::Internal, "could not write patch file")
            .with_details(error.to_string())
    })
}
