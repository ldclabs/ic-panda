//! Receive one secret package using the existing public share-session contract.
use super::*;
use crate::{
    cloud::Cloud,
    downloads::Slot,
    files::{BlobStore, CHUNK, FileManifest},
    hold::{Hold, Session, Unlocked},
    library::link,
    upload::ChunkRef,
};
use dmsg_types::MINUTE;
use std::collections::BTreeSet;
struct SensitiveManifest(FileManifest);
impl Drop for SensitiveManifest {
    fn drop(&mut self) {
        self.0.key.zeroize();
    }
}
impl std::ops::Deref for SensitiveManifest {
    type Target = FileManifest;
    fn deref(&self) -> &FileManifest {
        &self.0
    }
}
impl std::ops::DerefMut for SensitiveManifest {
    fn deref_mut(&mut self) -> &mut FileManifest {
        &mut self.0
    }
}
const PREFIX: &str = "secret-package-import:";

#[derive(Clone, Serialize, Deserialize)]
struct Descriptor {
    status: String,
    share_id: String,
    upload_id: String,
    object_id: String,
    version_id: String,
    manifest: ChunkRef,
    chunks: Vec<ChunkRef>,
    expires_at: u64,
    password: bool,
    key_wrap: Option<String>,
}
#[derive(Deserialize)]
struct Grant {
    upload_id: String,
    expires_at: u64,
    session_expires_at: u64,
    manifest: Slot,
    chunks: Vec<Slot>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Received {
    id: String,
    link_digest: String,
    session_id: String,
    started: bool,
    descriptor: Option<Descriptor>,
    channel_source: Option<Value>,
    key: String,
    file: Option<FileManifest>,
    digest: Option<String>,
    preview: Option<Value>,
    created_at: u64,
    expires_at: u64,
    imported: Option<Vec<String>>,
    request: Option<String>,
    selection: Option<Selection>,
    request_boot: Option<String>,
    request_session: Option<u64>,
    /// Receiver's allocation limit, retained through approval and resumed import.
    #[serde(default = "default_package_limit")]
    max_plain_bytes: u64,
}

fn default_package_limit() -> u64 {
    crate::files::MAX_FILE
}

fn check_package_size(size: u64, limit: u64) -> Result<()> {
    ensure_valid(
        limit > 0 && limit <= crate::files::MAX_FILE,
        "package limit",
    )?;
    ensure(size <= limit, Error::QuotaExceeded)
}
impl Drop for Received {
    fn drop(&mut self) {
        self.key.zeroize();
        if let Some(file) = &mut self.file {
            file.key.zeroize();
        }
    }
}

/// Exact user selection after the full package passed integrity validation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    /// Device-local received-package identifier.
    pub package_id: String,
    /// Complete verified package SHA-256.
    pub digest: String,
    /// Explicit field selection or value-free field metadata.
    pub fields: Vec<String>,
    /// Whether the owner explicitly selected notes.
    pub include_notes: bool,
    /// Explicit selected attachments; never an implicit all-files choice.
    pub attachments: Vec<usize>,
    /// Destination library folder for new local attachments.
    pub folder: String,
}

enum ImportPrep {
    Done(Value),
    Run(Box<Received>),
}

impl Client {
    fn received(&self, id: &str) -> Result<Received> {
        ensure_valid(hex_id(id, 64), "package id")?;
        self.workspace
            .private(self.keys()?, &format!("{PREFIX}{id}"))?
            .ok_or(Error::NotFound)
    }

    fn save_received(&self, received: &Received) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{PREFIX}{}", received.id), received)
    }

    fn secret_package_prepare_import(
        &mut self,
        selection: &Selection,
        max_plain_bytes: u64,
    ) -> Result<ImportPrep> {
        let mut received = self.received(&selection.package_id)?;
        let limit = received.max_plain_bytes.min(max_plain_bytes);
        check_package_size(
            received
                .file
                .as_ref()
                .ok_or(Error::RecoveryIncomplete)?
                .size,
            limit,
        )?;
        ensure(
            received.digest.as_ref() == Some(&selection.digest),
            Error::VersionConflict,
        )?;
        if let Some(ids) = &received.imported {
            ensure(
                received.selection.as_ref() == Some(selection),
                Error::IdempotencyConflict,
            )?;
            return Ok(ImportPrep::Done(
                json!({"state":"done","ids":ids,"files_local_only":true,"files_inherit_folder_grants":true}),
            ));
        }
        let now = now_ms();
        ensure(now < received.expires_at, Error::Expired)?;
        self.catalog()?.live_folder(&selection.folder)?;
        let preview = received.preview.as_ref().ok_or(Error::RecoveryIncomplete)?;
        let fields = preview["fields"].as_array().ok_or(Error::IntegrityFailed)?;
        let attachments = preview["attachments"]
            .as_array()
            .ok_or(Error::IntegrityFailed)?;
        ensure_valid(
            selection.fields.len() <= 64
                && selection.fields.iter().collect::<BTreeSet<_>>().len() == selection.fields.len()
                && selection
                    .fields
                    .iter()
                    .all(|id| fields.iter().any(|f| f["id"] == *id))
                && selection.attachments.iter().collect::<BTreeSet<_>>().len()
                    == selection.attachments.len()
                && selection.attachments.iter().all(|i| *i < attachments.len())
                && (!selection.include_notes || preview["notes"] == true)
                && (!selection.fields.is_empty()
                    || selection.include_notes
                    || !selection.attachments.is_empty()),
            "package selection",
        )?;
        let mut changed = false;
        if received.max_plain_bytes != limit {
            received.max_plain_bytes = limit;
            changed = true;
        }
        if let Some(old_request) = received.request.take() {
            if let Ok(mut r) = self.approval_record(&old_request)
                && r.state == State::AwaitingUser
            {
                r.state = State::Cancelled;
                r.updated_at = now;
                self.save_approval(&r)?;
            }
            changed = true;
        }
        if changed {
            self.save_received(&received)?;
        }
        Ok(ImportPrep::Run(Box::new(received)))
    }

    pub(crate) fn secret_package_detail(&self, selection: &Selection) -> Result<Value> {
        let r = self.received(&selection.package_id)?;
        ensure(
            r.digest.as_ref() == Some(&selection.digest),
            Error::VersionConflict,
        )?;
        Ok(
            json!({"selection":selection,"preview":r.preview,"channel_source":r.channel_source,"files_local_only":true,"files_inherit_folder_grants":true}),
        )
    }
}

