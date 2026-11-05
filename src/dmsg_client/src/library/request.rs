//! File requests: a link that lets someone without an account upload files
//! to this account. It is the reverse of a share link: the fragment of
//! `<site>/r/<account>/<request_id>#<page_key>` gives the power to encrypt,
//! never to decrypt. `page_key` opens a descriptor holding the link's HPKE
//! public key, title and instructions, and derives the upload token the relay
//! checks; the HPKE seed stays in management sealed under the account's root.
//! A browser seals each file's manifest to that public key and its chunks by
//! the rules of `content-v1.json`. Formats: `request-v1.json`.

use super::catalog::{FileView, Origin, safe_name};
use super::import;
use crate::codec::{b64, canonical, decode, sha256, unb64};
use crate::content::MAX_BACKUP_MANIFEST;
use crate::crypto::{
    HpkeEnvelope, Key, derive, hpke_open, hpke_public, open, random_id, random_key, seal,
};
use crate::files::{BlobStore, FileManifest, MAX_BACKUP, chunk_count};
use crate::hold::{Hold, Unlocked};
use crate::upload::{ManagementEnvelope, UploadPlan, hex_id, random_hex};
use crate::{Client, now_ms};
use dmsg_types::{DAY, Error, Hash, MINUTE, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};
use serde_json::json;
use zeroize::Zeroizing;

/// The upload kind of a file a request received.
pub const REQUEST_FILE: &str = "request_file";
/// The plaintext format of a descriptor.
pub const REQUEST_FORMAT: &str = "dmsg-request/1";
/// The plaintext format of a received file's manifest.
pub const REQUEST_FILE_FORMAT: &str = "dmsg-request-file/1";
/// Largest sealed descriptor, in bytes; the relay enforces the same.
pub const MAX_DESCRIPTOR: usize = 8 * 1024;
/// Largest name and note a sender may write, together, in bytes.
pub const MAX_SENDER: usize = 1024;
/// Bytes of the encapsulated key in front of a sealed manifest.
const ENC: usize = 32;
/// Largest title, instructions and label, in bytes: together they fit a
/// descriptor and the management sealed for other devices.
const MAX_TITLE: usize = 512;
const MAX_INSTRUCTIONS: usize = 4096;
const MAX_LABEL: usize = 256;
const LIFETIME_MS: u64 = 14 * DAY;
const MAX_LIFETIME_MS: u64 = 90 * DAY;
const MIB: u64 = 1024 * 1024;
const MIN_TOTAL: u64 = MIB;
/// What a link takes in all unless asked otherwise: Max gets more.
const DEFAULT_TOTAL: u64 = 500 * MIB;
const DEFAULT_TOTAL_MAX: u64 = 2048 * MIB;
/// The single-file limit below Max, above which a membership is Max.
const PLAIN_FILE_LIMIT: u64 = 100 * MIB;
/// Downloaded chunks stay pinned this long past the last progress.
const HOLD_MS: u64 = 10 * MINUTE;
const RECORD: &str = "lib-request:";
const RECEIVED: &str = "lib-request-file:";

/// What the person with the link reads, sealed under the link's page key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RequestDescriptor {
    /// [`REQUEST_FORMAT`].
    pub format: String,
    /// The requester's account.
    pub account: String,
    /// The link.
    pub request_id: String,
    /// The link's X25519 public key, base64url.
    pub hpke_pub: String,
    /// What is asked for.
    pub title: String,
    /// More about it; may be empty.
    pub instructions: String,
}

/// Who a sender says they are: unverified, and shown as such.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RequestSender {
    /// Their name.
    pub name: String,
    /// Their note.
    pub note: String,
}

/// What a received file's manifest carries, sealed to the link's public key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RequestFile {
    /// [`REQUEST_FILE_FORMAT`].
    pub format: String,
    /// The link.
    pub request_id: String,
    /// The upload.
    pub upload_id: String,
    /// The file, with its key.
    pub file: FileManifest,
    /// Who the sender says they are.
    pub sender: Option<RequestSender>,
    /// The upload this one replaces, as the sender's page says.
    pub replaces: Option<String>,
    /// When the sender's browser sealed it, Unix milliseconds; unverified.
    pub sent_at: u64,
}

/// The AAD of a descriptor.
pub fn descriptor_aad<'a>(
    account: &'a str,
    request_id: &'a str,
) -> (&'static str, &'a str, &'a str) {
    ("dmsg/request-descriptor/1", account, request_id)
}

