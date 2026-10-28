//! Cloud copies, accounting and recovery. The relay stores only encrypted
//! management information; names, source versions and link keys open here.
use crate::crypto::{open, seal};
use crate::upload::{ManagementEnvelope, UploadPlan, hex_id};
use crate::{Client, hold::Hold};
use dmsg_types::{Error, Hash, Result, ensure, ensure_valid};
use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

impl Client {
    pub(crate) fn seal_management(
        &self,
        plan: &UploadPlan,
        value: &Value,
    ) -> Result<ManagementEnvelope> {
        let account = self.account()?.id.to_string();
        let roots = self.roots()?;
        let generation = roots.generation;
        let root = roots.key(generation).ok_or(Error::RecoveryIncomplete)?;
        let bytes = serde_json::to_vec(value).map_err(|_| Error::IntegrityFailed)?;
        ensure_valid(bytes.len() <= 12 * 1024, "management metadata too large")?;
        Ok(ManagementEnvelope {
            generation,
            ciphertext: seal(
                root,
                &bytes,
                &(
                    "dmsg/storage-management/1",
                    account,
                    &plan.upload_id,
                    &plan.object_id,
                    &plan.version_id,
                    generation,
                ),
            ),
        })
    }

    fn open_management(&self, item: &Value) -> Result<Option<Value>> {
        if item["management"].is_null() {
            return Ok(None);
        }
        let envelope: ManagementEnvelope = serde_json::from_value(item["management"].clone())
            .map_err(|_| Error::IntegrityFailed)?;
        let root = self
            .roots()?
            .key(envelope.generation)
            .ok_or(Error::RecoveryIncomplete)?;
        let account = self.account()?.id.to_string();
        let bytes = open(
            root,
            &envelope.ciphertext,
            &(
                "dmsg/storage-management/1",
                account,
                string(item, "upload_id")?,
                string(item, "object_id")?,
                string(item, "version_id")?,
                envelope.generation,
            ),
        )?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| Error::IntegrityFailed)
    }

    /// The last explicit refresh, encrypted locally. It is never proof that a delete completed.
    #[deprecated(
        note = "Cloud-space inventory is deprecated; use cloud::cloud_usage for usage and link/cloud APIs for content."
    )]
    pub fn files_storage(&self) -> Result<Value> {
        Ok(self
            .cache_read(crate::cache::Kind::Storage, "")?
            .map(|s| s.data)
            .unwrap_or_else(|| json!({"entries":[],"loaded":false})))
    }
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key].as_str().ok_or(Error::IntegrityFailed)
}
fn number(value: &Value, key: &str) -> u64 {
    value[key].as_u64().unwrap_or(0)
}

async fn pages<H: Hold>(h: &H, path: &str) -> Result<Vec<Value>> {
    let mut after = 0;
    let mut entries = Vec::new();
    loop {
        let page = h.get(&format!("{path}?after={after}&limit=100")).await?;
        entries.extend(
            page["entries"]
                .as_array()
                .ok_or(Error::IntegrityFailed)?
                .iter()
                .cloned(),
        );
        if page["end"] == true {
            return Ok(entries);
        }
        let next = page["next_cursor"].as_u64().ok_or(Error::IntegrityFailed)?;
        ensure(next > after, Error::IntegrityFailed)?;
        after = next;
    }
}

fn base(kind: &str, resource: &str) -> Result<String> {
    match kind {
        "account" => {
            resource
                .parse::<dmsg_types::AccountId>()
                .map_err(|_| Error::IntegrityFailed)?;
            Ok(format!("/v1/accounts/{resource}"))
        }
        "channel" => {
            hex_id(resource)?;
            Ok(format!("/v1/channels/{resource}"))
        }
        _ => Err(Error::IntegrityFailed),
    }
}

fn phase(item: &Value) -> &'static str {
    match item["status"].as_str() {
        Some("deleted") if item["charged_bytes"] == 0 && item["reserved_bytes"] == 0 => "released",
        Some("deleted") => "releasing",
        Some("deleting") if !item["cleanup"]["error"].is_null() => "retrying",
        Some("deleting") => "deleting",
        Some("aborted") if item["reserved_bytes"] == 0 && item["cleanup"].is_null() => "released",
        Some("aborted") => "cancelling",
        Some("staging") => "staging",
        Some("committed") if item["kind"] == "share" && item["share"].is_null() => "unpublished",
        Some("committed")
            if item["owner_kind"] == "channel"
                && item["kind"] == "file"
                && item["publication"].is_null() =>
        {
            "unpublished"
        }
        _ => "stored",
    }
}

