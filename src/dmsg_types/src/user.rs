//! Stable account control: login bindings, approved devices, recovery and root commitments.
//!
//! A login Principal authenticates a caller; a device key authorizes a sensitive
//! operation. AccountId survives changes to either. Public views are not storage
//! records. Times are Unix milliseconds unless a field explicitly says otherwise.
use crate::{agent::*, handle::HandleIntent, *};
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Maximum device records allowed by account protocol validation (16).
pub const MAX_DEVICES: usize = 16;
/// Maximum login Principal bindings per account (8).
pub const MAX_AUTH_BINDINGS: usize = 8;
/// Recovery waiting period of a new account (72 hours); `SetRecoveryDelay`
/// changes it within one to seven days.
pub const DEFAULT_RECOVERY_DELAY_MS: u64 = 3 * DAY;
/// Largest account capacity (`max_accounts`) one user home accepts (21,000,000).
pub const MAX_HOME_ACCOUNTS: u64 = 21_000_000;
/// Latest operation receipts an account keeps for idempotent replay (16).
pub const MAX_OPERATION_RECEIPTS: usize = 16;
/// Approved login bindings an account holds until the new login accepts them (4).
pub const MAX_PENDING_BINDINGS: usize = 4;
/// Time a new login has to accept an approved binding (ten minutes).
pub const BINDING_ACCEPT_MS: u64 = 10 * MINUTE;
/// Longest remaining lifetime of an account admission ticket (ten minutes).
pub const MAX_ADMISSION_TTL_MS: u64 = 10 * MINUTE;
/// Remote calls an account's logins may start per UTC hour (120).
pub const MAX_HOURLY_ACCOUNT_CALLS: u32 = 120;

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
    /// Read account ciphertext through the cloud service. The user home only
    /// records it: every active device receives a content-root envelope.
    VaultUnlock,
    /// Reserve and commit content roots.
    RootManage,
    /// Approve formal document attestations.
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
    /// 32-byte X25519 public key receiving content-root and channel HPKE envelopes.
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

/// Commitment to an encrypted root bundle stored outside the user canister.
/// This contains neither the bundle nor a plaintext content key.
/// The commit checks that `recipients_digest` names exactly the active devices
/// and the vetKD recovery identity of this generation, and that
/// `bundle_digest = digest("dmsg/root-bundle-digest/2", (recipients_digest, body_digest))`.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ContentRootRef {
    /// Reserved content-root generation being committed.
    pub generation: u64,
    /// Root bundle suite identifier; current commit validation requires `dmsg-root-v2`.
    pub suite: String,
    /// Commitment to the externally stored encrypted root bundle.
    pub bundle_digest: Hash,
    /// `digest("dmsg/root-recipients/1", (sorted active device IDs, generation))`.
    pub recipients_digest: Hash,
    /// SHA-256 of the bundle body that carries the envelopes.
    pub body_digest: Hash,
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

/// Account limits on formal attestations and recovery derivations.
/// The default permits statement, file and app-action attestations, 20 per
/// day, which is also the user home's ceiling for root derivations.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SensitivePolicy {
    /// Whether sensitive execution is frozen by account policy.
    pub frozen: bool,
    /// Statement purposes allowed by this policy.
    pub allowed_purposes: Vec<KeyPurpose>,
    /// Maximum executions in a daily budget window.
    pub daily_executions: u32,
}