/// The HPKE info and AAD of a received file's manifest.
pub fn request_file_info<'a>(
    account: &'a str,
    request_id: &'a str,
    upload_id: &'a str,
    object_id: &'a str,
    version_id: &'a str,
) -> (&'static str, &'a str, &'a str, &'a str, &'a str, &'a str) {
    (
        "dmsg/request-file/1",
        account,
        request_id,
        upload_id,
        object_id,
        version_id,
    )
}

/// The token a link's page shows the relay.
pub fn upload_token(page_key: &Key, account: &str, request_id: &str) -> Zeroizing<Key> {
    derive(
        page_key,
        &("dmsg/request-upload-token/1", account, request_id),
    )
}

/// What the relay keeps of the token: its SHA-256, hex.
pub fn token_hash(token: &Key) -> String {
    hex::encode(*sha256(token))
}

/// Seal a descriptor under the link's page key, base64url.
pub fn seal_descriptor(page_key: &Key, descriptor: &RequestDescriptor) -> String {
    seal(
        page_key,
        &canonical(descriptor),
        &descriptor_aad(&descriptor.account, &descriptor.request_id),
    )
}

/// Open a descriptor and check it names this link.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for anything that does not verify.
pub fn open_descriptor(
    page_key: &Key,
    account: &str,
    request_id: &str,
    sealed: &str,
) -> Result<RequestDescriptor> {
    let plain = open(page_key, sealed, &descriptor_aad(account, request_id))?;
    let descriptor: RequestDescriptor = decode(&plain, MAX_DESCRIPTOR)?;
    ensure(
        descriptor.format == REQUEST_FORMAT
            && descriptor.account == account
            && descriptor.request_id == request_id,
        Error::IntegrityFailed,
    )?;
    Ok(descriptor)
}

/// Open a received file's manifest with the link's HPKE seed and check it
/// describes exactly `plan`, the upload the relay reports: only a holder of
/// the link can seal one, and the sender may be anyone, so nothing in it is
/// taken on trust.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for anything that does not verify.
pub fn open_request_file(
    seed: &Key,
    account: &str,
    request_id: &str,
    plan: &UploadPlan,
    sealed: &[u8],
) -> Result<RequestFile> {
    ensure(
        plan.kind == REQUEST_FILE
            && sealed.len() > ENC
            && sealed.len() as u64 == plan.manifest_size
            && hex::encode(*sha256(sealed)) == plan.manifest_digest,
        Error::IntegrityFailed,
    )?;
    let (enc, ciphertext) = sealed.split_at(ENC);
    let plain = hpke_open(
        seed,
        &HpkeEnvelope {
            enc: enc.to_vec(),
            ciphertext: ciphertext.to_vec(),
        },
        &request_file_info(
            account,
            request_id,
            &plan.upload_id,
            &plan.object_id,
            &plan.version_id,
        ),
    )?;
    let opened: RequestFile = decode(&plain, MAX_BACKUP_MANIFEST)?;
    let file = &opened.file;
    ensure(
        opened.format == REQUEST_FILE_FORMAT
            && opened.request_id == request_id
            && opened.upload_id == plan.upload_id
            && file.id == plan.object_id
            && file.version == plan.version_id
            && file.size > 0
            && file.size <= MAX_BACKUP
            && file.chunks.len() as u64 == chunk_count(file.size)
            && file.chunks.len() == plan.chunks.len()
            && file.chunks.iter().enumerate().all(|(i, chunk)| {
                chunk.id == format!("{}:{}:{i}", file.id, file.version)
                    && chunk.digest == plan.chunks[i].digest
                    && chunk.size
                        == (file.size - i as u64 * crate::files::CHUNK as u64)
                            .min(crate::files::CHUNK as u64)
            })
            && opened
                .sender
                .as_ref()
                .is_none_or(|s| s.name.len() + s.note.len() <= MAX_SENDER)
            && opened
                .replaces
                .as_deref()
                .is_none_or(|id| hex_id(id).is_ok()),
        Error::IntegrityFailed,
    )?;
    file.file_key().map_err(|_| Error::IntegrityFailed)?;
    Ok(opened)
}

/// The link of a request, with its page key in the fragment.
pub fn request_url(login_url: &str, account: &str, request_id: &str, page_key: &str) -> String {
    format!(
        "{}/r/{account}/{request_id}#{page_key}",
        super::link::site(login_url)
    )
}

