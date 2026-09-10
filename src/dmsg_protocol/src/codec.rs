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

/// Reject anonymous and management-canister Principals.
///
/// This is only a caller-shape check: it does not establish an account binding,
/// device approval, or permission. The caller must come from a trusted transport.
///
/// # Errors
/// Returns `Error::AuthRequired` for either excluded Principal.
pub fn authenticated(caller: Principal) -> Result<()> {
    ensure(
        caller != Principal::anonymous() && caller != Principal::management_canister(),
        Error::AuthRequired,
    )
}

/// SHA-256 of the exact supplied bytes, without CBOR framing or domain separation.
/// Use [`digest`] for versioned protocol commitments.
pub fn sha256(bytes: &[u8]) -> Hash {
    Hash::new(Sha256::digest(bytes).into())
}

/// Encode trusted protocol values using RFC 8949 core deterministic CBOR.
///
/// This is not semantic validation or a size limit. Use the public types to retain
/// byte-string representations, and use [`decode_canonical`] for incoming records.
/// Do not re-encode protected headers of an existing COSE artifact before verification.
///
/// # Panics
/// Panics if the value's `Serialize` implementation fails.
pub fn canonical<T: Serialize>(value: &T) -> Vec<u8> {
    cbor2::to_canonical_vec(value).expect("serializable protocol value")
}

/// Compute `SHA256(CBOR([1, domain, value]))` with deterministic encoding.
///
/// Use the exact domain and value shape required by the operation. The leading
/// 1 is the digest framing version; it does not replace the domain's version.
///
/// # Panics
/// Panics if the value's `Serialize` implementation fails, like [`canonical`].
pub fn digest<T: Serialize>(domain: &str, value: &T) -> Hash {
    sha256(&canonical(&(1u8, domain, value)))
}

/// Decode a protocol record and require byte-for-byte canonical re-encoding.
///
/// The target type must preserve the public wire representation. This generic
/// record decoder is not the COSE artifact verifier; use [`crate::verify_artifact`] for
/// signed documents, which preserves incoming protected-header bytes.
///
/// # Errors
/// Returns `Error::QuotaExceeded` above `dmsg_types::MAX_PAYLOAD` (65,536 bytes).
/// Decoding, re-encoding, or byte-equality failures return `Error::IntegrityFailed`.
/// Business authorization and field semantics are not checked.
pub fn decode_canonical<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T> {
    ensure(bytes.len() <= MAX_PAYLOAD, Error::QuotaExceeded)?;
    let value: T = cbor2::from_slice(bytes).map_err(|_| Error::IntegrityFailed)?;
    let encoded = cbor2::to_canonical_vec(&value).map_err(|_| Error::IntegrityFailed)?;
    ensure(encoded == bytes, Error::IntegrityFailed)?;
    Ok(value)
}

/// Strictly verify an Ed25519 signature over the supplied message bytes.
///
/// For device approvals, pass the protocol approval digest as the message.
/// This function adds no hashing, CBOR framing, or domain separation and does
/// not verify COSE profiles or establish the public key's owner.
///
/// # Errors
/// Invalid keys, signature encodings, and signatures return `Error::IntegrityFailed`.
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

/// Check `now < expires_at` and `expires_at - now <= maximum`.
///
/// All arguments are milliseconds; the first two are Unix timestamps and the
/// last is the maximum remaining lifetime. This function reads no clock.
///
/// # Errors
/// Returns `Error::Expired` both for expired deadlines and excessively long lifetimes.
pub fn expiry(now: u64, expires_at: u64, maximum: u64) -> Result<()> {
    ensure(
        now < expires_at && expires_at - now <= maximum,
        Error::Expired,
    )
}

/// Require at least one nonzero byte; this does not validate length or key structure.
///
/// # Errors
/// An empty or all-zero slice returns `Error::InvalidInput`.
pub fn nonzero(id: &[u8]) -> Result<()> {
    ensure_valid(id.iter().any(|b| *b != 0), "zero identifier/key")
}

/// Validate a 48-byte compressed vetKD transport public key.
///
/// Checks decoding into the BLS12-381 G1 subgroup and rejects the identity point.
/// This does not derive a root or prove possession of the transport secret key.
///
/// # Errors
/// Wrong length returns `Error::InvalidInput`; invalid points return `Error::IntegrityFailed`.
pub fn validate_transport_key(bytes: &[u8]) -> Result<()> {
    let compressed: &[u8; 48] = bytes
        .try_into()
        .map_err(|_| invalid("vetKD transport key length"))?;
    let point =
        Option::<ic_bls12_381::G1Affine>::from(ic_bls12_381::G1Affine::from_compressed(compressed))
            .ok_or(Error::IntegrityFailed)?;
    ensure(!bool::from(point.is_identity()), Error::IntegrityFailed)
}

/// Build the digest signed by a device to authorize an operation.
///
/// Uses `dmsg/device-approval/v2` and binds the target user canister, account,
/// operation domain, device, epoch, sequence, request ID, deadline, and
/// `digest(domain, command)`. The approval's signature field is excluded.
/// For execution requests, prefer [`crate::ExecuteRequestExt::approval_message`].
/// For account mutations use domain `dmsg/account/v2` and the command tuple
/// `(expected_version, AccountCommand)`.
///
/// This constructs bytes only; it neither verifies the signature nor consumes
/// sequences or checks expiry. Sign the returned digest bytes with the device key.
///
/// # Panics
/// Panics if command serialization fails.
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

/// Derive an execution ID under `dmsg/execution-request/v2`.
///
/// Binds `(account_id, security_epoch, device, sequence)`. It does not depend on
/// document content. Preserve the ID and original parameters while reconciling
/// an unknown result; changing content under the same ID is an idempotency conflict.
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

/// Check an approval sequence against the next expected device sequence.
///
/// This does not mutate the counter; the account service must consume it atomically.
///
/// # Errors
/// Older sequences return `Error::ResultExpired`; future sequences or an exhausted
/// `u64::MAX` counter return `Error::VersionConflict`.
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
