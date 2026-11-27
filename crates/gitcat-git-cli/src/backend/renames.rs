//! Renames between the index and the working tree.
//!
//! `git status` pairs a deletion with an addition only when both are staged.
//! A file renamed on disk is an unstaged deletion plus an untracked file, and
//! Git never compares an untracked file with anything. Marking the untracked
//! files intent-to-add in a scratch copy of the index makes them part of the
//! index-to-worktree diff, where `-M` pairs them the way GitKraken does
//! (`scripts/rename-lab/FINDINGS.md`). The real index is never written.

use tempfile::NamedTempFile;

use super::*;

impl GitCliBackend {
    /// Renames Git finds between an unstaged deletion and an untracked file.
    /// Nothing is asked of Git unless both kinds are present.
    pub(super) async fn worktree_renames(
        &self,
        path: &Path,
        status: &WorktreeStatus,
    ) -> ApiResult<Vec<DetectedRename>> {
        let deleted = status
            .entries
            .iter()
            .any(|entry| matches!(entry.worktree, Some(ChangeKind::Deleted)) && !entry.conflicted);
        let untracked: Vec<String> = status
            .entries
            .iter()
            .filter(|entry| {
                matches!(entry.worktree, Some(ChangeKind::Untracked)) && !entry.submodule
            })
            .map(|entry| entry.path.clone())
            .collect();
        if !deleted || untracked.is_empty() {
            return Ok(Vec::new());
        }
        let Some(index) = self.intent_to_add_index(path, &untracked).await? else {
            return Ok(Vec::new());
        };
        let args = os_args(&[
            "diff",
            "-M",
            "--numstat",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
        ]);
        let output = self
            .runner
            .run(
                Some(path),
                &args,
                None,
                CancellationToken::new(),
                scratch_index_options(&index, READ_OUTPUT_CAP),
            )
            .await?;
        if !output.success() {
            return Err(self.runner.failure_error(&output));
        }
        parse_numstat_renames(&output.stdout)
    }

    /// Adds the renames to `status`. A failure here costs only the pairing,
    /// so the status is still shown with the deletion and the new file apart.
    pub(super) async fn apply_worktree_renames(&self, path: &Path, status: &mut WorktreeStatus) {
        if let Ok(renames) = self.worktree_renames(path, status).await {
            merge_worktree_renames(status, &renames);
        }
    }

    /// The diff of an unstaged rename, against the path it came from. `None`
    /// when Git does not pair the two any more, so the caller can fall back to
    /// showing the new file on its own.
    pub(super) async fn worktree_rename_diff(
        &self,
        path: &Path,
        request: &DiffRequest,
        old_path: &str,
    ) -> ApiResult<Option<FileDiff>> {
        let Some(index) = self
            .intent_to_add_index(path, std::slice::from_ref(&request.path))
            .await?
        else {
            return Ok(None);
        };
        let mut args = os_args(&[
            "diff",
            "-M",
            "--patch",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
        ]);
        args.push(format!("--unified={}", unified_context(request)).into());
        if request.ignore_whitespace {
            args.push("--ignore-all-space".into());
        }
        args.push("--".into());
        args.push(old_path.into());
        args.push(request.path.as_str().into());
        let mut options = scratch_index_options(&index, request.max_bytes.clamp(1, MAX_DIFF_BYTES));
        options.allow_stdout_truncation = true;
        options.timeout = Some(Duration::from_secs(60));
        let output = self
            .runner
            .run(Some(path), &args, None, CancellationToken::new(), options)
            .await?;
        if !output.success() {
            return Err(self.runner.failure_error(&output));
        }
        let diff = parse_file_diff(&output.stdout, &request.path, output.stdout_truncated);
        Ok(diff
            .ok()
            .filter(|diff| diff.status == ChangeKind::Renamed && diff.new_path == request.path))
    }

    /// A copy of the index with `paths` marked intent-to-add. `None` when the
    /// repository has no index yet, which also means nothing can be renamed.
    async fn intent_to_add_index(
        &self,
        path: &Path,
        paths: &[String],
    ) -> ApiResult<Option<NamedTempFile>> {
        let located = self
            .read(
                Some(path),
                os_args(&["rev-parse", "--path-format=absolute", "--git-path", "index"]),
            )
            .await?;
        let source = PathBuf::from(located.stdout_lossy().trim());
        if !source.is_file() {
            return Ok(None);
        }
        let scratch = NamedTempFile::new().map_err(scratch_index_error)?;
        fs::copy(&source, scratch.path()).map_err(scratch_index_error)?;

        let mut list = Vec::new();
        for relative in paths {
            list.extend_from_slice(relative.as_bytes());
            list.push(0);
        }
        let args = os_args(&[
            "add",
            "--intent-to-add",
            "--pathspec-from-file=-",
            "--pathspec-file-nul",
        ]);
        let output = self
            .runner
            .run(
                Some(path),
                &args,
                Some(&list),
                CancellationToken::new(),
                scratch_index_options(&scratch, READ_OUTPUT_CAP),
            )
            .await?;
        if !output.success() {
            return Err(self.runner.failure_error(&output));
        }
        Ok(Some(scratch))
    }
}

/// Options for a command that reads and writes only the scratch index: it
/// leaves the repository's own files alone, so it runs as a read.
fn scratch_index_options(index: &NamedTempFile, output_cap: usize) -> GitRunOptions {
    let mut options = GitRunOptions::read_only(output_cap);
    options.allow_failure = true;
    options
        .extra_env
        .push(("GIT_INDEX_FILE".into(), index.path().as_os_str().to_owned()));
    options
}

fn scratch_index_error(error: std::io::Error) -> ApiError {
    ApiError::new(
        ErrorCode::Internal,
        "A scratch copy of the index could not be prepared",
    )
    .with_details(error.to_string())
}
