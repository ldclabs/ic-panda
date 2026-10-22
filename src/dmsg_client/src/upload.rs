//! Uploads of an account or a channel: a signed plan, ciphertext chunks, a
//! manifest and a finalize command. A retry reuses the stored plan and resumes after
//! the chunks already stored; an upload the relay already holds under the
//! same ID must carry the same plan. Each request waits on the relay
//! without the client ([`Hold`]).

use crate::client::Client;
use crate::files::{BlobStore, Source};
use crate::hold::Hold;
use dmsg_types::{Error, Hash, Result, ensure};
use serde::{Deserialize, Serialize};

/// One ciphertext chunk of a plan.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChunkRef {
    /// SHA-256 of the chunk, lowercase hex.
    pub digest: String,
    /// Chunk length in bytes.
    pub size: u64,
}

/// The upload plan the device signs with `dmsg/upload/reserve/v1`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UploadPlan {
    /// Upload ID, lowercase hex.
    pub upload_id: String,
    /// Object ID: the item, file or root bundle digest.
    pub object_id: String,
    /// Version ID of the object.
    pub version_id: String,
    /// Content-root generation the manifest is sealed under.
    pub root_generation: u64,
    /// Always 0 for account uploads.
    pub epoch: u64,
    /// Always null for account uploads.
    pub control_head: Option<String>,
    /// `root`, `vault` or `file`.
    pub kind: String,
    /// The chunks, in order.
    pub chunks: Vec<ChunkRef>,
    /// SHA-256 of the manifest bytes.
    pub manifest_digest: String,
    /// Manifest length in bytes.
    pub manifest_size: u64,
    /// When an unfinished upload lapses, in Unix milliseconds.
    pub expires_at: u64,
}

#[derive(Deserialize)]
struct UploadStatus {
    status: String,
    plan: UploadPlan,
    #[serde(default)]
    quota_state: Option<String>,
    #[serde(default)]
    lease_expires_at: Option<u64>,
}

#[derive(Serialize)]
struct UploadRef<'a> {
    upload_id: &'a str,
}

/// An unfinished upload must leave this much time to finish.
const UPLOAD_MARGIN_MS: u64 = 300_000;

/// A random nonzero ID as lowercase hex.
pub fn random_hex() -> String {
    hex::encode(crate::crypto::random_id())
}

/// Parse a 64-character lowercase hex ID.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for any other text or an all-zero ID.
pub fn hex_id(text: &str) -> Result<Hash> {
    ensure(
        text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
        Error::IntegrityFailed,
    )?;
    let bytes: [u8; 32] = hex::decode(text)
        .map_err(|_| Error::IntegrityFailed)?
        .try_into()
        .map_err(|_| Error::IntegrityFailed)?;
    ensure(bytes.iter().any(|b| *b != 0), Error::IntegrityFailed)?;
    Ok(Hash::new(bytes))
}

impl Client {
    /// The chunk store an upload reads from.
    pub(crate) fn source(&self, source: Source) -> Result<&dyn BlobStore> {
        Ok(match source {
            Source::Store => &self.workspace.store,
            Source::Library => self.blobs()?,
        })
    }

    /// [`upload`] by a caller that holds the client throughout.
    pub(crate) async fn upload(
        &mut self,
        base: &str,
        plan: &UploadPlan,
        manifest: &[u8],
        sent: usize,
        source: Source,
        progress: impl FnMut(&Self, usize) -> Result<()> + Send,
    ) -> Result<()> {
        upload(&self.hold(), base, plan, manifest, sent, source, progress).await
    }

    /// [`download`] by a caller that holds the client throughout.
    pub(crate) async fn download(
        &mut self,
        upload_id: &str,
        chunk: &str,
        digest: &str,
    ) -> Result<Vec<u8>> {
        download(&self.hold(), upload_id, chunk, digest).await
    }
}

