//! Content sync with the relay (`docs/protocol/cloud_zh.md`).
//!
//! Every local version gets a job when it is written: its uploads, with the
//! manifests already sealed under the root, and a request ID. The job holds
//! only ciphertext, signatures and progress, so it lives in plain storage.
//! Pushing uploads the chunks (resuming after the ones stored), then signs
//! the vault revision together with the proofs to submit it ([`Presigned`]);
//! until that deadline the submission and its reconciliation need no key, so
//! it completes even when the device locks meanwhile. A version sealed under
//! an older root or with a lapsed upload is resealed only after the relay
//! confirms the old submission can no longer commit.
//!
//! Pulling reads a complete export snapshot: every signed entry is checked
//! against the certified devices, every chunk against its digest, and each
//! new version is opened before any head moves. Local versions the relay's
//! head does not lead to become conflict copies and are still sent, so the
//! relay keeps them for every device; it refuses only a tombstone or restore
//! against a moved head, and a deleted item takes nothing but an explicit
//! restore, so an older edit never revives it.

use crate::account::Security;
use crate::client::Client;
use crate::cloud::{Evidence, Presigned};
use crate::codec::{canonical, sha256};
use crate::content::{
    EncryptedObject, ManifestContent, Opened, VAULT, empty_file, object_container, open_manifest,
    read_container, record, seal_manifest,
};
use crate::evidence::{Trust, under};
use crate::files::{FileManifest, MAX_FILE, Source, read_file};
use crate::hold::Hold;
use crate::migration::MIGRATION_PART;
use crate::now_ms;
use crate::store::{BLOCKED, LOCAL, ObjectRow, REPLACED, SYNCED};
use crate::upload::{ChunkRef, UploadPlan, hex_id, random_hex};
use dmsg_types::{AccountId, Error, Hash, Result, ensure, user::VaultWriteState};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_bytes::ByteBuf;
use serde_json::Value as Json;
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

const MAX_PLANS: usize = 10_000;
const MAX_REVISIONS: usize = 100_000;
const SNAPSHOT: &str = "snapshot-through";
const LAST_SYNC: &str = "last-sync";

/// A vault revision as the device signs it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Revision {
    item_id: String,
    revision_id: String,
    base_revision: Option<String>,
    upload_id: Option<String>,
    tombstone: bool,
    restore: bool,
}

/// The relay's verdict on a revision.
#[derive(Deserialize)]
struct Verdict {
    revision_id: String,
    head: String,
    conflict: bool,
    tombstone: bool,
}

/// One upload of a job and how far it got.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct UploadJob {
    plan: UploadPlan,
    manifest: ByteBuf,
    sent: u64,
    done: bool,
}

/// What it takes to send one local version.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct ContentJob {
    record_digest: String,
    request_id: Hash,
    restore: bool,
    file: Option<UploadJob>,
    vault: UploadJob,
    dispatch: Option<Presigned>,
}

/// The verified relay snapshot, before it is applied.
struct Snapshot {
    through: u64,
    plans: Vec<UploadPlan>,
    revisions: Vec<(Revision, bool)>,
}

#[derive(Serialize)]
struct ExportStart<'a> {
    export_id: &'a str,
}

/// The payload of a snapshot entry's command, checked against the certified
/// security of the epoch it was signed under, as the relay checked it when it
/// accepted the command; the evidence of each epoch is on the page and is
/// checked once per pull. Devices replaced since, by recovery or at the
/// device bound, still count for what they signed then.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for an entry that does not verify.
#[allow(clippy::too_many_arguments)]
fn verify_signed_entry<P: DeserializeOwned + Serialize>(
    trust: &Trust,
    account: &AccountId,
    epochs: &mut BTreeMap<u64, Security>,
    page: &Json,
    entry: &Json,
    cose: &str,
    action: &str,
    now: u64,
) -> Result<P> {
    let epoch = entry["security_epoch"]
        .as_u64()
        .ok_or(Error::IntegrityFailed)?;
    let security = match epochs.entry(epoch) {
        Entry::Occupied(checked) => checked.into_mut(),
        Entry::Vacant(slot) => {
            let kept = &page["security"][epoch.to_string()];
            let evidence: Evidence = serde_json::from_value(kept["evidence"].clone())
                .map_err(|_| Error::IntegrityFailed)?;
            let at = kept["stored_at"].as_u64().ok_or(Error::IntegrityFailed)?;
            slot.insert(
                trust
                    .epoch(&evidence, account, epoch, at, now)
                    .map_err(|_| Error::IntegrityFailed)?,
            )
        }
    };
    let (_, body) = under::<P>(security, cose, Some(action)).map_err(|_| Error::IntegrityFailed)?;
    Ok(body.payload)
}

/// What one sync did.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Local versions the relay accepted.
    pub sent: usize,
    /// Local versions that lost to the relay and stay conflict copies.
    pub blocked: usize,
    /// Revisions in the verified snapshot; none while locked.
    pub revisions: usize,
    /// Snapshot position.
    pub through: u64,
    /// Local versions still waiting.
    pub pending: u64,
    /// Conflict copies waiting for a decision.
    pub conflicts: u64,
    /// Local chunks nothing needed any more, now deleted.
    pub collected: u64,
}