impl Default for SensitivePolicy {
    fn default() -> Self {
        Self {
            frozen: false,
            allowed_purposes: vec![
                KeyPurpose::FileAttestation,
                KeyPurpose::Statement,
                KeyPurpose::AppAction,
            ],
            daily_executions: 20,
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

/// Login-authorized proposal for a replacement device, executed after the
/// account's recovery delay unless an active device disputes it.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecoveryRequest {
    /// Idempotency identifier; reuse only with identical operation parameters.
    pub op_id: OpId,
    /// Login Principal that submits the request and is bound after recovery completes.
    pub new_auth: Principal,
    /// Replacement device to enroll when recovery completes.
    pub device: DeviceInput,
    /// Exclusive deadline in Unix milliseconds (`now < expires_at`).
    pub expires_at: u64,
}

/// Delayed recovery state. A dispute by an active device cancels it.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PendingRecovery {
    /// Original delayed recovery proposal.
    pub request: RecoveryRequest,
    /// Earliest recovery execution time in Unix milliseconds.
    pub execute_after: u64,
}

/// Certified account security leaf (current schema 4).
/// The leaf path is the single raw 12-byte account ID. `devices_root` commits
/// to the complete device map, including revocation and replay state. Verify
/// certificate, witness and freshness before using it as authority.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SecuritySnapshot {
    /// Canonical absolute issuer URI identifying the signer.
    pub issuer: String,
    /// Fixed COSE canister responsible for recovery derivation.
    pub home_cose: Principal,
    /// Version of this public certified-leaf format, not the storage schema.
    pub schema: u16,
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// User canister authoritative for this account or deployment.
    pub home_user: Principal,
    /// Account mutation revision used for optimistic concurrency.
    pub account_version: u64,
    /// Account security revision used to invalidate stale approvals.
    pub security_epoch: u64,
    /// `dmsg/devices/v1` digest of the complete BTreeMap<Hash, Device>.
    pub devices_root: Hash,
    /// Configured recovery delay in milliseconds.
    pub recovery_delay_ms: u64,
    /// Commitment to pending recovery state, if any.
    pub pending_recovery_digest: Option<Hash>,
    /// Committed content-root generation; zero before initialization.
    pub content_root_generation: u64,
    /// Committed root-bundle digest, or None before initialization.
    pub content_root_digest: Option<Hash>,
    /// Whether vault writes may proceed with the current root.
    pub vault_write_state: VaultWriteState,
    /// `updated_at` of the account's current principal state, or None before
    /// the principal is enabled. A published document older than this is stale.
    pub principal_updated_at: Option<u64>,
}

/// Deployment configuration and account-creation limits for the user home.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UserInit {
    /// Deployment domain used in validation and derivation.
    pub environment: Environment,
    /// Fixed issuer URI prefix; ends in `/` or `:` with no query or fragment.
    pub issuer_namespace: String,
    /// Fixed COSE canister responsible for recovery derivation.
    pub home_cose: Principal,
    /// Name registry responsible for this operation/deployment.
    pub handle_canister: Principal,
    /// Configured escrow service.
    pub payment_canister: Principal,
    /// Fixed product commerce service, authoritative for resource leases.
    pub commerce_canister: Principal,
    /// Shared PANDA qualification service.
    pub membership_canister: Principal,
    /// Maximum accounts admitted by this home (1..=21,000,000, [`MAX_HOME_ACCOUNTS`]);
    /// adjustable by governance.
    pub max_accounts: u64,
    /// Account creation limit per UTC day (1..=100,000); adjustable by governance.
    pub daily_new_accounts: u32,
    /// HTTPS origin of Agent Delegation principal IDs, such as `https://id.dmsg.net`.
    /// Permanent: principal IDs never change.
    pub principal_origin: String,
    /// Directory canister that publishes principal documents.
    pub directory_canister: Principal,
    /// Fixed SNS governance caller allowed, besides controllers, to adjust
    /// account limits and the admission key.
    pub governance: Principal,
    /// Ed25519 public key of the account admission issuer. When set,
    /// `create_account` requires an [`AdmissionTicket`] signed by it; None
    /// admits any authenticated caller. Adjustable by governance.
    pub admission_key: Option<Hash>,
}

/// Operational counters of a user home.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UserStats {
    /// Accounts this home allocated.
    pub accounts: u64,
    /// Configured account capacity.
    pub max_accounts: u64,
    /// UTC day (days since the Unix epoch) of `created_today`.
    pub day: u64,
    /// Accounts created on `day`.
    pub created_today: u32,
    /// Configured daily new-account quota.
    pub daily_new_accounts: u32,
    /// Whether the device unlock secret has been generated.
    pub unlock_ready: bool,
    /// Stable memory in 64 KiB pages.
    pub stable_pages: u64,
    /// Cycle balance.
    pub cycles: u128,
}

/// Anti-abuse admission of one caller's account creation, signed by the home's
/// configured issuer over the `dmsg/account-admission/v1` digest of
/// `(home, caller, expires_at)`. It grants no account or device authority.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AdmissionTicket {
    /// Exclusive deadline in Unix milliseconds, at most ten minutes ahead.
    pub expires_at: u64,
    /// Issuer Ed25519 signature over the admission digest.
    pub signature: Ed25519Signature,
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
    pub proof: Ed25519Signature,
    /// Issuer ticket; required when the home has an `admission_key`.
    pub admission: Option<AdmissionTicket>,
}

