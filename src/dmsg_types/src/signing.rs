//! Preparation and verified views. The wire statement itself is RFC 9052 COSE_Sign1.
use crate::Hash;
use candid::CandidType;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// Portable statement preparation/parsed view, not the COSE wire payload.
/// The issuer and optional claims become protected headers; content determines
/// the document or explicit application-action profile. Use `dmsg_protocol` to validate and encode it.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Statement {
    /// Canonical absolute issuer URI identifying the signer.
    pub issuer: String,
    /// The object being described, not the account making the statement.
    pub subject: Option<String>,
    /// Claimed Unix seconds, never a TSA assertion or execution deadline.
    pub issued_at: Option<i64>,
    /// Document content and profile selection.
    pub content: StatementContent,
}

/// Closed payload selection. Application actions cannot use the document-only endpoint.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub enum StatementContent {
    /// Explicit application action profile; never accepted by the generic document signer.
    AppAction(Box<crate::app_action::AppAction>),
    /// Raw UTF-8 text, 1..4096 bytes; no whitespace trimming or Unicode normalization.
    Text(String),
    /// SHA-256 document profile with optional original-content metadata.
    Digest {
        /// SHA-256 of the exact original bytes, not their encoded digest text.
        sha256: Hash,
        /// Optional original media type; metadata does not verify the original bytes.
        content_type: Option<String>,
        /// Optional original-content URI; verification does not fetch it automatically.
        location: Option<String>,
    },
    /// A statement about one exact file, encoded as deterministic CBOR.
    /// The text conveys no automatic approval, authority or proof of file review.
    FileStatement {
        /// Raw UTF-8 text, 1..4096 bytes; preserved without normalization.
        text: String,
        /// SHA-256 of the exact original file bytes.
        sha256: Hash,
        /// Optional file media type, included in the signed payload.
        content_type: Option<String>,
        /// Optional file URI; verification never fetches it automatically.
        location: Option<String>,
    },
}

/// Portable signed document plus a public-only COSE_Key.
/// The attached key supports mathematical verification but does not establish
/// who owns the key or whether the signer had authority.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SignedArtifact {
    /// Tagged RFC 9052 COSE_Sign1 CBOR bytes.
    pub cose_sign1: ByteBuf,
    /// Public-only COSE_Key CBOR bytes; never include private key parameters.
    pub cose_key: ByteBuf,
}
