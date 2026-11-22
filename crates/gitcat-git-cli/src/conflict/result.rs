//! Reading and atomically writing the worktree copy of a conflicted file.

use super::*;

impl GitCliBackend {
    pub(crate) fn inspect_conflict_result(
        &self,
        path: &Path,
        conflict_path: &str,
    ) -> ApiResult<InspectedConflictResult> {
        validate_relative_path(conflict_path)?;
        // A submodule's working copy is a directory. It has no content to
        // compare, so it is described the way a missing file is.
        if fs::symlink_metadata(path.join(conflict_path)).is_ok_and(|metadata| metadata.is_dir()) {
            return Ok(InspectedConflictResult {
                content: ConflictFileContent {
                    kind: ConflictContentKind::Missing,
                    size: None,
                    text: None,
                    line_ending: None,
                },
                identity: missing_worktree_identity(),
            });
        }
        let target = checked_worktree_target(path, conflict_path, false)?;
        let metadata = match fs::symlink_metadata(&target) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(InspectedConflictResult {
                    content: ConflictFileContent {
                        kind: ConflictContentKind::Missing,
                        size: None,
                        text: None,
                        line_ending: None,
                    },
                    identity: missing_worktree_identity(),
                });
            }
            Err(error) => {
                return Err(ApiError::new(
                    ErrorCode::Io,
                    "Conflict result metadata could not be read",
                )
                .with_details(error.to_string()));
            }
        };
        if metadata.file_type().is_symlink() {
            let link = fs::read_link(&target).map_err(|error| {
                ApiError::new(ErrorCode::Io, "Conflict symlink target could not be read")
                    .with_details(error.to_string())
            })?;
            let link_bytes = os_string_bytes(link.as_os_str());
            let size = link_bytes.len() as u64;
            return Ok(InspectedConflictResult {
                content: ConflictFileContent {
                    kind: ConflictContentKind::Binary,
                    size: Some(size),
                    text: None,
                    line_ending: None,
                },
                identity: ConflictWorktreeIdentity {
                    kind: ConflictWorktreeKind::Symlink,
                    size: Some(size),
                    sha256: Some(sha256_hex(&link_bytes)),
                    line_ending: None,
                    mode: None,
                },
            });
        }
        if !metadata.is_file() {
            return Err(ApiError::new(
                ErrorCode::InvalidPath,
                "Conflict result path is not a regular file or symbolic link",
            ));
        }
        let mut file = fs::File::open(&target).map_err(|error| {
            ApiError::new(ErrorCode::Io, "Conflict result could not be opened")
                .with_details(error.to_string())
        })?;
        let mut hasher = Sha256::new();
        let mut preview = Vec::with_capacity(MAX_CONFLICT_TEXT_BYTES + 1);
        let mut buffer = [0_u8; 64 * 1024];
        let mut size = 0_u64;
        loop {
            let read = file.read(&mut buffer).map_err(|error| {
                ApiError::new(ErrorCode::Io, "Conflict result could not be read")
                    .with_details(error.to_string())
            })?;
            if read == 0 {
                break;
            }
            size = size.checked_add(read as u64).ok_or_else(|| {
                ApiError::new(ErrorCode::OutputTooLarge, "Conflict result size overflowed")
            })?;
            hasher.update(&buffer[..read]);
            if preview.len() <= MAX_CONFLICT_TEXT_BYTES {
                let remaining = MAX_CONFLICT_TEXT_BYTES + 1 - preview.len();
                preview.extend_from_slice(&buffer[..read.min(remaining)]);
            }
        }
        let content = if size > MAX_CONFLICT_TEXT_BYTES as u64 {
            ConflictFileContent {
                kind: ConflictContentKind::TooLarge,
                size: Some(size),
                text: None,
                line_ending: None,
            }
        } else {
            classify_conflict_content(preview, size)
        };
        Ok(InspectedConflictResult {
            identity: ConflictWorktreeIdentity {
                kind: ConflictWorktreeKind::Regular,
                size: Some(size),
                sha256: Some(format!("{:x}", hasher.finalize())),
                line_ending: content.line_ending,
                mode: conflict_file_mode(&metadata),
            },
            content,
        })
    }

    pub(crate) async fn inspect_conflict_result_async(
        &self,
        path: &Path,
        conflict_path: &str,
    ) -> ApiResult<InspectedConflictResult> {
        let backend = self.clone();
        let path = path.to_path_buf();
        let conflict_path = conflict_path.to_owned();
        tokio::task::spawn_blocking(move || backend.inspect_conflict_result(&path, &conflict_path))
            .await
            .map_err(blocking_conflict_task_error)?
    }

    pub(crate) fn write_conflict_result(
        &self,
        path: &Path,
        conflict_path: &str,
        text: &str,
        line_ending: ConflictLineEndingPolicy,
        expected_result: &ConflictWorktreeIdentity,
        desired_mode: &str,
    ) -> ApiResult<()> {
        let current = self.inspect_conflict_result(path, conflict_path)?;
        if current.identity != *expected_result {
            return Err(ApiError::new(
                ErrorCode::StaleSnapshot,
                "Conflict result changed after the editor was opened",
            )
            .with_details(conflict_path.to_owned()));
        }
        let bytes = encode_edited_conflict_text(text, line_ending, &current.content)?;
        if bytes.len() > MAX_CONFLICT_TEXT_BYTES {
            return Err(ApiError::new(
                ErrorCode::OutputTooLarge,
                "Edited conflict result exceeds the safe editor limit",
            )
            .with_details(format!(
                "maximum={} bytes, actual={} bytes",
                MAX_CONFLICT_TEXT_BYTES,
                bytes.len()
            )));
        }
        let target = checked_worktree_target(path, conflict_path, true)?;
        let parent = target.parent().ok_or_else(|| {
            ApiError::new(ErrorCode::InvalidPath, "Conflict result parent is missing")
        })?;
        let original_permissions = fs::symlink_metadata(&target)
            .ok()
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.permissions());
        let mut temporary = new_conflict_temporary(
            parent,
            original_permissions.is_none().then_some(desired_mode),
        )?;
        if let Some(permissions) = original_permissions {
            temporary
                .as_file()
                .set_permissions(permissions)
                .map_err(|error| {
                    ApiError::new(
                        ErrorCode::Io,
                        "Conflict result permissions could not be preserved",
                    )
                    .with_details(error.to_string())
                })?;
        }
        temporary.write_all(&bytes).map_err(|error| {
            ApiError::new(ErrorCode::Io, "Edited conflict result could not be written")
                .with_details(error.to_string())
        })?;
        temporary.flush().map_err(|error| {
            ApiError::new(ErrorCode::Io, "Edited conflict result could not be flushed")
                .with_details(error.to_string())
        })?;
        temporary.as_file().sync_all().map_err(|error| {
            ApiError::new(ErrorCode::Io, "Edited conflict result could not be synced")
                .with_details(error.to_string())
        })?;

        checked_worktree_target(path, conflict_path, true)?;
        let immediately_before_replace = self.inspect_conflict_result(path, conflict_path)?;
        if immediately_before_replace.identity != *expected_result {
            return Err(ApiError::new(
                ErrorCode::StaleSnapshot,
                "Conflict result changed while the edited result was being saved",
            )
            .with_details(conflict_path.to_owned()));
        }
        temporary.persist(&target).map_err(|error| {
            ApiError::new(
                ErrorCode::Io,
                "Edited conflict result could not atomically replace the working file",
            )
            .with_details(error.error.to_string())
        })?;
        Ok(())
    }

    pub(crate) async fn write_conflict_result_async(
        &self,
        path: &Path,
        conflict_path: &str,
        text: &str,
        line_ending: ConflictLineEndingPolicy,
        expected_result: &ConflictWorktreeIdentity,
        desired_mode: &str,
    ) -> ApiResult<()> {
        let backend = self.clone();
        let path = path.to_path_buf();
        let conflict_path = conflict_path.to_owned();
        let text = text.to_owned();
        let expected_result = expected_result.clone();
        let desired_mode = desired_mode.to_owned();
        tokio::task::spawn_blocking(move || {
            backend.write_conflict_result(
                &path,
                &conflict_path,
                &text,
                line_ending,
                &expected_result,
                &desired_mode,
            )
        })
        .await
        .map_err(blocking_conflict_task_error)?
    }
}

