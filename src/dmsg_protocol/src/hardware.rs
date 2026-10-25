//! Approval keys: P-256 keys a device's hardware keeps, used only to sign
//! the hardware commitment of a device approval. The user home requires
//! that signature for the purposes an account's hardware policy names.

use crate::digest;
use candid::Principal;
use dmsg_types::{
    AccountId, Error, HardwareProof, Hash, Result, ensure,
    integration::ApprovalPurpose,
    user::{AccountCommand, ApprovalKey, HardwarePurpose},
};
use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
use serde_bytes::Bytes;

/// The identifier of a SEC1 compressed P-256 approval key,
/// `digest("dmsg/hardware-key/v1", public_key)`.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for anything but a compressed point on
/// the curve.
pub fn approval_key_id(public_key: &[u8]) -> Result<Hash> {
    ensure(public_key.len() == 33, Error::IntegrityFailed)?;
    VerifyingKey::from_sec1_bytes(public_key).map_err(|_| Error::IntegrityFailed)?;
    Ok(digest("dmsg/hardware-key/v1", &Bytes::new(public_key)))
}

/// What an approval key signs for an approval: its `approval_message` and
/// the key, `digest("dmsg/hardware-approval/v1", (approval_message, key_id))`.
/// The device signs the same `approval_message` with Ed25519.
pub fn hardware_message(approval_message: Hash, key_id: Hash) -> Hash {
    digest("dmsg/hardware-approval/v1", &(approval_message, key_id))
}

/// What a new approval key signs to prove possession when it is registered
/// for `device_id`.
pub fn approval_key_message(
    canister: Principal,
    account_id: &AccountId,
    device_id: Hash,
    public_key: &[u8],
) -> Hash {
    digest(
        "dmsg/approval-key/v1",
        &(canister, account_id, device_id, Bytes::new(public_key)),
    )
}

/// Verify a P-256 ECDSA signature over SHA-256 of `message`. A high `s`
/// is refused, so each proof has one encoding.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for a malformed key, a high `s` or a
/// signature that does not verify.
pub fn verify_p256(public_key: &[u8], message: &[u8], signature: &[u8; 64]) -> Result<()> {
    let key = VerifyingKey::from_sec1_bytes(public_key).map_err(|_| Error::IntegrityFailed)?;
    let signature = Signature::from_slice(signature).map_err(|_| Error::IntegrityFailed)?;
    ensure(signature.normalize_s() == signature, Error::IntegrityFailed)?;
    key.verify(message, &signature)
        .map_err(|_| Error::IntegrityFailed)
}

/// Check `proof` for an approval whose `approval_message` is `message`,
/// against the approving device's registered `key`.
///
/// # Errors
/// Returns `Error::Forbidden` for another key and `Error::IntegrityFailed`
/// for a signature that does not verify.
pub fn verify_hardware(key: &ApprovalKey, message: Hash, proof: &HardwareProof) -> Result<()> {
    ensure(proof.key_id == key.key_id, Error::Forbidden)?;
    verify_p256(
        &key.public_key,
        hardware_message(message, key.key_id).as_slice(),
        &proof.signature,
    )
}

/// The purpose an account command falls under; None for the commands no
/// policy protects: root rotation, disputing a recovery, the principal's
/// display document and controller labels, and marking a controller
/// compromised.
pub fn command_purpose(command: &AccountCommand) -> Option<HardwarePurpose> {
    use AccountCommand::*;
    match command {
        SetApprovalKey { .. }
        | SetHardwarePolicy { .. }
        | SetPolicy { .. }
        | SetRecoveryDelay { .. }
        | BindAuth { .. }
        | RemoveAuth { .. } => Some(HardwarePurpose::AccountControl),
        AddDevice { .. } | RevokeDevice { .. } | SetDeviceCapabilities { .. } => {
            Some(HardwarePurpose::DeviceControl)
        }
        EnablePrincipal { .. } | RegisterController { .. } | RetireController { .. } => {
            Some(HardwarePurpose::PrincipalControl)
        }
        AuthorizeHandle { .. } => Some(HardwarePurpose::Payment),
        ReserveRoot { .. }
        | CommitRoot { .. }
        | DisputeRecovery { .. }
        | UpdatePrincipal { .. }
        | MarkControllerCompromised { .. }
        | RenameController { .. } => None,
    }
}

