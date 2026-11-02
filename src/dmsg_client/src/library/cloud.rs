//! The cloud side of the library: the account's backup inventory as the
//! relay lists it, fetching a backup into the library, deleting a cloud
//! copy, verifying one, and freeing a version's local chunks once its backup
//! is verified.
//!
//! The inventory is the set of committed `backup` uploads of every device of
//! the account, read by a cursor: each entry's signed plan is checked
//! against the certified evidence the relay kept from its reservation, so a
//! device replaced since (by recovery, or dropped at the device bound) still
//! counts; its manifest is downloaded and opened with the root of its
//! generation, and the result is cached as a sealed `lib-cloud` record; an
//! entry whose root this device lacked opens at a later refresh, once the
//! root is here. There is no mutable cloud catalog, so there is nothing to
//! reconcile: a deletion on either side changes only that side.

use super::backup::{BACKUP, BackupManifest};
use super::catalog::{
    Backup, FILE, FileRecord, FileView, Origin, ROOT, VERSION, Version, VersionRef, safe_name,
};
use super::{Client, VersionCheck};
use crate::cloud::Evidence;
use crate::codec::sha256;
use crate::content::{Opened, empty_file, open_manifest};
use crate::crypto::Key;
use crate::evidence::{Trust, Verified};
use crate::files::{BlobStore, MAX_BACKUP};
use crate::hold::Hold;
use crate::now_ms;
use crate::upload::{ChunkRef, UploadPlan};
use dmsg_types::{AccountId, Error, Hash, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use zeroize::Zeroizing;

pub(crate) const CLOUD: &str = "lib-cloud:";
const CURSOR: &str = "lib-cloud-cursor";
const OBSERVED: &str = "lib-cloud-observed";
/// Interrupted downloads stop holding chunks after this idle period.
const DOWNLOAD_HOLD_MS: u64 = 10 * dmsg_types::MINUTE;

/// The plan of one inventory entry of `account` and the device that
/// reserved it, checked against the certified evidence the relay kept from
/// the reservation, as a channel upload's descriptor is: the device was an
/// unrevoked content signer of the account then, whatever replaced it since.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for an entry that does not verify.
pub(crate) fn verify_entry(
    trust: &Trust,
    account: &AccountId,
    entry: &Json,
    now: u64,
) -> Result<(UploadPlan, String)> {
    let plan: UploadPlan =
        serde_json::from_value(entry["plan"].clone()).map_err(|_| Error::IntegrityFailed)?;
    let evidence: Evidence =
        serde_json::from_value(entry["evidence"].clone()).map_err(|_| Error::IntegrityFailed)?;
    let stored_at = entry["stored_at"].as_u64().ok_or(Error::IntegrityFailed)?;
    let verified: Verified<UploadPlan> = trust
        .command(
            entry["signature"]["cose_sign1"]
                .as_str()
                .unwrap_or_default(),
            &evidence,
            stored_at,
            Some("dmsg/upload/reserve/v1"),
            now,
        )
        .map_err(|_| Error::IntegrityFailed)?;
    let device = hex::encode(*verified.device);
    ensure(
        verified.account == *account
            && verified.body.payload == plan
            && entry["upload_id"] == plan.upload_id.as_str()
            && plan.kind == BACKUP
            && entry["device_id"] == device.as_str(),
        Error::IntegrityFailed,
    )?;
    Ok((plan, device))
}

/// A committed backup as this device knows it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct CloudEntry {
    pub(crate) plan: UploadPlan,
    pub(crate) device: String,
    pub(crate) committed_at: Option<u64>,
    /// The opened manifest; none while this device lacks the root of its
    /// generation.
    pub(crate) manifest: Option<BackupManifest>,
}

/// A backup in the cloud as it leaves the daemon: no key.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct CloudView {
    /// The upload.
    pub upload_id: String,
    /// File ID, once the manifest is open.
    pub file_id: Option<String>,
    /// Version ID, once the manifest is open.
    pub version_id: Option<String>,
    /// File name.
    pub name: Option<String>,
    /// The folders it was in, from the root.
    pub path: Vec<String>,
    /// Plaintext length.
    pub size: Option<u64>,
    /// Media type.
    pub mime: Option<String>,
    /// Plaintext SHA-256.
    pub sha256: Option<String>,
    /// Tags.
    pub tags: Vec<String>,
    /// Note.
    pub note: Option<String>,
    /// When the version was made.
    pub created_at: Option<u64>,
    /// The device that made it.
    pub device: String,
    /// The content-root generation the manifest is sealed under.
    pub generation: u64,
    /// When the relay committed it.
    pub committed_at: Option<u64>,
    /// Ciphertext bytes in the cloud.
    pub bytes: u64,
    /// This device has no root of that generation yet.
    pub needs_root: bool,
    /// The version is in this device's library, with its chunks.
    pub local: bool,
}

