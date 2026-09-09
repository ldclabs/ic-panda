use crate::*;
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};

/// Only algorithms that can produce a formal signature.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SigningAlgorithm {
    Ed25519,
    EcdsaSecp256k1,
}
impl From<SigningAlgorithm> for Algorithm {
    fn from(value: SigningAlgorithm) -> Self {
        match value {
            SigningAlgorithm::Ed25519 => Self::Ed25519,
            SigningAlgorithm::EcdsaSecp256k1 => Self::EcdsaSecp256k1,
        }
    }
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SigningPurpose {
    Statement,
    FileAttestation,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SigningKey {
    pub purpose: SigningPurpose,
    pub algorithm: SigningAlgorithm,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum KeySelector {
    Signing(SigningKey),
    ContentRoot { generation: u64 },
}
impl From<KeySelector> for KeyRequest {
    fn from(value: KeySelector) -> Self {
        match value {
            KeySelector::Signing(key) => Self {
                purpose: match key.purpose {
                    SigningPurpose::Statement => KeyPurpose::Statement,
                    SigningPurpose::FileAttestation => KeyPurpose::FileAttestation,
                },
                algorithm: key.algorithm.into(),
                generation: 1,
            },
            KeySelector::ContentRoot { generation } => Self {
                purpose: KeyPurpose::ContentRoot,
                algorithm: Algorithm::VetKdBls12381,
                generation,
            },
        }
    }
}

/// Small reference obtained from the authenticated key descriptor.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SigningKeyRef {
    pub algorithm: SigningAlgorithm,
    pub kid: ByteBuf,
    pub public_key_fingerprint: Hash,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SignRequest {
    pub account_id: AccountId,
    pub key: SigningKeyRef,
    pub statement: Statement,
    /// Checked browser origin; part of device approval, not of the portable statement.
    pub origin: String,
    pub max_cycles: u128,
    pub approval: Approval,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum RootTarget {
    Current { generation: u64 },
    Candidate { generation: u64, op_id: OpId },
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DeriveRootRequest {
    pub account_id: AccountId,
    pub target: RootTarget,
    pub transport_public_key: ByteArray<48>,
    pub max_cycles: u128,
    pub approval: Approval,
}
impl DeriveRootRequest {
    pub fn into_execution(self) -> ExecuteRequest {
        let (generation, root_op_id) = match self.target {
            RootTarget::Current { generation } => (generation, None),
            RootTarget::Candidate { generation, op_id } => (generation, Some(op_id)),
        };
        ExecuteRequest {
            account_id: self.account_id,
            max_cycles: self.max_cycles,
            approval: self.approval,
            kind: ExecutionKind::Derive {
                generation,
                root_op_id,
                transport_key: self.transport_public_key.to_vec().into(),
            },
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeyPurpose {
    FileAttestation,
    Statement,
    ContentRoot,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Algorithm {
    Ed25519,
    EcdsaSecp256k1,
    VetKdBls12381,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MasterKey {
    pub algorithm: Algorithm,
    pub key_name: String,
    pub expected_fingerprint: Hash,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CoseInit {
    pub issuer_namespace: String,
    pub environment: Environment,
    pub executing_canister: Principal,
    pub initial_home_user: Principal,
    pub derivation_version: u16,
    pub masters: Vec<MasterKey>,
    pub daily_executions: u32,
    pub daily_cycles: u128,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Initialization {
    Uninitialized,
    Initializing,
    Ready,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct KeyDescriptor {
    pub key_id: ByteBuf,
    pub account_id: AccountId,
    pub purpose: KeyPurpose,
    pub algorithm: Algorithm,
    pub home_cose: Principal,
    pub master_key_name: String,
    pub environment: Environment,
    pub derivation_version: u16,
    pub key_generation: u64,
    pub public_key: ByteBuf,
    pub public_key_fingerprint: Hash,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct KeyRequest {
    pub purpose: KeyPurpose,
    pub algorithm: Algorithm,
    pub generation: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ExecutionKind {
    Sign {
        key: KeyRequest,
        to_be_signed: ByteBuf,
        public_key_fingerprint: Hash,
        origin: String,
    },
    Derive {
        generation: u64,
        root_op_id: Option<OpId>,
        transport_key: ByteBuf,
    },
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecuteRequest {
    pub account_id: AccountId,
    pub kind: ExecutionKind,
    pub max_cycles: u128,
    pub approval: Approval,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionGrant {
    pub account_id: AccountId,
    pub home_user: Principal,
    pub home_cose: Principal,
    pub request_id: OpId,
    pub execution_sequence: u64,
    pub security_epoch: u64,
    pub device_id: Hash,
    #[serde(default)]
    pub device_sequence: u64,
    pub approved_at: u64,
    pub expires_at: u64,
    pub kind: ExecutionKind,
    pub max_cycles: u128,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ExecutionStatus {
    Authorized,
    Executing,
    Completed,
    Failed,
    Unknown,
    ResultExpired,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionResult {
    pub request_id: OpId,
    pub outcome: ExecutionOutcome,
    pub charged_cycles: u128,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ExecutionOutcome {
    Authorized,
    Executing,
    Completed(Box<ExecutionOutput>),
    Failed(Error),
    Unknown(Error),
    ResultExpired,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ExecutionOutput {
    Signature {
        artifact: SignedArtifact,
        key: KeyDescriptor,
    },
    EncryptedRootKey {
        encrypted_key: ByteBuf,
        key: KeyDescriptor,
    },
}
impl ExecutionOutput {
    pub fn bytes(&self) -> &ByteBuf {
        match self {
            Self::Signature { artifact, .. } => &artifact.cose_sign1,
            Self::EncryptedRootKey { encrypted_key, .. } => encrypted_key,
        }
    }
    pub fn key(&self) -> &KeyDescriptor {
        match self {
            Self::Signature { key, .. } | Self::EncryptedRootKey { key, .. } => key,
        }
    }
}
impl ExecutionResult {
    pub fn status(&self) -> ExecutionStatus {
        match self.outcome {
            ExecutionOutcome::Authorized => ExecutionStatus::Authorized,
            ExecutionOutcome::Executing => ExecutionStatus::Executing,
            ExecutionOutcome::Completed(_) => ExecutionStatus::Completed,
            ExecutionOutcome::Failed(_) => ExecutionStatus::Failed,
            ExecutionOutcome::Unknown(_) => ExecutionStatus::Unknown,
            ExecutionOutcome::ResultExpired => ExecutionStatus::ResultExpired,
        }
    }
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.outcome,
            ExecutionOutcome::Completed(_)
                | ExecutionOutcome::Failed(_)
                | ExecutionOutcome::ResultExpired
        )
    }
    pub fn output(&self) -> Result<&ExecutionOutput> {
        match &self.outcome {
            ExecutionOutcome::Completed(output) => Ok(output),
            ExecutionOutcome::Failed(e) | ExecutionOutcome::Unknown(e) => Err(e.clone()),
            ExecutionOutcome::ResultExpired => Err(Error::ResultExpired),
            _ => Err(Error::Pending),
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct KeyState {
    pub config: CoseInit,
    pub initialization: Initialization,
    pub fingerprints: Vec<Hash>,
    pub error: Option<String>,
}

/// Certified execution evidence binding an operation to the signed bytes and key.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionReceipt {
    pub schema: u16,
    pub account_id: AccountId,
    pub issuer: String,
    pub request_id: OpId,
    pub device_id: Hash,
    pub security_epoch: u64,
    pub approved_at: u64,
    pub expires_at: u64,
    pub origin: String,
    pub max_cycles: u128,
    pub to_be_signed_digest: Hash,
    pub public_key_fingerprint: Hash,
    pub status: ExecutionStatus,
    pub signature_digest: Option<Hash>,
}
