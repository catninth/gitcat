//! Submodule listing and maintenance.

use crate::*;

#[tauri::command]
pub(crate) async fn list_submodules(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
) -> ApiResult<Vec<SubmoduleInfo>> {
    core.list_submodules(&repository_id).await
}

#[tauri::command]
pub(crate) async fn add_submodule(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    url: String,
    path: String,
) -> ApiResult<MutationResult> {
    core.add_submodule(&repository_id, &url, &path).await
}

#[tauri::command]
pub(crate) async fn set_submodule_url(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    path: String,
    url: String,
) -> ApiResult<MutationResult> {
    core.set_submodule_url(&repository_id, &path, &url).await
}

#[tauri::command]
pub(crate) async fn update_submodule(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    path: String,
) -> ApiResult<MutationResult> {
    core.update_submodule(&repository_id, &path).await
}

#[tauri::command]
pub(crate) async fn remove_submodule(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    path: String,
    confirmed: bool,
) -> ApiResult<MutationResult> {
    core.remove_submodule(&repository_id, &path, confirmed)
        .await
}

#[tauri::command]
pub(crate) async fn sync_submodule(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    path: String,
    action: SubmoduleSyncAction,
) -> ApiResult<MutationResult> {
    core.sync_submodule(&repository_id, &path, action).await
}
