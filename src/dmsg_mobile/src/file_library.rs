use crate::*;
use dmsg_client::library::{
    backup, cloud,
    link::{self, LinkTerms},
};

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

    /// Share a library version by link; one freed from this device comes down
    /// from its backup only long enough to be re-encrypted.
    pub async fn create_file_link(
        &self,
        id: String,
        version: String,
        expires_in_ms: u64,
        max_downloads: u64,
        password: Option<String>,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let terms = LinkTerms {
                        expires_in_ms: Some(expires_in_ms),
                        max_downloads,
                        password: password.as_deref(),
                    };
                    link::share_link(h, &id, Some(&version), terms, &mut |_, _, _| {}).await
                })
            }))
            .await
    }

    /// Share a cloud backup by link; the library does not change.
    pub async fn create_backup_link(
        &self,
        upload: String,
        expires_in_ms: u64,
        max_downloads: u64,
        password: Option<String>,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let terms = LinkTerms {
                        expires_in_ms: Some(expires_in_ms),
                        max_downloads,
                        password: password.as_deref(),
                    };
                    link::share_backup_link(h, &upload, terms, &mut |_, _, _| {}).await
                })
            }))
            .await
    }

    /// The account's share links as saved here; `refresh` reads the relay's
    /// counts and the links other devices made first.
    pub async fn file_links(
        &self,
        refresh: bool,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    if refresh {
                        link::share_links_refresh(h).await
                    } else {
                        h.with(|c| c.files_share_links()).await
                    }
                })
            }))
            .await
    }

    pub async fn revoke_file_link(
        &self,
        share_id: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move { link::share_revoke(h, &share_id).await })
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
