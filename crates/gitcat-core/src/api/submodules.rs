//! Submodule operations, which also lock the child repository.

use super::*;

impl CoreApi {
    pub async fn list_submodules(
        &self,
        repository_id: &RepositoryId,
    ) -> ApiResult<Vec<SubmoduleInfo>> {
        let path = self.repository_path(repository_id).await?;
        self.backend.list_submodules(&path).await
    }

    pub async fn add_submodule(
        &self,
        repository_id: &RepositoryId,
        url: &str,
        path: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate_submodules(repository_id, |backend, root| async move {
            backend.add_submodule(&root, url, path).await
        })
        .await
    }

    pub async fn set_submodule_url(
        &self,
        repository_id: &RepositoryId,
        path: &str,
        url: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate_submodules(repository_id, |backend, root| async move {
            backend.set_submodule_url(&root, path, url).await
        })
        .await
    }

    pub async fn update_submodule(
        &self,
        repository_id: &RepositoryId,
        path: &str,
    ) -> ApiResult<MutationResult> {
        self.mutate_submodules(repository_id, |backend, root| async move {
            backend.update_submodule(&root, path).await
        })
        .await
    }

    pub async fn remove_submodule(
        &self,
        repository_id: &RepositoryId,
        path: &str,
        confirmed: bool,
    ) -> ApiResult<MutationResult> {
        self.mutate_submodules(repository_id, |backend, root| async move {
            backend.remove_submodule(&root, path, confirmed).await
        })
        .await
    }

    pub async fn sync_submodule(
        &self,
        repository_id: &RepositoryId,
        path: &str,
        action: SubmoduleSyncAction,
    ) -> ApiResult<MutationResult> {
        self.mutate_submodules(repository_id, |backend, root| async move {
            backend.sync_submodule(&root, path, action).await
        })
        .await
    }
}