/// The latest sync attempt.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LastSync {
    /// When it ended, in Unix milliseconds.
    pub at: u64,
    /// Its error code, if it failed.
    pub error: Option<String>,
}

fn plan_of(record: &EncryptedObject, upload_id: String, generation: u64, now: u64) -> UploadPlan {
    UploadPlan {
        upload_id,
        object_id: record.id.clone(),
        version_id: record.revision.clone(),
        root_generation: generation,
        epoch: 0,
        control_head: None,
        kind: VAULT.into(),
        chunks: vec![],
        manifest_digest: String::new(),
        manifest_size: 0,
        expires_at: now + dmsg_types::DAY,
    }
}

/// The file a version of `kind` carries: a legacy archive part's. Vault
/// items carry none; the file library keeps its files out of the vault.
fn record_file(kind: &str, payload: &[u8]) -> Result<Option<FileManifest>> {
    match kind {
        MIGRATION_PART => crate::migration::part_file(payload).map(Some),
        _ => Ok(None),
    }
}

/// The parent revision of a stored version.
fn record_parent(row: &ObjectRow) -> Option<String> {
    record(row).ok().and_then(|r| r.parent)
}

/// Local versions, parents before the versions that replace them.
fn ordered(rows: Vec<ObjectRow>) -> Result<Vec<ObjectRow>> {
    let pending: BTreeSet<String> = rows.iter().map(|r| r.revision.clone()).collect();
    let mut done: BTreeSet<String> = BTreeSet::new();
    let mut ordered = Vec::with_capacity(rows.len());
    let mut rest = rows;
    while !rest.is_empty() {
        let before = rest.len();
        let (ready, waiting): (Vec<_>, Vec<_>) = rest.into_iter().partition(|row| {
            let parent = record(row).ok().and_then(|r| r.parent);
            parent.is_none_or(|p| !pending.contains(&p) || done.contains(&p))
        });
        done.extend(ready.iter().map(|r| r.revision.clone()));
        ordered.extend(ready);
        rest = waiting;
        ensure(rest.len() < before, Error::IntegrityFailed)?;
    }
    Ok(ordered)
}

impl Client {
    /// The job of a new version: its uploads with manifests sealed under the
    /// version's root, and its request ID.
    pub(crate) fn prepare_job(
        &self,
        record: &EncryptedObject,
        file: Option<&FileManifest>,
    ) -> Result<ContentJob> {
        let restore = !record.tombstone
            && match &record.parent {
                Some(parent) => self
                    .workspace
                    .store
                    .object(&format!("{}:{parent}", record.id))?
                    .map(|row| crate::content::record(&row))
                    .transpose()?
                    .is_some_and(|p| p.tombstone),
                None => false,
            };
        let mut job = ContentJob {
            record_digest: record.digest.clone(),
            request_id: Hash::new(crate::crypto::random_id()),
            restore,
            file: None,
            vault: UploadJob {
                plan: plan_of(record, String::new(), record.generation, 0),
                manifest: ByteBuf::new(),
                sent: 0,
                done: false,
            },
            dispatch: None,
        };
        self.seal_uploads(&mut job, record, file, true, true)?;
        Ok(job)
    }

    /// Plan, or replan under new upload IDs, the job's uploads for the
    /// current root. The object and its file chunks stay the same.
    fn seal_uploads(
        &self,
        job: &mut ContentJob,
        record: &EncryptedObject,
        file: Option<&FileManifest>,
        vault: bool,
        files: bool,
    ) -> Result<()> {
        let account = self.account()?.id.to_string();
        let roots = self.roots()?;
        let generation = roots.generation;
        let root = roots.key(generation).ok_or(Error::RecoveryIncomplete)?;
        let now = now_ms();
        if let (Some(file), true) = (file, files) {
            let empty = empty_file();
            let chunks = if file.chunks.is_empty() {
                let digest = hex::encode(*sha256(&empty));
                self.workspace.store.put_chunk(&digest, &empty)?;
                vec![ChunkRef {
                    digest,
                    size: empty.len() as u64,
                }]
            } else {
                file.chunks
                    .iter()
                    .map(|c| {
                        let size = self
                            .workspace
                            .store
                            .chunk(&c.digest)?
                            .ok_or(Error::RecoveryIncomplete)?
                            .len();
                        Ok(ChunkRef {
                            digest: c.digest.clone(),
                            size: size as u64,
                        })
                    })
                    .collect::<Result<_>>()?
            };
            let mut plan = UploadPlan {
                upload_id: random_hex(),
                object_id: file.id.clone(),
                version_id: file.version.clone(),
                root_generation: generation,
                epoch: 0,
                control_head: None,
                kind: "file".into(),
                chunks,
                manifest_digest: String::new(),
                manifest_size: 0,
                expires_at: now + dmsg_types::DAY,
            };
            let manifest = seal_manifest(root, &account, &mut plan, ManifestContent::File(file));
            job.file = Some(UploadJob {
                plan,
                manifest: ByteBuf::from(manifest),
                sent: 0,
                done: false,
            });
        }
        if vault {
            let object = object_container(record);
            let digest = hex::encode(*sha256(&object));
            self.workspace.store.put_chunk(&digest, &object)?;
            let mut plan = plan_of(record, random_hex(), generation, now);
            plan.chunks = vec![ChunkRef {
                digest,
                size: object.len() as u64,
            }];
            let manifest = seal_manifest(root, &account, &mut plan, ManifestContent::None);
            job.vault = UploadJob {
                plan,
                manifest: ByteBuf::from(manifest),
                sent: 0,
                done: false,
            };
            // The revision names the upload: a new upload is a new request.
            job.request_id = Hash::new(crate::crypto::random_id());
            job.dispatch = None;
        }
        Ok(())
    }

