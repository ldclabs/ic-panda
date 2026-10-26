use super::*;

#[uniffi::export]
impl MobileEngine {
    pub async fn browser_pairings(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { dmsg_client::connect::pairings(h).await }))
            .await
    }

    pub async fn browser_invitations(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.connect_invitations() }))
            .await
    }

    pub async fn invite_browser(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { dmsg_client::connect::invite(h).await }))
            .await
    }

    pub async fn browser_candidates(&self, invite: String) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move { dmsg_client::connect::candidates(h, &invite).await })
        })
        .await
    }

    pub async fn grant_browser(&self, invite: String, pairing: String) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move { dmsg_client::connect::grant(h, &invite, &pairing).await })
        })
        .await
    }

    pub async fn revoke_browser(&self, pairing: String) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move { dmsg_client::connect::revoke(h, &pairing).await })
        })
        .await
    }

    pub async fn browser_request_approval(&self, request: String) -> Result<Option<String>> {
        self.host
            .run(move |c| Box::pin(async move { c.connect_approval(&request) }))
            .await
            .map_err(Into::into)
    }
}
