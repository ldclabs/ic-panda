//! Backups: a version uploaded to the relay as a `kind=backup` object, only
//! when a person asks for it. The chunks are the version's own sealed
//! chunks from the blob directory; the manifest is a `dmsg-backup/1` record
//! around the file manifest (key included), sealed under the current
//! content root. The upload's object and version IDs are random, so the
//! relay learns nothing about which file or version this is.
//!
//! A backup is a job in the plain `library_jobs` table: the signed plan,
//! the sealed manifest and how many chunks went. The job needs no key to
//! continue, so a locked device with the resident signing key finishes it;
//! the version record is updated once the device is unlocked. A failed
//! transient attempt starts its bounded backoff at [`RETRY_MS`]. Nothing creates a job but
//! [`backup`]: not an import, not an edit, not a cloud
//! inventory refresh, and a backup the person deleted from the cloud is not
//! uploaded again.

use super::catalog::Backup;
use super::{BACKUP_JOB, Client};
use crate::codec::sha256;
use crate::content::{ManifestContent, empty_file, seal_manifest};
use crate::files::{BlobStore, FileManifest, MAX_BACKUP, MAX_FILE, Source};
use crate::hold::Hold;
use crate::now_ms;
use crate::upload::{ChunkRef, PartProgress, UploadPlan, cancel_upload, random_hex, upload_parts};
use dmsg_types::{Error, Result, ensure, user::VaultWriteState};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// The upload kind of a backup.
pub const BACKUP: &str = "backup";
/// The format of a backup manifest's content.
pub const BACKUP_FORMAT: &str = "dmsg-backup/1";
/// A failed backup waits this long before its next attempt.
pub const RETRY_MS: u64 = 2_000;

/// What a backup's sealed manifest carries: the file and where it was.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BackupManifest {
    /// [`BACKUP_FORMAT`].
    pub format: String,
    /// The version's file manifest, key included.
    pub file: FileManifest,
    /// The names of the file's folders, from the root.
    pub path: Vec<String>,
    /// The file's tags.
    pub tags: Vec<String>,
    /// The file's note.
    pub note: String,
    /// When the version was made, Unix milliseconds.
    pub created_at: u64,
    /// When the file was last changed, Unix milliseconds.
    pub modified_at: u64,
    /// The device that made the version, hex.
    pub device: String,
}

/// A backup in flight: ciphertext references, the sealed manifest and
/// progress; never a key.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct BackupJob {
    pub(crate) file: String,
    pub(crate) version: String,
    pub(crate) size: u64,
    pub(crate) plan: UploadPlan,
    pub(crate) manifest: ByteBuf,
    pub(crate) sent: u64,
    #[serde(default)]
    pub(crate) acknowledged: std::collections::BTreeSet<usize>,
    #[serde(default)]
    pub(crate) attempts: u32,
    #[serde(default)]
    pub(crate) stage: String,
    #[serde(default)]
    pub(crate) updated_at: u64,
    pub(crate) created_at: u64,
    /// The error code of the last attempt, if it failed.
    pub(crate) error: Option<String>,
    /// When the last attempt failed.
    pub(crate) failed_at: Option<u64>,
    /// Committed on the relay; the version record follows once unlocked.
    pub(crate) committed: bool,
}

/// A backup job as it leaves the daemon.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct BackupJobView {
    /// File ID.
    pub file: String,
    /// Version ID.
    pub version: String,
    /// The upload.
    pub upload_id: String,
    /// Plaintext length.
    pub size: u64,
    /// Chunks the relay holds.
    pub sent: u64,
    /// Chunks in all.
    pub chunks: u64,
    /// Unix milliseconds.
    pub created_at: u64,
    /// The last attempt's error code.
    pub error: Option<String>,
    /// Committed; waiting for the device to unlock.
    pub committed: bool,
    /// Attempts since the last explicit retry.
    pub attempts: u32,
    /// Queued, uploading, committing, paused, retrying, failed or done.
    pub stage: String,
    /// Next automatic retry, in Unix milliseconds.
    pub retry_at: Option<u64>,
    /// Last durable progress update, in Unix milliseconds.
    pub updated_at: u64,
    /// Acknowledged ciphertext bytes.
    pub bytes: u64,
    /// Total ciphertext bytes.
    pub total_bytes: u64,
}

/// A version a backup request left out, and why.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    /// File ID.
    pub file: String,
    /// Version ID.
    pub version: String,
    /// `backed_up`, `in_progress`, `too_large` or `not_local`.
    pub reason: &'static str,
}