fn validate_descriptor(d: &Descriptor, id: &str) -> Result<()> {
    ensure(d.status == "available", Error::Expired)?;
    ensure(
        d.share_id == id
            && hex_id(&d.upload_id, 64)
            && hex_id(&d.object_id, 64)
            && hex_id(&d.version_id, 64)
            && !d.chunks.is_empty()
            && d.chunks.len() <= 100
            && d.manifest.size <= 1024 * 1024 + 64
            && hex_id(&d.manifest.digest, 64)
            && d.chunks
                .iter()
                .all(|c| c.size <= 1024 * 1024 + 64 && hex_id(&c.digest, 64)),
        Error::IntegrityFailed,
    )
}

async fn authorize<H: Hold>(h: &H, cloud: &Cloud, received: &mut Received) -> Result<Grant> {
    let descriptor = received.descriptor.as_ref().ok_or(Error::Forbidden)?;
    let value = cloud
        .public_share_session(&descriptor.share_id, &received.session_id, received.started)
        .await?;
    let grant: Grant = serde_json::from_value(value).map_err(|_| Error::IntegrityFailed)?;
    let now = crate::now_ms();
    ensure(
        grant.upload_id == descriptor.upload_id
            && now < grant.expires_at
            && grant.expires_at <= grant.session_expires_at
            && grant.session_expires_at <= descriptor.expires_at
            && grant.chunks.len() == descriptor.chunks.len(),
        Error::IntegrityFailed,
    )?;
    for (slot, expected) in std::iter::once(&grant.manifest)
        .chain(&grant.chunks)
        .zip(std::iter::once(&descriptor.manifest).chain(&descriptor.chunks))
    {
        ensure(
            slot.size == expected.size && slot.digest == expected.digest,
            Error::IntegrityFailed,
        )?;
    }
    received.started = true;
    h.with(|c| c.save_received(received)).await?;
    Ok(grant)
}

async fn ciphertext<H: Hold>(
    h: &H,
    cloud: &Cloud,
    received: &mut Received,
    grant: &mut Grant,
    index: Option<usize>,
) -> Result<Vec<u8>> {
    for attempt in 0..2 {
        if grant.expires_at <= crate::now_ms() + 1000 {
            *grant = authorize(h, cloud, received).await?;
        }
        let slot = match index {
            Some(i) => grant.chunks.get(i),
            None => Some(&grant.manifest),
        }
        .ok_or(Error::IntegrityFailed)?;
        match cloud.download_slot(slot, &slot.digest).await {
            Ok(bytes) => {
                h.with(|c| {
                    c.keys()?;
                    Ok(())
                })
                .await?;
                return Ok(bytes);
            }
            Err(Error::Expired | Error::NotFound | Error::Unavailable(_)) if attempt == 0 => {
                *grant = authorize(h, cloud, received).await?;
            }
            Err(Error::NotFound) => return Err(Error::IntegrityFailed),
            Err(error) => return Err(error),
        }
    }
    Err(Error::IntegrityFailed)
}

/// Download is explicit. Retries of the same link reuse the sealed random session ID.
pub async fn preview<H: Hold>(h: &H, url: &str, password: Option<&str>) -> Result<Value> {
    preview_limited(h, url, password, crate::files::MAX_FILE).await
}