pub(crate) fn blocking_conflict_task_error(error: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        ErrorCode::Internal,
        "Conflict file worker task failed unexpectedly",
    )
    .with_details(error.to_string())
}

pub(crate) fn missing_worktree_identity() -> ConflictWorktreeIdentity {
    ConflictWorktreeIdentity {
        kind: ConflictWorktreeKind::Missing,
        size: None,
        sha256: None,
        line_ending: None,
        mode: None,
    }
}

pub(crate) fn new_conflict_temporary(
    parent: &Path,
    desired_mode: Option<&str>,
) -> ApiResult<NamedTempFile> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut builder = tempfile::Builder::new();
        if let Some(desired_mode) = desired_mode {
            // Passing creation permissions lets the operating system apply the
            // process umask. A later chmod to 0644/0755 would bypass it.
            let mode = if desired_mode == "100755" {
                0o777
            } else {
                0o666
            };
            builder.permissions(fs::Permissions::from_mode(mode));
        }
        builder
            .tempfile_in(parent)
            .map_err(conflict_temporary_error)
    }
    #[cfg(not(unix))]
    {
        let _ = desired_mode;
        NamedTempFile::new_in(parent).map_err(conflict_temporary_error)
    }
}

pub(crate) fn conflict_temporary_error(error: std::io::Error) -> ApiError {
    ApiError::new(
        ErrorCode::Io,
        "Temporary conflict result could not be created",
    )
    .with_details(error.to_string())
}

