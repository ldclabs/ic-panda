//! `dmsg-commerce/1`: fixed subscription terms, resource leases and merchant accounting.
//! Amounts are ledger atomic units; times are Unix milliseconds unless explicitly named.
//! Constructing or decoding these records performs no validation or authorization.
use crate::{cose::Algorithm, membership::*, Environment, Hash};
use candid::{CandidType, Principal};
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
    /// Maximum retained beneficiary subjects.
    pub max_subjects: u64,
    /// New merchant-order limit per UTC day.
    pub daily_orders: u32,
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
    /// Known or unverifiable qualification loss; closed intervals preserve monthly history.
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
    /// Most recent qualification assessment.
    pub eligibility: Eligibility,
    /// Time the supporting qualification observation began, in Unix milliseconds.
    pub observed_at_ms: u64,
    /// Exclusive known qualification deadline in Unix milliseconds.
    pub qualified_until_ms: u64,
    /// Known-ineligibility repair deadline in Unix milliseconds, when applicable.
    pub repair_deadline_ms: Option<u64>,
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
