//! ICP formal signing, vetKD derivation, execution results and key provenance.
//!
//! Client requests go to the user home for authorization; execution grants are
//! restricted user-to-COSE contracts. Construct approvals with `dmsg_protocol`.
//! Business times are Unix milliseconds; costs are ICP cycles.
use crate::*;
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};

/// Only algorithms that can produce a formal signature.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SigningAlgorithm {
    /// Ed25519 formal signatures (COSE algorithm -19).
    Ed25519,
    /// ECDSA over secp256k1 (COSE ES256K, -47).
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
/// Domain of a formal document signing key; derived from the content profile.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SigningPurpose {
    /// Key domain for text statements.
    Statement,
    /// Key domain for SHA-256 document attestations.
    FileAttestation,
}
/// Formal signing purpose and algorithm, with generation fixed to 1 on conversion.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SigningKey {
    /// Key usage domain; part of key derivation.
    pub purpose: SigningPurpose,
    /// Cryptographic algorithm selected for this key or operation.
    pub algorithm: SigningAlgorithm,
}
/// Typed selector separating formal signing keys from vetKD content roots.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum KeySelector {
    /// Purpose-specific formal signing key.
    Signing(SigningKey),
    /// Content-root derivation domain selected by generation.
    ContentRoot {
        /// Content-root generation to derive.
        generation: u64,
    },
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
    /// Cryptographic algorithm selected for this key or operation.
    pub algorithm: SigningAlgorithm,
    /// Opaque COSE key identifier from an authenticated descriptor.
    pub kid: ByteBuf,
    /// RFC 9679 SHA-256 thumbprint of required public COSE key parameters.
    pub public_key_fingerprint: Hash,
}
/// Formal signing request submitted to the user canister.
/// Freeze the statement, authenticated key reference, origin and cycle limit
/// before computing the approval. The portable statement has no approval deadline.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SignRequest {
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// Algorithm, kid and thumbprint from the authenticated signing-key descriptor.
    pub key: SigningKeyRef,
    /// Prepared or parsed document claims and content.
    pub statement: Statement,
    /// Checked browser origin; part of device approval, not of the portable statement.
    pub origin: String,
    /// Maximum ICP cycles approved for this execution; not a token amount.
    pub max_cycles: u128,
    /// Device approval binding the complete request and replay context.
    pub approval: Approval,
}

/// Select the committed root or the candidate of a reserved root operation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum RootTarget {
    /// Currently committed content root.
    Current {
        /// Committed root generation.
        generation: u64,
    },
    /// Candidate root associated with an active reservation.
    Candidate {
        /// Reserved next root generation.
        generation: u64,
        /// Operation ID of the root reservation.
        op_id: OpId,
    },
}
/// Request a vetKD root encrypted to a caller-generated transport public key.
/// The result is encrypted key material, not a plaintext vault key.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DeriveRootRequest {
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// Committed or reserved candidate root to derive.
    pub target: RootTarget,
    /// 48-byte compressed vetKD transport public key; protocol validation checks the point.
    pub transport_public_key: ByteArray<48>,
    /// Maximum ICP cycles approved for this execution; not a token amount.
    pub max_cycles: u128,
    /// Device approval binding the complete request and replay context.
    pub approval: Approval,
}
impl DeriveRootRequest {
    /// Convert the typed root target to an execution request without validating or authorizing it.
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

/// Domain separation for formal signatures and content-root derivation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeyPurpose {
    /// Key domain for SHA-256 document attestations.
    FileAttestation,
    /// Key domain for text statements.
    Statement,
    /// Content-root derivation domain, selected by generation.
    ContentRoot,
}
/// Supported chain-key operations; vetKD derives keys and cannot sign documents.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Algorithm {
    /// Ed25519 formal signatures (COSE algorithm -19).
    Ed25519,
    /// ECDSA over secp256k1 (COSE ES256K, -47).
    EcdsaSecp256k1,
    /// BLS12-381 vetKD encrypted key derivation; not a document-signature algorithm.
    VetKdBls12381,
}
/// Pinned ICP master-key configuration checked during COSE initialization.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MasterKey {
    /// Cryptographic algorithm selected for this key or operation.
    pub algorithm: Algorithm,
    /// ICP management-canister master-key name.
    pub key_name: String,
    /// SHA-256 of raw management public-key bytes, not a COSE thumbprint.
    /// A zero pin is allowed only outside Production.
    pub expected_fingerprint: Hash,
}
/// Deployment configuration for the COSE executor.
/// Production validation requires fixed master names and nonzero fingerprints;
/// changing derivation inputs changes key identity.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CoseInit {
    /// Fixed issuer URI prefix; ends in `/` or `:` with no query or fragment.
    pub issuer_namespace: String,
    /// Deployment domain used in validation and derivation.
    pub environment: Environment,
    /// Expected identity of this COSE executor.
    pub executing_canister: Principal,
    /// User canister authorized to supply execution grants.
    pub initial_home_user: Principal,
    /// Key derivation format version; current public protocol uses 2.
    pub derivation_version: u16,
    /// Configured algorithm/master-key pins.
    pub masters: Vec<MasterKey>,
    /// Maximum executions in a daily budget window.
    pub daily_executions: u32,
    /// Maximum ICP cycles in a daily budget window.
    pub daily_cycles: u128,
}

