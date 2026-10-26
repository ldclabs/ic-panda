//! Opened copies: a version exported in plain text to
//! `open/<nonce>/<name>` (directory 0700, file 0600) for another program.
//! The copy is cleaned up after [`OPEN_TTL_MS`], on lock and on exit. A copy
//! the program changed is reported first (`files/open_changed`), so the
//! person can keep the change as a new version; until they decide it stays.

use super::Client;
use super::catalog::Origin;
use crate::files::{export_manifest, io};
use crate::now_ms;
use dmsg_types::{Error, MINUTE, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub(crate) const OPEN: &str = "lib-open:";
/// How long an unchanged copy stays.
pub const OPEN_TTL_MS: u64 = 30 * MINUTE;
/// The opened copies' directory in the workspace directory.
pub(crate) const OPEN_DIR: &str = "open";

/// An opened copy, sealed under the local data key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct OpenCopy {
    file: String,
    version: String,
    name: String,
    path: String,
    size: u64,
    sha256: String,
    opened_at: u64,
    /// The copy's bytes differ from the version's; reported once.
    changed: bool,
}

/// An opened copy as it leaves the daemon.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct OpenView {
    /// The copy's handle.
    pub nonce: String,
    /// File ID.
    pub file: String,
    /// Version ID.
    pub version: String,
    /// File name.
    pub name: String,
    /// The copy's absolute path.
    pub path: String,
    /// Unix milliseconds.
    pub opened_at: u64,
    /// The copy was changed since it was opened.
    pub changed: bool,
}

fn view(nonce: &str, copy: &OpenCopy) -> OpenView {
    OpenView {
        nonce: nonce.into(),
        file: copy.file.clone(),
        version: copy.version.clone(),
        name: copy.name.clone(),
        path: copy.path.clone(),
        opened_at: copy.opened_at,
        changed: copy.changed,
    }
}

/// The SHA-256 and length of a file on disk; `None` when it is gone.
fn digest_of(path: &str) -> Result<Option<(String, u64)>> {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io(e)),
    };
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        match std::io::Read::read(&mut file, &mut buffer).map_err(io)? {
            0 => break,
            n => {
                hasher.update(&buffer[..n]);
                size += n as u64;
            }
        }
    }
    Ok(Some((hex::encode(hasher.finalize()), size)))
}

/// Remove a copy's file and its directory.
fn remove_copy(path: &str) {
    let path = PathBuf::from(path);
    let _ = std::fs::remove_file(&path);
    remove_empty_dir(path.as_path());
}

/// Remove a copy's directory once nothing is left in it. A file another
/// program recreated meanwhile keeps it, so only an empty directory goes.
fn remove_empty_dir(path: &std::path::Path) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir(dir);
    }
}

impl Client {
    /// Native read-only previews are disposable even if another app changed a copy.
    pub fn use_read_only_previews(&mut self) {
        self.read_only_previews = true;
    }

    /// The opened copies' directory.
    fn open_dir(&self) -> Result<PathBuf> {
        Ok(self
            .blobs()?
            .dir()
            .parent()
            .ok_or(Error::AuthRequired)?
            .join(OPEN_DIR))
    }

    fn copies(&self) -> Result<Vec<(String, OpenCopy)>> {
        let mut copies = Vec::new();
        for key in self.workspace.store.sealed_keys(OPEN)? {
            if let Some(copy) = self.workspace.private::<OpenCopy>(self.keys()?, &key)? {
                copies.push((key[OPEN.len()..].to_string(), copy));
            }
        }
        Ok(copies)
    }

