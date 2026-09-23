//! Independent IC authentication verification. Trust roots are supplied by the relying product.
use crate::{decode_canonical, integration::*};
use candid::Principal;
use dmsg_types::{integration::*, *};
use ic_certification::{HashTree, LookupResult};

fn timestamp(bytes: &[u8]) -> Result<u64> {
    let mut n = 0u64;
    for (i, byte) in bytes.iter().copied().enumerate() {
        ensure(i < 10 && (i != 9 || byte <= 1), Error::IntegrityFailed)?;
        n |= u64::from(byte & 127) << (7 * i);
        if byte & 128 == 0 {
            ensure(
                i + 1 == bytes.len() && (i == 0 || byte != 0),
                Error::IntegrityFailed,
            )?;
            return Ok(n);
        }
    }
    Err(Error::IntegrityFailed)
}

/// Verify the root, expected home, certificate freshness, witness and complete pending request.
///
/// `expected` must come from the product's stored challenge, not the proof or browser.
/// The caller must separately consume that challenge and check temporary-key possession.
/// An already issued proof has at most the five-minute request lifetime after revocation;
/// its certificate must still be no more than sixty seconds old at redemption.
pub fn verify_authentication(
    batch: &CertifiedBatch,
    expected: &AuthenticationRequest,
    trusted_home: Principal,
    trusted_ic_root_der: &[u8],
    now_ms: u64,
) -> Result<AuthenticationResult> {
    ensure(
        batch.schema == 1
            && batch.canister == trusted_home
            && batch.entries.len() == 1
            && batch.certificate.len() <= MAX_PAYLOAD
            && trusted_ic_root_der.len() <= 256,
        Error::IntegrityFailed,
    )?;
    let entry = &batch.entries[0];
    ensure(entry.witness.len() <= MAX_PAYLOAD, Error::QuotaExceeded)?;
    let value = entry.value.as_ref().ok_or(Error::IntegrityFailed)?;
    let result: AuthenticationResult = decode_canonical(value)?;
    let key = authentication_key(&result.account_id, &expected.operation_id);
    ensure(entry.key.as_ref() == key.as_slice(), Error::IntegrityFailed)?;
    let certificate = ic_auth_verifier::parse_certificate_cbor(&batch.certificate)
        .map_err(|_| Error::IntegrityFailed)?;
    ic_auth_verifier::verify_certificate(
        &certificate,
        trusted_home.as_slice(),
        trusted_ic_root_der,
        u128::from(now_ms) * 1_000_000,
        60_000_000_000,
    )
    .map_err(|_| Error::IntegrityFailed)?;
    let LookupResult::Found(time) = certificate.tree.lookup_path([b"time".as_slice()]) else {
        return Err(Error::IntegrityFailed);
    };
    let at_ms = timestamp(time)? / 1_000_000;
    let witness: HashTree =
        cbor2::from_slice(&entry.witness).map_err(|_| Error::IntegrityFailed)?;
    let LookupResult::Found(root) = certificate.tree.lookup_path([
        b"canister".as_slice(),
        trusted_home.as_slice(),
        b"certified_data".as_slice(),
    ]) else {
        return Err(Error::IntegrityFailed);
    };
    ensure(root == witness.digest(), Error::IntegrityFailed)?;
    ensure(
        matches!(witness.lookup_path([key.as_slice()]), LookupResult::Found(bytes) if bytes == value.as_ref()),
        Error::IntegrityFailed,
    )?;
    match_authentication_result(&result, expected, trusted_home, at_ms, now_ms)?;
    Ok(result)
}

#[cfg(test)]
#[path = "authentication_tests.rs"]
mod tests;