/// Receive with a host-specific allocation limit. This limits the whole-package
/// plaintext buffer, not total RSS: chunk buffers, manifest parsing and the
/// separate password-derivation phase require additional memory.
pub async fn preview_limited<H: Hold>(
    h: &H,
    url: &str,
    password: Option<&str>,
    max_plain_bytes: u64,
) -> Result<Value> {
    check_package_size(0, max_plain_bytes)?;
    let bound = Unlocked::new(h).await?;
    let (cloud, site) = bound
        .with(|c| Ok((c.relay(), c.config.login_url.clone())))
        .await?;
    let link_url = url::Url::parse(url).map_err(|_| Error::InvalidInput("share link".into()))?;
    let site = url::Url::parse(&site).map_err(|_| Error::IntegrityFailed)?;
    ensure(
        link_url.origin() == site.origin()
            && link_url.username().is_empty()
            && link_url.password().is_none()
            && link_url.query().is_none(),
        Error::Forbidden,
    )?;
    let id = link_url
        .path()
        .strip_prefix("/s/")
        .filter(|id| hex_id(id, 64))
        .ok_or(Error::InvalidInput("share link".into()))?;
    let digest = hex::encode(Sha256::digest(url.as_bytes()));
    let found = bound
        .with(|c| {
            for key in c.workspace.store.sealed_keys(PREFIX)? {
                if let Some(record) = c.workspace.private::<Received>(c.keys()?, &key)?
                    && record.link_digest == digest
                {
                    return Ok(Some(record));
                }
            }
            Ok(None)
        })
        .await?;
    if let Some(record) = &found
        && record.preview.is_some()
        && now_ms() < record.expires_at
    {
        check_package_size(
            record.file.as_ref().ok_or(Error::RecoveryIncomplete)?.size,
            max_plain_bytes,
        )?;
        return Ok(
            json!({"package_id":record.id,"digest":record.digest,"preview":record.preview,"already_imported":record.imported,"files_local_only":true,"files_inherit_folder_grants":true}),
        );
    }
    let descriptor: Descriptor = serde_json::from_value(
        cloud
            .public(&format!("/public/shares/{id}"))
            .await
            .map_err(|_| Error::Unavailable("share descriptor unavailable".into()))?
            .ok_or(Error::NotFound)?,
    )
    .map_err(|_| Error::IntegrityFailed)?;
    validate_descriptor(&descriptor, id)?;
    // Each chunk has at most 64 bytes of framing. Reject definitely oversized
    // packages before consuming a counted download session or deriving a key.
    let minimum_plain_bytes = descriptor.chunks.iter().try_fold(0u64, |total, chunk| {
        total
            .checked_add(chunk.size.saturating_sub(64))
            .ok_or(Error::QuotaExceeded)
    })?;
    check_package_size(minimum_plain_bytes, max_plain_bytes)?;
    let key = if descriptor.password {
        ensure(link_url.fragment().is_none(), Error::Forbidden)?;
        link::unwrap_key(
            descriptor
                .key_wrap
                .as_deref()
                .ok_or(Error::IntegrityFailed)?,
            password.ok_or(Error::AuthRequired)?,
            id,
        )?
    } else {
        let bytes = Zeroizing::new(crate::codec::unb64(
            link_url.fragment().ok_or(Error::AuthRequired)?,
        )?);
        Zeroizing::new(
            bytes
                .as_slice()
                .try_into()
                .map_err(|_| Error::IntegrityFailed)?,
        )
    };
    let now = now_ms();
    let mut record = match found {
        Some(record) => {
            ensure(
                record
                    .descriptor
                    .as_ref()
                    .is_some_and(|d| d.upload_id == descriptor.upload_id)
                    && record.key == crate::codec::b64(&key[..]),
                Error::VersionConflict,
            )?;
            record
        }
        None => {
            bound
                .with(|c| {
                    ensure(
                        c.workspace.store.sealed_keys(PREFIX)?.len() < 128,
                        Error::QuotaExceeded,
                    )
                })
                .await?;
            Received {
                id: hex::encode(random_id()),
                link_digest: digest,
                session_id: hex::encode(random_id()),
                started: false,
                descriptor: Some(descriptor),
                channel_source: None,
                key: crate::codec::b64(&key[..]),
                file: None,
                digest: None,
                preview: None,
                created_at: now,
                expires_at: now + 30 * MINUTE,
                imported: None,
                request: None,
                selection: None,
                request_boot: None,
                request_session: None,
                max_plain_bytes,
            }
        }
    };
    record.expires_at = now + 30 * MINUTE;
    record.max_plain_bytes = record.max_plain_bytes.min(max_plain_bytes);
    // Persist before the counted POST; a lost response retries this exact session identity.
    bound.with(|c| c.save_received(&record)).await?;
    let mut grant = authorize(&bound, &cloud, &mut record).await?;
    let bytes = ciphertext(&bound, &cloud, &mut record, &mut grant, None).await?;
    let d = record.descriptor.as_ref().ok_or(Error::Forbidden)?;
    let plain = crate::crypto::open_bytes(
        &key,
        &bytes,
        &link::share_manifest_aad(&d.share_id, &d.upload_id, &d.object_id, &d.version_id),
    )?;
    let mut manifest: link::ShareFile = crate::codec::decode(&plain, 1024 * 1024)?;
    let file = &mut manifest.file;
    ensure(
        manifest.format == link::SHARE_FORMAT
            && manifest.share_id == d.share_id
            && file.id == d.object_id
            && file.version == d.version_id
            && file.mime == package::MIME
            && file.size <= crate::files::MAX_FILE
            && file.chunks.len() as u64 == crate::files::chunk_count(file.size)
            && file.chunks.len() == d.chunks.len()
            && file.key.is_empty(),
        Error::IntegrityFailed,
    )?;
    for (i, chunk) in file.chunks.iter().enumerate() {
        ensure(
            chunk.id == format!("{}:{}:{i}", file.id, file.version)
                && chunk.digest == d.chunks[i].digest
                && chunk.size == (file.size - i as u64 * CHUNK as u64).min(CHUNK as u64),
            Error::IntegrityFailed,
        )?;
    }
    let mut file = SensitiveManifest(manifest.file);
    check_package_size(file.size, record.max_plain_bytes)?;
    file.key = crate::codec::b64(&key[..]);
    let mut whole = Zeroizing::new(Vec::with_capacity(file.size as usize));
    for index in 0..file.chunks.len() {
        let bytes = ciphertext(&bound, &cloud, &mut record, &mut grant, Some(index)).await?;
        let plain = bound
            .with(|c| {
                c.blobs()?.put_chunk(&file.chunks[index].digest, &bytes)?;
                c.hold_chunks([(
                    record.id.clone(),
                    record.expires_at,
                    file.chunks[..=index]
                        .iter()
                        .map(|c| c.digest.clone())
                        .collect(),
                )])?;
                crate::files::read_chunk(c.blobs()?, &file, index as u64)
            })
            .await?;
        whole.extend_from_slice(&plain);
    }
    ensure(
        whole.len() as u64 == file.size && hex::encode(Sha256::digest(&whole)) == file.sha256,
        Error::IntegrityFailed,
    )?;
    let parsed = package::parse(&whole)?;
    record.preview = Some(parsed.manifest.preview());
    record.digest = Some(file.sha256.clone());
    record.file = Some(file.0.clone());
    file.key.zeroize();
    bound.with(|c|{c.save_received(&record)?;Ok(json!({"package_id":record.id,"digest":record.digest,"preview":record.preview,"files_local_only":true,"files_inherit_folder_grants":true}))}).await
}

