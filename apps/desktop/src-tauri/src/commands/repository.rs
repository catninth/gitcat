//! Probing Git and opening, watching and revealing repositories.

use crate::*;

#[tauri::command]
pub(crate) async fn git_probe(core: State<'_, Arc<CoreApi>>) -> ApiResult<GitVersion> {
    core.probe().await
}

#[tauri::command]
pub(crate) async fn repository_open(
    core: State<'_, Arc<CoreApi>>,
    path: String,
) -> ApiResult<OpenedRepository> {
    core.open(path).await.map(Into::into)
}

#[tauri::command]
pub(crate) async fn repository_init(
    core: State<'_, Arc<CoreApi>>,
    path: String,
    default_branch: String,
) -> ApiResult<OpenedRepository> {
    core.init(path, &default_branch).await.map(Into::into)
}

#[tauri::command]
pub(crate) async fn repository_clone(
    core: State<'_, Arc<CoreApi>>,
    options: CloneOptions,
) -> ApiResult<OpenedRepository> {
    core.clone_repository(&options, CancellationToken::new())
        .await
        .map(Into::into)
}

#[tauri::command]
pub(crate) async fn repository_close(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
) -> ApiResult<()> {
    core.close(&repository_id).await
}

#[tauri::command]
pub(crate) async fn repository_snapshot(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
) -> ApiResult<RepositorySnapshot> {
    core.snapshot(&repository_id).await
}

#[tauri::command]
pub(crate) async fn repository_watch(
    core: State<'_, Arc<CoreApi>>,
    watchers: State<'_, RepositoryWatchState>,
    app: AppHandle,
    repository_id: RepositoryId,
) -> ApiResult<()> {
    let paths = core.repository_watch_paths(&repository_id).await?;
    watchers.watch(app, repository_id, paths).map_err(|error| {
        ApiError::new(ErrorCode::Internal, "could not start repository watcher")
            .with_details(error.to_string())
    })
}

#[tauri::command]
pub(crate) fn repository_unwatch(watchers: State<'_, RepositoryWatchState>) {
    watchers.unwatch();
}

pub(crate) fn resolve_inside_root(
    root: &std::path::Path,
    relative: &str,
) -> ApiResult<std::path::PathBuf> {
    let normalized = relative.replace('\\', "/");
    let invalid = || ApiError::new(ErrorCode::InvalidPath, "invalid path");
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
        .map_err(|_| ApiError::new(ErrorCode::InvalidPath, "path does not exist"))?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(invalid());
    }
    Ok(canonical_target)
}

#[tauri::command]
pub(crate) async fn repository_reveal(
    core: State<'_, Arc<CoreApi>>,
    repository_id: RepositoryId,
    path: Option<String>,
) -> ApiResult<()> {
    let worktree = core.repository_root(&repository_id).await?;
    let root = match path.as_deref() {
        Some(relative) => resolve_inside_root(std::path::Path::new(&worktree), relative)?,
        None => std::path::PathBuf::from(&worktree),
    };
    // A file is shown selected in its folder; the platform call needs the
    // plain form of the path, not the verbatim one `canonicalize` returns.
    if root.is_file() {
        return tauri_plugin_opener::reveal_item_in_dir(dunce::simplified(&root)).map_err(
            |error| {
                ApiError::new(ErrorCode::Internal, "could not show file in folder")
                    .with_details(error.to_string())
            },
        );
    }
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
