//! Durable, explicit cloud downloads. Only immutable IDs and progress are kept;
//! lock or host cancellation leaves the same task resumable on unlock.
use super::{Client, cloud};
use crate::{
    hold::{Hold, Unlocked},
    now_ms,
};
use dmsg_types::{Error, Result, ensure};
use serde::{Deserialize, Serialize};

const PREFIX: &str = "lib-fetch:";

#[derive(Clone, Debug, Serialize, Deserialize)]
/// A resumable download in the account-encrypted workspace.
pub struct FetchJob {
    /// Immutable cloud upload identity.
    pub upload_id: String,
    /// Destination folder, or the backed-up location.
    pub folder: Option<String>,
    /// Verify a backup without importing its catalog entry.
    pub verify: bool,
    /// Queued, downloading, verifying, retrying, failed or done.
    pub stage: String,
    /// Verified chunks stored on this device.
    pub done: u64,
    /// Expected number of chunks.
    pub total: u64,
    /// Verified ciphertext bytes on this device.
    pub bytes: u64,
    /// Expected ciphertext bytes.
    pub total_bytes: u64,
    /// Explicit request time, Unix milliseconds.
    pub created_at: u64,
    /// Last durable progress time, Unix milliseconds.
    pub updated_at: u64,
    /// Last failure code.
    pub error: Option<String>,
    /// Next automatic retry, or none when a user action is needed.
    pub retry_at: Option<u64>,
    /// Consecutive attempts since an explicit retry.
    pub attempts: u32,
}

#[derive(Default, Serialize)]
/// Result of one serialized download pass.
pub struct FetchRun {
    /// Transfers fully verified and settled.
    pub completed: u64,
    /// Transfers that failed this pass.
    pub failed: u64,
    /// Transfers left to finish.
    pub pending: u64,
    /// Newly verified bytes retained during this pass.
    pub bytes: u64,
}

impl Client {
    /// Read durable downloads for the unlocked account.
    ///
    /// # Errors
    /// Returns session, storage or invalid target errors.
    pub fn files_download_status(&self) -> Result<Vec<FetchJob>> {
        self.keys()?;
        self.workspace
            .store
            .sealed_keys(PREFIX)?
            .into_iter()
            .map(|key| {
                self.workspace
                    .private(self.keys()?, &key)?
                    .ok_or(Error::NotFound)
            })
            .collect()
    }

