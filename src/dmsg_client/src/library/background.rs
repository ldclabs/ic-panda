//! Explicit background transfers delegate only immutable backup ciphertext.
//! The OS receives no signing/root/file keys. Finalization stays in an unlocked
//! foreground session, under the original journal and account.
use super::{backup, cloud};
use crate::{
    Client,
    codec::sha256,
    files::BlobStore,
    hold::{Hold, Unlocked},
    upload::{UploadPlan, random_hex},
};
use dmsg_types::{DAY, Error, Hash, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;

const PREFIX: &str = "mobile-transfer:";

#[derive(Serialize, Deserialize, Clone, Debug)]
/// One exact ciphertext slot, with a bearer URL scoped to its original plan.
pub struct CipherPart {
    /// Decimal chunk index or the encrypted manifest.
    pub slot: String,
    /// Exact ciphertext length.
    pub size: u64,
    /// SHA-256 of the ciphertext.
    pub digest: String,
    /// Scoped HTTPS transfer URL.
    pub url: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
/// The OS handoff contains only ciphertext locations and bounded authority.
pub struct CipherTransfer {
    /// Random transfer authorization ID.
    pub id: String,
    /// Owning account.
    pub account: String,
    /// Authorizing device ID.
    pub device: String,
    /// GET download or PUT upload.
    pub method: String,
    /// Original immutable upload ID.
    pub upload_id: String,
    /// Exclusive authorization deadline, Unix milliseconds.
    pub expires_at: u64,
    /// Previously verified immutable plan; contains no plaintext keys.
    pub plan: UploadPlan,
    /// At most one file’s bounded ciphertext blocks.
    pub parts: Vec<CipherPart>,
}

#[derive(Serialize, Deserialize)]
struct Journal {
    transfer: CipherTransfer,
    backup_key: Option<String>,
    folder: Option<String>,
    finished: bool,
    created_at: u64,
}

impl Client {
    fn cipher_journal(&self, id: &str) -> Result<Journal> {
        crate::upload::hex_id(id)?;
        let job: Journal = self
            .workspace
            .private(self.keys()?, &format!("{PREFIX}{id}"))?
            .ok_or(Error::NotFound)?;
        ensure(
            job.transfer.account == self.account()?.id.to_string(),
            Error::Forbidden,
        )?;
        Ok(job)
    }

    /// Read a single bounded ciphertext block; OS paths are never accepted.
    /// # Errors
    /// Locked, unknown job/slot, or a corrupt/missing ciphertext block.
    pub fn background_upload_part(&self, id: &str, slot: &str) -> Result<Vec<u8>> {
        let job = self.cipher_journal(id)?;
        ensure(
            !job.finished && job.transfer.method == "PUT",
            Error::Forbidden,
        )?;
        let part = job
            .transfer
            .parts
            .iter()
            .find(|p| p.slot == slot)
            .ok_or(Error::NotFound)?;
        let bytes = if slot == "manifest" {
            let key = job.backup_key.as_deref().ok_or(Error::IntegrityFailed)?;
            let backup: backup::BackupJob = self
                .workspace
                .store
                .library_job(key)?
                .ok_or(Error::NotFound)?;
            ensure(backup.plan == job.transfer.plan, Error::VersionConflict)?;
            backup.manifest.to_vec()
        } else {
            self.blobs()?
                .chunk(&part.digest)?
                .ok_or(Error::RecoveryIncomplete)?
        };
        verify_part(part, &bytes)?;
        Ok(bytes)
    }

    /// Admit downloaded ciphertext only against the original sealed journal.
    /// # Errors
    /// Locked, unknown job/slot, size or digest mismatch. No plaintext is returned.
    pub fn background_accept_part(&self, id: &str, slot: &str, bytes: &[u8]) -> Result<()> {
        let job = self.cipher_journal(id)?;
        if job.finished {
            return Ok(());
        }
        ensure(job.transfer.method == "GET", Error::Forbidden)?;
        let part = job
            .transfer
            .parts
            .iter()
            .find(|p| p.slot == slot)
            .ok_or(Error::NotFound)?;
        verify_part(part, bytes)?;
        self.blobs()?.put_chunk(&part.digest, bytes)
    }
}

fn verify_part(part: &CipherPart, bytes: &[u8]) -> Result<()> {
    ensure(
        bytes.len() as u64 == part.size && hex::encode(*sha256(bytes)) == part.digest,
        Error::IntegrityFailed,
    )
}

fn verify_grant(
    grant: &CipherTransfer,
    plan: &UploadPlan,
    id: &str,
    method: &str,
    actor: (&str, &str),
    origin: &str,
    now: u64,
) -> Result<()> {
    let (account, device) = actor;
    ensure(
        grant.id == id
            && grant.plan == *plan
            && grant.account == account
            && grant.device == device
            && grant.method == method
            && grant.upload_id == plan.upload_id
            && grant.expires_at > now
            && grant.expires_at <= now.saturating_add(DAY),
        Error::IntegrityFailed,
    )?;
    ensure(
        grant.parts.len() == plan.chunks.len() + usize::from(method == "PUT"),
        Error::IntegrityFailed,
    )?;
    for (index, part) in grant.parts.iter().enumerate() {
        let slot = if index == plan.chunks.len() {
            "manifest".into()
        } else {
            index.to_string()
        };
        let (size, digest) = if index == plan.chunks.len() {
            (plan.manifest_size, &plan.manifest_digest)
        } else {
            (plan.chunks[index].size, &plan.chunks[index].digest)
        };
        let url = url::Url::parse(&part.url).map_err(|_| Error::IntegrityFailed)?;
        let token = url
            .query_pairs()
            .filter(|(key, _)| key == "token")
            .map(|(_, v)| v.into_owned())
            .collect::<Vec<_>>();
        ensure(
            part.slot == slot
                && part.size == size
                && part.digest == *digest
                && size <= 1024 * 1024 + 64
                && url.origin().ascii_serialization() == origin.trim_end_matches('/')
                && url.path() == format!("/mobile-transfer/v1/{account}/{id}/{slot}")
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none()
                && url.query_pairs().count() == 1
                && token.len() == 1,
            Error::IntegrityFailed,
        )?;
        crate::upload::hex_id(&token[0])?;
    }
    Ok(())
}

async fn grant<H: Hold>(
    h: &H,
    plan: UploadPlan,
    method: &str,
    backup_key: Option<String>,
    folder: Option<String>,
) -> Result<CipherTransfer> {
    let h = Unlocked::new(h).await?;
    let (account, device, base, origin) = h
        .with(|c| {
            Ok((
                c.account()?.id.to_string(),
                hex::encode(*c.workspace.meta.device_id),
                c.base()?,
                c.config.relay.clone(),
            ))
        })
        .await?;
    let id = random_hex();
    let value = h
        .post(
            &format!("{base}/mobile/transfers"),
            "dmsg/mobile/transfer/grant/v1",
            &json!({"id":id,"upload_id":plan.upload_id,"method":method}),
            &Hash::new(crate::crypto::random_id()),
        )
        .await?;
    let transfer: CipherTransfer =
        serde_json::from_value(value).map_err(|_| Error::IntegrityFailed)?;
    verify_grant(
        &transfer,
        &plan,
        &id,
        method,
        (&account, &device),
        &origin,
        crate::now_ms(),
    )?;
    h.with(|c| {
        let now = crate::now_ms();
        for key in c.workspace.store.sealed_keys(PREFIX)? {
            if let Some(old) = c.workspace.private::<Journal>(c.keys()?, &key)?
                && old.created_at.saturating_add(30 * DAY) < now
            {
                c.workspace.delete_private(&key)?;
            }
        }
        c.workspace.put_private(
            c.keys()?,
            &format!("{PREFIX}{id}"),
            &Journal {
                transfer: transfer.clone(),
                backup_key,
                folder,
                finished: false,
                created_at: now,
            },
        )
    })
    .await?;
    Ok(transfer)
}

/// Reserve a pending backup and delegate transfer of its ciphertext only.
/// # Errors
/// As foreground backup/reservation; an expired plan must be resumed in foreground.
pub async fn upload<H: Hold>(h: &H, file: &str, version: &str) -> Result<CipherTransfer> {
    let h = Unlocked::new(h).await?;
    let (key, job, base) = h
        .with(|c| {
            let (key, job) = c
                .backup_jobs()?
                .into_iter()
                .find(|(_, j)| j.file == file && j.version == version)
                .ok_or(Error::NotFound)?;
            ensure(!job.committed, Error::VersionConflict)?;
            Ok((key, job, c.base()?))
        })
        .await?;
    h.post(
        &format!("{base}/uploads"),
        "dmsg/upload/reserve/v1",
        &job.plan,
        &Hash::new(crate::crypto::random_id()),
    )
    .await?;
    grant(&h, job.plan, "PUT", Some(key), None).await
}

/// The inventory was already verified/decrypted in the foreground. Delegate
/// only the chunks of this selected immutable backup, never a root/vault object.
/// # Errors
/// Missing inventory/root/destination or a rejected service authorization.
pub async fn download<H: Hold>(
    h: &H,
    upload: &str,
    folder: Option<String>,
) -> Result<CipherTransfer> {
    let h = Unlocked::new(h).await?;
    let plan = h
        .with(|c| {
            let entry = c.cloud_entry(upload)?.ok_or(Error::NotFound)?;
            ensure(entry.manifest.is_some(), Error::RecoveryIncomplete)?;
            if let Some(folder) = &folder {
                c.catalog()?.live_folder(folder)?;
            }
            Ok(entry.plan)
        })
        .await?;
    grant(&h, plan, "GET", None, folder).await
}

/// Foreground verification/commit. Repeating after a lost completion is safe.
/// # Errors
/// Locked, incomplete/tampered bytes, revoked rights or an unavailable service.
pub async fn finish<H: Hold>(h: &H, id: &str) -> Result<()> {
    let h = Unlocked::new(h).await?;
    let mut job = h.with(|c| c.cipher_journal(id)).await?;
    if job.finished {
        return Ok(());
    }
    if job.transfer.method == "PUT" {
        let base = h.with(|c| c.base()).await?;
        crate::upload::finalize(&h, &base, &job.transfer.plan).await?;
        h.with(|c| {
            let key = job.backup_key.as_deref().ok_or(Error::IntegrityFailed)?;
            if let Some(backup) = c.workspace.store.library_job::<backup::BackupJob>(key)? {
                ensure(backup.plan == job.transfer.plan, Error::VersionConflict)?;
                c.settle_backup(key, &backup, crate::now_ms())?;
            }
            Ok(())
        })
        .await?;
    } else {
        h.with(|c| {
            let entry = c
                .cloud_entry(&job.transfer.upload_id)?
                .ok_or(Error::NotFound)?;
            ensure(entry.plan == job.transfer.plan, Error::VersionConflict)?;
            for part in &job.transfer.parts {
                ensure(
                    c.blobs()?.has_chunk(&part.digest)?,
                    Error::RecoveryIncomplete,
                )?;
            }
            Ok(())
        })
        .await?;
        cloud::cloud_fetch(
            &h,
            &job.transfer.upload_id,
            job.folder.as_deref(),
            &mut |_, _| {},
        )
        .await?;
    }
    h.with(|c| {
        job.finished = true;
        c.workspace
            .put_private(c.keys()?, &format!("{PREFIX}{id}"), &job)
    })
    .await
}

/// Cancel future byte authorization; the original backup remains resumable.
/// # Errors
/// Locked or service unavailable. OS cancellation is independently immediate.
pub async fn revoke<H: Hold>(h: &H, id: &str) -> Result<()> {
    let base = h
        .with(|c| {
            c.cipher_journal(id)?;
            c.base()
        })
        .await?;
    h.post(
        &format!("{base}/mobile/transfers/revoke"),
        "dmsg/mobile/transfer/revoke/v1",
        &json!({"id":id}),
        &Hash::new(crate::crypto::random_id()),
    )
    .await?;
    h.with(|c| c.workspace.delete_private(&format!("{PREFIX}{id}")))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn os_handoff_rejects_redirected_expanded_or_cross_account_authority() {
        let now = 1_800_000_000_000;
        let id = "01".repeat(32);
        let account = "040g2081040g2081040g";
        let device = "02".repeat(32);
        let plan = UploadPlan {
            management: None,
            upload_id: "03".repeat(32),
            object_id: "04".repeat(32),
            version_id: "05".repeat(32),
            root_generation: 1,
            epoch: 0,
            control_head: None,
            kind: "backup".into(),
            chunks: vec![crate::upload::ChunkRef {
                digest: "06".repeat(32),
                size: 4,
            }],
            manifest_digest: "07".repeat(32),
            manifest_size: 8,
            expires_at: now + DAY,
        };
        let transfer = CipherTransfer {
            id: id.clone(),
            account: account.into(),
            device: device.clone(),
            method: "GET".into(),
            upload_id: plan.upload_id.clone(),
            expires_at: now + DAY,
            plan: plan.clone(),
            parts: vec![CipherPart {
                slot: "0".into(),
                size: 4,
                digest: plan.chunks[0].digest.clone(),
                url: format!(
                    "https://core.example.test/mobile-transfer/v1/{account}/{id}/0?token={}",
                    "08".repeat(32)
                ),
            }],
        };
        let check = |value: &CipherTransfer| -> Result<()> {
            verify_grant(
                value,
                &plan,
                &id,
                "GET",
                (account, &device),
                "https://core.example.test",
                now,
            )
        };
        assert!(check(&transfer).is_ok());
        let mut bad = transfer.clone();
        bad.parts[0].url = bad.parts[0]
            .url
            .replace("core.example.test", "elsewhere.example.test");
        assert!(check(&bad).is_err());
        bad = transfer.clone();
        bad.account = "foreign".into();
        assert!(check(&bad).is_err());
        bad = transfer.clone();
        bad.device = "09".repeat(32);
        assert!(check(&bad).is_err());
        bad = transfer.clone();
        bad.parts[0].digest = "10".repeat(32);
        assert!(check(&bad).is_err());
        bad = transfer.clone();
        bad.parts[0].url += "&callback=https://other.test";
        assert!(check(&bad).is_err());
        bad = transfer.clone();
        bad.expires_at += 1;
        assert!(check(&bad).is_err());
        bad = transfer.clone();
        bad.parts.push(bad.parts[0].clone());
        assert!(check(&bad).is_err());
    }
}
