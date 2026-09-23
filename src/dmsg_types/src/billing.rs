//! `dmsg-commerce/1`: fixed subscription terms, resource leases and merchant accounting.
//! Amounts are ledger atomic units; times are Unix milliseconds unless explicitly named.
//! Constructing or decoding these records performs no validation or authorization.
use crate::{cose::Algorithm, membership::*, Environment, Hash};
use candid::{CandidType, Principal};
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};

/// Base resource tier; it grants no account or cryptographic permissions.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PlanId {
    /// Free base tier.
    Free,
    /// Plus base tier.
    Plus,
    /// Pro base tier.
    Pro,
    /// Max base tier.
    Max,
}

/// Logical storage, active-channel and formal-execution resource limits.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ResourceLimits {
    /// Logical retained ciphertext capacity in bytes.
    pub storage_bytes: u64,
    /// Number of active sponsored channel slots.
    pub active_channels: u64,
    /// Full UTC-month allowance of weighted formal-execution units.
    pub monthly_execution_units: u64,
}

/// Versioned integer cost per formal-signing algorithm.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionWeights {
    /// Immutable version identifier within this policy or catalog.
    pub version: u64,
    /// Units charged for one Ed25519 formal execution.
    pub ed25519: u64,
    /// Units charged for one ES256K formal execution.
    pub ecdsa_secp256k1: u64,
}

impl ExecutionWeights {
    /// Return this algorithm's configured units; vetKD has no formal-signing charge.
    pub fn units(&self, algorithm: &Algorithm) -> Option<u64> {
        match algorithm {
            Algorithm::Ed25519 => Some(self.ed25519),
            Algorithm::EcdsaSecp256k1 => Some(self.ecdsa_secp256k1),
            _ => None,
        }
    }
}

/// Immutable base-plan price, limits, weights and policy snapshot.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlanVersion {
    /// Base resource tier.
    pub plan_id: PlanId,
    /// Immutable catalog version containing this plan.
    pub catalog_version: u64,
    /// Integer annual or product price in USD cents.
    pub price_cents: u64,
    /// Base resource limits.
    pub limits: ResourceLimits,
    /// Versioned execution-cost weights.
    pub weights: ExecutionWeights,
    /// Version of the accepted service terms.
    pub terms_version: u64,
    /// Qualification policy for SNS funding, when configured.
    pub membership_policy_version: Option<u64>,
}

/// Additional storage product with fixed size and annual price.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StorageProduct {
    /// Product identifier within this protocol.
    pub product_id: Hash,
    /// Logical retained ciphertext capacity in bytes.
    pub storage_bytes: u64,
    /// Integer annual price in USD cents, prorated to the base term endpoint.
    pub price_cents: u64,
}

/// Certified commercial catalog; asset identity and governance approval require trusted evidence.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Catalog {
    /// Public record format version, independent of stable storage.
    pub schema: u16,
    /// Immutable version identifier within this policy or catalog.
    pub version: u64,
    /// Inclusive activation time in Unix milliseconds.
    pub effective_at_ms: u64,
    /// Available base-plan snapshots.
    pub plans: Vec<PlanVersion>,
    /// Additional storage products in this catalog.
    pub storage_products: Vec<StorageProduct>,
    /// Accepted ICRC ledger canister; token symbols are not asset identities.
    pub ledger: Principal,
    /// Verified ledger decimal precision.
    pub decimals: u8,
    /// Configured network fee in ledger atomic units.
    pub ledger_fee: u128,
    /// Commitment to the accepted service terms.
    pub terms_digest: Hash,
}

/// Product commerce deployment, accepted asset and admission configuration.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CommerceInit {
    /// Deployment domain of the request or service.
    pub environment: Environment,
    /// Governance Principal controlling this service configuration.
    pub governance: Principal,
    /// Shared PANDA qualification authority.
    pub membership_canister: Principal,
    /// Permitted dMsg account-authority canisters.
    pub user_homes: Vec<Principal>,
    /// Fixed catalog snapshot.
    pub catalog: Catalog,
    /// ICRC account receiving earned service revenue.
    #[serde(with = "crate::account::account_cbor")]
    pub treasury: Account,
    /// Maximum retained beneficiary subjects.
    pub max_subjects: u64,
    /// New merchant-order limit per UTC day.
    pub daily_orders: u32,
}

