//! Preparation and verified views. The wire statement itself is RFC 9052 COSE_Sign1.
use crate::Hash;
use candid::CandidType;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// Portable document preparation/parsed view, not the COSE wire payload.
/// The issuer and optional claims become protected headers; content determines
/// the text or digest profile. Use `dmsg_protocol` to validate and encode it.
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
/// Document payload selection. Text is signed verbatim; digest signs 32 raw bytes.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub enum StatementContent {
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
/// Evidence status for one verification dimension; not a catch-all success flag.
/// Invalid signatures/content are reported as errors by protocol verification.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum VerificationStatus {
    /// This dimension was checked successfully against the supplied verification policy.
    Verified,
    /// Required evidence/input was not supplied, for example the original file.
    NotProvided,
    /// This dimension was not verified; no trust claim is made.
    NotChecked,
}
/// This report never promotes caller-supplied keys/tokens into trusted credentials.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct VerificationReport {
    /// Prepared or parsed document claims and content.
    pub statement: Statement,
    /// Whether the COSE profile and mathematical signature were verified.
    pub signature: VerificationStatus,
    /// Whether original content was verified; digest documents need the original bytes.
    pub content: VerificationStatus,
    /// Whether trusted evidence binds the signing key to the issuer.
    pub issuer_binding: VerificationStatus,
    /// Whether signing authority was independently checked.
    pub authorization: VerificationStatus,
    /// Whether trusted timestamp evidence was independently checked.
    pub timestamp: VerificationStatus,
    /// Whether current key/account status was independently checked.
    pub current_status: VerificationStatus,
}
