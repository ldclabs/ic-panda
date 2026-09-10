use candid::Principal;
use dmsg_types::*;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};

/// Construct string errors only on failure, not on every successful check.
pub(crate) fn ensure_valid(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(invalid(message))
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

/// Encode trusted protocol values. Panics if their `Serialize` implementation fails.
pub fn canonical<T: Serialize>(value: &T) -> Vec<u8> {
    cbor2::to_canonical_vec(value).expect("serializable protocol value")
}

pub fn digest<T: Serialize>(domain: &str, value: &T) -> Hash {
    sha256(&canonical(&(1u8, domain, value)))
}

pub fn decode_canonical<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T> {
    ensure(bytes.len() <= MAX_PAYLOAD, Error::QuotaExceeded)?;
    let value: T = cbor2::from_slice(bytes).map_err(|_| Error::IntegrityFailed)?;
    let encoded = cbor2::to_canonical_vec(&value).map_err(|_| Error::IntegrityFailed)?;
    ensure(encoded == bytes, Error::IntegrityFailed)?;
    Ok(value)
}

pub fn verify(key: &Hash, message: &[u8], signature: &[u8]) -> Result<()> {
    let key = VerifyingKey::from_bytes(key).map_err(|_| Error::IntegrityFailed)?;
    let sig = Signature::from_slice(signature).map_err(|_| Error::IntegrityFailed)?;
    key.verify_strict(message, &sig)
        .map_err(|_| Error::IntegrityFailed)
}

pub(crate) fn validate_ed25519_key(bytes: &[u8]) -> Result<()> {
    let bytes = bytes.try_into().map_err(|_| Error::IntegrityFailed)?;
    let key = VerifyingKey::from_bytes(bytes).map_err(|_| Error::IntegrityFailed)?;
    ensure(!key.is_weak(), Error::IntegrityFailed)
}

pub fn expiry(now: u64, expires_at: u64, maximum: u64) -> Result<()> {
    ensure(
        now < expires_at && expires_at - now <= maximum,
        Error::Expired,
    )
}

pub fn nonzero(id: &[u8]) -> Result<()> {
    ensure_valid(id.iter().any(|b| *b != 0), "zero identifier/key")
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

pub fn approval_message<T: Serialize>(
    canister: Principal,
    account_id: &AccountId,
    domain: &str,
    command: &T,
    approval: &Approval,
) -> Hash {
    digest(
        "dmsg/device-approval/v2",
        &(
            canister,
            account_id,
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

pub fn execution_request_id(
    account_id: &AccountId,
    security_epoch: u64,
    device: Hash,
    sequence: u64,
) -> OpId {
    digest(
        "dmsg/execution-request/v2",
        &(account_id, security_epoch, device, sequence),
    )
}

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
mod tests;
