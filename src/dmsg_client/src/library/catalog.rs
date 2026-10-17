//! The library's catalog: folders, files and versions, each a record sealed
//! under the local data key (`workspace.put_private`, so the device ID and
//! the record name bind it), with an in-memory index of the folders and
//! files while the device is unlocked. Nothing about the catalog is
//! written in plain text: not a name, a tag, a note or a size.

use crate::files::{FileChunk, FileManifest, MAX_FILE};
use crate::upload::ChunkRef;
use crate::workspace::{Keys, Workspace};
use dmsg_types::{ensure, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

/// The root folder; it has no record.
pub const ROOT: &str = "root";
pub(crate) const FOLDER: &str = "lib-folder:";
pub(crate) const FILE: &str = "lib-file:";
pub(crate) const VERSION: &str = "lib-version:";

/// Most tags on a file.
pub const MAX_TAGS: usize = 20;
/// Longest note.
pub const MAX_NOTE: usize = 2000;

/// A folder; moving it changes its parent.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Folder {
    /// The parent folder, [`ROOT`] at the top.
    pub parent: String,
    /// Name, 1 to 255 characters.
    pub name: String,
    /// Unix milliseconds.
    pub created_at: u64,
    /// Unix milliseconds.
    pub updated_at: u64,
    /// When it was moved to the trash, with everything in it.
    pub trashed_at: Option<u64>,
}

/// A file: its place, its metadata and the IDs of its versions.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileRecord {
    /// The folder.
    pub folder: String,
    /// Name, 1 to 255 characters.
    pub name: String,
    /// Media type, sniffed from the content.
    pub mime: String,
    /// Up to [`MAX_TAGS`] tags.
    pub tags: Vec<String>,
    /// A note of up to [`MAX_NOTE`] characters.
    pub note: String,
    /// Starred.
    pub starred: bool,
    /// Unix milliseconds.
    pub created_at: u64,
    /// Unix milliseconds.
    pub updated_at: u64,
    /// The version the file reads as; the newest unless chosen otherwise.
    pub current: String,
    /// Every version, oldest first.
    pub versions: Vec<VersionRef>,
    /// When it was moved to the trash.
    pub trashed_at: Option<u64>,
}

/// What a file record knows of a version without opening it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VersionRef {
    /// Version ID.
    pub id: String,
    /// Plaintext length.
    pub size: u64,
    /// Plaintext SHA-256, hex.
    pub sha256: String,
    /// Unix milliseconds.
    pub created_at: u64,
}

/// Where a version came from.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Origin {
    /// Imported from a file on this device.
    Imported,
    /// Saved from an opened copy that was changed, or written by an agent.
    Edited,
    /// Fetched from a cloud backup.
    Fetched {
        /// The backup upload.
        upload_id: String,
    },
    /// Written through the mounted disk.
    Mounted,
    /// Saved from a channel message.
    SharedIn {
        /// The channel.
        channel: String,
        /// The message's relay sequence.
        seq: u64,
    },
}

/// A version's cloud backup.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Backup {
    /// The committed `backup` upload.
    pub upload_id: String,
    /// The content-root generation its manifest is sealed under.
    pub generation: u64,
    /// Unix milliseconds.
    pub backed_up_at: u64,
    /// Every chunk was read back and checked.
    pub verified: bool,
}

/// One version of a file: its key and its chunks.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Version {
    /// Plaintext length.
    pub size: u64,
    /// Plaintext SHA-256, hex.
    pub sha256: String,
    /// Unix milliseconds.
    pub created_at: u64,
    /// The device that made it, hex.
    pub device: String,
    /// Where it came from.
    pub origin: Origin,
    /// The file key, base64url.
    pub key: String,
    /// The sealed chunks, in order.
    pub chunks: Vec<ChunkRef>,
    /// The sealed thumbnail's digest, for images.
    pub thumbnail: Option<String>,
    /// The chunks are in this device's blob directory.
    pub local: bool,
    /// Its cloud backup, if any.
    pub backup: Option<Backup>,
}

impl Version {
    /// The version's file manifest, key included.
    pub(crate) fn manifest(
        &self,
        file_id: &str,
        version_id: &str,
        record: &FileRecord,
    ) -> FileManifest {
        FileManifest {
            id: file_id.into(),
            version: version_id.into(),
            name: record.name.clone(),
            mime: record.mime.clone(),
            size: self.size,
            sha256: self.sha256.clone(),
            chunks: self
                .chunks
                .iter()
                .enumerate()
                .map(|(index, chunk)| FileChunk {
                    id: format!("{file_id}:{version_id}:{index}"),
                    digest: chunk.digest.clone(),
                    size: chunk.size,
                })
                .collect(),
            key: self.key.clone(),
        }
    }

