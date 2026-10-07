//! Full-fee PANDA commitments. No upgrade, replacement, buyout or early-close variants.
use crate::{
    integration::*, integration_billing::ProductAuthorizationRequest, membership::Eligibility,
    AccountId, Hash,
};
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

/// The commerce canister serving the accounts of one user home.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CommerceHome {
    /// Registered `dmsg_user` canister.
    pub user_home: Principal,
    /// Commerce canister listing that home; it holds the registrations and
    /// product adapters its accounts use.
    pub commerce_canister: Principal,
}

/// Deployment-wide service limits, independent of product plan names.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PandaServiceConfig {
    /// Commerce canister of each user home; entries are only appended.
    pub commerce_homes: Vec<CommerceHome>,
    /// Maximum claims simultaneously occupying a neuron, including unresolved Apply decisions.
    /// Must not exceed the service's 100,000 full-record limit.
    pub max_claims: u64,
    /// Successful new applications per UTC hour.
    pub hourly_applications: u64,
    /// At least 65 minutes; changing it cannot shorten an accepted application's cooling.
    pub cooling_ms: u64,
    /// SNS neuron reads per UTC minute; every active claim needs about one per hour.
    pub qualifications_per_minute: u64,
}

/// Complete neuron, economic owner, dMsg account and bill selected before device approval.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PandaApplicationTerms {
    /// Exact service whose global occupancy table will be used.
    pub home_membership: Principal,
    /// Registered account home.
    pub user_home: Principal,
    /// dMsg account approving this use, separate from the product beneficiary.
    pub approving_account: AccountId,
    /// Actual ICP caller controlling the SNS economic permissions.
    pub actor: Principal,
    /// Fixed SNS governance identity.
    pub sns_governance: Principal,
    /// Exact 32-byte SNS neuron ID.
    pub neuron_id: Hash,
    /// Authoritative product bill.
    pub offer: BillingOffer,
    /// Exact amount/rate/threshold/deadline accepted for this application.
    pub quote: PandaQuote,
}

/// Initial or post-cooling fresh approval of the same immutable terms.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PandaClaimRequest {
    /// Immutable terms; retries cannot replace them.
    pub terms: PandaApplicationTerms,
    /// Fresh account proof and product operator approval.
    pub authorization: ProductAuthorizationRequest,
}

/// Termination stops rights, while Released is reserved for the original commitment end.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PandaClaimStatus {
    /// Accepted application; qualification has not been established.
    Checking,
    /// First eligible observation recorded; cooling and fresh approval are required.
    CoolingDown,
    /// Durable Apply exists; cancellation and timeout release are forbidden.
    Applying,
    /// Product applied the full waiver. Commitment is immutable.
    Active,
    /// Product rights ended permanently; the neuron remains occupied until E.
    Terminated,
    /// Cancelled before any Apply was prepared.
    Cancelled,
    /// A definite rejection proves that no rights were delivered.
    Rejected,
    /// Original commitment end reached and occupancy released exactly once.
    Released,
}

/// Owner/consumer recovery view. It proves no physical custody of an SNS neuron.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PandaClaimView {
    /// Protocol version two.
    pub version: u16,
    /// Stable identity retained after release.
    pub claim_id: Hash,
    /// Complete frozen application.
    pub terms: PandaApplicationTerms,
    /// Current lifecycle.
    pub status: PandaClaimStatus,
    /// Last qualification result.
    pub eligibility: Eligibility,
    /// Observation start time, not the delayed callback time.
    pub observed_at_ms: u64,
    /// Exclusive short lease end; never later than the contract's E.
    pub valid_until_ms: u64,
    /// Monotone qualification/view revision.
    pub lease_revision: u64,
    /// Earliest time a fresh post-cooling approval can apply.
    pub cooling_until_ms: Option<u64>,
    /// Zero before successful Apply; fixed to the original E afterwards.
    pub committed_until_ms: u64,
    /// Accumulated verified repair time; Unverifiable pauses it.
    pub repair_elapsed_ms: u64,
    /// Durable terminal product receipt, if available.
    pub receipt: Option<ProductReceipt>,
}

/// Owner/adapter/governance operations, including terminated commitments still occupying capacity.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PandaOperationsPage {
    /// Accessible claims; stale eligibility is never a current lease.
    pub claims: Vec<PandaClaimView>,
    /// Last returned claim when more accessible retained records remain.
    pub next: Option<Hash>,
}
