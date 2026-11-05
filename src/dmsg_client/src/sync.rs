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
//! Initialization and recovery read a complete export snapshot; daily pulls
//! apply only the signed entries after a durable content cursor. Both paths check
//! every signed entry against the certified devices, every chunk against its digest, and each
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
const UPLOAD_HELD: &str = "sync-upload-held";

#[path = "sync_pull.rs"]
mod pull;

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
    #[serde(default)]
    cursor: Option<u64>,
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
#[derive(Clone, Default)]
struct Snapshot {
    through: u64,
    plans: Vec<UploadPlan>,
    revisions: Vec<(Revision, bool)>,
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
    /// Whether this pass established a complete snapshot baseline.
    pub full_snapshot: bool,
    /// Incremental pages checked, including an empty up-to-date page.
    pub delta_pages: u64,
    /// A blocked upload, while incoming changes can still be received.
    pub upload_error: Option<String>,
    /// Export pages checked, without resource identifiers.
    pub export_pages: u64,
    /// Newly downloaded ciphertext bytes; existing chunks are reused.
    pub downloaded_bytes: u64,
    /// Local entry verification milliseconds.
    pub verify_ms: u64,
    /// Local snapshot application milliseconds.
    pub apply_ms: u64,
    /// Whole pass milliseconds, including network waits.
    pub elapsed_ms: u64,
    /// Verified metadata changes applied in this pass.
    pub changes: usize,
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
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct LastSync {
    /// When it ended, in Unix milliseconds.
    pub at: u64,
    /// Its error code, if it failed.
    pub error: Option<String>,
    /// The failed step, without resource IDs, URLs or server messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<SyncStage>,
    /// When a sync last succeeded, in Unix milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok_at: Option<u64>,
    /// Incoming changes succeeded, but new uploads are blocked by a resource limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload_error: Option<String>,
    /// Consecutive failed passes, used for bounded automatic backoff.
    #[serde(default)]
    pub failures: u32,
    /// The earliest automatic retry of a failed pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_at: Option<u64>,
}

impl LastSync {
    /// When a sync last succeeded, also for a record written before
    /// `ok_at` was kept.
    pub fn succeeded_at(&self) -> Option<u64> {
        self.ok_at
            .or((self.error.is_none() && self.upload_error.is_none()).then_some(self.at))
    }

    /// When the scheduler may start a pass, including periodic and reconnect
    /// triggers. Every failure retries: its cause may be fixed elsewhere.
    pub fn automatic_at(&self) -> u64 {
        match &self.error {
            None => self.at,
            Some(code) => self
                .retry_at
                .unwrap_or_else(|| self.at.saturating_add(retry_delay(code, self.failures))),
        }
    }
}

/// Failures of the connection or the relay's moment, which pass by themselves.
pub(crate) fn transient(code: &str) -> bool {
    matches!(
        code,
        "UNAVAILABLE"
            | "NETWORK_UNAVAILABLE"
            | "EXECUTION_UNKNOWN"
            | "RESULT_EXPIRED"
            | "POLICY_STALE"
    )
}

/// The pause before attempt `failures + 1`, without jitter. Anything but a
/// transient failure waits long enough not to repeat its work needlessly.
fn retry_delay(code: &str, failures: u32) -> u64 {
    if transient(code) {
        match failures {
            0 | 1 => 5_000,
            2 => 15_000,
            3 => 30_000,
            n => 60_000u64.saturating_mul(1 << (n - 4).min(3)).min(300_000),
        }
    } else {
        match failures {
            0 | 1 => 15 * 60_000,
            2 => 30 * 60_000,
            _ => 60 * 60_000,
        }
    }
}

/// How far this device has verified the account's content; it moves whenever
/// a pass, complete or not, applied something.
///
/// # Errors
/// Local storage errors.
pub fn position(client: &Client) -> Result<u64> {
    let store = &client.workspace.store;
    Ok(store
        .meta(pull::CURSOR)?
        .or(store.meta(SNAPSHOT)?)
        .unwrap_or(0))
}

/// Whether a `content` hint tells this device something new: a device already
/// past its cursor, usually the one whose version moved it, has nothing to read.
pub fn hint_is_news(client: &Client, cursor: Option<u64>) -> bool {
    cursor.is_none_or(|cursor| position(client).is_ok_and(|at| at < cursor))
}

/// What makes a waiting retry worth starting early.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryCause {
    /// The person unlocked: login and keys may be what the pass lacked.
    Unlock,
    /// The connection came back, which only helps a transient failure.
    Network,
}

/// Bring a waiting automatic retry forward; a cooldown the relay asked for
/// stays. Whether there was a retry to bring forward.
///
/// # Errors
/// Local storage errors.
pub fn retry_early(client: &Client, cause: RetryCause) -> Result<bool> {
    let Some(mut last) = client.last_sync()? else {
        return Ok(false);
    };
    let now = now_ms();
    let Some(code) = last.error.as_deref() else {
        return Ok(false);
    };
    let applies = match cause {
        RetryCause::Unlock => code != "SYNC_BUSY",
        RetryCause::Network => transient(code),
    };
    if !applies || last.automatic_at() <= now {
        return Ok(false);
    }
    last.retry_at = Some(now);
    client.workspace.store.set_meta(LAST_SYNC, &last)?;
    Ok(true)
}