    /// `local`, `backed_up`, `cloud_only` or `too_large` (for the cloud).
    pub fn status(&self) -> &'static str {
        match (&self.backup, self.local) {
            (Some(_), false) => "cloud_only",
            (Some(_), true) => "backed_up",
            (None, _) if self.size > MAX_FILE => "too_large",
            (None, _) => "local",
        }
    }
}

/// A folder as it leaves the daemon.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct FolderView {
    /// Folder ID.
    pub id: String,
    /// Parent folder ID.
    pub parent: String,
    /// Name.
    pub name: String,
    /// Unix milliseconds.
    pub created_at: u64,
    /// Unix milliseconds.
    pub updated_at: u64,
    /// When it was trashed.
    pub trashed_at: Option<u64>,
    /// The names of the folders above it, from the root.
    pub path: Vec<String>,
}

/// A file as it leaves the daemon: no key, no chunk list.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct FileView {
    /// File ID.
    pub id: String,
    /// Folder ID.
    pub folder: String,
    /// Name.
    pub name: String,
    /// Media type.
    pub mime: String,
    /// The current version's length.
    pub size: u64,
    /// The current version's SHA-256.
    pub sha256: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Note.
    pub note: String,
    /// Starred.
    pub starred: bool,
    /// Unix milliseconds.
    pub created_at: u64,
    /// Unix milliseconds.
    pub updated_at: u64,
    /// The current version.
    pub current: String,
    /// How many versions.
    pub versions: usize,
    /// When it was trashed.
    pub trashed_at: Option<u64>,
    /// The current version's status.
    pub status: &'static str,
    /// The current version's backup.
    pub backup: Option<Backup>,
    /// The names of its folders, from the root.
    pub path: Vec<String>,
}

/// A version as it leaves the daemon.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct VersionView {
    /// Version ID.
    pub id: String,
    /// Length.
    pub size: u64,
    /// SHA-256.
    pub sha256: String,
    /// Unix milliseconds.
    pub created_at: u64,
    /// The device that made it.
    pub device: String,
    /// Where it came from.
    pub origin: Origin,
    /// Its chunks are on this device.
    pub local: bool,
    /// Its backup.
    pub backup: Option<Backup>,
    /// It is the file's current version.
    pub current: bool,
    /// Its status.
    pub status: &'static str,
}

/// The unlocked index of folders and files. The maps are read directly but
/// changed only through [`Catalog::put_file`] and its siblings, which keep
/// the name indexes in step.
pub(crate) struct Catalog {
    pub(crate) folders: BTreeMap<String, Folder>,
    pub(crate) files: BTreeMap<String, FileRecord>,
    /// The files of each folder, trashed ones included, by name and ID.
    file_names: BTreeMap<String, BTreeSet<(String, String)>>,
    /// The folders under each folder, by name and ID.
    folder_names: BTreeMap<String, BTreeSet<(String, String)>>,
    /// Version records read so far, by file and version.
    versions: Mutex<BTreeMap<(String, String), Version>>,
}

/// Check a folder or file name.
///
/// # Errors
/// Returns `Error::InvalidInput` for an empty name, one over 255
/// characters, a path separator or a control character.
pub fn validate_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.chars().count() <= 255
        && name != "."
        && name != ".."
        && !name
            .chars()
            .any(|c| c == '/' || c == '\\' || c == '\0' || c.is_control());
    dmsg_types::ensure_valid(ok, "name")
}

/// A name from outside (a shared file, a mounted write) made acceptable:
/// separators and control characters replaced, clipped to 255 characters,
/// `file` when nothing is left.
pub fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .take(255)
        .collect();
    match cleaned.trim() {
        "" | "." | ".." => "file".into(),
        trimmed => trimmed.into(),
    }
}

/// Check a file's tags.
///
/// # Errors
/// Returns `Error::InvalidInput` beyond [`MAX_TAGS`] tags or 40 characters.
pub fn validate_tags(tags: &[String]) -> Result<()> {
    dmsg_types::ensure_valid(
        tags.len() <= MAX_TAGS
            && tags
                .iter()
                .all(|t| !t.trim().is_empty() && t.chars().count() <= 40),
        "tags",
    )
}

