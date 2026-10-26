//! Importing: one pass over the plaintext through a 1 MiB buffer, hashing
//! the whole, sniffing the type from the first bytes, sealing each chunk
//! into the blob directory and recording progress in a sealed job, so an
//! interrupted import of the same file resumes with the same key and keeps
//! every chunk whose plaintext still matches. Images get a sealed
//! thumbnail. The version and its file are written in one transaction at
//! the end, and the job goes.

use super::Client;
use super::catalog::{FILE, FileRecord, Origin, Version, VersionRef, validate_name};
use crate::codec::{b64, sha256, unb64};
use crate::crypto::{open_bytes, random_key, seal_bytes};
use crate::files::{BlobStore, CHUNK, FileManifest, chunk_count, io, seal_chunk};
use crate::hold::Hold;
use crate::now_ms;
use crate::upload::{ChunkRef, random_hex};
use dmsg_types::{Error, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use zeroize::{Zeroize, Zeroizing};

pub(crate) const IMPORT: &str = "lib-import:";
/// Host-specific decode bounds. Oversized images still import normally;
/// only their optional thumbnail is omitted.
#[derive(Clone, Copy)]
pub struct ThumbnailBudget {
    /// Maximum compressed input retained for a thumbnail.
    pub source_bytes: u64,
    /// Maximum decoded pixel count.
    pub pixels: u64,
    /// Decoder allocation ceiling.
    pub allocation_bytes: u64,
}

impl Default for ThumbnailBudget {
    fn default() -> Self {
        Self {
            source_bytes: 64 * 1024 * 1024,
            pixels: 16_384 * 16_384,
            allocation_bytes: 512 * 1024 * 1024,
        }
    }
}

impl ThumbnailBudget {
    /// Small, optional thumbnails for mobile devices.
    pub const MOBILE: Self = Self {
        source_bytes: 8 * 1024 * 1024,
        pixels: 12_000_000,
        allocation_bytes: 48 * 1024 * 1024,
    };
}

static THUMBNAIL_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

/// Thumbnail edge, in pixels.
const THUMBNAIL: u32 = 256;
/// Bytes sniffed for the media type.
const SNIFF: usize = 8192;
/// Progress is reported every this many chunks.
const PROGRESS_EVERY: u64 = 16;

/// An unfinished import, sealed under the local data key.
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImportJob {
    folder: String,
    /// The file the version joins; a new file otherwise.
    file: Option<String>,
    /// The file ID the version is sealed under: `file`, or the new file's.
    file_id: String,
    name: String,
    size: u64,
    mime: String,
    version: String,
    key: String,
    origin: Origin,
    pub(crate) chunks: Vec<ChunkRef>,
    /// SHA-256 of each sealed chunk's plaintext.
    plain: Vec<String>,
    started_at: u64,
    #[serde(default)]
    receipt: Option<String>,
}

/// One import's state between bounded client steps. No account key leaves
/// the client; the version key and plaintext buffers are erased on drop.
struct Importing {
    job: ImportJob,
    resumed: bool,
    generation: u64,
    key: Zeroizing<[u8; 32]>,
    overall: Sha256,
    image: Option<Zeroizing<Vec<u8>>>,
    budget: ThumbnailBudget,
}

impl Drop for Importing {
    fn drop(&mut self) {
        self.job.key.zeroize();
    }
}

impl Importing {
    async fn thumbnail_async(&mut self) -> Option<(String, Vec<u8>)> {
        let image = self.image.take()?;
        let budget = self.budget;
        let permit = THUMBNAIL_SLOTS.acquire().await.ok()?;
        let png = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            thumbnail(&image, budget).map(Zeroizing::new)
        })
        .await
        .ok()??;
        let sealed = seal_bytes(
            &self.key,
            &png,
            &thumb_aad(&self.job.file_id, &self.job.version),
        );
        Some((hex::encode(*sha256(&sealed)), sealed))
    }

    /// Decode and seal a thumbnail without holding the client.
    fn thumbnail(&self) -> Option<(String, Vec<u8>)> {
        let png = self
            .image
            .as_deref()
            .and_then(|bytes| thumbnail(bytes, self.budget))?;
        let sealed = seal_bytes(
            &self.key,
            &png,
            &thumb_aad(&self.job.file_id, &self.job.version),
        );
        Some((hex::encode(*sha256(&sealed)), sealed))
    }
}