/// Validate and stage a complete `dmsg-secret-package/1` received over
/// `dmsg-nearby-share/1`, returning the same value-free preview shape used by
/// link and channel secret packages so the receiver enters the direct import
/// selection flow.
pub(crate) async fn stage_nearby_package<H: Hold>(
    h: &H,
    raw_bytes: Zeroizing<Vec<u8>>,
    expected_sha256: &str,
    max_plain_bytes: u64,
    peer_label: Option<&str>,
) -> Result<Value> {
    let size = raw_bytes.len() as u64;
    check_package_size(size, max_plain_bytes)?;
    ensure_valid(hex_id(expected_sha256, 64), "package digest")?;
    let actual = hex::encode(Sha256::digest(&raw_bytes));
    ensure(actual == expected_sha256, Error::IntegrityFailed)?;
    let parsed = package::parse(&raw_bytes)?;
    let preview = parsed.manifest.preview();
    // Standalone workspaces receive face to face too: bind the session, not an account.
    let bound = Session::new(h).await?;
    let now = now_ms();
    let expires_at = now + 30 * MINUTE;
    let id = hex::encode(random_id());
    let mut staged = bound
        .with(|c| c.secret_stage_start("nearby.dmsg-package", package::MIME, size))
        .await?;
    for block in raw_bytes.chunks(CHUNK) {
        bound
            .with(|c| c.secret_stage_part(&mut staged, block))
            .await?;
    }
    let staged = bound
        .with(|c| {
            let mut staged = staged;
            c.secret_stage_finish(&mut staged, Some(expected_sha256))?;
            Ok(staged)
        })
        .await?;
    let digests: Vec<String> = staged
        .manifest
        .chunks
        .iter()
        .map(|chunk| chunk.digest.clone())
        .collect();
    let mut record = Received {
        id: id.clone(),
        link_digest: format!("nearby:{expected_sha256}"),
        session_id: hex::encode(random_id()),
        started: true,
        descriptor: None,
        channel_source: Some(json!({"nearby": true, "peer": peer_label})),
        key: staged.manifest.key.clone(),
        file: Some(staged.manifest.clone()),
        digest: Some(expected_sha256.into()),
        preview: Some(preview),
        created_at: now,
        expires_at,
        imported: None,
        request: None,
        selection: None,
        request_boot: None,
        request_session: None,
        max_plain_bytes,
    };
    bound
        .with(|c| {
            c.hold_chunks([(id.clone(), expires_at, digests)])?;
            c.save_received(&record)?;
            record.key.zeroize();
            Ok(json!({
                "package_id": record.id,
                "digest": record.digest,
                "preview": record.preview,
                "files_local_only": true,
                "files_inherit_folder_grants": true
            }))
        })
        .await
}

