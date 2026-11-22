//! Listing, creating, applying and dropping stashes.

use super::*;

impl GitCliBackend {
    pub(super) async fn default_stash_message(&self, path: &Path) -> ApiResult<Option<String>> {
        let output = self
            .read_allow_failure(
                Some(path),
                os_args(&["symbolic-ref", "--quiet", "--short", "HEAD"]),
            )
            .await?;
        if !output.success() {
            return Ok(None);
        }
        let branch = output.stdout_lossy().trim().to_owned();
        Ok((!branch.is_empty()).then(|| format!("WIP on {branch}")))
    }

    pub(super) async fn stash_list_inner(&self, path: &Path) -> ApiResult<Vec<StashEntry>> {
        Ok(self.stash_graph(path).await?.entries())
    }

    pub(super) async fn stash_push_inner(
        &self,
        path: &Path,
        message: Option<&str>,
        include_untracked: bool,
    ) -> ApiResult<MutationResult> {
        if include_untracked {
            self.mutate(
                path,
                os_args(&["add", "-A", "--"]),
                None,
                CancellationToken::new(),
                false,
            )
            .await?;
        }
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
        let result = self
            .mutate(path, args, None, CancellationToken::new(), false)
            .await;
        if include_untracked && result.is_err() {
            let _ = self
                .read_allow_failure(Some(path), os_args(&["restore", "--staged", "--", "."]))
                .await;
        }
        result
    }

    pub(super) async fn stash_apply_inner(
        &self,
        path: &Path,
        oid: &str,
        pop: bool,
    ) -> ApiResult<MutationResult> {
        let index = resolve_stash_index(self.stash_list(path).await?, oid)?;
        let action = if pop { "pop" } else { "apply" };
        self.mutate(
            path,
            vec![
                "stash".into(),
                action.into(),
                format!("stash@{{{index}}}").into(),
            ],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }

    pub(super) async fn stash_drop_inner(
        &self,
        path: &Path,
        oid: &str,
        confirmed: bool,
    ) -> ApiResult<MutationResult> {
        if !confirmed {
            return Err(confirmation_required(
                "Dropping a stash requires confirmation",
            ));
        }
        let index = resolve_stash_index(self.stash_list(path).await?, oid)?;
        self.mutate(
            path,
            vec![
                "stash".into(),
                "drop".into(),
                format!("stash@{{{index}}}").into(),
            ],
            None,
            CancellationToken::new(),
            false,
        )
        .await
    }
}
