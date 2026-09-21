//! `dmsg-commerce/1`: fixed subscription terms, resource leases and merchant accounting.
//! Amounts are ledger atomic units; times are Unix milliseconds unless explicitly named.
#![allow(missing_docs)]
use crate::{cose::Algorithm, membership::*, Environment, Hash};
use candid::{CandidType, Principal};
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PlanId {
    Free,
    Plus,
    Pro,
    Max,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ResourceLimits {
    pub storage_bytes: u64,
    pub active_channels: u64,
    pub monthly_execution_units: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionWeights {
    pub version: u64,
    pub ed25519: u64,
    pub ecdsa_secp256k1: u64,
}

impl ExecutionWeights {
    pub fn units(&self, algorithm: &Algorithm) -> Option<u64> {
        match algorithm {
            Algorithm::Ed25519 => Some(self.ed25519),
            Algorithm::EcdsaSecp256k1 => Some(self.ecdsa_secp256k1),
            _ => None,
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlanVersion {
    pub plan_id: PlanId,
    pub catalog_version: u64,
    pub price_cents: u64,
    pub limits: ResourceLimits,
    pub weights: ExecutionWeights,
    pub terms_version: u64,
    pub membership_policy_version: Option<u64>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StorageProduct {
    pub product_id: Hash,
    pub storage_bytes: u64,
    pub duration_ms: u64,
    pub price_cents: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Catalog {
    pub schema: u16,
    pub version: u64,
    pub effective_at_ms: u64,
    pub plans: Vec<PlanVersion>,
    pub storage_products: Vec<StorageProduct>,
    pub ledger: Principal,
    pub decimals: u8,
    pub ledger_fee: u128,
    pub terms_digest: Hash,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CommerceInit {
    pub environment: Environment,
    pub governance: Principal,
    pub membership_canister: Principal,
    pub user_homes: Vec<Principal>,
    pub catalog: Catalog,
    #[serde(with = "crate::account::account_cbor")]
    pub treasury: Account,
    pub max_subjects: u64,
    pub daily_orders: u32,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum OrderAction {
    Subscribe { plan: PlanId },
    Renew { plan: PlanId },
    Upgrade { plan: PlanId },
    Storage { product_id: Hash },
    Buyout { contract_id: Hash },
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct QuoteOrder {
    pub op_id: Hash,
    pub beneficiary: Beneficiary,
    pub action: OrderAction,
    pub expected_business_revision: u64,
    #[serde(with = "crate::account::account_cbor")]
    pub payer: Account,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OrderQuote {
    pub home_commerce: Principal,
    pub request: QuoteOrder,
    pub catalog: Catalog,
    pub amount_atomic: u128,
    pub fee_reserve: u128,
    pub created_at_ms: u64,
    pub fund_by_ms: u64,
    pub activate_by_ms: u64,
    pub term: TermRule,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OpenOrder {
    pub quote: OrderQuote,
    pub authorization: MembershipIntent,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum OrderStatus {
    Authorizing,
    AwaitingFunding,
    Active,
    Closing,
    RefundCommitted,
    Cancelled,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BillingOrder {
    pub order_id: Hash,
    pub input: OpenOrder,
    pub receive_subaccount: Hash,
    pub status: OrderStatus,
    pub activated_contract_id: Option<Hash>,
    pub funding_block: Option<u64>,
    pub close_effective_at_ms: Option<u64>,
    pub confirmed_in: u128,
    pub service_reserve: u128,
    pub earned: u128,
    pub refundable: u128,
    pub fee_reserve: u128,
    pub outgoing: u128,
    pub transferred: u128,
    pub network_fees: u128,
    pub refunded_principal: u128,
    pub next_transfer: u64,
    pub busy_until_ms: u64,
    pub generation: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ContractSource {
    Cash { order_id: Hash },
    Sns { claim_id: Hash },
    Compensation,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipContract {
    /// Original annual interval anchor, retained across upgrades and buyouts.
    pub term_starts_at_ms: u64,
    /// Known loss of qualification; closed intervals preserve monthly history.
    pub resource_pauses: Vec<(u64, Option<u64>)>,
    pub contract_id: Hash,
    pub plan: PlanVersion,
    pub source: ContractSource,
    pub starts_at_ms: u64,
    pub expires_at_ms: u64,
    pub terminated_at_ms: Option<u64>,
    pub closing_at_ms: Option<u64>,
    pub last_issued_until_ms: u64,
    pub eligibility: Eligibility,
    pub observed_at_ms: u64,
    pub qualified_until_ms: u64,
    pub repair_deadline_ms: Option<u64>,
    pub unverifiable_since_ms: Option<u64>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StorageAddon {
    pub contract_id: Hash,
    pub order_id: Hash,
    pub storage_bytes: u64,
    pub starts_at_ms: u64,
    pub expires_at_ms: u64,
    pub last_issued_until_ms: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SourceStatus {
    Free,
    Active,
    Expired,
    Closing,
    RepairRequired,
    Suspended,
    Unverifiable,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EntitlementView {
    pub schema: u16,
    pub home_commerce: Principal,
    pub beneficiary: Beneficiary,
    pub business_revision: u64,
    pub lease_revision: u64,
    pub active_contract_id: Option<Hash>,
    pub plan_snapshot: PlanVersion,
    pub addons: Vec<StorageAddon>,
    pub effective_limits: ResourceLimits,
    pub next_limit_change_at_ms: Option<u64>,
    pub lease_source_contract_ids: Vec<Hash>,
    pub source_status: SourceStatus,
    pub eligibility_status: Eligibility,
    pub observed_at_ms: u64,
    pub issued_at_ms: u64,
    pub valid_until_ms: u64,
    pub effective_stop_at_ms: Option<u64>,
    pub repair_deadline_ms: Option<u64>,
    pub service_terminated_at_ms: Option<u64>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MonthSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub monthly_units: u64,
    pub source_contract_id: Option<Hash>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MonthEntitlement {
    pub beneficiary: Beneficiary,
    /// YYYYMM in UTC.
    pub month_utc: u32,
    pub month_revision: u64,
    pub business_revision: u64,
    pub weights: ExecutionWeights,
    pub segments: Vec<MonthSegment>,
    pub allowed_units: u64,
    pub calculation_version: u16,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionEntitlement {
    pub view: EntitlementView,
    pub month: MonthEntitlement,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CommercialReservation {
    pub reservation_id: Hash,
    pub month_utc: u32,
    pub units: u64,
    pub weight_policy_version: u64,
    pub business_revision: u64,
    pub lease_revision: u64,
    pub valid_until_ms: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionUsage {
    pub account_id: crate::AccountId,
    pub month_utc: u32,
    pub month_revision: u64,
    pub business_revision: u64,
    pub lease_revision: u64,
    pub weight_policy_version: u64,
    pub allowed_units: u64,
    pub held_units: u64,
    pub charged_units: u64,
    pub valid_until_ms: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MerchantDeposit {
    pub order_id: Hash,
    pub block: u64,
    #[serde(with = "crate::account::account_cbor")]
    pub from: Account,
    pub amount: u128,
    pub refundable: u128,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum MerchantTransferStatus {
    Superseded,
    Pending,
    InFlight,
    Unknown,
    Rejected,
    Succeeded,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MerchantTransfer {
    pub replaces: Option<u64>,
    pub replaced_by: Option<u64>,
    pub order_id: Hash,
    pub transfer_id: u64,
    #[serde(with = "crate::account::account_cbor")]
    pub to: Account,
    pub amount: u128,
    pub fee: u128,
    pub memo: Hash,
    pub created_at_time_ns: u64,
    pub status: MerchantTransferStatus,
    pub block: Option<u64>,
}