/// Fixed diagnostic names for the steps of an account sync.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncStage {
    /// Check device authority, root keys and fresh security evidence.
    Security,
    /// Reconcile earlier submissions whose replies were lost.
    Reconcile,
    /// Start an immutable export snapshot.
    ExportStart,
    /// Read and verify the snapshot's inventory pages.
    ExportPage,
    /// Read changes after the verified account cursor.
    DeltaPage,
    /// Fetch and verify missing manifests and chunks.
    Download,
    /// Open verified versions and advance local heads.
    Apply,
    /// Send pending versions and settle their jobs.
    Upload,
    /// Read final counts and collect unused local chunks.
    Finish,
}

impl SyncStage {
    /// A content-free name suitable for the daemon's run log.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Security => "security",
            Self::Reconcile => "reconcile",
            Self::ExportStart => "export_start",
            Self::ExportPage => "export_page",
            Self::DeltaPage => "delta_page",
            Self::Download => "download",
            Self::Apply => "apply",
            Self::Upload => "upload",
            Self::Finish => "finish",
        }
    }
}

/// Content-free progress for a running pass. Counts survive a resumed snapshot.
#[derive(Clone, Debug, Serialize)]
pub struct SyncProgress {
    /// The current network or verification phase.
    pub stage: SyncStage,
    /// Inventory entries downloaded and verified so far.
    pub entries: u64,
    /// Inventory entries expected in a full snapshot; zero for incremental reads.
    pub total: u64,
    /// Newly received ciphertext bytes in this pass.
    pub downloaded_bytes: u64,
    /// Full snapshot pages received in this pass.
    pub export_pages: u64,
    /// Incremental pages received in this pass.
    pub delta_pages: u64,
}

impl Default for SyncProgress {
    fn default() -> Self {
        Self {
            stage: SyncStage::Security,
            entries: 0,
            total: 0,
            downloaded_bytes: 0,
            export_pages: 0,
            delta_pages: 0,
        }
    }
}

struct Observer<'a> {
    progress: SyncProgress,
    notify: &'a mut (dyn FnMut(&SyncProgress) + Send),
}

impl<'a> Observer<'a> {
    fn new(notify: &'a mut (dyn FnMut(&SyncProgress) + Send)) -> Self {
        Self {
            progress: SyncProgress::default(),
            notify,
        }
    }

    fn stage(&mut self, stage: SyncStage) {
        self.progress.stage = stage;
        self.emit();
    }

    fn emit(&mut self) {
        (self.notify)(&self.progress);
    }
}

/// Whether the workspace has finished verifying its first content baseline.
/// # Errors
/// Local storage errors.
pub fn initialized(client: &Client) -> Result<bool> {
    Ok(client.workspace.store.meta::<u64>(pull::CURSOR)?.is_some())
}

