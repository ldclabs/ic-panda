//! Formal statements: preparation views, device attestation and certified receipts.
//!
//! The wire statement is an RFC 9052 COSE_Sign1 signed with the approving
//! device's Ed25519 key. The user home verifies the signature inside the
//! approving message and records a certified [`ExecutionReceipt`]; the
//! receipt, not the key, proves which account authorized the document.
use crate::{AccountId, Approval, Ed25519Signature, Hash, OpId};
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

/// Policy domain of a formal statement, derived from its content profile.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeyPurpose {
    /// Typed product actions; not a generic document.
    AppAction,
    /// SHA-256 document attestations.
    FileAttestation,
    /// Text and structured file statements.
    Statement,
}

/// Device-signed formal statement submitted to the user home for a certified
/// receipt. Freeze the statement and origin, sign the exact COSE Sig_structure
/// with the approving device's key, then compute the approval over all three.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AttestRequest {
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// Prepared or parsed document claims and content.
    pub statement: Statement,
    /// Checked browser origin; part of device approval, not of the portable statement.
    pub origin: String,
    /// Approving device's Ed25519 signature over the exact Sig_structure
    /// bytes, whose kid is the device key's RFC 9679 thumbprint.
    pub signature: Ed25519Signature,
    /// Device approval binding the complete request and replay context.
    pub approval: Approval,
}

/// Explicit application-action attestation. The user home confirms the exact
/// body and account linkage with the registered product authority before it
/// records the receipt.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AppActionAttestRequest {
    /// Account approving this action, separate from its product actor.
    pub account_id: AccountId,
    /// Issuer must match the user's fixed account namespace.
    pub issuer: String,
    /// Closed typed content obtained from the product preparation.
    pub action: crate::app_action::AppAction,
    /// Approving device's Ed25519 signature over the exact Sig_structure bytes.
    pub signature: Ed25519Signature,
    /// Exact device approval and original execution identity.
    pub approval: Approval,
}

/// Certified attestation evidence binding an approved statement to the signed
/// bytes and the device key. Current schema is 2; the single leaf path is
/// `b"execution/" || account_id || request_id`.
/// Authenticate the certificate and witness before matching these fields to an
/// artifact. This records dMsg authorization, not external application
/// permissions or a TSA timestamp.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionReceipt {
    /// Version of this public certified-leaf format, not the storage schema.
    pub schema: u16,
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// Canonical absolute issuer URI identifying the signer.
    pub issuer: String,
    /// Operation identity used to bind approval and reconcile retries.
    pub request_id: OpId,
    /// 32-byte device identifier, distinct from its signing public key.
    pub device_id: Hash,
    /// Account security revision used to invalidate stale approvals.
    pub security_epoch: u64,
    /// Time authorization was accepted, in Unix milliseconds.
    pub approved_at: u64,
    /// Exclusive deadline in Unix milliseconds (`now < expires_at`).
    pub expires_at: u64,
    /// Checked browser origin bound by the device approval.
    pub origin: String,
    /// SHA-256 of the exact COSE Sig_structure bytes.
    pub to_be_signed_digest: Hash,
    /// RFC 9679 SHA-256 thumbprint of the approving device's COSE_Key.
    pub public_key_fingerprint: Hash,
    /// SHA-256 of raw signature bytes; differs from the CTT imprint.
    pub signature_digest: Hash,
}
