use std::{
    collections::HashMap,
    ffi::OsString,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use gitcat_contracts::*;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use tokio_util::sync::CancellationToken;

use crate::{
    backend::GitCliBackend,
    limits::*,
    parse::parse_status,
    runner::{GitCommandOutput, GitRunOptions, os_args},
    validate::{is_full_oid, validate_relative_path},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnmergedIndexEntry {
    pub(crate) mode: String,
    pub(crate) oid: String,
    pub(crate) stage: u8,
}

#[derive(Debug)]
pub(crate) struct InspectedConflictResult {
    pub(crate) content: ConflictFileContent,
    pub(crate) identity: ConflictWorktreeIdentity,
}

impl GitCliBackend {
    pub(crate) async fn ensure_conflicted_path(
        &self,
        path: &Path,
        conflict_path: &str,
    ) -> ApiResult<()> {
        validate_relative_path(conflict_path)?;
        let output = self.status_output(path).await?;
        let status = parse_status(&output.stdout)?.status;
        if status
            .entries
            .iter()
            .any(|entry| entry.conflicted && entry.path == conflict_path)
        {
            Ok(())
        } else {
            Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "Selected path is not currently conflicted",
            )
            .with_details(conflict_path.to_owned()))
        }
    }

    pub(crate) async fn unmerged_index_entries(
        &self,
        path: &Path,
        conflict_path: &str,
    ) -> ApiResult<Vec<UnmergedIndexEntry>> {
        validate_relative_path(conflict_path)?;
        let mut args = os_args(&[
            "--literal-pathspecs",
            "ls-files",
            "--unmerged",
            "--stage",
            "-z",
            "--",
        ]);
        args.push(conflict_path.into());
        let output = self.read(Some(path), args).await?;
        parse_unmerged_index_entries(&output.stdout, conflict_path)
    }

    pub(crate) async fn ensure_expected_conflict(
        &self,
        path: &Path,
        conflict_path: &str,
        expected_state: &ConflictExpectedState,
    ) -> ApiResult<Vec<UnmergedIndexEntry>> {
        if let Err(error) = self.ensure_conflicted_path(path, conflict_path).await {
            let had_expected_stages = expected_state.base.is_some()
                || expected_state.ours.is_some()
                || expected_state.theirs.is_some();
            if error.code == ErrorCode::InvalidRequest && had_expected_stages {
                return Err(ApiError::new(
                    ErrorCode::StaleSnapshot,
                    "Conflict changed after the editor was opened",
                )
                .with_details(conflict_path.to_owned()));
            }
            return Err(error);
        }
        let entries = self.unmerged_index_entries(path, conflict_path).await?;
        let worktree = self
            .inspect_conflict_result_async(path, conflict_path)
            .await?;
        let actual = conflict_expected_state(&entries, worktree.identity);
        if actual != *expected_state {
            return Err(ApiError::new(
                ErrorCode::StaleSnapshot,
                "Conflict changed after the editor was opened",
            )
            .with_details(conflict_path.to_owned()));
        }
        Ok(entries)
    }

    pub(crate) async fn conflict_blob_content(
        &self,
        path: &Path,
        oid: &str,
        mode: &str,
    ) -> ApiResult<ConflictFileContent> {
        if !is_full_oid(oid) {
            return Err(ApiError::new(
                ErrorCode::GitCommandFailed,
                "Conflict index contains an invalid object ID",
            ));
        }
        let size_output = self
            .read(Some(path), vec!["cat-file".into(), "-s".into(), oid.into()])
            .await?;
        let size = size_output
            .stdout_lossy()
            .trim()
            .parse::<u64>()
            .map_err(|error| {
                ApiError::new(
                    ErrorCode::GitCommandFailed,
                    "Git returned an invalid conflict blob size",
                )
                .with_details(error.to_string())
            })?;
        if !matches!(mode, "100644" | "100755") {
            return Ok(ConflictFileContent {
                kind: ConflictContentKind::Binary,
                size: Some(size),
                text: None,
                line_ending: None,
            });
        }
        if size > MAX_CONFLICT_TEXT_BYTES as u64 {
            return Ok(ConflictFileContent {
                kind: ConflictContentKind::TooLarge,
                size: Some(size),
                text: None,
                line_ending: None,
            });
        }

        let content = self
            .read(
                Some(path),
                vec!["cat-file".into(), "blob".into(), oid.into()],
            )
            .await?
            .stdout;
        Ok(classify_conflict_content(content, size))
    }

    pub(crate) async fn conflict_index_version(
        &self,
        path: &Path,
        entry: &UnmergedIndexEntry,
    ) -> ApiResult<ConflictIndexVersion> {
        Ok(ConflictIndexVersion {
            oid: entry.oid.clone(),
            mode: entry.mode.clone(),
            content: self
                .conflict_blob_content(path, &entry.oid, &entry.mode)
                .await?,
        })
    }

    pub(crate) fn inspect_conflict_result(
        &self,
        path: &Path,
        conflict_path: &str,
    ) -> ApiResult<InspectedConflictResult> {
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

    pub(crate) async fn stage_conflict_path(
        &self,
        path: &Path,
        conflict_path: &str,
    ) -> ApiResult<MutationResult> {
        let mut args = os_args(&["--literal-pathspecs", "add", "--"]);
        args.push(conflict_path.into());
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    pub(crate) async fn conflicted_paths(&self, path: &Path) -> ApiResult<Vec<String>> {
        let output = self.status_output(path).await?;
        Ok(parse_status(&output.stdout)?
            .status
            .entries
            .into_iter()
            .filter(|entry| entry.conflicted)
            .map(|entry| entry.path)
            .collect())
    }

    pub(crate) async fn unmerged_stages(
        &self,
        path: &Path,
        conflict_paths: &[String],
    ) -> ApiResult<HashMap<String, u8>> {
        let mut stages: HashMap<String, u8> = HashMap::new();
        for chunk in conflict_paths.chunks(BULK_PATH_CHUNK) {
            let mut args = os_args(&[
                "--literal-pathspecs",
                "ls-files",
                "--unmerged",
                "--stage",
                "-z",
                "--",
            ]);
            args.extend(chunk.iter().map(OsString::from));
            let output = self.read(Some(path), args).await?;
            for record in output
                .stdout_lossy()
                .split('\0')
                .filter(|record| !record.is_empty())
            {
                let Some((meta, entry_path)) = record.split_once('\t') else {
                    continue;
                };
                let Some(stage) = meta
                    .split_whitespace()
                    .nth(2)
                    .and_then(|stage| stage.parse::<u8>().ok())
                else {
                    continue;
                };
                *stages.entry(entry_path.to_owned()).or_default() |= 1 << stage;
            }
        }
        Ok(stages)
    }

    pub(crate) async fn run_path_chunks(
        &self,
        path: &Path,
        prefix: &[&str],
        conflict_paths: &[String],
    ) -> ApiResult<()> {
        for chunk in conflict_paths.chunks(BULK_PATH_CHUNK) {
            let mut args = os_args(prefix);
            args.extend(chunk.iter().map(OsString::from));
            let mut options = GitRunOptions::mutation(READ_OUTPUT_CAP);
            options.allow_failure = true;
            let output = self
                .runner
                .run(Some(path), &args, None, CancellationToken::new(), options)
                .await?;
            if !output.success() {
                return Err(self.runner.failure_error(&output));
            }
        }
        Ok(())
    }
}

impl GitCliBackend {
    /// Stages one recorded index version as the resolution and, for a file,
    /// writes it to the working copy. A stage-0 entry replaces every unmerged
    /// stage of the path, so this works for binary files, symlinks and
    /// submodule pointers alike -- which `checkout --ours/--theirs` does not.
    pub(crate) async fn take_index_version(
        &self,
        path: &Path,
        conflict_path: &str,
        entry: &UnmergedIndexEntry,
    ) -> ApiResult<MutationResult> {
        let cacheinfo = format!("{},{},{}", entry.mode, entry.oid, conflict_path);
        let args = vec![
            OsString::from("update-index"),
            OsString::from("--cacheinfo"),
            OsString::from(cacheinfo),
        ];
        let result = self
            .mutate(path, args, None, CancellationToken::new(), false)
            .await?;
        if entry.mode == GITLINK_MODE {
            return Ok(result);
        }
        let mut args = os_args(&["--literal-pathspecs", "checkout", "--"]);
        args.push(conflict_path.into());
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    pub(crate) async fn resolution_warnings(
        &self,
        path: &Path,
        paths: &[String],
    ) -> ApiResult<Vec<ConflictResolutionWarning>> {
        let targets = if paths.is_empty() {
            self.conflicted_paths(path).await?
        } else {
            for target in paths {
                validate_relative_path(target)?;
            }
            paths.to_vec()
        };
        let root = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let mut warnings = Vec::new();
            for target in targets {
                if is_helper_copy(&root, &target) {
                    warnings.push(ConflictResolutionWarning {
                        path: target.clone(),
                        reason: ConflictWarningReason::HelperFile,
                    });
                }
                if worktree_has_conflict_markers(&root.join(&target)) {
                    warnings.push(ConflictResolutionWarning {
                        path: target,
                        reason: ConflictWarningReason::Markers,
                    });
                }
            }
            warnings
        })
        .await
        .map_err(blocking_conflict_task_error)
    }

    pub(crate) async fn describe_submodule_conflict(
        &self,
        path: &Path,
        conflict_path: &str,
    ) -> ApiResult<SubmoduleConflictDetails> {
        self.ensure_conflicted_path(path, conflict_path).await?;
        let entries = self.unmerged_index_entries(path, conflict_path).await?;
        if !entries.iter().any(|entry| entry.mode == GITLINK_MODE) {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "Selected conflict is not a submodule pointer",
            )
            .with_details(conflict_path.to_owned()));
        }
        let submodule = path.join(conflict_path);
        // An uninitialized submodule is an empty folder, and Git run there would
        // answer for the parent repository instead.
        let checked_out = submodule.join(".git").exists();
        let current = if checked_out {
            let output = self
                .read_allow_failure(
                    Some(&submodule),
                    os_args(&["rev-parse", "--verify", "HEAD"]),
                )
                .await?;
            let oid = output.stdout_lossy().trim().to_owned();
            if output.success() && is_full_oid(&oid) {
                Some(self.submodule_side(&submodule, true, oid).await?)
            } else {
                None
            }
        } else {
            None
        };
        let side = |stage| {
            entry_at_stage(&entries, stage)
                .filter(|entry| entry.mode == GITLINK_MODE)
                .map(|entry| entry.oid.clone())
        };
        let mut described = Vec::new();
        for oid in [side(1), side(2), side(3)] {
            described.push(match oid {
                Some(oid) => Some(self.submodule_side(&submodule, checked_out, oid).await?),
                None => None,
            });
        }
        let mut described = described.into_iter();
        Ok(SubmoduleConflictDetails {
            path: conflict_path.to_owned(),
            current,
            base: described.next().flatten(),
            ours: described.next().flatten(),
            theirs: described.next().flatten(),
        })
    }

    async fn submodule_side(
        &self,
        submodule: &Path,
        checked_out: bool,
        oid: String,
    ) -> ApiResult<SubmoduleConflictSide> {
        let mut side = SubmoduleConflictSide {
            oid,
            summary: None,
            author: None,
            committed_at: None,
        };
        if !checked_out {
            return Ok(side);
        }
        let output = self
            .read_allow_failure(
                Some(submodule),
                vec![
                    "log".into(),
                    "-1".into(),
                    "--format=%s%x00%an%x00%ct".into(),
                    side.oid.clone().into(),
                    "--".into(),
                ],
            )
            .await?;
        if output.success() {
            let text = output.stdout_lossy();
            let mut fields = text.trim_end().splitn(3, '\0');
            side.summary = fields.next().map(str::to_owned).filter(|s| !s.is_empty());
            side.author = fields.next().map(str::to_owned).filter(|s| !s.is_empty());
            side.committed_at = fields.next().and_then(|value| value.trim().parse().ok());
        }
        Ok(side)
    }
}