fn decorate(c: &mut Client, item: &mut Value, kind: &str, resource: &str) -> Result<()> {
    item["owner_kind"] = kind.into();
    item["resource"] = resource.into();
    item["id"] = format!("{kind}:{resource}:{}", string(item, "upload_id")?).into();
    item["phase"] = phase(item).into();
    if item["phase"] == "released" || (item["kind"] == "share" && item["share"].is_object()) {
        c.workspace
            .delete_private(&format!("lib-share-job:{}", string(item, "upload_id")?))?;
    }
    match c.open_management(item) {
        Ok(Some(metadata)) => {
            for key in ["name", "source_file", "source_version", "plain_bytes"] {
                item[key] = metadata[key].clone();
            }
            if matches!(
                metadata["intent"]["kind"].as_str(),
                Some("link" | "secret_link")
            ) {
                let mut record =
                    super::link::LinkRecord::from_value(metadata["intent"]["record"].clone())?;
                if !item["share"].is_null() {
                    record.revoked_at = item["share"]["revoked_at"].as_u64();
                    c.save_link(&record)?;
                    item["url"] = super::link::share_url(
                        &c.config.login_url,
                        &record.share_id,
                        (!record.password).then_some(record.key.as_str()),
                    )
                    .into();
                }
            }
            // Recovery material remains in an encrypted local record, never in the UI response.
            c.workspace.put_private(
                c.keys()?,
                &format!("lib-storage-intent:{}", string(item, "id")?),
                &metadata,
            )?;
            item["recoverable"] = metadata["intent"].is_object().into();
        }
        Ok(None) => {
            item["recoverable"] = false.into();
        }
        Err(Error::RecoveryIncomplete) => {
            item["metadata_error"] = "RECOVERY_INCOMPLETE".into();
        }
        Err(Error::Locked) => return Err(Error::Locked),
        Err(_) => {
            item["metadata_error"] = "INTEGRITY_FAILED".into();
        }
    }
    if item["name"].is_null()
        && kind == "account"
        && item["kind"] == "backup"
        && let Some(entry) = c.cloud_entry(string(item, "upload_id")?)?
        && let Some(manifest) = entry.manifest
    {
        item["name"] = manifest.file.name.into();
        item["source_file"] = manifest.file.id.into();
        item["source_version"] = manifest.file.version.into();
    }
    if let Some(source) = item["source_file"].as_str() {
        let available = c.files_show(source).is_ok();
        item["source_available"] = available.into();
    }
    item.as_object_mut()
        .ok_or(Error::IntegrityFailed)?
        .remove("management");
    Ok(())
}

/// Import the owner's encrypted link metadata even when its local source file is gone.
pub(crate) async fn sync_metadata<H: Hold>(h: &H) -> Result<()> {
    let (account, path) = h
        .with(|c| Ok((c.account()?.id.to_string(), c.base()?)))
        .await?;
    let entries = pages(h, &format!("{path}/storage")).await?;
    h.with(|c| {
        ensure(
            c.account()?.id.to_string() == account,
            Error::VersionConflict,
        )?;
        for mut item in entries {
            decorate(c, &mut item, "account", &account)?;
        }
        Ok(())
    })
    .await
}

