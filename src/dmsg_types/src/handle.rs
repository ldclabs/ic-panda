//! Canonical name ownership, registration charges, transfers and frozen imports.
//!
//! A handle is a mutable name pointing to a stable AccountId, not a login
//! Principal. Authorization and ledger charging are separate steps. Times are
//! Unix milliseconds and token amounts are integer ledger base units.
use crate::*;
use candid::{CandidType, Principal};
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};

/// Name ownership operation authorized by an account.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum HandleAction {
    /// Register an available canonical name.
    Register,
    /// Propose transfer of an owned name.
    Transfer,
    /// Accept a proposed ownership transfer.
    AcceptTransfer,
    /// Claim a name reserved in a frozen legacy snapshot.
    ClaimLegacy,
}
/// Exact name operation authorized by the user home for the handle canister.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HandleIntent {
    /// Name registry responsible for this operation/deployment.
    pub handle_canister: Principal,
    /// Name ownership action being authorized.
    pub action: HandleAction,
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// Recipient account for transfer-related operations, when required.
    pub target_account: Option<AccountId>,
    /// Canonical name: lowercase ASCII letters/digits/underscore, 1..20 bytes; no leading underscore.
    pub handle: String,
    /// Current record version required for compare-and-swap.
    pub expected_version: u64,
    /// Idempotency identifier; reuse only with identical operation parameters.
    pub op_id: OpId,
    /// Commitment to action-specific terms, including charge terms when applicable.
    pub terms_digest: Hash,
}
/// Name registry deployment, ledger and pending-operation limits.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HandleInit {
    /// User canister authoritative for this account or deployment.
    pub home_user: Principal,
    /// ICRC ledger canister for all amounts in this contract.
    pub ledger: Principal,
    /// Configured network fee in ledger base units.
    pub ledger_fee: u128,
    /// Maximum pending name operations allowed by this registry.
    pub max_pending: u32,
}
/// Manifest committing to frozen legacy name ownership for import.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LegacySnapshot {
    /// Legacy registry that produced the frozen snapshot.
    pub source_canister: Principal,
    /// Identifier of the frozen import snapshot.
    pub snapshot_id: Hash,
    /// Legacy state version at the write freeze.
    pub freeze_version: u64,
    /// Digest of the latest committed ownership event.
    pub event_tip: Hash,
    /// Number of name entries committed by the snapshot.
    pub count: u64,
    /// Commitment to the complete frozen name entries.
    pub entries_digest: Hash,
}
/// Frozen name claim; ambiguous ownership is quarantined for resolution.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LegacyReservation {
    /// Canonical name: lowercase ASCII letters/digits/underscore, 1..20 bytes; no leading underscore.
    pub handle: String,
    /// Owner Principal captured at the legacy freeze.
    pub legacy_owner: Principal,
    /// Legacy name-account Principal, if the name used one.
    pub legacy_name_principal: Option<Principal>,
    /// Frozen administrator principals, never ordinary delegators. Empty means
    /// an ambiguous name-account owner stays quarantined.
    pub frozen_admins: Vec<Principal>,
    /// Whether ambiguous ownership prevents ordinary claim activation.
    pub quarantined: bool,
}
/// Current canonical name ownership and event-chain tip.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HandleRecord {
    /// Canonical name: lowercase ASCII letters/digits/underscore, 1..20 bytes; no leading underscore.
    pub handle: String,
    /// Stable dMsg account that currently owns the name.
    pub owner_account: AccountId,
    /// Revision of this record/authorization, used for stale-state checks.
    pub version: u64,
    /// Digest of the latest committed ownership event.
    pub event_tip: Hash,
}
/// Append-only ownership event linked to its predecessor by digest.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HandleEvent {
    /// Monotonic sequence number in this operation/event stream.
    pub sequence: u64,
    /// Digest of the preceding ownership event.
    pub previous: Hash,
    /// Canonical name: lowercase ASCII letters/digits/underscore, 1..20 bytes; no leading underscore.
    pub handle: String,
    /// Previous owner account; None when assigning an unowned name.
    pub from: Option<AccountId>,
    /// Account receiving ownership of this name.
    pub to: AccountId,
    /// Revision of this record/authorization, used for stale-state checks.
    pub version: u64,
    /// Ownership change time in Unix milliseconds.
    pub at: u64,
    /// Frozen snapshot commitment associated with the event.
    pub legacy_snapshot: Hash,
}
/// Authorized name intent and fixed ledger charge terms.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Registration {
    /// Account-authorized name operation.
    pub intent: HandleIntent,
    /// ICRC account whose allowance funds the registration charge.
    #[serde(with = "crate::account::account_cbor")]
    pub payer: Account,
    /// Ledger network fee in integer base units.
    pub fee: u128,
}
/// Registration charge/commit lifecycle. Unknown charges require reconciliation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum HandlePhase {
    /// Name operation reserved before ledger charging.
    Reserved,
    /// Ledger charge is in flight.
    Charging,
    /// Ledger charge may have succeeded; reconcile before another charge.
    ChargeUnknown,
    /// Ledger charge succeeded; ownership commit remains.
    Paid,
    /// Name ownership change is committed.
    Committed,
    /// The applicable deadline has passed.
    Expired,
    /// A registration refund remains to be completed.
    RefundPending,
}
/// Query view of a name registration and its ledger charge.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HandleOperation {
    /// Fixed registration intent and payment terms.
    pub registration: Registration,
    /// Domain-separated commitment to this operation and its parameters.
    pub digest: Hash,
    /// Current registration lifecycle phase.
    pub phase: HandlePhase,
    /// Ledger transfer amount in base units, excluding registration.fee.
    pub amount: u128,
    /// Creation time in Unix milliseconds.
    pub created_at: u64,
    /// Exclusive deadline in Unix milliseconds (`now < expires_at`).
    pub expires_at: u64,
    /// Fixed 32-byte ledger memo used for transfer correlation/deduplication.
    pub memo: Hash,
    /// Confirmed ledger transaction index, if known.
    pub ledger_block: Option<u64>,
}

/// Progress of the restricted legacy name import process.
#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct SnapshotProgress {
    /// Frozen import manifest, or None before an import begins.
    pub snapshot: Option<LegacySnapshot>,
    /// Number of frozen entries imported so far.
    pub imported: u64,
    /// Running commitment to the imported entry sequence.
    pub rolling_digest: Hash,
    /// Last imported canonical name, for ordered continuation.
    pub last_handle: Option<String>,
    /// Whether the imported snapshot has been finalized.
    pub sealed: bool,
}