fn plan_of(record: &EncryptedObject, upload_id: String, generation: u64, now: u64) -> UploadPlan {
    UploadPlan {
        management: None,
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
        expires_at: crate::upload::expires_at(now),
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
        now: u64,
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
        self.seal_uploads(&mut job, record, file, true, true, now)?;
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
        now: u64,
    ) -> Result<()> {
        let account = self.account()?.id.to_string();
        let roots = self.roots()?;
        let generation = roots.generation;
        let root = roots.key(generation).ok_or(Error::RecoveryIncomplete)?;
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
                management: None,
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
                expires_at: crate::upload::expires_at(now),
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
    /// it only sends versions that need no reseal and reads nothing.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked without a resident key,
    /// `Error::RekeyRequired` while the account waits for a new root and
    /// `Error::DeviceNotApproved` for a revoked device, whose workspace is
    /// then erased.
    pub async fn sync(&mut self) -> Result<SyncReport> {
        retry_uploads(self)?;
        sync(&self.hold()).await
    }

    /// The latest sync attempt.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn last_sync(&self) -> Result<Option<LastSync>> {
        self.workspace.store.meta(LAST_SYNC)
    }

    /// Collect unreferenced cache bytes using the entire verified inventory and
    /// local jobs. An empty delta never removes history or unsent ciphertext.
    fn collect_chunks(&self, now: u64) -> Result<u64> {
        let mut keep = self.held_chunks(now)?;
        keep.extend(self.attachment_chunks()?);
        let mut add = |plan: &UploadPlan| {
            keep.extend(plan.chunks.iter().map(|c| c.digest.clone()));
            keep.insert(plan.manifest_digest.clone());
        };
        self.workspace.store.sync_plans()?.iter().for_each(&mut add);
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

    pub(crate) fn disposable_attachment_chunks(&self) -> Result<Vec<String>> {
        self.disposable_chunks(self.attachment_chunks()?)
    }

    pub(crate) fn disposable_chunks(
        &self,
        mut candidates: BTreeSet<String>,
    ) -> Result<Vec<String>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        // Keep even expired uploads: clearing a cache must never decide that
        // a user's pending send or root rotation may be discarded.
        let mut keep = self.held_chunks(0)?;
        let mut add = |plan: &UploadPlan| {
            keep.extend(plan.chunks.iter().map(|c| c.digest.clone()));
            keep.insert(plan.manifest_digest.clone());
        };
        for (_, job) in self.workspace.store.jobs::<ContentJob>()? {
            job.file
                .iter()
                .chain([&job.vault])
                .for_each(|u| add(&u.plan));
        }
        if let Some(plan) = self.root_plan()? {
            add(&plan);
        }
        for row in self.workspace.store.objects_of_kind(MIGRATION_PART)? {
            if let Some(file) = self.version_file(&record(&row)?)? {
                keep.extend(file.chunks.into_iter().map(|c| c.digest));
            }
        }
        candidates.retain(|digest| !keep.contains(digest));
        Ok(candidates.into_iter().collect())
    }

    /// [`flush`] for a caller that holds the client throughout.
    ///
    /// # Errors
    /// As [`flush`].
    pub async fn flush(&mut self) -> Result<usize> {
        flush(&self.hold()).await
    }

    /// Pending content's local write watermark for host sync scheduling.
    pub fn local_change(&self) -> Result<u64> {
        self.workspace.store.local_change()
    }

    /// What [`flush`] submits, by job: the presigned revisions whose
    /// deadline has not passed. They need no key.
    fn flush_due(&self) -> Result<Vec<(String, Presigned)>> {
        let now = now_ms();
        if self
            .last_sync()?
            .is_some_and(|last| now < last.automatic_at())
            || self
                .workspace
                .store
                .meta::<String>(pull::UPLOAD_BLOCKED)?
                .is_some()
        {
            return Ok(Vec::new());
        }
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
            if let Some(after) = store.meta::<u64>(pull::CURSOR)? {
                let cursor = verdict.cursor.ok_or(Error::UnsupportedProtocol)?;
                if cursor > after {
                    store.sync_confirm(key, cursor)?;
                }
            }
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

    /// Open every new version of a fetched snapshot, then move the heads in
    /// one transaction. Returns the number of revisions.
    #[cfg(test)]
    fn apply(&mut self, snapshot: &Snapshot) -> Result<usize> {
        self.apply_versions(snapshot, true, &[])
    }

    fn apply_versions(
        &mut self,
        snapshot: &Snapshot,
        complete: bool,
        retired: &[String],
    ) -> Result<usize> {
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
        let mut accepted = BTreeSet::new();
        for (revision, conflict) in &snapshot.revisions {
            let key = format!("{}:{}", revision.item_id, revision.revision_id);
            if let Some(parent) = &revision.base_revision {
                let parent_key = format!("{}:{parent}", revision.item_id);
                ensure(
                    accepted.contains(&parent_key)
                        || store
                            .object(&parent_key)?
                            .is_some_and(|r| r.state == SYNCED),
                    Error::RecoveryIncomplete,
                )?;
            }
            accepted.insert(key.clone());
            if !complete
                && !records.contains_key(&key)
                && let Some(known) = store.object(&key)?
            {
                records.insert(key.clone(), record(&known)?);
            }
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
            if complete {
                // A full snapshot must still account for every previously accepted version.
                for row in store.objects_in(SYNCED)? {
                    ensure(keys.contains(&row.key), Error::IntegrityFailed)?;
                }
            }
            for plan in &snapshot.plans {
                store.sync_put_plan(plan)?;
            }
            for upload in retired {
                store.sync_remove_plan(upload)?;
            }
            for (found, conflict, known) in &incoming {
                store.sync_confirmed(&found.key)?;
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
            ensure(
                !store.sync_missing_confirmation(snapshot.through)?,
                Error::RecoveryIncomplete,
            )?;
            store.set_meta(SNAPSHOT, &snapshot.through)?;
            pull::committed(store, snapshot.through, complete)
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

/// Sync without holding the client during network waits. Callers serialize
/// sync and flush passes for a workspace; local reads and edits may continue.
///
/// # Errors
/// As [`Client::sync`]. A lock or account switch interrupts an unlocked pass.
pub async fn sync<H: Hold>(h: &H) -> Result<SyncReport> {
    sync_observed(h, false, &mut |_| {}).await
}

/// Synchronize content with progress; receiving alone never reconciles or sends local jobs.
/// Both modes establish a full baseline only when needed, then use incremental pages,
/// and record the same failure/backoff state for the host scheduler.
/// # Errors
/// As [`sync`].
pub async fn sync_observed<H: Hold>(
    h: &H,
    receive_only: bool,
    notify: &mut (dyn FnMut(&SyncProgress) + Send),
) -> Result<SyncReport> {
    // A pass must not apply an old response after lock, logout or an account
    // switch. The generation also distinguishes locking and reopening the
    // same account while a request is in flight.
    let generation = h.with(|c| Ok(c.session_generation)).await?;
    let session = SyncSession { h, generation };
    let started = std::time::Instant::now();
    let mut stage = Observer::new(notify);
    let mut result = sync_once(&session, receive_only, &mut stage).await;
    if matches!(result, Err(Error::PolicyStale)) {
        session
            .with(|c| {
                c.published = None;
                Ok(())
            })
            .await?;
        result = sync_once(&session, receive_only, &mut stage).await;
    }
    if matches!(result, Err(Error::DeviceNotApproved)) {
        let security = crate::account::refreshed_security(&session).await?;
        session.with(|c| c.check_revoked(&security)).await?;
    }
    if let Ok(report) = &mut result {
        report.elapsed_ms = started.elapsed().as_millis() as u64;
    }
    session
        .with(|c| {
            let now = now_ms();
            if matches!(result, Err(Error::Locked | Error::AuthRequired))
                && c.session_state().state != "unlocked"
            {
                return Ok(());
            }
            let previous = c.last_sync()?;
            let upload_error = result.as_ref().ok().and_then(|r| r.upload_error.clone());
            let ok_at = if result.is_ok() && upload_error.is_none() {
                Some(now)
            } else {
                previous.as_ref().and_then(LastSync::succeeded_at)
            };
            let error = result.as_ref().err().map(crate::error_code);
            let failures = if error.is_some() {
                previous
                    .as_ref()
                    .filter(|s| s.error.is_some())
                    .map_or(1, |s| s.failures.saturating_add(1))
            } else {
                0
            };
            let retry_at = result.as_ref().err().map(|e| {
                if let Some(delay) = crate::transport::retry_after(e) {
                    return now.saturating_add(delay);
                }
                let delay = retry_delay(&crate::error_code(e), failures);
                let jitter = delay * u64::from(crate::crypto::random_id()[0]) / 1280;
                now.saturating_add(delay + jitter)
            });
            c.workspace.store.set_meta(
                LAST_SYNC,
                &LastSync {
                    at: now,
                    error,
                    stage: result.is_err().then_some(stage.progress.stage),
                    ok_at,
                    upload_error,
                    failures,
                    retry_at,
                },
            )?;
            if c.session_state().state == "unlocked" {
                // Synced encrypted channel preferences must also filter locked
                // notifications; update the minimal projection while keys exist.
                // A pass that moved nothing changed no preference, and a failed
                // one may have committed some before it stopped.
                if !result
                    .as_ref()
                    .is_ok_and(|r| r.changes + r.sent + r.blocked == 0)
                {
                    c.reconcile_channel_notifications(now)?;
                }
                c.notification_health(now)?;
            }
            Ok(())
        })
        .await?;
    result
}

struct SyncSession<'a, H> {
    h: &'a H,
    generation: u64,
}

/// Refresh the encrypted account inventory without flushing or uploading local
/// work. Used when reviewing account deletion from a newly authorized device.
/// Shared hosts serialize this call in the same sync lane as ordinary passes.
/// # Errors
/// Locked/revoked device, unavailable relay or an unverifiable snapshot.
pub async fn pull_metadata<H: Hold>(h: &H) -> Result<usize> {
    let h = crate::hold::Unlocked::new(h).await?;
    h.signed(|c| {
        let security = c.fresh()?;
        c.check_revoked(&security)?;
        c.roots()?;
        Ok(())
    })
    .await?;
    let mut noop = |_: &SyncProgress| {};
    let mut stage = Observer::new(&mut noop);
    let report = pull::full(&h, &mut stage).await?;
    Ok(report.revisions)
}

impl<H: Hold> Hold for SyncSession<'_, H> {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        self.h
            .with(|c| {
                ensure(c.session_generation == self.generation, Error::Locked)?;
                step(c)
            })
            .await
    }
}

async fn sync_once<H: Hold>(
    h: &H,
    receive_only: bool,
    stage: &mut Observer<'_>,
) -> Result<SyncReport> {
    stage.stage(SyncStage::Security);
    h.with(|c| c.signing().map(|_| ())).await?;
    h.signed(Client::fresh).await?;
    let unlocked = h
        .with(|c| {
            let security = c.fresh()?;
            c.check_revoked(&security)?;
            if c.keys.is_some() {
                c.roots()?;
            }
            Ok(c.keys.is_some())
        })
        .await?;
    stage.stage(SyncStage::Reconcile);
    if !receive_only {
        reconcile(h).await?;
    }
    let mut report = SyncReport::default();
    if unlocked {
        pull::run(h, stage, &mut report).await?;
    } else {
        report.through = h.with(|c| position(c)).await?;
    }
    stage.stage(SyncStage::Upload);
    let paused = h
        .with(|c| c.workspace.store.meta::<String>(pull::UPLOAD_BLOCKED))
        .await?;
    let (sent, blocked) = if receive_only || paused.is_some() {
        report.upload_error = paused;
        (0, 0)
    } else {
        let result = push(h).await?;
        report.upload_error = h.with(upload_error).await?;
        result
    };
    if unlocked && sent + blocked > 0 {
        pull::run(h, stage, &mut report).await?;
    }
    stage.stage(SyncStage::Finish);
    if unlocked {
        pull::close_completed(h).await;
    }
    h.with(|c| {
        report.sent = sent;
        report.blocked = blocked;
        report.pending = c.workspace.store.count_in(LOCAL)?;
        report.conflicts = c.workspace.store.count_conflicts(VAULT)?;
        if unlocked && (report.changes > 0 || sent + blocked > 0) {
            report.collected = c.collect_chunks(now_ms()).unwrap_or(0);
        }
        Ok(report)
    })
    .await
}

/// The longest wait between passes while the hint channel is connected.
const HINTED_SYNC_MS: u64 = 15 * dmsg_types::MINUTE;
/// An edit waits this long for the next one before a pass takes them.
const EDIT_SETTLE_MS: u64 = 500;
/// A wait with nothing moving grows to this many intervals.
const IDLE_INTERVALS: u32 = 5;

/// What a device sees when it asks whether a pass is due.
#[derive(Clone, Copy, Debug, Default)]
pub struct Moment<'a> {
    /// Unix milliseconds.
    pub now: u64,
    /// The session's account.
    pub account: Option<&'a str>,
    /// [`Client::local_change`].
    pub local_change: u64,
    /// The latest attempt.
    pub last: Option<&'a LastSync>,
    /// The wait between passes when nothing happens; none when zero, which
    /// also leaves edits for a manual pass.
    pub interval: u64,
    /// The hint channel is connected, so a periodic pass backs up a lost hint.
    pub hinted: bool,
    /// A hint or a returning network asked for a pass.
    pub requested: bool,
    /// Another pass is running.
    pub running: bool,
}

/// Whether a pass should start, and for whom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Due {
    /// Not now.
    No,
    /// Someone waits for it: an edit, a request or a failure's retry. It
    /// needs the workspace unlocked.
    Now,
    /// Only time passed.
    Periodic,
}

