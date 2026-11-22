//! Conflict preflight and resolving conflicted paths.

use super::*;

impl GitCliBackend {
    pub(super) async fn conflict_preflight_inner(
        &self,
        path: &Path,
        target: &str,
    ) -> ApiResult<ConflictPreflightResult> {
        // Resolve caller input before feature detection or merge simulation. The
        // merge-tree command receives only full object IDs, never an option-like ref.
        let target_oid = self.resolve_commit(path, target).await?;
        let version = self.probe().await?;
        if !supports_merge_tree_preflight(&version) {
            return Ok(ConflictPreflightResult {
                target: target.to_owned(),
                target_oid,
                state: ConflictPreflightState::Unavailable,
                conflicting_paths: Vec::new(),
                unavailable_reason: Some(
                    "Conflict preflight requires Git 2.38 or newer".to_owned(),
                ),
            });
        }

        let head_oid = self.resolve_commit(path, "HEAD").await?;
        let output = self
            .read_allow_failure(
                Some(path),
                vec![
                    "merge-tree".into(),
                    "--write-tree".into(),
                    "--name-only".into(),
                    "-z".into(),
                    "--no-messages".into(),
                    head_oid.into(),
                    target_oid.as_str().into(),
                ],
            )
            .await?;

        let exit_code = output.status.code();
        if !matches!(exit_code, Some(0 | 1)) {
            if merge_tree_preflight_unavailable(&output) {
                return Ok(ConflictPreflightResult {
                    target: target.to_owned(),
                    target_oid,
                    state: ConflictPreflightState::Unavailable,
                    conflicting_paths: Vec::new(),
                    unavailable_reason: Some(
                        "This Git build does not support conflict preflight".to_owned(),
                    ),
                });
            }
            return Err(self.runner.failure_error(&output));
        }

        let conflicting_paths = parse_merge_tree_paths(&output.stdout)?;
        let state = match exit_code {
            Some(0) if conflicting_paths.is_empty() => ConflictPreflightState::Clean,
            Some(1) if !conflicting_paths.is_empty() => ConflictPreflightState::Conflicting,
            _ => {
                return Err(ApiError::new(
                    ErrorCode::GitCommandFailed,
                    "Git conflict preflight returned an inconsistent result",
                ));
            }
        };

        Ok(ConflictPreflightResult {
            target: target.to_owned(),
            target_oid,
            state,
            conflicting_paths,
            unavailable_reason: None,
        })
    }

    pub(super) async fn conflict_resolution_warnings_inner(
        &self,
        path: &Path,
        paths: &[String],
    ) -> ApiResult<Vec<ConflictResolutionWarning>> {
        self.resolution_warnings(path, paths).await
    }

    pub(super) async fn conflict_details_inner(
        &self,
        path: &Path,
        conflict_path: &str,
    ) -> ApiResult<ConflictFileDetails> {
        self.ensure_conflicted_path(path, conflict_path).await?;
        let entries = self.unmerged_index_entries(path, conflict_path).await?;
        let base = match entry_at_stage(&entries, 1) {
            Some(entry) => Some(self.conflict_index_version(path, entry).await?),
            None => None,
        };
        let ours = match entry_at_stage(&entries, 2) {
            Some(entry) => Some(self.conflict_index_version(path, entry).await?),
            None => None,
        };
        let theirs = match entry_at_stage(&entries, 3) {
            Some(entry) => Some(self.conflict_index_version(path, entry).await?),
            None => None,
        };
        let worktree = self
            .inspect_conflict_result_async(path, conflict_path)
            .await?;

        Ok(ConflictFileDetails {
            path: conflict_path.to_owned(),
            expected_state: conflict_expected_state(&entries, worktree.identity),
            base,
            ours,
            theirs,
            result: worktree.content,
        })
    }

    pub(super) async fn resolve_conflict_inner(
        &self,
        path: &Path,
        conflict_path: &str,
        resolution: ConflictResolution,
        expected_state: &ConflictExpectedState,
    ) -> ApiResult<MutationResult> {
        let entries = self
            .ensure_expected_conflict(path, conflict_path, expected_state)
            .await?;

        if resolution == ConflictResolution::Base {
            let Some(base) = entry_at_stage(&entries, 1) else {
                return Err(ApiError::new(
                    ErrorCode::UnsupportedOperation,
                    "This conflict has no base version",
                )
                .with_details(format!(
                    "{conflict_path}: both sides added the path, so there is no common ancestor to keep."
                )));
            };
            return self.take_index_version(path, conflict_path, base).await;
        }

        let checkout_side = match resolution {
            ConflictResolution::Ours => Some(("--ours", 2, "ours")),
            ConflictResolution::Theirs => Some(("--theirs", 3, "theirs")),
            ConflictResolution::Base
            | ConflictResolution::MarkResolved
            | ConflictResolution::Delete => None,
        };

        if let Some((checkout_flag, stage, side_name)) = checkout_side {
            // A submodule pointer has no file content to check out; staging
            // the recorded commit is the whole resolution.
            if let Some(entry) =
                entry_at_stage(&entries, stage).filter(|entry| entry.mode == GITLINK_MODE)
            {
                return self.take_index_version(path, conflict_path, entry).await;
            }
            if entry_at_stage(&entries, stage).is_none() {
                return Err(ApiError::new(
                    ErrorCode::UnsupportedOperation,
                    format!("The {side_name} side has no file content for this conflict"),
                )
                .with_details(format!(
                    "{conflict_path}: the selected index stage is absent; use the explicit delete resolution if deletion is intended."
                )));
            }

            let mut args = os_args(&["--literal-pathspecs", "checkout", checkout_flag, "--"]);
            args.push(conflict_path.into());
            self.mutate(path, args, None, CancellationToken::new(), false)
                .await?;
        }

        if resolution == ConflictResolution::Delete {
            let mut args = os_args(&[
                "--literal-pathspecs",
                "rm",
                "--force",
                "--ignore-unmatch",
                "--",
            ]);
            args.push(conflict_path.into());
            return self
                .mutate(path, args, None, CancellationToken::new(), false)
                .await;
        }

        self.stage_conflict_path(path, conflict_path).await
    }