/// What a backup request started.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct BackupRequest {
    /// Jobs created.
    pub started: Vec<BackupJobView>,
    /// Versions left out.
    pub skipped: Vec<Skipped>,
    /// Plaintext bytes the started jobs will put in the cloud.
    /// Acknowledged ciphertext bytes.
    pub bytes: u64,
}

/// What one pass over the backup jobs did.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct BackupRun {
    /// Jobs that finished.
    pub completed: u64,
    /// Jobs that failed this pass and wait for the next.
    pub failed: u64,
    /// Jobs left, failed ones included.
    pub pending: u64,
    /// The pass paused because the device locked.
    pub paused: bool,
    /// Newly acknowledged bytes retained during this pass.
    pub bytes: u64,
}

fn view(job: &BackupJob, can_sign: bool) -> BackupJobView {
    BackupJobView {
        file: job.file.clone(),
        version: job.version.clone(),
        upload_id: job.plan.upload_id.clone(),
        size: job.size,
        sent: job.sent,
        chunks: job.plan.chunks.len() as u64,
        created_at: job.created_at,
        error: job.error.clone(),
        committed: job.committed,
        attempts: job.attempts,
        stage: if !can_sign {
            "paused".into()
        } else if job.committed {
            "committing".into()
        } else if job.error.is_some() {
            if retry_at(job).is_some() {
                "retrying"
            } else {
                "failed"
            }
            .into()
        } else if job.stage.is_empty() {
            "queued".into()
        } else {
            job.stage.clone()
        },
        retry_at: retry_at(job),
        updated_at: job.updated_at,
        bytes: acknowledged(job)
            .iter()
            .filter_map(|i| job.plan.chunks.get(*i))
            .map(|c| c.size)
            .sum(),
        total_bytes: job.plan.chunks.iter().map(|c| c.size).sum(),
    }
}

fn job_key(file: &str, version: &str) -> String {
    format!("{BACKUP_JOB}{file}:{version}")
}

impl Client {
    /// Every backup job.
    pub(crate) fn backup_jobs(&self) -> Result<Vec<(String, BackupJob)>> {
        self.workspace.store.library_jobs()
    }

