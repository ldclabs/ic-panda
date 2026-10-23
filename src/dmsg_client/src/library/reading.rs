//! Full-file reads in bounded client steps. The immutable version is pinned
//! at the start; each chunk checks that the same unlocked session remains.

use super::{
    Client, Head, MAX_PREVIEW_IMAGE, MAX_PREVIEW_TEXT_FILE, PREVIEW_TEXT, Preview, VerifyReport,
    VersionCheck, is_text,
};
use crate::codec::b64;
use crate::files::{FileManifest, chunk_count, io, read_chunk};
use crate::hold::Hold;
use dmsg_types::{Error, Result, ensure, ensure_valid};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use zeroize::{Zeroize, Zeroizing};

struct Reading {
    manifest: FileManifest,
    generation: u64,
    max: u64,
}

impl Drop for Reading {
    fn drop(&mut self) {
        self.manifest.key.zeroize();
    }
}

impl Reading {
    fn prepare(c: &mut Client, id: &str, version: Option<&str>) -> Result<Self> {
        let (_, _, manifest) = c.library_manifest(id, version)?;
        Ok(Self {
            manifest,
            generation: c.session_generation,
            max: c.config.files_max_bytes,
        })
    }

    fn check(&self, c: &Client) -> Result<()> {
        c.keys()?;
        ensure(c.session_generation == self.generation, Error::Locked)
    }

    async fn read<H: Hold>(&self, h: &H, out: &mut (dyn Write + Send)) -> Result<()> {
        let count = chunk_count(self.manifest.size);
        ensure(
            self.manifest.size <= self.max && self.manifest.chunks.len() as u64 == count,
            Error::IntegrityFailed,
        )?;
        let mut digest = Sha256::new();
        for index in 0..count {
            let plain = h
                .with(|c| {
                    self.check(c)?;
                    read_chunk(c.blobs()?, &self.manifest, index)
                })
                .await?;
            digest.update(plain.as_slice());
            out.write_all(&plain).map_err(io)?;
        }
        h.with(|c| self.check(c)).await?;
        ensure(
            hex::encode(digest.finalize()) == self.manifest.sha256,
            Error::IntegrityFailed,
        )
    }
}

/// Remove this operation's partial output on failure or cancellation.
struct Partial(PathBuf);

impl Drop for Partial {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Verify freshly downloaded chunks before a version joins the local catalog.
pub(crate) async fn check_manifest<H: Hold>(
    h: &H,
    manifest: &FileManifest,
    max: u64,
) -> Result<()> {
    let generation = h
        .with(|c| {
            c.keys()?;
            Ok(c.session_generation)
        })
        .await?;
    let reading = Reading {
        manifest: manifest.clone(),
        generation,
        max,
    };
    reading.read(h, &mut std::io::sink()).await
}

/// Verify and export one immutable version, releasing the client between chunks.
/// # Errors
/// As [`Client::files_export`]; locking stops the export and removes its partial output.
pub async fn export<H: Hold>(
    h: &H,
    id: &str,
    version: Option<&str>,
    out: &Path,
) -> Result<FileManifest> {
    ensure_valid(out.is_absolute() && !out.exists(), "out")?;
    let reading = h.with(|c| Reading::prepare(c, id, version)).await?;
    let name = out
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| dmsg_types::invalid("out"))?;
    let partial = out.with_file_name(format!(".{name}.dmsg-part"));
    let mut file = super::blobs::create_private_file(&partial)?;
    let partial = Partial(partial);
    reading.read(h, &mut file).await?;
    file.sync_all().map_err(io)?;
    h.with(|c| {
        reading.check(c)?;
        ensure_valid(!out.exists(), "out")?;
        std::fs::rename(&partial.0, out).map_err(io)?;
        Ok(reading.manifest.public())
    })
    .await
}