/// When the next automatic pass is due, for the daemon and the app alike. It
/// holds no clock and does no I/O: callers say what they see and what ran.
#[derive(Debug, Default)]
pub struct Schedule {
    seen: Option<(Option<String>, u64)>,
    changed_at: u64,
    passed_at: u64,
    idle_rounds: u32,
    through: Option<u64>,
}

impl Schedule {
    /// A schedule starting now counts as having just passed.
    pub fn new(now: u64) -> Self {
        Self {
            passed_at: now,
            ..Self::default()
        }
    }

    /// Whether a pass should start. A failure's retry time holds everything back.
    pub fn due(&mut self, at: &Moment<'_>) -> Due {
        let content = (at.account.map(str::to_string), at.local_change);
        if self.seen.as_ref() != Some(&content) {
            self.changed_at = at.now;
            self.seen = Some(content);
            self.idle_rounds = 0;
        }
        if at.last.is_some_and(|last| at.now < last.automatic_at()) {
            return Due::No;
        }
        let edited = at.interval > 0
            && at.local_change > 0
            && self.changed_at > self.passed_at
            && at.now >= self.changed_at + EDIT_SETTLE_MS;
        let retry = !at.running && at.last.is_some_and(|last| last.error.is_some());
        if at.requested || edited || retry {
            return Due::Now;
        }
        let wait = if at.interval > 0 && at.hinted {
            at.interval.max(HINTED_SYNC_MS)
        } else {
            at.interval
                .saturating_mul(u64::from(self.idle_rounds.min(IDLE_INTERVALS - 1) + 1))
        };
        let since = self.passed_at.max(at.last.map_or(0, |last| last.at));
        if wait > 0 && at.now >= since.saturating_add(wait) {
            Due::Periodic
        } else {
            Due::No
        }
    }