/// What a link asks for. Only `label`, who it is for, stays with the requester.
#[derive(Clone, Copy, Debug)]
pub struct RequestDraft<'a> {
    /// What is asked for.
    pub title: &'a str,
    /// More about it; may be empty.
    pub instructions: &'a str,
    /// Who it is for; may be empty.
    pub label: &'a str,
}

/// A link's terms; `None` takes the defaults.
#[derive(Clone, Copy, Debug, Default)]
pub struct RequestTerms {
    /// How long it stays open.
    pub expires_in_ms: Option<u64>,
    /// How much it takes in all.
    pub max_total_bytes: Option<u64>,
}

/// What opens a link again on another device, sealed under the account's root.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RequestSecrets {
    hpke_seed: String,
    page_key: String,
    label: String,
    title: String,
    instructions: String,
}

/// A link as this device keeps it.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
struct RequestRecord {
    request_id: String,
    /// `None` while this device lacks the root its management is sealed under.
    secrets: Option<RequestSecrets>,
    expires_at: u64,
    max_total_bytes: u64,
    created_at: u64,
    revoked_at: Option<u64>,
    status: String,
    files: u64,
    bytes: u64,
    last_stored_at: Option<u64>,
    seen_at: Option<u64>,
    last_checked_at: Option<u64>,
}

/// A file a link received, as this device keeps it.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
struct ReceivedRecord {
    request_id: String,
    plan: UploadPlan,
    /// `None` for a manifest that does not open.
    file: Option<RequestFile>,
    stored_at: u64,
    received_at: Option<u64>,
    withdrawn_at: Option<u64>,
    /// The library file and version it was saved as.
    saved: Option<(String, String)>,
}

/// A link as it leaves the daemon.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct RequestView {
    /// The link's ID.
    pub request_id: String,
    /// The link; absent while this device lacks the root that opens it.
    pub url: Option<String>,
    /// What is asked for.
    pub title: Option<String>,
    /// More about it.
    pub instructions: Option<String>,
    /// Who it is for.
    pub label: Option<String>,
    /// Unix milliseconds.
    pub expires_at: u64,
    /// How much it takes in all.
    pub max_total_bytes: u64,
    /// Unix milliseconds.
    pub created_at: u64,
    /// Unix milliseconds.
    pub revoked_at: Option<u64>,
    /// `open`, `full`, `paused`, `expired` or `revoked`.
    pub status: String,
    /// Files in use, as the relay last reported.
    pub files: u64,
    /// Their bytes.
    pub bytes: u64,
    /// When the latest file arrived.
    pub last_stored_at: Option<u64>,
    /// A file arrived since this device last listed them.
    pub unread: bool,
    /// When this device last read the relay.
    pub last_checked_at: Option<u64>,
}

/// A received file as it leaves the daemon. Its name and sender are what the
/// sender wrote, nothing more.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct ReceivedView {
    /// The link.
    pub request_id: String,
    /// The upload.
    pub upload_id: String,
    /// `received`, `saved`, `withdrawn` or `unreadable`.
    pub state: &'static str,
    /// Its name, made safe.
    pub name: Option<String>,
    /// As the sender says.
    pub mime: Option<String>,
    /// Plaintext length.
    pub size: Option<u64>,
    /// Who the sender says they are.
    pub sender: Option<RequestSender>,
    /// The upload this one replaces, as the sender's page says.
    pub replaces: Option<String>,
    /// When the sender's browser sealed it, as it says.
    pub sent_at: Option<u64>,
    /// When the relay stored it.
    pub stored_at: u64,
    /// Bytes stored in the cloud.
    pub bytes: u64,
    /// When a device of this account saved it.
    pub received_at: Option<u64>,
    /// When the sender withdrew it.
    pub withdrawn_at: Option<u64>,
    /// The library file it was saved as, on this device.
    pub saved_file: Option<String>,
}

#[derive(Deserialize)]
struct RemoteLink {
    request_id: String,
    expires_at: u64,
    max_total_bytes: u64,
    management: ManagementEnvelope,
    created_at: u64,
    revoked_at: Option<u64>,
    status: String,
    files: u64,
    bytes: u64,
    last_stored_at: Option<u64>,
}

#[derive(Deserialize)]
struct RemoteFile {
    upload_id: String,
    plan: UploadPlan,
    status: String,
    stored_at: u64,
    received_at: Option<u64>,
    withdrawn_at: Option<u64>,
}

#[derive(Deserialize)]
struct Page<T> {
    entries: Vec<T>,
    next_cursor: u64,
    end: bool,
}

fn parse<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T> {
    serde_json::from_value(value).map_err(|_| Error::IntegrityFailed)
}

