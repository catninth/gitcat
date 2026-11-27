//! Staging, discarding and line-level changes to the working tree.

use super::*;

impl GitCliBackend {
    pub(super) async fn is_untracked_file(&self, path: &Path, relative: &str) -> ApiResult<bool> {
        if !path.join(relative).is_file() {
            return Ok(false);
        }
        let args = vec![
            "ls-files".into(),
            "-z".into(),
            "--".into(),
            OsString::from(relative),
        ];
        let listed = self.read(Some(path), args).await?;
        Ok(listed.stdout.iter().all(|byte| *byte == 0))
    }

    pub(super) async fn untracked_diff(
        &self,
        path: &Path,
        request: &DiffRequest,
    ) -> ApiResult<FileDiff> {
        let mut args = os_args(&[
            "diff",
            "--no-index",
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
        args.push("/dev/null".into());
        args.push(request.path.as_str().into());
        let mut options = GitRunOptions::read_only(request.max_bytes.clamp(1, MAX_DIFF_BYTES));
        options.allow_stdout_truncation = true;
        options.allow_failure = true;
        options.timeout = Some(Duration::from_secs(60));
        let output = self
            .runner
            .run(Some(path), &args, None, CancellationToken::new(), options)
            .await?;
        if !matches!(output.status.code(), Some(0 | 1)) {
            return Err(self.runner.failure_error(&output));
        }
        parse_file_diff(&output.stdout, &request.path, output.stdout_truncated)
    }

    pub(super) async fn seed_empty_index_entry(
        &self,
        path: &Path,
        relative: &str,
    ) -> ApiResult<()> {
        let blob = self
            .runner
            .run(
                Some(path),
                &os_args(&["hash-object", "-w", "--stdin"]),
                Some(&[]),
                CancellationToken::new(),
                GitRunOptions::mutation(READ_OUTPUT_CAP),
            )
            .await?
            .stdout_lossy()
            .trim()
            .to_owned();
        self.runner
            .run(
                Some(path),
                &[
                    "update-index".into(),
                    "--add".into(),
                    "--cacheinfo".into(),
                    format!("100644,{blob},{relative}").into(),
                ],
                None,
                CancellationToken::new(),
                GitRunOptions::mutation(READ_OUTPUT_CAP),
            )
            .await?;
        Ok(())
    }

    // `untracked` reads the file against nothing, which only a discard needs:
    // staging seeds the index first and then has an ordinary diff to read.
    pub(super) async fn apply_line_patch(
        &self,
        path: &Path,
        request: &LinePatchRequest,
        untracked: bool,
    ) -> ApiResult<()> {
        let mut args = if untracked {
            os_args(&["diff", "--no-index"])
        } else if request.action == LinePatchAction::Unstage {
            os_args(&["diff", "--cached"])
        } else {
            os_args(&["diff"])
        };
        args.extend(os_args(&[
            "--patch",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--no-renames",
            "--unified=3",
            "--",
        ]));
        if untracked {
            args.push("/dev/null".into());
        }
        args.push(request.path.as_str().into());
        let mut options = GitRunOptions::read_only(MAX_DIFF_BYTES);
        options.allow_failure = untracked;
        options.timeout = Some(Duration::from_secs(60));
        let output = self
            .runner
            .run(Some(path), &args, None, CancellationToken::new(), options)
            .await?;
        if untracked && !matches!(output.status.code(), Some(0 | 1)) {
            return Err(self.runner.failure_error(&output));
        }

        let direction = if request.action == LinePatchAction::Stage {
            PatchDirection::Forward
        } else {
            PatchDirection::Reverse
        };
        let patch = build_line_patch(&output.stdout, &request.path, &request.lines, direction)?;
        // A file Git has never stored has no line endings of its own to
        // restore, so it keeps the ones it was written with.
        let mut apply = if untracked {
            os_args(&["-c", "core.autocrlf=false", "apply", "--whitespace=nowarn"])
        } else {
            os_args(&["apply", "--whitespace=nowarn"])
        };
        if request.action != LinePatchAction::Discard {
            apply.push("--cached".into());
        }
        if direction == PatchDirection::Reverse {
            apply.push("--reverse".into());
        }
        apply.push("-".into());
        self.runner
            .run(
                Some(path),
                &apply,
                Some(&patch),
                CancellationToken::new(),
                GitRunOptions::mutation(READ_OUTPUT_CAP),
            )
            .await?;
        Ok(())
    }

    pub(super) async fn line_stats(
        &self,
        path: &Path,
        cached: bool,
    ) -> ApiResult<HashMap<String, LineStats>> {
        let mut args = os_args(&[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "-M",
            "--numstat",
            "-z",
        ]);
        if cached {
            args.push("--cached".into());
        }
        let output = self.read_allow_failure(Some(path), args).await?;
        if !output.success() {
            return Ok(HashMap::new());
        }
        parse_line_stats(&output.stdout)
    }

    // Untracked files never reach `git diff --numstat`, so their additions come
    // from counting the lines on disk. Large or binary files stay unmeasured.
    pub(super) async fn untracked_line_stats(
        &self,
        path: &Path,
        paths: Vec<String>,
    ) -> ApiResult<HashMap<String, LineStats>> {
        if paths.is_empty() {
            return Ok(HashMap::new());
        }
        let root = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let mut stats = HashMap::new();
            for relative in paths.into_iter().take(MAX_UNTRACKED_STAT_FILES) {
                let file = root.join(&relative);
                let Ok(metadata) = fs::metadata(&file) else {
                    continue;
                };
                if !metadata.is_file() || metadata.len() > MAX_UNTRACKED_STAT_BYTES {
                    continue;
                }
                let Ok(bytes) = fs::read(&file) else {
                    continue;
                };
                if bytes.contains(&0) {
                    continue;
                }
                let lines = bytes.iter().filter(|byte| **byte == b'\n').count() as u64
                    + u64::from(!bytes.is_empty() && !bytes.ends_with(b"\n"));
                stats.insert(
                    relative,
                    LineStats {
                        additions: lines,
                        deletions: 0,
                    },
                );
            }
            stats
        })
        .await
        .map_err(blocking_line_stats_task_error)
    }