pub(crate) const GITLINK_MODE: &str = "160000";

/// `fd~HEAD` next to a directory `fd`: the copy Git writes aside when a file
/// and a directory claim the same name. Staging it as resolved would commit a
/// file nobody created.
pub(crate) fn is_helper_copy(root: &Path, relative: &str) -> bool {
    let Some((prefix, side)) = relative.rsplit_once('~') else {
        return false;
    };
    !prefix.is_empty() && !side.is_empty() && !side.contains('/') && root.join(prefix).is_dir()
}

/// A complete marker block left in the working copy. Only regular text files
/// within the editor's size limit are read; anything else cannot be checked
/// and is not reported.
pub(crate) fn worktree_has_conflict_markers(file: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(file) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() > MAX_CONFLICT_TEXT_BYTES as u64 {
        return false;
    }
    let Ok(bytes) = fs::read(file) else {
        return false;
    };
    if bytes.contains(&0) {
        return false;
    }
    has_conflict_markers(&String::from_utf8_lossy(&bytes))
}

pub(crate) fn has_conflict_markers(text: &str) -> bool {
    let (mut open, mut middle) = (false, false);
    for line in text.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with("<<<<<<<") {
            open = true;
        } else if open && line == "=======" {
            middle = true;
        } else if middle && line.starts_with(">>>>>>>") {
            return true;
        }
    }
    false
}

