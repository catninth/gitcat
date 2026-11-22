//! Staging, discarding and line-level working tree changes.

use super::*;

impl CoreApi {
    pub async fn stage(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.stage_paths(&path, paths).await
        })
        .await
    }

    pub async fn stage_paths(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
    ) -> ApiResult<MutationResult> {
        self.stage(repository_id, paths).await
    }

    pub async fn unstage(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.unstage_paths(&path, paths).await
        })
        .await
    }

    pub async fn unstage_paths(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
    ) -> ApiResult<MutationResult> {
        self.unstage(repository_id, paths).await
    }

    pub async fn discard(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.discard_paths(&path, paths).await
        })
        .await
    }

    pub async fn apply_diff_lines(
        &self,
        repository_id: &RepositoryId,
        request: &LinePatchRequest,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.apply_diff_lines(&path, request).await
        })
        .await
    }

    pub async fn stash_file(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
        message: Option<&str>,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.stash_paths(&path, paths, message).await
        })
        .await
    }

    pub async fn append_gitignore(
        &self,
        repository_id: &RepositoryId,
        patterns: &[String],
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.append_gitignore(&path, patterns).await
        })
        .await
    }

    pub async fn create_patch(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
        staged: bool,
    ) -> ApiResult<String> {
        let path = self.repository_path(repository_id).await?;
        self.backend.create_patch(&path, paths, staged).await
    }
}
