mod commands;
mod launch;
mod updater;
mod watcher;
mod window_state;

use std::sync::Arc;

use gitcat_contracts::{
    ApiError, ApiResult, AppMetadata, AppSettings, AvatarEntry, AvatarLookup, AvatarSettings,
    CheckSummary, CloneOptions, CommitActionAvailability, CommitDetails, CommitOptions,
    CommitSearchQuery, CommitSearchResult, ConflictExpectedState, ConflictFileDetails,
    ConflictLineEndingPolicy, ConflictPreflightResult, ConflictResolution,
    ConflictResolutionWarning, ContinueOperation, DeviceAuthorization, DiffRequest, ErrorCode,
    ExpectedState, FetchOptions, FileDiff, ForgeAccount, ForgeCredential, ForgeRepo,
    ForgeRepository, GitVersion, HistoryPage, HistoryQuery, LinePatchRequest, LoginPoll,
    MAX_UI_ZOOM_PERCENT, MIN_UI_ZOOM_PERCENT, MutationResult, NewRepository, PersistedState,
    PullOptions, PullRequestInfo, PushOptions, RepositoryId, RepositoryInfo, RepositorySnapshot,
    ResetMode, StashEntry, SubmoduleConflictDetails, SubmoduleInfo, SubmoduleSyncAction,
};
use gitcat_core::{CoreApi, JsonStateStore, export_settings, import_settings};
use gitcat_forge::{AvatarService, ForgeAuth, ForgeService, TokenStore};
use gitcat_git_cli::{GitCliBackend, GitCredentialSource};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WindowEvent};
use tokio_util::sync::CancellationToken;

use crate::launch::{
    OPEN_REQUEST_EVENT, OpenRequestPayload, PendingOpen, focus_window, repository_argument,
};
use crate::watcher::RepositoryWatchState;
use crate::window_state::WindowModeStore;

#[derive(Debug, Serialize)]
pub struct OpenedRepository {
    pub repository_id: RepositoryId,
    pub info: RepositoryInfo,
}

impl From<(RepositoryId, RepositoryInfo)> for OpenedRepository {
    fn from((repository_id, info): (RepositoryId, RepositoryInfo)) -> Self {
        Self {
            repository_id,
            info,
        }
    }
}

