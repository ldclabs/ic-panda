use candid::{CandidType, Principal};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_bytes::ByteBuf;
use sha2::{Digest, Sha256};

/// Fixed bytes in every Serde context, including tuples, options and map keys.
/// Candid remains `blob`; ByteArray validates the length when decoding.
pub type Hash = serde_bytes::ByteArray<32>;
pub type SubjectId = Hash;
pub type OpId = Hash;
pub type Result<T> = std::result::Result<T, Error>;
/// Business timestamps and durations are Unix milliseconds.
pub const SECOND: u64 = 1_000;
pub const MINUTE: u64 = 60 * SECOND;
pub const DAY: u64 = 24 * 60 * MINUTE;
/// ICP system/certificate and ICRC ledger timestamps remain nanoseconds.
pub const NANOS_PER_MILLISECOND: u64 = 1_000_000;
/// Reject pre-millisecond / integer-array development state on upgrade.
pub const STABLE_SCHEMA: u16 = 2;

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
pub const WINDOW: usize = 64;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Error {
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
pub fn authenticated(caller: Principal) -> Result<()> {
    ensure(
        caller != Principal::anonymous() && caller != Principal::management_canister(),
        Error::AuthRequired,
    )
}
pub fn sha256(bytes: &[u8]) -> Hash {
    Hash::new(Sha256::digest(bytes).into())
}

/// RFC 8949 core deterministic CBOR (bytewise map ordering). The schema is
/// serialized explicitly, with no floats, unrecognized fields or implicit JSON.
pub fn canonical<T: Serialize>(value: &T) -> Vec<u8> {
    cbor2::to_canonical_vec(value).expect("serializable protocol value")
}
pub fn digest<T: Serialize>(domain: &str, value: &T) -> Hash {
    sha256(&canonical(&(1u8, domain, value)))
}
pub fn decode_canonical<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T> {
    ensure(bytes.len() <= MAX_PAYLOAD, Error::QuotaExceeded)?;
    let value: T = cbor2::from_slice(bytes).map_err(|_| Error::IntegrityFailed)?;
    ensure(canonical(&value) == bytes, Error::IntegrityFailed)?;
    Ok(value)
}
pub fn verify(key: &Hash, message: &[u8], signature: &[u8]) -> Result<()> {
    let key = VerifyingKey::from_bytes(key).map_err(|_| Error::IntegrityFailed)?;
    let sig = Signature::from_slice(signature).map_err(|_| Error::IntegrityFailed)?;
    key.verify_strict(message, &sig)
        .map_err(|_| Error::IntegrityFailed)
}
pub fn expiry(now: u64, expires_at: u64, maximum: u64) -> Result<()> {
    ensure(
        now < expires_at && expires_at - now <= maximum,
        Error::Expired,
    )
}
pub fn nonzero(id: &Hash) -> Result<()> {
    ensure(*id != Hash::new([0; 32]), invalid("zero identifier/key"))
}
pub fn validate_transport_key(bytes: &[u8]) -> Result<()> {
    let compressed: &[u8; 48] = bytes
        .try_into()
        .map_err(|_| invalid("vetKD transport key length"))?;
    let point =
        Option::<ic_bls12_381::G1Affine>::from(ic_bls12_381::G1Affine::from_compressed(compressed))
            .ok_or(Error::IntegrityFailed)?;
    ensure(!bool::from(point.is_identity()), Error::IntegrityFailed)
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

/// Every device authorization binds the destination, method and exact command.
pub fn approval_message<T: Serialize>(
    canister: Principal,
    subject: SubjectId,
    domain: &str,
    command: &T,
    approval: &Approval,
) -> Hash {
    digest(
        "dmsg/device-approval/v1",
        &(
            canister,
            subject,
            domain,
            approval.device_id,
            approval.security_epoch,
            approval.sequence,
            approval.request_id,
            approval.expires_at,
            digest(domain, command),
        ),
    )
}

/// Execution IDs cannot be reused with a fresh approval sequence after result
/// eviction. A security epoch also separates device reenrollment/recovery.
pub fn execution_request_id(
    subject: SubjectId,
    security_epoch: u64,
    device: Hash,
    sequence: u64,
) -> OpId {
    digest(
        "dmsg/execution-request/v1",
        &(subject, security_epoch, device, sequence),
    )
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

/// Strict sequences for account mutations; bounded result retention never
/// resets the sequence, so evicting an operation cannot reauthorize it.
pub fn check_sequence(next: u64, sequence: u64) -> Result<()> {
    if sequence < next {
        Err(Error::ResultExpired)
    } else if sequence != next || next == u64::MAX {
        Err(Error::VersionConflict)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    #[test]
    fn signature_domains_and_canonical_encoding_are_bound() {
        let sk = SigningKey::from_bytes(&[7; 32]);
        let p = Principal::from_slice(&[1]);
        let mut a = Approval {
            device_id: Hash::new([2; 32]),
            security_epoch: 1,
            sequence: 0,
            request_id: Hash::new([3; 32]),
            expires_at: 100,
            signature: ByteBuf::new(),
        };
        let msg = approval_message(p, Hash::new([4; 32]), "root", &42u64, &a);
        a.signature = sk.sign(msg.as_slice()).to_bytes().to_vec().into();
        assert!(verify(
            &sk.verifying_key().to_bytes().into(),
            msg.as_slice(),
            &a.signature
        )
        .is_ok());
        assert!(verify(
            &sk.verifying_key().to_bytes().into(),
            approval_message(p, Hash::new([4; 32]), "sign", &42u64, &a).as_slice(),
            &a.signature
        )
        .is_err());
        assert_eq!(
            hex::encode(canonical(&(1u8, "dmsg", 42u64))),
            "830164646d7367182a"
        );
        assert!(decode_canonical::<u64>(&[0x18, 0x01]).is_err());
        assert!(decode_canonical::<u64>(&[0x01, 0x01]).is_err());
    }
}