/// Commercial change requested under a beneficiary-approved order.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum OrderAction {
    /// Start a base subscription.
    Subscribe {
        /// Requested tier or fixed plan snapshot.
        plan: PlanId,
    },
    /// Renew an existing benefit for a subsequent term.
    Renew {
        /// Requested tier or fixed plan snapshot.
        plan: PlanId,
    },
    /// Upgrade an existing base benefit.
    Upgrade {
        /// Requested tier or fixed plan snapshot.
        plan: PlanId,
    },
    /// Purchase the specified additional storage product.
    Storage {
        /// Product identifier within this protocol.
        product_id: Hash,
    },
    /// Replace the remaining SNS-funded term with cash.
    Buyout {
        /// Product contract identity.
        contract_id: Hash,
    },
}

/// Request for a fixed quote; creating this value neither charges nor authorizes payment.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct QuoteOrder {
    /// Operation identifier reused only with identical terms.
    pub op_id: Hash,
    /// Product subject receiving the benefit.
    pub beneficiary: Beneficiary,
    /// Requested commercial change.
    pub action: OrderAction,
    /// Product business revision required for compare-and-swap.
    pub expected_business_revision: u64,
    /// ICRC source account required for primary funding.
    #[serde(with = "crate::account::account_cbor")]
    pub payer: Account,
}

/// Frozen merchant terms; funding and activation have separate exclusive deadlines.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OrderQuote {
    /// Commerce canister authoritative for this order or lease.
    pub home_commerce: Principal,
    /// Original fixed request.
    pub request: QuoteOrder,
    /// Fixed catalog snapshot.
    pub catalog: Catalog,
    /// Service principal in ledger atomic units, excluding fee reserve.
    pub amount_atomic: u128,
    /// Ledger atomic units reserved for network fees.
    pub fee_reserve: u128,
    /// Record creation time in Unix milliseconds.
    pub created_at_ms: u64,
    /// Exclusive ledger-commit deadline for primary funding, in Unix milliseconds.
    pub fund_by_ms: u64,
    /// Exclusive service activation deadline in Unix milliseconds.
    pub activate_by_ms: u64,
    /// Requested or frozen benefit interval.
    pub term: TermRule,
}

/// Fixed quote and the exact authorization to open it.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OpenOrder {
    /// Frozen price, asset, interval and deadline terms.
    pub quote: OrderQuote,
    /// Exact beneficiary intent approved for this operation.
    pub authorization: MembershipIntent,
}

/// Merchant order lifecycle; a refund decision is separate from ledger payout.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum OrderStatus {
    /// Order is authorized and awaiting qualifying ledger funding.
    AwaitingFunding,
    /// Benefit is active under its recorded terms.
    Active,
    /// Benefit closure is in progress.
    Closing,
    /// Refund direction is fixed; payout may still be pending.
    RefundCommitted,
    /// Order was cancelled without activation.
    Cancelled,
}

/// Merchant accounting view in ledger atomic units, including pending allocations.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BillingOrder {
    /// Stable merchant order identity.
    pub order_id: Hash,
    /// Original quote and authorization.
    pub input: OpenOrder,
    /// Order deposit subaccount owned by home_commerce.
    pub receive_subaccount: Hash,
    /// Current lifecycle state of this record.
    pub status: OrderStatus,
    /// Contract activated from this order, if any.
    pub activated_contract_id: Option<Hash>,
    /// Ledger block selected as primary funding.
    pub funding_block: Option<u64>,
    /// Effective benefit stop time in Unix milliseconds, after issued leases.
    pub close_effective_at_ms: Option<u64>,
    /// Total verified deposits in ledger atomic units.
    pub confirmed_in: u128,
    /// Unperformed service principal retained in atomic units.
    pub service_reserve: u128,
    /// Earned service principal still held, in atomic units.
    pub earned: u128,
    /// Unallocated refundable balance in atomic units.
    pub refundable: u128,
    /// Ledger atomic units reserved for network fees.
    pub fee_reserve: u128,
    /// Amount plus network fee allocated to outstanding transfers, in atomic units.
    pub outgoing: u128,
    /// Total successful transfer amounts, excluding network fees, in atomic units.
    pub transferred: u128,
    /// Total successful outgoing ledger fees in atomic units.
    pub network_fees: u128,
    /// Service principal already allocated to refunds, in atomic units.
    pub refunded_principal: u128,
    /// Next per-order outgoing transfer identifier.
    pub next_transfer: u64,
    /// Exclusive in-flight operation deadline in Unix milliseconds.
    pub busy_until_ms: u64,
    /// Operation revision used to reject obsolete callbacks.
    pub generation: u64,
}

