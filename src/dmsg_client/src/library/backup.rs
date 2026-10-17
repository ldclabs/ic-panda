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
//! attempt waits [`RETRY_MS`] for the next. Nothing creates a job but
//! [`Client::files_backup`]: not an import, not an edit, not a cloud
//! inventory refresh, and a backup the person deleted from the cloud is not
//! uploaded again.

use super::catalog::Backup;
use super::{BACKUP_JOB, Client};
use crate::codec::sha256;
use crate::content::{ManifestContent, empty_file, seal_manifest};
use crate::files::{BlobStore, FileManifest, MAX_FILE, Source};
use crate::now_ms;
use crate::upload::{ChunkRef, UploadPlan, random_hex};
use dmsg_types::{DAY, Error, MINUTE, Result, ensure, user::VaultWriteState};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// The upload kind of a backup.
pub const BACKUP: &str = "backup";
/// The format of a backup manifest's content.
pub const BACKUP_FORMAT: &str = "dmsg-backup/1";
/// A failed backup waits this long before its next attempt.
pub const RETRY_MS: u64 = 5 * MINUTE;

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
    pub(crate) created_at: u64,
    /// The error code of the last attempt, if it failed.
    pub(crate) error: Option<String>,
    /// When the last attempt failed.
    pub(crate) failed_at: Option<u64>,
    /// The plan was replaced once after it lapsed.
    pub(crate) replanned: bool,
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
    /// The pass stopped at its time budget; the next one goes on from the
    /// last chunk sent.
    pub paused: bool,
}

