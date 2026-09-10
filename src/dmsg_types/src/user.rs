//! Stable account control: login bindings, approved devices, recovery and root commitments.
//!
//! A login Principal authenticates a caller; a device key authorizes a sensitive
//! operation. AccountId survives changes to either. Public views are not storage
//! records. Times are Unix milliseconds unless a field explicitly says otherwise.
use crate::{cose::*, handle::HandleIntent, *};
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::collections::BTreeMap;

/// Maximum device records allowed by account protocol validation (16).
pub const MAX_DEVICES: usize = 16;
/// Maximum login Principal bindings per account (8).
pub const MAX_AUTH_BINDINGS: usize = 8;

/// Account device role, distinct from ICP canister controller privileges.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ControllerRole {
    /// Device eligible for account administration, subject to required capabilities.
    Administrator,
    /// Device with limited capabilities; not an account administrator.
    Member,
}

/// Explicit device permission; authentication alone grants none of these rights.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    /// Sign ordinary content under device authority.
    ContentSign,
    /// Request content-root derivation for vault access.
    VaultUnlock,
    /// Reserve and commit content roots.
    RootManage,
    /// Approve formal document signing.
    FormalApprove,
    /// Authorize recipient payment offers.
    PaymentOffer,
}

/// Public keys, role and capabilities proposed for a device.
/// The protocol validates keys and requires proof of possession for enrollment.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DeviceInput {
    /// 32-byte device identifier, distinct from its signing public key.
    pub device_id: Hash,
    /// 32-byte Ed25519 verification key; protocol validation rejects invalid/weak keys.
    pub signing_pub: Hash,
    /// 32-byte X25519 public key for recovery/device HPKE envelopes.
    pub hpke_pub: Hash,
    /// Account device role; capabilities are checked separately.
    pub role: ControllerRole,
    /// Explicit operations permitted to this device.
    pub capabilities: Vec<Capability>,
}

/// Registered device and replay/revocation state.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// Device identity, keys and permissions registered at enrollment.
    pub input: DeviceInput,
    /// Enrollment time in Unix milliseconds.
    pub added_at: u64,
    /// Approving device ID, when enrollment had an existing-device sponsor.
    pub added_by: Option<Hash>,
    /// Revocation time in Unix milliseconds, or None if not revoked.
    pub revoked_at: Option<u64>,
    /// Next device approval sequence expected by the user home.
    pub next_sequence: u64,
}

/// Offline recovery public keys and delayed takeover policy.
/// No private recovery key or recovery code is stored in this value.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecoveryPolicy {
    /// Recovery-policy generation; replacement must increment the current value.
    pub generation: u64,
    /// 32-byte Ed25519 verification key; protocol validation rejects invalid/weak keys.
    pub signing_pub: Hash,
    /// 32-byte X25519 public key for recovery/device HPKE envelopes.
    pub hpke_pub: Hash,
    /// Recovery waiting period in milliseconds.
    pub delay_ms: u64,
}

/// Whether new vault writes may use the committed root.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum VaultWriteState {
    /// Required material has not been initialized.
    Uninitialized,
    /// Required material is initialized and ready for use.
    Ready,
    /// A new content root must be committed before writes can resume.
    RekeyRequired,
}

/// Account control state, independent of local application locking.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum AccountStatus {
    /// Account is not in disputed recovery.
    Active,
    /// Recovery has been disputed; sensitive operations are restricted.
    RecoveryDisputed,
}

/// Commitment to an encrypted root bundle stored outside the user canister.
/// This contains neither the bundle nor a plaintext content key.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ContentRootRef {
    /// Reserved content-root generation being committed.
    pub generation: u64,
    /// Root bundle suite identifier; current commit validation requires `dmsg-root-v1`.
    pub suite: String,
    /// Fixed COSE canister responsible for key derivation and execution.
    pub home_cose: Principal,
    /// Key derivation format version; current public protocol uses 2.
    pub derivation_version: u16,
    /// vetKD key generation; must equal this root generation.
    pub key_generation: u64,
    /// Commitment to the externally stored encrypted root bundle.
    pub bundle_digest: Hash,
    /// Recovery public-key generation used to wrap the bundle.
    pub recovery_generation: u64,
}

