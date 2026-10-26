use super::*;
use dmsg_client::library::background;

#[uniffi::export]
impl MobileEngine {
    pub async fn background_backup(
        &self,
        file: String,
        version: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move { background::upload(h, &file, &version).await })
            }))
            .await
    }

    pub async fn background_download(
        &self,
        upload: String,
        folder: Option<String>,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move { background::download(h, &upload, folder).await })
            }))
            .await
    }

    pub async fn background_upload_part(&self, id: String, slot: String) -> Result<Vec<u8>> {
        self.host
            .run(move |c| Box::pin(async move { c.background_upload_part(&id, &slot) }))
            .await
            .map_err(Into::into)
    }

    pub async fn background_accept_part(
        &self,
        id: String,
        slot: String,
        bytes: Vec<u8>,
    ) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.background_accept_part(&id, &slot, &bytes) }))
            .await
            .map_err(Into::into)
    }

    pub async fn background_finish(
        &self,
        id: String,
        operation: Arc<OperationToken>,
    ) -> Result<()> {
        operation
            .run(self.held(move |h| Box::pin(async move { background::finish(h, &id).await })))
            .await
    }

    pub async fn background_revoke(&self, id: String) -> Result<()> {
        self.held(move |h| Box::pin(async move { background::revoke(h, &id).await }))
            .await
    }
}