/// Build a preview with the same full-file integrity checks as the synchronous
/// reader, without holding the client for the entire file.
/// # Errors
/// As [`Client::files_preview`].
pub async fn preview<H: Hold>(h: &H, id: &str, version: Option<&str>) -> Result<Preview> {
    let (reading, thumbnail) = h
        .with(|c| {
            let (_, record, manifest) = c.library_manifest(id, version)?;
            let thumbnail = match record.thumbnail {
                Some(digest) if manifest.mime.starts_with("image/") => {
                    c.open_thumbnail(&manifest, &digest)?
                }
                _ => None,
            };
            Ok((
                Reading {
                    manifest,
                    generation: c.session_generation,
                    max: c.config.files_max_bytes,
                },
                thumbnail,
            ))
        })
        .await?;
    let mut result = Preview {
        kind: "none",
        mime: reading.manifest.mime.clone(),
        size: reading.manifest.size,
        content_base64url: None,
        thumbnail: false,
    };
    if let Some(bytes) = thumbnail {
        result.kind = "image";
        result.mime = "image/png".into();
        result.content_base64url = Some(b64(&bytes));
        result.thumbnail = true;
    } else if result.mime.starts_with("image/") && result.size <= MAX_PREVIEW_IMAGE {
        let mut bytes = Zeroizing::new(Vec::with_capacity(result.size as usize));
        reading.read(h, &mut *bytes).await?;
        result.kind = "image";
        result.content_base64url = Some(b64(&bytes));
    } else if is_text(&result.mime) && result.size <= MAX_PREVIEW_TEXT_FILE {
        let mut head = Head {
            bytes: Zeroizing::new(Vec::new()),
            limit: PREVIEW_TEXT,
        };
        reading.read(h, &mut head).await?;
        result.kind = "text";
        result.content_base64url = Some(b64(&head.bytes));
    }
    h.with(|c| reading.check(c)).await?;
    Ok(result)
}

/// Verify a file's versions one chunk at a time.
/// # Errors
/// As [`Client::files_verify`].
pub async fn verify<H: Hold>(h: &H, id: &str, version: Option<&str>) -> Result<VerifyReport> {
    let targets = h
        .with(|c| {
            let (file, version_id, _) = c.pick_version(id, version)?;
            Ok(match version {
                Some(_) => vec![version_id],
                None => file.versions.into_iter().map(|v| v.id).collect(),
            })
        })
        .await?;
    let mut versions = Vec::new();
    for version in targets {
        let reading = h
            .with(|c| {
                let (_, record, _) = c.library_manifest(id, Some(&version))?;
                if !record.local {
                    return Ok(None);
                }
                Reading::prepare(c, id, Some(&version)).map(Some)
            })
            .await?;
        let Some(reading) = reading else { continue };
        let checked = reading.read(h, &mut std::io::sink()).await;
        h.with(|c| reading.check(c)).await?;
        versions.push(VersionCheck {
            file: id.into(),
            version,
            ok: checked.is_ok(),
            error: checked.err().as_ref().map(crate::error_code),
        });
    }
    Ok(VerifyReport {
        versions,
        blobs_checked: 0,
        blobs_bad: vec![],
    })
}

/// Scan disk usage outside the client lock; account metadata is read locally.
/// # Errors
/// As [`Client::files_usage`].
pub async fn usage<H: Hold>(h: &H) -> Result<super::Usage> {
    let (mut usage, blobs, generation) = h
        .with(|c| {
            Ok((
                c.usage_metadata()?,
                c.blobs()?.clone(),
                c.session_generation,
            ))
        })
        .await?;
    let entries = blobs.entries()?;
    usage.blobs = entries.len() as u64;
    usage.blob_bytes = entries.iter().map(|(_, size)| size).sum();
    h.with(|c| {
        c.keys()?;
        ensure(c.session_generation == generation, Error::Locked)
    })
    .await?;
    Ok(usage)
}

/// Verify the catalog's local versions in bounded steps after the ciphertext
/// scan, then record both results in the same account workspace.
/// # Errors
/// As [`Client::files_check_record`]; locking during plaintext verification stops it.
pub async fn verify_library<H: Hold>(
    h: &H,
    blobs_checked: u64,
    blobs_bad: Vec<String>,
) -> Result<(super::Check, Vec<VersionCheck>)> {
    let (ids, generation) = h
        .with(|c| {
            let ids = if c.keys.is_some() {
                c.catalog()?.files.keys().cloned().collect::<Vec<_>>()
            } else {
                vec![]
            };
            Ok((ids, c.session_generation))
        })
        .await?;
    let mut versions = Vec::new();
    for id in ids {
        versions.extend(verify(h, &id, None).await?.versions);
    }
    h.with(|c| {
        ensure(c.session_generation == generation, Error::Locked)?;
        let check = super::Check {
            at: crate::now_ms(),
            blobs_checked,
            blobs_bad,
            versions_checked: versions.len() as u64,
            versions_bad: versions.iter().filter(|v| !v.ok).cloned().collect(),
        };
        c.workspace.store.set_meta(super::CHECK, &check)?;
        Ok((check, versions))
    })
    .await
}
