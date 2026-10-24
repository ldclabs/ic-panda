//! Approved secret snapshots. Publication is attempted once; uncertain outcomes only query the original ID.
use super::*;
use crate::{
    files::{CHUNK, Source as BlobSource},
    hold::{Hold, Unlocked},
    library::link::{self, LinkRecord, LinkSource},
    upload::UploadPlan,
};
use dmsg_types::{DAY, Hash, MINUTE};
use std::collections::BTreeSet;
const PREFIX: &str = "secret-share:";

/// Explicit attachment selection, never "whatever is current when uploaded".
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AttachmentInput {
    /// Selected Files object identifier.
    pub file_id: String,
    /// Exact immutable Files version.
    pub version_id: String,
}

/// Every field and attachment starts unselected in the UI.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShareInput {
    /// Selected source Vault item.
    pub item_id: String,
    /// Exact source Vault revision.
    pub revision: String,
    /// Explicit selected field IDs, without their values.
    pub field_ids: Vec<String>,
    /// Whether the owner explicitly selected notes.
    pub include_notes: bool,
    /// Explicit selected attachments; never an implicit all-files choice.
    pub attachments: Vec<AttachmentInput>,
    #[serde(default)]
    /// Requested lifetime, bounded to six minutes through seven days.
    pub expires_in_ms: Option<u64>,
    #[serde(default)]
    /// Maximum counted download sessions, not individual reads.
    pub max_download_sessions: Option<u64>,
}

/// A pinned attachment, with its display name frozen at preparation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachmentPlan {
    /// Pinned content source, with no decryption key.
    pub source: process::Source,
    /// Untrusted display name; never an output path.
    pub name: String,
    /// Declared media type.
    pub mime: String,
    /// Exact byte length.
    pub size: u64,
    /// Complete plaintext attachment digest.
    pub sha256: String,
}

