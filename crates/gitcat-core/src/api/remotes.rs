//! Remotes and fetch, pull and push.

use super::*;

impl CoreApi {
    pub async fn add_remote(
        &self,
        repository_id: &RepositoryId,
        name: &str,
        url: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.add_remote(&path, name, url).await
        })
        .await
    }

    pub async fn fetch(
        &self,
        repository_id: &RepositoryId,
        options: &FetchOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.fetch(&path, options, cancellation).await
        })
        .await
    }

    pub async fn pull(
        &self,
        repository_id: &RepositoryId,
        options: &PullOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        self.pull_with_progress(repository_id, options, cancellation, Arc::new(|_| {}))
            .await
    }

    pub async fn pull_with_progress(
        &self,
        repository_id: &RepositoryId,
        options: &PullOptions,
        cancellation: CancellationToken,
        progress: PullProgress,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend
                .pull_with_progress(&path, options, cancellation, progress)
                .await
        })
        .await
    }

    pub async fn push(
        &self,
        repository_id: &RepositoryId,
        options: &PushOptions,
        cancellation: CancellationToken,
    ) -> ApiResult<MutationResult> {
        self.mutate(repository_id, |backend, path| async move {
            backend.push(&path, options, cancellation).await
        })
        .await
    }
}