pub(crate) fn blocking_conflict_task_error(error: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        ErrorCode::Internal,
        "Conflict file worker task failed unexpectedly",
    )
    .with_details(error.to_string())
}

pub(crate) fn parse_unmerged_index_entries(
    stdout: &[u8],
    expected_path: &str,
) -> ApiResult<Vec<UnmergedIndexEntry>> {
    let mut entries = Vec::new();
    for record in stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::GitCommandFailed,
                    "Git returned malformed unmerged index data",
                )
            })?;
        if &record[tab + 1..] != expected_path.as_bytes() {
            return Err(ApiError::new(
                ErrorCode::GitCommandFailed,
                "Git returned an unexpected unmerged path",
            ));
        }
        let header = std::str::from_utf8(&record[..tab]).map_err(|error| {
            ApiError::new(
                ErrorCode::GitCommandFailed,
                "Git returned non-UTF-8 unmerged index metadata",
            )
            .with_details(error.to_string())
        })?;
        let mut fields = header.split_ascii_whitespace();
        let mode = fields.next().unwrap_or_default();
        let oid = fields.next().unwrap_or_default();
        let stage = fields
            .next()
            .unwrap_or_default()
            .parse::<u8>()
            .map_err(|error| {
                ApiError::new(
                    ErrorCode::GitCommandFailed,
                    "Git returned an invalid conflict stage",
                )
                .with_details(error.to_string())
            })?;
        if fields.next().is_some()
            || !matches!(stage, 1..=3)
            || mode.len() != 6
            || !mode.bytes().all(|byte| byte.is_ascii_digit())
            || !is_full_oid(oid)
            || entries
                .iter()
                .any(|entry: &UnmergedIndexEntry| entry.stage == stage)
        {
            return Err(ApiError::new(
                ErrorCode::GitCommandFailed,
                "Git returned inconsistent unmerged index metadata",
            ));
        }
        entries.push(UnmergedIndexEntry {
            mode: mode.to_owned(),
            oid: oid.to_owned(),
            stage,
        });
    }
    if entries.is_empty() {
        return Err(ApiError::new(
            ErrorCode::StaleSnapshot,
            "Conflict index entries no longer exist",
        )
        .with_details(expected_path.to_owned()));
    }
    entries.sort_unstable_by_key(|entry| entry.stage);
    Ok(entries)
}

