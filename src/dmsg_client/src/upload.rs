//! Uploads of an account or a channel: a signed plan, ciphertext chunks, a
//! manifest and a finalize command. A retry reuses the stored plan and resumes after
//! the chunks already stored; an upload the relay already holds under the
//! same ID must carry the same plan. Each request waits on the relay
//! without the client ([`Hold`]).

use crate::client::Client;
use crate::files::{BlobStore, Source};
use crate::hold::Hold;
use dmsg_types::{Error, Hash, Result, ensure};
use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// One ciphertext chunk of a plan.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChunkRef {
    /// SHA-256 of the chunk, lowercase hex.
    pub digest: String,
    /// Chunk length in bytes.
    pub size: u64,
}

/// Account-root-encrypted source and recovery information; opaque to the relay.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagementEnvelope {
    /// The account root generation used to encrypt the metadata.
    pub generation: u64,
    /// Authenticated encrypted metadata, base64url.
    pub ciphertext: String,
}

/// The upload plan the device signs with `dmsg/upload/reserve/v1`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UploadPlan {
    /// Optional encrypted management information of the uploader.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub management: Option<ManagementEnvelope>,
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
    public_url: Option<String>,
    #[serde(default)]
    lease_expires_at: Option<u64>,
}

#[derive(Serialize)]
struct UploadRef<'a> {
    upload_id: &'a str,
}

/// A new plan should leave this much time to start. Existing reservations
/// remain resumable until their actual deadline; their leases may be shorter.
const UPLOAD_MARGIN_MS: u64 = 300_000;

