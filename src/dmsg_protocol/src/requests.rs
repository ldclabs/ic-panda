use crate::*;
use candid::Principal;
use dmsg_types::{cose::*, user::*, *};

pub trait DeviceInputExt {
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

pub trait SignRequestExt {
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

pub trait CoseInitExt {
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

pub trait KeyRequestExt {
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

pub trait ExecuteRequestExt {
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