/// Readiness of configured chain-key public keys.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Initialization {
    /// Required material has not been initialized.
    Uninitialized,
    /// Public-key initialization is in progress.
    Initializing,
    /// Required material is initialized and ready for use.
    Ready,
}
/// Public key provenance and derivation identity.
/// Authenticate its source before using it to bind a key to an account.
/// Key identity depends on the home canister and derivation configuration.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct KeyDescriptor {
    /// COSE thumbprint bytes for formal signing; domain-separated derivation
    /// identifier for vetKD. Treat as opaque.
    pub key_id: ByteBuf,
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// Key usage domain; part of key derivation.
    pub purpose: KeyPurpose,
    /// Cryptographic algorithm selected for this key or operation.
    pub algorithm: Algorithm,
    /// Fixed COSE canister responsible for key derivation and execution.
    pub home_cose: Principal,
    /// ICP master-key name used for this derived key.
    pub master_key_name: String,
    /// Deployment domain used in validation and derivation.
    pub environment: Environment,
    /// Key derivation format version; current public protocol uses 2.
    pub derivation_version: u16,
    /// Generation included in key derivation; formal signing currently uses 1.
    pub key_generation: u64,
    /// Raw Ed25519 key (32 bytes), SEC1 secp256k1 key, or vetKD public key
    /// (96 bytes). This is not an encoded COSE_Key.
    pub public_key: ByteBuf,
    /// RFC 9679 SHA-256 thumbprint for signing keys; SHA-256 of raw public
    /// key bytes for vetKD.
    pub public_key_fingerprint: Hash,
}
/// Low-level key derivation selector. Prefer [`KeySelector`] when constructing requests.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct KeyRequest {
    /// Key usage domain; part of key derivation.
    pub purpose: KeyPurpose,
    /// Cryptographic algorithm selected for this key or operation.
    pub algorithm: Algorithm,
    /// Positive key generation; formal signing requires 1, content roots use
    /// the requested root generation.
    pub generation: u64,
}

/// Exact operation covered by device approval and execution authorization.
/// This low-level contract is not an unrestricted raw-signing API.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ExecutionKind {
    /// Formal document signing with fixed key, prepared bytes and origin.
    Sign {
        /// Purpose, algorithm and generation of the formal signing key.
        key: KeyRequest,
        /// Exact canonical COSE Sig_structure bytes; not a raw document hash.
        to_be_signed: ByteBuf,
        /// RFC 9679 SHA-256 thumbprint of required public COSE key parameters.
        public_key_fingerprint: Hash,
        /// Checked browser origin bound by the device approval.
        origin: String,
    },
    /// Content-root derivation encrypted to a vetKD transport key.
    Derive {
        /// Generation of the selected root or recovery material.
        generation: u64,
        /// Reservation operation ID for a candidate root; None for the committed root.
        root_op_id: Option<OpId>,
        /// 48-byte compressed vetKD transport public key after request conversion.
        transport_key: ByteBuf,
    },
}
/// Operation submitted for account policy checks and device authorization.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecuteRequest {
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// Exact signing or root-derivation operation covered by approval.
    pub kind: ExecutionKind,
    /// Maximum ICP cycles approved for this execution; not a token amount.
    pub max_cycles: u128,
    /// Device approval binding the complete request and replay context.
    pub approval: Approval,
}