/// Leave five minutes below the relay's 24-hour limit. This exceeds the
/// clock skew permitted by the account certificates and request proofs;
/// upload plans must not depend on network latency to fit below that limit.
pub(crate) fn expires_at(now: u64) -> u64 {
    now + dmsg_types::DAY - 5 * dmsg_types::MINUTE
}

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
        mut progress: impl FnMut(&Self, usize) -> Result<()> + Send,
    ) -> Result<()> {
        upload(&self.hold(), base, plan, manifest, sent, source, |c, n| {
            progress(c, n)
        })
        .await
        .map(|_| ())
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
/// under this plan returns at once, except an avatar awaiting public publication.
/// Returns the final public URL for avatars, and `None` for private uploads.
///
/// # Errors
/// A different plan under the same ID returns `Error::IdempotencyConflict`;
/// an ended upload or a new plan too near expiry returns `Error::ResultExpired`, and the
/// caller replans it under a new ID.
pub(crate) async fn upload<H: Hold>(
    h: &H,
    base: &str,
    plan: &UploadPlan,
    manifest: &[u8],
    mut sent: usize,
    source: Source,
    mut progress: impl FnMut(&mut Client, usize) -> Result<()> + Send,
) -> Result<Option<String>> {
    let mut acknowledged: BTreeSet<usize> = (0..sent.min(plan.chunks.len())).collect();
    upload_parts(
        h,
        base,
        plan,
        manifest,
        &acknowledged.clone(),
        source,
        |c, event| {
            match event {
                PartProgress::Reset => {
                    acknowledged.clear();
                    sent = 0;
                    return progress(c, 0);
                }
                PartProgress::Stored(index) => {
                    acknowledged.insert(index);
                }
                PartProgress::Committing => return Ok(()),
            }
            let through = (0..plan.chunks.len())
                .take_while(|i| acknowledged.contains(i))
                .count();
            if through != sent {
                sent = through;
                progress(c, through)?;
            }
            Ok(())
        },
    )
    .await
}

pub(crate) enum PartProgress {
    Reset,
    Stored(usize),
    Committing,
}

/// Resume independently acknowledged immutable blocks. Only a failed block is
/// retried; successful replies are journaled before another slot is admitted.
pub(crate) async fn upload_parts<H: Hold>(
    h: &H,
    base: &str,
    plan: &UploadPlan,
    manifest: &[u8],
    acknowledged: &BTreeSet<usize>,
    source: Source,
    mut progress: impl FnMut(&mut Client, PartProgress) -> Result<()> + Send,
) -> Result<Option<String>> {
    let path = format!("{base}/uploads/{}", plan.upload_id);
    // A pooled connection can close while a large edition is prepared.
    // Recheck this read once with fresh request authentication, keeping the
    // same upload ID. Mutation replies are still reconciled by their caller.
    let status = match h.find(&path).await {
        Err(error @ Error::Unavailable(_)) if crate::transport::retry_after(&error).is_none() => {
            h.find(&path).await?
        }
        result => result?,
    };
    let previous: Option<UploadStatus> = status
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| Error::IntegrityFailed)?;
    let now = crate::now_ms();
    let reserve = match &previous {
        Some(previous) => {
            ensure(previous.plan == *plan, Error::IdempotencyConflict)?;
            if previous.status == "committed" {
                if plan.kind == "avatar" && previous.public_url.is_none() {
                    return finalize(h, base, plan).await;
                }
                return Ok(previous.public_url.clone());
            }
            ensure(
                previous.status == "staging"
                    && previous.quota_state.as_deref() != Some("released")
                    && plan.expires_at > now
                    && previous.lease_expires_at.is_none_or(|t| t > now),
                Error::ResultExpired,
            )?;
            previous.quota_state.as_deref() == Some("pending")
        }
        None => {
            ensure(
                plan.expires_at > now + UPLOAD_MARGIN_MS,
                Error::ResultExpired,
            )?;
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
    if previous.is_none() {
        h.with(|c| progress(c, PartProgress::Reset)).await?;
    }
    let width = if plan.kind == "envelopes" { 8 } else { 4 };
    let count = plan.chunks.len();
    let parts: Vec<usize> = (0..=count)
        .filter(|i| previous.is_none() || *i == count || !acknowledged.contains(i))
        .collect();
    let mut pending = stream::iter(parts.into_iter().map(|index| {
        let path = &path;
        async move {
            let part = if index == count {
                "manifest".into()
            } else {
                index.to_string()
            };
            let path = format!("{path}/chunks/{part}");
            let mut result = Ok(serde_json::Value::Null);
            for attempt in 0..3 {
                result = h
                    .send(|c| {
                        if index == count {
                            return c.signed_put(&path, manifest);
                        }
                        let chunk = &plan.chunks[index];
                        let bytes = c
                            .source(source)?
                            .chunk(&chunk.digest)?
                            .ok_or(Error::RecoveryIncomplete)?;
                        ensure(bytes.len() as u64 == chunk.size, Error::IntegrityFailed)?;
                        c.signed_put(&path, &bytes)
                    })
                    .await;
                match &result {
                    Err(error @ (Error::Unavailable(_) | Error::ExecutionUnknown))
                        if attempt < 2 && crate::transport::retry_after(error).is_none() =>
                    {
                        futures_timer::Delay::new(std::time::Duration::from_millis(500 << attempt))
                            .await;
                    }
                    _ => break,
                }
            }
            (index, result)
        }
    }))
    .buffer_unordered(width);
    while let Some((index, result)) = pending.next().await {
        result?;
        if index < count {
            h.with(|c| progress(c, PartProgress::Stored(index))).await?;
        }
    }
    h.with(|c| progress(c, PartProgress::Committing)).await?;
    finalize(h, base, plan).await
}

// A committed avatar may still be waiting for its public object. Reuse the same
// plan and finalize without reserving, uploading or charging again.
pub(crate) async fn finalize<H: Hold>(
    h: &H,
    base: &str,
    plan: &UploadPlan,
) -> Result<Option<String>> {
    // Only large backups need a fresh full authorization window. Ordinary
    // content sync already reconciles its own mutations and keeps its hot path.
    if plan.chunks.len() > 128 {
        h.warm(crate::client::Warm::RelayFresh).await?;
    }
    let result = h
        .post(
            &format!("{base}/uploads/finalize"),
            "dmsg/upload/finalize/v1",
            &UploadRef {
                upload_id: &plan.upload_id,
            },
            &Hash::new(crate::crypto::random_id()),
        )
        .await;
    let result = match result {
        Ok(result) => result,
        Err(error @ (Error::ExecutionUnknown | Error::Unavailable(_))) if plan.kind == "backup" => {
            if let Ok(Some(status)) = h.find(&format!("{base}/uploads/{}", plan.upload_id)).await {
                let status: UploadStatus =
                    serde_json::from_value(status).map_err(|_| Error::IntegrityFailed)?;
                ensure(status.plan == *plan, Error::IdempotencyConflict)?;
                if status.status == "committed"
                    && (plan.kind != "avatar" || status.public_url.is_some())
                {
                    return Ok(status.public_url);
                }
            }
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    ensure(
        result["upload_id"] == plan.upload_id.as_str()
            && result["digest"] == plan.manifest_digest.as_str()
            && result["object_id"] == plan.object_id.as_str()
            && result["version_id"] == plan.version_id.as_str(),
        Error::IntegrityFailed,
    )?;
    if plan.kind == "avatar" {
        Ok(Some(
            result["public_url"]
                .as_str()
                .ok_or(Error::IntegrityFailed)?
                .to_owned(),
        ))
    } else {
        Ok(None)
    }
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

/// Download ordinary file parts directly, checking each expected digest.
pub(crate) async fn download_file_chunks<H: Hold>(
    h: &H,
    into: Source,
    parts: &[(String, String)],
    mut done: impl FnMut(usize) + Send,
) -> Result<()> {
    crate::downloads::chunks(h, parts, |c, group, fetched| {
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

#[cfg(test)]
#[path = "upload_tests.rs"]
mod tests;