    /// `backing_up` or `failed` while a version's backup job is in flight.
    pub(crate) fn backup_state(&self, file: &str, version: &str) -> Result<Option<&'static str>> {
        Ok(self
            .workspace
            .store
            .library_job::<BackupJob>(&job_key(file, version))?
            .map(|job| match job.error {
                Some(_) => "failed",
                None => "backing_up",
            }))
    }

    /// The backup jobs in flight; readable while locked.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn files_backup_status(&self) -> Result<Vec<BackupJobView>> {
        Ok(self
            .backup_jobs()?
            .iter()
            .map(|(_, job)| view(job, self.signing().is_ok()))
            .collect())
    }

    /// The sealed plan and manifest of a version's backup under the current
    /// root; the chunks are the version's own.
    pub(crate) fn plan_backup(
        &mut self,
        file_id: &str,
        version_id: &str,
        now: u64,
    ) -> Result<BackupJob> {
        let (file, version, manifest) = self.library_manifest(file_id, Some(version_id))?;
        ensure(manifest.size <= MAX_BACKUP, Error::QuotaExceeded)?;
        let path = self.catalog()?.path(&file.folder);
        let account = self.account()?.id.to_string();
        let roots = self.roots()?;
        let generation = roots.generation;
        let root = roots.key(generation).ok_or(Error::RecoveryIncomplete)?;
        let blobs = self.blobs()?;
        let chunks = if manifest.chunks.is_empty() {
            let empty = empty_file();
            let digest = hex::encode(*sha256(&empty));
            blobs.put_chunk(&digest, &empty)?;
            vec![ChunkRef {
                digest,
                size: empty.len() as u64,
            }]
        } else {
            manifest
                .chunks
                .iter()
                .map(|c| {
                    let size = blobs.size(&c.digest)?.ok_or(Error::RecoveryIncomplete)?;
                    Ok(ChunkRef {
                        digest: c.digest.clone(),
                        size,
                    })
                })
                .collect::<Result<_>>()?
        };
        let mut plan = UploadPlan {
            management: None,
            upload_id: random_hex(),
            object_id: random_hex(),
            version_id: random_hex(),
            root_generation: generation,
            epoch: 0,
            control_head: None,
            kind: BACKUP.into(),
            chunks,
            manifest_digest: String::new(),
            manifest_size: 0,
            expires_at: crate::upload::expires_at(now),
        };
        let record = BackupManifest {
            format: BACKUP_FORMAT.into(),
            file: manifest,
            path,
            tags: file.tags.clone(),
            note: file.note.clone(),
            created_at: version.created_at,
            modified_at: file.updated_at,
            device: version.device.clone(),
        };
        let sealed = seal_manifest(root, &account, &mut plan, ManifestContent::Backup(&record));
        plan.management = Some(self.seal_management(
            &plan,
            &serde_json::json!({
                "name":record.file.name,"source_file":file_id,"source_version":version_id,
                "plain_bytes":record.file.size,"intent":{"kind":"backup"}
            }),
        )?);
        Ok(BackupJob {
            file: file_id.into(),
            version: version_id.into(),
            size: version.size,
            plan,
            manifest: ByteBuf::from(sealed),
            sent: 0,
            acknowledged: Default::default(),
            attempts: 0,
            stage: "queued".into(),
            updated_at: now,
            created_at: now,
            error: None,
            failed_at: None,
            committed: false,
        })
    }

    /// The versions `ids` names to back up, each planned, and those left
    /// out: each file's current version, or every version with
    /// `all_versions`; folders recursively.
    fn plan_backups(
        &mut self,
        ids: &[String],
        all_versions: bool,
        max_file_bytes: u64,
        now: u64,
    ) -> Result<(BackupRequest, Vec<BackupJob>)> {
        let mut targets: Vec<(String, String)> = Vec::new();
        for id in ids {
            let files: Vec<String> = {
                let catalog = self.catalog()?;
                if catalog.files.contains_key(id) {
                    vec![id.clone()]
                } else if catalog.has_folder(id) {
                    catalog
                        .subtree(id)
                        .iter()
                        .flat_map(|folder| catalog.files_in(folder))
                        .filter(|f| !catalog.file_trashed(&catalog.files[f]))
                        .collect()
                } else {
                    return Err(Error::NotFound);
                }
            };
            for file in files {
                let record = self.file_record(&file)?;
                match all_versions {
                    true => {
                        targets.extend(record.versions.iter().map(|v| (file.clone(), v.id.clone())))
                    }
                    false => targets.push((file.clone(), record.current.clone())),
                }
            }
        }
        targets.sort();
        targets.dedup();
        let mut request = BackupRequest::default();
        let mut jobs = Vec::new();
        for (file, version_id) in targets {
            let version = self.version(&file, &version_id)?;
            let key = job_key(&file, &version_id);
            if let Some(mut pending) = self.workspace.store.library_job::<BackupJob>(&key)? {
                if pending.error.is_some() {
                    pending.error = None;
                    pending.failed_at = None;
                    pending.attempts = 0;
                    pending.stage = "queued".into();
                    pending.updated_at = now;
                    self.workspace.store.set_library_job(&key, &pending)?;
                    request.bytes += pending.size;
                    request.started.push(view(&pending, true));
                } else {
                    request.skipped.push(Skipped {
                        file,
                        version: version_id,
                        reason: "in_progress",
                    });
                }
                continue;
            }
            let reason = if version.backup.is_some() {
                Some("backed_up")
            } else if version.size > max_file_bytes {
                Some("too_large")
            } else if !version.local {
                Some("not_local")
            } else {
                None
            };
            match reason {
                Some(reason) => request.skipped.push(Skipped {
                    file,
                    version: version_id,
                    reason,
                }),
                None => {
                    let job = self.plan_backup(&file, &version_id, now)?;
                    request.bytes += job.size;
                    jobs.push(job);
                }
            }
        }
        Ok((request, jobs))
    }

    /// Record a committed backup on its version and drop the job; `false`
    /// while locked, when it waits for the unlock.
    pub(crate) fn settle_backup(&mut self, key: &str, job: &BackupJob, now: u64) -> Result<bool> {
        if self.keys.is_none() {
            return Ok(false);
        }
        match self.version(&job.file, &job.version) {
            Ok(mut version) => {
                version.backup = Some(Backup {
                    upload_id: job.plan.upload_id.clone(),
                    generation: job.plan.root_generation,
                    backed_up_at: now,
                    verified: true,
                });
                self.save_version(&job.file, &job.version, &version)?;
            }
            // The version went while the backup was in flight: the cloud
            // copy stays, as any other backup of a deleted file does.
            Err(Error::NotFound) => {}
            Err(error) => return Err(error),
        }
        self.workspace.store.delete_library_job(key)?;
        self.library = None;
        Ok(true)
    }
}