/// What a refresh of the inventory found.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct CloudRefresh {
    /// New backups seen.
    pub added: u64,
    /// Backups seen deleted.
    pub deleted: u64,
    /// Backups whose root this device lacks.
    pub needs_root: u64,
    /// The cursor afterwards.
    pub cursor: u64,
}

/// The cloud's room and what the library's backups take of it.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct CloudUsage {
    /// Bytes reserved by uploads in flight.
    pub reserved: u64,
    /// Bytes of committed objects.
    pub committed: u64,
    /// The plan's storage limit, when the relay knows it.
    pub storage_limit: Option<u64>,
    /// Current membership's maximum plaintext bytes per new cloud backup.
    pub backup_max_file_bytes: Option<u64>,
    /// `within_limit`, `over_limit` or `unknown`.
    pub status: String,
    /// The plan's active-channel limit, when the relay knows it.
    pub channel_limit: Option<u64>,
    /// Channels this account pays for.
    pub active_channels: Option<u64>,
    /// The relay's open over-limit notice: `id`, `detected_at`,
    /// `acknowledged_at`, `service_terminated_at` and
    /// `minimum_export_until`, in Unix milliseconds.
    pub notice: Option<Json>,
    /// Backups this device knows of.
    pub backups: u64,
    /// Their ciphertext bytes.
    pub backup_bytes: u64,
}

fn plan_bytes(plan: &UploadPlan) -> u64 {
    plan.chunks.iter().map(|c| c.size).sum::<u64>() + plan.manifest_size
}

impl Client {
    pub(crate) fn clear_cloud_inventory(&self) -> Result<()> {
        for key in self.workspace.store.sealed_keys(CLOUD)? {
            self.workspace.store.delete_sealed(&key)?;
        }
        for key in [CURSOR, OBSERVED] {
            self.workspace.store.delete_sealed(key)?;
        }
        Ok(())
    }

    /// Whether this device has completed a backup inventory observation.
    pub fn files_cloud_observation(&self) -> Result<Json> {
        let observed = self.workspace.private::<u64>(self.keys()?, OBSERVED)?;
        Ok(json!({"complete":observed.is_some(),"observed_at_ms":observed}))
    }

    /// Whether the backup inventory needs a visible-view refresh.
    pub fn files_cloud_stale(&self) -> Result<bool> {
        let observed = self
            .workspace
            .private::<u64>(self.keys()?, OBSERVED)?
            .unwrap_or(0);
        Ok(crate::display::stale(observed, 60_000, now_ms()))
    }

    fn cloud_entries(&self) -> Result<Vec<(String, CloudEntry)>> {
        let mut entries = Vec::new();
        for key in self.workspace.store.sealed_keys(CLOUD)? {
            if let Some(entry) = self.workspace.private::<CloudEntry>(self.keys()?, &key)? {
                entries.push((key[CLOUD.len()..].to_string(), entry));
            }
        }
        Ok(entries)
    }

    pub(crate) fn cloud_entry(&self, upload_id: &str) -> Result<Option<CloudEntry>> {
        self.workspace
            .private(self.keys()?, &format!("{CLOUD}{upload_id}"))
    }