/// Source of the base benefit; SNS qualification is not cash revenue.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ContractSource {
    /// Benefit funded by a merchant order.
    Cash {
        /// Stable merchant order identity.
        order_id: Hash,
    },
    /// Benefit supported by a qualification claim.
    Sns {
        /// Stable qualification claim identity.
        claim_id: Hash,
    },
    /// Product-granted compensation benefit.
    Compensation,
}

/// Fixed base contract and its separately observed resource qualification.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipContract {
    /// Original annual interval anchor, retained across upgrades and buyouts.
    pub term_starts_at_ms: u64,
    /// Known loss of qualification; closed intervals preserve monthly history.
    pub resource_pauses: Vec<(u64, Option<u64>)>,
    /// Product contract identity.
    pub contract_id: Hash,
    /// Requested tier or fixed plan snapshot.
    pub plan: PlanVersion,
    /// Cash, SNS or compensation origin of this contract.
    pub source: ContractSource,
    /// Inclusive interval start in Unix milliseconds.
    pub starts_at_ms: u64,
    /// Exclusive interval end in Unix milliseconds.
    pub expires_at_ms: u64,
    /// Service termination time in Unix milliseconds, when present.
    pub terminated_at_ms: Option<u64>,
    /// Scheduled benefit close time in Unix milliseconds, when present.
    pub closing_at_ms: Option<u64>,
    /// Latest resource-lease deadline already issued, in Unix milliseconds.
    pub last_issued_until_ms: u64,
    /// Most recent qualification assessment.
    pub eligibility: Eligibility,
    /// Time the supporting qualification observation began, in Unix milliseconds.
    pub observed_at_ms: u64,
    /// Exclusive known qualification deadline in Unix milliseconds.
    pub qualified_until_ms: u64,
    /// Qualification repair deadline in Unix milliseconds, when applicable.
    pub repair_deadline_ms: Option<u64>,
    /// Start of an unresolved qualification observation, in Unix milliseconds.
    pub unverifiable_since_ms: Option<u64>,
}

/// Paid storage entitlement with its own interval and issued-lease deadline.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StorageAddon {
    /// Product contract identity.
    pub contract_id: Hash,
    /// Stable merchant order identity.
    pub order_id: Hash,
    /// Logical retained ciphertext capacity in bytes.
    pub storage_bytes: u64,
    /// Inclusive interval start in Unix milliseconds.
    pub starts_at_ms: u64,
    /// Exclusive interval end in Unix milliseconds.
    pub expires_at_ms: u64,
    /// Latest resource-lease deadline already issued, in Unix milliseconds.
    pub last_issued_until_ms: u64,
}

/// Current resource-source state, distinct from account security state.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SourceStatus {
    /// Free base tier.
    Free,
    /// Benefit is active under its recorded terms.
    Active,
    /// The source interval has expired.
    Expired,
    /// Benefit closure is in progress.
    Closing,
    /// Known qualification loss is within the repair period.
    RepairRequired,
    /// New paid resource admission is suspended.
    Suspended,
    /// Current supporting facts cannot be verified.
    Unverifiable,
}