async fn received_bytes<H: Hold>(h: &H, received: &Received) -> Result<Zeroizing<Vec<u8>>> {
    let file = received.file.as_ref().ok_or(Error::RecoveryIncomplete)?;
    check_package_size(file.size, received.max_plain_bytes)?;
    let mut bytes = Zeroizing::new(Vec::with_capacity(file.size as usize));
    for i in 0..file.chunks.len() {
        let block = h
            .with(|c| {
                ensure(now_ms() < received.expires_at, Error::Expired)?;
                crate::files::read_chunk(c.blobs()?, file, i as u64)
            })
            .await?;
        bytes.extend_from_slice(&block);
    }
    ensure(
        hex::encode(Sha256::digest(&bytes)) == file.sha256,
        Error::IntegrityFailed,
    )?;
    Ok(bytes)
}

/// Import a validated received snapshot directly into Vault/Files in one atomic transaction.
/// Duplicate submission of the same selection returns the original receipt; a different
/// selection after import fails with `Error::IdempotencyConflict`.
pub async fn import<H: Hold>(h: &H, selection: Selection) -> Result<Value> {
    import_limited(h, selection, crate::files::MAX_FILE).await
}

/// Direct import with a host-specific whole-package plaintext buffer limit.
pub async fn import_limited<H: Hold>(
    h: &H,
    selection: Selection,
    max_plain_bytes: u64,
) -> Result<Value> {
    let bound = Session::new(h).await?;
    let received = match bound
        .with(|c| c.secret_package_prepare_import(&selection, max_plain_bytes))
        .await?
    {
        ImportPrep::Done(receipt) => return Ok(receipt),
        ImportPrep::Run(received) => received,
    };
    let bytes = received_bytes(&bound, &received).await?;
    let parsed = package::parse(&bytes)?;
    let mut item = parsed
        .manifest
        .new_item(&selection.fields, selection.include_notes)?;
    let picks: BTreeSet<_> = selection.attachments.iter().copied().collect();
    ensure_valid(
        picks.len() == selection.attachments.len()
            && picks.iter().all(|i| *i < parsed.attachments.len()),
        "package attachments",
    )?;
    let mut files = Vec::new();
    for index in picks {
        let metadata = &parsed.manifest.attachments[index];
        let data = parsed.attachments[index];
        let name = metadata
            .name
            .rsplit(['/', '\\'])
            .next()
            .filter(|n| !n.is_empty() && *n != "." && *n != "..")
            .unwrap_or("attachment.bin");
        let name = name
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect::<String>();
        let name = if name.is_empty() {
            "attachment.bin"
        } else {
            &name
        };
        let mut file = bound
            .with(|c| c.secret_stage_start(name, &metadata.mime, metadata.size))
            .await?;
        for block in data.chunks(CHUNK) {
            bound
                .with(|c| c.secret_stage_part(&mut file, block))
                .await?;
        }
        bound
            .with(|c| c.secret_stage_finish(&mut file, Some(&metadata.sha256)))
            .await?;
        item.files.push(file.id().into());
        files.push(file);
    }
    bound
        .with(|c| {
            let now = now_ms();
            let mut current = c.received(&received.id)?;
            if let Some(ids) = &current.imported {
                ensure(
                    current.selection.as_ref() == Some(&selection),
                    Error::IdempotencyConflict,
                )?;
                return Ok(
                    json!({"state":"done","ids":ids,"files_local_only":true,"files_inherit_folder_grants":true}),
                );
            }
            ensure(
                current.digest.as_deref() == Some(selection.digest.as_str())
                    && now < current.expires_at,
                Error::Expired,
            )?;
            c.catalog()?.live_folder(&selection.folder)?;
            let mut imported_ids = Vec::new();
            c.secret_import_commit_with(vec![item], files, &selection.folder, |c, ids| {
                imported_ids = ids.to_vec();
                current.imported = Some(ids.to_vec());
                current.selection = Some(selection.clone());
                current.request = None;
                current.request_boot = None;
                current.request_session = None;
                c.save_received(&current)
            })?;
            c.release_chunks(&received.id)?;
            Ok(
                json!({"state":"done","ids":imported_ids,"files_local_only":true,"files_inherit_folder_grants":true}),
            )
        })
        .await
}

/// Receive a verified channel attachment without exporting a plaintext package to Files or disk.
pub async fn channel_preview<H: Hold>(h: &H, channel: &str, seq: u64) -> Result<Value> {
    channel_preview_limited(h, channel, seq, crate::files::MAX_FILE).await
}