    pub(crate) fn save_cloud_entry(&self, upload_id: &str, entry: &CloudEntry) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{CLOUD}{upload_id}"), entry)
    }

    /// Forget a cloud copy here: its cache, and the backup mark of the
    /// local version it was of.
    pub(crate) fn forget_cloud(&mut self, upload_id: &str) -> Result<()> {
        // Backup ownership is durable library state, independent of the
        // reconstructible inventory (which the person may have cleared).
        for key in self.workspace.store.sealed_keys(VERSION)? {
            let (file, id) = key[VERSION.len()..]
                .split_once(':')
                .ok_or(Error::IntegrityFailed)?;
            let mut version = self.version(file, id)?;
            if version
                .backup
                .as_ref()
                .is_some_and(|b| b.upload_id == upload_id)
            {
                version.backup = None;
                self.save_version(file, id, &version)?;
            }
        }
        self.workspace
            .delete_private(&format!("{CLOUD}{upload_id}"))
    }

    /// Mark a local version as backed up by `upload_id`, if it is here and
    /// not marked yet.
    pub(crate) fn mark_backed_up(
        &mut self,
        manifest: &BackupManifest,
        upload_id: &str,
        generation: u64,
        at: u64,
    ) -> Result<()> {
        let (file, version_id) = (&manifest.file.id, &manifest.file.version);
        let known = self
            .catalog()?
            .files
            .get(file)
            .is_some_and(|f| f.versions.iter().any(|v| v.id == *version_id));
        if !known {
            return Ok(());
        }
        let mut version = self.version(file, version_id)?;
        if version.backup.is_none() {
            version.backup = Some(Backup {
                upload_id: upload_id.into(),
                generation,
                backed_up_at: at,
                verified: false,
            });
            self.save_version(file, version_id, &version)?;
        }
        Ok(())
    }

    /// The backups this device knows of, by path and name.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn files_cloud_list(&mut self) -> Result<Vec<CloudView>> {
        let mut views = Vec::new();
        for (upload_id, entry) in self.cloud_entries()? {
            let local = match &entry.manifest {
                Some(manifest) => {
                    self.catalog()?
                        .files
                        .get(&manifest.file.id)
                        .is_some_and(|f| f.versions.iter().any(|v| v.id == manifest.file.version))
                        && self
                            .version(&manifest.file.id, &manifest.file.version)
                            .is_ok_and(|v| v.local)
                }
                None => false,
            };
            let manifest = entry.manifest.as_ref();
            views.push(CloudView {
                upload_id,
                file_id: manifest.map(|m| m.file.id.clone()),
                version_id: manifest.map(|m| m.file.version.clone()),
                name: manifest.map(|m| m.file.name.clone()),
                path: manifest.map(|m| m.path.clone()).unwrap_or_default(),
                size: manifest.map(|m| m.file.size),
                mime: manifest.map(|m| m.file.mime.clone()),
                sha256: manifest.map(|m| m.file.sha256.clone()),
                tags: manifest.map(|m| m.tags.clone()).unwrap_or_default(),
                note: manifest.map(|m| m.note.clone()),
                created_at: manifest.map(|m| m.created_at),
                device: entry.device.clone(),
                generation: entry.plan.root_generation,
                committed_at: entry.committed_at,
                bytes: plan_bytes(&entry.plan),
                needs_root: entry.manifest.is_none(),
                local,
            });
        }
        views.sort_by(|a, b| {
            (&a.path, &a.name, &a.created_at).cmp(&(&b.path, &b.name, &b.created_at))
        });
        Ok(views)
    }

    /// Keep a backup [`cloud_fetch`] downloaded: the version joins its file
    /// here, or a new file is made in `folder`, else where the backup says
    /// it was. A version already here gets its chunks back.
    fn fetched(
        &mut self,
        upload_id: &str,
        folder: Option<&str>,
        entry: &CloudEntry,
        manifest: &BackupManifest,
    ) -> Result<FileView> {
        let now = now_ms();
        let file = &manifest.file;
        let backup = Backup {
            upload_id: upload_id.into(),
            generation: entry.plan.root_generation,
            backed_up_at: entry.committed_at.unwrap_or(now),
            verified: true,
        };
        let chunks: Vec<ChunkRef> = file
            .chunks
            .iter()
            .map(|c| ChunkRef {
                digest: c.digest.clone(),
                size: c.size,
            })
            .collect();
        let existing = self.catalog()?.files.get(&file.id).cloned();
        match existing {
            Some(mut record) => {
                if record.versions.iter().any(|v| v.id == file.version) {
                    let mut version = self.version(&file.id, &file.version)?;
                    version.local = true;
                    version.backup = Some(backup);
                    self.save_version(&file.id, &file.version, &version)?;
                } else {
                    let version = Version {
                        size: file.size,
                        sha256: file.sha256.clone(),
                        created_at: manifest.created_at,
                        device: manifest.device.clone(),
                        origin: Origin::Fetched {
                            upload_id: upload_id.into(),
                        },
                        key: file.key.clone(),
                        chunks,
                        thumbnail: None,
                        local: true,
                        backup: Some(backup),
                    };
                    let newest = record
                        .versions
                        .iter()
                        .all(|v| v.created_at <= manifest.created_at);
                    record.versions.push(VersionRef {
                        id: file.version.clone(),
                        size: file.size,
                        sha256: file.sha256.clone(),
                        created_at: manifest.created_at,
                    });
                    record.versions.sort_by_key(|v| v.created_at);
                    if newest {
                        record.current = file.version.clone();
                    }
                    record.updated_at = now;
                    self.workspace.store.atomic(|_| {
                        self.save_version(&file.id, &file.version, &version)?;
                        self.workspace.put_private(
                            self.keys()?,
                            &format!("{FILE}{}", file.id),
                            &record,
                        )
                    })?;
                    self.catalog()?.put_file(&file.id, record);
                }
            }
            None => {
                let folder = match folder {
                    Some(folder) => folder.to_string(),
                    None if manifest.path.is_empty() => ROOT.into(),
                    None => self.files_mkdir_path(&manifest.path.join("/"))?.id,
                };
                let name = self.catalog()?.free_name(&folder, &safe_name(&file.name));
                let version = Version {
                    size: file.size,
                    sha256: file.sha256.clone(),
                    created_at: manifest.created_at,
                    device: manifest.device.clone(),
                    origin: Origin::Fetched {
                        upload_id: upload_id.into(),
                    },
                    key: file.key.clone(),
                    chunks,
                    thumbnail: None,
                    local: true,
                    backup: Some(backup),
                };
                let record = FileRecord {
                    folder,
                    name,
                    mime: file.mime.clone(),
                    tags: manifest.tags.clone(),
                    note: manifest.note.clone(),
                    starred: false,
                    created_at: manifest.created_at,
                    updated_at: now,
                    current: file.version.clone(),
                    versions: vec![VersionRef {
                        id: file.version.clone(),
                        size: file.size,
                        sha256: file.sha256.clone(),
                        created_at: manifest.created_at,
                    }],
                    trashed_at: None,
                };
                self.workspace.store.atomic(|_| {
                    self.save_version(&file.id, &file.version, &version)?;
                    self.workspace
                        .put_private(self.keys()?, &format!("{FILE}{}", file.id), &record)
                })?;
                self.catalog()?.put_file(&file.id, record);
            }
        }
        Ok(self.files_show(&file.id)?.file)
    }

    /// The chunks of a backup this device lacks, after checking the plan
    /// against the manifest.
    fn missing_chunks(
        &self,
        upload_id: &str,
        plan: &UploadPlan,
        manifest: &BackupManifest,
    ) -> Result<(u64, Vec<(String, String)>)> {
        let file = &manifest.file;
        ensure(file.size <= MAX_BACKUP, Error::IntegrityFailed)?;
        let empty = hex::encode(*sha256(&empty_file()));
        let planned: Vec<&str> = plan.chunks.iter().map(|c| c.digest.as_str()).collect();
        let own: Vec<&str> = file.chunks.iter().map(|c| c.digest.as_str()).collect();
        ensure(
            planned == own || (own.is_empty() && planned == [empty.as_str()]),
            Error::IntegrityFailed,
        )?;
        let objects = format!("{}/objects/{upload_id}/chunks", self.base()?);
        let mut missing = vec![];
        for (index, chunk) in file.chunks.iter().enumerate() {
            // This is an explicit restore/verify, not a list read. An existing
            // but corrupt block must be replaceable by the verified cloud copy.
            let present = match self.blobs()?.chunk(&chunk.digest) {
                Ok(bytes) => bytes.is_some(),
                Err(Error::IntegrityFailed) => {
                    self.blobs()?.delete(&chunk.digest)?;
                    false
                }
                Err(error) => return Err(error),
            };
            if !present {
                missing.push((format!("{objects}/{index}"), chunk.digest.clone()));
            }
        }
        Ok((file.chunks.len() as u64, missing))
    }

    /// Mark a version of a verified backup as checked, with its chunks here.
    fn verified(&mut self, upload_id: &str, manifest: &BackupManifest) -> Result<()> {
        let file = &manifest.file;
        if let Ok(mut version) = self.version(&file.id, &file.version) {
            version.local = true;
            if let Some(backup) = version.backup.as_mut().filter(|b| b.upload_id == upload_id) {
                backup.verified = true;
            }
            self.save_version(&file.id, &file.version, &version)?;
        }
        Ok(())
    }

    /// Check a deletion of cloud copies before anything goes: the copy a
    /// version released from this device reads from is its last one.
    pub(crate) fn deletable(&self, upload_ids: &[String]) -> Result<()> {
        ensure_valid(!upload_ids.is_empty(), "upload_ids")?;
        for key in self.workspace.store.sealed_keys(VERSION)? {
            let (file, id) = key[VERSION.len()..]
                .split_once(':')
                .ok_or(Error::IntegrityFailed)?;
            let version = self.version(file, id)?;
            let last = !version.local
                && version
                    .backup
                    .is_some_and(|b| upload_ids.contains(&b.upload_id));
            ensure_valid(
                !last,
                "upload_ids: the only copy of a version freed here; fetch it first",
            )?;
        }
        Ok(())
    }

    /// Keep one inventory entry the relay listed; whether its manifest must
    /// still be opened.
    fn inventory_entry(
        &mut self,
        report: &mut CloudRefresh,
        trust: &Trust,
        entry: &Json,
        cursor: &mut u64,
    ) -> Result<Option<(UploadPlan, String, Option<u64>)>> {
        let id = self.account()?.id;
        let at = entry["cursor"].as_u64().ok_or(Error::IntegrityFailed)?;
        ensure(at > *cursor, Error::IntegrityFailed)?;
        *cursor = at;
        let (plan, device) = verify_entry(trust, &id, entry, now_ms())?;
        let upload_id = plan.upload_id.clone();
        if entry["state"] != "committed" {
            if self.cloud_entry(&upload_id)?.is_some() {
                report.deleted += 1;
            }
            self.forget_cloud(&upload_id)?;
            return Ok(None);
        }
        if self.cloud_entry(&upload_id)?.is_some() {
            return Ok(None);
        }
        Ok(Some((plan, device, entry["committed_at"].as_u64())))
    }

    /// Cache a new inventory entry, its manifest opened when this device
    /// has the root of its generation.
    fn keep_entry(
        &mut self,
        report: &mut CloudRefresh,
        plan: UploadPlan,
        device: String,
        committed_at: Option<u64>,
        manifest: Option<BackupManifest>,
    ) -> Result<()> {
        let upload_id = plan.upload_id.clone();
        match &manifest {
            Some(manifest) => self.mark_backed_up(
                manifest,
                &upload_id,
                plan.root_generation,
                committed_at.unwrap_or(now_ms()),
            )?,
            None => report.needs_root += 1,
        }
        self.save_cloud_entry(
            &upload_id,
            &CloudEntry {
                plan,
                device,
                committed_at,
                manifest,
            },
        )?;
        report.added += 1;
        Ok(())
    }

    /// The root of `generation`, while this device has it.
    fn root_of(&self, generation: u64) -> Result<Option<Zeroizing<Key>>> {
        Ok(self.roots()?.key(generation).map(|k| Zeroizing::new(*k)))
    }

    /// Free a version's chunks on this device, keeping only its verified
    /// backup; reading it then needs a fetch. Thumbnails stay.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for a version without a verified backup.
    pub fn files_evict(&mut self, id: &str, version: Option<&str>) -> Result<FileView> {
        let (_, version_id, mut record) = self.pick_version(id, version)?;
        ensure_valid(
            record.backup.as_ref().is_some_and(|b| b.verified),
            "version has no verified backup",
        )?;
        record.local = false;
        self.save_version(id, &version_id, &record)?;
        self.collect_blobs()?;
        Ok(self.files_show(id)?.file)
    }
}