/// Refresh cloud copies and the payer's complete accounting ledger. Failed sources
/// are explicit; missing permissions never grant access to channel contents.
#[deprecated(
    note = "Cloud-space inventory is deprecated; use cloud::cloud_usage and link::share_links_refresh."
)]
pub async fn refresh<H: Hold>(h: &H) -> Result<Value> {
    let scope = h
        .with(|c| c.cache_scope(crate::cache::Kind::Storage))
        .await?;
    let (account, path, channels) = h
        .with(|c| {
            c.roots()?;
            Ok((c.account()?.id.to_string(), c.base()?, c.channel_list()?))
        })
        .await?;
    // Warm shared evidence once before independent reads start together.
    h.signed(|_| Ok(())).await?;
    let ledger_path = format!("{path}/storage/ledger");
    let own_path = format!("{path}/storage");
    let (ledger, own) = futures_util::try_join!(pages(h, &ledger_path), pages(h, &own_path))?;
    let mut sources = BTreeSet::new();
    for channel in &channels {
        sources.insert(channel.id.clone());
    }
    for charge in &ledger {
        if charge["consumer"]["kind"] == "channel" {
            sources.insert(string(&charge["consumer"], "id")?.to_owned());
        }
    }
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    for mut item in own {
        h.with(|c| {
            ensure(
                c.account()?.id.to_string() == account,
                Error::VersionConflict,
            )?;
            decorate(c, &mut item, "account", &account)
        })
        .await?;
        entries.push(item);
    }
    let fetched: Vec<_> = stream::iter(sources)
        .map(|channel| async move {
            let result = pages(h, &format!("/v1/channels/{channel}/storage")).await;
            (channel, result)
        })
        .buffer_unordered(4)
        .collect()
        .await;
    for (channel, result) in fetched {
        match result {
            Ok(items) => {
                for mut item in items {
                    h.with(|c| {
                        ensure(
                            c.account()?.id.to_string() == account,
                            Error::VersionConflict,
                        )?;
                        decorate(c, &mut item, "channel", &channel)
                    })
                    .await?;
                    entries.push(item);
                }
            }
            Err(error) => {
                errors.push(json!({"resource":channel,"error":crate::error_code(&error)}))
            }
        }
    }
    for item in &mut entries {
        if let Some(channel) = channels
            .iter()
            .find(|channel| item["resource"] == channel.id)
        {
            item["channel_name"] = channel.name.clone().into();
        }
    }
    // Keep attempts visible when reservation failed or its reply is still unknown.
    // Never interpret an absent remote row as proof that an in-flight write was cancelled.
    let pending: Vec<Value> = h
        .with(|c| {
            let mut local = Vec::new();
            for key in c.workspace.store.sealed_keys("lib-share-job:")? {
                if let Some(job) = c.workspace.private::<Value>(c.keys()?, &key)? {
                    local.push(job);
                }
            }
            Ok(local)
        })
        .await?;
    for job in pending {
        let plan: UploadPlan =
            serde_json::from_value(job["plan"].clone()).map_err(|_| Error::IntegrityFailed)?;
        if entries
            .iter()
            .any(|item| item["upload_id"] == plan.upload_id)
        {
            continue;
        }
        let mut item = json!({"upload_id":plan.upload_id,"object_id":plan.object_id,"version_id":plan.version_id,
            "kind":plan.kind,"status":"staging","local_only":true,"generation":0,"created_at":plan.expires_at.saturating_sub(dmsg_types::DAY),
            "management":plan.management,"bytes":plan.chunks.iter().map(|c|c.size).sum::<u64>()+plan.manifest_size,
            "payer":account,"uploader":account,"charged_bytes":null,"reserved_bytes":null,"charge_id":format!("local:{}",plan.upload_id),"can_cancel":true,"can_delete":false});
        h.with(|c| {
            ensure(
                c.account()?.id.to_string() == account,
                Error::VersionConflict,
            )?;
            decorate(c, &mut item, "account", &account)
        })
        .await?;
        entries.push(item);
    }
    let links = super::link::share_links_remote(h).await?;
    let mut known = BTreeSet::new();
    for item in &mut entries {
        known.insert(string(item, "charge_id")?.to_owned());
        if let Some(link) = links
            .iter()
            .find(|link| item["share"]["share_id"] == link.share_id)
        {
            item["share"] = serde_json::to_value(link).map_err(|_| Error::IntegrityFailed)?;
        }
    }
    // A sponsor can see accounting IDs and bytes even after leaving the channel.
    // Budget charges also explain space that is not a deletable file (messages/history).
    for charge in &ledger {
        if !known.contains(string(charge, "charge_id")?)
            && (number(charge, "reserved") > 0 || number(charge, "committed") > 0)
        {
            let kind = string(&charge["consumer"], "kind")?;
            let resource = string(&charge["consumer"], "id")?;
            entries.push(json!({"id":format!("charge:{}",string(charge,"charge_id")?),"owner_kind":kind,"resource":resource,
                "upload_id":charge["upload_id"],"kind":if charge["purpose"]=="budget" {"history"}else{"restricted"},
                "payer":account,"bytes":number(charge,"committed")+number(charge,"reserved"),
                "charged_bytes":charge["committed"],"reserved_bytes":charge["reserved"],"phase":"stored",
                "can_delete":false,"can_cancel":false,"created_at":0,"charge_id":charge["charge_id"]}));
        }
    }
    let quota = h.get(&format!("{path}/quota")).await?;
    let ledger_reserved: u64 = ledger.iter().map(|c| number(c, "reserved")).sum();
    let ledger_committed: u64 = ledger.iter().map(|c| number(c, "committed")).sum();
    let pending: u64 = entries
        .iter()
        .filter(|i| {
            i["payer"] == account
                && matches!(
                    i["phase"].as_str(),
                    Some("deleting" | "retrying" | "releasing")
                )
        })
        .map(|i| number(i, "charged_bytes"))
        .sum();
    let mut categories: BTreeMap<String, u64> = BTreeMap::new();
    for item in &entries {
        if item["payer"] == account {
            *categories
                .entry(item["kind"].as_str().unwrap_or("other").into())
                .or_default() += number(item, "charged_bytes");
        }
    }
    let result = json!({"loaded":true,"account":account,"entries":entries,"errors":errors,"links":links,
        "usage":quota,"categories":categories,"pending_release":pending,"refreshed_at":crate::now_ms(),
        "reconciliation":{"ledger_reserved":ledger_reserved,"ledger_committed":ledger_committed,
            "consistent":ledger_reserved==number(&quota,"reserved") && ledger_committed==number(&quota,"committed")}});
    h.with(|c| {
        ensure(
            c.account()?.id.to_string() == account,
            Error::VersionConflict,
        )?;
        c.cache_save_checked(
            &scope,
            crate::cache::Kind::Storage,
            "",
            &result,
            crate::now_ms(),
        )
    })
    .await?;
    Ok(result)
}

