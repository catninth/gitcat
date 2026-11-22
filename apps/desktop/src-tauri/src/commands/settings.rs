//! Persisted workspace state and settings import/export.

use crate::*;

#[tauri::command]
pub(crate) async fn persisted_state_load(
    store: State<'_, JsonStateStore>,
) -> ApiResult<PersistedState> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.load())
        .await
        .map_err(task_join_error)?
}

#[tauri::command]
pub(crate) async fn persisted_state_save(
    store: State<'_, JsonStateStore>,
    state: PersistedState,
) -> ApiResult<()> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.save(&state))
        .await
        .map_err(task_join_error)?
}

#[tauri::command]
pub(crate) async fn settings_export(settings: AppSettings, destination: String) -> ApiResult<()> {
    tauri::async_runtime::spawn_blocking(move || export_settings(&settings, destination))
        .await
        .map_err(task_join_error)?
}

#[tauri::command]
pub(crate) async fn settings_import(source: String) -> ApiResult<AppSettings> {
    tauri::async_runtime::spawn_blocking(move || import_settings(source))
        .await
        .map_err(task_join_error)?
}
