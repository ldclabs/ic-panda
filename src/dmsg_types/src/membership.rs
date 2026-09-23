//! `membership/1`: product-neutral PANDA qualification and durable benefit decisions.
//! All business timestamps are UTC Unix milliseconds; intervals are [start, end).
//! Constructing or decoding these records performs no validation or authorization.
use crate::{Environment, Hash};
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// Product, authority and opaque subject binding; decoding does not establish control.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Beneficiary {
    /// Product identifier within this protocol.
    pub product_id: String,
    /// Canister authoritative for the beneficiary subject.
    pub authority_canister: Principal,
    /// Registered schema identifying how to interpret subject_bytes.
    pub subject_schema: String,
    /// Opaque subject bytes in the registered schema.
    pub subject_bytes: ByteBuf,
}

/// A device-approved, purpose-separated intent. Actor authentication is also required.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipIntent {
    /// Stable idempotency identifier for this application.
    pub application_id: Hash,
    /// Deployment domain of the request or service.
    pub environment: Environment,
    /// Exact service authorized to consume this intent.
    pub service_canister: Principal,
    /// Product subject receiving the benefit.
    pub beneficiary: Beneficiary,
    /// Authenticated economic actor approving the action.
    pub actor: Principal,
    /// Domain-separated commitment to the exact requested action.
    pub action_digest: Hash,
    /// Challenge preventing reuse as a different authorization.
    pub nonce: Hash,
    /// Exclusive validity deadline in Unix milliseconds.
    pub valid_until_ms: u64,
}

/// Authority response for an exact intent, with a short validity deadline.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipAuthorization {
    /// Commitment to the complete approved intent.
    pub intent_digest: Hash,
    /// Account security revision checked by the authority.
    pub security_epoch: u64,
    /// Authority verification time in Unix milliseconds.
    pub verified_at_ms: u64,
    /// Exclusive validity deadline in Unix milliseconds.
    pub valid_until_ms: u64,
}

/// Integer PANDA stake requirement; AnnualPrice is rounded up by the protocol helper.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Threshold {
    /// Fixed positive PANDA atomic stake.
    FixedPanda {
        /// Positive PANDA amount in ledger atomic units.
        atomic: u128,
    },
    /// Annual USD-cent price converted at a rational PANDA/USD ratio.
    AnnualPrice {
        /// Integer annual or product price in USD cents.
        price_cents: u64,
        /// Positive numerator of the PANDA-per-USD conversion ratio.
        r_num: u128,
        /// Positive denominator of the PANDA-per-USD conversion ratio.
        r_den: u128,
    },
}

/// Immutable product benefit and qualification policy snapshot.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipPolicy {
    /// Immutable version identifier within this policy or catalog.
    pub version: u64,
    /// Product identifier within this protocol.
    pub product_id: String,
    /// Opaque product benefit identity.
    pub benefit_id: Hash,
    /// Required PANDA stake calculation under this policy.
    pub threshold: Threshold,
    /// Inclusive activation time in Unix milliseconds.
    pub effective_at_ms: u64,
    /// Governance-approved service cost reservation, not cash revenue.
    pub subsidy_units: u64,
}

/// Governance-registered product adapter and permitted beneficiary authorities.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProductConfig {
    /// Product identifier within this protocol.
    pub product_id: String,
    /// Product canister applying membership decisions.
    pub adapter: Principal,
    /// Canisters permitted to authorize product subjects.
    pub authorities: Vec<Principal>,
    /// Registered schema identifying how to interpret subject_bytes.
    pub subject_schema: String,
    /// Expected subject byte length.
    pub subject_size: u16,
}

/// Shared qualification service deployment and admission limits.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipInit {
    /// Deployment domain of the request or service.
    pub environment: Environment,
    /// Governance Principal controlling this service configuration.
    pub governance: Principal,
    /// Pinned PANDA SNS root canister.
    pub sns_root: Principal,
    /// Pinned PANDA ledger canister.
    pub panda_ledger: Principal,
    /// Registered product adapters and subject schemas.
    pub products: Vec<ProductConfig>,
    /// Initial immutable qualification policies.
    pub policies: Vec<MembershipPolicy>,
    /// Total allowed service subsidy units, not ledger funds.
    pub subsidy_budget: u64,
    /// Maximum retained qualification claims.
    pub max_claims: u64,
    /// New application limit per UTC hour.
    pub hourly_applications: u32,
    /// Cooling duration in milliseconds before qualification can activate.
    pub cooling_ms: u64,
}

/// Calendar-year term or an explicit half-open UTC millisecond interval.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TermRule {
    /// One calendar year; February 29 is clamped to February 28.
    CalendarYear,
    /// Explicit half-open interval.
    Fixed {
        /// Inclusive interval start in Unix milliseconds.
        starts_at_ms: u64,
        /// Exclusive interval end in Unix milliseconds.
        expires_at_ms: u64,
    },
}

/// Relationship between a new claim and an existing benefit commitment.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ClaimChange {
    /// Start a new independent benefit.
    Start,
    /// Renew an existing benefit for a subsequent term.
    Renew {
        /// Existing claim whose benefit is being changed.
        previous_claim: Hash,
    },
    /// Upgrade an existing base benefit.
    Upgrade {
        /// Existing claim whose benefit is being changed.
        previous_claim: Hash,
    },
    /// Replace the qualifying claim supporting an existing benefit.
    Replace {
        /// Existing claim whose benefit is being changed.
        previous_claim: Hash,
    },
}