/// Sensitive account mutation covered by an [`Approval`].
/// Apply through [`AccountMutation`] with the expected account version.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum AccountCommand {
    /// Enroll a device with proof of possession.
    AddDevice {
        /// New device identity, keys and permissions to enroll.
        device: DeviceInput,
        /// New device Ed25519 proof of possession; use the matching protocol digest helper.
        proof: Ed25519Signature,
    },
    /// Revoke a device and invalidate affected security state.
    RevokeDevice {
        /// 32-byte device identifier, distinct from its signing public key.
        device_id: Hash,
    },
    /// Change only an existing device's capabilities under administrator approval.
    SetDeviceCapabilities {
        /// Existing device identity; keys and device role remain unchanged.
        device_id: Hash,
        /// Complete replacement capability set, subject to normal device validation.
        capabilities: Vec<Capability>,
    },
    /// Approve binding another login Principal. It is bound when that
    /// Principal calls `accept_auth_binding` with the same nonce within ten
    /// minutes; any security-epoch change withdraws the approval.
    BindAuth {
        /// Login Principal to bind.
        principal: Principal,
        /// One-time challenge from the new login's binding request.
        nonce: Hash,
    },
    /// Remove a login Principal binding.
    RemoveAuth {
        /// Login Principal to bind or remove.
        principal: Principal,
    },
    /// Change the delayed-recovery waiting period (one to seven days).
    SetRecoveryDelay {
        /// Recovery waiting period in milliseconds.
        delay_ms: u64,
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
    /// Cancel a pending recovery; any active device may approve this.
    DisputeRecovery {
        /// Operation ID of the pending recovery request.
        op_id: OpId,
    },
    /// Publish this account as an Agent Delegation principal with no controllers.
    EnablePrincipal {
        /// Display type of the principal.
        principal_type: PrincipalType,
    },
    /// Bind the next self-held controller key, proving possession of its
    /// private key with the matching protocol digest helper.
    RegisterController {
        /// Next unused controller generation.
        generation: u32,
        /// Raw Ed25519 public key of the controller.
        public_key: Hash,
        /// Optional display label.
        name: Option<String>,
        /// Explicit delegation ceiling chosen by the owner.
        delegation: DelegationAuthority,
        /// Earlier generations whose credentials this key may manage.
        supersedes: Vec<u32>,
        /// Controller Ed25519 proof of possession over the registration digest.
        proof: Ed25519Signature,
    },
    /// Retire a current controller; it can never sign or return.
    RetireController {
        /// Controller generation to retire.
        generation: u32,
    },
    /// Retire a controller if needed and record its earliest untrusted time.
    MarkControllerCompromised {
        /// Controller generation.
        generation: u32,
        /// New cutoff; may only be added or moved earlier, never before `valid_from`.
        invalid_from: u64,
    },
    /// Change only a controller's display label.
    RenameController {
        /// Controller generation.
        generation: u32,
        /// New label, or None to remove it.
        name: Option<String>,
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

/// Account query view, not a stable-storage record or a certified proof by itself.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AccountInfo {
    /// Immutable creation time recorded by the user home, in Unix milliseconds.
    pub created_at_ms: u64,
    /// Canonical absolute issuer URI identifying the signer.
    pub issuer: String,
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// User canister authoritative for this account or deployment.
    pub home_user: Principal,
    /// Fixed COSE canister responsible for recovery derivation.
    pub home_cose: Principal,
    /// Login Principals bound to this stable dMsg account.
    pub auth_bindings: Vec<Principal>,
    /// Account mutation revision used for optimistic concurrency.
    pub account_version: u64,
    /// Account security revision used to invalidate stale approvals.
    pub security_epoch: u64,
    /// Complete device map keyed by device ID, including revoked devices.
    pub devices: BTreeMap<Hash, Device>,
    /// Configured recovery delay in milliseconds.
    pub recovery_delay_ms: u64,
    /// Active delayed recovery procedure, if any.
    pub pending_recovery: Option<PendingRecovery>,
    /// Device enrolled by the latest completed recovery and the root generation
    /// it may derive; None once that device commits a new root.
    pub recovered_device: Option<(Hash, u64)>,
    /// Committed content-root bundle reference, if initialized.
    pub current_root: Option<ContentRootRef>,
    /// Outstanding root reservation, if present.
    pub root_slot: Option<RootReservation>,
    /// Whether vault writes may proceed with the current root.
    pub vault_write_state: VaultWriteState,
    /// Account-specific attestation restrictions and budgets.
    pub sensitive_policy: SensitivePolicy,
}
