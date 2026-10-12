//! The vault: encrypted items (encryption design §6) and their sync through
//! the relay (`docs/protocol/cloud_zh.md`).
//!
//! Each version is an [`EncryptedObject`]: the payload sealed under a random
//! item key with `ContentAAD`, the item key sealed under the content root
//! with `WrapAAD`. Sync uploads the object as `kind=vault` and signs a vault
//! revision; pulling reads a full export snapshot, verifies every signed
//! entry and ciphertext, and only then moves the item heads.

use crate::client::Client;
use crate::cloud::verify_command;
use crate::codec::{canonical, decode, sha256, unb64};
use crate::crypto::{open, random_key, seal, Key};
use crate::now_ms;
use crate::root::RootKeys;
use crate::store::{ObjectRow, LOCAL, SYNCED};
use crate::upload::{hex_id, random_hex, ChunkRef, UploadPlan};
use dmsg_types::{ensure, ensure_valid, Error, Hash, Result};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use serde_json::Value as Json;
use std::collections::{BTreeMap, BTreeSet};
use zeroize::Zeroizing;

/// Object kind of vault items.
pub const VAULT: &str = "vault";
/// Largest encoded payload of an ordinary object.
pub const MAX_OBJECT: usize = 200_000;
/// Largest object container chunk.
const MAX_CONTAINER: usize = crate::cloud::MAX_CHUNK;

/// A vault item's plaintext.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Item {
    /// `note`, `login`, `api` or `key`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Title, 1 to 200 characters after trimming.
    pub title: String,
    /// Up to 20 tags of at most 40 characters.
    pub tags: Vec<String>,
    /// Body text.
    pub body: String,
    /// Login name.
    pub username: String,
    /// Secret field; shown only on explicit request.
    pub secret: String,
    /// Related URL.
    pub url: String,
    /// Starred.
    pub favorite: bool,
    /// Creation time in Unix milliseconds.
    pub created_at: u64,
    /// Last edit in Unix milliseconds.
    pub updated_at: u64,
    /// Conflict copies this version resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_conflicts: Option<Vec<String>>,
}

impl Item {
    /// Check the item limits of the vault.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` naming the field, or
    /// `Error::QuotaExceeded` when body and secret exceed 32 KiB.
    pub fn validate(&self) -> Result<()> {
        let chars = |s: &str| s.chars().count();
        let title = self.title.trim();
        ensure_valid(
            matches!(self.kind.as_str(), "note" | "login" | "api" | "key"),
            "type",
        )?;
        ensure_valid(!title.is_empty() && chars(title) <= 200, "title")?;
        ensure_valid(
            self.tags.len() <= 20 && self.tags.iter().all(|t| chars(t.trim()) <= 40),
            "tags",
        )?;
        ensure_valid(chars(&self.body) <= 20_000, "body")?;
        ensure_valid(chars(&self.username) <= 512, "username")?;
        ensure_valid(chars(&self.secret) <= 10_000, "secret")?;
        ensure_valid(chars(&self.url) <= 2048, "url")?;
        ensure(
            self.body.len() + self.secret.len() <= 32 * 1024,
            Error::QuotaExceeded,
        )
    }
}

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
    /// Kept as a conflict copy.
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
    chunks: Vec<ChunkRef>,
    content: Option<()>,
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

/// Seal a vault object's manifest and complete the plan's manifest digest and size.
pub fn seal_manifest(root: &Key, account: &str, plan: &mut UploadPlan) -> Vec<u8> {
    let manifest = Manifest {
        format: "dmsg-cloud-manifest/1".into(),
        account_id: account.into(),
        upload_id: plan.upload_id.clone(),
        object_id: plan.object_id.clone(),
        version_id: plan.version_id.clone(),
        kind: plan.kind.clone(),
        root_generation: plan.root_generation,
        chunks: plan.chunks.clone(),
        content: None,
    };
    let sealed = seal(
        root,
        &canonical(&manifest),
        &manifest_context(account, plan),
    );
    let bytes = unb64(&sealed).expect("sealed text is base64url");
    plan.manifest_digest = hex::encode(*sha256(&bytes));
    plan.manifest_size = bytes.len() as u64;
    bytes
}