fn management_aad<'a>(
    account: &'a str,
    request_id: &'a str,
    generation: u64,
) -> (&'static str, &'a str, &'a str, u64) {
    (
        "dmsg/file-request-management/1",
        account,
        request_id,
        generation,
    )
}

fn check_draft(draft: &RequestDraft) -> Result<()> {
    ensure_valid(
        !draft.title.trim().is_empty() && draft.title.len() <= MAX_TITLE,
        "title",
    )?;
    ensure_valid(draft.instructions.len() <= MAX_INSTRUCTIONS, "instructions")?;
    ensure_valid(draft.label.len() <= MAX_LABEL, "label")
}

fn bytes_of(plan: &UploadPlan) -> u64 {
    plan.chunks.iter().map(|c| c.size).sum::<u64>() + plan.manifest_size
}

impl RequestRecord {
    fn status(&self, now: u64) -> &str {
        match self.revoked_at {
            Some(_) => "revoked",
            None if self.expires_at <= now => "expired",
            None => &self.status,
        }
    }
}

impl ReceivedRecord {
    fn view(&self) -> ReceivedView {
        let file = self.file.as_ref();
        ReceivedView {
            request_id: self.request_id.clone(),
            upload_id: self.plan.upload_id.clone(),
            state: match (&self.saved, self.withdrawn_at, file) {
                (Some(_), _, _) => "saved",
                (None, Some(_), _) => "withdrawn",
                (None, None, None) => "unreadable",
                (None, None, Some(_)) => "received",
            },
            name: file.map(|f| safe_name(&f.file.name)),
            mime: file.map(|f| f.file.mime.clone()),
            size: file.map(|f| f.file.size),
            sender: file.and_then(|f| f.sender.clone()),
            replaces: file.and_then(|f| f.replaces.clone()),
            sent_at: file.map(|f| f.sent_at),
            stored_at: self.stored_at,
            bytes: bytes_of(&self.plan),
            received_at: self.received_at,
            withdrawn_at: self.withdrawn_at,
            saved_file: self.saved.as_ref().map(|(file, _)| file.clone()),
        }
    }
}

impl Client {
    /// Where this account's links live on the relay.
    fn requests_base(&self) -> Result<String> {
        Ok(format!("/v1/file-requests/{}", self.account()?.id))
    }

    fn seal_request_secrets(
        &self,
        request_id: &str,
        secrets: &RequestSecrets,
    ) -> Result<ManagementEnvelope> {
        let account = self.account()?.id.to_string();
        let roots = self.roots()?;
        let generation = roots.generation;
        let root = roots.key(generation).ok_or(Error::RecoveryIncomplete)?;
        Ok(ManagementEnvelope {
            generation,
            ciphertext: seal(
                root,
                &canonical(secrets),
                &management_aad(&account, request_id, generation),
            ),
        })
    }

    fn open_request_secrets(
        &self,
        request_id: &str,
        envelope: &ManagementEnvelope,
    ) -> Result<Option<RequestSecrets>> {
        let Some(root) = self.root_of(envelope.generation)? else {
            return Ok(None);
        };
        let account = self.account()?.id.to_string();
        let plain = open(
            &root,
            &envelope.ciphertext,
            &management_aad(&account, request_id, envelope.generation),
        )?;
        decode(&plain, 16 * 1024).map(Some)
    }

    fn request_record(&self, request_id: &str) -> Result<Option<RequestRecord>> {
        self.workspace
            .private(self.keys()?, &format!("{RECORD}{request_id}"))
    }

    fn save_request(&self, record: &RequestRecord) -> Result<()> {
        self.workspace.put_private(
            self.keys()?,
            &format!("{RECORD}{}", record.request_id),
            record,
        )
    }

    fn received_record(&self, upload_id: &str) -> Result<Option<ReceivedRecord>> {
        self.workspace
            .private(self.keys()?, &format!("{RECEIVED}{upload_id}"))
    }

    fn keep_received(&self, record: &ReceivedRecord) -> Result<()> {
        self.workspace.put_private(
            self.keys()?,
            &format!("{RECEIVED}{}", record.plan.upload_id),
            record,
        )
    }

