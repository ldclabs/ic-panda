//! Account uploads: a signed plan, ciphertext chunks, an encrypted manifest
//! and a finalize command. A retry reuses the stored plan; an upload the
//! relay already holds under the same ID must carry the same plan.

use crate::client::Client;
use dmsg_types::{ensure, Error, Hash, Result};
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
}

#[derive(Serialize)]
struct UploadRef<'a> {
    upload_id: &'a str,
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
    /// Upload `chunks` and `manifest` under `plan`, or confirm the relay
    /// already committed exactly this plan.
    ///
    /// # Errors
    /// A different plan under the same ID returns `Error::IdempotencyConflict`;
    /// an ended upload returns `Error::ResultExpired`.
    pub(crate) async fn upload(
        &mut self,
        plan: &UploadPlan,
        chunks: &[Vec<u8>],
        manifest: &[u8],
    ) -> Result<()> {
        ensure(chunks.len() == plan.chunks.len(), Error::IntegrityFailed)?;
        let base = self.base()?;
        let path = format!("{base}/uploads/{}", plan.upload_id);
        let previous: Option<UploadStatus> = self
            .relay_find(&path)
            .await?
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| Error::IntegrityFailed)?;
        if let Some(previous) = &previous {
            ensure(previous.plan == *plan, Error::IdempotencyConflict)?;
            if previous.status == "committed" {
                return Ok(());
            }
            ensure(previous.status == "staging", Error::ResultExpired)?;
        } else {
            self.relay_post(
                &format!("{base}/uploads"),
                "dmsg/upload/reserve/v1",
                plan,
                &Hash::new(crate::crypto::random_id()),
            )
            .await?;
        }
        for (index, chunk) in chunks.iter().enumerate() {
            self.relay_put(&format!("{path}/chunks/{index}"), chunk)
                .await?;
        }
        self.relay_put(&format!("{path}/chunks/manifest"), manifest)
            .await?;
        let result = self
            .relay_post(
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
                && result["digest"] == plan.manifest_digest.as_str(),
            Error::IntegrityFailed,
        )
    }

    /// Download a committed upload's manifest and check its digest.
    ///
    /// # Errors
    /// A digest mismatch returns `Error::IntegrityFailed`.
    pub(crate) async fn download(
        &mut self,
        upload_id: &str,
        chunk: &str,
        digest: &str,
    ) -> Result<Vec<u8>> {
        let base = self.base()?;
        self.relay_chunk(
            &format!("{base}/objects/{upload_id}/chunks/{chunk}"),
            digest,
        )
        .await
    }
}