/// A bounded explicit management action on one immutable upload.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// Account or channel owner namespace.
    pub owner_kind: String,
    /// Account or channel ID.
    pub resource: String,
    /// Immutable upload ID.
    pub upload_id: String,
    /// Observed deletion generation.
    pub generation: u64,
    /// Delete, cancel, retry or recover.
    pub action: String,
    #[serde(default)]
    /// Link to revoke when deleting a live share.
    pub share_id: Option<String>,
}

/// Explicit bounded bulk actions. One failure does not hide other outcomes.
#[deprecated(
    note = "Generic cloud-space actions are deprecated; use content-specific link/cloud operations."
)]
pub async fn act<H: Hold>(h: &H, targets: &[Target]) -> Result<Value> {
    ensure_valid(!targets.is_empty() && targets.len() <= 50, "targets")?;
    let mut outcomes = Vec::new();
    for target in targets {
        let result = act_one(h, target).await;
        outcomes.push(match result {
            Ok(value) => json!({"target":target,"accepted":true,"result":value}),
            Err(error) => {
                json!({"target":target,"accepted":false,"error":crate::error_code(&error),"message":error.to_string()})
            }
        });
    }
    Ok(json!({"outcomes":outcomes}))
}

async fn act_one<H: Hold>(h: &H, target: &Target) -> Result<Value> {
    hex_id(&target.upload_id)?;
    let path = base(&target.owner_kind, &target.resource)?;
    if target.action == "cancel" && target.owner_kind == "account" {
        let status = h
            .find(&format!("{path}/uploads/{}", target.upload_id))
            .await?;
        if status.is_none() {
            return h
                .with(|c| {
                    ensure(
                        c.account()?.id.to_string() == target.resource,
                        Error::Forbidden,
                    )?;
                    let key = format!("lib-share-job:{}", target.upload_id);
                    let job: Value = c
                        .workspace
                        .private(c.keys()?, &key)?
                        .ok_or(Error::NotFound)?;
                    ensure(
                        job["plan"]["expires_at"]
                            .as_u64()
                            .is_some_and(|t| t < crate::now_ms()),
                        Error::ExecutionUnknown,
                    )?;
                    c.workspace.delete_private(&key)?;
                    Ok(json!({"local_cancelled":true}))
                })
                .await;
        }
    }
    if target.action == "recover" {
        return recover(h, target).await;
    }
    if target.action == "delete"
        && let Some(share) = &target.share_id
    {
        ensure(
            target.owner_kind == "account",
            Error::InvalidInput("share target must be an account".into()),
        )?;
        let status = h
            .get(&format!("{path}/uploads/{}", target.upload_id))
            .await?;
        ensure(
            status["plan"]["kind"] == "share" && status["plan"]["object_id"] == *share,
            Error::IntegrityFailed,
        )?;
        return serde_json::to_value(super::link::share_revoke(h, share).await?)
            .map_err(|_| Error::IntegrityFailed);
    }
    if target.action == "delete" && target.owner_kind == "account" {
        h.with(|c| c.deletable(std::slice::from_ref(&target.upload_id)))
            .await?;
    }
    let (suffix, action, payload) = match target.action.as_str() {
        "delete" => (
            "objects/collect",
            "dmsg/object/collect/v1",
            json!({"upload_id":target.upload_id,"generation":target.generation}),
        ),
        "cancel" => (
            "uploads/cancel",
            "dmsg/upload/cancel/v1",
            json!({"upload_id":target.upload_id}),
        ),
        "retry" => (
            "storage/retry",
            "dmsg/storage/retry/v1",
            json!({"upload_id":target.upload_id,"generation":target.generation}),
        ),
        _ => return Err(dmsg_types::invalid("action")),
    };
    let response = h
        .post(
            &format!("{path}/{suffix}"),
            action,
            &payload,
            &Hash::new(crate::crypto::random_id()),
        )
        .await?;
    if target.action == "delete" && target.owner_kind == "account" {
        // The relay accepted retirement; the local backup cache must stop
        // claiming this copy is live, as `files/cloud_delete` already does.
        h.with(|c| c.forget_cloud(&target.upload_id)).await?;
    }
    Ok(response)
}

