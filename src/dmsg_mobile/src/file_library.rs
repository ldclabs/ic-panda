use crate::*;
use dmsg_client::library::{backup, cloud};

#[uniffi::export]
impl MobileEngine {
    /// The current account's backup limit and remaining cloud capacity.
    pub async fn file_backup_capacity(
        &self,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(|h| Box::pin(async { cloud::cloud_usage(h).await })))
            .await
    }

    /// Keep the catalog entry and verified cloud copy, removing local chunks only.
    pub async fn evict_file(
        &self,
        id: String,
        version: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    // Reconcile deleted backups before allowing the local copy to go.
                    cloud::cloud_refresh(h).await?;
                    h.with(|c| c.files_evict(&id, Some(&version))).await
                })
            }))
            .await
    }

    pub async fn retry_backup(&self, id: String, version: String) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.files_backup_retry(Some(&id), Some(&version)) }))
            .await?;
        Ok(())
    }

    pub async fn retry_download(&self, upload: String) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.files_download_retry(Some(&upload)) }))
            .await?;
        Ok(())
    }

    pub async fn cancel_backup(
        &self,
        id: String,
        version: String,
        operation: Arc<OperationToken>,
    ) -> Result<()> {
        operation
            .run(self.held(move |h| {
                Box::pin(async move { backup::backup_cancel(h, &id, &version).await })
            }))
            .await
    }
}