    fn request_view(&self, record: &RequestRecord, now: u64) -> Result<RequestView> {
        let secrets = record.secrets.as_ref();
        let account = self.account()?.id.to_string();
        Ok(RequestView {
            request_id: record.request_id.clone(),
            url: secrets.map(|s| {
                request_url(
                    &self.config.login_url,
                    &account,
                    &record.request_id,
                    &s.page_key,
                )
            }),
            title: secrets.map(|s| s.title.clone()),
            instructions: secrets.map(|s| s.instructions.clone()),
            label: secrets.map(|s| s.label.clone()),
            expires_at: record.expires_at,
            max_total_bytes: record.max_total_bytes,
            created_at: record.created_at,
            revoked_at: record.revoked_at,
            status: record.status(now).to_owned(),
            files: record.files,
            bytes: record.bytes,
            last_stored_at: record.last_stored_at,
            unread: record
                .last_stored_at
                .is_some_and(|at| record.seen_at.is_none_or(|seen| at > seen)),
            last_checked_at: record.last_checked_at,
        })
    }

    /// Keep what the relay says of a link, opening its management the first
    /// time this device has the root for it.
    fn keep_link(&self, remote: RemoteLink, now: u64) -> Result<RequestRecord> {
        let known = self.request_record(&remote.request_id)?;
        let seen_at = known.as_ref().and_then(|k| k.seen_at);
        let secrets = match known.and_then(|k| k.secrets) {
            Some(secrets) => Some(secrets),
            None => self.open_request_secrets(&remote.request_id, &remote.management)?,
        };
        let record = RequestRecord {
            request_id: remote.request_id,
            secrets,
            expires_at: remote.expires_at,
            max_total_bytes: remote.max_total_bytes,
            created_at: remote.created_at,
            revoked_at: remote.revoked_at,
            status: remote.status,
            files: remote.files,
            bytes: remote.bytes,
            last_stored_at: remote.last_stored_at,
            seen_at,
            last_checked_at: Some(now),
        };
        self.save_request(&record)?;
        Ok(record)
    }

    /// The links as this device last read them, newest first.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn files_requests(&self) -> Result<Vec<RequestView>> {
        let now = now_ms();
        let keys = self.keys()?;
        let mut views = Vec::new();
        for key in self.workspace.store.sealed_keys(RECORD)? {
            if let Some(record) = self.workspace.private::<RequestRecord>(keys, &key)? {
                views.push(self.request_view(&record, now)?);
            }
        }
        views.sort_by_key(|v| std::cmp::Reverse(v.created_at));
        Ok(views)
    }
}

/// Make a link. It works as soon as the relay answers; the HPKE seed and page
/// key leave this device only sealed under the account's root.
///
/// # Errors
/// Returns `Error::InvalidInput` for a draft or terms out of bounds,
/// `Error::RekeyRequired` while the vault cannot be written and the relay's
/// refusal when the plan does not allow another link.
pub async fn request_create<H: Hold>(
    h: &H,
    draft: RequestDraft<'_>,
    terms: RequestTerms,
) -> Result<RequestView> {
    check_draft(&draft)?;
    let lifetime = terms.expires_in_ms.unwrap_or(LIFETIME_MS);
    ensure_valid(lifetime > 0 && lifetime <= MAX_LIFETIME_MS, "expires_in_ms")?;
    if let Some(total) = terms.max_total_bytes {
        ensure_valid(total >= MIN_TOTAL, "max_total_bytes")?;
    }
    let (_, security) = h.signed(Client::fresh).await?;
    ensure(
        security.snapshot.vault_write_state == dmsg_types::user::VaultWriteState::Ready,
        Error::RekeyRequired,
    )?;
    h.signed(Client::root_now).await?;
    let (account, account_base, base) = h
        .with(|c| Ok((c.account()?.id.to_string(), c.base()?, c.requests_base()?)))
        .await?;
    let max_total_bytes = match terms.max_total_bytes {
        Some(total) => total,
        None => {
            let quota = h.get(&format!("{account_base}/quota")).await?;
            let lifecycle = &quota["lifecycle"];
            let wanted = match lifecycle["backup_max_file_bytes"].as_u64() {
                Some(limit) if limit > PLAIN_FILE_LIMIT => DEFAULT_TOTAL_MAX,
                _ => DEFAULT_TOTAL,
            };
            lifecycle["storage_limit"]
                .as_u64()
                .map_or(wanted, |limit| wanted.min(limit))
        }
    };
    let request_id = random_hex();
    let page_key = Zeroizing::new(random_key());
    let seed = Zeroizing::new(random_key());
    let secrets = RequestSecrets {
        hpke_seed: b64(&seed[..]),
        page_key: b64(&page_key[..]),
        label: draft.label.to_owned(),
        title: draft.title.to_owned(),
        instructions: draft.instructions.to_owned(),
    };
    let descriptor = seal_descriptor(
        &page_key,
        &RequestDescriptor {
            format: REQUEST_FORMAT.into(),
            account: account.clone(),
            request_id: request_id.clone(),
            hpke_pub: b64(&hpke_public(&seed)),
            title: secrets.title.clone(),
            instructions: secrets.instructions.clone(),
        },
    );
    ensure_valid(descriptor.len() <= MAX_DESCRIPTOR, "instructions")?;
    let management = h
        .with(|c| c.seal_request_secrets(&request_id, &secrets))
        .await?;
    let token = upload_token(&page_key, &account, &request_id);
    let reply = h
        .post(
            &format!("{base}/links"),
            "dmsg/request/create/v1",
            &json!({
                "request_id": request_id,
                "expires_at": now_ms() + lifetime.min(MAX_LIFETIME_MS - MINUTE),
                "max_total_bytes": max_total_bytes,
                "token_hash": token_hash(&token),
                "descriptor": descriptor,
                "management": management,
            }),
            &Hash::new(random_id()),
        )
        .await?;
    let remote: RemoteLink = parse(reply)?;
    ensure(remote.request_id == request_id, Error::IntegrityFailed)?;
    h.with(|c| {
        let now = now_ms();
        let record = c.keep_link(remote, now)?;
        c.request_view(&record, now)
    })
    .await
}

