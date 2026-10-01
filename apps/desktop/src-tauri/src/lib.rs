mod launch;
mod updater;
mod watcher;
mod window_state;

use std::sync::Arc;

use gitcat_contracts::{
    ApiError, ApiResult, AppMetadata, AppSettings, AvatarEntry, AvatarLookup, AvatarSettings,
    CheckSummary, CloneOptions, CommitActionAvailability, CommitDetails, CommitOptions,
    CommitSearchQuery, CommitSearchResult, ConflictExpectedState, ConflictFileDetails,
    ConflictLineEndingPolicy, ConflictPreflightResult, ConflictResolution, ContinueOperation,
    DeviceAuthorization, DiffRequest, ErrorCode, ExpectedState, FetchOptions, FileDiff,
    ForgeAccount, ForgeCredential, ForgeRepo, ForgeRepository, GitVersion, HistoryPage,
    HistoryQuery, LinePatchRequest, LoginPoll, MAX_UI_ZOOM_PERCENT, MIN_UI_ZOOM_PERCENT,
    MutationResult, NewRepository, PersistedState, PullOptions, PullRequestInfo, PushOptions,
    RepositoryId, RepositoryInfo, RepositorySnapshot, ResetMode, StashEntry,
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

#[tauri::command]
async fn git_probe(core: State<'_, Arc<CoreApi>>) -> ApiResult<GitVersion> {
    core.probe().await
}

#[tauri::command]
async fn repository_open(
    core: State<'_, Arc<CoreApi>>,
    path: String,
) -> ApiResult<OpenedRepository> {
    core.open(path).await.map(Into::into)
}

#[tauri::command]
async fn repository_init(
    core: State<'_, Arc<CoreApi>>,
    path: String,
    default_branch: String,
) -> ApiResult<OpenedRepository> {
    core.init(path, &default_branch).await.map(Into::into)
}

#[tauri::command]
async fn repository_clone(
    core: State<'_, Arc<CoreApi>>,
    options: CloneOptions,
) -> ApiResult<OpenedRepository> {
    core.clone_repository(&options, CancellationToken::new())
        .await
        .map(Into::into)
}

#[tauri::command]
async fn repository_close(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
) -> ApiResult<()> {
    core.close(&repository_id).await
}

#[tauri::command]
async fn repository_snapshot(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
) -> ApiResult<RepositorySnapshot> {
    core.snapshot(&repository_id).await
}

#[tauri::command]
async fn repository_watch(
    core: State<'_, Arc<CoreApi>>,
    watchers: State<'_, RepositoryWatchState>,
    app: AppHandle,
    repository_id: RepositoryId,
) -> ApiResult<()> {
    let root = core.repository_root(&repository_id).await?;
    watchers.watch(app, repository_id, root).map_err(|error| {
        ApiError::new(ErrorCode::Internal, "could not start repository watcher")
            .with_details(error.to_string())
    })
}

#[tauri::command]
fn repository_unwatch(watchers: State<'_, RepositoryWatchState>) {
    watchers.unwatch();
}

fn resolve_inside_root(root: &std::path::Path, relative: &str) -> ApiResult<std::path::PathBuf> {
    let normalized = relative.replace('\\', "/");
    let invalid = || ApiError::new(ErrorCode::InvalidPath, "invalid folder path");
    if normalized.trim().is_empty() {
        return Err(invalid());
    }
    let mut target = root.to_path_buf();
    for segment in normalized.split('/').filter(|segment| !segment.is_empty()) {
        if segment == "." || segment == ".." || segment.contains(':') {
            return Err(invalid());
        }
        target.push(segment);
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|_| ApiError::new(ErrorCode::Internal, "could not resolve repository root"))?;
    let canonical_target = target
        .canonicalize()
        .map_err(|_| ApiError::new(ErrorCode::InvalidPath, "folder does not exist"))?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(invalid());
    }
    if !canonical_target.is_dir() {
        return Err(invalid());
    }
    Ok(canonical_target)
}

#[tauri::command]
async fn repository_reveal(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    path: Option<String>,
) -> ApiResult<()> {
    let worktree = core.repository_root(&repository_id).await?;
    let root = match path.as_deref() {
        Some(relative) => resolve_inside_root(std::path::Path::new(&worktree), relative)?,
        None => std::path::PathBuf::from(&worktree),
    };
    // Single-purpose file-manager launch for a known, open repository root.
    // No shell and no caller-supplied arguments: only the resolved worktree path.
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("explorer");
        command.arg(&root);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(&root);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(&root);
        command
    };
    // `explorer` exits non-zero even on success, so spawn without inspecting status.
    command.spawn().map(|_| ()).map_err(|error| {
        ApiError::new(ErrorCode::Internal, "could not open repository folder")
            .with_details(error.to_string())
    })
}

