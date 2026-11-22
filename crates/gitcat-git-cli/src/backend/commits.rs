//! Creating, rewording and acting on commits.

use super::*;

impl GitCliBackend {
    pub(super) async fn short_oid(&self, path: &Path, oid: &str) -> ApiResult<String> {
        let output = self
            .read(
                Some(path),
                vec![
                    "rev-parse".into(),
                    "--short".into(),
                    "--verify".into(),
                    "--end-of-options".into(),
                    oid.into(),
                ],
            )
            .await?;
        Ok(output.stdout_lossy().trim().to_owned())
    }

    pub(super) async fn resolve_commit(&self, path: &Path, revision: &str) -> ApiResult<String> {
        if revision.is_empty() || revision.contains(['\0', '\n', '\r']) {
            return Err(ApiError::new(
                ErrorCode::InvalidRevision,
                "Git revision is empty or malformed",
            ));
        }
        let expression = format!("{revision}^{{commit}}");
        let output = self
            .read(
                Some(path),
                vec![
                    "rev-parse".into(),
                    "--verify".into(),
                    "--end-of-options".into(),
                    expression.into(),
                ],
            )
            .await
            .map_err(|error| {
                if matches!(
                    error.code,
                    ErrorCode::GitCommandFailed | ErrorCode::InvalidRevision
                ) {
                    ApiError::new(
                        ErrorCode::InvalidRevision,
                        "Git revision could not be resolved",
                    )
                } else {
                    error
                }
            })?;
        let oid = output.stdout_lossy().trim().to_owned();
        if !is_full_oid(&oid) {
            return Err(ApiError::new(
                ErrorCode::InvalidRevision,
                "Git did not resolve the revision to a full object ID",
            ));
        }
        Ok(oid)
    }

    pub(super) async fn commit_parent_oids(
        &self,
        path: &Path,
        oid: &str,
    ) -> ApiResult<Vec<String>> {
        let output = self
            .read(
                Some(path),
                vec![
                    "show".into(),
                    "-s".into(),
                    "--format=%P".into(),
                    oid.into(),
                    "--".into(),
                ],
            )
            .await?;
        Ok(output
            .stdout_lossy()
            .split_whitespace()
            .map(str::to_owned)
            .collect())
    }

    pub(super) async fn is_ancestor(
        &self,
        path: &Path,
        ancestor: &str,
        descendant: &str,
    ) -> ApiResult<bool> {
        let output = self
            .read_allow_failure(
                Some(path),
                vec![
                    "merge-base".into(),
                    "--is-ancestor".into(),
                    ancestor.into(),
                    descendant.into(),
                ],
            )
            .await?;
        Ok(output.success())
    }

    pub(super) async fn range_has_merge(
        &self,
        path: &Path,
        base: &str,
        head: &str,
    ) -> ApiResult<bool> {
        // Merges strictly after `base` would be linearized by a plain rebase,
        // so callers reject rewording when any exist.
        let output = self
            .read(
                Some(path),
                vec![
                    "rev-list".into(),
                    "--merges".into(),
                    "--max-count=1".into(),
                    format!("{base}..{head}").into(),
                    "--".into(),
                ],
            )
            .await?;
        Ok(!output.stdout_lossy().trim().is_empty())
    }

    pub(super) async fn read_commit_author(
        &self,
        path: &Path,
        oid: &str,
    ) -> ApiResult<CommitAuthor> {
        let output = self
            .read(
                Some(path),
                vec![
                    "show".into(),
                    "-s".into(),
                    "--date=raw".into(),
                    "--format=%an%x00%ae%x00%ad".into(),
                    oid.into(),
                    "--".into(),
                ],
            )
            .await?;
        let raw = output.stdout_lossy();
        let mut parts = raw.trim_end_matches(['\n', '\r']).splitn(3, '\0');
        Ok(CommitAuthor {
            name: parts.next().unwrap_or_default().to_owned(),
            email: parts.next().unwrap_or_default().to_owned(),
            date: parts.next().unwrap_or_default().to_owned(),
        })
    }

