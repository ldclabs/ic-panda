//! Binding checks complement (never replace) authentication of an ICP certificate.
use crate::*;
use dmsg_types::{cose::*, *};

pub fn execution_receipt_key(account: &AccountId, request: OpId) -> Vec<u8> {
    [
        b"execution/".as_slice(),
        account.as_slice(),
        request.as_slice(),
    ]
    .concat()
}

pub fn artifact_signing_bytes(artifact: &SignedArtifact) -> Result<Vec<u8>> {
    verify_and_parse_artifact(artifact)?.signing_bytes()
}

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

/// The caller must first authenticate the receipt's certificate, canister and witness.
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