/// Open a vault manifest and check it repeats its plan.
///
/// # Errors
/// Any difference returns `Error::IntegrityFailed`.
pub fn open_manifest(root: &Key, account: &str, plan: &UploadPlan, bytes: &[u8]) -> Result<()> {
    ensure(
        bytes.len() as u64 == plan.manifest_size
            && hex::encode(*sha256(bytes)) == plan.manifest_digest,
        Error::IntegrityFailed,
    )?;
    let plain = open(
        root,
        &crate::codec::b64(bytes),
        &manifest_context(account, plan),
    )?;
    let manifest: Manifest = decode(&plain, 65_536)?;
    ensure(
        manifest.format == "dmsg-cloud-manifest/1"
            && manifest.account_id == account
            && manifest.upload_id == plan.upload_id
            && manifest.object_id == plan.object_id
            && manifest.version_id == plan.version_id
            && manifest.kind == plan.kind
            && manifest.root_generation == plan.root_generation
            && manifest.chunks == plan.chunks
            && manifest.content.is_none(),
        Error::IntegrityFailed,
    )
}

/// A vault revision as the device signs it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Revision {
    item_id: String,
    revision_id: String,
    base_revision: Option<String>,
    upload_id: Option<String>,
    tombstone: bool,
    restore: bool,
}

#[derive(Serialize, Deserialize)]
struct ContentJob {
    record_digest: String,
    request_id: Hash,
    plan: UploadPlan,
    manifest: ByteBuf,
}

#[derive(Serialize)]
struct ExportStart<'a> {
    export_id: &'a str,
}

/// One item's current version.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct VaultEntry {
    /// Item ID.
    pub id: String,
    /// Current revision.
    pub revision: String,
    /// Plaintext.
    pub item: Item,
    /// The relay holds this version.
    pub synced: bool,
}

/// What one sync did.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Local versions the relay accepted.
    pub sent: usize,
    /// Revisions in the verified snapshot.
    pub revisions: usize,
    /// Snapshot position.
    pub through: u64,
}