pub(crate) fn entry_at_stage(
    entries: &[UnmergedIndexEntry],
    stage: u8,
) -> Option<&UnmergedIndexEntry> {
    entries.iter().find(|entry| entry.stage == stage)
}

pub(crate) fn conflict_expected_state(
    entries: &[UnmergedIndexEntry],
    result: ConflictWorktreeIdentity,
) -> ConflictExpectedState {
    let identity = |stage| {
        entry_at_stage(entries, stage).map(|entry| ConflictStageIdentity {
            oid: entry.oid.clone(),
            mode: entry.mode.clone(),
        })
    };
    ConflictExpectedState {
        base: identity(1),
        ours: identity(2),
        theirs: identity(3),
        result,
    }
}

pub(crate) fn classify_conflict_content(bytes: Vec<u8>, reported_size: u64) -> ConflictFileContent {
    if bytes.len() > MAX_CONFLICT_TEXT_BYTES {
        return ConflictFileContent {
            kind: ConflictContentKind::TooLarge,
            size: Some(bytes.len() as u64),
            text: None,
            line_ending: None,
        };
    }
    if bytes.contains(&0) {
        return ConflictFileContent {
            kind: ConflictContentKind::Binary,
            size: Some(reported_size),
            text: None,
            line_ending: None,
        };
    }
    match String::from_utf8(bytes) {
        Ok(text) => {
            let line_ending = detect_line_ending(&text);
            ConflictFileContent {
                kind: ConflictContentKind::Text,
                size: Some(reported_size),
                text: Some(text),
                line_ending: Some(line_ending),
            }
        }
        Err(_) => ConflictFileContent {
            kind: ConflictContentKind::Binary,
            size: Some(reported_size),
            text: None,
            line_ending: None,
        },
    }
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

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(unix)]
pub(crate) fn os_string_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes().to_vec()
}