/// Back up files and folders to the cloud: each file's current version, or
/// every version with `all_versions`; folders recursively. A version
/// already backed up, in flight, over the membership's cloud backup bound or not on
/// this device is skipped and named. Persist the jobs and return; the daemon
/// runs them in the background, locked too with a resident device key.
///
/// # Errors
/// Returns `Error::RekeyRequired` while the account waits for a new root,
/// `Error::QuotaExceeded` when the cloud has no room for all of it (nothing
/// starts then) and `Error::NotFound` for an unknown ID.
pub async fn backup<H: Hold>(h: &H, ids: &[String], all_versions: bool) -> Result<BackupRequest> {
    backup_checked(h, ids, all_versions, None).await
}

/// Plan exactly the file version the user selected, rechecking after quota I/O.
pub async fn backup_version<H: Hold>(h: &H, id: &str, version: &str) -> Result<BackupRequest> {
    backup_checked(h, &[id.to_string()], false, Some((id, version))).await
}

async fn backup_checked<H: Hold>(
    h: &H,
    ids: &[String],
    all_versions: bool,
    expected: Option<(&str, &str)>,
) -> Result<BackupRequest> {
    let (_, security) = h
        .signed(|c| {
            c.roots()?;
            c.fresh()
        })
        .await?;
    ensure(
        security.snapshot.vault_write_state == VaultWriteState::Ready,
        Error::RekeyRequired,
    )?;
    let usage = super::cloud::cloud_usage(h).await?;
    let max_file_bytes = usage.backup_max_file_bytes.ok_or(Error::MembershipStale)?;
    ensure(
        matches!(max_file_bytes, MAX_FILE | MAX_BACKUP),
        Error::IntegrityFailed,
    )?;
    let (mut request, jobs) = h
        .with(|c| {
            if let Some((id, version)) = expected {
                ensure(
                    c.file_record(id)?.current == version,
                    Error::VersionConflict,
                )?;
            }
            c.plan_backups(ids, all_versions, max_file_bytes, now_ms())
        })
        .await?;
    if !jobs.is_empty() {
        // The cloud's room, as the relay projects it; the relay checks
        // again at each reservation.
        if let Some(limit) = usage.storage_limit {
            let planned: u64 = jobs
                .iter()
                .map(|j| j.plan.chunks.iter().map(|c| c.size).sum::<u64>() + j.plan.manifest_size)
                .sum();
            ensure(
                usage.reserved + usage.committed + planned <= limit,
                Error::QuotaExceeded,
            )?;
        }
    }
    h.signed(|c| {
        if let Some((id, version)) = expected {
            ensure(
                c.file_record(id)?.current == version,
                Error::VersionConflict,
            )?;
        }
        c.root_now()?;
        let generation = c.roots()?.generation;
        for job in &jobs {
            ensure(job.plan.root_generation == generation, Error::RekeyRequired)?;
            let version = c.version(&job.file, &job.version)?;
            ensure(
                version.local && version.backup.is_none(),
                Error::VersionConflict,
            )?;
        }
        for job in &jobs {
            c.workspace
                .store
                .set_library_job(&job_key(&job.file, &job.version), job)?;
        }
        Ok(())
    })
    .await?;
    request
        .started
        .extend(jobs.iter().map(|job| view(job, true)));
    Ok(request)
}

fn acknowledged(job: &BackupJob) -> std::collections::BTreeSet<usize> {
    if job.acknowledged.is_empty() {
        (0..job.sent as usize).collect()
    } else {
        job.acknowledged.clone()
    }
}

fn retry_at(job: &BackupJob) -> Option<u64> {
    job.failed_at
        .filter(|_| {
            matches!(
                job.error.as_deref(),
                Some(
                    "UNAVAILABLE"
                        | "NETWORK_UNAVAILABLE"
                        | "SYNC_BUSY"
                        | "EXECUTION_UNKNOWN"
                        | "POLICY_STALE"
                        | "AUTH_REQUIRED"
                )
            )
        })
        .map(|at| {
            at.saturating_add((RETRY_MS << job.attempts.saturating_sub(1).min(5)).min(60_000))
        })
}

