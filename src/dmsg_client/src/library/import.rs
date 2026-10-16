//! Importing: one pass over the plaintext through a 1 MiB buffer, hashing
//! the whole, sniffing the type from the first bytes, sealing each chunk
//! into the blob directory and recording progress in a sealed job, so an
//! interrupted import of the same file resumes with the same key and keeps
//! every chunk whose plaintext still matches. Images get a sealed
//! thumbnail. The version and its file are written in one transaction at
//! the end, and the job goes.

use super::catalog::{validate_name, FileRecord, Origin, Version, VersionRef, FILE};
use super::Client;
use crate::codec::{b64, sha256, unb64};
use crate::crypto::{open, random_key, seal};
use crate::files::{chunk_count, io, seal_chunk, BlobStore, FileManifest, CHUNK};
use crate::now_ms;
use crate::upload::{random_hex, ChunkRef};
use dmsg_types::{ensure, ensure_valid, Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use zeroize::Zeroizing;

pub(crate) const IMPORT: &str = "lib-import:";
/// Largest image a thumbnail is made of.
const MAX_THUMBNAIL_SOURCE: u64 = 64 * 1024 * 1024;
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

fn thumb_aad<'a>(file_id: &'a str, version: &'a str) -> (&'static str, &'a str, &'a str) {
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
fn thumbnail(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(512 * 1024 * 1024);
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
        let (mut job, resumed) = match self.import_job(folder, name, size, into.as_deref())? {
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
                },
                false,
            ),
        };
        let file_id = job.file_id.clone();
        let job_key = format!("{IMPORT}{}", job.version);
        self.workspace.put_private(self.keys()?, &job_key, &job)?;
        let key = Zeroizing::new(
            <[u8; 32]>::try_from(Zeroizing::new(unb64(&job.key)?).as_slice())
                .map_err(|_| Error::IntegrityFailed)?,
        );
        let count = chunk_count(size);
        let mut overall = Sha256::new();
        let mut buffer = Zeroizing::new(vec![0u8; CHUNK]);
        // Images small enough for a thumbnail are kept whole until the end.
        let mut image: Option<Zeroizing<Vec<u8>>> = None;
        for index in 0..count {
            let length = fill(reader, &mut buffer)?;
            let expected = (size - index * CHUNK as u64).min(CHUNK as u64);
            ensure(length as u64 == expected, Error::IntegrityFailed)?;
            let plaintext = &buffer[..length];
            if index == 0 {
                let mime = sniff(&plaintext[..length.min(SNIFF)], name);
                ensure(
                    job.mime.is_empty() || job.mime == mime,
                    Error::IntegrityFailed,
                )?;
                job.mime = mime;
                if job.mime.starts_with("image/") && size <= MAX_THUMBNAIL_SOURCE {
                    image = Some(Zeroizing::new(Vec::with_capacity(size as usize)));
                }
            }
            if let Some(image) = image.as_mut() {
                image.extend_from_slice(plaintext);
            }
            overall.update(plaintext);
            let plain = hex::encode(*sha256(plaintext));
            if let Some(stored) = job.chunks.get(index as usize) {
                ensure(
                    job.plain[index as usize] == plain
                        && self.blobs()?.has_chunk(&stored.digest)?,
                    Error::IntegrityFailed,
                )?;
            } else {
                let (sealed, digest) =
                    seal_chunk(&key, &file_id, &job.version, index, count, plaintext);
                self.blobs()?.put_chunk(&digest, &sealed)?;
                job.chunks.push(ChunkRef {
                    digest,
                    size: length as u64,
                });
                job.plain.push(plain);
                self.workspace.put_private(self.keys()?, &job_key, &job)?;
            }
            if (index + 1) % PROGRESS_EVERY == 0 || index + 1 == count {
                progress(index + 1, count);
            }
        }
        ensure(fill(reader, &mut buffer[..1])? == 0, Error::IntegrityFailed)?;
        if size == 0 {
            job.mime = sniff(&[], name);
        }
        let digest = hex::encode(overall.finalize());
        let thumbnail = match image.as_deref().and_then(|bytes| thumbnail(bytes)) {
            Some(png) => {
                let sealed = unb64(&seal(&key, &png, &thumb_aad(&file_id, &job.version)))?;
                let digest = hex::encode(*sha256(&sealed));
                self.blobs()?.put_chunk(&digest, &sealed)?;
                Some(digest)
            }
            None => None,
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
        let mut file = match &into {
            Some(id) => self.file_record(id)?,
            None => FileRecord {
                folder: folder.into(),
                name: name.into(),
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
            self.workspace.delete_private(&job_key)
        })?;
        self.catalog()?.files.insert(file_id.clone(), file);
        let duplicate_of = self.files_with_digest(&digest, &file_id)?;
        let view = self.files_show(&file_id)?.file;
        Ok(Imported {
            file: view,
            version: job.version,
            resumed,
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
        Ok(Some(open(
            &key,
            &b64(&sealed),
            &thumb_aad(&manifest.id, &manifest.version),
        )?))
    }

    /// Import a folder's ordinary files, recursing into its folders, as
    /// [`Self::files_import`] does; returns what each produced.
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
        let mut imported = Vec::new();
        let mut entries: Vec<_> = std::fs::read_dir(path)
            .map_err(io)?
            .collect::<std::io::Result<_>>()
            .map_err(io)?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let file_type = entry.file_type().map_err(io)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            // Finder's and Explorer's bookkeeping files are not content.
            if matches!(name, ".DS_Store" | "Thumbs.db" | "desktop.ini") || name.starts_with("._") {
                continue;
            }
            let child = entry.path();
            if file_type.is_dir() {
                let sub = self.files_mkdir(folder, name)?;
                imported.extend(self.files_import_dir(&child, &sub.id, keep_both, progress)?);
            } else if file_type.is_file() {
                let mut report = |done, total| progress(&child, done, total);
                imported.push(self.files_import(&child, folder, keep_both, &mut report)?);
            }
        }
        Ok(imported)
    }
}
