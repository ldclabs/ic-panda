//! Binding checks complement (never replace) authentication of an ICP certificate.
use crate::*;
use cose2::Sign1Message;
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
    verify_artifact(artifact)?;
    let message =
        Sign1Message::from_slice(&artifact.cose_sign1).map_err(|_| Error::IntegrityFailed)?;
    Sign1Message::to_be_signed(
        message.protected_raw(),
        &[],
        message.payload.as_deref().ok_or(Error::IntegrityFailed)?,
    )
    .map_err(|_| Error::IntegrityFailed)
}
pub fn match_signing_result(
    artifact: &SignedArtifact,
    expected_tbs: &[u8],
    fingerprint: Hash,
) -> Result<()> {
    ensure(
        artifact_signing_bytes(artifact)? == expected_tbs
            && key_thumbprint(&artifact.cose_key)? == fingerprint,
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
    let statement = verify_artifact(artifact)?;
    ensure(
        statement.issuer == receipt.issuer
            && sha256(&artifact_signing_bytes(artifact)?) == receipt.to_be_signed_digest
            && key_thumbprint(&artifact.cose_key)? == receipt.public_key_fingerprint
            && Some(signature_digest(&artifact.cose_sign1)?) == receipt.signature_digest,
        Error::IntegrityFailed,
    )
}