    pub(super) async fn apply_line_stats(
        &self,
        path: &Path,
        status: &mut WorktreeStatus,
    ) -> ApiResult<()> {
        if status.entries.is_empty() {
            return Ok(());
        }
        let untracked: Vec<String> = status
            .entries
            .iter()
            .filter(|entry| {
                matches!(entry.worktree, Some(ChangeKind::Untracked)) && !entry.submodule
            })
            .map(|entry| entry.path.clone())
            .collect();
        let (worktree, index, new_files) = tokio::try_join!(
            self.line_stats(path, false),
            self.line_stats(path, true),
            self.untracked_line_stats(path, untracked)
        )?;
        for entry in &mut status.entries {
            entry.index_stats = index.get(&entry.path).copied();
            entry.worktree_stats = match entry.worktree {
                Some(ChangeKind::Untracked) => new_files.get(&entry.path).copied(),
                // Counted against the path it came from when the rename was found.
                Some(ChangeKind::Renamed) => entry.worktree_stats,
                _ => worktree.get(&entry.path).copied(),
            };
        }
        Ok(())
    }

    pub(super) async fn stage_paths_inner(
        &self,
        path: &Path,
        paths: &[String],
    ) -> ApiResult<MutationResult> {
        if paths.is_empty() {
            return self.mutation_result(path, self.head_oid(path).await?).await;
        }
        validate_paths(paths)?;
        let mut args = os_args(&["add", "--"]);
        args.extend(paths.iter().map(OsString::from));
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    pub(super) async fn unstage_paths_inner(
        &self,
        path: &Path,
        paths: &[String],
    ) -> ApiResult<MutationResult> {
        if paths.is_empty() {
            return self.mutation_result(path, self.head_oid(path).await?).await;
        }
        validate_paths(paths)?;
        let mut args = if self.head_oid(path).await?.is_some() {
            os_args(&["restore", "--staged", "--"])
        } else {
            os_args(&["rm", "--cached", "--force", "--ignore-unmatch", "--"])
        };
        args.extend(paths.iter().map(OsString::from));
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    pub(super) async fn discard_paths_inner(
        &self,
        path: &Path,
        paths: &[String],
    ) -> ApiResult<MutationResult> {
        if paths.is_empty() {
            return self.mutation_result(path, self.head_oid(path).await?).await;
        }
        validate_paths(paths)?;
        let before = self.head_oid(path).await?;

        let mut ls_args = os_args(&["ls-files", "-z", "--"]);
        ls_args.extend(paths.iter().map(OsString::from));
        let listed = self.read(Some(path), ls_args).await?;
        let listed_text = listed.stdout_lossy();
        let tracked: std::collections::HashSet<&str> = listed_text
            .split('\0')
            .filter(|entry| !entry.is_empty())
            .collect();
        let tracked_paths: Vec<&String> = paths
            .iter()
            .filter(|p| tracked.contains(p.as_str()))
            .collect();
        let untracked_paths: Vec<&String> = paths
            .iter()
            .filter(|p| !tracked.contains(p.as_str()))
            .collect();

        if !tracked_paths.is_empty() {
            let mut args = if self.head_oid(path).await?.is_some() {
                os_args(&["restore", "--staged", "--worktree", "--source=HEAD", "--"])
            } else {
                os_args(&["rm", "--cached", "--force", "--ignore-unmatch", "--"])
            };
            args.extend(tracked_paths.iter().map(|p| OsString::from(p.as_str())));
            let output = self
                .runner
                .run(Some(path), &args, None, CancellationToken::new(), {
                    let mut options = GitRunOptions::mutation(READ_OUTPUT_CAP);
                    options.allow_failure = true;
                    options
                })
                .await?;
            if !output.success() {
                return Err(self.runner.failure_error(&output));
            }
        }

        if !untracked_paths.is_empty() {
            let mut args = os_args(&["clean", "--force", "-d", "--"]);
            args.extend(untracked_paths.iter().map(|p| OsString::from(p.as_str())));
            let output = self
                .runner
                .run(Some(path), &args, None, CancellationToken::new(), {
                    let mut options = GitRunOptions::mutation(READ_OUTPUT_CAP);
                    options.allow_failure = true;
                    options
                })
                .await?;
            if !output.success() {
                return Err(self.runner.failure_error(&output));
            }
        }

        self.mutation_result(path, before).await
    }

    pub(super) async fn apply_diff_lines_inner(
        &self,
        path: &Path,
        request: &LinePatchRequest,
    ) -> ApiResult<MutationResult> {
        validate_relative_path(&request.path)?;
        let before = self.head_oid(path).await?;
        if request.lines.is_empty() {
            return self.mutation_result(path, before).await;
        }
        let untracked = request.action != LinePatchAction::Unstage
            && self.is_untracked_file(path, &request.path).await?;
        // Staging part of a new file needs something in the index to patch.
        // An empty blob is that; an intent-to-add entry is not, because
        // `git apply --cached` treats one as absent.
        let seeded = untracked && request.action == LinePatchAction::Stage;
        if seeded {
            self.seed_empty_index_entry(path, &request.path).await?;
        }
        let result = self
            .apply_line_patch(path, request, untracked && !seeded)
            .await;
        if result.is_err() && seeded {
            let _ = self
                .read_allow_failure(
                    Some(path),
                    vec![
                        "rm".into(),
                        "--cached".into(),
                        "--quiet".into(),
                        "--force".into(),
                        "--".into(),
                        request.path.as_str().into(),
                    ],
                )
                .await;
        }
        result?;
        self.mutation_result(path, before).await
    }

    pub(super) async fn stash_paths_inner(
        &self,
        path: &Path,
        paths: &[String],
        message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        validate_paths(paths)?;
        if paths.is_empty() {
            return self.mutation_result(path, self.head_oid(path).await?).await;
        }
        let mut add_args = os_args(&["add", "-A", "--"]);
        add_args.extend(paths.iter().map(OsString::from));
        self.mutate(path, add_args, None, CancellationToken::new(), false)
            .await?;

        let mut args = os_args(&["stash", "push"]);
        let message = match message {
            Some(message) => Some(message.to_owned()),
            None => self.default_stash_message(path).await?,
        };
        if let Some(message) = message {
            if message.len() > MAX_COMMIT_MESSAGE_BYTES || message.contains('\0') {
                return Err(ApiError::new(
                    ErrorCode::InvalidSettings,
                    "Stash message is too large",
                ));
            }
            args.push("--message".into());
            args.push(message.into());
        }
        args.push("--".into());
        args.extend(paths.iter().map(OsString::from));
        let result = self
            .mutate(path, args, None, CancellationToken::new(), false)
            .await;
        if result.is_err() {
            let mut restore_args = os_args(&["restore", "--staged", "--"]);
            restore_args.extend(paths.iter().map(OsString::from));
            let _ = self.read_allow_failure(Some(path), restore_args).await;
        }
        result
    }

    pub(super) async fn append_gitignore_inner(
        &self,
        path: &Path,
        patterns: &[String],
    ) -> ApiResult<MutationResult> {
        let before = self.head_oid(path).await?;
        let cleaned: Vec<String> = patterns
            .iter()
            .map(|pattern| pattern.trim().to_owned())
            .filter(|pattern| !pattern.is_empty())
            .collect();
        for pattern in &cleaned {
            if pattern.contains('\0') || pattern.contains('\n') {
                return Err(ApiError::new(
                    ErrorCode::InvalidRequest,
                    "Ignore pattern contains an invalid character",
                ));
            }
        }
        if cleaned.is_empty() {
            return self.mutation_result(path, before).await;
        }

        let gitignore = path.join(".gitignore");
        let existing = std::fs::read_to_string(&gitignore).unwrap_or_default();
        let present: std::collections::HashSet<&str> =
            existing.lines().map(|line| line.trim()).collect();
        let additions: Vec<&String> = cleaned
            .iter()
            .filter(|pattern| !present.contains(pattern.as_str()))
            .collect();
        if additions.is_empty() {
            return self.mutation_result(path, before).await;
        }

        let mut next = existing;
        if !next.is_empty() && !next.ends_with('\n') {
            next.push('\n');
        }
        for pattern in additions {
            next.push_str(pattern);
            next.push('\n');
        }
        std::fs::write(&gitignore, next).map_err(|error| {
            ApiError::new(ErrorCode::Internal, "Could not update .gitignore")
                .with_details(error.to_string())
        })?;
        self.mutation_result(path, before).await
    }

    pub(super) async fn create_patch_inner(
        &self,
        path: &Path,
        paths: &[String],
        staged: bool,
    ) -> ApiResult<String> {
        validate_paths(paths)?;
        let mut args = if staged {
            os_args(&["diff", "--cached", "--"])
        } else {
            os_args(&["diff", "--"])
        };
        args.extend(paths.iter().map(OsString::from));
        let output = self.read(Some(path), args).await?;
        Ok(output.stdout_lossy())
    }
}