impl Catalog {
    /// Read every folder and file record.
    pub(crate) fn load(workspace: &Workspace, keys: &Keys) -> Result<Self> {
        let mut catalog = Self {
            folders: BTreeMap::new(),
            files: BTreeMap::new(),
            file_names: BTreeMap::new(),
            folder_names: BTreeMap::new(),
            versions: Mutex::new(BTreeMap::new()),
        };
        for key in workspace.store.sealed_keys(FOLDER)? {
            if let Some(folder) = workspace.private::<Folder>(keys, &key)? {
                catalog.put_folder(&key[FOLDER.len()..], folder);
            }
        }
        for key in workspace.store.sealed_keys(FILE)? {
            if let Some(file) = workspace.private::<FileRecord>(keys, &key)? {
                catalog.put_file(&key[FILE.len()..], file);
            }
        }
        Ok(catalog)
    }

    /// Add or replace a folder.
    pub(crate) fn put_folder(&mut self, id: &str, folder: Folder) {
        self.drop_folder(id);
        self.folder_names
            .entry(folder.parent.clone())
            .or_default()
            .insert((folder.name.clone(), id.into()));
        self.folders.insert(id.into(), folder);
    }

    /// Forget a folder.
    pub(crate) fn drop_folder(&mut self, id: &str) {
        if let Some(old) = self.folders.remove(id)
            && let Some(names) = self.folder_names.get_mut(&old.parent)
        {
            names.remove(&(old.name, id.into()));
        }
    }

    /// Add or replace a file.
    pub(crate) fn put_file(&mut self, id: &str, file: FileRecord) {
        if let Some(old) = self.files.remove(id)
            && let Some(names) = self.file_names.get_mut(&old.folder)
        {
            names.remove(&(old.name, id.into()));
        }
        self.file_names
            .entry(file.folder.clone())
            .or_default()
            .insert((file.name.clone(), id.into()));
        self.files.insert(id.into(), file);
    }

    /// Forget a file and its versions.
    pub(crate) fn drop_file(&mut self, id: &str) {
        if let Some(old) = self.files.remove(id) {
            if let Some(names) = self.file_names.get_mut(&old.folder) {
                names.remove(&(old.name, id.into()));
            }
            let mut versions = self.versions.lock().unwrap_or_else(|e| e.into_inner());
            for version in old.versions {
                versions.remove(&(id.into(), version.id));
            }
        }
    }

