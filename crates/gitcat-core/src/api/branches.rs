//! Branch creation, checkout, renaming, deletion and merging.

use super::*;

impl CoreApi {
    pub async fn create_branch(
        &self,
        repository_id: &RepositoryId,
        name: &str,
        start_oid: &str,
        checkout: bool,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend
                .create_branch(&path, name, start_oid, checkout)
                .await
        })
        .await
    }

    pub async fn checkout_branch(
        &self,
        repository_id: &RepositoryId,
        name: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.checkout_branch(&path, name).await
        })
        .await
    }

    pub async fn rename_branch(
        &self,
        repository_id: &RepositoryId,
        old_name: &str,
        new_name: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.rename_branch(&path, old_name, new_name).await
        })
        .await
    }

    pub async fn delete_branch(
        &self,
        repository_id: &RepositoryId,
        name: &str,
        force: bool,
        confirmed: bool,
        expected: &ExpectedState,
    ) -> ApiResult<MutationResult> {
        if force && !confirmed {
            return Err(ApiError::new(
                ErrorCode::ProtectedOperation,
                "Forced branch deletion requires explicit confirmation",
            ));
        }
        self.mutate_expected(repository_id, expected, |backend, path| async move {
            backend.delete_branch(&path, name, force, confirmed).await
        })
        .await
    }

    pub async fn delete_remote_branch(
        &self,
        repository_id: &RepositoryId,
        remote: &str,
        branch: &str,
        confirmed: bool,
    ) -> ApiResult<MutationResult> {
        if !confirmed {
            return Err(ApiError::new(
                ErrorCode::ProtectedOperation,
                "Deleting a remote branch requires explicit confirmation",
            ));
        }
        self.mutate(repository_id, |backend, path| async move {
            backend
                .delete_remote_branch(&path, remote, branch, confirmed)
                .await
        })
        .await
    }

    pub async fn set_upstream(
        &self,
        repository_id: &RepositoryId,
        branch: &str,
        upstream: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.set_upstream(&path, branch, upstream).await
        })
        .await
    }

    pub async fn merge_branch(
        &self,
        repository_id: &RepositoryId,
        branch: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.merge_branch(&path, branch).await
        })
        .await
    }
}