/// What an import produced.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Imported {
    /// The file.
    pub file: super::FileView,
    /// The new version.
    pub version: String,
    /// An interrupted import was resumed.
    pub resumed: bool,
    /// Files, not in the trash, that already hold these bytes.
    pub duplicate_of: Vec<String>,
}

pub(crate) fn thumb_aad<'a>(
    file_id: &'a str,
    version: &'a str,
) -> (&'static str, &'a str, &'a str) {
    ("dmsg/file-thumb/1", file_id, version)
}

/// A media type from the first bytes, else from the name's extension.
pub(crate) fn sniff(head: &[u8], name: &str) -> String {
    if let Some(kind) = infer::get(head) {
        return kind.mime_type().into();
    }
    let extension = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match extension.as_deref() {
        Some("txt" | "log") => "text/plain",
        Some("md" | "markdown") => "text/markdown",
        Some("csv") => "text/csv",
        Some("html" | "htm") => "text/html",
        Some("css") => "text/css",
        Some("js" | "mjs") => "text/javascript",
        Some("ts") => "text/typescript",
        Some("rs") => "text/x-rust",
        Some("py") => "text/x-python",
        Some("json") => "application/json",
        Some("toml") => "application/toml",
        Some("yaml" | "yml") => "application/yaml",
        Some("xml") => "application/xml",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
    .into()
}

/// Read up to the buffer's length.
pub(crate) fn fill(reader: &mut dyn Read, buffer: &mut [u8]) -> Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]).map_err(io)? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

