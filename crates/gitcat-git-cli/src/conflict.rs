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

mod content;
mod index;
mod result;

pub(crate) use content::*;
pub(crate) use index::*;
pub(crate) use result::*;

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
        // A submodule pointer names a commit in the submodule's own object
        // store; the parent has no blob to read.
        if mode == GITLINK_MODE {
            return Ok(ConflictFileContent {
                kind: ConflictContentKind::Binary,
                size: None,
                text: None,
                line_ending: None,
            });
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

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