    /// The file manifest a version carries.
    fn version_file(&self, record: &EncryptedObject) -> Result<Option<FileManifest>> {
        match record.kind.as_str() {
            VAULT | MIGRATION_PART => record_file(
                &record.kind,
                &record.open(self.environment(), self.roots()?)?,
            ),
            _ => Ok(None),
        }
    }

    /// Read the relay snapshot, send local versions, and read it again when
    /// anything changed. Locked with a resident device key ("后台在线"),
    /// it only sends versions that need no reseal and caches ciphertext.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked without a resident key,
    /// `Error::RekeyRequired` while the account waits for a new root and
    /// `Error::DeviceNotApproved` for a revoked device, whose workspace is
    /// then erased.
    pub async fn sync(&mut self) -> Result<SyncReport> {
        let result = self.sync_once().await;
        // The relay can know of a revocation before the evidence this device
        // cached expires: read the account again, and erase it if revoked.
        if matches!(result, Err(Error::DeviceNotApproved)) {
            let account = self.account()?.clone();
            let security = self.refresh_security(account.home, &account.id).await?;
            self.check_revoked(&security)?;
        }
        let last = LastSync {
            at: now_ms(),
            error: result.as_ref().err().map(crate::error_code),
        };
        self.workspace.store.set_meta(LAST_SYNC, &last)?;
        result
    }