/// A 256 px PNG thumbnail of an image, if it decodes.
fn thumbnail(bytes: &[u8], budget: ThumbnailBudget) -> Option<Vec<u8>> {
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    if u64::from(width) * u64::from(height) > budget.pixels {
        return None;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(budget.allocation_bytes);
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let small = decoded.thumbnail(THUMBNAIL, THUMBNAIL);
    let mut out = std::io::Cursor::new(Vec::new());
    small.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

impl Client {
    /// Limit optional image work without lowering the file-content limit.
    pub fn set_thumbnail_budget(&mut self, budget: ThumbnailBudget) {
        self.thumbnail_budget = budget;
    }

    /// Let `cancel` stop the imports that follow, until it is taken away
    /// with `None`. A stopped import leaves nothing: its job and chunks go.
    pub fn files_import_cancel(&mut self, cancel: Option<Arc<AtomicBool>>) {
        self.import_cancel = cancel;
    }

    /// Import the file at `path` into `folder`. A file of the same name
    /// there takes it as a new version, unless `keep_both` names the import
    /// `name (2)` and so on. An interrupted import of the same file resumes.
    ///
    /// # Errors
    /// Returns `Error::QuotaExceeded` beyond `files_max_bytes`,
    /// `Error::NotFound` for an unknown folder and `Error::IntegrityFailed`
    /// when a resumed file changed.
    pub fn files_import(
        &mut self,
        path: &Path,
        folder: &str,
        keep_both: bool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Imported> {
        ensure_valid(path.is_absolute(), "path")?;
        let metadata = std::fs::metadata(path).map_err(io)?;
        ensure_valid(metadata.is_file(), "path")?;
        ensure(
            metadata.len() <= self.config.files_max_bytes,
            Error::QuotaExceeded,
        )?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| dmsg_types::invalid("name"))?
            .to_string();
        validate_name(&name)?;
        let (into, name) = {
            let catalog = self.catalog()?;
            catalog.live_folder(folder)?;
            match keep_both {
                true => (None, catalog.free_name(folder, &name)),
                false => (catalog.find(folder, &name), name),
            }
        };
        let mut file = std::fs::File::open(path).map_err(io)?;
        self.import_stream(
            &mut file,
            metadata.len(),
            &name,
            folder,
            into,
            Origin::Imported,
            progress,
        )
    }

    /// Import `size` bytes from `reader` as a new version of `into`, else
    /// as a new file `name` in `folder`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn import_stream(
        &mut self,
        reader: &mut dyn Read,
        size: u64,
        name: &str,
        folder: &str,
        into: Option<String>,
        origin: Origin,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Imported> {
        let mut importing = self.import_start(size, name, folder, into, origin)?;
        let count = chunk_count(size);
        let mut buffer = Zeroizing::new(vec![0u8; CHUNK]);
        for index in 0..count {
            self.import_check(&importing)?;
            let length = fill(reader, &mut buffer)?;
            self.import_part(&mut importing, index, &buffer[..length])?;
            if (index + 1) % PROGRESS_EVERY == 0 || index + 1 == count {
                progress(index + 1, count);
            }
        }
        ensure(fill(reader, &mut buffer[..1])? == 0, Error::IntegrityFailed)?;
        let thumbnail = importing.thumbnail();
        self.import_finish(importing, thumbnail)
    }

    fn import_start(
        &mut self,
        size: u64,
        name: &str,
        folder: &str,
        into: Option<String>,
        origin: Origin,
    ) -> Result<Importing> {
        ensure(size <= self.config.files_max_bytes, Error::QuotaExceeded)?;
        validate_name(name)?;
        self.catalog()?.live_folder(folder)?;
        if let Some(file) = &into {
            let record = self.file_record(file)?;
            ensure_valid(
                record.folder == folder && record.trashed_at.is_none(),
                "file",
            )?;
        }
        let now = now_ms();
        let (job, resumed) = match self.import_job(folder, name, size, into.as_deref())? {
            Some(job) => (job, true),
            None => (
                ImportJob {
                    folder: folder.into(),
                    file: into.clone(),
                    file_id: into.clone().unwrap_or_else(random_hex),
                    name: name.into(),
                    size,
                    mime: String::new(),
                    version: random_hex(),
                    key: b64(&Zeroizing::new(random_key())[..]),
                    origin,
                    chunks: vec![],
                    plain: vec![],
                    started_at: now,
                    receipt: None,
                },
                false,
            ),
        };
        let job_key = format!("{IMPORT}{}", job.version);
        self.workspace.put_private(self.keys()?, &job_key, &job)?;
        let key = Zeroizing::new(
            <[u8; 32]>::try_from(Zeroizing::new(unb64(&job.key)?).as_slice())
                .map_err(|_| Error::IntegrityFailed)?,
        );
        Ok(Importing {
            job,
            resumed,
            generation: self.session_generation,
            key,
            overall: Sha256::new(),
            image: None,
            budget: self.thumbnail_budget,
        })
    }

    fn import_check(&mut self, importing: &Importing) -> Result<()> {
        self.keys()?;
        ensure(
            self.session_generation == importing.generation,
            Error::Locked,
        )?;
        if self
            .import_cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
        {
            self.workspace
                .delete_private(&format!("{IMPORT}{}", importing.job.version))?;
            self.collect_blobs()?;
            return Err(Error::Unavailable("import cancelled".into()));
        }
        Ok(())
    }

    fn import_part(
        &mut self,
        importing: &mut Importing,
        index: u64,
        plaintext: &[u8],
    ) -> Result<()> {
        self.import_check(importing)?;
        let job = &mut importing.job;
        let length = plaintext.len();
        let count = chunk_count(job.size);
        let expected = (job.size - index * CHUNK as u64).min(CHUNK as u64);
        ensure(length as u64 == expected, Error::IntegrityFailed)?;
        if index == 0 {
            let mime = sniff(&plaintext[..length.min(SNIFF)], &job.name);
            ensure(
                job.mime.is_empty() || job.mime == mime,
                Error::IntegrityFailed,
            )?;
            job.mime = mime;
            if job.mime.starts_with("image/") && job.size <= importing.budget.source_bytes {
                importing.image = Some(Zeroizing::new(Vec::with_capacity(job.size as usize)));
            }
        }
        if let Some(image) = importing.image.as_mut() {
            image.extend_from_slice(plaintext);
        }
        importing.overall.update(plaintext);
        let plain = hex::encode(*sha256(plaintext));
        if let Some(stored) = job.chunks.get(index as usize) {
            ensure(
                job.plain[index as usize] == plain && self.blobs()?.has_chunk(&stored.digest)?,
                Error::IntegrityFailed,
            )?;
        } else {
            let (sealed, digest) = seal_chunk(
                &importing.key,
                &job.file_id,
                &job.version,
                index,
                count,
                plaintext,
            );
            self.blobs()?.put_chunk(&digest, &sealed)?;
            job.chunks.push(ChunkRef {
                digest,
                size: length as u64,
            });
            job.plain.push(plain);
            self.workspace
                .put_private(self.keys()?, &format!("{IMPORT}{}", job.version), &job)?;
        }
        Ok(())
    }

    fn import_finish(
        &mut self,
        mut importing: Importing,
        thumbnail: Option<(String, Vec<u8>)>,
    ) -> Result<Imported> {
        self.import_check(&importing)?;
        let job = &mut importing.job;
        // Existing files keep a rename, move or metadata edit made while
        // their new version was read. A new file still needs its name free.
        if let Some(id) = &job.file {
            let current = self.file_record(id)?;
            ensure(current.trashed_at.is_none(), Error::VersionConflict)?;
            self.catalog()?.live_folder(&current.folder)?;
        } else {
            let catalog = self.catalog()?;
            catalog.live_folder(&job.folder)?;
            ensure(
                catalog.find(&job.folder, &job.name).is_none(),
                Error::VersionConflict,
            )?;
        }
        let now = now_ms();
        let size = job.size;
        let file_id = job.file_id.clone();
        let job_key = format!("{IMPORT}{}", job.version);
        if size == 0 {
            job.mime = sniff(&[], &job.name);
        }
        let digest = hex::encode(importing.overall.clone().finalize());
        let thumbnail = if let Some((digest, sealed)) = thumbnail {
            self.blobs()?.put_chunk(&digest, &sealed)?;
            Some(digest)
        } else {
            None
        };
        let version = Version {
            size,
            sha256: digest.clone(),
            created_at: now,
            device: hex::encode(*self.workspace.meta.device_id),
            origin: job.origin.clone(),
            key: job.key.clone(),
            chunks: job.chunks.clone(),
            thumbnail,
            local: true,
            backup: None,
        };
        let mut file = match &job.file {
            Some(id) => self.file_record(id)?,
            None => FileRecord {
                folder: job.folder.clone(),
                name: job.name.clone(),
                mime: job.mime.clone(),
                tags: vec![],
                note: String::new(),
                starred: false,
                created_at: now,
                updated_at: now,
                current: String::new(),
                versions: vec![],
                trashed_at: None,
            },
        };
        file.mime = job.mime.clone();
        file.current = job.version.clone();
        file.updated_at = now;
        file.versions.push(VersionRef {
            id: job.version.clone(),
            size,
            sha256: digest.clone(),
            created_at: now,
        });
        self.workspace.store.atomic(|_| {
            self.save_version(&file_id, &job.version, &version)?;
            self.workspace
                .put_private(self.keys()?, &format!("{FILE}{file_id}"), &file)?;
            if let Some(receipt) = &job.receipt {
                self.workspace.put_private(
                    self.keys()?,
                    &format!("lib-import-complete:{receipt}"),
                    &(file_id.clone(), job.version.clone()),
                )?;
            }
            self.workspace.delete_private(&job_key)
        })?;
        self.catalog()?.put_file(&file_id, file);
        let duplicate_of = self.files_with_digest(&digest, &file_id)?;
        let view = self.files_show(&file_id)?.file;
        Ok(Imported {
            file: view,
            version: job.version.clone(),
            resumed: importing.resumed,
            duplicate_of,
        })
    }

    /// The unfinished import of this file into this place.
    fn import_job(
        &self,
        folder: &str,
        name: &str,
        size: u64,
        into: Option<&str>,
    ) -> Result<Option<ImportJob>> {
        for key in self.workspace.store.sealed_keys(IMPORT)? {
            let job: Option<ImportJob> = self.workspace.private(self.keys()?, &key)?;
            if let Some(job) = job.filter(|j| {
                j.folder == folder && j.name == name && j.size == size && j.file.as_deref() == into
            }) {
                return Ok(Some(job));
            }
        }
        Ok(None)
    }

    /// Every unfinished import.
    pub(crate) fn import_jobs(&self) -> Result<Vec<ImportJob>> {
        let mut jobs = Vec::new();
        for key in self.workspace.store.sealed_keys(IMPORT)? {
            if let Some(job) = self.workspace.private::<ImportJob>(self.keys()?, &key)? {
                jobs.push(job);
            }
        }
        Ok(jobs)
    }

    /// Open a version's sealed thumbnail, if its chunk is still here.
    pub(crate) fn open_thumbnail(
        &self,
        manifest: &FileManifest,
        digest: &str,
    ) -> Result<Option<Zeroizing<Vec<u8>>>> {
        let Some(sealed) = self.blobs()?.chunk(digest)? else {
            return Ok(None);
        };
        let key = manifest.file_key()?;
        Ok(Some(open_bytes(
            &key,
            &sealed,
            &thumb_aad(&manifest.id, &manifest.version),
        )?))
    }

    /// Import the folder at `path` as a folder of the same name in
    /// `folder`, its ordinary files and folders within, as
    /// [`Self::files_import`] does; returns what each file produced. An
    /// existing folder of that name takes the files, unless `keep_both`
    /// names the new one `name (2)` and so on.
    ///
    /// # Errors
    /// As [`Self::files_import`]; the first failure stops the walk.
    pub fn files_import_dir(
        &mut self,
        path: &Path,
        folder: &str,
        keep_both: bool,
        progress: &mut dyn FnMut(&Path, u64, u64),
    ) -> Result<Vec<Imported>> {
        ensure_valid(path.is_absolute() && path.is_dir(), "path")?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| dmsg_types::invalid("name"))?;
        let name = match keep_both {
            true => self.catalog()?.free_folder_name(folder, name),
            false => name.to_string(),
        };
        let folder = &self.files_mkdir(folder, &name)?.id;
        let mut imported = Vec::new();
        for (child, is_dir) in directory_entries(path)? {
            if is_dir {
                imported.extend(self.files_import_dir(&child, folder, keep_both, progress)?);
            } else {
                let mut report = |done, total| progress(&child, done, total);
                imported.push(self.files_import(&child, folder, keep_both, &mut report)?);
            }
        }
        Ok(imported)
    }
}

/// Import a local file in bounded steps, releasing the client between chunks.
/// # Errors
/// As [`Client::files_import`]; locking or changing accounts stops the import.
pub async fn file<H: Hold>(
    h: &H,
    path: &Path,
    folder: &str,
    keep_both: bool,
    progress: &mut (dyn FnMut(u64, u64) + Send),
) -> Result<Imported> {
    file_once(h, path, folder, keep_both, None, progress).await
}

/// Import with a host-owned stable receipt, so a lost completion cannot create
/// a second library entry. The receipt is local to the unlocked account.
pub async fn file_once<H: Hold>(
    h: &H,
    path: &Path,
    folder: &str,
    keep_both: bool,
    receipt: Option<&str>,
    progress: &mut (dyn FnMut(u64, u64) + Send),
) -> Result<Imported> {
    if let Some(receipt) = receipt {
        ensure_valid(
            receipt.len() == 64 && receipt.bytes().all(|b| b.is_ascii_hexdigit()),
            "import receipt",
        )?;
        let completed = h
            .with(|c| {
                let found: Option<(String, String)> = c
                    .workspace
                    .private(c.keys()?, &format!("lib-import-complete:{receipt}"))?;
                found
                    .map(|(id, version)| {
                        Ok(Imported {
                            file: c.files_show(&id)?.file,
                            version,
                            resumed: true,
                            duplicate_of: vec![],
                        })
                    })
                    .transpose()
            })
            .await?;
        if let Some(result) = completed {
            return Ok(result);
        }
    }
    ensure_valid(path.is_absolute(), "path")?;
    let metadata = std::fs::metadata(path).map_err(io)?;
    ensure_valid(metadata.is_file(), "path")?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| dmsg_types::invalid("name"))?;
    let size = metadata.len();
    let mut importing = h
        .with(|c| {
            let catalog = c.catalog()?;
            catalog.live_folder(folder)?;
            let (into, name) = if keep_both {
                (None, catalog.free_name(folder, name))
            } else {
                (catalog.find(folder, name), name.to_owned())
            };
            let mut started = c.import_start(size, &name, folder, into, Origin::Imported)?;
            started.job.receipt = receipt.map(str::to_owned);
            c.workspace.put_private(
                c.keys()?,
                &format!("{IMPORT}{}", started.job.version),
                &started.job,
            )?;
            Ok(started)
        })
        .await?;
    let mut reader = std::fs::File::open(path).map_err(io)?;
    let count = chunk_count(size);
    let mut buffer = Zeroizing::new(vec![0u8; CHUNK]);
    for index in 0..count {
        h.with(|c| c.import_check(&importing)).await?;
        let length = fill(&mut reader, &mut buffer)?;
        h.with(|c| c.import_part(&mut importing, index, &buffer[..length]))
            .await?;
        if (index + 1) % PROGRESS_EVERY == 0 || index + 1 == count {
            progress(index + 1, count);
        }
    }
    ensure(
        fill(&mut reader, &mut buffer[..1])? == 0,
        Error::IntegrityFailed,
    )?;
    let thumbnail = importing.thumbnail_async().await;
    h.with(|c| c.import_finish(importing, thumbnail)).await
}

