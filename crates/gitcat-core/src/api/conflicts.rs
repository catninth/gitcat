//! Conflict preflight, details and resolution.

use super::*;

impl CoreApi {
    pub async fn conflict_preflight(
        &self,
        repository_id: &RepositoryId,
        target: &str,
    ) -> ApiResult<ConflictPreflightResult> {
        let path = self.repository_path(repository_id).await?;
        self.backend.conflict_preflight(&path, target).await
    }

    pub async fn conflict_resolution_warnings(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
    ) -> ApiResult<Vec<ConflictResolutionWarning>> {
        let repository_path = self.repository_path(repository_id).await?;
        self.backend
            .conflict_resolution_warnings(&repository_path, paths)
            .await
    }

    pub async fn submodule_conflict_details(
        &self,
        repository_id: &RepositoryId,
        path: &str,
    ) -> ApiResult<SubmoduleConflictDetails> {
        let repository_path = self.repository_path(repository_id).await?;
        self.backend
            .submodule_conflict_details(&repository_path, path)
            .await
    }

    pub async fn conflict_details(
        &self,
        repository_id: &RepositoryId,
        path: &str,
    ) -> ApiResult<ConflictFileDetails> {
        let repository_path = self.repository_path(repository_id).await?;
        self.backend.conflict_details(&repository_path, path).await
    }

    pub async fn resolve_conflict(
        &self,
        repository_id: &RepositoryId,
        path: &str,
        resolution: ConflictResolution,
        expected_state: &ConflictExpectedState,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, repository_path| async move {
            backend
                .resolve_conflict(&repository_path, path, resolution, expected_state)
                .await
        })
        .await
    }

    pub async fn save_conflict_result(
        &self,
        repository_id: &RepositoryId,
        path: &str,
        text: &str,
        line_ending: ConflictLineEndingPolicy,
        expected_state: &ConflictExpectedState,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, repository_path| async move {
            backend
                .save_conflict_result(&repository_path, path, text, line_ending, expected_state)
                .await
        })
        .await
    }

    pub async fn resolve_conflicts(
        &self,
        repository_id: &RepositoryId,
        paths: &[String],
        resolution: ConflictResolution,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, repository_path| async move {
            backend
                .resolve_conflicts(&repository_path, paths, resolution)
                .await
        })
        .await
    }

    pub async fn auto_resolve_conflicts(
        &self,
        repository_id: &RepositoryId,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.auto_resolve_conflicts(&path).await
        })
        .await
    }
}