/// The purpose an application approval falls under: payments, not team
/// wallets, which their cosigners guard.
pub fn application_purpose(purpose: &ApprovalPurpose) -> Option<HardwarePurpose> {
    match purpose {
        ApprovalPurpose::CashCheckout | ApprovalPurpose::PandaSubscription => {
            Some(HardwarePurpose::Payment)
        }
        ApprovalPurpose::WalletCreate
        | ApprovalPurpose::WalletProposal
        | ApprovalPurpose::WalletJoin => None,
    }
}

/// Check a hardware policy as proposed: empty, or each purpose once with
/// the three control purposes among them.
///
/// # Errors
/// Returns `Error::InvalidInput` otherwise.
pub fn validate_hardware_policy(purposes: &[HardwarePurpose]) -> Result<()> {
    let unique = purposes
        .iter()
        .enumerate()
        .all(|(i, p)| !purposes[..i].contains(p));
    dmsg_types::ensure_valid(
        unique
            && (purposes.is_empty()
                || dmsg_types::user::CONTROL_PURPOSES
                    .iter()
                    .all(|p| purposes.contains(p))),
        "hardware policy",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::SigningKey;

    fn key() -> SigningKey {
        SigningKey::from_slice(&[7; 32]).unwrap()
    }

    fn public(key: &SigningKey) -> Vec<u8> {
        key.verifying_key().to_sec1_point(true).as_bytes().to_vec()
    }

    #[test]
    fn approval_key_ids_need_a_compressed_point() {
        let public = public(&key());
        assert_eq!(public.len(), 33);
        let id = approval_key_id(&public).unwrap();
        assert_eq!(id, digest("dmsg/hardware-key/v1", &Bytes::new(&public)));
        let uncompressed = key()
            .verifying_key()
            .to_sec1_point(false)
            .as_bytes()
            .to_vec();
        assert_eq!(approval_key_id(&uncompressed), Err(Error::IntegrityFailed));
        let mut off_curve = public.clone();
        off_curve[1..].fill(0xff);
        assert_eq!(approval_key_id(&off_curve), Err(Error::IntegrityFailed));
    }

    #[test]
    fn proofs_bind_the_approval_and_the_key_with_a_low_s() {
        use p256::ecdsa::signature::Signer;
        let signing = key();
        let public = public(&signing);
        let registered = ApprovalKey {
            key_id: approval_key_id(&public).unwrap(),
            public_key: public.into(),
            registered_at: 1,
            registered_by: Hash::new([1; 32]),
        };
        let message = Hash::new([3; 32]);
        let signature: Signature =
            signing.sign(hardware_message(message, registered.key_id).as_slice());
        let low = signature.normalize_s();
        let proof = HardwareProof {
            key_id: registered.key_id,
            signature: dmsg_types::P256Signature::new(low.to_bytes().into()),
        };
        assert_eq!(verify_hardware(&registered, message, &proof), Ok(()));
        // Another approval, another key ID or the high-s twin fail.
        assert_eq!(
            verify_hardware(&registered, Hash::new([4; 32]), &proof),
            Err(Error::IntegrityFailed)
        );
        let other = HardwareProof {
            key_id: Hash::new([5; 32]),
            ..proof.clone()
        };
        assert_eq!(
            verify_hardware(&registered, message, &other),
            Err(Error::Forbidden)
        );
        let (r, s) = low.split_scalars();
        let high = Signature::from_scalars(r, -s).unwrap();
        let high = HardwareProof {
            signature: dmsg_types::P256Signature::new(high.to_bytes().into()),
            ..proof
        };
        assert_eq!(
            verify_hardware(&registered, message, &high),
            Err(Error::IntegrityFailed)
        );
    }

    #[test]
    fn policies_hold_the_control_purposes_once() {
        use HardwarePurpose::*;
        assert!(validate_hardware_policy(&[]).is_ok());
        assert!(
            validate_hardware_policy(&[AccountControl, DeviceControl, PrincipalControl]).is_ok()
        );
        assert!(
            validate_hardware_policy(&[
                Payment,
                AccountControl,
                DeviceControl,
                PrincipalControl,
                Attestation
            ])
            .is_ok()
        );
        assert!(validate_hardware_policy(&[AccountControl, DeviceControl, Payment]).is_err());
        assert!(
            validate_hardware_policy(&[
                AccountControl,
                DeviceControl,
                PrincipalControl,
                DeviceControl
            ])
            .is_err()
        );
    }
}