/// Read the account's links from the relay; those another device made open
/// here once this device has the root they are sealed under.
///
/// # Errors
/// Returns `Error::Locked` while locked and the relay's failures.
pub async fn requests_refresh<H: Hold>(h: &H) -> Result<Vec<RequestView>> {
    let base = h.with(|c| c.requests_base()).await?;
    let mut after = 0;
    loop {
        let page: Page<RemoteLink> = parse(
            h.get(&format!("{base}/links?after={after}&limit=100"))
                .await?,
        )?;
        h.with(|c| {
            let now = now_ms();
            for remote in page.entries {
                c.keep_link(remote, now)?;
            }
            Ok(())
        })
        .await?;
        if page.end {
            break;
        }
        ensure(page.next_cursor > after, Error::IntegrityFailed)?;
        after = page.next_cursor;
    }
    h.with(|c| c.files_requests()).await
}

/// Close a link. Files already received stay until they are deleted.
///
/// # Errors
/// As [`requests_refresh`].
pub async fn request_revoke<H: Hold>(h: &H, request_id: &str) -> Result<RequestView> {
    hex_id(request_id)?;
    let base = h.with(|c| c.requests_base()).await?;
    let reply = h
        .post(
            &format!("{base}/links/revoke"),
            "dmsg/request/revoke/v1",
            &json!({"request_id": request_id}),
            &Hash::new(random_id()),
        )
        .await?;
    let remote: RemoteLink = parse(reply)?;
    ensure(remote.request_id == request_id, Error::IntegrityFailed)?;
    h.with(|c| {
        let now = now_ms();
        let record = c.keep_link(remote, now)?;
        c.request_view(&record, now)
    })
    .await
}

/// A received file's manifest, or `None` for one that does not open: anyone
/// with the link can send anything, and it must not spoil the rest of the list.
async fn open_received<H: Hold>(
    h: &H,
    base: &str,
    seed: &Key,
    account: &str,
    request_id: &str,
    plan: &UploadPlan,
) -> Result<Option<RequestFile>> {
    let sealed = match crate::downloads::one(
        h,
        format!("{base}/objects/{}/chunks/manifest", plan.upload_id),
        &plan.manifest_digest,
    )
    .await
    {
        Ok(sealed) => sealed,
        Err(Error::IntegrityFailed) => return Ok(None),
        Err(other) => return Err(other),
    };
    Ok(open_request_file(seed, account, request_id, plan, &sealed).ok())
}

/// Tell the relay this account has the file, so its sender can no longer
/// withdraw it. A file withdrawn meanwhile stays saved here.
async fn acknowledge<H: Hold>(h: &H, base: &str, upload_id: &str) -> Option<u64> {
    let reply = h
        .post(
            &format!("{base}/files/ack"),
            "dmsg/request/file-ack/v1",
            &json!({"upload_id": upload_id}),
            &Hash::new(random_id()),
        )
        .await
        .ok()?;
    reply["received_at"].as_u64()
}