#[tauri::command]
async fn history_page(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    query: HistoryQuery,
) -> ApiResult<HistoryPage> {
    core.history(&repository_id, &query).await
}

#[tauri::command]
async fn history_search(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    query: CommitSearchQuery,
) -> ApiResult<CommitSearchResult> {
    core.search(&repository_id, &query).await
}

#[tauri::command]
async fn commit_details(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
    parent_index: usize,
) -> ApiResult<CommitDetails> {
    core.details(&repository_id, &oid, parent_index).await
}

#[tauri::command]
async fn file_diff(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    request: DiffRequest,
) -> ApiResult<FileDiff> {
    core.diff(&repository_id, &request).await
}

#[tauri::command]
async fn conflicts_preflight(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    target: String,
) -> ApiResult<ConflictPreflightResult> {
    core.conflict_preflight(&repository_id, &target).await
}

#[tauri::command]
async fn conflict_details(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    path: String,
) -> ApiResult<ConflictFileDetails> {
    core.conflict_details(&repository_id, &path).await
}

#[tauri::command]
async fn paths_stage(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    paths: Vec<String>,
) -> ApiResult<MutationResult> {
    core.stage(&repository_id, &paths).await
}

#[tauri::command]
async fn paths_unstage(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    paths: Vec<String>,
) -> ApiResult<MutationResult> {
    core.unstage(&repository_id, &paths).await
}

#[tauri::command]
async fn paths_discard(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    paths: Vec<String>,
) -> ApiResult<MutationResult> {
    core.discard(&repository_id, &paths).await
}

#[tauri::command]
async fn diff_lines_apply(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    request: LinePatchRequest,
) -> ApiResult<MutationResult> {
    core.apply_diff_lines(&repository_id, &request).await
}

#[tauri::command]
async fn path_stash(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    paths: Vec<String>,
    message: Option<String>,
) -> ApiResult<MutationResult> {
    core.stash_file(&repository_id, &paths, message.as_deref())
        .await
}

#[tauri::command]
async fn gitignore_append(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    patterns: Vec<String>,
) -> ApiResult<MutationResult> {
    core.append_gitignore(&repository_id, &patterns).await
}

