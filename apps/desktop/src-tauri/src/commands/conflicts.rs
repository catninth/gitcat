//! Conflict preflight, details and resolution.

use crate::*;

#[tauri::command]
pub(crate) async fn conflicts_preflight(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    target: String,
) -> ApiResult<ConflictPreflightResult> {
    core.conflict_preflight(&repository_id, &target).await
}

#[tauri::command]
pub(crate) async fn conflict_resolution_warnings(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    paths: Vec<String>,
) -> ApiResult<Vec<ConflictResolutionWarning>> {
    core.conflict_resolution_warnings(&repository_id, &paths)
        .await
}

#[tauri::command]
pub(crate) async fn submodule_conflict_details(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    path: String,
) -> ApiResult<SubmoduleConflictDetails> {
    core.submodule_conflict_details(&repository_id, &path).await
}

#[tauri::command]
pub(crate) async fn conflict_details(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    path: String,
) -> ApiResult<ConflictFileDetails> {
    core.conflict_details(&repository_id, &path).await
}

#[tauri::command]
pub(crate) async fn conflict_resolve(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    path: String,
    resolution: ConflictResolution,
    expected_state: ConflictExpectedState,
) -> ApiResult<MutationResult> {
    core.resolve_conflict(&repository_id, &path, resolution, &expected_state)
        .await
}

#[tauri::command]
pub(crate) async fn conflict_save_edited(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    path: String,
    text: String,
    line_ending: ConflictLineEndingPolicy,
    expected_state: ConflictExpectedState,
) -> ApiResult<MutationResult> {
    core.save_conflict_result(&repository_id, &path, &text, line_ending, &expected_state)
        .await
}

#[tauri::command]
pub(crate) async fn conflicts_resolve(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    paths: Vec<String>,
    resolution: ConflictResolution,
) -> ApiResult<MutationResult> {
    core.resolve_conflicts(&repository_id, &paths, resolution)
        .await
}

#[tauri::command]
pub(crate) async fn conflicts_auto_resolve(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
) -> ApiResult<MutationResult> {
    core.auto_resolve_conflicts(&repository_id).await
}
