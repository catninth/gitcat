//! Commits and continuing, aborting or skipping a stopped operation.

use super::*;

impl CoreApi {
    pub async fn commit(
        &self,
        repository_id: &RepositoryId,
        options: &CommitOptions,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.create_commit(&path, options).await
        })
        .await
    }

    pub async fn create_commit(
        &self,
        repository_id: &RepositoryId,
        options: &CommitOptions,
    ) -> ApiResult<MutationResult> {
        self.commit(repository_id, options).await
    }

    /// Composes the first commit for a repository that has none.
    ///
    /// The empty repository is the one case where GitCat can offer a commit
    /// rather than wait for one: see [`GitBackend::create_initial_commit`].
    pub async fn create_initial_commit(
        &self,
        repository_id: &RepositoryId,
        message: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.create_initial_commit(&path, message).await
        })
        .await
    }

    pub async fn reword_commit(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
        message: &str,
        expected: &ExpectedState,
    ) -> ApiResult<MutationResult> {
        self.mutate_expected(repository_id, expected, |backend, path| async move {
            backend.reword_commit(&path, oid, message).await
        })
        .await
    }

    pub async fn checkout_commit(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.checkout_commit(&path, oid).await
        })
        .await
    }

    pub async fn create_tag(
        &self,
        repository_id: &RepositoryId,
        name: &str,
        oid: &str,
        message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.create_tag(&path, name, oid, message).await
        })
        .await
    }

    pub async fn cherry_pick(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
        mainline_parent: Option<u32>,
        commit: bool,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend
                .cherry_pick(&path, oid, mainline_parent, commit)
                .await
        })
        .await
    }

    pub async fn revert_commit(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
        mainline_parent: Option<u32>,
        commit: bool,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend
                .revert_commit(&path, oid, mainline_parent, commit)
                .await
        })
        .await
    }

    pub async fn reset_to_commit(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
        mode: ResetMode,
        confirmed: bool,
        expected: &ExpectedState,
    ) -> ApiResult<MutationResult> {
        if matches!(mode, ResetMode::Hard) && !confirmed {
            return Err(ApiError::new(
                ErrorCode::ProtectedOperation,
                "Hard reset requires explicit confirmation",
            ));
        }
        self.mutate_expected(repository_id, expected, |backend, path| async move {
            backend.reset_to_commit(&path, oid, mode, confirmed).await
        })
        .await
    }

    pub async fn commit_action_availability(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
    ) -> ApiResult<Vec<CommitActionAvailability>> {
        let path = self.repository_path(repository_id).await?;
        self.backend.commit_action_availability(&path, oid).await
    }

    pub async fn continue_operation(
        &self,
        repository_id: &RepositoryId,
        operation: ContinueOperation,
        message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.continue_operation(&path, operation, message).await
        })
        .await
    }

    pub async fn abort_operation(
        &self,
        repository_id: &RepositoryId,
        operation: ContinueOperation,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.abort_operation(&path, operation).await
        })
        .await
    }

    pub async fn skip_operation(
        &self,
        repository_id: &RepositoryId,
        operation: ContinueOperation,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.skip_operation(&path, operation).await
        })
        .await
    }
}