/// Channel receive with a smaller whole-package buffer budget for constrained hosts.
pub async fn channel_preview_limited<H: Hold>(
    h: &H,
    channel: &str,
    seq: u64,
    max_plain_bytes: u64,
) -> Result<Value> {
    check_package_size(0, max_plain_bytes)?;
    let bound = Unlocked::new(h).await?;
    let message = bound
        .with(|c| c.snapshot_channel_message(channel, seq))
        .await?;
    let reference = message.file.as_ref().ok_or(Error::NotFound)?;
    ensure(reference.mime == package::MIME, Error::UnsupportedProtocol)?;
    check_package_size(reference.size, max_plain_bytes)?;
    let source = json!({"channel_id":channel,"seq":seq,"message_digest":message.digest,"sender_account":message.account});
    let identity = hex::encode(Sha256::digest(canonical(&(
        "dmsg/channel-secret-import/1",
        &source,
    ))));
    if let Some(view) = bound.with(|c| {
        for key in c.workspace.store.sealed_keys(PREFIX)? {
            if let Some(record) = c.workspace.private::<Received>(c.keys()?, &key)?
                && record.link_digest == identity && (record.imported.is_some() || now_ms() < record.expires_at)
            {
                check_package_size(record.file.as_ref().ok_or(Error::RecoveryIncomplete)?.size, max_plain_bytes)?;
                return Ok(Some(json!({"package_id":record.id,"digest":record.digest,"preview":record.preview,"channel_source":source,"already_imported":record.imported,"files_local_only":true,"files_inherit_folder_grants":true})));
            }
        }
        ensure(c.workspace.store.sealed_keys(PREFIX)?.len() < 128, Error::QuotaExceeded)?;
        Ok(None)
    }).await? {return Ok(view);}
    let reference = message.file.as_ref().ok_or(Error::NotFound)?;
    let file = SensitiveManifest(
        crate::channel::fetch_file(&bound, channel, reference, &message.account).await?,
    );
    channel_received(
        &bound,
        channel,
        seq,
        &message,
        &file,
        source,
        identity,
        max_plain_bytes,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn channel_received<H: Hold>(
    bound: &H,
    channel: &str,
    seq: u64,
    message: &crate::channel::Message,
    file: &SensitiveManifest,
    source: Value,
    identity: String,
    max_plain_bytes: u64,
) -> Result<Value> {
    check_package_size(file.size, max_plain_bytes)?;
    let mut whole = Zeroizing::new(Vec::with_capacity(file.size as usize));
    let id = hex::encode(random_id());
    let now = now_ms();
    let expires_at = now + 30 * MINUTE;
    for (i, chunk) in file.chunks.iter().enumerate() {
        let bytes = bound
            .with(|c| {
                ensure(
                    c.snapshot_channel_message(channel, seq)?.digest == message.digest,
                    Error::VersionConflict,
                )?;
                let encrypted = c
                    .workspace
                    .store
                    .chunk(&chunk.digest)?
                    .ok_or(Error::RecoveryIncomplete)?;
                c.blobs()?.put_chunk(&chunk.digest, &encrypted)?;
                c.hold_chunks([(
                    id.clone(),
                    expires_at,
                    file.chunks[..=i].iter().map(|p| p.digest.clone()).collect(),
                )])?;
                crate::files::read_chunk(c.blobs()?, file, i as u64)
            })
            .await?;
        whole.extend_from_slice(&bytes);
    }
    ensure(
        whole.len() as u64 == file.size && hex::encode(Sha256::digest(&whole)) == file.sha256,
        Error::IntegrityFailed,
    )?;
    let parsed = package::parse(&whole)?;
    let record = Received {
        id,
        link_digest: identity,
        session_id: String::new(),
        started: false,
        descriptor: None,
        channel_source: Some(source),
        key: String::new(),
        file: Some(file.0.clone()),
        digest: Some(file.sha256.clone()),
        preview: Some(parsed.manifest.preview()),
        created_at: now_ms(),
        expires_at,
        imported: None,
        request: None,
        selection: None,
        request_boot: None,
        request_session: None,
        max_plain_bytes,
    };
    bound.with(|c| {
        ensure(c.snapshot_channel_message(channel, seq)?.digest == message.digest, Error::VersionConflict)?;
        // Concurrent previews converge on one receipt identity instead of creating duplicate imports.
        for key in c.workspace.store.sealed_keys(PREFIX)? {
            if let Some(prior) = c.workspace.private::<Received>(c.keys()?, &key)?
                && prior.link_digest == record.link_digest && (prior.imported.is_some() || now_ms() < prior.expires_at)
            {return Ok(json!({"package_id":prior.id,"digest":prior.digest,"preview":prior.preview,"channel_source":prior.channel_source,"already_imported":prior.imported,"files_local_only":true,"files_inherit_folder_grants":true}));}
        }
        c.save_received(&record)?;
        Ok(json!({"package_id":record.id,"digest":record.digest,"preview":record.preview,"channel_source":record.channel_source,"files_local_only":true,"files_inherit_folder_grants":true}))
    }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        testing::{TempDir, unlocked},
        vault::Field,
    };
    fn fixture() -> (TempDir, Client, Selection) {
        let dir = TempDir::new();
        let mut c = unlocked(&dir.0, false, None);
        let manifest = package::Manifest {
            format: package::FORMAT.into(),
            snapshot_id: "ca".repeat(32),
            created_at: 1,
            item: package::Snapshot {
                kind: "api".into(),
                title: "Receiver fixture".into(),
                fields: vec![
                    Field::new("f0_credential", "concealed", "SECRET_RECEIVER_FIXTURE"),
                    Field::new("f1", "text", "not selected"),
                ],
                notes: Some("unselected notes".into()),
            },
            attachments: vec![package::Attachment {
                name: "../fixture.txt".into(),
                mime: "text/plain".into(),
                size: 4,
                sha256: hex::encode(Sha256::digest(b"test")),
            }],
        };
        let mut bytes = manifest.header().unwrap();
        bytes.extend_from_slice(b"test");
        let mut file = c
            .secret_stage_start("received", package::MIME, bytes.len() as u64)
            .unwrap();
        c.secret_stage_part(&mut file, &bytes).unwrap();
        c.secret_stage_finish(&mut file, None).unwrap();
        let now = now_ms();
        let id = hex::encode(random_id());
        let digest = file.manifest.sha256.clone();
        let record = Received {
            id: id.clone(),
            link_digest: "a0".repeat(32),
            session_id: "a1".repeat(32),
            started: true,
            descriptor: Some(Descriptor {
                status: "available".into(),
                share_id: "a2".repeat(32),
                upload_id: "a3".repeat(32),
                object_id: "a2".repeat(32),
                version_id: "a4".repeat(32),
                manifest: ChunkRef {
                    size: 1,
                    digest: "b0".repeat(32),
                },
                chunks: vec![],
                expires_at: now + MINUTE,
                password: false,
                key_wrap: None,
            }),
            channel_source: None,
            key: file.manifest.key.clone(),
            file: Some(file.manifest.clone()),
            digest: Some(digest.clone()),
            preview: Some(manifest.preview()),
            created_at: now,
            expires_at: now + MINUTE,
            imported: None,
            request: None,
            selection: None,
            request_boot: None,
            request_session: None,
            max_plain_bytes: crate::files::MAX_FILE,
        };
        c.save_received(&record).unwrap();
        (
            dir,
            c,
            Selection {
                package_id: id,
                digest,
                fields: vec!["f0_credential".into()],
                include_notes: false,
                attachments: vec![0],
                folder: "root".into(),
            },
        )
    }

    #[tokio::test]
    async fn constrained_receiver_rechecks_cached_packages_and_import_budget() {
        let (_dir, mut c, selection) = fixture();
        let mut record = c.received(&selection.package_id).unwrap();
        let size = record.file.as_ref().unwrap().size;
        let mut url = url::Url::parse(&c.config.login_url).unwrap();
        url.set_path(&format!("/s/{}", "a2".repeat(32)));
        url.set_fragment(Some(&record.key));
        let url = url.to_string();
        record.link_digest = hex::encode(Sha256::digest(url.as_bytes()));
        c.save_received(&record).unwrap();

        // The cached path cannot bypass a smaller host budget or start a session.
        assert!(matches!(
            preview_limited(&c.hold(), &url, None, size - 1).await,
            Err(Error::QuotaExceeded)
        ));
        let preview = preview_limited(&c.hold(), &url, None, size).await.unwrap();
        assert_eq!(preview["package_id"], selection.package_id);
        assert_eq!(preview["files_inherit_folder_grants"], true);
        assert!(!preview.to_string().contains("SECRET_RECEIVER_FIXTURE"));
        assert!(matches!(
            import_limited(&c.hold(), selection.clone(), size - 1).await,
            Err(Error::QuotaExceeded)
        ));
        assert!(c.approvals(true).unwrap().is_empty());

        let result = import_limited(&c.hold(), selection.clone(), size)
            .await
            .unwrap();
        let stored = c.received(&selection.package_id).unwrap();
        assert_eq!(stored.max_plain_bytes, size);
        assert_eq!(result["state"], "done");
        assert_eq!(result["files_local_only"], true);
        assert_eq!(
            import_limited(&c.hold(), selection, size).await.unwrap()["state"],
            "done"
        );
        assert!(c.approvals(true).unwrap().is_empty());
    }

    #[tokio::test]
    async fn persisted_receiver_budget_is_checked_before_allocating_import_buffer() {
        let (_dir, mut c, selection) = fixture();
        let mut record = c.received(&selection.package_id).unwrap();
        record.max_plain_bytes = record.file.as_ref().unwrap().size - 1;
        assert!(matches!(
            received_bytes(&c.hold(), &record).await,
            Err(Error::QuotaExceeded)
        ));
        let mut legacy = serde_json::to_value(&record).unwrap();
        legacy.as_object_mut().unwrap().remove("max_plain_bytes");
        let legacy: Received = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.max_plain_bytes, crate::files::MAX_FILE);
    }

    #[tokio::test]
    async fn imported_selection_receipt_is_atomic_and_cannot_be_reused_for_other_content() {
        let (_dir, mut c, selection) = fixture();
        let result = import(&c.hold(), selection.clone()).await.unwrap();
        assert_eq!(result["state"], "done");
        let duplicate = import(&c.hold(), selection.clone()).await.unwrap();
        assert_eq!(duplicate, result);
        let id = duplicate["ids"][0].as_str().unwrap();
        let entry = c.vault_show(id).unwrap();
        assert_eq!(entry.item.value("credential"), "SECRET_RECEIVER_FIXTURE");
        assert_eq!(entry.item.fields.len(), 1);
        assert!(entry.item.notes.is_empty());
        assert_eq!(entry.item.files.len(), 1);
        let file = c.files_show(&entry.item.files[0]).unwrap();
        assert!(file.versions.iter().all(|v| v.backup.is_none()));
        assert_eq!(file.file.name, "fixture.txt");
        let mut changed = selection.clone();
        changed.fields = vec!["f1".into()];
        assert_eq!(
            import(&c.hold(), changed).await,
            Err(Error::IdempotencyConflict)
        );
        assert_eq!(
            c.vault_list(&crate::vault::Filter::default())
                .unwrap()
                .len(),
            1
        );
        assert!(c.approvals(true).unwrap().is_empty());
    }

    #[tokio::test]
    async fn invalid_or_expired_selection_never_imports() {
        let (_dir, mut c, selection) = fixture();
        let mut bad = selection.clone();
        bad.fields.push(bad.fields[0].clone());
        assert!(import(&c.hold(), bad).await.is_err());
        let mut expired = c.received(&selection.package_id).unwrap();
        expired.expires_at = 1;
        c.save_received(&expired).unwrap();
        assert_eq!(import(&c.hold(), selection).await, Err(Error::Expired));
        assert!(
            c.vault_list(&crate::vault::Filter::default())
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn locked_client_never_commits_plaintext_or_a_receipt() {
        let (_dir, mut c, selection) = fixture();
        c.lock();
        assert_eq!(import(&c.hold(), selection).await, Err(Error::Locked));
    }
    #[tokio::test]
    async fn channel_ciphertext_is_verified_before_preview_and_reuses_the_import_receipt() {
        let (_dir, mut c, mut selection) = fixture();
        let file = SensitiveManifest(
            c.received(&selection.package_id)
                .unwrap()
                .file
                .clone()
                .unwrap(),
        );
        let (channel, _) = crate::channel::tests::active_channel(&mut c);
        let target = c.snapshot_channel_target(&channel).unwrap();
        let (reference, _, _) = c
            .snapshot_channel_upload(&target, &file, &"aa".repeat(32))
            .unwrap();
        for chunk in &file.chunks {
            c.workspace
                .store
                .put_chunk(
                    &chunk.digest,
                    &c.blobs().unwrap().chunk(&chunk.digest).unwrap().unwrap(),
                )
                .unwrap();
        }
        let message = crate::channel::Message {
            seq: 1,
            account: c.account().unwrap().id.to_string(),
            file: Some(reference),
            digest: "dd".repeat(32),
            ..Default::default()
        };
        c.workspace
            .put_private(
                c.keys().unwrap(),
                &format!("channel-msg:{channel}:{:020}", 1),
                &message,
            )
            .unwrap();
        let source = json!({"channel_id":channel,"seq":1,"message_digest":message.digest});
        let identity = hex::encode(Sha256::digest(canonical(&source)));
        let preview = channel_received(
            &c.hold(),
            &channel,
            1,
            &message,
            &file,
            source.clone(),
            identity.clone(),
            crate::files::MAX_FILE,
        )
        .await
        .unwrap();
        assert!(!preview.to_string().contains("SECRET_RECEIVER_FIXTURE"));
        let duplicate = channel_received(
            &c.hold(),
            &channel,
            1,
            &message,
            &file,
            source.clone(),
            identity.clone(),
            crate::files::MAX_FILE,
        )
        .await
        .unwrap();
        assert_eq!(preview["package_id"], duplicate["package_id"]);
        assert_eq!(preview["channel_source"], source);
        selection.package_id = preview["package_id"].as_str().unwrap().into();
        let receipt = import(&c.hold(), selection.clone()).await.unwrap();
        assert_eq!(receipt["state"], "done");
        assert_eq!(
            c.vault_show(receipt["ids"][0].as_str().unwrap())
                .unwrap()
                .item
                .value("credential"),
            "SECRET_RECEIVER_FIXTURE"
        );
        assert_eq!(import(&c.hold(), selection).await.unwrap(), receipt);
        assert!(c.approvals(true).unwrap().is_empty());
        let mut broken = SensitiveManifest(file.0.clone());
        broken.sha256 = "00".repeat(32);
        assert!(
            channel_received(
                &c.hold(),
                &channel,
                1,
                &message,
                &broken,
                source.clone(),
                "ff".repeat(32),
                crate::files::MAX_FILE,
            )
            .await
            .is_err()
        );
        let withdrawn = crate::channel::Message {
            retracted: true,
            ..message.clone()
        };
        c.workspace
            .put_private(
                c.keys().unwrap(),
                &format!("channel-msg:{channel}:{:020}", 1),
                &withdrawn,
            )
            .unwrap();
        assert!(
            channel_received(
                &c.hold(),
                &channel,
                1,
                &message,
                &file,
                source,
                identity,
                crate::files::MAX_FILE,
            )
            .await
            .is_err()
        );
    }
}
