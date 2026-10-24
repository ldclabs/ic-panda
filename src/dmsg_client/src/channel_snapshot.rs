//! Fixed-destination secret snapshots. No generic outbox or publication replay.
use super::*;

/// Value-free channel scope shown in a snapshot approval.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Target {
    /// Exact destination channel.
    pub channel_id: String,
    /// Display name at preparation time.
    pub name: String,
    /// Fixed content epoch.
    pub epoch: u64,
    /// Fixed signed control head, including membership changes.
    pub control_head: String,
    /// Fixed ACL revision.
    pub acl_version: u64,
}

impl Client {
    pub(crate) fn snapshot_channel_target(&self, channel: &str) -> Result<Target> {
        let view = self.channel_view(channel)?;
        let ledger = view.ledger.ok_or(Error::NotFound)?;
        ensure(
            ledger.status == Status::Active && may_publish(&ledger, &self.me()?),
            Error::Forbidden,
        )?;
        Ok(Target {
            channel_id: channel.into(),
            name: view.name,
            epoch: ledger.epoch,
            control_head: ledger.head,
            acl_version: ledger.acl_version,
        })
    }

    pub(crate) fn snapshot_channel_check(&self, target: &Target) -> Result<Ledger> {
        let ledger = self
            .local(&target.channel_id)?
            .ledger
            .ok_or(Error::NotFound)?;
        ensure(
            ledger.epoch == target.epoch
                && ledger.head == target.control_head
                && ledger.acl_version == target.acl_version,
            Error::PolicyStale,
        )?;
        ensure(
            ledger.status == Status::Active && may_publish(&ledger, &self.me()?),
            Error::Forbidden,
        )?;
        Ok(ledger)
    }

    pub(crate) fn snapshot_channel_upload(
        &self,
        target: &Target,
        file: &FileManifest,
        upload_id: &str,
    ) -> Result<(FileRef, UploadPlan, Vec<u8>)> {
        let ledger = self.snapshot_channel_check(target)?;
        let (reference, upload) = self.prepare_upload_id(
            &target.channel_id,
            &ledger,
            file,
            self.blobs()?,
            upload_id.into(),
        )?;
        Ok((reference, upload.plan, upload.manifest.into_vec()))
    }

    fn snapshot_message(
        &self,
        target: &Target,
        reference: &FileRef,
        message_id: &str,
    ) -> Result<MessagePayload> {
        self.snapshot_channel_check(target)?;
        ensure(
            reference.epoch == target.epoch && reference.mime == crate::secrets::package::MIME,
            Error::IntegrityFailed,
        )?;
        let ring = self.keyring(&target.channel_id)?.1;
        let root = Zeroizing::new(unb64(
            ring.keys
                .get(&target.epoch)
                .ok_or(Error::RecoveryIncomplete)?,
        )?);
        let mut payload = MessagePayload {
            attachment_upload: Some(reference.upload_id.clone()),
            channel_id: target.channel_id.clone(),
            epoch: target.epoch,
            control_head: target.control_head.clone(),
            acl_version: target.acl_version,
            message_id: message_id.into(),
            client_created_at: now_ms(),
            ciphertext: String::new(),
        };
        let device = self.device_hex();
        let key = message_key(&root, &target.channel_id, target.epoch, message_id, &device);
        let body = Zeroizing::new(canonical(&Body {
            text: String::new(),
            file: Some(reference.clone()),
            reply_to: None,
            edit_of: None,
            retract: None,
            agent: None,
            packet: None,
        }));
        payload.ciphertext = seal(&key, &body, &message_aad(&self.me()?, &device, &payload));
        Ok(payload)
    }

    fn snapshot_receipt(&self, target: &Target, message_id: &str) -> Result<Option<Json>> {
        let operation: Operation = self
            .job(&target.channel_id, &format!("secret-snapshot:{message_id}"))?
            .ok_or(Error::NotFound)?;
        let Some(receipt) = operation.result else {
            return Ok(None);
        };
        ensure(
            operation.payload["message_id"] == message_id
                && operation.payload["control_head"] == target.control_head
                && receipt["digest"] == hex_sha256(&unb64(&operation.cose)?)
                && receipt["seq"].as_u64().is_some_and(|n| n > 0)
                && receipt["stored_at"].as_u64().is_some(),
            Error::IntegrityFailed,
        )?;
        Ok(Some(receipt))
    }

    pub(crate) fn snapshot_channel_message(&self, channel: &str, seq: u64) -> Result<Message> {
        let message = self.message_at(channel, seq)?;
        ensure(
            !message.retracted
                && !message.unreadable
                && !message.file_deleted
                && !message.hidden
                && !message.amends(),
            Error::Forbidden,
        )?;
        let file = message.file.as_ref().ok_or(Error::NotFound)?;
        ensure(
            file.mime == crate::secrets::package::MIME && file.size <= MAX_FILE,
            Error::InvalidInput("secret package attachment".into()),
        )?;
        let ledger = self.local(channel)?.ledger.ok_or(Error::NotFound)?;
        ensure(readable(&ledger, &self.me()?, file.epoch), Error::Forbidden)?;
        Ok(message)
    }
}

