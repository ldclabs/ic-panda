//! Binding checks complement (never replace) authentication of an ICP certificate.
use crate::*;
use dmsg_types::{cose::*, *};

/// Return the single certified-tree path segment for an execution receipt.
///
/// The 54 bytes are `b"execution/" || account[12] || request[32]`, using raw IDs
/// rather than hex/Xid text. Use this expected path when verifying the IC witness.
pub fn execution_receipt_key(account: &AccountId, request: OpId) -> Vec<u8> {
    [
        b"execution/".as_slice(),
        account.as_slice(),
        request.as_slice(),
    ]
    .concat()
}

/// Verify an artifact and reconstruct its exact COSE Sig_structure bytes.
///
/// Preserves the original protected-header bytes and uses empty external AAD.
/// This checks mathematical validity but establishes no issuer/key trust.
///
/// # Errors
/// Propagates profile, key, signature, size and decoding errors from artifact verification.
pub fn artifact_signing_bytes(artifact: &SignedArtifact) -> Result<Vec<u8>> {
    verify_and_parse_artifact(artifact)?.signing_bytes()
}

/// Verify a returned artifact and bind it to expected signing bytes and key thumbprint.
///
/// `expected_tbs` must be the frozen Sig_structure supplied to the signer;
/// `fingerprint` must come from an authenticated source. This does not verify
/// an ICP certificate or the source of either expected value.
///
/// # Errors
/// Propagates artifact verification errors; a bytes/thumbprint mismatch returns
/// `Error::IntegrityFailed`.
pub fn match_signing_result(
    artifact: &SignedArtifact,
    expected_tbs: &[u8],
    fingerprint: Hash,
) -> Result<()> {
    let verified = verify_and_parse_artifact(artifact)?;
    ensure(
        verified.signing_bytes()? == expected_tbs && thumbprint(&verified.key)? == fingerprint,
        Error::IntegrityFailed,
    )
}

/// Verify an artifact and match it against an already authenticated execution receipt.
///
/// The caller must first authenticate the IC certificate, expected user canister,
/// requested account/request path, witness and leaf bytes. This function requires
/// schema 1 and Completed, then matches issuer, signing-bytes SHA-256, public-key
/// thumbprint and raw-signature SHA-256. It does not independently check account ID,
/// request ID, origin, approval expiry or external application permissions.
///
/// # Errors
/// Propagates artifact verification errors; unsupported receipt schema, noncompleted
/// status or mismatched bindings return `Error::IntegrityFailed`.
pub fn match_execution_receipt(
    artifact: &SignedArtifact,
    receipt: &ExecutionReceipt,
) -> Result<()> {
    ensure(
        receipt.schema == 1 && receipt.status == ExecutionStatus::Completed,
        Error::IntegrityFailed,
    )?;
    let verified = verify_and_parse_artifact(artifact)?;
    ensure(
        verified.statement.issuer == receipt.issuer
            && sha256(&verified.signing_bytes()?) == receipt.to_be_signed_digest
            && thumbprint(&verified.key)? == receipt.public_key_fingerprint
            && Some(sha256(verified.message.signature())) == receipt.signature_digest,
        Error::IntegrityFailed,
    )
}