    pub(crate) fn save_fetch(&self, job: &FetchJob) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{PREFIX}{}", job.upload_id), job)
    }

    /// Queue a download, reusing any unfinished task for the upload.
    ///
    /// # Errors
    /// Returns session, storage or invalid target errors.
    pub fn files_download_start(
        &mut self,
        upload: &str,
        folder: Option<&str>,
        verify: bool,
    ) -> Result<FetchJob> {
        let now = now_ms();
        for mut job in self.files_download_status()? {
            if job.upload_id == upload && job.stage != "done" {
                // A repeated click may join this request, but a verification
                // must never silently replace a restore or its destination.
                ensure(
                    job.verify == verify && (verify || job.folder.as_deref() == folder),
                    Error::VersionConflict,
                )?;
                if job.error.is_some() {
                    job.error = None;
                    job.retry_at = None;
                    job.attempts = 0;
                    job.stage = "queued".into();
                    job.updated_at = now;
                    self.save_fetch(&job)?;
                }
                return Ok(job);
            }
            if job.stage == "done" && job.updated_at + dmsg_types::DAY < now {
                self.workspace
                    .delete_private(&format!("{PREFIX}{}", job.upload_id))?;
            }
        }
        let entry = self.cloud_entry(upload)?.ok_or(Error::NotFound)?;
        let manifest = entry.manifest.as_ref().ok_or(Error::RecoveryIncomplete)?;
        if !verify {
            ensure(
                manifest.file.size <= self.config.files_max_bytes,
                Error::QuotaExceeded,
            )?;
        }
        if let Some(folder) = folder {
            self.catalog()?.live_folder(folder)?;
        }
        let job = FetchJob {
            upload_id: upload.into(),
            folder: folder.map(str::to_owned),
            verify,
            stage: "queued".into(),
            done: 0,
            total: manifest.file.chunks.len() as u64,
            bytes: 0,
            total_bytes: entry.plan.chunks.iter().map(|c| c.size).sum(),
            created_at: now,
            updated_at: now,
            error: None,
            retry_at: None,
            attempts: 0,
        };
        self.save_fetch(&job)?;
        Ok(job)
    }

    pub(crate) fn fetch_progress(
        &self,
        upload: &str,
        done: u64,
        bytes: u64,
        stage: &str,
        now: u64,
    ) -> Result<()> {
        if let Some(mut job) = self
            .workspace
            .private::<FetchJob>(self.keys()?, &format!("{PREFIX}{upload}"))?
        {
            job.done = done;
            job.bytes = bytes;
            job.stage = stage.into();
            job.updated_at = now;
            self.save_fetch(&job)?;
        }
        Ok(())
    }

    /// Retry selected downloads without changing their identity or cached blocks.
    ///
    /// # Errors
    /// Returns session, storage or invalid target errors.
    pub fn files_download_retry(&self, upload: Option<&str>) -> Result<()> {
        let now = now_ms();
        for mut job in self.files_download_status()? {
            if upload.is_none_or(|id| id == job.upload_id) && job.stage != "done" {
                job.error = None;
                job.retry_at = None;
                job.attempts = 0;
                job.stage = "queued".into();
                job.updated_at = now;
                self.save_fetch(&job)?;
            }
        }
        Ok(())
    }

    /// Forget a download; in-flight local steps stop before publishing it.
    ///
    /// # Errors
    /// Returns session, storage or invalid target errors.
    pub fn files_download_cancel(&self, upload: &str) -> Result<()> {
        self.keys()?;
        self.workspace.delete_private(&format!("{PREFIX}{upload}"))
    }
}

/// The host takes one download lane. Partial ciphertext is reused and pinned by
/// cloud_fetch; full-file validation must finish before the job becomes done.
///
/// # Errors
/// Returns session or storage failures; per-file failures remain in their jobs.
pub async fn run<H: Hold>(h: &H, changed: &mut (dyn FnMut(&FetchJob) + Send)) -> Result<FetchRun> {
    let scoped = Unlocked::new(h).await?;
    let h = &scoped;
    let jobs = h.with(|c| c.files_download_status()).await?;
    let mut report = FetchRun::default();
    for queued in jobs {
        let Some(mut job) = h
            .with(|c| {
                // A previous transfer may have waited while this one was cancelled
                // or retried. Read and start the current job in one local step.
                let Some(mut job): Option<FetchJob> = c
                    .workspace
                    .private(c.keys()?, &format!("{PREFIX}{}", queued.upload_id))?
                else {
                    return Ok(None);
                };
                let now = now_ms();
                if job.stage == "done" {
                    return Ok(None);
                }
                if job.error.is_some() && job.retry_at.is_none_or(|at| at > now) {
                    report.pending += 1;
                    return Ok(None);
                }
                job.stage = "downloading".into();
                job.error = None;
                job.retry_at = None;
                job.attempts = job.attempts.saturating_add(1);
                job.updated_at = now;
                c.save_fetch(&job)?;
                Ok(Some(job))
            })
            .await?
        else {
            continue;
        };
        changed(&job);
        let before_bytes = job.bytes;
        let upload = job.upload_id.clone();
        let folder = job.folder.clone();
        // The callback runs under Hold::with; retaining it in cloud_fetch would
        // require a second lock, so journal progress through a scoped Hold.
        let tracking = Tracking {
            inner: h,
            upload: &upload,
        };
        let verify = job.verify;
        let mut progress = |done, total| {
            job.done = done;
            job.stage = if done == total {
                "verifying"
            } else {
                "downloading"
            }
            .into();
            changed(&job);
        };
        let result = if verify {
            cloud::verify_download(&tracking, &upload, &mut progress).await
        } else {
            cloud::cloud_fetch(&tracking, &upload, folder.as_deref(), &mut progress)
                .await
                .map(|_| ())
        };
        let now = now_ms();
        h.with(|c| {
            let Some(mut current): Option<FetchJob> = c
                .workspace
                .private(c.keys()?, &format!("{PREFIX}{upload}"))?
            else {
                return Ok(());
            };
            report.bytes += current.bytes.saturating_sub(before_bytes);
            current.updated_at = now;
            match result {
                Ok(()) => {
                    current.stage = "done".into();
                    current.done = current.total;
                    current.bytes = current.total_bytes;
                    report.completed += 1;
                }
                Err(Error::Locked) => return Err(Error::Locked),
                Err(error) => {
                    current.error = Some(crate::error_code(&error));
                    current.stage = "failed".into();
                    if matches!(
                        error,
                        Error::Unavailable(_)
                            | Error::ExecutionUnknown
                            | Error::PolicyStale
                            | Error::AuthRequired
                    ) {
                        current.retry_at = Some(now.saturating_add(
                            crate::transport::retry_after(&error).unwrap_or(
                                (2000 << current.attempts.saturating_sub(1).min(5)).min(60_000),
                            ),
                        ));
                        current.stage = "retrying".into();
                    }
                    report.failed += 1;
                    report.pending += 1;
                }
            }
            c.save_fetch(&current)?;
            changed(&current);
            Ok(())
        })
        .await?;
    }
    Ok(report)
}