#[cfg(windows)]
pub(crate) fn os_string_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    value
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>()
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn os_string_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    value.to_string_lossy().as_bytes().to_vec()
}

pub(crate) fn detect_line_ending(text: &str) -> ConflictLineEnding {
    let bytes = text.as_bytes();
    let mut saw_lf = false;
    let mut saw_crlf = false;
    let mut saw_bare_cr = false;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                saw_crlf = true;
                index += 2;
            }
            b'\r' => {
                saw_bare_cr = true;
                index += 1;
            }
            b'\n' => {
                saw_lf = true;
                index += 1;
            }
            _ => index += 1,
        }
    }
    match (saw_lf, saw_crlf, saw_bare_cr) {
        (false, false, false) => ConflictLineEnding::None,
        (true, false, false) => ConflictLineEnding::Lf,
        (false, true, false) => ConflictLineEnding::CrLf,
        _ => ConflictLineEnding::Mixed,
    }
}

pub(crate) fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

pub(crate) fn encode_edited_conflict_text(
    text: &str,
    policy: ConflictLineEndingPolicy,
    current: &ConflictFileContent,
) -> ApiResult<Vec<u8>> {
    match current.kind {
        ConflictContentKind::Missing | ConflictContentKind::Text => {
            let normalized = normalize_line_endings(text);
            match policy {
                ConflictLineEndingPolicy::Lf => Ok(normalized.into_bytes()),
                ConflictLineEndingPolicy::CrLf => Ok(normalized.replace('\n', "\r\n").into_bytes()),
                ConflictLineEndingPolicy::Preserve => {
                    match current.line_ending.unwrap_or(ConflictLineEnding::None) {
                        ConflictLineEnding::CrLf => {
                            Ok(normalized.replace('\n', "\r\n").into_bytes())
                        }
                        ConflictLineEnding::Lf | ConflictLineEnding::None => {
                            Ok(normalized.into_bytes())
                        }
                        ConflictLineEnding::Mixed => {
                            let original = current.text.as_deref().unwrap_or_default();
                            if normalize_line_endings(original) == normalized {
                                Ok(original.as_bytes().to_vec())
                            } else {
                                Err(ApiError::new(
                                    ErrorCode::InvalidRequest,
                                    "Edited mixed line endings require an explicit LF or CRLF policy",
                                ))
                            }
                        }
                    }
                }
            }
        }
        ConflictContentKind::Binary | ConflictContentKind::TooLarge => Err(ApiError::new(
            ErrorCode::UnsupportedOperation,
            "Built-in text editor cannot save binary or oversized conflict content",
        )),
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

pub(crate) fn summarize_paths(paths: &[&String]) -> String {
    let listed = paths
        .iter()
        .take(MAX_REPORTED_BULK_PATHS)
        .map(|path| path.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if paths.len() > MAX_REPORTED_BULK_PATHS {
        format!(
            "{listed} and {} more",
            paths.len() - MAX_REPORTED_BULK_PATHS
        )
    } else {
        listed
    }
}

pub(crate) fn supports_merge_tree_preflight(version: &GitVersion) -> bool {
    version.major > 2 || (version.major == 2 && version.minor >= 38)
}

pub(crate) fn merge_tree_preflight_unavailable(output: &GitCommandOutput) -> bool {
    let diagnostic = format!(
        "{}\n{}",
        output.stderr_lossy_redacted(),
        output.stdout_lossy()
    )
    .to_ascii_lowercase();
    output.status.code() == Some(129)
        || diagnostic.contains("unknown option")
        || diagnostic.contains("unrecognized option")
        || diagnostic.contains("unknown switch")
}

pub(crate) fn parse_merge_tree_paths(stdout: &[u8]) -> ApiResult<Vec<String>> {
    let mut fields = stdout.split(|byte| *byte == 0);
    let tree_oid = fields
        .next()
        .map(String::from_utf8_lossy)
        .map(|value| value.trim().to_owned())
        .unwrap_or_default();
    if !is_full_oid(&tree_oid) {
        return Err(ApiError::new(
            ErrorCode::GitCommandFailed,
            "Git conflict preflight did not return a valid tree object ID",
        ));
    }

    let mut paths: Vec<String> = fields
        .filter(|field| !field.is_empty())
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect();
    paths.sort_unstable();
    paths.dedup();
    Ok(paths)
}