/// User-home authorization passed to the configured COSE home.
/// This is a restricted cross-canister contract, not a bearer token that any
/// caller may construct and redeem.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionGrant {
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// User canister authoritative for this account or deployment.
    pub home_user: Principal,
    /// Fixed COSE canister responsible for key derivation and execution.
    pub home_cose: Principal,
    /// Operation identity used to bind approval and reconcile retries.
    pub request_id: OpId,
    /// User-home execution sequence for replay protection.
    pub execution_sequence: u64,
    /// Account security revision used to invalidate stale approvals.
    pub security_epoch: u64,
    /// 32-byte device identifier, distinct from its signing public key.
    pub device_id: Hash,
    /// Device approval sequence consumed by this authorization.
    #[serde(default)]
    pub device_sequence: u64,
    /// Time authorization was accepted, in Unix milliseconds.
    pub approved_at: u64,
    /// Exclusive deadline in Unix milliseconds (`now < expires_at`).
    pub expires_at: u64,
    /// Authorized operation, identical to the approved request.
    pub kind: ExecutionKind,
    /// Maximum ICP cycles approved for this execution; not a token amount.
    pub max_cycles: u128,
}
/// Execution lifecycle without the result payload.
/// Unknown outcomes must be reconciled using the original request ID.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ExecutionStatus {
    /// Authorization is committed; no completed result is yet available.
    Authorized,
    /// Execution has started and may be awaiting an external call.
    Executing,
    /// Execution completed with a retained output.
    Completed,
    /// Execution returned a known failure.
    Failed,
    /// Execution outcome is uncertain; reconcile using the same request ID.
    Unknown,
    /// Retained output is no longer available; this does not authorize replay.
    ResultExpired,
}
/// Query/retry view of an execution and its charged cycles.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionResult {
    /// Operation identity used to bind approval and reconcile retries.
    pub request_id: OpId,
    /// Execution state with any available output or error.
    pub outcome: ExecutionOutcome,
    /// ICP cycles charged for this execution.
    pub charged_cycles: u128,
}
/// Execution lifecycle carrying either output or failure information.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ExecutionOutcome {
    /// Authorization is committed; no completed result is yet available.
    Authorized,
    /// Execution has started and may be awaiting an external call.
    Executing,
    /// Execution completed with a retained output.
    Completed(Box<ExecutionOutput>),
    /// Execution returned a known failure.
    Failed(Error),
    /// Execution outcome is uncertain; reconcile using the same request ID.
    Unknown(Error),
    /// Retained output is no longer available; this does not authorize replay.
    ResultExpired,
}
/// Successful formal signature or encrypted vetKD derivation result.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ExecutionOutput {
    /// Portable signed document and its key provenance.
    Signature {
        /// Portable COSE document and public key.
        artifact: SignedArtifact,
        /// Public descriptor of the key used to produce this output.
        key: KeyDescriptor,
    },
    /// Encrypted vetKD output and the derivation-key descriptor.
    EncryptedRootKey {
        /// vetKD result encrypted to the supplied transport public key.
        encrypted_key: ByteBuf,
        /// Public descriptor of the key used to produce this output.
        key: KeyDescriptor,
    },
}
impl ExecutionOutput {
    /// Borrow the COSE_Sign1 bytes or encrypted vetKD bytes, according to the variant.
    pub fn bytes(&self) -> &ByteBuf {
        match self {
            Self::Signature { artifact, .. } => &artifact.cose_sign1,
            Self::EncryptedRootKey { encrypted_key, .. } => encrypted_key,
        }
    }
    /// Borrow the public descriptor attached to either output variant.
    pub fn key(&self) -> &KeyDescriptor {
        match self {
            Self::Signature { key, .. } | Self::EncryptedRootKey { key, .. } => key,
        }
    }
}
impl ExecutionResult {
    /// Project the outcome to its payload-free lifecycle status.
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
    /// Whether this result is Completed, Failed or ResultExpired.
    /// Unknown is deliberately nonterminal and must be reconciled.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.outcome,
            ExecutionOutcome::Completed(_)
                | ExecutionOutcome::Failed(_)
                | ExecutionOutcome::ResultExpired
        )
    }
    /// Borrow completed output.
    ///
    /// # Errors
    /// Returns the stored failure/unknown error, ResultExpired for pruned output,
    /// or Pending while authorization/execution is in progress.
    pub fn output(&self) -> Result<&ExecutionOutput> {
        match &self.outcome {
            ExecutionOutcome::Completed(output) => Ok(output),
            ExecutionOutcome::Failed(e) | ExecutionOutcome::Unknown(e) => Err(e.clone()),
            ExecutionOutcome::ResultExpired => Err(Error::ResultExpired),
            _ => Err(Error::Pending),
        }
    }
}

/// COSE deployment configuration and master-key initialization diagnostics.
#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct KeyState {
    /// Deployment configuration used by the executor.
    pub config: CoseInit,
    /// Master public-key initialization state.
    pub initialization: Initialization,
    /// SHA-256 of raw initialized master public keys, in config.masters order.
    pub fingerprints: Vec<Hash>,
    /// Initialization diagnostic, when one is available.
    pub error: Option<String>,
}

/// Certified execution evidence binding an operation to the signed bytes and key.
/// Current schema is 1; the single leaf path is `b"execution/" || account_id || request_id`.
/// Authenticate the certificate and witness before matching these fields to an artifact.
/// This records dMsg authorization, not external application permissions or a TSA timestamp.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionReceipt {
    /// Version of this public certified-leaf format, not the storage schema.
    pub schema: u16,
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// Canonical absolute issuer URI identifying the signer.
    pub issuer: String,
    /// Operation identity used to bind approval and reconcile retries.
    pub request_id: OpId,
    /// 32-byte device identifier, distinct from its signing public key.
    pub device_id: Hash,
    /// Account security revision used to invalidate stale approvals.
    pub security_epoch: u64,
    /// Time authorization was accepted, in Unix milliseconds.
    pub approved_at: u64,
    /// Exclusive deadline in Unix milliseconds (`now < expires_at`).
    pub expires_at: u64,
    /// Checked browser origin bound by the device approval.
    pub origin: String,
    /// Maximum ICP cycles approved for this execution; not a token amount.
    pub max_cycles: u128,
    /// SHA-256 of the exact COSE Sig_structure bytes.
    pub to_be_signed_digest: Hash,
    /// RFC 9679 SHA-256 thumbprint of required public COSE key parameters.
    pub public_key_fingerprint: Hash,
    /// Lifecycle state recorded for the authorized execution.
    pub status: ExecutionStatus,
    /// SHA-256 of raw signature bytes, if available; differs from the CTT imprint.
    pub signature_digest: Option<Hash>,
}
