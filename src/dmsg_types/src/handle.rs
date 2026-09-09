use crate::*;
use candid::{CandidType, Principal};
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum HandleAction {
    Register,
    Transfer,
    AcceptTransfer,
    ClaimLegacy,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HandleIntent {
    pub handle_canister: Principal,
    pub action: HandleAction,
    pub account_id: AccountId,
    pub target_account: Option<AccountId>,
    pub handle: String,
    pub expected_version: u64,
    pub op_id: OpId,
    pub terms_digest: Hash,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HandleInit {
    pub home_user: Principal,
    pub ledger: Principal,
    pub ledger_fee: u128,
    pub max_pending: u32,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LegacySnapshot {
    pub source_canister: Principal,
    pub snapshot_id: Hash,
    pub freeze_version: u64,
    pub event_tip: Hash,
    pub count: u64,
    pub entries_digest: Hash,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LegacyReservation {
    pub handle: String,
    pub legacy_owner: Principal,
    pub legacy_name_principal: Option<Principal>,
    /// Frozen administrator principals, never ordinary delegators. Empty means
    /// an ambiguous name-account owner stays quarantined.
    pub frozen_admins: Vec<Principal>,
    pub quarantined: bool,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HandleRecord {
    pub handle: String,
    pub owner_account: AccountId,
    pub version: u64,
    pub event_tip: Hash,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HandleEvent {
    pub sequence: u64,
    pub previous: Hash,
    pub handle: String,
    pub from: Option<AccountId>,
    pub to: AccountId,
    pub version: u64,
    pub at: u64,
    pub legacy_snapshot: Hash,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Registration {
    pub intent: HandleIntent,
    #[serde(with = "crate::account::account_cbor")]
    pub payer: Account,
    pub fee: u128,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum HandlePhase {
    Reserved,
    Charging,
    ChargeUnknown,
    Paid,
    Committed,
    Expired,
    RefundPending,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HandleOperation {
    pub registration: Registration,
    pub digest: Hash,
    pub phase: HandlePhase,
    pub amount: u128,
    pub created_at: u64,
    pub expires_at: u64,
    pub memo: Hash,
    pub ledger_block: Option<u64>,
}

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct SnapshotProgress {
    pub snapshot: Option<LegacySnapshot>,
    pub imported: u64,
    pub rolling_digest: Hash,
    pub last_handle: Option<String>,
    pub sealed: bool,
}