/// Certified resource lease projection; validate beneficiary, revisions and expiry before use.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EntitlementView {
    /// Public record format version, independent of stable storage.
    pub schema: u16,
    /// Commerce canister authoritative for this order or lease.
    pub home_commerce: Principal,
    /// Product subject receiving the benefit.
    pub beneficiary: Beneficiary,
    /// Revision of business terms or qualification, independent of lease refreshes.
    pub business_revision: u64,
    /// Monotonic projection/lease revision, including refreshes.
    pub lease_revision: u64,
    /// Currently selected base contract, absent for Free.
    pub active_contract_id: Option<Hash>,
    /// Base-plan terms used for this projection.
    pub plan_snapshot: PlanVersion,
    /// Separately valid paid storage entitlements.
    pub addons: Vec<StorageAddon>,
    /// Combined currently applicable resource limits.
    pub effective_limits: ResourceLimits,
    /// Next known resource-limit transition in Unix milliseconds.
    pub next_limit_change_at_ms: Option<u64>,
    /// Contracts supporting this issued lease.
    pub lease_source_contract_ids: Vec<Hash>,
    /// Current resource-source lifecycle.
    pub source_status: SourceStatus,
    /// Qualification assessment supporting this projection.
    pub eligibility_status: Eligibility,
    /// Time the supporting qualification observation began, in Unix milliseconds.
    pub observed_at_ms: u64,
    /// Projection issuance time in Unix milliseconds.
    pub issued_at_ms: u64,
    /// Exclusive validity deadline in Unix milliseconds.
    pub valid_until_ms: u64,
    /// Known resource stop time in Unix milliseconds.
    pub effective_stop_at_ms: Option<u64>,
    /// Qualification repair deadline in Unix milliseconds, when applicable.
    pub repair_deadline_ms: Option<u64>,
    /// Known service termination time in Unix milliseconds.
    pub service_terminated_at_ms: Option<u64>,
}

/// Contiguous half-open UTC millisecond interval weighted by a full-month quota.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MonthSegment {
    /// Inclusive segment start in Unix milliseconds.
    pub start_ms: u64,
    /// Exclusive segment end in Unix milliseconds.
    pub end_ms: u64,
    /// Full-month units applicable during this segment.
    pub monthly_units: u64,
    /// Contract supporting the segment, or None for Free.
    pub source_contract_id: Option<Hash>,
}

/// Versioned monthly quota calculation; retain prior segments when terms change.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MonthEntitlement {
    /// Product subject receiving the benefit.
    pub beneficiary: Beneficiary,
    /// YYYYMM in UTC.
    pub month_utc: u32,
    /// Revision of the retained monthly calculation.
    pub month_revision: u64,
    /// Revision of business terms or qualification, independent of lease refreshes.
    pub business_revision: u64,
    /// Versioned execution-cost weights.
    pub weights: ExecutionWeights,
    /// Contiguous quota segments, limited to 64 by protocol validation.
    pub segments: Vec<MonthSegment>,
    /// Total monthly units, rounding down once after summing weighted durations.
    pub allowed_units: u64,
    /// Version of the monthly allowance algorithm.
    pub calculation_version: u16,
}

/// Consistent resource lease and monthly formal-execution allocation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionEntitlement {
    /// Resource qualification and lease projection.
    pub view: EntitlementView,
    /// Monthly quota calculation matching this beneficiary.
    pub month: MonthEntitlement,
}

/// User-home reservation for one approved formal execution, not a bearer authorization.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CommercialReservation {
    /// Execution request identity reserved exactly once.
    pub reservation_id: Hash,
    /// UTC year and month encoded as YYYYMM.
    pub month_utc: u32,
    /// Weighted formal-execution units reserved for this request.
    pub units: u64,
    /// Execution weight version used for this accounting.
    pub weight_policy_version: u64,
    /// Revision of business terms or qualification, independent of lease refreshes.
    pub business_revision: u64,
    /// Monotonic projection/lease revision, including refreshes.
    pub lease_revision: u64,
    /// Exclusive validity deadline in Unix milliseconds.
    pub valid_until_ms: u64,
}