impl Client {
    /// Explicit retry reuses each original upload and its acknowledged chunks.
    ///
    /// # Errors
    /// Returns session or storage errors.
    pub fn files_backup_retry(&self, file: Option<&str>, version: Option<&str>) -> Result<()> {
        self.keys()?;
        let now = now_ms();
        for (key, mut job) in self.backup_jobs()? {
            if file.is_none_or(|f| f == job.file) && version.is_none_or(|v| v == job.version) {
                job.error = None;
                job.failed_at = None;
                job.attempts = 0;
                job.stage = "queued".into();
                job.updated_at = now;
                self.workspace.store.set_library_job(&key, &job)?;
            }
        }
        Ok(())
    }
}

/// Run saved backups continuously. The caller serializes passes; network waits
/// never hold Client. Lock pauses without a failure cooldown, and retries keep
/// the original upload and every independently acknowledged block.
pub async fn backup_run<H: Hold>(
    h: &H,
    progress: &mut (dyn FnMut(&BackupJobView) + Send),
) -> Result<BackupRun> {
    let jobs = h
        .with(|c| {
            c.signing()?;
            c.backup_jobs()
        })
        .await?;
    let mut run = BackupRun::default();
    for (key, mut job) in jobs {
        let now = now_ms();
        if job.committed {
            if h.with(|c| c.settle_backup(&key, &job, now)).await? {
                let mut completed = view(&job, true);
                completed.stage = "done".into();
                progress(&completed);
                run.completed += 1;
            } else {
                run.pending += 1;
            }
            continue;
        }
        if run.paused
            || job
                .failed_at
                .is_some_and(|_| retry_at(&job).is_none_or(|at| now < at))
        {
            run.pending += 1;
            continue;
        }
        job.error = None;
        job.failed_at = None;
        job.stage = "uploading".into();
        job.updated_at = now;
        job.attempts = job.attempts.saturating_add(1);
        h.with(|c| c.workspace.store.set_library_job(&key, &job))
            .await?;
        let before_bytes = view(&job, true).bytes;
        let outcome = send_backup(h, &key, &mut job, progress).await;
        let now = now_ms();
        h.with(|c| {
            let Some(mut stored) = c.workspace.store.library_job::<BackupJob>(&key)? else {
                return Ok(());
            };
            run.bytes += view(&stored, true).bytes.saturating_sub(before_bytes);
            stored.updated_at = now;
            match outcome {
                Ok(()) => {
                    stored.committed = true;
                    stored.error = None;
                    stored.failed_at = None;
                    c.workspace.store.set_library_job(&key, &stored)?;
                    if c.settle_backup(&key, &stored, now)? {
                        let mut completed = view(&stored, true);
                        completed.stage = "done".into();
                        progress(&completed);
                        run.completed += 1;
                    } else {
                        run.pending += 1;
                    }
                }
                Err(ref error) if matches!(error, Error::Locked) || c.signing().is_err() => {
                    stored.stage = "paused".into();
                    stored.error = None;
                    stored.failed_at = None;
                    c.workspace.store.set_library_job(&key, &stored)?;
                    run.paused = true;
                    run.pending += 1;
                }
                Err(error) => {
                    stored.error = Some(crate::error_code(&error));
                    stored.failed_at = Some(now);
                    stored.stage = "failed".into();
                    if let Some(delay) = crate::transport::retry_after(&error) {
                        // Keep the server's minimum retry time, with the same job.
                        stored.failed_at = Some(now.saturating_add(delay).saturating_sub(RETRY_MS));
                        stored.attempts = 1;
                    }
                    c.workspace.store.set_library_job(&key, &stored)?;
                    run.failed += 1;
                    run.pending += 1;
                }
            }
            Ok(())
        })
        .await?;
    }
    Ok(run)
}