#[tauri::command]
async fn file_patch_save(
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

#[tauri::command]
async fn conflict_resolve(
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
async fn conflict_save_edited(
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
async fn conflicts_resolve(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    paths: Vec<String>,
    resolution: ConflictResolution,
) -> ApiResult<MutationResult> {
    core.resolve_conflicts(&repository_id, &paths, resolution)
        .await
}

#[tauri::command]
async fn conflicts_auto_resolve(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
) -> ApiResult<MutationResult> {
    core.auto_resolve_conflicts(&repository_id).await
}

#[tauri::command]
async fn create_commit(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    options: CommitOptions,
) -> ApiResult<MutationResult> {
    core.commit(&repository_id, &options).await
}

#[tauri::command]
async fn commit_initial(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    message: String,
) -> ApiResult<MutationResult> {
    core.create_initial_commit(&repository_id, &message).await
}

#[tauri::command]
async fn commit_reword(
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
async fn branch_create(
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
async fn branch_checkout(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    name: String,
) -> ApiResult<MutationResult> {
    core.checkout_branch(&repository_id, &name).await
}

#[tauri::command]
async fn branch_rename(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    old_name: String,
    new_name: String,
) -> ApiResult<MutationResult> {
    core.rename_branch(&repository_id, &old_name, &new_name)
        .await
}

#[tauri::command]
async fn branch_delete(
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
async fn branch_set_upstream(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    branch: String,
    upstream: String,
) -> ApiResult<MutationResult> {
    core.set_upstream(&repository_id, &branch, &upstream).await
}

#[tauri::command]
async fn branch_merge(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    branch: String,
) -> ApiResult<MutationResult> {
    core.merge_branch(&repository_id, &branch).await
}

/// Points a new remote name at a URL, without fetching it.
#[tauri::command]
async fn remote_add(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    name: String,
    url: String,
) -> ApiResult<MutationResult> {
    core.add_remote(&repository_id, &name, &url).await
}

#[tauri::command]
async fn remote_fetch(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    options: FetchOptions,
) -> ApiResult<MutationResult> {
    core.fetch(&repository_id, &options, CancellationToken::new())
        .await
}

#[tauri::command]
async fn remote_pull(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    options: PullOptions,
) -> ApiResult<MutationResult> {
    core.pull(&repository_id, &options, CancellationToken::new())
        .await
}

#[tauri::command]
async fn remote_push(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    options: PushOptions,
) -> ApiResult<MutationResult> {
    core.push(&repository_id, &options, CancellationToken::new())
        .await
}

#[tauri::command]
async fn commit_checkout(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
) -> ApiResult<MutationResult> {
    core.checkout_commit(&repository_id, &oid).await
}

#[tauri::command]
async fn tag_create(
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
async fn commit_cherry_pick(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
    mainline_parent: Option<u32>,
) -> ApiResult<MutationResult> {
    core.cherry_pick(&repository_id, &oid, mainline_parent)
        .await
}

#[tauri::command]
async fn commit_revert(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
    mainline_parent: Option<u32>,
) -> ApiResult<MutationResult> {
    core.revert_commit(&repository_id, &oid, mainline_parent)
        .await
}

#[tauri::command]
async fn commit_reset(
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
async fn commit_action_availability(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
) -> ApiResult<Vec<CommitActionAvailability>> {
    core.commit_action_availability(&repository_id, &oid).await
}

#[tauri::command]
async fn operation_continue(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    operation: ContinueOperation,
) -> ApiResult<MutationResult> {
    core.continue_operation(&repository_id, operation).await
}

#[tauri::command]
async fn operation_abort(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    operation: ContinueOperation,
) -> ApiResult<MutationResult> {
    core.abort_operation(&repository_id, operation).await
}

#[tauri::command]
async fn operation_skip(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    operation: ContinueOperation,
) -> ApiResult<MutationResult> {
    core.skip_operation(&repository_id, operation).await
}

#[tauri::command]
async fn stash_list(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
) -> ApiResult<Vec<StashEntry>> {
    core.stash_list(&repository_id).await
}

#[tauri::command]
async fn stash_push(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    message: Option<String>,
    include_untracked: bool,
) -> ApiResult<MutationResult> {
    core.stash_push(&repository_id, message.as_deref(), include_untracked)
        .await
}

#[tauri::command]
async fn stash_apply(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
    pop: bool,
) -> ApiResult<MutationResult> {
    core.stash_apply(&repository_id, &oid, pop).await
}

#[tauri::command]
async fn stash_drop(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    oid: String,
    confirmed: bool,
    expected: ExpectedState,
) -> ApiResult<MutationResult> {
    core.stash_drop(&repository_id, &oid, confirmed, &expected)
        .await
}

#[tauri::command]
async fn persisted_state_load(store: State<'_, JsonStateStore>) -> ApiResult<PersistedState> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.load())
        .await
        .map_err(task_join_error)?
}

#[tauri::command]
async fn persisted_state_save(
    store: State<'_, JsonStateStore>,
    state: PersistedState,
) -> ApiResult<()> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.save(&state))
        .await
        .map_err(task_join_error)?
}

/// Resolves commit author avatars for one repository page. Authors that stay
/// unresolved are absent from the result and keep their drawn initial.
#[tauri::command]
async fn avatars_resolve(
    avatars: State<'_, AvatarService>,
    lookup: AvatarLookup,
    settings: AvatarSettings,
) -> ApiResult<Vec<AvatarEntry>> {
    avatars.resolve(&lookup, settings).await
}

/// Stores or clears the token for one host. Passing `None` removes it.
#[tauri::command]
async fn forge_token_set(
    tokens: State<'_, Arc<TokenStore>>,
    forge: State<'_, ForgeService>,
    host: String,
    token: Option<String>,
) -> ApiResult<()> {
    tokens.set(&host, token.as_deref())?;
    forge.forget_host(&host);
    Ok(())
}

/// Which hosts hold a token, with a hint too short to be the credential. The
/// token itself never travels back to the webview.
///
/// Asked of `ForgeAuth` rather than the store, because only it can say which
/// of the scopes GitCat needs a stored sign-in is short of.
#[tauri::command]
async fn forge_credentials(auth: State<'_, Arc<ForgeAuth>>) -> ApiResult<Vec<ForgeCredential>> {
    auth.credentials()
}

/// The pull requests open against one repository.
///
/// `refresh` bypasses the short-lived cache, which is what a manual refresh
/// after a push wants; an ordinary redraw leaves it alone.
#[tauri::command]
async fn forge_pull_requests(
    forge: State<'_, ForgeService>,
    repo: ForgeRepo,
    refresh: bool,
) -> ApiResult<Vec<PullRequestInfo>> {
    forge.pull_requests(&repo, refresh).await
}

/// Starts a device-flow sign-in and returns what the user has to type where.
/// The device code itself stays in the backend.
#[tauri::command]
async fn forge_login_start(
    auth: State<'_, Arc<ForgeAuth>>,
    host: String,
) -> ApiResult<DeviceAuthorization> {
    auth.begin(&host).await
}

/// Asks once whether the sign-in has been authorised. The webview repeats this
/// at the interval the start reported.
#[tauri::command]
async fn forge_login_poll(auth: State<'_, Arc<ForgeAuth>>, host: String) -> ApiResult<LoginPoll> {
    auth.poll(&host).await
}

/// Forgets the credential for one host, along with any sign-in in flight and
/// everything the service answered while it was held.
#[tauri::command]
async fn forge_sign_out(
    auth: State<'_, Arc<ForgeAuth>>,
    forge: State<'_, ForgeService>,
    host: String,
) -> ApiResult<()> {
    auth.sign_out(&host)?;
    forge.forget_host(&host);
    Ok(())
}

/// The account a stored credential belongs to, or `None` when the host holds
/// no credential.
#[tauri::command]
async fn forge_account(
    auth: State<'_, Arc<ForgeAuth>>,
    host: String,
) -> ApiResult<Option<ForgeAccount>> {
    auth.account(&host).await
}

/// Every repository the signed-in account can reach on one host. The list is
/// searched in the webview, so it is fetched whole and cached.
#[tauri::command]
async fn forge_repositories(
    forge: State<'_, ForgeService>,
    host: String,
    refresh: bool,
) -> ApiResult<Vec<ForgeRepository>> {
    forge.repositories(&host, refresh).await
}

/// Creates a repository on a connected hosting service. The local repository
/// is initialised separately: what comes back is only the remote side.
#[tauri::command]
async fn forge_create_repository(
    forge: State<'_, ForgeService>,
    request: NewRepository,
) -> ApiResult<ForgeRepository> {
    forge.create_repository(&request).await
}

/// Rolled-up check state for a handful of commits, normally the branch tips
/// currently painted. The service caps how many it will ask about.
#[tauri::command]
async fn forge_checks(
    forge: State<'_, ForgeService>,
    repo: ForgeRepo,
    oids: Vec<String>,
    refresh: bool,
) -> ApiResult<Vec<CheckSummary>> {
    forge.checks(&repo, &oids, refresh).await
}

#[tauri::command]
async fn settings_export(settings: AppSettings, destination: String) -> ApiResult<()> {
    tauri::async_runtime::spawn_blocking(move || export_settings(&settings, destination))
        .await
        .map_err(task_join_error)?
}

#[tauri::command]
async fn settings_import(source: String) -> ApiResult<AppSettings> {
    tauri::async_runtime::spawn_blocking(move || import_settings(source))
        .await
        .map_err(task_join_error)?
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
            git_probe,
            launch_repository_path,
            repository_open,
            repository_init,
            repository_clone,
            repository_close,
            repository_snapshot,
            repository_watch,
            repository_unwatch,
            repository_reveal,
            history_page,
            history_search,
            commit_details,
            file_diff,
            conflicts_preflight,
            conflict_details,
            paths_stage,
            paths_unstage,
            paths_discard,
            diff_lines_apply,
            path_stash,
            gitignore_append,
            file_patch_save,
            conflict_resolve,
            conflict_save_edited,
            conflicts_resolve,
            conflicts_auto_resolve,
            create_commit,
            commit_initial,
            commit_reword,
            branch_create,
            branch_checkout,
            branch_rename,
            branch_delete,
            branch_set_upstream,
            branch_merge,
            remote_add,
            remote_fetch,
            remote_pull,
            remote_push,
            commit_checkout,
            tag_create,
            commit_cherry_pick,
            commit_revert,
            commit_reset,
            commit_action_availability,
            operation_continue,
            operation_abort,
            operation_skip,
            stash_list,
            stash_push,
            stash_apply,
            stash_drop,
            persisted_state_load,
            persisted_state_save,
            settings_export,
            settings_import,
            avatars_resolve,
            forge_token_set,
            forge_credentials,
            forge_pull_requests,
            forge_checks,
            forge_login_start,
            forge_login_poll,
            forge_sign_out,
            forge_account,
            forge_repositories,
            forge_create_repository,
        ])
        .run(tauri::generate_context!())
        .expect("error while running GitCat");
}