/// The manifest of a committed backup, opened with the root of its
/// generation; `None` while this device lacks that root.
async fn open_backup<H: Hold>(
    h: &H,
    account: &str,
    upload_id: &str,
    plan: &UploadPlan,
) -> Result<Option<BackupManifest>> {
    let Some(root) = h.with(|c| c.root_of(plan.root_generation)).await? else {
        return Ok(None);
    };
    let base = h.with(|c| c.base()).await?;
    let bytes = crate::downloads::one(
        h,
        format!("{base}/objects/{upload_id}/chunks/manifest"),
        &plan.manifest_digest,
    )
    .await?;
    match open_manifest(&root, account, plan, &bytes)? {
        Opened::Backup(manifest) => Ok(Some(manifest)),
        _ => Err(Error::IntegrityFailed),
    }
}

/// Read the relay's backup inventory from this device's cursor on: check
/// every entry's signed plan against the evidence of its reservation
/// ([`verify_entry`]), open each new manifest with the root of its
/// generation and cache it; forget the ones deleted. Cached entries that
/// lacked their root open once the root is here. A local version a backup
/// is of is marked backed up.
///
/// # Errors
/// Returns `Error::Locked` while locked and `Error::IntegrityFailed` for an
/// entry that does not verify.
pub async fn cloud_refresh<H: Hold>(h: &H) -> Result<CloudRefresh> {
    let scoped = crate::display::Scoped {
        session: h.with(|c| c.display_session()).await?,
        inner: h,
    };
    let h = &scoped;
    let (account, base, mut cursor, rootless) = h
        .with(|c| {
            c.roots()?;
            let cursor: u64 = c.workspace.private(c.keys()?, CURSOR)?.unwrap_or(0);
            // Entries an earlier refresh cached without their root.
            let rootless: Vec<String> = c
                .cloud_entries()?
                .into_iter()
                .filter(|(_, entry)| entry.manifest.is_none())
                .map(|(upload_id, _)| upload_id)
                .collect();
            Ok((c.account()?.id.to_string(), c.base()?, cursor, rootless))
        })
        .await?;
    let trust = h.trust().await?;
    let mut report = CloudRefresh::default();
    loop {
        let page = h
            .get(&format!("{base}/backups?after={cursor}&limit=100"))
            .await?;
        let entries = page["entries"].as_array().ok_or(Error::IntegrityFailed)?;
        ensure(entries.len() <= 100, Error::IntegrityFailed)?;
        for entry in entries {
            let Some((plan, device, committed_at)) = h
                .with(|c| c.inventory_entry(&mut report, &trust, entry, &mut cursor))
                .await?
            else {
                continue;
            };
            let manifest = open_backup(h, &account, &plan.upload_id, &plan).await?;
            h.with(|c| c.keep_entry(&mut report, plan, device, committed_at, manifest))
                .await?;
        }
        h.with(|c| c.workspace.put_private(c.keys()?, CURSOR, &cursor))
            .await?;
        if page["end"] == true {
            break;
        }
        ensure(!entries.is_empty(), Error::IntegrityFailed)?;
    }
    // Those still listed open once their root is here.
    for upload_id in rootless {
        let Some(entry) = h.with(|c| c.cloud_entry(&upload_id)).await? else {
            continue;
        };
        let Some(manifest) = open_backup(h, &account, &upload_id, &entry.plan).await? else {
            report.needs_root += 1;
            continue;
        };
        h.with(|c| {
            c.mark_backed_up(
                &manifest,
                &upload_id,
                entry.plan.root_generation,
                entry.committed_at.unwrap_or(now_ms()),
            )?;
            c.save_cloud_entry(
                &upload_id,
                &CloudEntry {
                    manifest: Some(manifest),
                    ..entry
                },
            )
        })
        .await?;
    }
    report.cursor = cursor;
    h.with(|c| c.workspace.put_private(c.keys()?, OBSERVED, &now_ms()))
        .await?;
    Ok(report)
}