/// Re-encrypt a verified shared manifest without assembling its plaintext
/// into one allocation or publishing a partially verified library version.
/// # Errors
/// As [`file`].
pub(crate) async fn from_manifest<H: Hold>(
    h: &H,
    manifest: &crate::files::FileManifest,
    folder: &str,
    origin: Origin,
    progress: &mut (dyn FnMut(u64, u64) + Send),
) -> Result<Imported> {
    let name = super::catalog::safe_name(&manifest.name);
    let mut importing = h
        .with(|c| {
            let into = c.catalog()?.find(folder, &name);
            c.import_start(manifest.size, &name, folder, into, origin)
        })
        .await?;
    let count = chunk_count(manifest.size);
    for index in 0..count {
        let plain = h
            .with(|c| crate::files::read_chunk(&c.workspace.store, manifest, index))
            .await?;
        h.with(|c| c.import_part(&mut importing, index, &plain))
            .await?;
        progress(index + 1, count);
    }
    ensure(
        hex::encode(importing.overall.clone().finalize()) == manifest.sha256,
        Error::IntegrityFailed,
    )?;
    let thumbnail = importing.thumbnail_async().await;
    h.with(|c| c.import_finish(importing, thumbnail)).await
}

/// Import a directory while yielding between files and chunks.
/// # Errors
/// As [`file`].
pub async fn directory<H: Hold>(
    h: &H,
    path: &Path,
    parent: &str,
    keep_both: bool,
    progress: &mut (dyn FnMut(&Path, u64, u64) + Send),
) -> Result<Vec<Imported>> {
    ensure_valid(path.is_absolute() && path.is_dir(), "path")?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| dmsg_types::invalid("name"))?;
    let folder = h
        .with(|c| {
            let name = if keep_both {
                c.catalog()?.free_folder_name(parent, name)
            } else {
                name.to_owned()
            };
            Ok(c.files_mkdir(parent, &name)?.id)
        })
        .await?;
    let mut imported = Vec::new();
    for (child, is_dir) in directory_entries(path)? {
        if is_dir {
            imported.extend(Box::pin(directory(h, &child, &folder, keep_both, progress)).await?);
        } else {
            let mut report = |done, total| progress(&child, done, total);
            imported.push(file(h, &child, &folder, keep_both, &mut report).await?);
        }
    }
    Ok(imported)
}