async fn send_backup<H: Hold>(
    h: &H,
    key: &str,
    job: &mut BackupJob,
    progress: &mut (dyn FnMut(&BackupJobView) + Send),
) -> Result<()> {
    let base = h.with(|c| c.base()).await?;
    for attempt in 0..2 {
        let plan = job.plan.clone();
        let outcome = upload_parts(
            h,
            &base,
            &plan,
            &job.manifest,
            &acknowledged(job),
            Source::Library,
            |c, event| {
                let mut stored: BackupJob =
                    c.workspace.store.library_job(key)?.ok_or(Error::NotFound)?;
                ensure(stored.plan == plan, Error::VersionConflict)?;
                match event {
                    PartProgress::Reset => {
                        stored.acknowledged.clear();
                        stored.sent = 0;
                    }
                    PartProgress::Stored(index) => {
                        stored.acknowledged = acknowledged(&stored);
                        stored.acknowledged.insert(index);
                        stored.sent = stored.acknowledged.len() as u64;
                        stored.stage = "uploading".into();
                    }
                    PartProgress::Committing => stored.stage = "committing".into(),
                }
                stored.updated_at = now_ms();
                c.workspace.store.set_library_job(key, &stored)?;
                progress(&view(&stored, c.signing().is_ok()));
                Ok(())
            },
        )
        .await;
        match outcome {
            Ok(_) => return Ok(()),
            Err(Error::ResultExpired)
                if attempt == 0 && h.with(|c| Ok(c.keys.is_some())).await? =>
            {
                cancel_upload(h, &job.plan.upload_id).await?;
                *job = h
                    .with(|c| {
                        let fresh = c.plan_backup(&job.file, &job.version, now_ms())?;
                        c.workspace.store.set_library_job(key, &fresh)?;
                        Ok(fresh)
                    })
                    .await?;
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!()
}

/// Cancel a version's backup in flight: the staging upload is cancelled on
/// the relay and the job goes. A committed backup is not a job any more;
/// delete it from the cloud instead.
///
/// # Errors
/// Returns `Error::NotFound` without such a job.
pub async fn backup_cancel<H: Hold>(h: &H, file: &str, version: &str) -> Result<()> {
    let key = job_key(file, version);
    let job: BackupJob = h
        .with(|c| {
            let job: BackupJob = c
                .workspace
                .store
                .library_job(&key)?
                .ok_or(Error::NotFound)?;
            if job.committed {
                // Settle what is committed instead of losing track of it.
                ensure(c.settle_backup(&key, &job, now_ms())?, Error::Locked)?;
                return Err(Error::VersionConflict);
            }
            Ok(job)
        })
        .await?;
    cancel_upload(h, &job.plan.upload_id).await?;
    h.with(|c| c.workspace.store.delete_library_job(&key)).await
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    use serde_json::Value;
    use std::sync::Mutex;

    struct Interrupted(Mutex<Client>);
    impl Hold for Interrupted {
        async fn with<T: Send>(
            &self,
            step: impl FnOnce(&mut Client) -> Result<T> + Send,
        ) -> Result<T> {
            step(&mut self.0.lock().unwrap())
        }
        async fn find(&self, _: &str) -> Result<Option<Value>> {
            Err(Error::Locked)
        }
    }

    #[tokio::test]
    async fn lock_pauses_without_a_failure_deadline_and_manual_retry_reuses_the_plan() {
        let dir = crate::testing::TempDir::new();
        let mut c = crate::testing::unlocked(&dir.0, false, None);
        let source = dir.path("lock.txt");
        std::fs::write(&source, b"resume me").unwrap();
        let imported = c
            .files_import(&source, super::super::catalog::ROOT, false, &mut |_, _| {})
            .unwrap();
        let job = c
            .plan_backup(&imported.file.id, &imported.version, now_ms())
            .unwrap();
        let key = job_key(&job.file, &job.version);
        c.workspace.store.set_library_job(&key, &job).unwrap();
        let h = Interrupted(Mutex::new(c));
        let result = backup_run(&h, &mut |_| {}).await.unwrap();
        assert!(result.paused);
        assert_eq!(result.failed, 0);
        h.with(|c| {
            let mut stored: BackupJob = c.workspace.store.library_job(&key)?.unwrap();
            assert!(stored.error.is_none() && stored.failed_at.is_none());
            stored.error = Some("UNAVAILABLE".into());
            stored.failed_at = Some(now_ms());
            stored.attempts = 1;
            for code in ["UNAVAILABLE", "NETWORK_UNAVAILABLE", "SYNC_BUSY"] {
                stored.error = Some(code.into());
                assert_eq!(retry_at(&stored), stored.failed_at.map(|at| at + 2000));
            }
            c.workspace.store.set_library_job(&key, &stored)?;
            let (request, new_jobs) =
                c.plan_backups(std::slice::from_ref(&job.file), false, MAX_BACKUP, now_ms())?;
            assert!(new_jobs.is_empty());
            assert!(request.skipped.is_empty());
            assert_eq!(request.started[0].upload_id, job.plan.upload_id);
            c.files_backup_retry(Some(&job.file), Some(&job.version))?;
            let retried: BackupJob = c.workspace.store.library_job(&key)?.unwrap();
            assert_eq!(retried.plan, job.plan);
            assert!(retried.failed_at.is_none() && retried.error.is_none());
            Ok(())
        })
        .await
        .unwrap();
    }
}
