//! Continuing, aborting and skipping an operation Git has stopped in.

use super::*;

impl GitCliBackend {
    pub(super) async fn continue_operation_inner(
        &self,
        path: &Path,
        operation: ContinueOperation,
        message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        let git_dir = self.git_dir(path).await?;
        let state = operation_state_from_git_dir(&git_dir);
        ensure_operation(state, operation)?;
        if let Some(message) = message.map(str::trim).filter(|message| !message.is_empty()) {
            validate_message(message)?;
            // The apply backend rebuilds its message from the patch, so only
            // files Git reads back on `--continue` are written.
            let file =
                operation_message_file(&git_dir, state).filter(|file| !file.ends_with("msg-clean"));
            if let Some(file) = file {
                fs::write(&file, format!("{message}\n")).map_err(|error| {
                    ApiError::new(ErrorCode::Io, "Commit message could not be written")
                        .with_details(error.to_string())
                })?;
            }
        }
        let args = match operation {
            ContinueOperation::Merge => os_args(&["merge", "--continue"]),
            ContinueOperation::Rebase => os_args(&["rebase", "--continue"]),
            ContinueOperation::CherryPick => os_args(&["cherry-pick", "--continue"]),
            ContinueOperation::Revert => os_args(&["revert", "--continue"]),
        };
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    pub(super) async fn abort_operation_inner(
        &self,
        path: &Path,
        operation: ContinueOperation,
    ) -> ApiResult<MutationResult> {
        ensure_operation(self.operation_state(path).await?, operation)?;
        let args = match operation {
            ContinueOperation::Merge => os_args(&["merge", "--abort"]),
            ContinueOperation::Rebase => os_args(&["rebase", "--abort"]),
            ContinueOperation::CherryPick => os_args(&["cherry-pick", "--abort"]),
            ContinueOperation::Revert => os_args(&["revert", "--abort"]),
        };
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }

    pub(super) async fn skip_operation_inner(
        &self,
        path: &Path,
        operation: ContinueOperation,
    ) -> ApiResult<MutationResult> {
        ensure_operation(self.operation_state(path).await?, operation)?;
        let args = match operation {
            ContinueOperation::Merge => {
                return Err(ApiError::new(
                    ErrorCode::UnsupportedOperation,
                    "A merge cannot skip a commit; resolve the conflicts or abort the merge",
                ));
            }
            ContinueOperation::Rebase => os_args(&["rebase", "--skip"]),
            ContinueOperation::CherryPick => os_args(&["cherry-pick", "--skip"]),
            ContinueOperation::Revert => os_args(&["revert", "--skip"]),
        };
        self.mutate(path, args, None, CancellationToken::new(), false)
            .await
    }
}