    /// A pass starts.
    pub fn started(&mut self, now: u64) {
        self.passed_at = now;
    }

    /// A pass ended with what it sent and how far it reached, if it ran. One
    /// that moved nothing makes the next wait longer; a version the relay
    /// holds back needs no faster polling, as a new edit resets the wait.
    pub fn finished(&mut self, report: Option<(usize, u64)>) {
        self.idle_rounds = match report {
            Some((0, through)) if self.through == Some(through) => {
                self.idle_rounds.saturating_add(1)
            }
            _ => 0,
        };
        if let Some((_, through)) = report {
            self.through = Some(through);
        }
    }
}

/// Resume an upload blocked by a confirmed resource limit after an owner retry or quota refresh.
/// # Errors
/// Local storage errors.
pub fn retry_uploads(client: &mut Client) -> Result<()> {
    let now = now_ms();
    if let Some(last) = client.last_sync()?
        && last.error.as_deref() == Some("SYNC_BUSY")
        && let Some(at) = last.retry_at.filter(|at| *at > now)
    {
        return Err(crate::transport::busy(at - now));
    }
    client.workspace.store.delete_meta(UPLOAD_HELD)?;
    client.workspace.store.delete_meta(pull::UPLOAD_BLOCKED)
}

/// Recovery must establish a new complete baseline while keeping local drafts and the rollback floor.
pub(crate) fn require_full(client: &mut Client) -> Result<()> {
    pull::require_full(&client.workspace.store)
}

/// An authenticated quota refresh can resume uploads when storage availability improves.
pub(crate) fn observe_storage(client: &mut Client, available: u64) -> Result<()> {
    const AVAILABLE: &str = "sync-storage-available";
    let store = &client.workspace.store;
    let previous = store.meta::<u64>(AVAILABLE)?;
    store.atomic(|store| {
        store.set_meta(AVAILABLE, &available)?;
        if available > 0 && previous.is_none_or(|p| available > p) {
            store.delete_meta(pull::UPLOAD_BLOCKED)?;
        }
        Ok(())
    })
}

