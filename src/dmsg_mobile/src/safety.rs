use super::*;
use serde_json::{Value, json};

#[derive(uniffi::Record)]
pub struct ReportInput {
    pub kind: String,
    pub target: String,
    pub resource: String,
    pub sequence: Option<u64>,
    pub reason: String,
    pub note: String,
    pub disclose: bool,
}

#[uniffi::export]
impl MobileEngine {
    pub async fn blocked_accounts(&self) -> Result<Vec<String>> {
        self.host
            .run(|c| Box::pin(async { c.blocked_accounts() }))
            .await
            .map_err(Into::into)
    }

    pub async fn set_blocked_account(
        &self,
        account: String,
        blocked: bool,
    ) -> Result<ObjectSnapshot> {
        self.host
            .run(move |c| Box::pin(async move { c.set_blocked_account(&account, blocked) }))
            .await?;
        self.sync_blocked_accounts().await
    }

    pub async fn sync_blocked_accounts(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { dmsg_client::safety::sync_blocks(h).await }))
            .await
    }

    pub async fn prepare_report(&self, input: ReportInput) -> Result<ObjectSnapshot> {
        let ReportInput {
            kind,
            target,
            resource,
            sequence,
            reason,
            note,
            disclose,
        } = input;
        self.object(move |c| {
            Box::pin(async move {
                c.prepare_content_report(
                    &kind, &target, &resource, sequence, &reason, &note, disclose,
                )
            })
        })
        .await
    }

    pub async fn submit_report(
        &self,
        id: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move { dmsg_client::safety::submit_report(h, &id).await })
            }))
            .await
    }

    pub async fn report_history(&self, refresh: bool) -> Result<ObjectSnapshot> {
        if refresh {
            self.mobile_read("reports".into()).await
        } else {
            self.object(|c| Box::pin(async { c.content_reports() }))
                .await
        }
    }

    pub async fn account_deletion_status(&self) -> Result<ObjectSnapshot> {
        self.mobile_read("status".into()).await
    }

    pub async fn account_deletion_preview(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async {
            dmsg_client::sync::pull_metadata(h).await?;
            let (wallets, wallets_available) = match dmsg_client::wallet::list(h).await {
                Ok(wallets) => (wallets, true),
                Err(error @ (Error::Locked | Error::AuthRequired | Error::DeviceNotApproved)) => return Err(error),
                Err(_) => (json!([]), false),
            };
            let (account, channels) = h.with(|c| Ok((c.session_state().account.ok_or(Error::AuthRequired)?, c.channel_list()?))).await?;
            let mut owned = Vec::new();
            for channel in channels {
                match dmsg_client::channel::pull(h, &channel.id, None, None).await {
                    Ok(ledger) if ledger.owner == account && ledger.status != dmsg_client::channel_log::Status::Archived => {
                        owned.push(json!({"id":channel.id,"name":channel.name}));
                    }
                    Ok(_) | Err(Error::NotFound | Error::Forbidden) => {}
                    Err(error) => return Err(error),
                }
            }
            h.with(|c| {
                let status = c.status()?;
                ensure(status.account.as_deref() == Some(account.as_str()), Error::Locked)?;
                let value = json!({"account":account,"pending_sync":status.pending_sync,"pending_account_operation":c.pending()?.is_some(),"owned_channels":owned,"wallets":wallets,"wallets_available":wallets_available});
                let digest = hex::encode(*dmsg_protocol::digest("dmsg/service-deletion/1", &value));
                Ok(json!({"digest":digest,"details":value}))
            }).await
        })).await
    }

    pub async fn request_account_deletion(
        &self,
        digest: String,
        wallet_access_reviewed: bool,
        request_id: String,
    ) -> Result<ObjectSnapshot> {
        let preview: Value = serde_json::from_str(&self.account_deletion_preview().await?.json)
            .map_err(|_| Error::IntegrityFailed)?;
        ensure(preview["digest"] == digest, Error::VersionConflict)?;
        ensure(
            preview["details"]["pending_account_operation"] == false
                && preview["details"]["owned_channels"]
                    .as_array()
                    .is_some_and(|v| v.is_empty()),
            Error::Pending,
        )?;
        ensure(
            wallet_access_reviewed
                || (preview["details"]["wallets_available"] == true
                    && preview["details"]["wallets"]
                        .as_array()
                        .is_some_and(|v| v.is_empty())),
            Error::Forbidden,
        )?;
        let platform = self.platform.clone();
        let shown = digest.clone();
        let id = request_id.clone();
        runtime()
            .spawn_blocking(move || platform.confirm(id, shown, "account_deletion".into()))
            .await
            .map_err(|_| Error::Locked)?
            .map_err(platform::platform_error)?;
        let fresh: Value = serde_json::from_str(&self.account_deletion_preview().await?.json)
            .map_err(|_| Error::IntegrityFailed)?;
        ensure(fresh["digest"] == digest, Error::VersionConflict)?;
        // Erase editable public profile data while this account can still
        // publish. Public cryptographic identifiers/history are retained.
        self.object(|c| {
            Box::pin(async {
                let mut document = dmsg_client::principal::editable(&c.cooperative_hold()).await?;
                if document.name.is_some()
                    || document.description.is_some()
                    || document.avatar_url.is_some()
                    || !document.links.is_empty()
                    || !document.extra.is_empty()
                {
                    document.name = None;
                    document.description = None;
                    document.avatar_url = None;
                    document.links.clear();
                    document.extra.clear();
                    let published = c.publish_document(&document).await?;
                    ensure(
                        published.state.version == published.published_version,
                        Error::Pending,
                    )?;
                } else if let Some(info) = c.principal().await?
                    && info.state.version != info.published_version
                {
                    c.principal_publish().await?;
                    let current = c.principal().await?.ok_or(Error::IntegrityFailed)?;
                    ensure(
                        current.state.version == current.published_version,
                        Error::Pending,
                    )?;
                }
                Ok(())
            })
        })
        .await?;
        self.mobile_post(
            "delete",
            json!({"confirmation":"delete-service-account"}),
            Some(request_id),
        )
        .await
    }

    pub async fn remove_local_account(&self, account: String) -> Result<SessionSnapshot> {
        let transition = self.host.begin_transition()?;
        self.platform.cancel_presence();
        self.cancel_login();
        transition
            .run(move |c| Box::pin(async move { c.remove_local_account(&account).map(|_| ()) }))
            .await?;
        transition.finish()?;
        self.session_state().await
    }
}