/// Download the chunks of a backup this device lacks into the blob
/// directory, checking each against its digest and the whole against the
/// manifest.
async fn fetch_chunks<H: Hold>(
    h: &H,
    upload_id: &str,
    plan: &UploadPlan,
    manifest: &BackupManifest,
    progress: &mut (dyn FnMut(u64, u64) + Send),
) -> Result<String> {
    let hold = crate::upload::random_hex();
    let pin = |c: &Client, now: u64| {
        c.hold_chunks([(
            hold.clone(),
            now + DOWNLOAD_HOLD_MS,
            manifest
                .file
                .chunks
                .iter()
                .map(|chunk| chunk.digest.clone())
                .collect(),
        )])
    };
    let (count, missing) = h
        .with(|c| {
            let missing = c.missing_chunks(upload_id, plan, manifest)?;
            pin(c, now_ms())?;
            Ok(missing)
        })
        .await?;
    let mut done = count - missing.len() as u64;
    let total_bytes: u64 = plan.chunks.iter().map(|c| c.size).sum();
    let missing_bytes: u64 = missing
        .iter()
        .filter_map(|(path, _)| {
            path.rsplit('/')
                .next()?
                .parse::<usize>()
                .ok()
                .and_then(|i| plan.chunks.get(i))
        })
        .map(|c| c.size)
        .sum();
    let mut bytes_done = total_bytes.saturating_sub(missing_bytes);
    h.with(|c| c.fetch_progress(upload_id, done, bytes_done, "downloading", now_ms()))
        .await?;
    progress(done, count);
    crate::downloads::chunks(h, &missing, |c, group, fetched| {
        let now = now_ms();
        for ((_, digest), bytes) in group.iter().zip(fetched) {
            c.blobs()?.put_chunk(digest, &bytes)?;
            bytes_done += bytes.len() as u64;
        }
        pin(c, now)?;
        done += group.len() as u64;
        c.fetch_progress(
            upload_id,
            done,
            bytes_done,
            if done == count {
                "verifying"
            } else {
                "downloading"
            },
            now,
        )?;
        progress(done, count);
        Ok(())
    })
    .await?;
    h.with(|c| c.fetch_progress(upload_id, done, bytes_done, "verifying", now_ms()))
        .await?;
    super::reading::check_manifest(h, &manifest.file, MAX_BACKUP).await?;
    Ok(hold)
}