async fn reconcile<H: Hold>(h: &H) -> Result<()> {
    flush(h).await?;
    let (base, jobs) = h
        .with(|c| Ok((c.base()?, c.workspace.store.jobs::<ContentJob>()?)))
        .await?;
    for (key, job) in jobs {
        let Some(dispatch) = job.dispatch else {
            continue;
        };
        let path = format!("{base}/operations/{}", hex::encode(*job.request_id));
        if let Some(found) = h.find(&path).await?.filter(|f| f["found"] == true) {
            h.with(|c| c.flush_settle(&key, &dispatch, Ok(found["result"].clone())))
                .await?;
        }
    }
    Ok(())
}

/// An item the relay refused. Its later versions wait behind it, and the rest
/// of the queue moves on.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct HeldUpload {
    code: String,
    attempts: u32,
    retry_at: u64,
}

/// A refusal of the version itself, which sending it again soon cannot change.
/// Everything else concerns the connection or the account and stops the pass.
fn refuses_version(error: &Error) -> bool {
    matches!(
        error,
        Error::InvalidInput(_)
            | Error::IntegrityFailed
            | Error::IdempotencyConflict
            | Error::NotFound
            | Error::RecoveryIncomplete
            | Error::Expired
    )
}

fn held_uploads(c: &Client) -> Result<BTreeMap<String, HeldUpload>> {
    Ok(c.workspace.store.meta(UPLOAD_HELD)?.unwrap_or_default())
}

fn set_held_uploads(c: &Client, held: &BTreeMap<String, HeldUpload>) -> Result<()> {
    if held.is_empty() {
        c.workspace.store.delete_meta(UPLOAD_HELD)
    } else {
        c.workspace.store.set_meta(UPLOAD_HELD, held)
    }
}

/// Why local versions are not leaving: the account's limit, or a refused item.
fn upload_error(c: &mut Client) -> Result<Option<String>> {
    if let Some(code) = c.workspace.store.meta(pull::UPLOAD_BLOCKED)? {
        return Ok(Some(code));
    }
    Ok(held_uploads(c)?.into_values().next().map(|held| held.code))
}

async fn push<H: Hold>(h: &H) -> Result<(usize, usize)> {
    let (rows, mut held) = h
        .with(|c| {
            let rows = ordered(c.workspace.store.objects_in(LOCAL)?)?;
            let mut held = held_uploads(c)?;
            // An item that no longer waits has nothing left to hold.
            held.retain(|item, _| rows.iter().any(|row| row.item == *item));
            set_held_uploads(c, &held)?;
            Ok((rows, held))
        })
        .await?;
    let (mut sent, mut blocked) = (0, 0);
    let now = now_ms();
    for row in rows {
        if held.get(&row.item).is_some_and(|hold| hold.retry_at > now) {
            continue;
        }
        if !h
            .with(|c| {
                Ok(c.workspace
                    .store
                    .object(&row.key)?
                    .is_some_and(|r| r.state == LOCAL))
            })
            .await?
        {
            continue;
        }
        let item = row.item.clone();
        match push_one(h, row).await {
            Ok(accepted) => {
                if accepted {
                    sent += 1;
                } else {
                    blocked += 1;
                }
                if held.remove(&item).is_some() {
                    h.with(|c| set_held_uploads(c, &held)).await?;
                }
            }
            Err(Error::Locked) if h.with(|c| Ok(c.keys.is_none())).await? => {}
            Err(Error::QuotaExceeded) => {
                h.with(|c| {
                    c.workspace
                        .store
                        .set_meta(pull::UPLOAD_BLOCKED, &"QUOTA_EXCEEDED")
                })
                .await?;
                break;
            }
            Err(error) if refuses_version(&error) => {
                let code = crate::error_code(&error);
                let attempts = held.get(&item).map_or(1, |hold| hold.attempts + 1);
                let retry_at = now_ms().saturating_add(retry_delay(&code, attempts));
                held.insert(
                    item,
                    HeldUpload {
                        code,
                        attempts,
                        retry_at,
                    },
                );
                h.with(|c| set_held_uploads(c, &held)).await?;
            }
            Err(error) => return Err(error),
        }
    }
    Ok((sent, blocked))
}