    fn save_copy(&self, nonce: &str, copy: &OpenCopy) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{OPEN}{nonce}"), copy)
    }

    /// Export a version to a fresh directory under `open/` for another
    /// program; returns where.
    ///
    /// # Errors
    /// As [`Self::files_export`].
    pub fn files_open(&mut self, id: &str, version: Option<&str>) -> Result<OpenView> {
        let (_, _, manifest) = self.library_manifest(id, version)?;
        let nonce = hex::encode(&crate::crypto::random_id()[..16]);
        let dir = self.open_dir()?.join(&nonce);
        super::blobs::create_private_dir(&dir)?;
        let path = dir.join(&manifest.name);
        let exported =
            export_manifest(self.blobs()?, &manifest, self.config.files_max_bytes, &path);
        if exported.is_err() {
            let _ = std::fs::remove_dir(&dir);
        }
        exported?;
        let copy = OpenCopy {
            file: id.into(),
            version: manifest.version.clone(),
            name: manifest.name.clone(),
            path: path.display().to_string(),
            size: manifest.size,
            sha256: manifest.sha256.clone(),
            opened_at: now_ms(),
            changed: false,
        };
        self.save_copy(&nonce, &copy)?;
        Ok(view(&nonce, &copy))
    }

    /// The opened copies.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn files_open_list(&mut self) -> Result<Vec<OpenView>> {
        Ok(self
            .copies()?
            .iter()
            .map(|(nonce, copy)| view(nonce, copy))
            .collect())
    }

    /// Look at every opened copy: one that is gone is forgotten, one whose
    /// bytes changed is marked and returned the first time, and an
    /// unchanged one past its time is removed.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn files_open_sweep(&mut self, now: u64) -> Result<Vec<OpenView>> {
        let mut changed = Vec::new();
        for (nonce, mut copy) in self.copies()? {
            let Some((digest, size)) = digest_of(&copy.path)? else {
                // The file is gone; its now-empty directory can go too.
                remove_empty_dir(std::path::Path::new(&copy.path));
                self.workspace.delete_private(&format!("{OPEN}{nonce}"))?;
                continue;
            };
            if digest != copy.sha256 || size != copy.size {
                if !copy.changed {
                    copy.changed = true;
                    self.save_copy(&nonce, &copy)?;
                    changed.push(view(&nonce, &copy));
                }
                continue;
            }
            if copy.changed {
                // Changed back to the version's bytes.
                copy.changed = false;
                self.save_copy(&nonce, &copy)?;
            }
            if copy.opened_at + OPEN_TTL_MS <= now {
                remove_copy(&copy.path);
                self.workspace.delete_private(&format!("{OPEN}{nonce}"))?;
            }
        }
        Ok(changed)
    }

    /// Remove an opened copy, or every one; with `save`, a changed copy is
    /// first imported as a new version of its file.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown nonce.
    pub fn files_open_cleanup(
        &mut self,
        nonce: Option<&str>,
        save: bool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Vec<OpenView>> {
        let copies = self.copies()?;
        if let Some(nonce) = nonce {
            ensure(copies.iter().any(|(n, _)| n == nonce), Error::NotFound)?;
        }
        let mut removed = Vec::new();
        for (found, copy) in copies {
            if nonce.is_some_and(|n| n != found) {
                continue;
            }
            let current = if save { digest_of(&copy.path)? } else { None };
            let changed = current
                .as_ref()
                .is_some_and(|(digest, size)| *digest != copy.sha256 || *size != copy.size);
            if save && changed {
                let (_, size) = current.expect("a changed copy exists");
                let file = self.file_record(&copy.file)?;
                let mut reader = std::fs::File::open(&copy.path).map_err(io)?;
                self.import_stream(
                    &mut reader,
                    size,
                    &file.name,
                    &file.folder,
                    Some(copy.file.clone()),
                    Origin::Edited,
                    progress,
                )?;
            }
            remove_copy(&copy.path);
            self.workspace.delete_private(&format!("{OPEN}{found}"))?;
            removed.push(view(&found, &copy));
        }
        Ok(removed)
    }

    /// On lock: remove the unchanged copies. A changed copy stays for the
    /// next unlock's sweep, which reports it.
    pub(crate) fn open_copies_on_lock(&mut self) -> Result<()> {
        if self.read_only_previews {
            self.files_open_cleanup(None, false, &mut |_, _| {})?;
            return Ok(());
        }
        for (nonce, copy) in self.copies()? {
            let unchanged = digest_of(&copy.path)?
                .is_none_or(|(digest, size)| digest == copy.sha256 && size == copy.size);
            if unchanged {
                remove_copy(&copy.path);
                self.workspace.delete_private(&format!("{OPEN}{nonce}"))?;
            }
        }
        Ok(())
    }
}

