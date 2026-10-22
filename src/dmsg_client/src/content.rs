//! Encrypted content objects (encryption design §6) and their cloud forms
//! (`docs/protocol/cloud_zh.md`, "R0 本地内容到云端的映射").
//!
//! Each version is an [`EncryptedObject`]: the payload sealed under a random
//! item key with `ContentAAD`, the item key sealed under the content root
//! with `WrapAAD`. Editing or deleting writes a new version whose parent is
//! the version it replaces; concurrent branches stay visible as conflict
//! copies, and resolving one is another write.
//!
//! On the wire a version is a `kind=vault` upload whose single chunk is the
//! object container and whose manifest is sealed under the root. A file is a
//! separate `kind=file` upload of its encrypted chunks, with the file
//! manifest, key included, inside the sealed upload manifest; a backup of
//! the file library is a `kind=backup` upload whose manifest carries a
//! `dmsg-backup/1` record around the file manifest.

use crate::client::Client;
use crate::codec::{canonical, decode, decode_value, sha256};
use crate::crypto::{Key, open, open_bytes, random_key, seal, seal_bytes};
use crate::files::FileManifest;
use crate::library::backup::{BACKUP, BACKUP_FORMAT, BackupManifest};
use crate::root::RootKeys;
use crate::store::{LOCAL, ObjectRow};
use crate::upload::{UploadPlan, random_hex};
use dmsg_types::{Error, Result, ensure};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Object kind of vault items, files included.
pub const VAULT: &str = "vault";
/// Largest encoded payload of an ordinary object.
pub const MAX_OBJECT: usize = 200_000;
/// Largest object container chunk.
pub(crate) const MAX_CONTAINER: usize = crate::cloud::MAX_CHUNK;
/// Largest sealed upload manifest.
pub(crate) const MAX_MANIFEST: usize = 65_536;

/// One encrypted object version.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EncryptedObject {
    /// `id:revision`.
    pub key: String,
    /// Item ID, hex.
    pub id: String,
    /// Revision ID, hex.
    pub revision: String,
    /// The revision this one replaces.
    pub parent: Option<String>,
    /// Account ID text.
    pub subject_id: String,
    /// Writing device, hex.
    pub device_id: String,
    /// Content-root generation of `key_envelope`.
    pub generation: u64,
    /// Object kind.
    pub kind: String,
    /// The payload under the item key, base64url COSE_Encrypt0.
    pub ciphertext: String,
    /// The item key under the root, base64url COSE_Encrypt0.
    pub key_envelope: String,
    /// `hex(SHA-256(CBOR([ContentAAD, ciphertext, key_envelope])))`.
    pub digest: String,
    /// A deletion.
    pub tombstone: bool,
    /// Written as a conflict copy; the store tracks the current flag.
    pub conflict: bool,
    /// Creation time in Unix milliseconds.
    pub created_at: u64,
}

type ContentAad<'a> = (
    &'static str,
    &'a str,
    &'a str,
    u64,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a Option<String>,
    bool,
);

impl EncryptedObject {
    /// `ContentAAD`.
    fn aad<'a>(&'a self, environment: &'a str) -> ContentAad<'a> {
        (
            "dmsg/content/1",
            environment,
            &self.subject_id,
            self.generation,
            &self.id,
            &self.revision,
            &self.device_id,
            &self.kind,
            &self.parent,
            self.tombstone,
        )
    }

    /// `WrapAAD` of the item-version key.
    fn wrap(&self) -> (&'static str, &str, &str, u64, &str, &'static str) {
        (
            "dmsg/wrap/1",
            &self.subject_id,
            &self.id,
            self.generation,
            &self.revision,
            "item-version",
        )
    }

    fn compute_digest(&self, environment: &str) -> String {
        hex::encode(*sha256(&canonical(&(
            self.aad(environment),
            &self.ciphertext,
            &self.key_envelope,
        ))))
    }

    /// Seal `payload` as a new version.
    #[allow(clippy::too_many_arguments)]
    pub fn seal(
        environment: &str,
        root: (u64, &Key),
        account: &str,
        device: &str,
        kind: &str,
        id: String,
        parent: Option<String>,
        tombstone: bool,
        payload: &[u8],
        now: u64,
    ) -> Self {
        let revision = random_hex();
        let mut record = Self {
            key: format!("{id}:{revision}"),
            id,
            revision,
            parent,
            subject_id: account.into(),
            device_id: device.into(),
            generation: root.0,
            kind: kind.into(),
            ciphertext: String::new(),
            key_envelope: String::new(),
            digest: String::new(),
            tombstone,
            conflict: false,
            created_at: now,
        };
        let key = Zeroizing::new(random_key());
        record.ciphertext = seal(&key, payload, &record.aad(environment));
        record.key_envelope = seal(root.1, key.as_slice(), &record.wrap());
        record.digest = record.compute_digest(environment);
        record
    }