fn view(job: &BackupJob) -> BackupJobView {
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
            .map(|(_, job)| view(job))
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
            expires_at: now + DAY,
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
        Ok(BackupJob {
            file: file_id.into(),
            version: version_id.into(),
            size: version.size,
            plan,
            manifest: ByteBuf::from(sealed),
            sent: 0,
            created_at: now,
            error: None,
            failed_at: None,
            replanned: false,
            committed: false,
        })
    }

    /// Back up files and folders to the cloud: each file's current version,
    /// or every version with `all_versions`; folders recursively. A version
    /// already backed up, in flight, over the cloud's 100 MiB bound or not on
    /// this device is skipped and named. The jobs start at once and continue
    /// in the background, locked too with a resident device key.
    ///
    /// # Errors
    /// Returns `Error::RekeyRequired` while the account waits for a new
    /// root, `Error::QuotaExceeded` when the cloud has no room for all of
    /// it (nothing starts then) and `Error::NotFound` for an unknown ID.
    pub async fn files_backup(
        &mut self,
        ids: &[String],
        all_versions: bool,
    ) -> Result<BackupRequest> {
        self.roots()?;
        let security = self.cloud_state().await?;
        ensure(
            security.snapshot.vault_write_state == VaultWriteState::Ready,
            Error::RekeyRequired,
        )?;
        let now = now_ms();
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
            let reason = if version.backup.is_some() {
                Some("backed_up")
            } else if self.backup_state(&file, &version_id)?.is_some() {
                Some("in_progress")
            } else if version.size > MAX_FILE {
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
        if !jobs.is_empty() {
            // The cloud's room, as the relay projects it; the relay checks
            // again at each reservation.
            let usage = self.files_cloud_usage().await?;
            if let Some(limit) = usage.storage_limit {
                let planned: u64 = jobs
                    .iter()
                    .map(|j| {
                        j.plan.chunks.iter().map(|c| c.size).sum::<u64>() + j.plan.manifest_size
                    })
                    .sum();
                ensure(
                    usage.reserved + usage.committed + planned <= limit,
                    Error::QuotaExceeded,
                )?;
            }
        }
        for job in &jobs {
            self.workspace
                .store
                .set_library_job(&job_key(&job.file, &job.version), job)?;
            request.started.push(view(job));
        }
        if !jobs.is_empty() {
            // Failures stay in the jobs for the next pass.
            let _ = self.files_backup_run(None, &mut |_, _, _| {}).await;
        }
        Ok(request)
    }

    /// One pass over the backup jobs: upload what is left of each, commit,
    /// and record the version's backup. Works locked with a resident
    /// device key; the version record then follows at the next pass after
    /// an unlock. A job that failed less than [`RETRY_MS`] ago waits.
    /// With `budget_ms`, no chunk starts after that long and the pass says
    /// it `paused`, so other work gets the client in between.
    /// `progress(file, sent, chunks)` follows each upload.
    ///
    /// # Errors
    /// Returns `Error::Locked` without a signing key; a job's own failure
    /// is recorded in the job and counted, never returned.
    pub async fn files_backup_run(
        &mut self,
        budget_ms: Option<u64>,
        progress: &mut (dyn FnMut(&str, u64, u64) + Send),
    ) -> Result<BackupRun> {
        self.signing()?;
        let deadline = budget_ms.map(|ms| now_ms().saturating_add(ms));
        let mut run = BackupRun::default();
        for (key, mut job) in self.backup_jobs()? {
            // Each job's time is read after the uploads before it.
            let now = now_ms();
            if job.committed {
                if self.settle_backup(&key, &job, now)? {
                    run.completed += 1;
                } else {
                    run.pending += 1;
                }
                continue;
            }
            if run.paused || job.failed_at.is_some_and(|at| now < at + RETRY_MS) {
                run.pending += 1;
                continue;
            }
            let mut paused = false;
            let outcome = self
                .send_backup(&key, &mut job, deadline, &mut paused, progress)
                .await;
            let now = now_ms();
            match outcome {
                // Its chunks so far are recorded; nothing failed.
                Err(_) if paused => {
                    run.paused = true;
                    run.pending += 1;
                }
                Ok(()) => {
                    job.committed = true;
                    job.error = None;
                    job.failed_at = None;
                    self.workspace.store.set_library_job(&key, &job)?;
                    if self.settle_backup(&key, &job, now)? {
                        run.completed += 1;
                    } else {
                        run.pending += 1;
                    }
                }
                Err(error) => {
                    job.error = Some(crate::error_code(&error));
                    job.failed_at = Some(now);
                    self.workspace.store.set_library_job(&key, &job)?;
                    run.failed += 1;
                    run.pending += 1;
                }
            }
        }
        Ok(run)
    }

    /// Upload one job, resuming after the chunks it recorded; a lapsed plan
    /// is replaced once, which needs the device unlocked. Past `deadline`
    /// it stops after a recorded chunk and sets `paused`.
    async fn send_backup(
        &mut self,
        key: &str,
        job: &mut BackupJob,
        deadline: Option<u64>,
        paused: &mut bool,
        progress: &mut (dyn FnMut(&str, u64, u64) + Send),
    ) -> Result<()> {
        let base = self.base()?;
        let chunks = job.plan.chunks.len() as u64;
        let file = job.file.clone();
        let mut step = |client: &Self, sent: usize| {
            progress(&file, sent as u64, chunks);
            let mut stored: BackupJob = client
                .workspace
                .store
                .library_job(key)?
                .ok_or(Error::IntegrityFailed)?;
            stored.sent = sent as u64;
            client.workspace.store.set_library_job(key, &stored)?;
            if (sent as u64) < chunks && deadline.is_some_and(|d| now_ms() >= d) {
                *paused = true;
                return Err(Error::Pending);
            }
            Ok(())
        };
        let sent = job.sent as usize;
        match self
            .upload(
                &base,
                &job.plan,
                &job.manifest,
                sent,
                Source::Library,
                &mut step,
            )
            .await
        {
            Ok(()) => {
                job.sent = chunks;
                Ok(())
            }
            Err(Error::ResultExpired) if !job.replanned && self.keys.is_some() => {
                self.cancel_upload(&job.plan.upload_id).await?;
                let fresh = self.plan_backup(&job.file, &job.version, now_ms())?;
                job.plan = fresh.plan;
                job.manifest = fresh.manifest;
                job.sent = 0;
                job.replanned = true;
                self.workspace.store.set_library_job(key, job)?;
                self.upload(
                    &base,
                    &job.plan,
                    &job.manifest,
                    0,
                    Source::Library,
                    &mut step,
                )
                .await?;
                job.sent = chunks;
                Ok(())
            }
            Err(error) => Err(error),
        }
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

    /// Cancel a version's backup in flight: the staging upload is cancelled
    /// on the relay and the job goes. A committed backup is not a job any
    /// more; delete it from the cloud instead.
    ///
    /// # Errors
    /// Returns `Error::NotFound` without such a job.
    pub async fn files_backup_cancel(&mut self, file: &str, version: &str) -> Result<()> {
        let key = job_key(file, version);
        let job: BackupJob = self
            .workspace
            .store
            .library_job(&key)?
            .ok_or(Error::NotFound)?;
        if job.committed {
            // Settle what is committed instead of losing track of it.
            ensure(self.settle_backup(&key, &job, now_ms())?, Error::Locked)?;
            return Err(Error::VersionConflict);
        }
        self.cancel_upload(&job.plan.upload_id).await?;
        self.workspace.store.delete_library_job(&key)
    }
}