    /// Rebuilds `oid` with `message`, preserving its tree, parents and
    /// authorship, and returns the new (dangling) commit object id.
    pub(super) async fn rebuild_commit_message(
        &self,
        path: &Path,
        oid: &str,
        message: &str,
    ) -> ApiResult<String> {
        let tree = self
            .read(
                Some(path),
                vec![
                    "rev-parse".into(),
                    "--verify".into(),
                    "--end-of-options".into(),
                    format!("{oid}^{{tree}}").into(),
                ],
            )
            .await?
            .stdout_lossy()
            .trim()
            .to_owned();
        if !is_full_oid(&tree) {
            return Err(ApiError::new(
                ErrorCode::GitCommandFailed,
                "Git did not resolve the commit tree",
            ));
        }
        let parents = self.commit_parent_oids(path, oid).await?;
        let author = self.read_commit_author(path, oid).await?;

        let mut args: Vec<OsString> = vec!["commit-tree".into(), tree.into()];
        for parent in parents {
            args.push("-p".into());
            args.push(parent.into());
        }
        args.push("-F".into());
        args.push("-".into());

        let mut options = GitRunOptions::mutation(READ_OUTPUT_CAP);
        options.extra_env = author.into_env();
        let output = self
            .runner
            .run(
                Some(path),
                &args,
                Some(message.as_bytes()),
                CancellationToken::new(),
                options,
            )
            .await?;
        let new_oid = output.stdout_lossy().trim().to_owned();
        if !is_full_oid(&new_oid) {
            return Err(ApiError::new(
                ErrorCode::GitCommandFailed,
                "Git did not return a rewritten commit object",
            ));
        }
        Ok(new_oid)
    }