    /// Check the digest and open the payload with the root of its generation.
    ///
    /// # Errors
    /// A changed field returns `Error::IntegrityFailed`; a generation this
    /// device has no root for returns `Error::RecoveryIncomplete`.
    pub fn open(&self, environment: &str, roots: &RootKeys) -> Result<Zeroizing<Vec<u8>>> {
        ensure(
            self.compute_digest(environment) == self.digest
                && self.key == format!("{}:{}", self.id, self.revision),
            Error::IntegrityFailed,
        )?;
        let root = roots
            .key(self.generation)
            .ok_or(Error::RecoveryIncomplete)?;
        let key = open(root, &self.key_envelope, &self.wrap())?;
        let key: Key = key
            .as_slice()
            .try_into()
            .map_err(|_| Error::IntegrityFailed)?;
        let payload = open(
            &Zeroizing::new(key),
            &self.ciphertext,
            &self.aad(environment),
        )?;
        ensure(payload.len() <= MAX_OBJECT, Error::QuotaExceeded)?;
        Ok(payload)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Container {
    format: String,
    record: EncryptedObject,
}

/// The cloud object container `{format: "dmsg-cloud-object/1", record}`.
pub fn object_container(record: &EncryptedObject) -> Vec<u8> {
    canonical(&Container {
        format: "dmsg-cloud-object/1".into(),
        record: record.clone(),
    })
}

/// Read an object container.
///
/// # Errors
/// Another format returns `Error::UnsupportedProtocol`.
pub fn read_container(bytes: &[u8]) -> Result<EncryptedObject> {
    let container: Container = decode(bytes, MAX_CONTAINER)?;
    ensure(
        container.format == "dmsg-cloud-object/1",
        Error::UnsupportedProtocol,
    )?;
    Ok(container.record)
}

/// The placeholder chunk of an empty file's upload, which needs one
/// non-empty chunk: `{format: "dmsg-cloud-empty-file/1"}`.
pub fn empty_file() -> Vec<u8> {
    #[derive(Serialize)]
    struct Empty {
        format: &'static str,
    }
    canonical(&Empty {
        format: "dmsg-cloud-empty-file/1",
    })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    account_id: String,
    upload_id: String,
    object_id: String,
    version_id: String,
    kind: String,
    root_generation: u64,
    chunks: Vec<crate::upload::ChunkRef>,
    /// By `kind`: a `file` upload's file manifest, a `backup` upload's
    /// `dmsg-backup/1` record, nothing for the others.
    content: Option<cbor2::Value>,
}

/// What an upload manifest carries besides its plan, to seal.
#[derive(Clone, Copy, Debug)]
pub enum ManifestContent<'a> {
    /// A vault object, the root or an avatar: nothing.
    None,
    /// A `file` upload: its file manifest, key included.
    File(&'a FileManifest),
    /// A `backup` upload: the library's backup record.
    Backup(&'a BackupManifest),
}

/// What an opened upload manifest carried.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Opened {
    /// Nothing.
    None,
    /// A `file` upload's file manifest.
    File(FileManifest),
    /// A `backup` upload's record.
    Backup(BackupManifest),
}

fn manifest_context<'a>(
    account: &'a str,
    plan: &'a UploadPlan,
) -> (
    &'static str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    u64,
) {
    (
        "dmsg/cloud-manifest/1",
        account,
        &plan.upload_id,
        &plan.object_id,
        &plan.version_id,
        &plan.kind,
        plan.root_generation,
    )
}

/// Seal an upload manifest with its content, and complete the plan's
/// manifest digest and size.
pub fn seal_manifest(
    root: &Key,
    account: &str,
    plan: &mut UploadPlan,
    content: ManifestContent<'_>,
) -> Vec<u8> {
    let value =
        |bytes: Vec<u8>| decode_value(&bytes, MAX_MANIFEST).expect("canonical CBOR decodes");
    let manifest = Manifest {
        format: "dmsg-cloud-manifest/1".into(),
        account_id: account.into(),
        upload_id: plan.upload_id.clone(),
        object_id: plan.object_id.clone(),
        version_id: plan.version_id.clone(),
        kind: plan.kind.clone(),
        root_generation: plan.root_generation,
        chunks: plan.chunks.clone(),
        content: match content {
            ManifestContent::None => None,
            ManifestContent::File(file) => Some(value(canonical(file))),
            ManifestContent::Backup(backup) => Some(value(canonical(backup))),
        },
    };
    let bytes = seal_bytes(
        root,
        &Zeroizing::new(canonical(&manifest)),
        &manifest_context(account, plan),
    );
    plan.manifest_digest = hex::encode(*sha256(&bytes));
    plan.manifest_size = bytes.len() as u64;
    bytes
}

/// Open an upload manifest, check it repeats its plan and return what it
/// carries for its kind.
///
/// # Errors
/// Any difference returns `Error::IntegrityFailed`.
pub fn open_manifest(root: &Key, account: &str, plan: &UploadPlan, bytes: &[u8]) -> Result<Opened> {
    ensure(
        bytes.len() as u64 == plan.manifest_size
            && hex::encode(*sha256(bytes)) == plan.manifest_digest,
        Error::IntegrityFailed,
    )?;
    let plain = open_bytes(root, bytes, &manifest_context(account, plan))?;
    let manifest: Manifest = decode(&plain, MAX_MANIFEST)?;
    ensure(
        manifest.format == "dmsg-cloud-manifest/1"
            && manifest.account_id == account
            && manifest.upload_id == plan.upload_id
            && manifest.object_id == plan.object_id
            && manifest.version_id == plan.version_id
            && manifest.kind == plan.kind
            && manifest.root_generation == plan.root_generation
            && manifest.chunks == plan.chunks,
        Error::IntegrityFailed,
    )?;
    match (plan.kind.as_str(), manifest.content) {
        ("file", Some(content)) => {
            let file: FileManifest = decode(&canonical(&content), MAX_MANIFEST)?;
            ensure(
                file.id == plan.object_id && file.version == plan.version_id,
                Error::IntegrityFailed,
            )?;
            Ok(Opened::File(file))
        }
        (BACKUP, Some(content)) => {
            let backup: BackupManifest = decode(&canonical(&content), MAX_MANIFEST)?;
            ensure(backup.format == BACKUP_FORMAT, Error::IntegrityFailed)?;
            Ok(Opened::Backup(backup))
        }
        ("file" | BACKUP, None) => Err(Error::IntegrityFailed),
        (_, content) => ensure(content.is_none(), Error::IntegrityFailed).map(|_| Opened::None),
    }
}

/// The stored record of an object row.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for a corrupt row.
pub(crate) fn record(row: &ObjectRow) -> Result<EncryptedObject> {
    decode(&row.frame, MAX_CONTAINER)
}

impl Client {
    pub(crate) fn environment(&self) -> &'static str {
        crate::config::environment_name(&self.config.environment)
    }

