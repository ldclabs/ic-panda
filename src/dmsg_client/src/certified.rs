//! Certified query answers: the IC certificate under the trusted root, its
//! time within 60 seconds, the canister's certified data, and the witness
//! for one requested path. A requested leaf is either present with exactly
//! the returned bytes or proven absent; nothing else is accepted.

use candid::Principal;
use dmsg_types::{CertifiedBatch, Error, Result, ensure};
use ic_certification_v3::{HashTree, LookupResult};

/// The leaf of `entry_key` in `batch`, at `path` in the canister's tree
/// (the key itself when `path` is empty): `None` for a proven absence.
///
/// # Errors
/// Anything uncertified, stale or inconsistent returns `Error::IntegrityFailed`.
pub fn certified_path(
    batch: &CertifiedBatch,
    entry_key: &[u8],
    path: &[&[u8]],
    canister: Principal,
    root_key: &[u8],
    now: u64,
) -> Result<Option<Vec<u8>>> {
    ensure(
        batch.schema == 1
            && batch.canister == canister
            && batch.certificate.len() <= 65_536
            && root_key.len() <= 256,
        Error::IntegrityFailed,
    )?;
    let entry = batch
        .entries
        .iter()
        .find(|e| e.key.as_ref() == entry_key)
        .ok_or(Error::IntegrityFailed)?;
    ensure(entry.witness.len() <= 262_144, Error::QuotaExceeded)?;
    let certificate = ic_auth_verifier::parse_certificate_cbor(&batch.certificate)
        .map_err(|_| Error::IntegrityFailed)?;
    ic_auth_verifier::verify_certificate(
        &certificate,
        canister.as_slice(),
        root_key,
        u128::from(now) * 1_000_000,
        60_000_000_000,
    )
    .map_err(|_| Error::IntegrityFailed)?;
    let LookupResult::Found(root) = certificate.tree.lookup_path([
        b"canister".as_slice(),
        canister.as_slice(),
        b"certified_data".as_slice(),
    ]) else {
        return Err(Error::IntegrityFailed);
    };
    let witness: HashTree =
        cbor2::from_slice(&entry.witness).map_err(|_| Error::IntegrityFailed)?;
    ensure(root == witness.digest(), Error::IntegrityFailed)?;
    let lookup = match path.is_empty() {
        true => witness.lookup_path([entry_key]),
        false => witness.lookup_path(path.iter().copied()),
    };
    match lookup {
        LookupResult::Found(value) => {
            ensure(
                entry.value.as_deref().map(|v| v.as_slice()) == Some(value),
                Error::IntegrityFailed,
            )?;
            Ok(Some(value.to_vec()))
        }
        LookupResult::Absent => {
            ensure(entry.value.is_none(), Error::IntegrityFailed)?;
            Ok(None)
        }
        _ => Err(Error::IntegrityFailed),
    }
}

/// [`certified_path`] of a single-label key.
///
/// # Errors
/// As [`certified_path`].
pub fn certified_leaf(
    batch: &CertifiedBatch,
    key: &[u8],
    canister: Principal,
    root_key: &[u8],
    now: u64,
) -> Result<Option<Vec<u8>>> {
    certified_path(batch, key, &[], canister, root_key, now)
}