#[cfg(unix)]
pub(crate) fn conflict_file_mode(metadata: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;

    Some(metadata.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
pub(crate) fn conflict_file_mode(_metadata: &fs::Metadata) -> Option<u32> {
    None
}

pub(crate) fn checked_worktree_target(
    repository: &Path,
    relative_path: &str,
    reject_target_symlink: bool,
) -> ApiResult<PathBuf> {
    validate_relative_path(relative_path)?;
    let root = dunce::canonicalize(repository).map_err(|error| {
        ApiError::new(
            ErrorCode::InvalidRepository,
            "Repository root could not be resolved",
        )
        .with_details(error.to_string())
    })?;
    let lexical_target = root.join(relative_path);
    let parent = lexical_target.parent().ok_or_else(|| {
        ApiError::new(
            ErrorCode::InvalidPath,
            "Conflict result has no repository-relative parent",
        )
    })?;
    let canonical_parent = dunce::canonicalize(parent).map_err(|error| {
        ApiError::new(
            ErrorCode::InvalidPath,
            "Conflict result parent could not be resolved",
        )
        .with_details(error.to_string())
    })?;
    if !canonical_parent.starts_with(&root) {
        return Err(ApiError::new(
            ErrorCode::ProtectedOperation,
            "Conflict result path escapes the repository through a symlink",
        ));
    }
    let file_name = lexical_target.file_name().ok_or_else(|| {
        ApiError::new(ErrorCode::InvalidPath, "Conflict result path is malformed")
    })?;
    let target = canonical_parent.join(file_name);
    match fs::symlink_metadata(&target) {
        Ok(metadata) => {
            if reject_target_symlink && metadata.file_type().is_symlink() {
                return Err(ApiError::new(
                    ErrorCode::ProtectedOperation,
                    "Edited conflict result cannot replace a symbolic link",
                ));
            }
            if !metadata.file_type().is_symlink() {
                let canonical_target = dunce::canonicalize(&target).map_err(|error| {
                    ApiError::new(
                        ErrorCode::InvalidPath,
                        "Conflict result could not be resolved",
                    )
                    .with_details(error.to_string())
                })?;
                if !canonical_target.starts_with(&root) {
                    return Err(ApiError::new(
                        ErrorCode::ProtectedOperation,
                        "Conflict result path escapes the repository",
                    ));
                }
                if !metadata.is_file() {
                    return Err(ApiError::new(
                        ErrorCode::InvalidPath,
                        "Conflict result path is not a regular file",
                    ));
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(
                ApiError::new(ErrorCode::Io, "Conflict result metadata could not be read")
                    .with_details(error.to_string()),
            );
        }
    }
    Ok(target)
}