    /// Puts a `README.md` in an empty repository and answers with its path.
    ///
    /// A file that is already there is left exactly as it is: this seeds a
    /// starting point for a repository that has none, it never overwrites what
    /// the user wrote. The heading is the repository folder's own name, which
    /// is what the hosting services fill a new README with.
    pub(super) async fn write_initial_readme(&self, path: &Path) -> ApiResult<String> {
        let root = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let target = root.join(INITIAL_README);
            if target.exists() {
                return Ok(INITIAL_README.to_owned());
            }
            let name = root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Repository");
            fs::write(&target, format!("# {name}\n")).map_err(|error| {
                ApiError::new(ErrorCode::Io, "The initial README could not be written")
                    .with_details(error.to_string())
            })?;
            Ok(INITIAL_README.to_owned())
        })
        .await
        .map_err(|error| {
            ApiError::new(
                ErrorCode::Internal,
                "Writing the initial README did not finish",
            )
            .with_details(error.to_string())
        })?
    }

    pub(super) async fn create_commit_inner(
        &self,
        path: &Path,
        options: &CommitOptions,
    ) -> ApiResult<MutationResult> {
        validate_message(&options.message)?;
        let mut args = os_args(&["commit", "-F", "-"]);
        if options.amend {
            args.push("--amend".into());
        }
        if options.signoff {
            args.push("--signoff".into());
        }
        self.mutate(
            path,
            args,
            Some(options.message.as_bytes()),
            CancellationToken::new(),
            false,
        )
        .await
    }

    pub(super) async fn create_initial_commit_inner(
        &self,
        path: &Path,
        message: &str,
    ) -> ApiResult<MutationResult> {
        validate_message(message)?;
        if self.head_oid(path).await?.is_some() {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "The repository already has a commit",
            ));
        }
        let (_, refs, parsed) = self.generation_and_refs(path).await?;
        // A remote that already has branches is a history to check out, not
        // an empty repository: a root commit made here would share nothing
        // with it, and the first push would be refused or diverge.
        if refs
            .split(|byte| *byte == b'\n')
            .any(|line| line.starts_with(b"refs/remotes/"))
        {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "The remote already has commits; check out one of its branches instead",
            ));
        }
        // Whatever the user staged is the commit they meant to make; the README
        // is only for the repository that has nothing staged at all.
        let staged = parsed
            .status
            .entries
            .iter()
            .any(|entry| entry.index.is_some() && !entry.conflicted);
        if !staged {
            let readme = self.write_initial_readme(path).await?;
            let mut args = os_args(&["add", "--"]);
            args.push(OsString::from(readme));
            self.mutate(path, args, None, CancellationToken::new(), false)
                .await?;
        }
        self.mutate(
            path,
            os_args(&["commit", "-F", "-"]),
            Some(message.as_bytes()),
            CancellationToken::new(),
            false,
        )
        .await
    }

    pub(super) async fn reword_commit_inner(
        &self,
        path: &Path,
        oid: &str,
        message: &str,
    ) -> ApiResult<MutationResult> {
        validate_message(message)?;
        let target = self.resolve_commit(path, oid).await?;

        // Rewriting is only defined while no merge/rebase/cherry-pick is in flight.
        if self.operation_state(path).await? != RepositoryOperationState::Normal {
            return Err(ApiError::new(
                ErrorCode::OperationInProgress,
                "Finish the in-progress Git operation before editing a commit message",
            ));
        }

        let head = self.head_oid(path).await?.ok_or_else(|| {
            ApiError::new(
                ErrorCode::InvalidRequest,
                "The repository has no commit to edit",
            )
        })?;

        // Amending HEAD needs no replay and never touches the working tree.
        if target == head {
            return self
                .mutate(
                    path,
                    os_args(&["commit", "--amend", "--only", "-F", "-"]),
                    Some(message.as_bytes()),
                    CancellationToken::new(),
                    false,
                )
                .await;
        }

        // Older commits are reworded by rebuilding the commit object and
        // replaying its descendants onto it. The tree is byte-identical, so the
        // replay never conflicts; these guards keep it from silently losing
        // history it cannot faithfully reproduce.
        if !self.is_ancestor(path, &target, &head).await? {
            return Err(ApiError::new(
                ErrorCode::UnsupportedOperation,
                "Only commits reachable from the current branch can be edited",
            ));
        }
        if self.range_has_merge(path, &target, &head).await? {
            return Err(ApiError::new(
                ErrorCode::UnsupportedOperation,
                "Cannot edit this message because a later commit is a merge",
            ));
        }

        let rebuilt = self.rebuild_commit_message(path, &target, message).await?;
        let mut args = os_args(&["rebase", "--autostash", "--onto"]);
        args.push(rebuilt.into());
        args.push(target.into());
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    pub(super) async fn checkout_commit_inner(
        &self,
        path: &Path,
        oid: &str,
    ) -> ApiResult<MutationResult> {
        let oid = self.resolve_commit(path, oid).await?;
        self.mutate(
            path,
            vec!["switch".into(), "--detach".into(), oid.into()],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    pub(super) async fn create_tag_inner(
        &self,
        path: &Path,
        name: &str,
        oid: &str,
        message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        self.validate_tag_name(name).await?;
        let oid = self.resolve_commit(path, oid).await?;
        let (args, stdin) = if let Some(message) = message {
            validate_message(message)?;
            (
                vec![
                    "tag".into(),
                    "-a".into(),
                    "-F".into(),
                    "-".into(),
                    name.into(),
                    oid.into(),
                ],
                Some(message.as_bytes()),
            )
        } else {
            (
                vec!["tag".into(), "--".into(), name.into(), oid.into()],
                None,
            )
        };
        self.mutate(path, args, stdin, CancellationToken::new(), false)
            .await
    }

    pub(super) async fn cherry_pick_inner(
        &self,
        path: &Path,
        oid: &str,
        mainline_parent: Option<u32>,
        commit: bool,
    ) -> ApiResult<MutationResult> {
        let oid = self.resolve_commit(path, oid).await?;
        let parents = self.commit_parent_oids(path, &oid).await?;
        validate_mainline_parent(parents.len(), mainline_parent)?;
        let mut args = os_args(&["cherry-pick"]);
        if !commit {
            args.push("--no-commit".into());
        }
        if let Some(parent) = mainline_parent {
            args.extend(["-m".into(), parent.to_string().into()]);
        }
        args.push(oid.into());
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    pub(super) async fn revert_commit_inner(
        &self,
        path: &Path,
        oid: &str,
        mainline_parent: Option<u32>,
        commit: bool,
    ) -> ApiResult<MutationResult> {
        let oid = self.resolve_commit(path, oid).await?;
        let parents = self.commit_parent_oids(path, &oid).await?;
        validate_mainline_parent(parents.len(), mainline_parent)?;
        let mut args = os_args(&["revert", "--no-edit"]);
        if !commit {
            args.push("--no-commit".into());
        }
        if let Some(parent) = mainline_parent {
            args.extend(["-m".into(), parent.to_string().into()]);
        }
        args.push(oid.into());
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    pub(super) async fn reset_to_commit_inner(
        &self,
        path: &Path,
        oid: &str,
        mode: ResetMode,
        confirmed: bool,
    ) -> ApiResult<MutationResult> {
        if mode == ResetMode::Hard && !confirmed {
            return Err(confirmation_required("Hard reset requires confirmation"));
        }
        let snapshot = self.snapshot(path).await?;
        if snapshot.operation_state != RepositoryOperationState::Normal {
            return Err(ApiError::new(
                ErrorCode::OperationInProgress,
                "Finish or abort the current Git operation before resetting",
            ));
        }
        if !matches!(snapshot.head, HeadState::Branch { .. }) {
            return Err(ApiError::new(
                ErrorCode::UnsupportedOperation,
                "Reset is only available while a local branch is checked out",
            ));
        }
        let oid = self.resolve_commit(path, oid).await?;
        let flag = match mode {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
        };
        self.mutate(
            path,
            vec!["reset".into(), flag.into(), oid.into()],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    pub(super) async fn commit_action_availability_inner(
        &self,
        path: &Path,
        oid: &str,
    ) -> ApiResult<Vec<CommitActionAvailability>> {
        let oid = self.resolve_commit(path, oid).await?;
        let (status_output, operation_state) =
            tokio::try_join!(self.status_output(path), self.operation_state(path))?;
        let parsed_status = parse_status(&status_output.stdout)?;
        let operation_busy = operation_state != RepositoryOperationState::Normal;
        let dirty = !parsed_status.status.clean;
        let reset_unavailable = !matches!(parsed_status.head, HeadState::Branch { .. });
        let head_oid = match &parsed_status.head {
            HeadState::Branch { oid, .. } | HeadState::Detached { oid } => Some(oid.as_str()),
            HeadState::Unborn { .. } => None,
        };
        let target_in_head_history = if let Some(head_oid) = head_oid {
            Some(self.is_ancestor(path, &oid, head_oid).await?)
        } else {
            None
        };
        let cherry_pick_unavailable = match target_in_head_history {
            Some(true) => Some("Commit is already in the current HEAD history"),
            Some(false) => None,
            None => Some("Check out a commit before cherry-picking"),
        };
        let revert_unavailable = match target_in_head_history {
            Some(true) => None,
            Some(false) => Some("Commit is not in the current HEAD history"),
            None => Some("Check out a commit before reverting"),
        };
        // Rewording replays the descendants of the target, so it needs the same
        // guards `reword_commit` itself enforces: reachable from HEAD and no
        // merge in the replayed range.
        let reword_unavailable = match (head_oid, target_in_head_history) {
            (Some(head_oid), Some(true)) => {
                if head_oid == oid.as_str() {
                    None
                } else if self.range_has_merge(path, &oid, head_oid).await? {
                    Some("A later commit is a merge, so this message cannot be edited")
                } else {
                    None
                }
            }
            (_, Some(false)) => {
                Some("Only commits reachable from the current branch can be edited")
            }
            _ => Some("Check out a commit before editing its message"),
        };
        let action = |kind, requires_clean, requires_confirmation, unavailable: Option<&str>| {
            let disabled_reason = if operation_busy {
                Some("Finish or abort the current Git operation first".to_owned())
            } else if kind == CommitActionKind::Reset && reset_unavailable {
                Some("Check out a local branch before resetting".to_owned())
            } else if let Some(reason) = unavailable {
                Some(reason.to_owned())
            } else if requires_clean && dirty {
                Some("Working tree must be clean for this action".to_owned())
            } else {
                None
            };
            CommitActionAvailability {
                kind,
                enabled: disabled_reason.is_none(),
                disabled_reason,
                requires_confirmation,
            }
        };
        Ok(vec![
            action(CommitActionKind::Checkout, true, false, None),
            action(CommitActionKind::CreateBranch, false, false, None),
            action(
                CommitActionKind::CherryPick,
                true,
                false,
                cherry_pick_unavailable,
            ),
            action(CommitActionKind::Revert, true, false, revert_unavailable),
            action(CommitActionKind::Reset, false, true, None),
            action(CommitActionKind::CreateTag, false, false, None),
            action(CommitActionKind::Reword, false, false, reword_unavailable),
            CommitActionAvailability {
                kind: CommitActionKind::CopySha,
                enabled: true,
                disabled_reason: None,
                requires_confirmation: false,
            },
        ])
    }
}