async fn push_one<H: Hold>(h: &H, mut row: ObjectRow) -> Result<bool> {
    let (_, (mut record, job, replace)) = h
        .signed(|c| {
            let security = c.fresh()?;
            ensure(
                security.snapshot.vault_write_state == VaultWriteState::Ready,
                Error::RekeyRequired,
            )?;
            let record = record(&row)?;
            let job = match c.workspace.store.job::<ContentJob>(&row.key)? {
                Some(job) => job,
                None => {
                    let job =
                        c.prepare_job(&record, c.version_file(&record)?.as_ref(), now_ms())?;
                    c.workspace.store.set_job(&row.key, &job)?;
                    job
                }
            };
            ensure(
                job.record_digest == record.digest,
                Error::IdempotencyConflict,
            )?;
            let parent_replaced = match &record.parent {
                Some(parent) => c
                    .workspace
                    .store
                    .object(&format!("{}:{parent}", record.id))?
                    .is_some_and(|p| p.state == REPLACED),
                None => false,
            };
            let replace =
                record.generation != security.snapshot.content_root_generation || parent_replaced;
            if replace {
                ensure(
                    job.dispatch.as_ref().is_none_or(|d| d.deadline <= now_ms()),
                    Error::ExecutionUnknown,
                )?;
                c.roots()?;
            }
            Ok((record, job, replace))
        })
        .await?;
    if replace {
        for upload in job.file.iter().chain([&job.vault]) {
            if !upload.done {
                crate::upload::cancel_upload(h, &upload.plan.upload_id).await?;
            }
        }
        let replacement = h
            .with(|c| {
                c.roots()?;
                let child = c.workspace.store.objects_in(LOCAL)?.iter().any(|r| {
                    r.item == record.id && record_parent(r).as_deref() == Some(&record.revision)
                });
                if child {
                    c.workspace.store.set_object(&row.key, REPLACED, false)?;
                    c.workspace.store.delete_job(&row.key)?;
                    return Ok(None);
                }
                let file = c.version_file(&record)?;
                let payload = record.open(c.environment(), c.roots()?)?;
                let parent = c.sent_ancestor(&record.id, record.parent.clone())?;
                let (replacement, _) = c.reseal(&row, &record, parent, &payload, file.as_ref())?;
                let row = c
                    .workspace
                    .store
                    .object(&replacement.key)?
                    .ok_or(Error::IntegrityFailed)?;
                Ok(Some((row, replacement)))
            })
            .await?;
        let Some(replacement) = replacement else {
            return Ok(true);
        };
        (row, record) = replacement;
    }
    for file in [true, false] {
        send_upload(h, &row.key, &record, file).await?;
    }
    let (relay, dispatch) = h
        .signed(|c| {
            let security = c.fresh()?;
            ensure(
                security.snapshot.vault_write_state == VaultWriteState::Ready
                    && security.snapshot.content_root_generation == record.generation,
                Error::RekeyRequired,
            )?;
            let mut job: ContentJob = c
                .workspace
                .store
                .job(&row.key)?
                .ok_or(Error::IntegrityFailed)?;
            ensure(
                job.record_digest == record.digest,
                Error::IdempotencyConflict,
            )?;
            let now = now_ms();
            if let Some(dispatch) = job.dispatch.as_ref().filter(|d| d.deadline > now + 5_000) {
                return Ok(dispatch.clone());
            }
            let revision = Revision {
                item_id: record.id.clone(),
                revision_id: record.revision.clone(),
                base_revision: record.parent.clone(),
                upload_id: (!record.tombstone).then(|| job.vault.plan.upload_id.clone()),
                tombstone: record.tombstone,
                restore: job.restore,
            };
            let base = c.base()?;
            let request = hex::encode(*job.request_id);
            let dispatch = c.cloud.presign(
                &c.signer(&security)?,
                &format!("{base}/vault"),
                "dmsg/vault/revision/v1",
                &revision,
                &job.request_id,
                &format!("{base}/operations/{request}"),
                now,
            )?;
            job.dispatch = Some(dispatch.clone());
            c.workspace.store.set_job(&row.key, &job)?;
            Ok(dispatch)
        })
        .await?;
    let mut verdict = relay.submit(&dispatch, now_ms()).await;
    if matches!(
        verdict,
        Err(Error::ExecutionUnknown | Error::Unavailable(_))
    ) && verdict
        .as_ref()
        .err()
        .is_some_and(|error| crate::transport::retry_after(error).is_none())
    {
        let now = now_ms();
        if now < dispatch.deadline {
            // A lost reply must not stop the rest of a large import. Submit
            // first checks the operation, then reuses the exact signed
            // command only if it is absent. Keep the original deadline.
            verdict = relay.submit(&dispatch, now).await;
        }
    }
    h.with(|c| {
        let job: ContentJob = c
            .workspace
            .store
            .job(&row.key)?
            .ok_or(Error::IntegrityFailed)?;
        ensure(
            job.dispatch.as_ref() == Some(&dispatch),
            Error::IdempotencyConflict,
        )?;
        match verdict {
            Ok(verdict) => c.settle(&row.key, &verdict).map(|_| true),
            Err(Error::VersionConflict) => c.block(&row.key).map(|_| false),
            Err(error) => Err(error),
        }
    })
    .await
}

