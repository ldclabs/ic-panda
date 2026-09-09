//! Preparation and verified views. The wire statement itself is RFC 9052 COSE_Sign1.
use crate::Hash;
use candid::CandidType;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Statement {
    pub issuer: String,
    /// The object being described, not the account making the statement.
    pub subject: Option<String>,
    /// Claimed Unix seconds, never a TSA assertion or execution deadline.
    pub issued_at: Option<i64>,
    pub content: StatementContent,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub enum StatementContent {
    Text(String),
    Digest {
        sha256: Hash,
        content_type: Option<String>,
        location: Option<String>,
    },
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SignedArtifact {
    pub cose_sign1: ByteBuf,
    pub cose_key: ByteBuf,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum VerificationStatus {
    Verified,
    NotProvided,
    NotChecked,
}
/// This report never promotes caller-supplied keys/tokens into trusted credentials.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct VerificationReport {
    pub statement: Statement,
    pub signature: VerificationStatus,
    pub content: VerificationStatus,
    pub issuer_binding: VerificationStatus,
    pub authorization: VerificationStatus,
    pub timestamp: VerificationStatus,
    pub current_status: VerificationStatus,
}