/// Temporary compare-and-swap slot for a candidate content root.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RootReservation {
    /// Idempotency identifier; reuse only with identical operation parameters.
    pub op_id: OpId,
    /// Committed root generation expected for compare-and-swap.
    pub expected_generation: u64,
    /// Allocated candidate generation; abandoned reservations may leave gaps.
    pub generation: u64,
    /// Account security revision used to invalidate stale approvals.
    pub security_epoch: u64,
    /// Exclusive deadline in Unix milliseconds (`now < expires_at`).
    pub expires_at: u64,
}

/// Account limits on sensitive chain-key execution.
/// The default permits statement/file signing, 20 executions and
/// 1,000,000,000,000 cycles per day; deployment limits also apply.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SensitivePolicy {
    /// Whether sensitive execution is frozen by account policy.
    pub frozen: bool,
    /// Key purposes allowed by this policy.
    pub allowed_purposes: Vec<KeyPurpose>,
    /// Maximum executions in a daily budget window.
    pub daily_executions: u32,
    /// Maximum ICP cycles in a daily budget window.
    pub daily_cycles: u128,
}

impl Default for SensitivePolicy {
    fn default() -> Self {
        Self {
            frozen: false,
            allowed_purposes: vec![KeyPurpose::FileAttestation, KeyPurpose::Statement],
            daily_executions: 20,
            daily_cycles: 1_000_000_000_000,
        }
    }
}

/// Idempotent account mutation result, distinct from a certified execution receipt.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OperationReceipt {
    /// Operation identifier for idempotent lookup.
    pub id: OpId,
    /// Domain-separated commitment to this operation and its parameters.
    pub digest: Hash,
    /// Account mutation revision used for optimistic concurrency.
    pub account_version: u64,
}

/// Recovery-key-authorized proposal for a new login binding and device.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecoveryRequest {
    /// Idempotency identifier; reuse only with identical operation parameters.
    pub op_id: OpId,
    /// Login Principal to bind after recovery completes.
    pub new_auth: Principal,
    /// Replacement device to enroll when recovery completes.
    pub device: DeviceInput,
    /// Recovery-policy generation authorizing this request.
    pub generation: u64,
    /// Exclusive deadline in Unix milliseconds (`now < expires_at`).
    pub expires_at: u64,
}

/// Delayed recovery state, including any dispute and renewed confirmation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PendingRecovery {
    /// Original delayed recovery proposal.
    pub request: RecoveryRequest,
    /// Earliest recovery execution time in Unix milliseconds.
    pub execute_after: u64,
    /// Digest identifying the dispute, if one exists.
    pub dispute: Option<Hash>,
    /// Whether the recovery key has reconfirmed after a dispute.
    pub reconfirmed: bool,
    /// Reconfirmation with its own deadline; None retains the original deadline.
    #[serde(default)]
    pub confirmation: Option<RecoveryConfirmation>,
}

/// Recovery-key reconfirmation binding a disputed recovery request.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecoveryConfirmation {
    /// Operation identity used to bind approval and reconcile retries.
    pub request_id: OpId,
    /// Digest of the dispute explicitly acknowledged by the recovery key.
    pub dispute: Hash,
    /// Exclusive deadline in Unix milliseconds (`now < expires_at`).
    pub expires_at: u64,
}

impl PendingRecovery {
    /// Effective recovery deadline in Unix milliseconds, preferring reconfirmation when present.
    pub fn expires_at(&self) -> u64 {
        self.confirmation
            .as_ref()
            .map_or(self.request.expires_at, |c| c.expires_at)
    }
}