async fn send_upload<H: Hold>(
    h: &H,
    key: &str,
    record: &EncryptedObject,
    file: bool,
) -> Result<()> {
    let mut replanned = false;
    let mut retried = false;
    loop {
        let (base, job) = h
            .with(|c| {
                Ok((
                    c.base()?,
                    c.workspace
                        .store
                        .job::<ContentJob>(key)?
                        .ok_or(Error::IntegrityFailed)?,
                ))
            })
            .await?;
        let upload = if file {
            job.file.as_ref()
        } else {
            Some(&job.vault)
        };
        let Some(upload) = upload.filter(|u| !u.done) else {
            return Ok(());
        };
        // Re-read the job at every write: another local action must not have
        // replaced the upload while this request was waiting on the relay.
        let update = |c: &Client, sent: Option<usize>| {
            let mut job: ContentJob = c.workspace.store.job(key)?.ok_or(Error::IntegrityFailed)?;
            let current = if file {
                job.file.as_mut().ok_or(Error::IntegrityFailed)?
            } else {
                &mut job.vault
            };
            ensure(current.plan == upload.plan, Error::IdempotencyConflict)?;
            match sent {
                Some(sent) => current.sent = sent as u64,
                None => current.done = true,
            };
            c.workspace.store.set_job(key, &job)
        };
        match crate::upload::upload(
            h,
            &base,
            &upload.plan,
            &upload.manifest,
            upload.sent as usize,
            Source::Store,
            |c, sent| update(c, Some(sent)),
        )
        .await
        {
            Ok(_) => return h.with(|c| update(c, None)).await,
            Err(error @ (Error::ExecutionUnknown | Error::Unavailable(_)))
                if !retried && crate::transport::retry_after(&error).is_none() =>
            {
                // Re-read the same upload's status and stored chunk progress.
                // Reserve/finalize may already have succeeded; neither a new
                // upload ID nor a new content version is needed to resume it.
                retried = true;
            }
            Err(Error::ResultExpired) if !replanned => {
                h.with(|c| {
                    let job: ContentJob =
                        c.workspace.store.job(key)?.ok_or(Error::IntegrityFailed)?;
                    ensure(
                        file || job.dispatch.as_ref().is_none_or(|d| d.deadline <= now_ms()),
                        Error::ExecutionUnknown,
                    )
                })
                .await?;
                replanned = true;
                crate::upload::cancel_upload(h, &upload.plan.upload_id).await?;
                h.with(|c| {
                    let manifest = c.version_file(record)?;
                    let mut job: ContentJob =
                        c.workspace.store.job(key)?.ok_or(Error::IntegrityFailed)?;
                    c.seal_uploads(&mut job, record, manifest.as_ref(), !file, file, now_ms())?;
                    c.workspace.store.set_job(key, &job)
                })
                .await?;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Read and verify the relay's export snapshot, caching every vault and
/// file chunk it lacks; nothing is opened yet.
/// Download the manifests and chunks of committed uploads that the
/// store lacks; every one is checked against its digest.
async fn cache<H: Hold>(h: &H, plans: &[UploadPlan], stage: &mut Observer<'_>) -> Result<u64> {
    let (missing, files, missing_sizes, file_sizes, bytes) = h
        .with(|c| {
            let base = c.base()?;
            let mut seen = BTreeSet::new();
            let mut bytes = 0;
            let mut missing = Vec::new();
            let mut files = Vec::new();
            let mut missing_sizes = Vec::new();
            let mut file_sizes = Vec::new();
            for plan in plans {
                let parts = plan
                    .chunks
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (i.to_string(), &c.digest, c.size))
                    .chain([(
                        "manifest".to_string(),
                        &plan.manifest_digest,
                        plan.manifest_size,
                    )]);
                for (name, digest, size) in parts {
                    if seen.insert(digest) && !c.workspace.store.has_chunk(digest)? {
                        bytes += size;
                        let path = format!("{base}/objects/{}/chunks/{name}", plan.upload_id);
                        if matches!(plan.kind.as_str(), "file" | "backup" | "share") {
                            files.push((path, digest.clone()));
                            file_sizes.push(size);
                        } else {
                            missing.push((path, digest.clone()));
                            missing_sizes.push(size);
                        }
                    }
                }
            }
            Ok((missing, files, missing_sizes, file_sizes, bytes))
        })
        .await?;
    let mut received = 0;
    crate::upload::download_chunks(h, Source::Store, &missing, |count| {
        stage.progress.downloaded_bytes += missing_sizes[received..received + count]
            .iter()
            .sum::<u64>();
        received += count;
        stage.emit();
    })
    .await?;
    received = 0;
    crate::upload::download_file_chunks(h, Source::Store, &files, |count| {
        stage.progress.downloaded_bytes +=
            file_sizes[received..received + count].iter().sum::<u64>();
        received += count;
        stage.emit();
    })
    .await?;
    Ok(bytes)
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
        if let Err(error) = &sent
            && let Some(delay) = crate::transport::retry_after(error)
        {
            h.with(|c| {
                let previous = c.last_sync()?;
                let ok_at = previous.as_ref().and_then(LastSync::succeeded_at);
                let mut last = previous.unwrap_or_default();
                last.ok_at = ok_at;
                last.at = now_ms();
                last.error = Some("SYNC_BUSY".into());
                last.stage = Some(SyncStage::Upload);
                last.retry_at = Some(last.at.saturating_add(delay));
                c.workspace.store.set_meta(LAST_SYNC, &last)
            })
            .await?;
            return Err(error.clone());
        }
        settled += usize::from(h.with(|c| c.flush_settle(&key, &dispatch, sent)).await?);
    }
    Ok(settled)
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