/// Frozen qualification request and beneficiary authorization.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ClaimRequest {
    /// Exact beneficiary intent approved for this operation.
    pub authorization: MembershipIntent,
    /// Fixed PANDA SNS neuron identifier.
    pub neuron_id: Hash,
    /// Immutable qualification policy version.
    pub policy_version: u64,
    /// Opaque product benefit identity.
    pub benefit_id: Hash,
    /// Product business revision required for compare-and-swap.
    pub expected_business_revision: u64,
    /// Requested or frozen benefit interval.
    pub term: TermRule,
    /// Requested relationship to an earlier claim.
    pub change: ClaimChange,
}

/// Qualification observation; unverifiable is distinct from known ineligibility.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Eligibility {
    /// Known to satisfy the fixed qualification policy.
    Eligible,
    /// Known not to satisfy the policy.
    Ineligible,
    /// Current supporting facts cannot be verified.
    Unverifiable,
}

/// Durable qualification and product-decision lifecycle.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ClaimStatus {
    /// Initial qualification checks are pending.
    Checking,
    /// Qualified observation is waiting out the admission cooling period.
    CoolingDown,
    /// A durable product decision awaits acknowledgment.
    Applying,
    /// Benefit is active under its recorded terms.
    Active,
    /// Benefit closure is in progress.
    Closing,
    /// Exclusive binding has been released.
    Released,
    /// The request or transfer was definitively rejected.
    Rejected,
}

/// Public qualification leaf deliberately excludes economic identities and raw neurons.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ClaimView {
    /// Public record format version, independent of stable storage.
    pub schema: u16,
    /// Membership canister authoritative for this claim.
    pub home_membership: Principal,
    /// Stable qualification claim identity.
    pub claim_id: Hash,
    /// Product subject receiving the benefit.
    pub beneficiary: Beneficiary,
    /// Opaque product benefit identity.
    pub benefit_id: Hash,
    /// Immutable qualification policy version.
    pub policy_version: u64,
    /// Current lifecycle state of this record.
    pub status: ClaimStatus,
    /// Most recent qualification assessment.
    pub eligibility: Eligibility,
    /// Inclusive interval start in Unix milliseconds.
    pub starts_at_ms: u64,
    /// Exclusive interval end in Unix milliseconds.
    pub expires_at_ms: u64,
    /// Time the supporting qualification observation began, in Unix milliseconds.
    pub observed_at_ms: u64,
    /// Exclusive validity deadline in Unix milliseconds.
    pub valid_until_ms: u64,
    /// Monotonic projection/lease revision, including refreshes.
    pub lease_revision: u64,
    /// Idempotency identifier of the product decision.
    pub decision_id: Option<Hash>,
    /// Earliest exclusive-binding release time in Unix milliseconds.
    pub release_after_ms: u64,
}

/// Product action delivered by the configured membership authority.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum DecisionKind {
    /// Apply the frozen benefit terms.
    Apply,
    /// Close the specified benefit commitment.
    Close,
}

/// Immutable action delivered at least once to the registered product adapter.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipDecision {
    /// Idempotency identifier of the product decision.
    pub decision_id: Hash,
    /// Stable qualification claim identity.
    pub claim_id: Hash,
    /// Product action to apply.
    pub kind: DecisionKind,
    /// Original fixed request.
    pub request: ClaimRequest,
    /// Immutable policy snapshot used for this decision.
    pub policy: MembershipPolicy,
    /// Required PANDA stake in atomic units, rounded up.
    pub required_atomic: u128,
    /// Inclusive interval start in Unix milliseconds.
    pub starts_at_ms: u64,
    /// Exclusive interval end in Unix milliseconds.
    pub expires_at_ms: u64,
    /// Exclusive product application deadline in Unix milliseconds.
    pub apply_by_ms: u64,
    /// Time the supporting qualification observation began, in Unix milliseconds.
    pub observed_at_ms: u64,
    /// Exclusive supporting qualification deadline in Unix milliseconds.
    pub qualification_until_ms: u64,
}

/// Product result for a specific immutable membership decision.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum DecisionOutcome {
    /// The product durably applied the decision.
    Applied,
    /// The request or transfer was definitively rejected.
    Rejected,
}

/// Idempotent product acknowledgment, including its remaining benefit commitment.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipDecisionReceipt {
    /// Idempotency identifier of the product decision.
    pub decision_id: Hash,
    /// Commitment to the exact decision being acknowledged.
    pub decision_digest: Hash,
    /// Recorded result of applying the decision.
    pub outcome: DecisionOutcome,
    /// Product contract identity.
    pub contract_id: Option<Hash>,
    /// Inclusive interval start in Unix milliseconds.
    pub starts_at_ms: u64,
    /// Exclusive interval end in Unix milliseconds.
    pub expires_at_ms: u64,
    /// Revision of business terms or qualification, independent of lease refreshes.
    pub business_revision: u64,
    /// Product confirms it cannot grant any benefit from this claim beyond this time.
    pub commitment_until_ms: u64,
}