/// Check cancellation at every local step without retaining a content key.
struct Tracking<'a, H> {
    inner: &'a H,
    upload: &'a str,
}
impl<H: Hold> Hold for Tracking<'_, H> {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        self.inner
            .with(|c| {
                let key = format!("{PREFIX}{}", self.upload);
                let job: Option<FetchJob> = c.workspace.private(c.keys()?, &key)?;
                ensure(job.is_some(), Error::NotFound)?;
                step(c)
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{
        backup::{BACKUP_FORMAT, BackupManifest},
        catalog::ROOT,
        cloud::CloudEntry,
    };

    fn fixture() -> (crate::testing::TempDir, Client, String, String) {
        let dir = crate::testing::TempDir::new();
        let mut c = crate::testing::unlocked(&dir.0, false, None);
        let source = dir.path("resume.txt");
        std::fs::write(&source, b"verified resumable download").unwrap();
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
                    format: BACKUP_FORMAT.into(),
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
        version.local = false;
        c.save_version(&imported.file.id, &imported.version, &version)
            .unwrap();
        (dir, c, upload, imported.file.id)
    }

    #[tokio::test]
    async fn lock_keeps_one_download_and_unlock_resumes_cached_bytes_before_success() {
        let (_dir, mut c, upload, file) = fixture();
        let job = c.files_download_start(&upload, Some(ROOT), false).unwrap();
        assert_eq!(
            c.files_download_start(&upload, Some(ROOT), false)
                .unwrap()
                .created_at,
            job.created_at
        );
        assert_eq!(c.files_download_status().unwrap().len(), 1);
        assert!(matches!(
            c.files_download_start(&upload, None, true),
            Err(Error::VersionConflict)
        ));
        assert!(matches!(
            c.files_download_start(&upload, None, false),
            Err(Error::VersionConflict)
        ));
        c.lock();
        assert!(matches!(
            run(&c.hold(), &mut |_| {}).await,
            Err(Error::Locked)
        ));
        c.keys = Some(c.workspace.unlock(&crate::testing::SECRET).unwrap());
        let mut stages = Vec::new();
        let report = run(&c.hold(), &mut |job| stages.push(job.stage.clone()))
            .await
            .unwrap();
        assert_eq!(report.completed, 1);
        assert!(stages.iter().any(|stage| stage == "verifying"));
        assert_eq!(stages.last().map(String::as_str), Some("done"));
        assert!(c.files_show(&file).unwrap().file.local);
        let saved = &c.files_download_status().unwrap()[0];
        assert_eq!(saved.bytes, saved.total_bytes);
        assert_eq!(
            run(&c.hold(), &mut |_| panic!("done jobs must not restart"))
                .await
                .unwrap()
                .completed,
            0
        );
    }

    #[tokio::test]
    async fn all_bytes_do_not_mean_success_when_plaintext_integrity_fails() {
        let (_dir, mut c, upload, _) = fixture();
        let mut entry = c.cloud_entry(&upload).unwrap().unwrap();
        entry.manifest.as_mut().unwrap().file.sha256 = "00".repeat(32);
        c.save_cloud_entry(&upload, &entry).unwrap();
        c.files_download_start(&upload, None, false).unwrap();
        let mut stages = Vec::new();
        let report = run(&c.hold(), &mut |job| stages.push(job.stage.clone()))
            .await
            .unwrap();
        assert_eq!(report.failed, 1);
        assert!(!stages.iter().any(|stage| stage == "done"));
        let failed = &c.files_download_status().unwrap()[0];
        assert_eq!(failed.error.as_deref(), Some("INTEGRITY_FAILED"));
        assert!(failed.retry_at.is_none());
        c.files_download_retry(Some(&upload)).unwrap();
        assert!(c.files_download_status().unwrap()[0].error.is_none());
        c.files_download_cancel(&upload).unwrap();
        assert!(c.files_download_status().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancelling_a_queued_download_during_another_transfer_keeps_it_cancelled() {
        use std::sync::{
            Mutex,
            atomic::{AtomicBool, Ordering},
        };

        struct Shared(Mutex<Client>);

        impl Hold for Shared {
            async fn with<T: Send>(
                &self,
                step: impl FnOnce(&mut Client) -> Result<T> + Send,
            ) -> Result<T> {
                step(&mut self.0.lock().unwrap())
            }
        }

        let (_dir, mut c, upload, _) = fixture();
        let cancelled = "ff".repeat(32);
        let mut entry = c.cloud_entry(&upload).unwrap().unwrap();
        entry.plan.upload_id = cancelled.clone();
        c.save_cloud_entry(&cancelled, &entry).unwrap();
        c.files_download_start(&upload, None, true).unwrap();
        c.files_download_start(&cancelled, None, true).unwrap();
        let h = Shared(Mutex::new(c));
        let once = AtomicBool::new(false);
        let report = run(&h, &mut |job| {
            if job.upload_id == upload && !once.swap(true, Ordering::SeqCst) {
                h.0.lock()
                    .unwrap()
                    .files_download_cancel(&cancelled)
                    .unwrap();
            }
        })
        .await
        .unwrap();
        assert_eq!(report.completed, 1);
        assert!(
            h.0.lock()
                .unwrap()
                .files_download_status()
                .unwrap()
                .iter()
                .all(|job| job.upload_id != cancelled)
        );
    }

    #[tokio::test]
    async fn queued_verification_preserves_network_retry_deadlines() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (_dir, mut c, upload, _) = fixture();
        let entry = c.cloud_entry(&upload).unwrap().unwrap();
        c.blobs()
            .unwrap()
            .delete(&entry.plan.chunks[0].digest)
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        crate::testing::relay_to(
            &mut c,
            &format!("http://{}", listener.local_addr().unwrap()),
        );
        crate::testing::warm(&mut c);
        c.files_download_start(&upload, None, true).unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.windows(4).any(|b| b == b"\r\n\r\n") {
                let mut buf = [0; 8192];
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buf[..n]);
            }
            let body = r#"{"ok":false,"error":{"code":"UNAVAILABLE","message":"busy"}}"#;
            socket.write_all(format!("HTTP/1.1 429 Test\r\ncontent-type: application/json\r\nretry-after: 7\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        });
        let before = now_ms();
        let report = run(&c.hold(), &mut |_| {}).await.unwrap();
        server.await.unwrap();
        assert_eq!(report.failed, 1);
        let job = &c.files_download_status().unwrap()[0];
        assert_eq!(job.error.as_deref(), Some("SYNC_BUSY"));
        assert_eq!(job.stage, "retrying");
        assert!(job.retry_at.is_some_and(|at| at >= before + 7000));
    }
}