/// Certified account security leaf (current schema 2).
/// The leaf path is the single raw 12-byte account ID. `devices_root` commits
/// to the complete device map, including revocation and replay state. Verify
/// certificate, witness and freshness before using it as authority.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SecuritySnapshot {
    /// Canonical absolute issuer URI identifying the signer.
    pub issuer: String,
    /// Fixed COSE canister responsible for key derivation and execution.
    pub home_cose: Principal,
    /// Version of this public certified-leaf format, not the storage schema.
    pub schema: u16,
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// User canister authoritative for this account or deployment.
    pub home_user: Principal,
    /// Current account control status.
    pub account_status: AccountStatus,
    /// Account mutation revision used for optimistic concurrency.
    pub account_version: u64,
    /// Account security revision used to invalidate stale approvals.
    pub security_epoch: u64,
    /// `dmsg/devices/v1` digest of the complete BTreeMap<Hash, Device>.
    pub devices_root: Hash,
    /// Current recovery-policy generation.
    pub recovery_root_version: u64,
    /// Recovery encryption public key, or None before configuration.
    pub recovery_hpke_pub: Option<Hash>,
    /// Recovery Ed25519 public key, or None before configuration.
    pub recovery_signing_pub: Option<Hash>,
    /// Replay counter for recovery authorization.
    pub recovery_nonce: u64,
    /// Configured recovery delay in milliseconds, if recovery is configured.
    pub recovery_delay_ms: Option<u64>,
    /// Commitment to pending recovery state, if any.
    pub pending_recovery_digest: Option<Hash>,
    /// Committed content-root generation; zero before initialization.
    pub content_root_generation: u64,
    /// Committed root-bundle digest, or None before initialization.
    pub content_root_digest: Option<Hash>,
    /// Whether vault writes may proceed with the current root.
    pub vault_write_state: VaultWriteState,
}

/// Deployment configuration and account-creation limits for the user home.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UserInit {
    /// Deployment domain used in validation and derivation.
    pub environment: Environment,
    /// Fixed issuer URI prefix; ends in `/` or `:` with no query or fragment.
    pub issuer_namespace: String,
    /// Fixed COSE canister responsible for key derivation and execution.
    pub home_cose: Principal,
    /// Name registry responsible for this operation/deployment.
    pub handle_canister: Principal,
    /// Configured escrow service.
    pub payment_canister: Principal,
    /// Maximum accounts admitted by this deployment.
    pub max_accounts: u64,
    /// Account creation limit per daily budget window.
    pub daily_new_accounts: u32,
}

/// Authenticated account creation with initial-device proof of possession.
/// The user canister allocates the AccountId; clients do not choose it.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CreateAccount {
    /// Initial administrator device with its keys and requested capabilities.
    pub device: DeviceInput,
    /// Idempotency identifier; reuse only with identical operation parameters.
    pub op_id: OpId,
    /// Exclusive deadline in Unix milliseconds (`now < expires_at`).
    pub expires_at: u64,
    /// Initial device Ed25519 proof over the account-creation digest.
    pub proof: ByteBuf,
}