/// Fetch a backup into the library: its chunks are downloaded and checked,
/// then the version joins its file here, or a new file is made in
/// `folder`, else where the backup says it was. A version already here
/// gets its chunks back. Backed-up status is kept.
///
/// # Errors
/// Returns `Error::NotFound` for an unknown backup,
/// `Error::RecoveryIncomplete` for one whose root this device lacks and
/// `Error::IntegrityFailed` for anything that does not verify.
pub async fn cloud_fetch<H: Hold>(
    h: &H,
    upload_id: &str,
    folder: Option<&str>,
    progress: &mut (dyn FnMut(u64, u64) + Send),
) -> Result<FileView> {
    let entry = h
        .with(|c| {
            let entry = c.cloud_entry(upload_id)?.ok_or(Error::NotFound)?;
            if let Some(folder) = folder {
                c.catalog()?.live_folder(folder)?;
            }
            Ok(entry)
        })
        .await?;
    let manifest = entry.manifest.clone().ok_or(Error::RecoveryIncomplete)?;
    let hold = fetch_chunks(h, upload_id, &entry.plan, &manifest, progress).await?;
    h.with(|c| {
        let result = c.fetched(upload_id, folder, &entry, &manifest);
        c.release_chunks(&hold)?;
        result
    })
    .await
}

