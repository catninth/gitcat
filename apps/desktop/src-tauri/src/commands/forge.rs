//! Hosting service sign-in, tokens, avatars, pull requests and checks.

use crate::*;

/// Resolves commit author avatars for one repository page. Authors that stay
/// unresolved are absent from the result and keep their drawn initial.
#[tauri::command]
pub(crate) async fn avatars_resolve(
    avatars: State<'_, AvatarService>,
    lookup: AvatarLookup,
    settings: AvatarSettings,
) -> ApiResult<Vec<AvatarEntry>> {
    avatars.resolve(&lookup, settings).await
}

/// Stores or clears the token for one host. Passing `None` removes it.
#[tauri::command]
pub(crate) async fn forge_token_set(
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
pub(crate) async fn forge_credentials(
    auth: State<'_, Arc<ForgeAuth>>,
) -> ApiResult<Vec<ForgeCredential>> {
    auth.credentials()
}

/// The pull requests open against one repository.
///
/// `refresh` bypasses the short-lived cache, which is what a manual refresh
/// after a push wants; an ordinary redraw leaves it alone.
#[tauri::command]
pub(crate) async fn forge_pull_requests(
    forge: State<'_, ForgeService>,
    repo: ForgeRepo,
    refresh: bool,
) -> ApiResult<Vec<PullRequestInfo>> {
    forge.pull_requests(&repo, refresh).await
}

/// Where a remote's repository lives now, which differs from its URL once the
/// repository has been transferred or renamed.
#[tauri::command]
pub(crate) async fn forge_repository_location(
    forge: State<'_, ForgeService>,
    repo: ForgeRepo,
) -> ApiResult<ForgeRepo> {
    forge.location(&repo).await
}

/// Starts a device-flow sign-in and returns what the user has to type where.
/// The device code itself stays in the backend.
#[tauri::command]
pub(crate) async fn forge_login_start(
    auth: State<'_, Arc<ForgeAuth>>,
    host: String,
) -> ApiResult<DeviceAuthorization> {
    auth.begin(&host).await
}

/// Asks once whether the sign-in has been authorised. The webview repeats this
/// at the interval the start reported.
#[tauri::command]
pub(crate) async fn forge_login_poll(
    auth: State<'_, Arc<ForgeAuth>>,
    host: String,
) -> ApiResult<LoginPoll> {
    auth.poll(&host).await
}

/// Forgets the credential for one host, along with any sign-in in flight and
/// everything the service answered while it was held.
#[tauri::command]
pub(crate) async fn forge_sign_out(
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
pub(crate) async fn forge_account(
    auth: State<'_, Arc<ForgeAuth>>,
    host: String,
) -> ApiResult<Option<ForgeAccount>> {
    auth.account(&host).await
}

/// Every repository the signed-in account can reach on one host. The list is
/// searched in the webview, so it is fetched whole and cached.
#[tauri::command]
pub(crate) async fn forge_repositories(
    forge: State<'_, ForgeService>,
    host: String,
    refresh: bool,
) -> ApiResult<Vec<ForgeRepository>> {
    forge.repositories(&host, refresh).await
}

/// Creates a repository on a connected hosting service. The local repository
/// is initialised separately: what comes back is only the remote side.
#[tauri::command]
pub(crate) async fn forge_create_repository(
    forge: State<'_, ForgeService>,
    request: NewRepository,
) -> ApiResult<ForgeRepository> {
    forge.create_repository(&request).await
}

/// Rolled-up check state for a handful of commits, normally the branch tips
/// currently painted. The service caps how many it will ask about.
#[tauri::command]
pub(crate) async fn forge_checks(
    forge: State<'_, ForgeService>,
    repo: ForgeRepo,
    oids: Vec<String>,
    refresh: bool,
) -> ApiResult<Vec<CheckSummary>> {
    forge.checks(&repo, &oids, refresh).await
}