pub(crate) async fn prepare<H: Hold>(h: &H, channel: &str) -> Result<Target> {
    pull(h, channel, None, None).await?;
    h.with(|c| c.snapshot_channel_target(channel)).await
}

pub(crate) async fn publication<H: Hold>(
    h: &H,
    target: &Target,
    reference: &FileRef,
    message_id: &str,
) -> Result<Prepared> {
    pull(h, &target.channel_id, None, None).await?;
    h.with(|c| c.snapshot_channel_check(target)).await?;
    open_epoch(h, &target.channel_id, target.epoch).await?;
    let payload = h
        .with(|c| {
            c.snapshot_channel_check(target)?;
            if let Some(prior) =
                c.job::<Operation>(&target.channel_id, &format!("secret-snapshot:{message_id}"))?
            {
                ensure(
                    prior.payload["control_head"] == target.control_head
                        && prior.payload["attachment_upload"] == reference.upload_id,
                    Error::IdempotencyConflict,
                )?;
                from(&prior.payload)
            } else {
                c.snapshot_message(target, reference, message_id)
            }
        })
        .await?;
    // Only this dedicated journal can see the operation; ordinary message resend cannot find it.
    match journaled(
        h,
        &target.channel_id,
        &format!("secret-snapshot:{message_id}"),
        &base(&target.channel_id)?,
        MESSAGE,
        json(&payload),
        "/messages",
    )
    .await?
    {
        Posting::Send(request) => Ok(request),
        Posting::Done(_) => Err(Error::ExecutionUnknown),
    }
}

pub(crate) async fn settle<H: Hold>(
    h: &H,
    target: &Target,
    message_id: &str,
    reply: Result<Json>,
) -> Result<Json> {
    h.with(|c| {
        c.journaled_settle(
            &target.channel_id,
            &format!("secret-snapshot:{message_id}"),
            reply,
        )?;
        c.snapshot_receipt(target, message_id)?
            .ok_or(Error::ExecutionUnknown)
    })
    .await
}

pub(crate) async fn reconcile<H: Hold>(
    h: &H,
    target: &Target,
    message_id: &str,
) -> Result<Option<Json>> {
    if !committed(
        h,
        &target.channel_id,
        &format!("secret-snapshot:{message_id}"),
    )
    .await?
    {
        return Ok(None);
    }
    h.with(|c| c.snapshot_receipt(target, message_id)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_file_and_message_use_independent_keys_and_never_enter_generic_outbox() {
        let dir = crate::testing::TempDir::new();
        let mut c = crate::testing::unlocked(&dir.0, false, None);
        let (channel, _) = crate::channel::tests::active_channel(&mut c);
        let target = c.snapshot_channel_target(&channel).unwrap();
        let mut file = c
            .secret_stage_start("secret.dmsg-package", crate::secrets::package::MIME, 7)
            .unwrap();
        c.secret_stage_part(&mut file, b"fixture").unwrap();
        c.secret_stage_finish(&mut file, None).unwrap();
        let upload_id = "aa".repeat(32);
        let (reference, plan, manifest) = c
            .snapshot_channel_upload(&target, &file.manifest, &upload_id)
            .unwrap();
        let opened = shared_manifest(&channel, &reference, &plan, &manifest).unwrap();
        assert_eq!(opened.sha256, file.manifest.sha256);
        assert!(opened.key.is_empty());
        assert_eq!(reference.upload_id, upload_id);
        let mut malformed = file.manifest.clone();
        malformed.size = 2 * crate::files::CHUNK as u64;
        let (bad_ref, bad_plan, bad_sealed) = c
            .snapshot_channel_upload(&target, &malformed, &"ee".repeat(32))
            .unwrap();
        assert!(shared_manifest(&channel, &bad_ref, &bad_plan, &bad_sealed).is_err());

        let id = "bb".repeat(32);
        let payload = c.snapshot_message(&target, &reference, &id).unwrap();
        assert!(
            !canonical(&payload)
                .windows(reference.key.len())
                .any(|w| w == reference.key.as_bytes())
        );
        let message = open_message(
            &channel,
            &c.keyring(&channel).unwrap().1,
            Message {
                account: c.me().unwrap(),
                device: c.device_hex(),
                message_id: id,
                epoch: target.epoch,
                ..Default::default()
            },
            &payload,
        );
        assert!(!message.unreadable);
        assert_eq!(message.file.as_ref().unwrap().key, reference.key);
        assert!(c.channel_outbox(&channel).unwrap().is_empty());
        let mut wrong = target.clone();
        wrong.channel_id = "dd".repeat(32);
        assert!(c.snapshot_channel_check(&wrong).is_err());
        wrong = target;
        wrong.epoch += 1;
        assert_eq!(
            c.snapshot_channel_check(&wrong).unwrap_err(),
            Error::PolicyStale
        );
    }
}