/// Sensitive account mutation covered by an [`Approval`].
/// Apply through [`AccountMutation`] with the expected account version.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum AccountCommand {
    /// Enroll a device with proof of possession.
    AddDevice {
        /// New device identity, keys and permissions to enroll.
        device: DeviceInput,
        /// Profile-specific proof of possession/confirmation; use the matching protocol digest helper.
        proof: ByteBuf,
    },
    /// Revoke a device and invalidate affected security state.
    RevokeDevice {
        /// 32-byte device identifier, distinct from its signing public key.
        device_id: Hash,
    },
    /// Bind another proven login Principal.
    BindAuth {
        /// Login Principal to bind or remove.
        principal: Principal,
        /// One-time challenge binding the login Principal authorization.
        nonce: Hash,
    },
    /// Remove a login Principal binding.
    RemoveAuth {
        /// Login Principal to bind or remove.
        principal: Principal,
    },
    /// Install recovery public keys with proof of possession.
    SetRecovery {
        /// New recovery or sensitive-execution policy, as selected by the command.
        policy: RecoveryPolicy,
        /// Profile-specific proof of possession/confirmation; use the matching protocol digest helper.
        proof: ByteBuf,
    },
    /// Confirm configured recovery material.
    ConfirmRecovery {
        /// Profile-specific proof of possession/confirmation; use the matching protocol digest helper.
        proof: ByteBuf,
    },
    /// Replace sensitive execution policy.
    SetPolicy {
        /// New recovery or sensitive-execution policy, as selected by the command.
        policy: SensitivePolicy,
    },
    /// Reserve the next content-root generation using compare-and-swap.
    ReserveRoot {
        /// Committed root generation expected for compare-and-swap.
        expected_generation: u64,
        /// Idempotency identifier; reuse only with identical operation parameters.
        op_id: OpId,
    },
    /// Commit the bundle reference for an active root reservation.
    CommitRoot {
        /// Committed root generation expected for compare-and-swap.
        expected_generation: u64,
        /// Idempotency identifier; reuse only with identical operation parameters.
        op_id: OpId,
        /// Candidate encrypted root-bundle commitment to commit.
        root: ContentRootRef,
    },
    /// Authorize a specific operation at the configured name registry.
    AuthorizeHandle {
        /// Account-authorized name operation.
        intent: HandleIntent,
    },
    /// Record a dispute against a pending recovery operation.
    DisputeRecovery {
        /// Idempotency identifier; reuse only with identical operation parameters.
        op_id: OpId,
        /// Dispute digest to bind a later recovery-key reconfirmation.
        dispute: Hash,
    },
}

/// Compare-and-swap account update with device approval.
/// The approval binds both the expected version and the complete command.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AccountMutation {
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// Current record version required for compare-and-swap.
    pub expected_version: u64,
    /// Complete account mutation covered by the approval.
    pub command: AccountCommand,
    /// Device approval binding the complete request and replay context.
    pub approval: Approval,
}

/// Device and snapshot view for authorization checks.
/// The struct itself carries no IC certificate or witness; its source must be trusted.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DeviceEvidence {
    /// Account security snapshot used to evaluate this device.
    pub snapshot: SecuritySnapshot,
    /// Device registration, revocation state and replay sequence.
    pub device: Device,
    /// Time this evidence was observed, in Unix milliseconds; not a certificate by itself.
    pub observed_at: u64,
}

/// Account query view, not a stable-storage record or a certified proof by itself.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AccountInfo {
    /// Canonical absolute issuer URI identifying the signer.
    pub issuer: String,
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// User canister authoritative for this account or deployment.
    pub home_user: Principal,
    /// Fixed COSE canister responsible for key derivation and execution.
    pub home_cose: Principal,
    /// Login Principals bound to this stable dMsg account.
    pub auth_bindings: Vec<Principal>,
    /// Account mutation revision used for optimistic concurrency.
    pub account_version: u64,
    /// Account security revision used to invalidate stale approvals.
    pub security_epoch: u64,
    /// Current account control state.
    pub status: AccountStatus,
    /// Complete device map keyed by device ID, including revoked devices.
    pub devices: BTreeMap<Hash, Device>,
    /// Configured offline recovery policy, if present.
    pub recovery: Option<RecoveryPolicy>,
    /// Whether recovery setup confirmation has completed.
    pub recovery_checked: bool,
    /// Replay counter for recovery authorization.
    pub recovery_nonce: u64,
    /// Active delayed recovery procedure, if any.
    pub pending_recovery: Option<PendingRecovery>,
    /// Committed content-root bundle reference, if initialized.
    pub current_root: Option<ContentRootRef>,
    /// Outstanding root reservation, if present.
    pub root_slot: Option<RootReservation>,
    /// Whether vault writes may proceed with the current root.
    pub vault_write_state: VaultWriteState,
    /// Account-specific chain-key execution restrictions and budgets.
    pub sensitive_policy: SensitivePolicy,
}
