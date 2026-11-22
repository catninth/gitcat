//! Creating, switching, renaming, deleting and merging branches.

use super::*;

impl GitCliBackend {
    pub(super) async fn create_branch_inner(
        &self,
        path: &Path,
        name: &str,
        start_oid: &str,
        checkout: bool,
    ) -> ApiResult<MutationResult> {
        self.validate_branch_name(name).await?;
        let oid = self.resolve_commit(path, start_oid).await?;
        let args = if checkout {
            vec!["switch".into(), "-c".into(), name.into(), oid.into()]
        } else {
            vec!["branch".into(), "--".into(), name.into(), oid.into()]
        };
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    pub(super) async fn checkout_branch_inner(
        &self,
        path: &Path,
        name: &str,
    ) -> ApiResult<MutationResult> {
        self.validate_branch_name(name).await?;
        self.mutate(
            path,
            vec!["switch".into(), name.into()],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    pub(super) async fn rename_branch_inner(
        &self,
        path: &Path,
        old_name: &str,
        new_name: &str,
    ) -> ApiResult<MutationResult> {
        self.validate_branch_name(old_name).await?;
        self.validate_branch_name(new_name).await?;
        self.mutate(
            path,
            vec![
                "branch".into(),
                "-m".into(),
                old_name.into(),
                new_name.into(),
            ],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    pub(super) async fn delete_branch_inner(
        &self,
        path: &Path,
        name: &str,
        force: bool,
        confirmed: bool,
    ) -> ApiResult<MutationResult> {
        self.validate_branch_name(name).await?;
        if force && !confirmed {
            return Err(confirmation_required(
                "Force-deleting a branch requires confirmation",
            ));
        }
        let mode = if force { "-D" } else { "-d" };
        self.mutate(
            path,
            vec!["branch".into(), mode.into(), "--".into(), name.into()],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    pub(super) async fn delete_remote_branch_inner(
        &self,
        path: &Path,
        remote: &str,
        branch: &str,
        confirmed: bool,
    ) -> ApiResult<MutationResult> {
        if !confirmed {
            return Err(confirmation_required(
                "Deleting a remote branch requires confirmation",
            ));
        }
        self.validate_branch_name(branch).await?;
        if branch == "HEAD" {
            return Err(ApiError::new(
                ErrorCode::InvalidRefName,
                "The remote HEAD is not a branch",
            ));
        }
        let remotes = self.validate_remote_selection(path, Some(remote)).await?;
        // The full ref name keeps a tag of the same name out of it.
        let mut args = os_args(&["push", "--porcelain", "--delete", "--", remote]);
        args.push(format!("refs/heads/{branch}").into());
        let run = self.network_options(&remotes, Some(remote), true).await;
        self.mutate_network(path, args, CancellationToken::new(), run)
            .await
    }

    pub(super) async fn set_upstream_inner(
        &self,
        path: &Path,
        branch: &str,
        upstream: &str,
    ) -> ApiResult<MutationResult> {
        self.validate_branch_name(branch).await?;
        self.validate_branch_name(upstream).await?;
        let upstream_ref = format!("refs/remotes/{upstream}");
        let verify = self
            .read_allow_failure(
                Some(path),
                vec![
                    "show-ref".into(),
                    "--verify".into(),
                    "--quiet".into(),
                    upstream_ref.into(),
                ],
            )
            .await?;
        if !verify.success() {
            return Err(ApiError::new(
                ErrorCode::InvalidRefName,
                "Selected upstream branch does not exist",
            ));
        }
        self.mutate(
            path,
            vec![
                "branch".into(),
                format!("--set-upstream-to={upstream}").into(),
                "--".into(),
                branch.into(),
            ],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    pub(super) async fn merge_branch_inner(
        &self,
        path: &Path,
        branch: &str,
    ) -> ApiResult<MutationResult> {
        self.validate_branch_name(branch).await?;
        let oid = self.resolve_commit(path, branch).await?;
        // Naming the branch, rather than the commit it resolved to, is what
        // lets Git write "Merge branch 'x'" into the commit and into MERGE_MSG
        // instead of "Merge commit '<oid>'". Only a name that is a branch or a
        // remote-tracking branch and still resolves to the same commit is
        // passed on; anything else is merged by its object id as before.
        let mut target = oid;
        for candidate in [
            format!("refs/heads/{branch}"),
            format!("refs/remotes/{branch}"),
        ] {
            let output = self
                .read_allow_failure(
                    Some(path),
                    vec![
                        "rev-parse".into(),
                        "--verify".into(),
                        "--quiet".into(),
                        format!("{candidate}^{{commit}}").into(),
                    ],
                )
                .await?;
            // `branch` itself already resolved to this commit, so passing the
            // name Git was given produces the same merge with its own wording.
            if output.success()
                && output.stdout_lossy().trim() == target
                && !branch.starts_with('-')
            {
                target = branch.to_owned();
                break;
            }
        }
        self.mutate(
            path,
            vec!["merge".into(), "--no-edit".into(), target.into()],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }
}