async fn recover<H: Hold>(h: &H, target: &Target) -> Result<Value> {
    let path = base(&target.owner_kind, &target.resource)?;
    let id = format!(
        "{}:{}:{}",
        target.owner_kind, target.resource, target.upload_id
    );
    let metadata: Value = h
        .with(|c| {
            c.workspace
                .private(c.keys()?, &format!("lib-storage-intent:{id}"))?
                .ok_or(Error::RecoveryIncomplete)
        })
        .await?;
    // Secret snapshots are resumed only through their owner approval and original publication journal.
    ensure(
        !matches!(
            metadata["intent"]["kind"].as_str(),
            Some("secret_link" | "secret_channel")
        ),
        Error::Forbidden,
    )?;
    let links = metadata["intent"]["kind"] == "link";
    let pending: Option<Value> = if links && target.owner_kind == "account" {
        h.with(|c| {
            c.workspace
                .private(c.keys()?, &format!("lib-share-job:{}", target.upload_id))
        })
        .await?
    } else {
        None
    };
    if let Some(pending) = pending {
        let plan: UploadPlan =
            serde_json::from_value(pending["plan"].clone()).map_err(|_| Error::IntegrityFailed)?;
        let manifest = crate::codec::unb64(string(&pending, "manifest")?)?;
        crate::upload::upload(
            h,
            &path,
            &plan,
            &manifest,
            0,
            crate::files::Source::Library,
            |_, _| Ok(()),
        )
        .await?;
    }
    h.post(
        &format!("{path}/uploads/finalize"),
        "dmsg/upload/finalize/v1",
        &json!({"upload_id":target.upload_id}),
        &Hash::new(crate::crypto::random_id()),
    )
    .await?;
    if metadata["intent"]["kind"] == "link" {
        ensure(
            target.owner_kind == "account",
            Error::InvalidInput("link target must be an account".into()),
        )?;
        let record = super::link::LinkRecord::from_value(metadata["intent"]["record"].clone())?;
        ensure(record.upload_id == target.upload_id, Error::IntegrityFailed)?;
        let existing = super::link::share_links_remote(h).await?;
        if existing.iter().any(|link| link.share_id == record.share_id) {
            return Ok(json!({"published":true}));
        }
        ensure(record.expires_at > crate::now_ms(), Error::ResultExpired)?;
        return h.post(&format!("{path}/shares"),"dmsg/share/create/v1",&json!({"share_id":record.share_id,"upload_id":record.upload_id,"expires_at":record.expires_at,"max_downloads":record.max_downloads,"key_wrap":metadata["intent"]["key_wrap"]}),&Hash::new(crate::crypto::random_id())).await;
    }
    if metadata["intent"]["kind"] == "channel" {
        ensure(
            target.owner_kind == "channel",
            Error::InvalidInput("attachment target must be a channel".into()),
        )?;
        return crate::channel::resume_file(
            h,
            &target.resource,
            &target.upload_id,
            &metadata["intent"],
        )
        .await;
    }
    Ok(json!({"committed":true}))
}

