use crate::*;
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeyPurpose {
    Identity,
    FileAttestation,
    Statement,
    ProviderController,
    ContentRoot,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Algorithm {
    Ed25519,
    Bip340,
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
    pub environment: Environment,
    pub executing_canister: Principal,
    pub initial_home_user: Principal,
    pub derivation_version: u16,
    pub masters: Vec<MasterKey>,
    pub daily_executions: u32,
    pub daily_cycles: u128,
}
impl CoseInit {
    pub fn validate(&self, id: Principal) -> Result<()> {
        ensure(
            self.executing_canister == id && self.derivation_version == 1,
            invalid("immutable key home/version"),
        )?;
        authenticated(self.initial_home_user)?;
        ensure(
            !self.masters.is_empty() && self.masters.len() <= 4,
            invalid("masters"),
        )?;
        for (i, k) in self.masters.iter().enumerate() {
            ensure(
                !self.masters[..i].iter().any(|x| x.algorithm == k.algorithm),
                invalid("duplicate algorithm"),
            )?;
            if self.environment == Environment::Production {
                ensure(k.key_name == "key_1", invalid("production requires key_1"))?;
                nonzero(&k.expected_fingerprint)?;
            } else {
                ensure(
                    matches!(k.key_name.as_str(), "key_1" | "test_key_1" | "dfx_test_key"),
                    invalid("key name"),
                )?;
            }
        }
        ensure(
            self.daily_executions > 0 && self.daily_cycles > 0,
            invalid("hard budgets"),
        )
    }
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Initialization {
    Uninitialized,
    Initializing,
    Ready,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct KeyDescriptor {
    pub key_id: Hash,
    pub subject: SubjectId,
    pub purpose: KeyPurpose,
    pub algorithm: Algorithm,
    pub home_cose: Principal,
    pub master_key_name: String,
    pub environment: Environment,
    pub derivation_version: u16,
    pub key_generation: u64,
    pub provider: Option<String>,
    pub public_key: ByteBuf,
    pub public_key_fingerprint: Hash,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct KeyRequest {
    pub purpose: KeyPurpose,
    pub algorithm: Algorithm,
    pub generation: u64,
    pub provider: Option<String>,
}
impl KeyRequest {
    pub fn validate(&self) -> Result<()> {
        ensure(self.generation > 0, invalid("generation"))?;
        ensure(self.provider.is_none(), Error::UnsupportedProtocol)?;
        match self.purpose {
            KeyPurpose::ContentRoot => ensure(
                self.algorithm == Algorithm::VetKdBls12381,
                Error::UnsupportedProtocol,
            ),
            KeyPurpose::Statement | KeyPurpose::FileAttestation => ensure(
                self.generation == 1 && self.algorithm != Algorithm::VetKdBls12381,
                Error::UnsupportedProtocol,
            ),
            _ => Err(Error::UnsupportedProtocol), // external protocol release gates
        }
    }
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FormalPayload {
    pub schema: u16,
    pub subject: SubjectId,
    pub request_id: OpId,
    pub origin: String,
    pub audience: String,
    pub expires_at: u64,
    pub body: FormalBody,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub enum FormalBody {
    FileAttestation {
        sha256: Hash,
        size: u64,
        version: Hash,
        project: String,
    },
    Statement {
        text: String,
    },
}
pub fn validate_payload(
    bytes: &[u8],
    subject: SubjectId,
    request_id: OpId,
    key: &KeyRequest,
    expires_at: u64,
) -> Result<FormalPayload> {
    key.validate()?;
    let p: FormalPayload = decode_canonical(bytes)?;
    ensure(
        p.schema == 1
            && p.subject == subject
            && p.request_id == request_id
            && p.expires_at == expires_at,
        Error::IntegrityFailed,
    )?;
    let origin = url::Url::parse(&p.origin).map_err(|_| invalid("origin"))?;
    let extension_origin = p
        .origin
        .strip_prefix("chrome-extension://")
        .is_some_and(|id| id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b)));
    ensure(
        ((origin.scheme() == "https" && origin.origin().ascii_serialization() == p.origin)
            || extension_origin)
            && p.origin.len() <= 256,
        invalid("origin"),
    )?;
    ensure(
        !p.audience.is_empty() && p.audience.len() <= 256,
        invalid("audience"),
    )?;
    match &p.body {
        FormalBody::FileAttestation {
            sha256,
            size,
            version,
            project,
        } => {
            nonzero(sha256)?;
            nonzero(version)?;
            ensure(
                *size <= 100 * 1024 * 1024
                    && !project.is_empty()
                    && project.len() <= 256
                    && key.purpose == KeyPurpose::FileAttestation,
                Error::UnsupportedProtocol,
            )?;
        }
        FormalBody::Statement { text } => ensure(
            !text.is_empty() && text.len() <= 4096 && key.purpose == KeyPurpose::Statement,
            Error::UnsupportedProtocol,
        )?,
    }
    Ok(p)
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ExecutionKind {
    Sign {
        key: KeyRequest,
        canonical_payload: ByteBuf,
    },
    Derive {
        generation: u64,
        root_op_id: Option<OpId>,
        transport_key: ByteBuf,
    },
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecuteRequest {
    pub subject: SubjectId,
    pub kind: ExecutionKind,
    pub max_cycles: u128,
    pub approval: Approval,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionGrant {
    pub subject: SubjectId,
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
    pub status: ExecutionStatus,
    pub result: Option<ByteBuf>,
    pub key: Option<KeyDescriptor>,
    pub charged_cycles: u128,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AuthorizedExecution {
    pub grant: ExecutionGrant,
    pub command_digest: Hash,
    pub result: ExecutionResult,
}