/// Delete cloud copies: an explicit, signed collection of each object on
/// the relay, which frees its quota. Files on this device stay; their
/// versions only lose the backup mark. The copy a version released from
/// this device ([`Client::files_evict`]) reads from is its last one, so it
/// is refused until the version is fetched back or deleted here.
///
/// # Errors
/// Returns `Error::InvalidInput` for such a last copy, before anything
/// goes. Relay refusals, such as an object an export lease still protects,
/// stop at that object; the ones before it are gone.
pub async fn cloud_delete<H: Hold>(h: &H, upload_ids: &[String]) -> Result<u64> {
    let base = h
        .with(|c| {
            c.deletable(upload_ids)?;
            c.base()
        })
        .await?;
    let mut deleted = 0;
    for upload_id in upload_ids {
        h.post(
            &format!("{base}/objects/collect"),
            "dmsg/object/collect/v1",
            &json!({"upload_id": upload_id, "generation": 0}),
            &Hash::new(crate::crypto::random_id()),
        )
        .await?;
        h.with(|c| c.forget_cloud(upload_id)).await?;
        deleted += 1;
    }
    Ok(deleted)
}

/// Download every chunk of a backup and check the whole file against its
/// manifest. A version of it on this device is then marked verified, with
/// its chunks present.
///
/// # Errors
/// As [`cloud_fetch`].
pub async fn cloud_verify<H: Hold>(
    h: &H,
    upload_id: &str,
    progress: &mut (dyn FnMut(u64, u64) + Send),
) -> Result<VersionCheck> {
    let entry = h
        .with(|c| c.cloud_entry(upload_id)?.ok_or(Error::NotFound))
        .await?;
    let manifest = entry.manifest.clone().ok_or(Error::RecoveryIncomplete)?;
    let checked = fetch_chunks(h, upload_id, &entry.plan, &manifest, progress).await;
    if let Ok(hold) = &checked {
        h.with(|c| {
            let result = c.verified(upload_id, &manifest);
            c.release_chunks(hold)?;
            result
        })
        .await?;
    }
    let file = &manifest.file;
    Ok(VersionCheck {
        file: file.id.clone(),
        version: file.version.clone(),
        ok: checked.is_ok(),
        error: checked.err().as_ref().map(crate::error_code),
    })
}

/// A queued verification retains the original error (including Retry-After),
/// instead of reducing it to the synchronous inspection RPC's display code.
pub(crate) async fn verify_download<H: Hold>(
    h: &H,
    upload_id: &str,
    progress: &mut (dyn FnMut(u64, u64) + Send),
) -> Result<()> {
    let entry = h
        .with(|c| c.cloud_entry(upload_id)?.ok_or(Error::NotFound))
        .await?;
    let manifest = entry.manifest.as_ref().ok_or(Error::RecoveryIncomplete)?;
    let hold = fetch_chunks(h, upload_id, &entry.plan, manifest, progress).await?;
    h.with(|c| {
        let result = c.verified(upload_id, manifest);
        c.release_chunks(&hold)?;
        result
    })
    .await
}

