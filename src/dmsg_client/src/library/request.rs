//! File requests: a link that lets someone without an account upload files
//! to this account. It is the reverse of a share link: the fragment of
//! `<site>/r/<account>/<request_id>#<page_key>` gives the power to encrypt,
//! never to decrypt. `page_key` opens a descriptor holding the link's HPKE
//! public key, title and instructions, and derives the upload token the relay
//! checks; the HPKE seed stays in management sealed under the account's root.
//! A browser seals each file's manifest to that public key and its chunks by
//! the rules of `content-v1.json`. Formats: `request-v1.json`.

use crate::codec::{canonical, decode, sha256};
use crate::content::MAX_BACKUP_MANIFEST;
use crate::crypto::{HpkeEnvelope, Key, derive, hpke_open, open, seal};
use crate::files::{FileManifest, MAX_BACKUP, chunk_count};
use crate::upload::{UploadPlan, hex_id};
use dmsg_types::{Error, Result, ensure};
use serde::{Deserialize, Serialize};
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

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