// WebView2 stops painting after a zoom change until its bounds change, which
// left the window blank until the user resized it; nudging the bounds by one
// pixel and back makes it lay out and paint again at the new factor.
#[tauri::command]
fn set_interface_zoom(webview: tauri::Webview, percent: u16) -> Result<(), String> {
    let factor = f64::from(percent.clamp(MIN_UI_ZOOM_PERCENT, MAX_UI_ZOOM_PERCENT)) / 100.0;
    webview
        .set_zoom(factor)
        .map_err(|error| error.to_string())?;
    let bounds = webview.bounds().map_err(|error| error.to_string())?;
    let scale = webview
        .window()
        .scale_factor()
        .map_err(|error| error.to_string())?;
    let size = bounds.size.to_physical::<u32>(scale);
    let mut nudged = bounds;
    nudged.size = tauri::PhysicalSize::new(size.width, size.height.saturating_sub(1)).into();
    webview
        .set_bounds(nudged)
        .map_err(|error| error.to_string())?;
    webview
        .set_bounds(bounds)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn app_metadata() -> AppMetadata {
    AppMetadata {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        commit: option_env!("GITCAT_BUILD_COMMIT")
            .unwrap_or("unknown")
            .to_owned(),
    }
}

/// Hands the frontend the folder GitCat was launched with, if there was one.
///
/// The Explorer context menu starts GitCat with a folder on the command line,
/// and at that point there is no window to send an event to, so the folder
/// waits here until the frontend has restored its workspace and asks.
#[tauri::command]
fn launch_repository_path(pending: State<'_, PendingOpen>) -> Option<String> {
    pending.take()
}

/// Restarts GitCat after an update has been installed.
///
/// This exists instead of the process plugin's `relaunch` because the
/// single-instance lock has to be released first: the replacement process
/// starts before this one is gone, and would otherwise hand its arguments to a
/// window that is on its way out and exit.
fn app_relaunch(app: AppHandle) {
    tauri_plugin_single_instance::destroy(&app);
    app.restart();
}

/// Lets Git authenticate as whoever GitCat is signed in as.
///
/// Without this a clone or a push would still depend on the user having a
/// system credential manager set up as well, which is the one thing signing in
/// inside the application was supposed to replace.
struct ForgeCredentials(Arc<ForgeAuth>);

#[async_trait::async_trait]
impl GitCredentialSource for ForgeCredentials {
    async fn token_for(&self, host: &str) -> Option<String> {
        self.0.access_token(host).await
    }

    /// A sign-in the service refused is renewed here, so a fetch or a push that
    /// met a revoked token repairs itself instead of reporting a failure the
    /// refresh token could have answered.
    async fn renew_rejected(&self, host: &str) -> Option<String> {
        self.0.renew_rejected(host).await
    }
}

fn task_join_error(error: impl std::fmt::Display) -> ApiError {
    ApiError::new(
        gitcat_contracts::ErrorCode::Internal,
        "background state task failed",
    )
    .with_details(error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Registered first, as the plugin requires. A second launch -- right
        // -clicking another folder in Explorer, say -- has to reach the window
        // that is already open: two instances would each hold their own copy of
        // the workspace and the last one to exit would win.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                focus_window(&window);
            }
            if let Some(path) = repository_argument(argv) {
                let _ = tauri::Emitter::emit(app, OPEN_REQUEST_EVENT, OpenRequestPayload { path });
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            // The token itself goes to the operating system credential store,
            // so a settings export cannot carry one off the machine.
            let tokens = Arc::new(TokenStore::new(&data_dir));
            let auth = Arc::new(ForgeAuth::new(tokens.clone()));

            let backend = Arc::new(
                GitCliBackend::default().with_credentials(Arc::new(ForgeCredentials(auth.clone()))),
            );
            app.manage(Arc::new(CoreApi::new(backend)));
            app.manage(JsonStateStore::new(data_dir.join("state.json")));
            app.manage(AvatarService::new(data_dir.join("avatars"), auth.clone()));
            app.manage(ForgeService::new(auth.clone()));
            app.manage(auth);
            app.manage(tokens);
            app.manage(RepositoryWatchState::default());
            app.manage(PendingOpen::new(repository_argument(std::env::args())));
            app.manage(WindowModeStore::new(data_dir.join("window.json")));
            updater::setup(app.handle())?;

            if let Some(window) = app.get_webview_window("main") {
                app.state::<WindowModeStore>().restore(&window);
                let _ = window.show();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::Resized(_)) {
                window.state::<WindowModeStore>().remember(window);
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_metadata,
            set_interface_zoom,
            updater::get_update_state,
            updater::check_update,
            updater::install_update,
            commands::repository::git_probe,
            launch_repository_path,
            commands::repository::repository_open,
            commands::repository::repository_init,
            commands::repository::repository_clone,
            commands::repository::repository_close,
            commands::repository::repository_snapshot,
            commands::repository::repository_watch,
            commands::repository::repository_unwatch,
            commands::repository::repository_reveal,
            commands::history::history_page,
            commands::history::history_search,
            commands::history::commit_details,
            commands::history::file_diff,
            commands::conflicts::conflicts_preflight,
            commands::conflicts::conflict_details,
            commands::conflicts::conflict_resolution_warnings,
            commands::conflicts::submodule_conflict_details,
            commands::worktree::paths_stage,
            commands::worktree::paths_unstage,
            commands::worktree::paths_discard,
            commands::worktree::diff_lines_apply,
            commands::worktree::path_stash,
            commands::worktree::gitignore_append,
            commands::worktree::file_patch_save,
            commands::conflicts::conflict_resolve,
            commands::conflicts::conflict_save_edited,
            commands::conflicts::conflicts_resolve,
            commands::conflicts::conflicts_auto_resolve,
            commands::commits::create_commit,
            commands::commits::commit_initial,
            commands::commits::commit_reword,
            commands::branches::branch_create,
            commands::branches::branch_checkout,
            commands::branches::branch_rename,
            commands::branches::branch_delete,
            commands::branches::remote_branch_delete,
            commands::branches::branch_set_upstream,
            commands::branches::branch_merge,
            commands::remotes::remote_add,
            commands::submodules::list_submodules,
            commands::submodules::add_submodule,
            commands::submodules::set_submodule_url,
            commands::submodules::update_submodule,
            commands::submodules::remove_submodule,
            commands::submodules::sync_submodule,
            commands::remotes::remote_fetch,
            commands::remotes::remote_pull,
            commands::remotes::remote_push,
            commands::commits::commit_checkout,
            commands::commits::tag_create,
            commands::commits::commit_cherry_pick,
            commands::commits::commit_revert,
            commands::commits::commit_reset,
            commands::commits::commit_action_availability,
            commands::commits::operation_continue,
            commands::commits::operation_abort,
            commands::commits::operation_skip,
            commands::stash::stash_list,
            commands::stash::stash_push,
            commands::stash::stash_apply,
            commands::stash::stash_drop,
            commands::settings::persisted_state_load,
            commands::settings::persisted_state_save,
            commands::settings::settings_export,
            commands::settings::settings_import,
            commands::forge::avatars_resolve,
            commands::forge::forge_token_set,
            commands::forge::forge_credentials,
            commands::forge::forge_pull_requests,
            commands::forge::forge_checks,
            commands::forge::forge_repository_location,
            commands::forge::forge_login_start,
            commands::forge::forge_login_poll,
            commands::forge::forge_sign_out,
            commands::forge::forge_account,
            commands::forge::forge_repositories,
            commands::forge::forge_create_repository,
        ])
        .run(tauri::generate_context!())
        .expect("error while running GitCat");
}