impl Client {
    fn environment(&self) -> &'static str {
        crate::config::environment_name(&self.config.environment)
    }

    fn roots(&self) -> Result<&RootKeys> {
        self.keys()?;
        self.root.as_ref().ok_or(Error::RecoveryIncomplete)
    }

    fn record(row: &ObjectRow) -> Result<EncryptedObject> {
        decode(&row.frame, MAX_CONTAINER)
    }

    fn entry(&self, row: &ObjectRow) -> Result<VaultEntry> {
        let record = Self::record(row)?;
        let item: Item = decode(&record.open(self.environment(), self.roots()?)?, MAX_OBJECT)?;
        Ok(VaultEntry {
            id: record.id,
            revision: record.revision,
            item,
            synced: row.state == SYNCED,
        })
    }

    fn write(
        &mut self,
        kind: &str,
        id: String,
        parent: Option<String>,
        payload: &[u8],
    ) -> Result<EncryptedObject> {
        ensure(payload.len() <= MAX_OBJECT, Error::QuotaExceeded)?;
        let account = self.account()?.id.to_string();
        let roots = self.roots()?;
        let record = EncryptedObject::seal(
            self.environment(),
            (
                roots.generation,
                roots
                    .key(roots.generation)
                    .ok_or(Error::RecoveryIncomplete)?,
            ),
            &account,
            &hex::encode(*self.workspace.meta.device_id),
            kind,
            id,
            parent,
            false,
            payload,
            now_ms(),
        );
        let row = ObjectRow {
            key: record.key.clone(),
            item: record.id.clone(),
            revision: record.revision.clone(),
            kind: record.kind.clone(),
            frame: canonical(&record),
            state: LOCAL.into(),
            created_at: record.created_at,
        };
        self.workspace.store.atomic(|store| {
            store.put_object(&row)?;
            store.set_head(&row.item, &row.revision, kind)
        })?;
        Ok(record)
    }

    /// Add a new item, or a new version of `id` replacing its current head.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::RecoveryIncomplete`
    /// before the device holds the content root.
    pub fn vault_put(&mut self, id: Option<&str>, item: &Item) -> Result<VaultEntry> {
        item.validate()?;
        let (id, parent) = match id {
            Some(id) => {
                let head = self.workspace.store.head(id)?.ok_or(Error::NotFound)?;
                (id.to_string(), Some(head))
            }
            None => (random_hex(), None),
        };
        let record = self.write(VAULT, id, parent, &canonical(item))?;
        Ok(VaultEntry {
            id: record.id,
            revision: record.revision,
            item: item.clone(),
            synced: false,
        })
    }

    /// The current version of every item, by ID.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn vault_list(&self) -> Result<Vec<VaultEntry>> {
        self.roots()?;
        let mut entries = Vec::new();
        for (item, revision) in self.workspace.store.heads(VAULT)? {
            let row = self
                .workspace
                .store
                .object(&format!("{item}:{revision}"))?
                .ok_or(Error::IntegrityFailed)?;
            let record = Self::record(&row)?;
            if !record.tombstone {
                entries.push(self.entry(&row)?);
            }
        }
        Ok(entries)
    }

    /// The current version of one item.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown or deleted item.
    pub fn vault_show(&self, id: &str) -> Result<VaultEntry> {
        let revision = self.workspace.store.head(id)?.ok_or(Error::NotFound)?;
        let row = self
            .workspace
            .store
            .object(&format!("{id}:{revision}"))?
            .ok_or(Error::NotFound)?;
        ensure(!Self::record(&row)?.tombstone, Error::NotFound)?;
        self.entry(&row)
    }

    /// Read the relay snapshot, send local versions, and read it again when
    /// anything was sent.
    ///
    /// # Errors
    /// Returns `Error::RekeyRequired` while the account waits for a new root.
    pub async fn sync(&mut self) -> Result<SyncReport> {
        self.roots()?;
        let mut report = self.pull().await?;
        let sent = self.push().await?;
        if sent > 0 {
            report = self.pull().await?;
        }
        report.sent = sent;
        Ok(report)
    }

    async fn push(&mut self) -> Result<usize> {
        let rows = self.workspace.store.objects_in(LOCAL)?;
        let pending: BTreeSet<String> = rows.iter().map(|r| r.revision.clone()).collect();
        // Parents first: a version waits for the local version it replaces.
        let mut ordered: Vec<ObjectRow> = Vec::with_capacity(rows.len());
        let mut rest = rows;
        while !rest.is_empty() {
            let before = rest.len();
            let (ready, waiting): (Vec<_>, Vec<_>) = rest.into_iter().partition(|row| {
                let parent = Self::record(row).ok().and_then(|r| r.parent);
                parent.is_none_or(|p| {
                    !pending.contains(&p) || ordered.iter().any(|o| o.revision == p)
                })
            });
            ordered.extend(ready);
            rest = waiting;
            ensure(rest.len() < before, Error::IntegrityFailed)?;
        }
        for row in &ordered {
            self.push_one(row).await?;
        }
        Ok(ordered.len())
    }

    async fn push_one(&mut self, row: &ObjectRow) -> Result<()> {
        let mut record = Self::record(row)?;
        let generation = self.roots()?.generation;
        if record.generation != generation {
            // Sealed under an earlier root: reseal under the current one.
            let payload = record.open(self.environment(), self.roots()?)?;
            let replacement = self.write(
                &record.kind,
                record.id.clone(),
                record.parent.clone(),
                &payload,
            )?;
            self.workspace
                .store
                .set_object_state(&row.key, "replaced")?;
            record = replacement;
        }
        let account = self.account()?.id.to_string();
        let job_key = format!("content-job:{}", record.key);
        let job = match self
            .workspace
            .private::<ContentJob>(self.keys()?, &job_key)?
        {
            Some(job) => job,
            None => {
                let object = object_container(&record);
                let mut plan = UploadPlan {
                    upload_id: random_hex(),
                    object_id: record.id.clone(),
                    version_id: record.revision.clone(),
                    root_generation: generation,
                    epoch: 0,
                    control_head: None,
                    kind: VAULT.into(),
                    chunks: vec![ChunkRef {
                        digest: hex::encode(*sha256(&object)),
                        size: object.len() as u64,
                    }],
                    manifest_digest: String::new(),
                    manifest_size: 0,
                    expires_at: now_ms() + dmsg_types::DAY,
                };
                let root = self
                    .roots()?
                    .key(generation)
                    .ok_or(Error::RecoveryIncomplete)?;
                let manifest = seal_manifest(root, &account, &mut plan);
                let job = ContentJob {
                    record_digest: record.digest.clone(),
                    request_id: Hash::new(crate::crypto::random_id()),
                    plan,
                    manifest: ByteBuf::from(manifest),
                };
                self.workspace.put_private(self.keys()?, &job_key, &job)?;
                job
            }
        };
        ensure(
            job.record_digest == record.digest,
            Error::IdempotencyConflict,
        )?;
        let state = self.cloud_state().await?;
        ensure(
            state.info.vault_write_state == dmsg_types::user::VaultWriteState::Ready
                && state.info.current_root.as_ref().map(|r| r.generation) == Some(generation),
            Error::RekeyRequired,
        )?;
        let base = self.base()?;
        let request = hex::encode(*job.request_id);
        let stored = self
            .relay_find(&format!("{base}/operations/{request}"))
            .await?;
        let result = match stored.filter(|s| s["found"] == true) {
            Some(stored) => stored["result"].clone(),
            None => {
                let object = object_container(&record);
                self.upload(&job.plan, &[object], &job.manifest).await?;
                let revision = Revision {
                    item_id: record.id.clone(),
                    revision_id: record.revision.clone(),
                    base_revision: record.parent.clone(),
                    upload_id: Some(job.plan.upload_id.clone()),
                    tombstone: record.tombstone,
                    restore: false,
                };
                self.relay_post(
                    &format!("{base}/vault"),
                    "dmsg/vault/revision/v1",
                    &revision,
                    &job.request_id,
                )
                .await?
            }
        };
        ensure(
            result["revision_id"] == record.revision.as_str()
                && result["tombstone"] == record.tombstone
                && result["conflict"].is_boolean()
                && result["head"].as_str().is_some_and(|h| hex_id(h).is_ok()),
            Error::IntegrityFailed,
        )?;
        self.workspace.store.set_object_state(&record.key, SYNCED)?;
        self.workspace.delete_private(&job_key)
    }

    /// Verify the relay's full snapshot and adopt its committed versions.
    async fn pull(&mut self) -> Result<SyncReport> {
        let account = self.account()?.id.to_string();
        let base = self.base()?;
        let export_id = random_hex();
        let start = self
            .relay_post(
                &format!("{base}/exports"),
                "dmsg/export/start/v1",
                &ExportStart {
                    export_id: &export_id,
                },
                &Hash::new(crate::crypto::random_id()),
            )
            .await?;
        let through = start["through"].as_u64().ok_or(Error::IntegrityFailed)?;
        ensure(
            start["account"] == account.as_str() && start["scope"] == "cloud_snapshot",
            Error::IntegrityFailed,
        )?;
        let mut cursor = 0u64;
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        let mut records: BTreeMap<String, EncryptedObject> = BTreeMap::new();
        let mut revisions: Vec<(Revision, bool)> = Vec::new();
        loop {
            let page = self
                .relay_get(&format!(
                    "{base}/exports/{export_id}?after={cursor}&limit=100"
                ))
                .await?;
            let state = self.cloud_state().await?;
            let entries = page["entries"].as_array().ok_or(Error::IntegrityFailed)?;
            ensure(entries.len() <= 100, Error::IntegrityFailed)?;
            for entry in entries {
                let at = entry["cursor"].as_u64().ok_or(Error::IntegrityFailed)?;
                ensure(at > cursor && at <= through, Error::IntegrityFailed)?;
                cursor = at;
                let kind = entry["kind"].as_str().ok_or(Error::IntegrityFailed)?;
                *counts.entry(kind.into()).or_default() += 1;
                match kind {
                    "object" => {
                        if let Some(record) = self.pull_object(&state, &account, entry).await? {
                            ensure(
                                records
                                    .get(&record.key)
                                    .is_none_or(|r| r.digest == record.digest),
                                Error::IdempotencyConflict,
                            )?;
                            records.insert(record.key.clone(), record);
                        }
                    }
                    "revision" => {
                        let value = &entry["value"];
                        let revision: Revision = verify_command(
                            value["signed"]["cose_sign1"].as_str().unwrap_or_default(),
                            &state.info.issuer,
                            &state.info.devices,
                            "dmsg/vault/revision/v1",
                        )?;
                        let fields =
                            serde_json::to_value(&revision).map_err(|_| Error::IntegrityFailed)?;
                        for (name, field) in fields.as_object().into_iter().flatten() {
                            ensure(value[name] == *field, Error::IntegrityFailed)?;
                        }
                        let conflict = value["conflict"].as_bool().ok_or(Error::IntegrityFailed)?;
                        revisions.push((revision, conflict));
                    }
                    _ => {}
                }
            }
            let end = page["inventory_end"]
                .as_bool()
                .ok_or(Error::IntegrityFailed)?;
            ensure(
                page["next_cursor"].as_u64() == Some(cursor) && (end || !entries.is_empty()),
                Error::IntegrityFailed,
            )?;
            if end {
                break;
            }
        }
        for count in start["counts"].as_array().ok_or(Error::IntegrityFailed)? {
            let kind = count["kind"].as_str().ok_or(Error::IntegrityFailed)?;
            ensure(
                counts.get(kind).copied().unwrap_or(0) == count["n"].as_u64().unwrap_or(u64::MAX),
                Error::RecoveryIncomplete,
            )?;
        }
        let report = SyncReport {
            sent: 0,
            revisions: revisions.len(),
            through,
        };
        self.workspace.store.atomic(|store| {
            for (revision, conflict) in &revisions {
                let key = format!("{}:{}", revision.item_id, revision.revision_id);
                let record = records.get(&key).ok_or(Error::RecoveryIncomplete)?;
                ensure(
                    record.tombstone == revision.tombstone
                        && record.parent == revision.base_revision,
                    Error::RecoveryIncomplete,
                )?;
                let mut stored = record.clone();
                stored.conflict = *conflict;
                match store.object(&key)? {
                    Some(row) if Self::record(&row)?.digest == record.digest => {
                        store.set_object_state(&key, SYNCED)?
                    }
                    Some(_) => return Err(Error::IdempotencyConflict),
                    None => store.put_object(&ObjectRow {
                        key: key.clone(),
                        item: stored.id.clone(),
                        revision: stored.revision.clone(),
                        kind: stored.kind.clone(),
                        frame: canonical(&stored),
                        state: SYNCED.into(),
                        created_at: stored.created_at,
                    })?,
                }
                if !conflict {
                    store.set_head(&stored.id, &stored.revision, &stored.kind)?;
                }
            }
            Ok(())
        })?;
        Ok(report)
    }

    /// Verify one snapshot object; vault objects are downloaded and opened,
    /// others are only counted here.
    async fn pull_object(
        &mut self,
        state: &crate::account::AccountState,
        account: &str,
        entry: &Json,
    ) -> Result<Option<EncryptedObject>> {
        ensure(
            entry["missing_chunks"].as_array().is_none_or(Vec::is_empty),
            Error::RecoveryIncomplete,
        )?;
        let plan: UploadPlan =
            serde_json::from_value(entry["plan"].clone()).map_err(|_| Error::IntegrityFailed)?;
        let signed: UploadPlan = verify_command(
            entry["signature"]["cose_sign1"]
                .as_str()
                .unwrap_or_default(),
            &state.info.issuer,
            &state.info.devices,
            "dmsg/upload/reserve/v1",
        )?;
        ensure(
            signed == plan
                && entry["value"]["upload_id"] == plan.upload_id.as_str()
                && entry["value"]["digest"] == plan.manifest_digest.as_str(),
            Error::IntegrityFailed,
        )?;
        if plan.kind != VAULT {
            return Ok(None);
        }
        ensure(plan.chunks.len() == 1, Error::IntegrityFailed)?;
        let manifest = self
            .download(&plan.upload_id, "manifest", &plan.manifest_digest)
            .await?;
        let chunk = self
            .download(&plan.upload_id, "0", &plan.chunks[0].digest)
            .await?;
        let roots = self.roots()?;
        let root = roots
            .key(plan.root_generation)
            .ok_or(Error::RecoveryIncomplete)?;
        open_manifest(root, account, &plan, &manifest)?;
        let record = read_container(&chunk)?;
        ensure(
            record.subject_id == account
                && record.id == plan.object_id
                && record.revision == plan.version_id
                && record.kind == VAULT,
            Error::IntegrityFailed,
        )?;
        decode::<Item>(&record.open(self.environment(), roots)?, MAX_OBJECT)?;
        Ok(Some(record))
    }
}

#[cfg(test)]
#[path = "vault_tests.rs"]
mod tests;
