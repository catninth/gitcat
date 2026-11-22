//! Adding remotes and fetching, pulling and pushing.

use crate::*;

/// Points a new remote name at a URL, without fetching it.
#[tauri::command]
pub(crate) async fn remote_add(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    name: String,
    url: String,
) -> ApiResult<MutationResult> {
    core.add_remote(&repository_id, &name, &url).await
}

#[tauri::command]
pub(crate) async fn remote_fetch(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    options: FetchOptions,
) -> ApiResult<MutationResult> {
    core.fetch(&repository_id, &options, CancellationToken::new())
        .await
}

#[tauri::command]
pub(crate) async fn remote_pull(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    options: PullOptions,
    on_progress: tauri::ipc::Channel<gitcat_contracts::PullPhase>,
) -> ApiResult<MutationResult> {
    // A phase the webview can no longer receive is not worth failing the pull.
    let progress: gitcat_contracts::PullProgress = Arc::new(move |phase| {
        let _ = on_progress.send(phase);
    });
    core.pull_with_progress(&repository_id, &options, CancellationToken::new(), progress)
        .await
}

#[tauri::command]
pub(crate) async fn remote_push(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    options: PushOptions,
) -> ApiResult<MutationResult> {
    core.push(&repository_id, &options, CancellationToken::new())
        .await
}
