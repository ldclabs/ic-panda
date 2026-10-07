//! vetKD recovery derivation: the one chain-key operation dMsg still runs.
//!
//! Content roots are random client bytes wrapped to every active device. The
//! root bundle also carries an identity-based-encryption envelope to the
//! account's vetKD identity `(account_id, generation)`, encrypted offline with
//! the derived public key the COSE home publishes. Only a completed delayed
//! recovery authorizes the user home to grant derivations, to the device it
//! enrolled and of the committed generation, until that device commits a new
//! root; the COSE home executes them. Formal document signatures are device signatures recorded by
//! the user home; see [`crate::signing`].
//! Business times are Unix milliseconds; costs are ICP cycles.
use crate::*;
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};

/// Pinned vetKD master-key configuration checked during COSE initialization.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MasterKey {
    /// ICP management-canister vetKD master-key name.
    pub key_name: String,
    /// SHA-256 of the derived content-root public key for the configured
    /// context. A zero pin is allowed only outside Production.
    pub expected_fingerprint: Hash,
}

/// Deployment configuration for the COSE executor.
/// Production validation requires `key_1` and a nonzero fingerprint;
/// changing derivation inputs changes key identity.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CoseInit {
    /// Fixed issuer URI prefix; ends in `/` or `:` with no query or fragment.
    pub issuer_namespace: String,
    /// Deployment domain used in validation and derivation.
    pub environment: Environment,
    /// Expected identity of this COSE executor.
    pub executing_canister: Principal,
    /// User homes authorized to supply execution grants, each only for the
    /// accounts whose IDs carry its allocator fingerprint. Append-only.
    pub user_homes: Vec<Principal>,
    /// Key derivation format version; current public protocol uses 2.
    pub derivation_version: u16,
    /// vetKD master-key pin.
    pub master: MasterKey,
    /// Maximum derivations in a daily budget window; adjustable by governance.
    pub daily_executions: u32,
    /// Maximum reserved ICP cycles in a daily budget window; adjustable by governance.
    pub daily_cycles: u128,
    /// Fixed SNS governance caller allowed, besides controllers, to run administrative operations.
    pub governance: Principal,
}

/// Readiness of the configured chain-key public key.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Initialization {
    /// Required material has not been initialized.
    Uninitialized,
    /// Public-key initialization is in progress.
    Initializing,
    /// Required material is initialized and ready for use.
    Ready,
}

/// COSE deployment configuration and master-key initialization diagnostics.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct KeyState {
    /// Deployment configuration used by the executor.
    pub config: CoseInit,
    /// Master public-key initialization state.
    pub initialization: Initialization,
    /// SHA-256 of the initialized derived public key, once Ready.
    pub fingerprint: Option<Hash>,
    /// Initialization diagnostic, when one is available.
    pub error: Option<String>,
}

/// Public provenance of the content-root vetKD key.
/// Authenticate its source before encrypting to it: key identity depends on the
/// home canister and derivation configuration.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct KeyDescriptor {
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// Fixed COSE canister responsible for key derivation and execution.
    pub home_cose: Principal,
    /// ICP master-key name used for this derived key.
    pub master_key_name: String,
    /// Deployment domain used in validation and derivation.
    pub environment: Environment,
    /// Key derivation format version; current public protocol uses 2.
    pub derivation_version: u16,
    /// Content-root generation this descriptor was requested for; it is the
    /// IBE identity together with the account ID.
    pub key_generation: u64,
    /// Raw 96-byte vetKD derived public key of the content-root context,
    /// shared by every account of this executor.
    pub public_key: ByteBuf,
    /// SHA-256 of `public_key`; the configured master pin.
    pub public_key_fingerprint: Hash,
}

/// Request the vetKD key of the committed content root, encrypted to a
/// caller-generated transport public key. Authorized only for the device a
/// completed delayed recovery enrolled, until that device commits a new root.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DeriveRootRequest {
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// Committed content-root generation to derive.
    pub generation: u64,
    /// 48-byte compressed vetKD transport public key; protocol validation checks the point.
    pub transport_public_key: ByteArray<48>,
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
    pub device_sequence: u64,
    /// Time authorization was accepted, in Unix milliseconds.
    pub approved_at: u64,
    /// Exclusive deadline in Unix milliseconds (`now < expires_at`).
    pub expires_at: u64,
    /// Committed content-root generation to derive.
    pub generation: u64,
    /// 48-byte compressed vetKD transport public key; protocol validation checks the point.
    pub transport_key: ByteArray<48>,
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

/// Query/retry view of a derivation and its conservative management-call cost bound.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionResult {
    /// Operation identity used to bind approval and reconcile retries.
    pub request_id: OpId,
    /// Execution state with any available output or error.
    pub outcome: ExecutionOutcome,
    /// Conservative ICP cost upper bound, including maximum response/callback
    /// reservations, less refunded attached cycles. Not an actual charge or bill.
    /// Zero before dispatch or when the management call was not sent.
    pub cycles_cost_upper_bound: u128,
    /// Threshold fee the returned management call consumed: its attached
    /// request cycles less the refund. Set on Completed and Failed results,
    /// zero otherwise; daily budgets settle to it. Excludes message fees.
    pub cycles_charged: u128,
}

/// Execution lifecycle carrying either output or failure information.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ExecutionOutcome {
    /// Authorization is committed; no completed result is yet available.
    Authorized,
    /// Execution has started and may be awaiting an external call.
    Executing,
    /// Execution completed with a retained output.
    Completed(EncryptedRootKey),
    /// Execution returned a known failure.
    Failed(Error),
    /// Execution outcome is uncertain; reconcile using the same request ID.
    Unknown(Error),
    /// Retained output is no longer available; this does not authorize replay.
    ResultExpired,
}

/// Encrypted vetKD output and the derivation-key descriptor.
/// The vetKey it decrypts to opens the root bundle's IBE recovery envelope.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EncryptedRootKey {
    /// vetKD result encrypted to the supplied transport public key.
    pub encrypted_key: ByteBuf,
    /// Public descriptor of the key used to produce this output.
    pub key: KeyDescriptor,
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
}

/// Public maintenance page of expired COSE execution results.
/// The caller supplies next_after to continue, including across upgrades.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionCleanup {
    /// Exclusive account cursor for the next page; None ends this pass.
    pub next_after: Option<AccountId>,
    /// Number of homes inspected (at most 64).
    pub homes_scanned: u32,
    /// Expired terminal results deleted; replay high-water marks are preserved.
    pub results_removed: u32,
}

/// Operational counters of a COSE executor.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CoseStats {
    /// Accounts that have reached this executor; their records are kept.
    pub accounts: u64,
    /// Account capacity shared by every configured user home.
    pub max_accounts: u64,
    /// Retained execution results.
    pub results: u64,
    /// Management calls this module instance is awaiting.
    pub in_flight: u64,
    /// Executions recorded as Unknown, including calls an upgrade abandoned.
    pub unknown: u64,
    /// UTC day (days since the Unix epoch) of the usage below.
    pub budget_day: u64,
    /// Derivations counted today against `daily_executions`.
    pub executions_today: u32,
    /// Reserved or settled cycles today against `daily_cycles`.
    pub cycles_today: u128,
    /// Stable memory in 64 KiB pages.
    pub stable_pages: u64,
    /// Cycle balance.
    pub cycles: u128,
}