fn directory_entries(path: &Path) -> Result<Vec<(std::path::PathBuf, bool)>> {
    let mut entries: Vec<_> = std::fs::read_dir(path)
        .map_err(io)?
        .collect::<std::io::Result<_>>()
        .map_err(io)?;
    entries.sort_by_key(|e| e.file_name());
    let mut children = Vec::new();
    for entry in entries {
        let kind = entry.file_type().map_err(io)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if matches!(name, ".DS_Store" | "Thumbs.db" | "desktop.ini") || name.starts_with("._") {
            continue;
        }
        if kind.is_dir() || kind.is_file() {
            children.push((entry.path(), kind.is_dir()));
        }
    }
    Ok(children)
}

#[cfg(test)]
mod mobile_tests {
    use super::*;

    #[tokio::test]
    async fn completed_import_receipt_survives_a_lost_response() {
        let dir = crate::testing::TempDir::new();
        let source = dir.0.join("original.txt");
        std::fs::write(&source, b"selected bytes").unwrap();
        let mut c = crate::testing::unlocked(&dir.0.join("workspace"), false, None);
        let receipt = "ab".repeat(32);
        let first = file_once(
            &c.cooperative_hold(),
            &source,
            "root",
            true,
            Some(&receipt),
            &mut |_, _| {},
        )
        .await
        .unwrap();
        let again = file_once(
            &c.cooperative_hold(),
            &source,
            "root",
            true,
            Some(&receipt),
            &mut |_, _| {},
        )
        .await
        .unwrap();
        assert_eq!(first.file.id, again.file.id);
        assert_eq!(first.version, again.version);
        assert_eq!(c.files_list("root", false).unwrap().files.len(), 1);
        c.lock();
        assert!(
            file_once(
                &c.cooperative_hold(),
                &source,
                "root",
                true,
                Some(&receipt),
                &mut |_, _| {}
            )
            .await
            .is_err()
        );
    }

    #[test]
    fn thumbnail_pixel_budget_skips_optional_work_without_rejecting_the_file() {
        let image = image::DynamicImage::new_rgb8(200, 200);
        let mut out = std::io::Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Png).unwrap();
        let bytes = out.into_inner();
        assert!(
            thumbnail(
                &bytes,
                ThumbnailBudget {
                    pixels: 100,
                    ..ThumbnailBudget::MOBILE
                }
            )
            .is_none()
        );
        assert!(thumbnail(&bytes, ThumbnailBudget::MOBILE).is_some());
    }
}