/// The complete, value-free approval plan.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Plan {
    /// Explicit owner selection.
    pub input: ShareInput,
    /// Owning account.
    pub account_id: String,
    /// Owning device; grants never synchronize.
    pub device_id: String,
    /// Untrusted source title shown for review.
    pub title: String,
    /// Source Vault template type.
    pub item_type: String,
    /// Explicit field selection or value-free field metadata.
    pub fields: Vec<Value>,
    /// Explicit selected attachments; never an implicit all-files choice.
    pub attachments: Vec<AttachmentPlan>,
    /// Random snapshot identity independent of the source item.
    pub snapshot_id: String,
    /// Original share publication identity, retained across interruptions.
    pub share_id: String,
    /// Original immutable upload identity, retained across interruptions.
    pub upload_id: String,
    /// Independent snapshot file version.
    pub file_version: String,
    /// Creation time in Unix milliseconds.
    pub created_at: u64,
    /// Absolute expiry in Unix milliseconds.
    pub expires_at: u64,
    /// Absolute deadline to upload and publish this original plan.
    pub publish_before: u64,
    /// Maximum counted download sessions, not individual reads.
    pub max_download_sessions: u64,
    /// Whether the independently generated package key is password-wrapped.
    pub password: bool,
    /// Exact package plaintext length.
    pub plain_bytes: u64,
    /// Conservative ciphertext/upload estimate, not a billing quote.
    pub estimated_ciphertext_bytes: u64,
}
impl Plan {
    fn digest(&self) -> String {
        hex::encode(Sha256::digest(canonical(&(
            "dmsg/secret-share-plan/1",
            self,
        ))))
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Job {
    id: String,
    plan: Plan,
    key: String,
    key_wrap: Option<String>,
    phase: String,
    request: Option<String>,
    boot: String,
    session: u64,
    upload: Option<UploadPlan>,
    manifest: Option<String>,
    publish_attempted: bool,
    uploaded_chunks: usize,
    cancel_requested: bool,
    error: Option<String>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

/// Opaque preparation passed to a blocking password worker, never serialized to clients.
pub struct Preparation {
    job: Job,
}
impl Preparation {
    /// Password derivation runs outside the client lock; the password is not retained.
    pub fn wrap_password(mut self, password: Option<&str>) -> Result<Self> {
        if let Some(password) = password {
            let key = Zeroizing::new(crate::codec::unb64(&self.job.key)?);
            let key: &[u8; 32] = key
                .as_slice()
                .try_into()
                .map_err(|_| Error::IntegrityFailed)?;
            self.job.key_wrap = Some(link::wrap_key(key, password, &self.job.plan.share_id)?);
            self.job.plan.password = true;
        }
        Ok(self)
    }
}

/// Approved work to run without holding the client during network IO.
pub struct ShareRun {
    id: String,
    request: String,
}

impl Client {
    fn share_job(&self, id: &str) -> Result<Job> {
        ensure_valid(hex_id(id, 64), "share job")?;
        self.workspace
            .private(self.keys()?, &format!("{PREFIX}{id}"))?
            .ok_or(Error::NotFound)
    }

    fn save_share_job(&self, job: &Job) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{PREFIX}{}", job.id), job)
    }

    fn snapshot(&self, plan: &Plan) -> Result<package::Manifest> {
        let mut entry = self.vault_show(&plan.input.item_id)?;
        let result = (|| {
            ensure(
                !entry.deleted && !entry.item.reserved() && entry.revision == plan.input.revision,
                failure("SECRET_VERSION_STALE"),
            )?;
            ensure(
                !self
                    .workspace
                    .store
                    .conflicts(crate::content::VAULT)?
                    .iter()
                    .any(|c| c.item == entry.id),
                Error::VersionConflict,
            )?;
            let fields = plan
                .input
                .field_ids
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    let mut field = entry
                        .item
                        .fields
                        .iter()
                        .find(|f| &f.id == id)
                        .cloned()
                        .ok_or(Error::NotFound)?;
                    let role = crate::vault::template(&entry.item.kind)
                        .iter()
                        .find(|(id, kind)| field.id == *id && field.kind == *kind)
                        .map(|(id, _)| *id);
                    field.id = role.map_or_else(|| format!("f{i}"), |role| format!("f{i}_{role}"));
                    Ok(field)
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(package::Manifest {
                format: package::FORMAT.into(),
                snapshot_id: plan.snapshot_id.clone(),
                created_at: plan.created_at,
                item: package::Snapshot {
                    kind: entry.item.kind.clone(),
                    title: entry.item.title.clone(),
                    fields,
                    notes: plan.input.include_notes.then(|| entry.item.notes.clone()),
                },
                attachments: plan
                    .attachments
                    .iter()
                    .map(|a| package::Attachment {
                        name: a.name.clone(),
                        mime: a.mime.clone(),
                        size: a.size,
                        sha256: a.sha256.clone(),
                    })
                    .collect(),
            })
        })();
        for f in &mut entry.item.fields {
            f.value.zeroize();
        }
        entry.item.notes.zeroize();
        result
    }

    /// Validate a local snapshot and allocate stable identities; no network or ciphertext upload.
    pub fn secret_share_prepare(&mut self, input: ShareInput) -> Result<Preparation> {
        self.keys()?;
        let now = now_ms();
        ensure_valid(
            hex_id(&input.item_id, 64)
                && hex_id(&input.revision, 64)
                && input.field_ids.len() <= 64
                && input.attachments.len() <= 20
                && (!input.field_ids.is_empty()
                    || input.include_notes
                    || !input.attachments.is_empty()),
            "snapshot selection",
        )?;
        ensure_valid(
            input.field_ids.iter().collect::<BTreeSet<_>>().len() == input.field_ids.len(),
            "duplicate field",
        )?;
        let expires = input.expires_in_ms.unwrap_or(DAY);
        let count = input.max_download_sessions.unwrap_or(1);
        ensure_valid(
            (6 * MINUTE..=7 * DAY).contains(&expires) && (1..=100).contains(&count),
            "share lifetime or sessions",
        )?;
        ensure(
            self.workspace.store.sealed_keys(PREFIX)?.len() < 256,
            Error::QuotaExceeded,
        )?;
        let mut entry = self.vault_show(&input.item_id)?;
        let title = entry.item.title.clone();
        let item_type = entry.item.kind.clone();
        let files = entry.item.files.clone();
        let metadata = input
            .field_ids
            .iter()
            .map(|id| {
                let f = entry
                    .item
                    .fields
                    .iter()
                    .find(|f| &f.id == id)
                    .ok_or(Error::NotFound)?;
                Ok(json!({"id":f.id,"type":f.kind,"label":f.label,"section":f.section}))
            })
            .collect::<Result<Vec<_>>>();
        for f in &mut entry.item.fields {
            f.value.zeroize();
        }
        entry.item.notes.zeroize();
        let metadata = metadata?;
        let mut selected = BTreeSet::new();
        let mut attachments = Vec::new();
        for a in &input.attachments {
            ensure(
                files.contains(&a.file_id) && selected.insert((&a.file_id, &a.version_id)),
                Error::Forbidden,
            )?;
            let (source, size) = self.secret_source(
                &format!("dmsg-file://{}/{}", a.file_id, a.version_id),
                crate::files::MAX_FILE,
            )?;
            let (_, _, mut file) = self.library_manifest(&a.file_id, Some(&a.version_id))?;
            file.key.zeroize();
            attachments.push(AttachmentPlan {
                source,
                name: file.name,
                mime: file.mime,
                size,
                sha256: file.sha256,
            });
        }
        let mut plan = Plan {
            input,
            account_id: self.account()?.id.to_string(),
            device_id: self.secret_device(),
            title,
            item_type,
            fields: metadata,
            attachments,
            snapshot_id: hex::encode(random_id()),
            share_id: hex::encode(random_id()),
            upload_id: hex::encode(random_id()),
            file_version: hex::encode(random_id()),
            created_at: now,
            expires_at: now + expires,
            publish_before: (now + DAY).min(now + expires - 1),
            max_download_sessions: count,
            password: false,
            plain_bytes: 0,
            estimated_ciphertext_bytes: 0,
        };
        let header = self.snapshot(&plan)?.header()?;
        plan.plain_bytes = plan
            .attachments
            .iter()
            .try_fold(header.len() as u64, |n, a| {
                n.checked_add(a.size).ok_or(Error::QuotaExceeded)
            })?;
        ensure(
            plan.plain_bytes <= crate::files::MAX_FILE,
            Error::QuotaExceeded,
        )?;
        plan.estimated_ciphertext_bytes =
            plan.plain_bytes + plan.plain_bytes.div_ceil(CHUNK as u64) * 16 + 65536;
        Ok(Preparation {
            job: Job {
                id: hex::encode(random_id()),
                plan,
                key: crate::codec::b64(&crate::crypto::random_key()),
                key_wrap: None,
                phase: "prepared".into(),
                request: None,
                boot: self.secret_boot.clone(),
                session: self.session_generation,
                upload: None,
                manifest: None,
                publish_attempted: false,
                uploaded_chunks: 0,
                cancel_requested: false,
                error: None,
            },
        })
    }

    /// Seal the prepared key/wrap and metadata after the worker returns to the same session.
    pub fn secret_share_save_preparation(&mut self, prepared: Preparation) -> Result<Value> {
        self.share_check(&prepared.job, true, now_ms())?;
        self.save_share_job(&prepared.job)?;
        self.share_job_view(&prepared.job, false)
    }

    fn share_check(&self, job: &Job, session: bool, now: u64) -> Result<()> {
        self.keys()?;
        ensure(
            job.plan.account_id == self.account()?.id.to_string()
                && job.plan.device_id == self.secret_device(),
            Error::Forbidden,
        )?;
        if let Some(current) = self
            .workspace
            .private::<Job>(self.keys()?, &format!("{PREFIX}{}", job.id))?
        {
            ensure(
                !current.cancel_requested && current.plan.digest() == job.plan.digest(),
                Error::Expired,
            )?;
            if matches!(job.phase.as_str(), "encrypting" | "uploading") {
                ensure(
                    current.request == job.request
                        && matches!(current.phase.as_str(), "encrypting" | "uploading"),
                    Error::Expired,
                )?;
                let request =
                    self.approval_record(job.request.as_deref().ok_or(Error::IntegrityFailed)?)?;
                ensure(
                    request.state == State::Executing && request.started,
                    Error::Expired,
                )?;
            }
        }
        if session {
            ensure(
                job.boot == self.secret_boot && job.session == self.session_generation,
                failure("SECRET_SESSION_ENDED"),
            )?;
        }
        ensure(
            !job.cancel_requested && now < job.plan.publish_before && now < job.plan.expires_at,
            Error::Expired,
        )?;
        let _ = self.snapshot(&job.plan)?;
        for attachment in &job.plan.attachments {
            self.secret_source_check(&attachment.source)?;
        }
        Ok(())
    }

    fn share_job_view(&self, job: &Job, include_link: bool) -> Result<Value> {
        let mut value = json!({"id":job.id,"state":job.phase,"plan":job.plan,"request_id":job.request,"error":job.error,"cancel_requested":job.cancel_requested,"device_local":true});
        let total = job.upload.as_ref().map_or(0, |p| p.chunks.len());
        value["uploaded_chunks"] =
            json!(if matches!(job.phase.as_str(), "published" | "revoked") {
                total
            } else {
                job.uploaded_chunks
            });
        value["total_chunks"] = json!(total);
        if include_link && job.phase == "published" && !job.cancel_requested {
            let record = self
                .link_records()?
                .into_iter()
                .find(|r| r.share_id == job.plan.share_id)
                .ok_or(Error::RecoveryIncomplete)?;
            let link = self.link_view(&record, now_ms());
            if link.state == "active" {
                value["link"] = json!(link);
            }
        }
        Ok(value)
    }

    /// Inspect metadata; a link is returned only after confirmed publication and explicit request.
    pub fn secret_share_get(&self, id: &str, include_link: bool) -> Result<Value> {
        self.share_job_view(&self.share_job(id)?, include_link)
    }

    /// Device-local snapshot jobs, without link keys.
    pub fn secret_shares(&self) -> Result<Value> {
        let mut jobs = Vec::new();
        for key in self.workspace.store.sealed_keys(PREFIX)? {
            if let Some(job) = self.workspace.private::<Job>(self.keys()?, &key)? {
                let mut view = self.share_job_view(&job, false)?;
                view["plan"] = json!({"title":job.plan.title,"share_id":job.plan.share_id,"expires_at":job.plan.expires_at,"plain_bytes":job.plan.plain_bytes});
                jobs.push(view);
            }
        }
        Ok(json!({"shares":jobs,"device_local":true}))
    }

    /// A fresh approval is required to resume an unpublished interrupted upload.
    pub fn secret_share_request(&mut self, source: &str, id: &str) -> Result<Value> {
        ensure(matches!(source, "gui" | "cli"), Error::Forbidden)?;
        let mut job = self.share_job(id)?;
        ensure(
            !job.publish_attempted
                && !matches!(job.phase.as_str(), "published" | "publishing" | "unknown"),
            Error::ExecutionUnknown,
        )?;
        if let Some(request) = &job.request {
            let r = self.approval_record(request)?;
            if matches!(r.state, State::AwaitingUser | State::Executing) {
                return self.approval_view(&r);
            }
        }
        let now = now_ms();
        self.share_check(&job, false, now)?;
        job.boot = self.secret_boot.clone();
        job.session = self.session_generation;
        let request = self.approval_create_at(
            source,
            Action::SecretShare {
                id: id.into(),
                digest: job.plan.digest(),
            },
            now,
            now + crate::approval::HIGH_RISK_TTL_MS,
        )?;
        job.phase = "awaiting_user".into();
        job.request = Some(request.id.clone());
        job.error = None;
        self.save_share_job(&job)?;
        self.approval_view(&request)
    }

    pub(crate) fn secret_share_detail(&self, id: &str, digest: &str) -> Result<Value> {
        let job = self.share_job(id)?;
        ensure(job.plan.digest() == digest, Error::IntegrityFailed)?;
        Ok(json!(job.plan))
    }

    pub(crate) fn secret_share_run(&mut self, r: &mut Request, now: u64) -> Result<ShareRun> {
        let Action::SecretShare { id, digest } = &r.action else {
            return Err(Error::Forbidden);
        };
        let mut job = self.share_job(id)?;
        ensure(
            job.plan.digest() == *digest
                && job.request.as_deref() == Some(&r.id)
                && !job.publish_attempted,
            Error::Forbidden,
        )?;
        self.share_check(&job, true, now)?;
        job.phase = if job.upload.is_some() {
            "uploading"
        } else {
            "encrypting"
        }
        .into();
        r.started = true;
        self.workspace.store.atomic(|_| {
            self.write_approval(r)?;
            self.save_share_job(&job)
        })?;
        self.running.insert(r.id.clone(), now);
        Ok(ShareRun {
            id: id.clone(),
            request: r.id.clone(),
        })
    }

    fn share_settle(
        &mut self,
        id: &str,
        phase: &str,
        error: Option<String>,
        state: State,
    ) -> Result<Value> {
        let mut job = self.share_job(id)?;
        let (phase, state) = if job.cancel_requested && !job.publish_attempted {
            ("cancelled", State::Cancelled)
        } else {
            (phase, state)
        };
        let (phase, state) = if job.phase == "revoked" {
            ("revoked", State::Done)
        } else if job.cancel_requested && job.publish_attempted && phase != "revoked" {
            ("revoke_pending", State::Lost)
        } else {
            (phase, state)
        };
        job.phase = phase.into();
        job.error = error.clone();
        if let Some(request) = &job.request {
            let mut r = self.approval_record(request)?;
            r.state = state;
            r.error = error;
            r.updated_at = now_ms();
            r.delivery_report = Some(json!({"share_id":job.plan.share_id,"state":phase}));
            self.workspace.store.atomic(|_| {
                self.save_share_job(&job)?;
                self.write_approval(&r)
            })?;
            self.running.remove(request);
        } else {
            self.save_share_job(&job)?;
        }
        self.share_job_view(&job, false)
    }

    /// Stop unpublished work. After publication is attempted, cancellation remains pending until revoke is confirmed.
    pub fn secret_share_cancel(&mut self, id: &str) -> Result<Value> {
        let mut job = self.share_job(id)?;
        job.cancel_requested = true;
        self.save_share_job(&job)?;
        if !job.publish_attempted {
            self.release_chunks(&job.plan.share_id)?;
            return self.share_settle(id, "cancelled", None, State::Cancelled);
        }
        self.share_job_view(&job, false)
    }

    /// Mark interruption for explicit continuation; never let generic approval reconciliation publish.
    pub(crate) fn secret_share_interrupted(&mut self, r: &mut Request) -> Result<()> {
        let Action::SecretShare { id, .. } = &r.action else {
            return Err(Error::Forbidden);
        };
        let job = self.share_job(id)?;
        let phase = if job.publish_attempted {
            "unknown"
        } else {
            "needs_approval"
        };
        let state = if job.publish_attempted {
            State::Lost
        } else {
            State::Failed
        };
        self.share_settle(
            id,
            phase,
            Some(
                if job.publish_attempted {
                    "EXECUTION_UNKNOWN"
                } else {
                    "SECRET_SESSION_ENDED"
                }
                .into(),
            ),
            state,
        )?;
        Ok(())
    }
}

fn share_record(job: &Job) -> LinkRecord {
    LinkRecord {
        source: LinkSource::VaultSnapshot {
            item_id: job.plan.input.item_id.clone(),
            item_revision: job.plan.input.revision.clone(),
            snapshot_id: job.plan.snapshot_id.clone(),
        },
        name: job.plan.title.clone(),
        size: job.plan.plain_bytes,
        share_id: job.plan.share_id.clone(),
        upload_id: job.plan.upload_id.clone(),
        key: job.key.clone(),
        expires_at: job.plan.expires_at,
        max_downloads: job.plan.max_download_sessions,
        password: job.plan.password,
        created_at: job.plan.created_at,
        last_checked_at: None,
        revoked_at: None,
        downloads: 0,
    }
}

async fn append<H: Hold>(
    h: &H,
    job: &Job,
    staged: &mut staging::StagedFile,
    pending: &mut Zeroizing<Vec<u8>>,
    bytes: &[u8],
) -> Result<()> {
    pending.extend_from_slice(bytes);
    while pending.len() >= CHUNK {
        h.with(|c| {
            c.share_check(job, true, now_ms())?;
            c.secret_stage_part(staged, &pending[..CHUNK])
        })
        .await?;
        let remainder = Zeroizing::new(pending[CHUNK..].to_vec());
        pending.zeroize();
        pending.extend_from_slice(&remainder);
    }
    Ok(())
}

impl ShareRun {
    /// Encrypt a snapshot in bounded chunks, upload immutable ciphertext, then publish at most once.
    pub async fn run<H: Hold>(self, h: &H) -> Result<Value> {
        let bound = Unlocked::new(h).await?;
        let outcome = self.execute(&bound).await;
        match outcome {
            Ok(()) => {
                let cancel = bound
                    .with(|c| Ok(c.share_job(&self.id)?.cancel_requested))
                    .await?;
                if cancel {
                    revoke(&bound, &self.id).await
                } else {
                    bound
                        .with(|c| c.share_settle(&self.id, "published", None, State::Done))
                        .await
                }
            }
            Err(error) => {
                bound
                    .with(|c| {
                        let job = c.share_job(&self.id)?;
                        if job.publish_attempted {
                            c.share_settle(
                                &self.id,
                                "unknown",
                                Some("EXECUTION_UNKNOWN".into()),
                                State::Lost,
                            )
                        } else {
                            c.share_settle(
                                &self.id,
                                "needs_approval",
                                Some(crate::error_code(&error)),
                                State::Failed,
                            )
                        }
                    })
                    .await
            }
        }
    }

    async fn execute<H: Hold>(&self, h: &H) -> Result<()> {
        let mut job = h
            .with(|c| {
                let job = c.share_job(&self.id)?;
                c.share_check(&job, true, now_ms())?;
                ensure(
                    job.request.as_deref() == Some(&self.request) && !job.publish_attempted,
                    Error::Forbidden,
                )?;
                Ok(job)
            })
            .await?;
        if job.upload.is_none() {
            let (header, mut staged) = h
                .with(|c| {
                    c.share_check(&job, true, now_ms())?;
                    let header = c.snapshot(&job.plan)?.header()?;
                    let mut staged = c.secret_stage_start(
                        "secret.dmsg-package",
                        package::MIME,
                        job.plan.plain_bytes,
                    )?;
                    staged.manifest.id = job.plan.share_id.clone();
                    staged.manifest.version = job.plan.file_version.clone();
                    staged.manifest.key.zeroize();
                    staged.manifest.key = job.key.clone();
                    Ok((header, staged))
                })
                .await?;
            let mut pending = Zeroizing::new(Vec::with_capacity(2 * CHUNK));
            append(h, &job, &mut staged, &mut pending, &header).await?;
            for attachment in &job.plan.attachments {
                let process::Source::Library {
                    file_id,
                    version_id,
                    ..
                } = &attachment.source
                else {
                    return Err(Error::Forbidden);
                };
                let mut digest = Sha256::new();
                for index in 0..crate::files::chunk_count(attachment.size) {
                    let bytes = h
                        .with(|c| {
                            c.share_check(&job, true, now_ms())?;
                            let (_, _, mut source) =
                                c.library_manifest(file_id, Some(version_id))?;
                            let bytes = crate::files::read_chunk(c.blobs()?, &source, index);
                            source.key.zeroize();
                            bytes
                        })
                        .await?;
                    digest.update(&*bytes);
                    append(h, &job, &mut staged, &mut pending, &bytes).await?;
                }
                ensure(
                    hex::encode(digest.finalize()) == attachment.sha256,
                    Error::IntegrityFailed,
                )?;
            }
            h.with(|c| {
                c.share_check(&job, true, now_ms())?;
                if !pending.is_empty() {
                    c.secret_stage_part(&mut staged, &pending)?;
                }
                c.secret_stage_finish(&mut staged, None)
            })
            .await?;
            h.with(|c|{
                c.share_check(&job,true,now_ms())?;
                let edition=staged.manifest.clone();let prepared=c.share_edition_upload(edition.public(),edition,job.key_wrap.clone(),Some(job.plan.upload_id.clone()))?;
                let mut plan=prepared.plan;plan.expires_at=job.plan.publish_before;
                plan.management=Some(c.seal_management(&plan,&json!({"name":job.plan.title,"plain_bytes":job.plan.plain_bytes,"source_type":"vault_snapshot","intent":{"kind":"secret_link","record":share_record(&job),"key_wrap":job.key_wrap}}))?);
                job.upload=Some(plan);job.manifest=Some(crate::codec::b64(&prepared.manifest_bytes));job.phase="uploading".into();
                c.workspace.store.atomic(|_|{c.save_share_job(&job)?;c.hold_chunks([(job.plan.share_id.clone(),job.plan.publish_before,job.upload.as_ref().ok_or(Error::IntegrityFailed)?.chunks.iter().map(|c|c.digest.clone()).collect())])})
            }).await?;
        }
        let plan = job.upload.as_ref().ok_or(Error::IntegrityFailed)?;
        let manifest = crate::codec::unb64(job.manifest.as_ref().ok_or(Error::IntegrityFailed)?)?;
        let base = h.with(|c| c.base()).await?;
        crate::upload::upload(
            h,
            &base,
            plan,
            &manifest,
            0,
            BlobSource::Library,
            |c, count| {
                c.share_check(&job, true, now_ms())?;
                let mut current = c.share_job(&job.id)?;
                current.uploaded_chunks = count;
                c.save_share_job(&current)
            },
        )
        .await?;
        let payload = json!({"share_id":job.plan.share_id,"upload_id":job.plan.upload_id,"expires_at":job.plan.expires_at,"max_downloads":job.plan.max_download_sessions,"key_wrap":job.key_wrap});
        let request_id = Hash::new(random_id());
        let (relay, prepared) = h
            .signed(|c| {
                c.signed_post(
                    &format!("{base}/shares"),
                    "dmsg/share/create/v1",
                    &payload,
                    &request_id,
                )
            })
            .await?;
        h.with(|c| {
            let now = now_ms();
            c.share_check(&job, true, now)?;
            let mut current = c.share_job(&job.id)?;
            ensure(
                !current.publish_attempted && current.request.as_deref() == Some(&self.request),
                Error::Forbidden,
            )?;
            current.publish_attempted = true;
            current.phase = "publishing".into();
            c.save_share_job(&current)
        })
        .await?;
        // From this point any failure is uncertain. There is no publication retry path.
        let result = relay.send(prepared).await?;
        ensure(
            result["share_id"] == job.plan.share_id,
            Error::IntegrityFailed,
        )?;
        h.with(|c| {
            let current = c.share_job(&job.id)?;
            if !c
                .link_records()?
                .iter()
                .any(|r| r.share_id == current.plan.share_id)
            {
                c.save_link(&share_record(&current))?;
            }
            c.release_chunks(&current.plan.share_id)
        })
        .await
    }
}

/// Query the original share identity after an uncertain publish; never sends another create.
pub async fn reconcile<H: Hold>(h: &H, id: &str) -> Result<Value> {
    let bound = Unlocked::new(h).await?;
    let job = bound.with(|c| c.share_job(id)).await?;
    if job.cancel_requested {
        return revoke(&bound, id).await;
    }
    ensure(job.publish_attempted, Error::Forbidden)?;
    let links = link::share_links_remote(&bound).await?;
    if let Some(remote) = links.iter().find(|l| l.share_id == job.plan.share_id) {
        bound
            .with(|c| {
                let current = c.share_job(id)?;
                let mut record = share_record(&current);
                record.revoked_at = remote.revoked_at;
                record.downloads = remote.downloads;
                record.last_checked_at = remote.last_checked_at;
                c.save_link(&record)?;
                c.release_chunks(&current.plan.share_id)?;
                c.share_settle(
                    id,
                    if remote.revoked_at.is_some() {
                        "revoked"
                    } else {
                        "published"
                    },
                    None,
                    State::Done,
                )
            })
            .await
    } else {
        bound
            .with(|c| c.share_job_view(&c.share_job(id)?, false))
            .await
    }
}

/// Revoke an existing original share. Unknown publication is queried before any revoke attempt.
pub async fn revoke<H: Hold>(h: &H, id: &str) -> Result<Value> {
    let bound = Unlocked::new(h).await?;
    bound.with(|c| c.secret_share_cancel(id)).await?;
    let job = bound.with(|c| c.share_job(id)).await?;
    if !job.publish_attempted {
        return bound.with(|c| c.share_job_view(&job, false)).await;
    }
    let links = link::share_links_remote(&bound).await?;
    if links.iter().any(|l| l.share_id == job.plan.share_id) {
        link::share_revoke(&bound, &job.plan.share_id).await?;
        bound
            .with(|c| c.share_settle(id, "revoked", None, State::Done))
            .await
    } else {
        bound.with(|c| c.share_job_view(&job, false)).await
    }
}

#[cfg(test)]
#[path = "share_tests.rs"]
mod tests;