/// Estimate a new encrypted copy and show who pays before creating it.
pub async fn quote<H: Hold>(
    h: &H,
    file: &str,
    version: Option<&str>,
    channel: Option<&str>,
) -> Result<Value> {
    let (account, source) = h
        .with(|c| {
            Ok((
                c.account()?.id.to_string(),
                c.library_manifest(file, version)?.2,
            ))
        })
        .await?;
    let payer = if let Some(channel) = channel {
        hex_id(channel)?;
        let head = h.get(&format!("/v1/channels/{channel}")).await?;
        string(&head["billing"], "billing_account")?.to_owned()
    } else {
        account.clone()
    };
    let quota = if payer == account {
        h.get(&format!("/v1/accounts/{account}/quota")).await?
    } else {
        Value::Null
    };
    let (owner_kind, resource) = channel.map_or(("account", account.as_str()), |channel| {
        ("channel", channel)
    });
    let mut copies = pages(h, &format!("{}/storage", base(owner_kind, resource)?)).await?;
    h.with(|c| {
        ensure(
            c.account()?.id.to_string() == account,
            Error::VersionConflict,
        )?;
        for item in &mut copies {
            decorate(c, item, owner_kind, resource)?;
        }
        Ok(())
    })
    .await?;
    let estimate = source.size + crate::files::chunk_count(source.size) * 64 + 16 * 1024;
    let existing: Vec<Value> = copies
        .iter()
        .filter(|item| {
            item["source_file"] == file
                && item["source_version"] == source.version
                && item["phase"] == "stored"
        })
        .cloned()
        .collect();
    Ok(
        json!({"payer":payer,"estimated_bytes":estimate,"plain_bytes":source.size,"usage":quota,"existing":existing}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deletion_needs_both_physical_cleanup_and_quota_release() {
        assert_eq!(
            phase(&json!({"status":"deleted","charged_bytes":50,"reserved_bytes":0})),
            "releasing"
        );
        assert_eq!(
            phase(&json!({"status":"deleted","charged_bytes":0,"reserved_bytes":0})),
            "released"
        );
        assert_eq!(
            phase(&json!({"status":"deleted","charged_bytes":null,"reserved_bytes":0})),
            "releasing"
        );
        assert_eq!(
            phase(
                &json!({"status":"deleting","cleanup":{"attempts":2,"error":{"code":"UNAVAILABLE"}}})
            ),
            "retrying"
        );
    }
    #[test]
    fn metadata_is_account_root_encrypted_and_bound_to_the_exact_upload() {
        let dir = crate::testing::TempDir::new();
        let client = crate::testing::unlocked(&dir.0, false, None);
        let plan: UploadPlan=serde_json::from_value(json!({"upload_id":"01".repeat(32),"object_id":"02".repeat(32),"version_id":"03".repeat(32),"root_generation":0,"epoch":0,"control_head":null,"kind":"share","chunks":[],"manifest_digest":"04".repeat(32),"manifest_size":1,"expires_at":1})).unwrap();
        let value = json!({"name":"private-name","source_file":"private-source","intent":{"key":"private-key"}});
        let sealed = client.seal_management(&plan, &value).unwrap();
        assert!(!sealed.ciphertext.contains("private-name"));
        let mut item = json!({"upload_id":plan.upload_id,"object_id":plan.object_id,"version_id":plan.version_id,"management":sealed});
        assert_eq!(client.open_management(&item).unwrap(), Some(value));
        item["upload_id"] = "ff".repeat(32).into();
        assert!(client.open_management(&item).is_err());
    }

    #[test]
    fn sharing_management_restores_old_file_links_and_current_sources() {
        use super::super::link::{LinkRecord, LinkSource};

        let dir = crate::testing::TempDir::new();
        let mut client = crate::testing::unlocked(&dir.0, false, None);
        let plan: UploadPlan = serde_json::from_value(json!({
            "upload_id":"01".repeat(32), "object_id":"02".repeat(32),
            "version_id":"03".repeat(32), "root_generation":0, "epoch":0,
            "control_head":null, "kind":"share", "chunks":[],
            "manifest_digest":"04".repeat(32), "manifest_size":1, "expires_at":1,
        }))
        .unwrap();
        let old = json!({
            "file":"05".repeat(32), "version":"06".repeat(32),
            "name":"fixture.txt", "size":7, "share_id":plan.object_id,
            "upload_id":plan.upload_id, "key":crate::codec::b64(&[7;32]),
            "expires_at":crate::now_ms() + dmsg_types::DAY, "max_downloads":3,
            "password":false, "created_at":1, "revoked_at":null, "downloads":1,
        });
        let file = LinkRecord::from_value(old.clone()).unwrap();
        let mut snapshot = file.clone();
        snapshot.source = LinkSource::VaultSnapshot {
            item_id: "08".repeat(32),
            item_revision: "09".repeat(32),
            snapshot_id: "0a".repeat(32),
        };
        for (record, expected, kind) in [
            (old, &file, "link"),
            (json!(file), &file, "link"),
            (json!(snapshot), &snapshot, "secret_link"),
        ] {
            let metadata = json!({"intent":{"kind":kind,"record":record}});
            let sealed = client.seal_management(&plan, &metadata).unwrap();
            let mut item = json!({
                "upload_id":plan.upload_id, "object_id":plan.object_id,
                "version_id":plan.version_id, "kind":"share", "status":"committed",
                "management":sealed, "share":{"revoked_at":2},
            });
            let account = client.account().unwrap().id.to_string();
            decorate(&mut client, &mut item, "account", &account).unwrap();
            assert_eq!(item["recoverable"], true);
            assert!(item["url"].as_str().unwrap().ends_with(&expected.key));
            let records = client.link_records().unwrap();
            assert_eq!(records.len(), 1);
            let mut expected = expected.clone();
            expected.revoked_at = Some(2);
            assert_eq!(json!(records[0]), json!(expected));
            let links = client.files_share_links().unwrap();
            assert_eq!(links[0].source.as_ref(), Some(&expected.source));
            assert_eq!(links[0].state, "revoked");
        }
    }

    #[tokio::test]
    async fn storage_reads_four_channels_together_and_keeps_partial_errors() {
        use std::sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        };
        struct Relay {
            client: Mutex<Client>,
            barrier: tokio::sync::Barrier,
            started: AtomicUsize,
            active: AtomicUsize,
            peak: AtomicUsize,
        }
        impl Hold for Relay {
            async fn with<T: Send>(
                &self,
                step: impl FnOnce(&mut Client) -> Result<T> + Send,
            ) -> Result<T> {
                step(&mut self.client.lock().unwrap())
            }

            async fn get(&self, path: &str) -> Result<Value> {
                if path.starts_with("/v1/channels/") {
                    let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
                    self.peak.fetch_max(active, Ordering::SeqCst);
                    let index = self.started.fetch_add(1, Ordering::SeqCst);
                    if index < 4 {
                        self.barrier.wait().await;
                    }
                    tokio::task::yield_now().await;
                    self.active.fetch_sub(1, Ordering::SeqCst);
                    if index == 4 {
                        return Err(Error::Forbidden);
                    }
                }
                if path.ends_with("/quota") {
                    return Ok(json!({"reserved":0,"committed":0}));
                }
                Ok(json!({"entries":[],"end":true,"next_cursor":0}))
            }
        }
        let dir = crate::testing::TempDir::new();
        let mut client = crate::testing::unlocked(&dir.0, false, None);
        crate::testing::warm(&mut client);
        for n in 1..=5 {
            crate::testing::joined_channel(&mut client, &format!("{n:064x}"));
        }
        let relay = Relay {
            client: Mutex::new(client),
            barrier: tokio::sync::Barrier::new(4),
            started: 0.into(),
            active: 0.into(),
            peak: 0.into(),
        };

        #[allow(deprecated)]
        let result = tokio::time::timeout(std::time::Duration::from_secs(3), refresh(&relay))
            .await
            .expect("independent channel inventories must not wait for each other")
            .unwrap();
        assert_eq!(relay.peak.load(Ordering::SeqCst), 4);
        assert_eq!(relay.started.load(Ordering::SeqCst), 5);
        assert_eq!(result["errors"].as_array().unwrap().len(), 1);
        assert_eq!(result["errors"][0]["error"], "FORBIDDEN");
        assert_eq!(result["reconciliation"]["consistent"], true);
    }
}
