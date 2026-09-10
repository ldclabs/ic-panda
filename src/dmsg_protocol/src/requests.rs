use crate::*;
use candid::Principal;
use dmsg_types::{cose::*, user::*, *};

/// Local validation of a device enrollment proposal; no device registration or authorization.
pub trait DeviceInputExt {
    /// Check nonzero device/HPKE bytes, 1..5 unique capabilities and a nonweak Ed25519 key.
    ///
    /// This does not verify proof of possession, fully validate an HPKE point or
    /// apply account-specific role rules.
    ///
    /// # Errors
    /// Invalid identifiers/capabilities return `Error::InvalidInput`; invalid signing
    /// keys return `Error::IntegrityFailed`.
    fn validate(&self) -> Result<()>;
}

impl DeviceInputExt for DeviceInput {
    fn validate(&self) -> Result<()> {
        nonzero(self.device_id.as_slice())?;
        nonzero(self.hpke_pub.as_slice())?;
        ensure_valid(
            !self.capabilities.is_empty() && self.capabilities.len() <= 5,
            "capabilities",
        )?;
        // At most five capabilities: a slice scan avoids a tree allocation.
        ensure_valid(
            self.capabilities
                .iter()
                .enumerate()
                .all(|(i, capability)| !self.capabilities[..i].contains(capability)),
            "duplicate capability",
        )?;
        validate_ed25519_key(self.signing_pub.as_slice())
    }
}

/// Convert the typed signing interface into the low-level approved execution contract.
pub trait SignRequestExt {
    /// Consume the request and prepare its canonical signing bytes and derived purpose.
    ///
    /// Validates origin, statement and kid, fixing signing generation to 1. Preserves
    /// the supplied approval and fingerprint without verifying either. Compute the
    /// execution approval via [`ExecuteRequestExt`] after freezing the request.
    ///
    /// # Errors
    /// Propagates origin and [`prepare_cose`] validation errors.
    fn into_execution(self) -> Result<ExecuteRequest>;
}

impl SignRequestExt for SignRequest {
    fn into_execution(self) -> Result<ExecuteRequest> {
        validate_origin(&self.origin)?;
        let algorithm: Algorithm = self.key.algorithm.into();
        let purpose = statement_purpose(&self.statement);
        let (_, bytes) = prepare_cose(&self.statement, &algorithm, &self.key.kid)?;
        Ok(ExecuteRequest {
            account_id: self.account_id,
            max_cycles: self.max_cycles,
            approval: self.approval,
            kind: ExecutionKind::Sign {
                key: KeyRequest {
                    purpose,
                    algorithm,
                    generation: 1,
                },
                to_be_signed: bytes.into(),
                public_key_fingerprint: self.key.public_key_fingerprint,
                origin: self.origin,
            },
        })
    }
}

/// Validate fixed COSE deployment configuration without accessing ICP master keys.
pub trait CoseInitExt {
    /// Check executing canister against id, derivation version 2, user home, namespace,
    /// 1..3 distinct algorithms, key names and positive budgets. Production requires
    /// key_1 and nonzero master fingerprints; this does not fetch/check actual keys.
    ///
    /// # Errors
    /// Invalid configuration returns `Error::InvalidInput`; excluded user-home
    /// Principals return `Error::AuthRequired`.
    fn validate(&self, id: Principal) -> Result<()>;
}

impl CoseInitExt for CoseInit {
    fn validate(&self, id: Principal) -> Result<()> {
        ensure_valid(
            self.executing_canister == id && self.derivation_version == 2,
            "immutable key home/version",
        )?;
        authenticated(self.initial_home_user)?;
        validate_namespace(&self.issuer_namespace)?;
        ensure_valid(
            !self.masters.is_empty() && self.masters.len() <= 3,
            "masters",
        )?;
        for (i, k) in self.masters.iter().enumerate() {
            ensure_valid(
                !self.masters[..i].iter().any(|x| x.algorithm == k.algorithm),
                "duplicate algorithm",
            )?;
            if self.environment == Environment::Production {
                ensure_valid(k.key_name == "key_1", "production requires key_1")?;
                nonzero(k.expected_fingerprint.as_slice())?;
            } else {
                ensure_valid(
                    matches!(k.key_name.as_str(), "key_1" | "test_key_1" | "dfx_test_key"),
                    "key name",
                )?;
            }
        }
        ensure_valid(
            self.daily_executions > 0 && self.daily_cycles > 0,
            "hard budgets",
        )
    }
}

/// Check supported purpose, algorithm and generation combinations.
pub trait KeyRequestExt {
    /// Require a positive generation, vetKD for ContentRoot, and generation 1 with
    /// Ed25519 or ES256K for formal signing. Does not check account/root existence.
    ///
    /// # Errors
    /// Zero generation returns `Error::InvalidInput`; incompatible combinations
    /// return `Error::UnsupportedProtocol`.
    fn validate(&self) -> Result<()>;
}

impl KeyRequestExt for KeyRequest {
    fn validate(&self) -> Result<()> {
        ensure_valid(self.generation > 0, "generation")?;
        match self.purpose {
            KeyPurpose::ContentRoot => ensure(
                self.algorithm == Algorithm::VetKdBls12381,
                Error::UnsupportedProtocol,
            ),
            KeyPurpose::Statement | KeyPurpose::FileAttestation => ensure(
                self.generation == 1 && self.algorithm != Algorithm::VetKdBls12381,
                Error::UnsupportedProtocol,
            ),
        }
    }
}

/// Build the device approval digest for a frozen signing or root-derivation request.
pub trait ExecuteRequestExt {
    /// Bind kind and max_cycles under dmsg/execute/v3, inside dmsg/device-approval/v2.
    ///
    /// Includes the target user home and replay context via [`approval_message`].
    /// The signature field is ignored. This constructs a digest, not an authorization
    /// check, and performs no request validation or sequence mutation.
    fn approval_message(&self, home_user: Principal) -> Hash;
}
impl ExecuteRequestExt for ExecuteRequest {
    /// Shared by typed sign/root requests: changing the public Candid interface
    /// does not change the approved bytes or key derivation domains.
    fn approval_message(&self, home_user: Principal) -> Hash {
        approval_message(
            home_user,
            &self.account_id,
            "dmsg/execute/v3",
            &(&self.kind, self.max_cycles),
            &self.approval,
        )
    }
}

/// Build the recovery-key reconfirmation digest under `dmsg/recovery-reconfirm/v2`.
///
/// Binds user home, account, recovery nonce, full recovery request and dispute
/// confirmation. Sign the digest with the offline recovery signing key, not a
/// device key. This does not check policy, deadlines, or signature validity.
pub fn recovery_confirmation_message(
    home: Principal,
    account_id: &AccountId,
    nonce: u64,
    request: &RecoveryRequest,
    confirmation: &RecoveryConfirmation,
) -> Hash {
    digest(
        "dmsg/recovery-reconfirm/v2",
        &(home, account_id, nonce, request, confirmation),
    )
}

#[cfg(test)]
mod tests;
