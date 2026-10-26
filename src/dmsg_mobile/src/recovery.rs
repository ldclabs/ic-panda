//! Content-free, certified recovery observations, available while locked.
use super::*;

#[uniffi::export]
impl MobileEngine {
    pub async fn recovery_warning(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.recovery_warning() }))
            .await
    }

    pub async fn refresh_recovery_warning(
        &self,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.object(|c| {
                Box::pin(async {
                    // A failed refresh leaves the last certified observation intact.
                    tokio::time::timeout(
                        std::time::Duration::from_secs(12),
                        dmsg_client::recovery_watch::refresh(&c.cooperative_hold()),
                    )
                    .await
                    .map_err(|_| Error::Unavailable("recovery refresh timed out".into()))??;
                    c.recovery_warning()
                })
            }))
            .await
    }
}