struct PendingCopy {
    path: PathBuf,
    published: bool,
}
impl Drop for PendingCopy {
    fn drop(&mut self) {
        if !self.published {
            remove_copy(&self.path.display().to_string());
        }
    }
}

/// Open a verified local copy without holding the client across the full read.
/// # Errors
/// As [`Client::files_open`]; locking cancels the operation.
pub async fn open<H: crate::hold::Hold>(
    h: &H,
    id: &str,
    version: Option<&str>,
) -> Result<OpenView> {
    let (nonce, path, version, generation) = h
        .with(|c| {
            let (_, _, manifest) = c.library_manifest(id, version)?;
            let nonce = hex::encode(&crate::crypto::random_id()[..16]);
            let dir = c.open_dir()?.join(&nonce);
            super::blobs::create_private_dir(&dir)?;
            Ok((
                nonce,
                dir.join(&manifest.name),
                manifest.version,
                c.session_generation,
            ))
        })
        .await?;
    let mut pending = PendingCopy {
        path: path.clone(),
        published: false,
    };
    let result = async {
        let manifest = super::reading::export(h, id, Some(&version), &path).await?;
        h.with(|c| {
            c.keys()?;
            ensure(c.session_generation == generation, Error::Locked)?;
            let copy = OpenCopy {
                file: id.into(),
                version: manifest.version,
                name: manifest.name,
                path: path.display().to_string(),
                size: manifest.size,
                sha256: manifest.sha256,
                opened_at: now_ms(),
                changed: false,
            };
            c.save_copy(&nonce, &copy)?;
            Ok(view(&nonce, &copy))
        })
        .await
    }
    .await;
    pending.published = result.is_ok();
    result
}

/// Check copies edited by other programs without holding the client during
/// their disk reads. A copy changed or removed by another operation is left alone.
/// # Errors
/// As [`Client::files_open_sweep`].
pub async fn sweep<H: crate::hold::Hold>(h: &H) -> Result<Vec<OpenView>> {
    let (copies, generation) = h.with(|c| Ok((c.copies()?, c.session_generation))).await?;
    let mut changed = Vec::new();
    for (nonce, mut copy) in copies {
        let current = digest_of(&copy.path)?;
        let update = h
            .with(|c| {
                c.keys()?;
                ensure(c.session_generation == generation, Error::Locked)?;
                let key = format!("{OPEN}{nonce}");
                let stored: Option<OpenCopy> = c.workspace.private(c.keys()?, &key)?;
                if stored.as_ref() != Some(&copy) {
                    return Ok(None);
                }
                let Some((digest, size)) = current else {
                    remove_empty_dir(std::path::Path::new(&copy.path));
                    c.workspace.delete_private(&key)?;
                    return Ok(None);
                };
                if digest != copy.sha256 || size != copy.size {
                    if !copy.changed {
                        copy.changed = true;
                        c.save_copy(&nonce, &copy)?;
                        return Ok(Some(view(&nonce, &copy)));
                    }
                } else {
                    if copy.changed {
                        copy.changed = false;
                        c.save_copy(&nonce, &copy)?;
                    }
                    if copy.opened_at + OPEN_TTL_MS <= now_ms() {
                        remove_copy(&copy.path);
                        c.workspace.delete_private(&key)?;
                    }
                }
                Ok(None)
            })
            .await?;
        changed.extend(update);
    }
    Ok(changed)
}
