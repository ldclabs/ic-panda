use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// Fixed bytes in every Serde context, including tuples, options and map keys.
/// Candid remains `blob`; ByteArray validates the length when decoding.
pub type Hash = serde_bytes::ByteArray<32>;
pub type OpId = Hash;
pub type Result<T> = std::result::Result<T, Error>;
/// Business timestamps and durations are Unix milliseconds.
pub const SECOND: u64 = 1_000;
pub const MINUTE: u64 = 60 * SECOND;
pub const DAY: u64 = 24 * 60 * MINUTE;
/// ICP system/certificate and ICRC ledger timestamps remain nanoseconds.
pub const NANOS_PER_MILLISECOND: u64 = 1_000_000;

pub const fn nanos_to_millis(nanos: u64) -> u64 {
    nanos / NANOS_PER_MILLISECOND
}

pub fn millis_to_nanos(millis: u64) -> Result<u64> {
    millis
        .checked_mul(NANOS_PER_MILLISECOND)
        .ok_or_else(|| invalid("timestamp overflow"))
}
pub const MAX_PAYLOAD: usize = 65_536;
pub const MAX_BATCH: usize = 64;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Error {
    IdTimestampOutOfRange,
    IdCapacityExceeded,
    IdGeneratorStateConflict,
    AuthRequired,
    DeviceNotApproved,
    Forbidden,
    Locked,
    PolicyStale,
    RekeyRequired,
    VersionConflict,
    IdempotencyConflict,
    QuotaExceeded,
    IntegrityFailed,
    RecoveryIncomplete,
    UnsupportedProtocol,
    ExecutionUnknown,
    ResultExpired,
    FeeBlocked,
    LegacyWriteDisabled,
    MigrationKeyUnavailable,
    NotFound,
    Expired,
    Pending,
    InvalidInput(String),
    Unavailable(String),
}

pub fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}
pub fn ensure(condition: bool, error: Error) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(error)
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Environment {
    Local,
    Staging,
    Production,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Approval {
    pub device_id: Hash,
    pub security_epoch: u64,
    pub sequence: u64,
    pub request_id: OpId,
    pub expires_at: u64,
    pub signature: ByteBuf,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CertifiedEntry {
    pub key: ByteBuf,
    pub value: Option<ByteBuf>,
    pub witness: ByteBuf,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CertifiedBatch {
    pub schema: u16,
    pub canister: Principal,
    pub certificate: ByteBuf,
    pub entries: Vec<CertifiedEntry>,
}