/// List a link's files, newest first: each new manifest is downloaded and
/// opened here, nothing else. The file itself comes down only on [`request_save`].
///
/// # Errors
/// Returns `Error::NotFound` for a link this device has not listed and
/// `Error::RecoveryIncomplete` for one whose root it lacks.
pub async fn request_files<H: Hold>(h: &H, request_id: &str) -> Result<Vec<ReceivedView>> {
    hex_id(request_id)?;
    let bound = Unlocked::new(h).await?;
    let h = &bound;
    let (account, base, seed) = h
        .with(|c| {
            let record = c.request_record(request_id)?.ok_or(Error::NotFound)?;
            let secrets = record.secrets.ok_or(Error::RecoveryIncomplete)?;
            Ok((
                c.account()?.id.to_string(),
                c.requests_base()?,
                key_from_text(&secrets.hpke_seed)?,
            ))
        })
        .await?;
    let mut records = Vec::new();
    let mut after = 0;
    loop {
        let page: Page<RemoteFile> = parse(
            h.get(&format!(
                "{base}/files?request={request_id}&after={after}&limit=100"
            ))
            .await?,
        )?;
        for entry in page.entries {
            ensure(
                entry.plan.upload_id == entry.upload_id && entry.plan.kind == REQUEST_FILE,
                Error::IntegrityFailed,
            )?;
            // Deleted by this account, here or on another device.
            if entry.withdrawn_at.is_none() && entry.status != "committed" {
                h.with(|c| {
                    c.workspace
                        .delete_private(&format!("{RECEIVED}{}", entry.upload_id))
                })
                .await?;
                continue;
            }
            let known = h.with(|c| c.received_record(&entry.upload_id)).await?;
            let record = match known {
                Some(known) => ReceivedRecord {
                    received_at: entry.received_at,
                    withdrawn_at: entry.withdrawn_at,
                    ..known
                },
                None => {
                    let file = match entry.withdrawn_at {
                        Some(_) => None,
                        None => {
                            open_received(h, &base, &seed, &account, request_id, &entry.plan)
                                .await?
                        }
                    };
                    ReceivedRecord {
                        request_id: request_id.to_owned(),
                        plan: entry.plan,
                        file,
                        stored_at: entry.stored_at,
                        received_at: entry.received_at,
                        withdrawn_at: entry.withdrawn_at,
                        saved: None,
                    }
                }
            };
            h.with(|c| c.keep_received(&record)).await?;
            records.push(record);
        }
        if page.end {
            break;
        }
        ensure(page.next_cursor > after, Error::IntegrityFailed)?;
        after = page.next_cursor;
    }
    // An acknowledgement that was lost when the file was saved.
    for record in &mut records {
        if record.saved.is_some() && record.received_at.is_none() && record.withdrawn_at.is_none() {
            record.received_at = acknowledge(h, &base, &record.plan.upload_id).await;
            if record.received_at.is_some() {
                h.with(|c| c.keep_received(record)).await?;
            }
        }
    }
    h.with(|c| {
        if let Some(mut request) = c.request_record(request_id)? {
            request.seen_at = Some(now_ms());
            c.save_request(&request)?;
        }
        Ok(())
    })
    .await?;
    let mut views: Vec<ReceivedView> = records.iter().map(ReceivedRecord::view).collect();
    views.sort_by_key(|v| std::cmp::Reverse(v.stored_at));
    Ok(views)
}

/// The folder a received file goes to unless one is chosen: `root/title/label`.
fn default_folder(root: &str, secrets: &RequestSecrets) -> String {
    let mut path = format!("{}/{}", safe_name(root), safe_name(&secrets.title));
    if !secrets.label.trim().is_empty() {
        path.push('/');
        path.push_str(&safe_name(&secrets.label));
    }
    path
}

