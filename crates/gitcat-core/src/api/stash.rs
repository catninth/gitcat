//! Stash list and stash mutations.

use super::*;

impl CoreApi {
    pub async fn stash_list(&self, repository_id: &RepositoryId) -> ApiResult<Vec<StashEntry>> {
        let path = self.repository_path(repository_id).await?;
        self.backend.stash_list(&path).await
    }

    pub async fn stash_push(
        &self,
        repository_id: &RepositoryId,
        message: Option<&str>,
        include_untracked: bool,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.stash_push(&path, message, include_untracked).await
        })
        .await
    }

    pub async fn stash_apply(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
        pop: bool,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.stash_apply(&path, oid, pop).await
        })
        .await
    }

    pub async fn stash_drop(
        &self,
        repository_id: &RepositoryId,
        oid: &str,
        confirmed: bool,
        expected: &ExpectedState,
    ) -> ApiResult<MutationResult> {
        if !confirmed {
            return Err(ApiError::new(
                ErrorCode::ProtectedOperation,
                "Dropping a stash requires explicit confirmation",
            ));
        }
        self.mutate_expected(repository_id, expected, |backend, path| async move {
            backend.stash_drop(&path, oid, confirmed).await
        })
        .await
    }
}