/// The cloud's room as the relay projects it, with what this device knows
/// of the backups in it.
///
/// # Errors
/// Relay failures; `Error::Locked` while locked.
pub async fn cloud_usage<H: Hold>(h: &H) -> Result<CloudUsage> {
    let base = h.with(|c| c.base()).await?;
    let quota = h.get(&format!("{base}/quota")).await?;
    h.with(|c| {
        let mut usage = CloudUsage {
            reserved: quota["reserved"].as_u64().unwrap_or(0),
            committed: quota["committed"].as_u64().unwrap_or(0),
            storage_limit: quota["lifecycle"]["storage_limit"].as_u64(),
            backup_max_file_bytes: quota["lifecycle"]["backup_max_file_bytes"].as_u64(),
            status: quota["lifecycle"]["status"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            channel_limit: quota["lifecycle"]["channel_limit"].as_u64(),
            active_channels: quota["lifecycle"]["active_channels"].as_u64(),
            notice: Some(&quota["lifecycle"]["notice"])
                .filter(|n| n.is_object() && n["resolved"] != true)
                .cloned(),
            ..CloudUsage::default()
        };
        ensure(c.base()? == base, Error::Locked)?;
        if let Some(limit) = usage.storage_limit {
            crate::sync::observe_storage(
                c,
                limit.saturating_sub(usage.reserved.saturating_add(usage.committed)),
            )?;
        }
        for (_, entry) in c.cloud_entries()? {
            usage.backups += 1;
            usage.backup_bytes += plan_bytes(&entry.plan);
        }
        Ok(usage)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn restoring_keeps_downloaded_chunks_through_concurrent_collection() {
        use std::sync::{
            Mutex,
            atomic::{AtomicBool, Ordering},
        };
        struct Collecting {
            client: Mutex<Client>,
            collecting: AtomicBool,
        }
        impl Hold for Collecting {
            async fn with<T: Send>(
                &self,
                step: impl FnOnce(&mut Client) -> Result<T> + Send,
            ) -> Result<T> {
                let mut c = self.client.lock().unwrap();
                let result = step(&mut c);
                if self.collecting.load(Ordering::Relaxed) {
                    c.collect_blobs()?;
                }
                result
            }
        }
        let dir = crate::testing::TempDir::new();
        let mut c = crate::testing::unlocked(&dir.0, false, None);
        let source = dir.path("restore.txt");
        std::fs::write(&source, b"resumed download").unwrap();
        let imported = c
            .files_import(&source, ROOT, false, &mut |_, _| {})
            .unwrap();
        let (file, mut version, manifest) = c.library_manifest(&imported.file.id, None).unwrap();
        let job = c
            .plan_backup(&imported.file.id, &imported.version, now_ms())
            .unwrap();
        let upload = job.plan.upload_id.clone();
        c.save_cloud_entry(
            &upload,
            &CloudEntry {
                plan: job.plan,
                device: version.device.clone(),
                committed_at: Some(7),
                manifest: Some(BackupManifest {
                    format: super::super::backup::BACKUP_FORMAT.into(),
                    file: manifest,
                    path: vec![],
                    tags: file.tags,
                    note: file.note,
                    created_at: version.created_at,
                    modified_at: file.updated_at,
                    device: version.device.clone(),
                }),
            },
        )
        .unwrap();
        // Bytes from an interrupted download exist, but the file is not yet local.
        version.local = false;
        c.save_version(&imported.file.id, &imported.version, &version)
            .unwrap();
        let h = Collecting {
            client: Mutex::new(c),
            collecting: AtomicBool::new(false),
        };
        let restored = cloud_fetch(&h, &upload, Some(ROOT), &mut |_, _| {
            h.collecting.store(true, Ordering::Relaxed);
        })
        .await
        .unwrap();
        assert!(restored.local);
        h.with(|c| {
            let out = dir.path("restored.txt");
            c.files_export(&imported.file.id, None, &out)?;
            assert_eq!(std::fs::read(out).unwrap(), b"resumed download");
            assert!(c.held_chunks(now_ms())?.is_empty());
            Ok(())
        })
        .await
        .unwrap();
    }

    #[test]
    fn explicit_restore_replaces_corrupt_blocks_but_listing_does_not() {
        let dir = crate::testing::TempDir::new();
        let mut client = crate::testing::unlocked(&dir.0, false, None);
        let path = dir.path("notes.txt");
        std::fs::write(&path, b"kept cloud copy").unwrap();
        let imported = client
            .files_import(&path, ROOT, false, &mut |_, _| {})
            .unwrap();
        let (file, version, manifest) = client.library_manifest(&imported.file.id, None).unwrap();
        let job = client
            .plan_backup(&imported.file.id, &imported.version, now_ms())
            .unwrap();
        let backup = BackupManifest {
            format: super::super::backup::BACKUP_FORMAT.into(),
            file: manifest.clone(),
            path: vec![],
            tags: file.tags,
            note: file.note,
            created_at: version.created_at,
            modified_at: file.updated_at,
            device: version.device,
        };
        let digest = &manifest.chunks[0].digest;
        let path = client
            .blobs()
            .unwrap()
            .dir()
            .join(&digest[..2])
            .join(digest);
        std::fs::write(&path, b"damaged").unwrap();
        client.files_list(ROOT, false).unwrap();
        assert!(path.exists(), "a list is not a repair or full-file scan");
        let (_, missing) = client
            .missing_chunks(&job.plan.upload_id, &job.plan, &backup)
            .unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].1, *digest);
        assert!(!path.exists());
    }
}