/// Save a received file into the library, on an explicit request: its chunks
/// come down and are checked, the file is encrypted anew under the library's
/// own key, the sender's key and the downloaded ciphertext are then forgotten,
/// and the relay hears the account has it. It always becomes a new file, so
/// two senders' files of one name stay apart.
///
/// # Errors
/// Returns `Error::NotFound` for an unknown or withdrawn file and
/// `Error::IntegrityFailed` for anything that does not verify.
pub async fn request_save<H: Hold>(
    h: &H,
    upload_id: &str,
    folder: Option<&str>,
    root: &str,
    progress: &mut (dyn FnMut(&'static str, u64, u64) + Send),
) -> Result<FileView> {
    hex_id(upload_id)?;
    let bound = Unlocked::new(h).await?;
    let h = &bound;
    let (record, secrets, base) = h
        .with(|c| {
            let record = c.received_record(upload_id)?.ok_or(Error::NotFound)?;
            let request = c
                .request_record(&record.request_id)?
                .ok_or(Error::NotFound)?;
            Ok((
                record,
                request.secrets.ok_or(Error::RecoveryIncomplete)?,
                c.requests_base()?,
            ))
        })
        .await?;
    if let Some((file, _)) = &record.saved {
        // Saved before: it may only be the acknowledgement that was lost.
        let shown = h.with(|c| c.files_show(file)).await;
        match shown {
            Ok(details) => {
                if record.received_at.is_none() {
                    acknowledge(h, &base, upload_id).await;
                }
                return Ok(details.file);
            }
            Err(Error::NotFound) => {}
            Err(other) => return Err(other),
        }
    }
    ensure(record.withdrawn_at.is_none(), Error::NotFound)?;
    let opened = record.file.clone().ok_or(Error::IntegrityFailed)?;
    let manifest = opened.file;
    let hold = random_hex();
    let digests: Vec<String> = manifest.chunks.iter().map(|c| c.digest.clone()).collect();
    let pin =
        |c: &Client, now: u64| c.hold_chunks([(hold.clone(), now + HOLD_MS, digests.clone())]);
    let (target, missing) = h
        .with(|c| {
            let target = match folder {
                Some(folder) => {
                    c.catalog()?.live_folder(folder)?;
                    folder.to_owned()
                }
                None => c.files_mkdir_path(&default_folder(root, &secrets))?.id,
            };
            pin(c, now_ms())?;
            let mut missing = Vec::new();
            for (index, chunk) in manifest.chunks.iter().enumerate() {
                let present = match c.blobs()?.chunk(&chunk.digest) {
                    Ok(bytes) => bytes.is_some(),
                    Err(Error::IntegrityFailed) => {
                        c.blobs()?.delete(&chunk.digest)?;
                        false
                    }
                    Err(other) => return Err(other),
                };
                if !present {
                    missing.push((
                        format!("{base}/objects/{upload_id}/chunks/{index}"),
                        chunk.digest.clone(),
                    ));
                }
            }
            Ok((target, missing))
        })
        .await?;
    let count = manifest.chunks.len() as u64;
    let mut done = count - missing.len() as u64;
    progress("download", done, count);
    let saved = async {
        crate::downloads::chunks(h, &missing, |c, group, fetched| {
            for ((_, digest), bytes) in group.iter().zip(fetched) {
                c.blobs()?.put_chunk(digest, &bytes)?;
            }
            pin(c, now_ms())?;
            done += group.len() as u64;
            progress("download", done, count);
            Ok(())
        })
        .await?;
        h.with(|c| pin(c, now_ms())).await?;
        import::from_received(
            h,
            &manifest,
            &target,
            Origin::Received {
                request: record.request_id.clone(),
                upload: upload_id.to_owned(),
            },
            &mut |done, total| progress("save", done, total),
        )
        .await
    }
    .await;
    h.with(|c| {
        c.release_chunks(&hold)?;
        c.collect_blobs()?;
        Ok(())
    })
    .await?;
    let imported = saved?;
    let mut record = record;
    record.saved = Some((imported.file.id.clone(), imported.version.clone()));
    h.with(|c| c.keep_received(&record)).await?;
    record.received_at = acknowledge(h, &base, upload_id).await;
    h.with(|c| c.keep_received(&record)).await?;
    Ok(imported.file)
}

/// Delete a received file's cloud copy, which frees its storage; a copy saved
/// to the library stays there.
///
/// # Errors
/// As [`requests_refresh`].
pub async fn request_delete_file<H: Hold>(h: &H, upload_id: &str) -> Result<()> {
    hex_id(upload_id)?;
    let base = h.with(|c| c.requests_base()).await?;
    h.post(
        &format!("{base}/objects/collect"),
        "dmsg/object/collect/v1",
        &json!({"upload_id": upload_id, "generation": 0}),
        &Hash::new(random_id()),
    )
    .await?;
    h.with(|c| {
        c.workspace
            .delete_private(&format!("{RECEIVED}{upload_id}"))
    })
    .await
}

/// A base64url key of 32 bytes.
fn key_from_text(text: &str) -> Result<Zeroizing<Key>> {
    let bytes = Zeroizing::new(unb64(text).map_err(|_| Error::IntegrityFailed)?);
    Ok(Zeroizing::new(
        <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| Error::IntegrityFailed)?,
    ))
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