/// Upload `plan` under `base` (`/v1/accounts/<id>` or `/v1/channels/<id>`):
/// its chunks come from `source` by digest, starting after the `sent`
/// already stored, then `manifest`, then finalize. Each chunk is read and
/// signed under the client and sent without it; `progress` records each
/// stored chunk under the client. An upload the relay already committed
/// under this plan returns at once.
///
/// # Errors
/// A different plan under the same ID returns `Error::IdempotencyConflict`;
/// an ended or nearly lapsed upload returns `Error::ResultExpired`, and the
/// caller replans it under a new ID.
pub(crate) async fn upload<H: Hold>(
    h: &H,
    base: &str,
    plan: &UploadPlan,
    manifest: &[u8],
    mut sent: usize,
    source: Source,
    mut progress: impl FnMut(&Client, usize) -> Result<()> + Send,
) -> Result<()> {
    let path = format!("{base}/uploads/{}", plan.upload_id);
    let previous: Option<UploadStatus> = h
        .find(&path)
        .await?
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| Error::IntegrityFailed)?;
    let now = crate::now_ms();
    let reserve = match &previous {
        Some(previous) => {
            ensure(previous.plan == *plan, Error::IdempotencyConflict)?;
            if previous.status == "committed" {
                return Ok(());
            }
            ensure(
                previous.status == "staging"
                    && previous.quota_state.as_deref() != Some("released")
                    && plan.expires_at > now + UPLOAD_MARGIN_MS
                    && previous
                        .lease_expires_at
                        .is_none_or(|t| t > now + UPLOAD_MARGIN_MS),
                Error::ResultExpired,
            )?;
            previous.quota_state.as_deref() == Some("pending")
        }
        None => {
            ensure(
                plan.expires_at > now + UPLOAD_MARGIN_MS,
                Error::ResultExpired,
            )?;
            sent = 0;
            true
        }
    };
    if reserve {
        h.post(
            &format!("{base}/uploads"),
            "dmsg/upload/reserve/v1",
            plan,
            &Hash::new(crate::crypto::random_id()),
        )
        .await?;
    }
    for (index, chunk) in plan.chunks.iter().enumerate().skip(sent) {
        h.send(|c| {
            let bytes = c
                .source(source)?
                .chunk(&chunk.digest)?
                .ok_or(Error::RecoveryIncomplete)?;
            ensure(bytes.len() as u64 == chunk.size, Error::IntegrityFailed)?;
            c.signed_put(&format!("{path}/chunks/{index}"), &bytes)
        })
        .await?;
        h.with(|c| progress(c, index + 1)).await?;
    }
    h.send(|c| c.signed_put(&format!("{path}/chunks/manifest"), manifest))
        .await?;
    let result = h
        .post(
            &format!("{base}/uploads/finalize"),
            "dmsg/upload/finalize/v1",
            &UploadRef {
                upload_id: &plan.upload_id,
            },
            &Hash::new(crate::crypto::random_id()),
        )
        .await?;
    ensure(
        result["upload_id"] == plan.upload_id.as_str()
            && result["digest"] == plan.manifest_digest.as_str()
            && result["object_id"] == plan.object_id.as_str()
            && result["version_id"] == plan.version_id.as_str(),
        Error::IntegrityFailed,
    )
}

/// Cancel a staging upload of the account that can no longer finish, so
/// its quota returns; an upload already ended stays as it is.
///
/// # Errors
/// Relay errors other than an absent or ended upload.
pub(crate) async fn cancel_upload<H: Hold>(h: &H, upload_id: &str) -> Result<()> {
    let base = h.with(|c| c.base()).await?;
    let found = h.find(&format!("{base}/uploads/{upload_id}")).await?;
    if found.is_some_and(|f| f["status"] == "staging") {
        h.post(
            &format!("{base}/uploads/cancel"),
            "dmsg/upload/cancel/v1",
            &UploadRef { upload_id },
            &Hash::new(crate::crypto::random_id()),
        )
        .await?;
    }
    Ok(())
}

/// Download chunks by `(path, digest)` into `into`, a few side by side,
/// each group signed with fresh evidence; every one is checked against its
/// digest, and `done` hears how many each group stored.
///
/// # Errors
/// A digest mismatch returns `Error::IntegrityFailed`.
pub(crate) async fn download_chunks<H: Hold>(
    h: &H,
    into: Source,
    parts: &[(String, String)],
    mut done: impl FnMut(usize) + Send,
) -> Result<()> {
    h.chunks(parts, |c, group, fetched| {
        for ((_, digest), bytes) in group.iter().zip(fetched) {
            c.source(into)?.put_chunk(digest, &bytes)?;
        }
        done(group.len());
        Ok(())
    })
    .await
}

/// Download one chunk or the manifest of the account's committed upload
/// and check its digest.
///
/// # Errors
/// A digest mismatch returns `Error::IntegrityFailed`.
pub(crate) async fn download<H: Hold>(
    h: &H,
    upload_id: &str,
    chunk: &str,
    digest: &str,
) -> Result<Vec<u8>> {
    let base = h.with(|c| c.base()).await?;
    h.chunk(
        &format!("{base}/objects/{upload_id}/chunks/{chunk}"),
        digest,
    )
    .await
}