    /// A version record read before.
    pub(crate) fn cached_version(&self, file: &str, version: &str) -> Option<Version> {
        self.versions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(file.into(), version.into()))
            .cloned()
    }

    /// Remember a version record as stored, or forget it with `None`.
    pub(crate) fn cache_version(&self, file: &str, version: &str, record: Option<&Version>) {
        let mut versions = self.versions.lock().unwrap_or_else(|e| e.into_inner());
        let key = (file.to_string(), version.to_string());
        match record {
            Some(record) => versions.insert(key, record.clone()),
            None => versions.remove(&key),
        };
    }

    /// Whether `folder` exists: the root, or a known folder.
    pub(crate) fn has_folder(&self, folder: &str) -> bool {
        folder == ROOT || self.folders.contains_key(folder)
    }

    /// A known folder that is not in the trash, by itself or through a parent.
    pub(crate) fn live_folder(&self, folder: &str) -> Result<()> {
        ensure(
            self.has_folder(folder) && !self.folder_trashed(folder),
            Error::NotFound,
        )
    }

    /// Whether a folder, or one above it, is in the trash.
    pub(crate) fn folder_trashed(&self, folder: &str) -> bool {
        let mut cursor = folder;
        let mut steps = 0;
        while let Some(found) = self.folders.get(cursor) {
            if found.trashed_at.is_some() {
                return true;
            }
            cursor = &found.parent;
            steps += 1;
            if steps > self.folders.len() {
                return true;
            }
        }
        false
    }

    /// Whether a file is in the trash, by itself or through its folder.
    pub(crate) fn file_trashed(&self, file: &FileRecord) -> bool {
        file.trashed_at.is_some() || self.folder_trashed(&file.folder)
    }

    /// The names of the folders down to `folder`, root excluded.
    pub(crate) fn path(&self, folder: &str) -> Vec<String> {
        let mut names = Vec::new();
        let mut cursor = folder;
        while let Some(found) = self.folders.get(cursor) {
            names.push(found.name.clone());
            cursor = &found.parent;
            if names.len() > self.folders.len() {
                break;
            }
        }
        names.reverse();
        names
    }

    /// Whether `ancestor` is `folder` or above it.
    pub(crate) fn within(&self, folder: &str, ancestor: &str) -> bool {
        let mut cursor = folder;
        let mut steps = 0;
        loop {
            if cursor == ancestor {
                return true;
            }
            match self.folders.get(cursor) {
                Some(found) if steps <= self.folders.len() => {
                    cursor = &found.parent;
                    steps += 1;
                }
                _ => return false,
            }
        }
    }

    /// The folders directly under `parent`, by name.
    pub(crate) fn children(&self, parent: &str) -> Vec<String> {
        self.folder_names
            .get(parent)
            .into_iter()
            .flatten()
            .map(|(_, id)| id.clone())
            .collect()
    }

    /// `folder` and every folder under it.
    pub(crate) fn subtree(&self, folder: &str) -> Vec<String> {
        let mut found = vec![folder.to_string()];
        let mut index = 0;
        while index < found.len() {
            let parent = found[index].clone();
            found.extend(self.children(&parent));
            index += 1;
        }
        found
    }

    /// The files directly in `folder`, by name.
    pub(crate) fn files_in(&self, folder: &str) -> Vec<String> {
        self.file_names
            .get(folder)
            .into_iter()
            .flatten()
            .map(|(_, id)| id.clone())
            .collect()
    }

    /// A file of this name in `folder` that is not in the trash.
    pub(crate) fn find(&self, folder: &str, name: &str) -> Option<String> {
        named(&self.file_names, folder, name)
            .find(|id| self.files[*id].trashed_at.is_none())
            .cloned()
    }

    /// A folder of this name under `parent` that is not in the trash.
    pub(crate) fn find_folder(&self, parent: &str, name: &str) -> Option<String> {
        named(&self.folder_names, parent, name)
            .find(|id| self.folders[*id].trashed_at.is_none())
            .cloned()
    }

    /// A name free in `folder`: `name`, else `name (2)`, `name (3)`, …
    pub(crate) fn free_name(&self, folder: &str, name: &str) -> String {
        if self.find(folder, name).is_none() {
            return name.to_string();
        }
        let (stem, extension) = match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
            _ => (name, String::new()),
        };
        (2..)
            .map(|n| format!("{stem} ({n}){extension}"))
            .find(|candidate| self.find(folder, candidate).is_none())
            .expect("an unbounded sequence has a free name")
    }

    /// A folder view.
    pub(crate) fn folder_view(&self, id: &str, folder: &Folder) -> FolderView {
        FolderView {
            id: id.into(),
            parent: folder.parent.clone(),
            name: folder.name.clone(),
            created_at: folder.created_at,
            updated_at: folder.updated_at,
            trashed_at: folder.trashed_at,
            path: self.path(&folder.parent),
        }
    }

    /// A file view with its current version's status.
    pub(crate) fn file_view(&self, id: &str, file: &FileRecord, current: &Version) -> FileView {
        FileView {
            id: id.into(),
            folder: file.folder.clone(),
            name: file.name.clone(),
            mime: file.mime.clone(),
            size: current.size,
            sha256: current.sha256.clone(),
            tags: file.tags.clone(),
            note: file.note.clone(),
            starred: file.starred,
            created_at: file.created_at,
            updated_at: file.updated_at,
            current: file.current.clone(),
            versions: file.versions.len(),
            trashed_at: file.trashed_at,
            status: current.status(),
            backup: current.backup.clone(),
            path: self.path(&file.folder),
        }
    }
}

/// The IDs of the entries named `name` in `parent` of a name index.
fn named<'a>(
    index: &'a BTreeMap<String, BTreeSet<(String, String)>>,
    parent: &str,
    name: &'a str,
) -> impl Iterator<Item = &'a String> {
    index
        .get(parent)
        .into_iter()
        .flat_map(move |names| names.range((name.to_string(), String::new())..))
        .take_while(move |(n, _)| n == name)
        .map(|(_, id)| id)
}

/// A version view.
pub(crate) fn version_view(id: &str, version: &Version, current: bool) -> VersionView {
    VersionView {
        id: id.into(),
        size: version.size,
        sha256: version.sha256.clone(),
        created_at: version.created_at,
        device: version.device.clone(),
        origin: version.origin.clone(),
        local: version.local,
        backup: version.backup.clone(),
        current,
        status: version.status(),
    }
}