    /// The latest sync attempt.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn last_sync(&self) -> Result<Option<LastSync>> {
        self.workspace.store.meta(LAST_SYNC)
    }

    async fn sync_once(&mut self) -> Result<SyncReport> {
        self.signing()?;
        let security = self.cloud_state().await?;
        self.check_revoked(&security)?;
        let unlocked = self.keys.is_some();
        if unlocked {
            self.roots()?;
        }
        self.reconcile().await?;
        let mut report = SyncReport::default();
        let mut snapshot = self.fetch().await?;
        if unlocked {
            report.revisions = self.apply(&snapshot)?;
        }
        report.through = snapshot.through;
        let (sent, blocked) = self.push().await?;
        if unlocked && sent + blocked > 0 {
            snapshot = self.fetch().await?;
            report.revisions = self.apply(&snapshot)?;
            report.through = snapshot.through;
        }
        report.sent = sent;
        report.blocked = blocked;
        report.pending = self.workspace.store.count_in(LOCAL)?;
        report.conflicts = self.workspace.store.count_conflicts(VAULT)?;
        if unlocked {
            // Housekeeping: a failure keeps every chunk until the next sync.
            report.collected = self.collect_chunks(&snapshot, now_ms()).unwrap_or(0);
        }
        Ok(report)
    }

    /// Delete the local chunks nothing needs any more: those of versions
    /// replaced before they were sent, of uploads that lapsed or were
    /// abandoned, and of attachments a channel download cached. Kept are
    /// the chunks of the relay snapshot just applied, since every pull
    /// checks them all and would download a missing one again; of every
    /// unsent version's job, which replans from them; and of each channel
    /// upload and root upload that may still be sent before its plan
    /// lapses. The file library's chunks live in its own directory and are
    /// collected by [`crate::library::gc`]. Returns how many were deleted.
    fn collect_chunks(&self, snapshot: &Snapshot, now: u64) -> Result<u64> {
        let mut keep = self.held_chunks(now)?;
        let mut add = |plan: &UploadPlan| {
            keep.extend(plan.chunks.iter().map(|c| c.digest.clone()));
            keep.insert(plan.manifest_digest.clone());
        };
        snapshot.plans.iter().for_each(&mut add);
        for (_, job) in self.workspace.store.jobs::<ContentJob>()? {
            job.file
                .iter()
                .chain([&job.vault])
                .for_each(|u| add(&u.plan));
        }
        if let Some(plan) = self.root_plan()?.filter(|p| p.expires_at > now) {
            add(&plan);
        }
        let garbage: Vec<String> = self
            .workspace
            .store
            .chunk_digests()?
            .into_iter()
            .filter(|d| !keep.contains(d))
            .collect();
        self.workspace.store.delete_chunks(&garbage)?;
        Ok(garbage.len() as u64)
    }

    /// [`flush`] for a caller that holds the client throughout.
    ///
    /// # Errors
    /// As [`flush`].
    pub async fn flush(&mut self) -> Result<usize> {
        flush(&self.hold()).await
    }

    /// What [`flush`] submits, by job: the presigned revisions whose
    /// deadline has not passed. They need no key.
    fn flush_due(&self) -> Result<Vec<(String, Presigned)>> {
        let now = now_ms();
        Ok(self
            .workspace
            .store
            .jobs::<ContentJob>()?
            .into_iter()
            .filter_map(|(key, job)| Some((key, job.dispatch.filter(|d| d.deadline > now)?)))
            .collect())
    }

    /// Apply the relay's answer to a submission of [`Self::flush_due`],
    /// unless its job changed meanwhile; whether it settled.
    fn flush_settle(&self, key: &str, dispatch: &Presigned, sent: Result<Json>) -> Result<bool> {
        let job: Option<ContentJob> = self.workspace.store.job(key)?;
        if job.and_then(|j| j.dispatch).as_ref() != Some(dispatch) {
            return Ok(false);
        }
        match sent {
            Ok(verdict) => self.settle(key, &verdict).map(|()| true),
            Err(Error::VersionConflict) => self.block(key).map(|()| true),
            Err(_) => Ok(false),
        }
    }

    /// Settle every submitted revision the relay already decided, with
    /// fresh proofs; a lapsed submission that never committed stays queued
    /// and is signed again.
    async fn reconcile(&mut self) -> Result<()> {
        self.flush().await?;
        let base = self.base()?;
        for (key, job) in self.workspace.store.jobs::<ContentJob>()? {
            if job.dispatch.is_none() {
                continue;
            }
            let path = format!("{base}/operations/{}", hex::encode(*job.request_id));
            if let Some(found) = self.relay_find(&path).await?.filter(|f| f["found"] == true) {
                self.settle(&key, &found["result"])?;
            }
        }
        Ok(())
    }

    /// Apply the relay's verdict on a version.
    fn settle(&self, key: &str, verdict: &Json) -> Result<()> {
        let verdict: Verdict =
            serde_json::from_value(verdict.clone()).map_err(|_| Error::IntegrityFailed)?;
        let row = self
            .workspace
            .store
            .object(key)?
            .ok_or(Error::IntegrityFailed)?;
        let stored = record(&row)?;
        hex_id(&verdict.head)?;
        ensure(
            verdict.revision_id == row.revision
                && verdict.tombstone == stored.tombstone
                && (verdict.conflict || verdict.head == row.revision),
            Error::IntegrityFailed,
        )?;
        self.workspace.store.atomic(|store| {
            store.set_object(key, SYNCED, verdict.conflict)?;
            store.delete_job(key)?;
            // A conflict copy steps aside for the relay's head, once known.
            if verdict.conflict
                && store.head(&row.item)?.as_deref() == Some(&row.revision)
                && store
                    .object(&format!("{}:{}", row.item, verdict.head))?
                    .is_some()
            {
                store.set_head(&row.item, &verdict.head, &row.kind)?;
            }
            Ok(())
        })
    }

    /// The relay refused a version against its head: keep it as a local
    /// conflict copy and step the head back to its parent.
    fn block(&self, key: &str) -> Result<()> {
        let row = self
            .workspace
            .store
            .object(key)?
            .ok_or(Error::IntegrityFailed)?;
        let parent = record(&row)?.parent;
        self.workspace.store.atomic(|store| {
            store.set_object(key, BLOCKED, true)?;
            store.delete_job(key)?;
            if store.head(&row.item)?.as_deref() == Some(&row.revision)
                && let Some(parent) = &parent
            {
                store.set_head(&row.item, parent, &row.kind)?;
            }
            Ok(())
        })
    }

    /// Send every local version, parents first. Returns how many the relay
    /// accepted and how many it refused.
    async fn push(&mut self) -> Result<(usize, usize)> {
        let (mut sent, mut blocked) = (0, 0);
        for row in ordered(self.workspace.store.objects_in(LOCAL)?)? {
            // A parent refused earlier in this run takes its children along.
            if self
                .workspace
                .store
                .object(&row.key)?
                .is_none_or(|r| r.state != LOCAL)
            {
                continue;
            }
            match self.push_one(row).await {
                Ok(true) => sent += 1,
                Ok(false) => blocked += 1,
                // Locked with a resident key: this one needs the root.
                Err(Error::Locked) if self.keys.is_none() => {}
                Err(error) => return Err(error),
            }
        }
        Ok((sent, blocked))
    }

    /// Send one local version; `false` when the relay refused it.
    async fn push_one(&mut self, mut row: ObjectRow) -> Result<bool> {
        let security = self.cloud_state().await?;
        let snapshot = &security.snapshot;
        ensure(
            snapshot.vault_write_state == VaultWriteState::Ready,
            Error::RekeyRequired,
        )?;
        let generation = snapshot.content_root_generation;
        let mut record = record(&row)?;
        let job = match self.workspace.store.job::<ContentJob>(&row.key)? {
            Some(job) => job,
            None => {
                let job = self.prepare_job(&record, self.version_file(&record)?.as_ref())?;
                self.workspace.store.set_job(&row.key, &job)?;
                job
            }
        };
        ensure(
            job.record_digest == record.digest,
            Error::IdempotencyConflict,
        )?;
        let now = now_ms();
        // Anything below changes what was signed, so the old submission must
        // be unable to commit: past its deadline, and not stored.
        let lapsed = job.dispatch.as_ref().is_none_or(|d| d.deadline <= now);
        let parent_replaced = match &record.parent {
            Some(parent) => self
                .workspace
                .store
                .object(&format!("{}:{parent}", record.id))?
                .is_some_and(|p| p.state == REPLACED),
            None => false,
        };
        if record.generation != generation || parent_replaced {
            ensure(lapsed, Error::ExecutionUnknown)?;
            self.roots()?;
            for upload in job.file.iter().chain([&job.vault]) {
                if !upload.done {
                    self.cancel_upload(&upload.plan.upload_id).await?;
                }
            }
            // Of a chain never sent, only its newest version is resealed:
            // the others are replaced by it.
            let child = self.workspace.store.objects_in(LOCAL)?.iter().any(|r| {
                r.item == record.id && record_parent(r).as_deref() == Some(&record.revision)
            });
            if child {
                self.workspace.store.set_object(&row.key, REPLACED, false)?;
                self.workspace.store.delete_job(&row.key)?;
                return Ok(true);
            }
            let file = self.version_file(&record)?;
            let payload = record.open(self.environment(), self.roots()?)?;
            let parent = self.sent_ancestor(&record.id, record.parent.clone())?;
            let (replacement, _) = self.reseal(&row, &record, parent, &payload, file.as_ref())?;
            row = self
                .workspace
                .store
                .object(&replacement.key)?
                .ok_or(Error::IntegrityFailed)?;
            record = replacement;
        }
        for file in [true, false] {
            self.send_upload(&row.key, &record, file, lapsed).await?;
        }
        let mut job: ContentJob = self
            .workspace
            .store
            .job(&row.key)?
            .ok_or(Error::IntegrityFailed)?;
        let base = self.base()?;
        let fresh = job
            .dispatch
            .as_ref()
            .filter(|d| d.deadline > now_ms() + 5_000);
        let dispatch = match fresh {
            Some(dispatch) => dispatch.clone(),
            None => {
                let revision = Revision {
                    item_id: record.id.clone(),
                    revision_id: record.revision.clone(),
                    base_revision: record.parent.clone(),
                    upload_id: (!record.tombstone).then(|| job.vault.plan.upload_id.clone()),
                    tombstone: record.tombstone,
                    restore: job.restore,
                };
                let security = self.cloud_state().await?;
                let request = hex::encode(*job.request_id);
                let dispatch = self.cloud.presign(
                    &self.signer(&security)?,
                    &format!("{base}/vault"),
                    "dmsg/vault/revision/v1",
                    &revision,
                    &job.request_id,
                    &format!("{base}/operations/{request}"),
                    now_ms(),
                )?;
                job.dispatch = Some(dispatch.clone());
                self.workspace.store.set_job(&row.key, &job)?;
                dispatch
            }
        };
        match self.cloud.submit(&dispatch, now_ms()).await {
            Ok(verdict) => self.settle(&row.key, &verdict).map(|_| true),
            Err(Error::VersionConflict) => self.block(&row.key).map(|_| false),
            Err(error) => Err(error),
        }
    }

    /// Finish the job's file or vault upload, resuming after the chunks the
    /// job recorded; a lapsed upload is replanned once under a new ID.
    async fn send_upload(
        &mut self,
        key: &str,
        record: &EncryptedObject,
        file: bool,
        lapsed: bool,
    ) -> Result<()> {
        let mut replanned = false;
        loop {
            let job: ContentJob = self
                .workspace
                .store
                .job(key)?
                .ok_or(Error::IntegrityFailed)?;
            let upload = if file { job.file } else { Some(job.vault) };
            let Some(upload) = upload.filter(|u| !u.done) else {
                return Ok(());
            };
            let progress = |client: &Self, sent: usize| {
                let mut job: ContentJob = client
                    .workspace
                    .store
                    .job(key)?
                    .ok_or(Error::IntegrityFailed)?;
                match file {
                    true => job.file.as_mut().ok_or(Error::IntegrityFailed)?.sent = sent as u64,
                    false => job.vault.sent = sent as u64,
                }
                client.workspace.store.set_job(key, &job)
            };
            let base = self.base()?;
            match self
                .upload(
                    &base,
                    &upload.plan,
                    &upload.manifest,
                    upload.sent as usize,
                    Source::Store,
                    progress,
                )
                .await
            {
                Ok(()) => {
                    let mut job: ContentJob = self
                        .workspace
                        .store
                        .job(key)?
                        .ok_or(Error::IntegrityFailed)?;
                    match file {
                        true => job.file.as_mut().ok_or(Error::IntegrityFailed)?.done = true,
                        false => job.vault.done = true,
                    }
                    return self.workspace.store.set_job(key, &job);
                }
                // A new vault upload changes the signed revision.
                Err(Error::ResultExpired) if !replanned && (file || lapsed) => {
                    replanned = true;
                    self.cancel_upload(&upload.plan.upload_id).await?;
                    let manifest = self.version_file(record)?;
                    let mut job: ContentJob = self
                        .workspace
                        .store
                        .job(key)?
                        .ok_or(Error::IntegrityFailed)?;
                    self.seal_uploads(&mut job, record, manifest.as_ref(), !file, file)?;
                    self.workspace.store.set_job(key, &job)?;
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// The nearest ancestor from `parent` on that was sent, skipping
    /// versions replaced before they were.
    fn sent_ancestor(&self, item: &str, mut parent: Option<String>) -> Result<Option<String>> {
        while let Some(revision) = &parent {
            match self.workspace.store.object(&format!("{item}:{revision}"))? {
                Some(row) if row.state == REPLACED => parent = record(&row)?.parent,
                _ => break,
            }
        }
        Ok(parent)
    }

    /// Reseal a version as a new version under the current root, with the
    /// same payload and `parent`, its nearest sent ancestor.
    fn reseal(
        &mut self,
        row: &ObjectRow,
        record: &EncryptedObject,
        parent: Option<String>,
        payload: &[u8],
        file: Option<&FileManifest>,
    ) -> Result<(EncryptedObject, bool)> {
        let head = self.workspace.store.head(&record.id)?;
        self.workspace.store.set_object(&row.key, REPLACED, false)?;
        self.workspace.store.delete_job(&row.key)?;
        // The replacement takes the replaced version's place as head.
        if head.as_deref() == Some(&record.revision) {
            match &parent {
                Some(parent) => self
                    .workspace
                    .store
                    .set_head(&record.id, parent, &record.kind)?,
                None => self.workspace.store.delete_head(&record.id)?,
            }
        }
        self.write(
            &record.kind,
            record.id.clone(),
            parent,
            record.tombstone,
            payload,
            file,
        )
    }

    /// Read and verify the relay's export snapshot, caching every vault and
    /// file chunk it lacks; nothing is opened yet.
    async fn fetch(&mut self) -> Result<Snapshot> {
        let id = self.account()?.id;
        let account = id.to_string();
        let trust = self.trust().await?;
        let base = self.base()?;
        let export_id = random_hex();
        let start = self
            .relay_post(
                &format!("{base}/exports"),
                "dmsg/export/start/v1",
                &ExportStart {
                    export_id: &export_id,
                },
                &Hash::new(crate::crypto::random_id()),
            )
            .await?;
        let through = start["through"].as_u64().ok_or(Error::IntegrityFailed)?;
        ensure(
            start["account"] == account.as_str() && start["scope"] == "cloud_snapshot",
            Error::IntegrityFailed,
        )?;
        let mut cursor = 0u64;
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        let mut snapshot = Snapshot {
            through,
            plans: vec![],
            revisions: vec![],
        };
        let mut epochs: BTreeMap<u64, Security> = BTreeMap::new();
        loop {
            let page = self
                .relay_get(&format!(
                    "{base}/exports/{export_id}?after={cursor}&limit=100"
                ))
                .await?;
            let now = now_ms();
            let entries = page["entries"].as_array().ok_or(Error::IntegrityFailed)?;
            ensure(entries.len() <= 100, Error::IntegrityFailed)?;
            let mut fetched = Vec::new();
            for entry in entries {
                let at = entry["cursor"].as_u64().ok_or(Error::IntegrityFailed)?;
                ensure(at > cursor && at <= through, Error::IntegrityFailed)?;
                cursor = at;
                let kind = entry["kind"].as_str().ok_or(Error::IntegrityFailed)?;
                *counts.entry(kind.into()).or_default() += 1;
                match kind {
                    "object" => {
                        ensure(
                            entry["missing_chunks"].as_array().is_none_or(Vec::is_empty),
                            Error::RecoveryIncomplete,
                        )?;
                        let plan: UploadPlan = serde_json::from_value(entry["plan"].clone())
                            .map_err(|_| Error::IntegrityFailed)?;
                        let signed: UploadPlan = verify_signed_entry(
                            &trust,
                            &id,
                            &mut epochs,
                            &page,
                            entry,
                            entry["signature"]["cose_sign1"]
                                .as_str()
                                .unwrap_or_default(),
                            "dmsg/upload/reserve/v1",
                            now,
                        )?;
                        ensure(
                            signed == plan
                                && entry["value"]["upload_id"] == plan.upload_id.as_str()
                                && entry["value"]["digest"] == plan.manifest_digest.as_str(),
                            Error::IntegrityFailed,
                        )?;
                        if matches!(plan.kind.as_str(), "vault" | "file") {
                            ensure(
                                snapshot.plans.len() + fetched.len() < MAX_PLANS,
                                Error::QuotaExceeded,
                            )?;
                            fetched.push(plan);
                        }
                    }
                    "revision" => {
                        let value = &entry["value"];
                        let revision: Revision = verify_signed_entry(
                            &trust,
                            &id,
                            &mut epochs,
                            &page,
                            entry,
                            value["signed"]["cose_sign1"].as_str().unwrap_or_default(),
                            "dmsg/vault/revision/v1",
                            now,
                        )?;
                        let fields =
                            serde_json::to_value(&revision).map_err(|_| Error::IntegrityFailed)?;
                        for (name, field) in fields.as_object().into_iter().flatten() {
                            ensure(value[name] == *field, Error::IntegrityFailed)?;
                        }
                        let conflict = value["conflict"].as_bool().ok_or(Error::IntegrityFailed)?;
                        ensure(
                            snapshot.revisions.len() < MAX_REVISIONS,
                            Error::QuotaExceeded,
                        )?;
                        snapshot.revisions.push((revision, conflict));
                    }
                    _ => {}
                }
            }
            self.cache(&fetched).await?;
            snapshot.plans.extend(fetched);
            let end = page["inventory_end"]
                .as_bool()
                .ok_or(Error::IntegrityFailed)?;
            ensure(
                page["next_cursor"].as_u64() == Some(cursor) && (end || !entries.is_empty()),
                Error::IntegrityFailed,
            )?;
            if end {
                break;
            }
        }
        for count in start["counts"].as_array().ok_or(Error::IntegrityFailed)? {
            let kind = count["kind"].as_str().ok_or(Error::IntegrityFailed)?;
            ensure(
                counts.get(kind).copied().unwrap_or(0) == count["n"].as_u64().unwrap_or(u64::MAX),
                Error::RecoveryIncomplete,
            )?;
        }
        Ok(snapshot)
    }

    /// Download the manifests and chunks of committed uploads that the
    /// store lacks; every one is checked against its digest.
    async fn cache(&mut self, plans: &[UploadPlan]) -> Result<()> {
        let base = self.base()?;
        let mut seen = BTreeSet::new();
        let mut missing = Vec::new();
        for plan in plans {
            let parts = plan
                .chunks
                .iter()
                .enumerate()
                .map(|(i, c)| (i.to_string(), &c.digest))
                .chain([("manifest".to_string(), &plan.manifest_digest)]);
            for (name, digest) in parts {
                if seen.insert(digest) && !self.workspace.store.has_chunk(digest)? {
                    let path = format!("{base}/objects/{}/chunks/{name}", plan.upload_id);
                    missing.push((path, digest.clone()));
                }
            }
        }
        self.download_chunks(Source::Store, &missing, |_| {}).await
    }

    /// Open every new version of a fetched snapshot, then move the heads in
    /// one transaction. Returns the number of revisions.
    fn apply(&mut self, snapshot: &Snapshot) -> Result<usize> {
        let account = self.account()?.id.to_string();
        let store = &self.workspace.store;
        let mut records: BTreeMap<String, EncryptedObject> = BTreeMap::new();
        let mut files: BTreeMap<(String, String), FileManifest> = BTreeMap::new();
        for plan in &snapshot.plans {
            let manifest = store
                .chunk(&plan.manifest_digest)?
                .ok_or(Error::RecoveryIncomplete)?;
            let root = self
                .roots()?
                .key(plan.root_generation)
                .ok_or(Error::RecoveryIncomplete)?;
            let content = open_manifest(root, &account, plan, &manifest)?;
            // Backups never appear in the vault's snapshot.
            ensure(
                !matches!(content, Opened::Backup(_)),
                Error::IntegrityFailed,
            )?;
            if let Opened::File(file) = content {
                let chunks: Vec<&str> = match file.chunks.is_empty() {
                    true => vec![],
                    false => file.chunks.iter().map(|c| c.digest.as_str()).collect(),
                };
                let planned: Vec<&str> = plan.chunks.iter().map(|c| c.digest.as_str()).collect();
                let empty = hex::encode(*sha256(&empty_file()));
                ensure(
                    planned == chunks || (chunks.is_empty() && planned == [empty.as_str()]),
                    Error::IntegrityFailed,
                )?;
                let id = (file.id.clone(), file.version.clone());
                ensure(
                    files.get(&id).is_none_or(|f| *f == file),
                    Error::IdempotencyConflict,
                )?;
                files.insert(id, file);
                continue;
            }
            ensure(plan.chunks.len() == 1, Error::IntegrityFailed)?;
            let bytes = store
                .chunk(&plan.chunks[0].digest)?
                .ok_or(Error::RecoveryIncomplete)?;
            let found = read_container(&bytes)?;
            ensure(
                found.subject_id == account
                    && found.id == plan.object_id
                    && found.revision == plan.version_id
                    && found.key == format!("{}:{}", found.id, found.revision),
                Error::IntegrityFailed,
            )?;
            ensure(
                records
                    .get(&found.key)
                    .is_none_or(|r| r.digest == found.digest),
                Error::IdempotencyConflict,
            )?;
            records.insert(found.key.clone(), found);
        }
        let mut incoming: Vec<(EncryptedObject, bool, bool)> = Vec::new();
        let mut resolved: BTreeSet<String> = BTreeSet::new();
        let mut heads: BTreeMap<String, (String, String)> = BTreeMap::new();
        for (revision, conflict) in &snapshot.revisions {
            let key = format!("{}:{}", revision.item_id, revision.revision_id);
            let found = records.get(&key).ok_or(Error::RecoveryIncomplete)?;
            ensure(
                found.tombstone == revision.tombstone && found.parent == revision.base_revision,
                Error::RecoveryIncomplete,
            )?;
            let known = store.object(&key)?;
            if let Some(row) = &known {
                ensure(
                    record(row)?.digest == found.digest,
                    Error::IdempotencyConflict,
                )?;
            } else {
                let payload = found.open(self.environment(), self.roots()?)?;
                if found.kind == VAULT {
                    resolved.extend(crate::vault::resolved_in(&payload));
                }
                if let Some(file) = &record_file(&found.kind, &payload)? {
                    let id = (file.id.clone(), file.version.clone());
                    ensure(files.get(&id) == Some(file), Error::RecoveryIncomplete)?;
                    read_file(&self.workspace.store, file, MAX_FILE, None)?;
                }
            }
            if !conflict {
                heads.insert(
                    found.id.clone(),
                    (found.revision.clone(), found.kind.clone()),
                );
            }
            incoming.push((found.clone(), *conflict, known.is_some()));
        }
        let keys: BTreeSet<String> = incoming.iter().map(|(r, _, _)| r.key.clone()).collect();
        store.atomic(|store| {
            let previous: u64 = store.meta(SNAPSHOT)?.unwrap_or(0);
            ensure(snapshot.through >= previous, Error::IntegrityFailed)?;
            // The relay may not drop a version it once accepted.
            for row in store.objects_in(SYNCED)? {
                ensure(keys.contains(&row.key), Error::IntegrityFailed)?;
            }
            for (found, conflict, known) in &incoming {
                match known {
                    true => {
                        store.set_object(&found.key, SYNCED, *conflict)?;
                        store.delete_job(&found.key)?;
                    }
                    false => store.put_object(&ObjectRow {
                        key: found.key.clone(),
                        item: found.id.clone(),
                        revision: found.revision.clone(),
                        kind: found.kind.clone(),
                        frame: canonical(found),
                        state: SYNCED.into(),
                        conflict: *conflict,
                        resolved: false,
                        created_at: found.created_at,
                    })?,
                }
            }
            for key in &resolved {
                store.set_resolved(key)?;
            }
            for (item, (revision, kind)) in &heads {
                let local = store.head(item)?;
                let keep = match local.as_deref() {
                    Some(local) if local != revision => {
                        Self::reconcile_branch(store, item, local, revision)?
                    }
                    _ => false,
                };
                if !keep {
                    store.set_head(item, revision, kind)?;
                }
            }
            store.set_meta(SNAPSHOT, &snapshot.through)
        })?;
        Ok(snapshot.revisions.len())
    }

    /// A local head that differs from the relay's: keep it when it leads
    /// to the relay's head, otherwise its unsent versions become conflict
    /// copies, still to be sent. Returns whether the local head stays.
    fn reconcile_branch(
        store: &crate::store::Store,
        item: &str,
        local: &str,
        relay: &str,
    ) -> Result<bool> {
        let mut branch = Vec::new();
        let mut cursor = Some(local.to_string());
        while let Some(revision) = cursor {
            if revision == relay {
                return Ok(true);
            }
            let Some(row) = store.object(&format!("{item}:{revision}"))? else {
                break;
            };
            if row.state != LOCAL {
                break;
            }
            cursor = record(&row)?.parent;
            branch.push(row.key);
        }
        for key in &branch {
            store.set_object(key, LOCAL, true)?;
        }
        Ok(false)
    }
}

/// Submit the presigned revisions whose deadline has not passed, each
/// without the client, and settle them; how many settled. They need no key,
/// so a locked device with the resident signing key flushes too.
///
/// # Errors
/// Storage failures; relay failures leave the job for later.
pub async fn flush<H: Hold>(h: &H) -> Result<usize> {
    let (relay, due) = h.with(|c| Ok((c.relay(), c.flush_due()?))).await?;
    let mut settled = 0;
    for (key, dispatch) in due {
        let sent = relay.submit(&dispatch, now_ms()).await;
        settled += usize::from(h.with(|c| c.flush_settle(&key, &dispatch, sent)).await?);
    }
    Ok(settled)
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