    /// The opened content roots.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::RecoveryIncomplete`
    /// before the device holds the content root.
    pub(crate) fn roots(&self) -> Result<&RootKeys> {
        self.keys()?;
        self.root.as_ref().ok_or(Error::RecoveryIncomplete)
    }

    /// The current version of an item and its record.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown item.
    pub(crate) fn current(&self, id: &str) -> Result<(ObjectRow, EncryptedObject)> {
        let revision = self.workspace.store.head(id)?.ok_or(Error::NotFound)?;
        let row = self
            .workspace
            .store
            .object(&format!("{id}:{revision}"))?
            .ok_or(Error::IntegrityFailed)?;
        let record = record(&row)?;
        Ok((row, record))
    }

    /// Seal and store a new version with its sync job. A version whose
    /// parent is no longer the item's head is a conflict copy: it is sent
    /// like any other version, and the head stays.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::QuotaExceeded` for
    /// an oversized payload.
    pub(crate) fn write(
        &mut self,
        kind: &str,
        id: String,
        parent: Option<String>,
        tombstone: bool,
        payload: &[u8],
        file: Option<&FileManifest>,
    ) -> Result<(EncryptedObject, bool)> {
        ensure(payload.len() <= MAX_OBJECT, Error::QuotaExceeded)?;
        let account = self.account()?.id.to_string();
        let roots = self.roots()?;
        let generation = roots.generation;
        let root = roots.key(generation).ok_or(Error::RecoveryIncomplete)?;
        let record = EncryptedObject::seal(
            self.environment(),
            (generation, root),
            &account,
            &hex::encode(*self.workspace.meta.device_id),
            kind,
            id,
            parent,
            tombstone,
            payload,
            crate::now_ms(),
        );
        let job = self.prepare_job(&record, file, record.created_at)?;
        let head = self.workspace.store.head(&record.id)?;
        let conflict = head != record.parent;
        let row = ObjectRow {
            key: record.key.clone(),
            item: record.id.clone(),
            revision: record.revision.clone(),
            kind: record.kind.clone(),
            frame: canonical(&record),
            state: LOCAL.into(),
            conflict,
            resolved: false,
            created_at: record.created_at,
        };
        self.workspace.store.atomic(|store| {
            store.put_object(&row)?;
            store.set_job(&row.key, &job)?;
            if !conflict {
                store.set_head(&row.item, &row.revision, kind)?;
            }
            Ok(())
        })?;
        Ok((record, conflict))
    }
}

#[cfg(test)]
#[path = "content_tests.rs"]
mod tests;
