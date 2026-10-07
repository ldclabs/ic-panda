//! Binding checks complement (never replace) authentication of an ICP certificate.
use crate::*;
use dmsg_types::*;

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

/// Verify an artifact and match it against an already authenticated execution receipt.
///
/// The caller must first authenticate the IC certificate, expected user canister,
/// requested account/request path, witness and leaf bytes. This function requires
/// schema 2, then matches issuer, signing-bytes SHA-256, public-key thumbprint
/// and raw-signature SHA-256. It does not independently check account ID,
/// request ID, origin, approval expiry or external application permissions.
///
/// # Errors
/// Propagates artifact verification errors; unsupported receipt schema or
/// mismatched bindings return `Error::IntegrityFailed`.
pub fn match_execution_receipt(
    artifact: &SignedArtifact,
    receipt: &ExecutionReceipt,
) -> Result<()> {
    ensure(receipt.schema == 2, Error::IntegrityFailed)?;
    let verified = verify_and_parse_artifact(artifact)?;
    ensure(
        verified.statement.issuer == receipt.issuer
            && sha256(&verified.signing_bytes()?) == receipt.to_be_signed_digest
            && thumbprint(&verified.key)? == receipt.public_key_fingerprint
            && sha256(verified.message.signature()) == receipt.signature_digest,
        Error::IntegrityFailed,
    )
}