    pub(super) async fn save_conflict_result_inner(
        &self,
        path: &Path,
        conflict_path: &str,
        text: &str,
        line_ending: ConflictLineEndingPolicy,
        expected_state: &ConflictExpectedState,
    ) -> ApiResult<MutationResult> {
        let entries = self
            .ensure_expected_conflict(path, conflict_path, expected_state)
            .await?;
        let desired_mode = [2, 3, 1]
            .into_iter()
            .filter_map(|stage| entry_at_stage(&entries, stage))
            .find(|entry| matches!(entry.mode.as_str(), "100644" | "100755"))
            .map(|entry| entry.mode.as_str())
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::UnsupportedOperation,
                    "Edited text cannot resolve a non-regular-file conflict",
                )
            })?;
        if !matches!(
            expected_state.result.kind,
            ConflictWorktreeKind::Regular | ConflictWorktreeKind::Missing
        ) {
            return Err(ApiError::new(
                ErrorCode::UnsupportedOperation,
                "Edited text cannot resolve a non-regular-file conflict",
            ));
        }
        self.write_conflict_result_async(
            path,
            conflict_path,
            text,
            line_ending,
            &expected_state.result,
            desired_mode,
        )
        .await?;
        self.stage_conflict_path(path, conflict_path).await
    }

    pub(super) async fn resolve_conflicts_inner(
        &self,
        path: &Path,
        conflict_paths: &[String],
        resolution: ConflictResolution,
    ) -> ApiResult<MutationResult> {
        validate_paths(conflict_paths)?;
        let conflicted = self.conflicted_paths(path).await?;
        let mut targets = if conflict_paths.is_empty() {
            conflicted.clone()
        } else {
            conflict_paths.to_vec()
        };
        targets.sort();
        targets.dedup();
        if targets.is_empty() {
            return self.mutation_result(path, self.head_oid(path).await?).await;
        }

        let unconflicted: Vec<&String> = targets
            .iter()
            .filter(|target| !conflicted.contains(target))
            .collect();
        if !unconflicted.is_empty() {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "Selected paths are not currently conflicted",
            )
            .with_details(summarize_paths(&unconflicted)));
        }

        if resolution == ConflictResolution::Base {
            return Err(ApiError::new(
                ErrorCode::UnsupportedOperation,
                "Keeping the base version is chosen one file at a time",
            ));
        }

        let checkout_side = match resolution {
            ConflictResolution::Ours => Some(("--ours", 2u8, "ours")),
            ConflictResolution::Theirs => Some(("--theirs", 3u8, "theirs")),
            ConflictResolution::Base
            | ConflictResolution::MarkResolved
            | ConflictResolution::Delete => None,
        };

        if let Some((checkout_flag, stage, side_name)) = checkout_side {
            let stages = self.unmerged_stages(path, &targets).await?;
            let missing: Vec<&String> = targets
                .iter()
                .filter(|target| {
                    stages.get(*target).copied().unwrap_or_default() & (1 << stage) == 0
                })
                .collect();
            if !missing.is_empty() {
                return Err(ApiError::new(
                    ErrorCode::UnsupportedOperation,
                    format!("The {side_name} side has no file content for some conflicts"),
                )
                .with_details(format!(
                    "{}: resolve these individually, or use the explicit delete resolution if deletion is intended.",
                    summarize_paths(&missing)
                )));
            }
            let before_oid = self.head_oid(path).await?;
            self.run_path_chunks(
                path,
                &["--literal-pathspecs", "checkout", checkout_flag, "--"],
                &targets,
            )
            .await?;
            self.run_path_chunks(path, &["--literal-pathspecs", "add", "--"], &targets)
                .await?;
            return self.mutation_result(path, before_oid).await;
        }

        let before_oid = self.head_oid(path).await?;
        let prefix: &[&str] = if resolution == ConflictResolution::Delete {
            &[
                "--literal-pathspecs",
                "rm",
                "--force",
                "--ignore-unmatch",
                "--",
            ]
        } else {
            &["--literal-pathspecs", "add", "--all", "--"]
        };
        self.run_path_chunks(path, prefix, &targets).await?;
        self.mutation_result(path, before_oid).await
    }

    pub(super) async fn auto_resolve_conflicts_inner(
        &self,
        path: &Path,
    ) -> ApiResult<MutationResult> {
        // `rerere` only reuses a repository-local resolution recorded for the
        // exact conflict preimage. It never chooses current or incoming content.
        self.mutate(
            path,
            os_args(&[
                "-c",
                "rerere.enabled=true",
                "-c",
                "rerere.autoupdate=true",
                "rerere",
                "--rerere-autoupdate",
            ]),
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }
}