/// Monthly execution counters; retrying an existing operation must not reserve again.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionUsage {
    /// Stable 12-byte dMsg account identity.
    pub account_id: crate::AccountId,
    /// UTC year and month encoded as YYYYMM.
    pub month_utc: u32,
    /// Revision of the retained monthly calculation.
    pub month_revision: u64,
    /// Revision of business terms or qualification, independent of lease refreshes.
    pub business_revision: u64,
    /// Monotonic projection/lease revision, including refreshes.
    pub lease_revision: u64,
    /// Execution weight version used for this accounting.
    pub weight_policy_version: u64,
    /// Total monthly units, rounding down once after summing weighted durations.
    pub allowed_units: u64,
    /// Units reserved by nonterminal executions.
    pub held_units: u64,
    /// Units charged to completed executions.
    pub charged_units: u64,
    /// Exclusive validity deadline in Unix milliseconds.
    pub valid_until_ms: u64,
}

/// Verified order deposit and its remaining refund allocation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MerchantDeposit {
    /// Stable merchant order identity.
    pub order_id: Hash,
    /// Verified ledger transaction index, when available.
    pub block: u64,
    /// Verified source ICRC account.
    #[serde(with = "crate::account::account_cbor")]
    pub from: Account,
    /// Ledger transfer amount in atomic units, excluding the network fee.
    pub amount: u128,
    /// Unallocated refundable balance in atomic units.
    pub refundable: u128,
}

/// Ledger transfer lifecycle with explicit ambiguous and superseded states.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum MerchantTransferStatus {
    /// A new transfer replaces this known-unsent transfer.
    Superseded,
    /// Transfer is prepared but has not started.
    Pending,
    /// Ledger call is in progress.
    InFlight,
    /// An earlier ledger attempt may have committed; reconcile the same transfer.
    Unknown,
    /// The request or transfer was definitively rejected.
    Rejected,
    /// Ledger execution has been confirmed.
    Succeeded,
}

/// Fixed ledger outbox parameters; preserve memo and timestamp when retrying.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MerchantTransfer {
    /// Earlier transfer replaced by this known-unsent fee revision.
    pub replaces: Option<u64>,
    /// Replacement transfer identifier, when superseded.
    pub replaced_by: Option<u64>,
    /// Stable merchant order identity.
    pub order_id: Hash,
    /// Transfer identifier within the order.
    pub transfer_id: u64,
    /// Fixed outgoing ICRC destination.
    #[serde(with = "crate::account::account_cbor")]
    pub to: Account,
    /// Ledger transfer amount in atomic units, excluding the network fee.
    pub amount: u128,
    /// Fixed outgoing network fee in ledger atomic units.
    pub fee: u128,
    /// Fixed ledger memo preserved on retries.
    pub memo: Hash,
    /// Sender timestamp in Unix nanoseconds, preserved on retries.
    pub created_at_time_ns: u64,
    /// Current lifecycle state of this record.
    pub status: MerchantTransferStatus,
    /// Verified ledger transaction index, when available.
    pub block: Option<u64>,
    /// Most recent typed ledger rejection, retained for diagnosis and fee repair.
    pub last_error: Option<icrc_ledger_types::icrc1::transfer::TransferError>,
}

/// Public order advancement result. Financial and authorization details require a private query.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OrderProgress {
    /// Original order identity.
    pub order_id: Hash,
    /// Committed lifecycle status.
    pub status: OrderStatus,
}

impl From<BillingOrder> for OrderProgress {
    fn from(order: BillingOrder) -> Self {
        Self {
            order_id: order.order_id,
            status: order.status,
        }
    }
}

/// Public outgoing-transfer advancement result, without account or amount details.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TransferProgress {
    /// Original order identity.
    pub order_id: Hash,
    /// Per-order transfer identifier.
    pub transfer_id: u64,
    /// Current transfer lifecycle status.
    pub status: MerchantTransferStatus,
    /// Replacement leg, when this leg is superseded.
    pub replaced_by: Option<u64>,
}

impl From<MerchantTransfer> for TransferProgress {
    fn from(transfer: MerchantTransfer) -> Self {
        Self {
            order_id: transfer.order_id,
            transfer_id: transfer.transfer_id,
            status: transfer.status,
            replaced_by: transfer.replaced_by,
        }
    }
}
