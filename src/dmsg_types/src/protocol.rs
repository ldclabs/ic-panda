//! Shared fixed bytes, time units, device approvals, errors and IC query proofs.
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// Fixed bytes in every Serde context, including tuples, options and map keys.
/// Candid remains `blob`; ByteArray validates the length when decoding.
pub type Hash = serde_bytes::ByteArray<32>;
/// 32-byte operation identifier. An alias of Hash, so callers must preserve its semantic role.
pub type OpId = Hash;
/// Result with a dMsg application error; does not represent transport errors.
pub type Result<T> = std::result::Result<T, Error>;
/// One second expressed in milliseconds; business timestamps use Unix milliseconds.
pub const SECOND: u64 = 1_000;
/// One minute expressed in milliseconds.
pub const MINUTE: u64 = 60 * SECOND;
/// One day expressed in milliseconds.
pub const DAY: u64 = 24 * 60 * MINUTE;
/// ICP system/certificate and ICRC ledger timestamps remain nanoseconds.
pub const NANOS_PER_MILLISECOND: u64 = 1_000_000;

/// Convert nanoseconds to milliseconds, truncating any fractional millisecond.
pub const fn nanos_to_millis(nanos: u64) -> u64 {
    nanos / NANOS_PER_MILLISECOND
}

/// Convert milliseconds to nanoseconds.
///
/// # Errors
/// Returns [`Error::InvalidInput`] if multiplication would overflow u64.
pub fn millis_to_nanos(millis: u64) -> Result<u64> {
    millis
        .checked_mul(NANOS_PER_MILLISECOND)
        .ok_or_else(|| invalid("timestamp overflow"))
}
/// Maximum generic protocol decode / prepared signing size in bytes (65,536).
/// Document profiles impose additional limits.
pub const MAX_PAYLOAD: usize = 65_536;
/// Maximum number of entries in bounded batch operations (64).
pub const MAX_BATCH: usize = 64;

/// Application errors returned by dMsg canisters and protocol helpers.
/// Transport failures and Candid decoding failures are separate from this enum.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Account-ID timestamp cannot fit the generator format.
    IdTimestampOutOfRange,
    /// Account-ID counter space is exhausted for this timestamp.
    IdCapacityExceeded,
    /// Persisted account-ID allocator state does not match its configured namespace.
    IdGeneratorStateConflict,
    /// An authenticated and appropriately bound caller is required.
    AuthRequired,
    /// The device is missing, revoked or not approved for this operation.
    DeviceNotApproved,
    /// Caller, role or capability does not authorize this operation.
    Forbidden,
    /// Sensitive operations are locked by account state or policy.
    Locked,
    /// Approval or policy evidence refers to an outdated security state.
    PolicyStale,
    /// A new content root must be committed before writes can resume.
    RekeyRequired,
    /// Expected version/generation differs from current state; reread before preparing a new approval.
    VersionConflict,
    /// An operation ID was reused with different parameters.
    IdempotencyConflict,
    /// An execution, cycle, storage or admission limit was exceeded.
    QuotaExceeded,
    /// A digest, signature, proof or accounting invariant failed validation.
    IntegrityFailed,
    /// Required recovery setup/confirmation has not completed.
    RecoveryIncomplete,
    /// The profile, algorithm or protocol version is unsupported.
    UnsupportedProtocol,
    /// Execution may have occurred; reconcile the original request instead of creating another.
    ExecutionUnknown,
    /// Retained output is no longer available; this does not authorize replay.
    ResultExpired,
    /// The approved fee or available fee reserve cannot support the transfer.
    FeeBlocked,
    /// Legacy business writes are frozen.
    LegacyWriteDisabled,
    /// Required legacy key material is unavailable.
    MigrationKeyUnavailable,
    /// The requested record does not exist.
    NotFound,
    /// The applicable deadline has passed.
    Expired,
    /// The operation is not yet complete; query/retry the same operation as appropriate.
    Pending,
    /// A supplied field or combination is invalid; carries a diagnostic.
    InvalidInput(String),
    /// A required service or resource is unavailable; carries a diagnostic.
    Unavailable(String),
}

/// Construct an invalid-input error with an owned diagnostic.
pub fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}
/// Return Ok(()) when condition holds, otherwise return the supplied error.
pub fn ensure(condition: bool, error: Error) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(error)
    }
}

/// Deployment domain used to separate identities and key derivation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Environment {
    /// Local development deployment.
    Local,
    /// Pre-production deployment domain.
    Staging,
    /// Production deployment with pinned chain-key configuration.
    Production,
}

/// Device Ed25519 approval over a domain-separated operation digest.
/// Construct the digest with the matching `dmsg_protocol` helper; signing the
/// Serde encoding of this struct is not a valid approval.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Approval {
    /// 32-byte device identifier, distinct from its signing public key.
    pub device_id: Hash,
    /// Account security revision used to invalidate stale approvals.
    pub security_epoch: u64,
    /// Next sequence expected for this device; contributes to replay protection.
    pub sequence: u64,
    /// Operation identity used to bind approval and reconcile retries.
    pub request_id: OpId,
    /// Exclusive deadline in Unix milliseconds (`now < expires_at`).
    pub expires_at: u64,
    /// Device Ed25519 signature over the operation-specific approval digest.
    pub signature: ByteBuf,
}

/// One requested leaf and its IC hash-tree witness.
/// A missing value must be checked as an absence proof, not treated as an empty leaf.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CertifiedEntry {
    /// Raw requested leaf path segment; compare with the exact expected path.
    pub key: ByteBuf,
    /// Exact serialized leaf bytes, or None for requested absence.
    pub value: Option<ByteBuf>,
    /// CBOR-encoded IC hash-tree witness for this requested leaf.
    pub witness: ByteBuf,
}
/// IC-certified query response. Merely decoding this value authenticates nothing.
/// Verify the IC certificate against a trusted root, the expected canister and
/// certificate time, then each witness, requested path and exact leaf bytes.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CertifiedBatch {
    /// Version of this public certified-leaf format, not the storage schema.
    pub schema: u16,
    /// Canister identity to match against the expected service and certificate.
    pub canister: Principal,
    /// IC data certificate bytes; verify against a trusted IC root key.
    pub certificate: ByteBuf,
    /// Requested leaves and their membership/absence witnesses.
    pub entries: Vec<CertifiedEntry>,
}
