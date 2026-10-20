//! Full-fee PANDA commitments. No upgrade, replacement, buyout or early-close variants.
use crate::{
    AccountId, Hash, integration::*, integration_billing::ProductAuthorizationRequest,
    membership::Eligibility,
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
    /// Must not exceed the service's 1,000,000 full-record limit.
    pub max_claims: u64,
    /// Successful new applications per UTC hour.
    pub hourly_applications: u64,
    /// SNS neuron reads per UTC minute; every active claim needs about one per hour.
    pub qualifications_per_minute: u64,
    /// Quote and application calls per UTC minute; activations have a separate
    /// budget of the same size. Each actor may use ten of either per minute.
    pub authorizations_per_minute: u64,
    /// Product adapter calls (reserve, release, apply, decision lookup) per UTC
    /// minute; each actor may use ten per minute.
    pub product_calls_per_minute: u64,
}

/// What a client shows before quoting a PANDA subscription: the SNS whose
/// neurons qualify, its ledger, whether new applications are paused and the
/// announced conversion rates.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PandaServiceView {
    /// PANDA SNS governance, which lists the qualifying neurons.
    pub governance: Principal,
    /// Eight-decimal PANDA ledger.
    pub panda_ledger: Principal,
    /// New applications are paused; existing claims keep refreshing.
    pub paused: bool,
    /// Announced rate policies, current and scheduled.
    pub rates: Vec<PandaRatePolicy>,
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

/// Initial approval of the same immutable terms.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PandaClaimRequest {
    /// Immutable terms; retries cannot replace them.
    pub terms: PandaApplicationTerms,
    /// Fresh account proof and product operator approval.
    pub authorization: ProductAuthorizationRequest,
}

/// Termination stops rights; Released follows once the issued lease has ended.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PandaClaimStatus {
    /// Accepted application; Apply has not been prepared.
    Checking,
    /// Durable Apply exists; cancellation and timeout release are forbidden.
    Applying,
    /// Product applied the full waiver. Commitment is immutable.
    Active,
    /// Product rights ended permanently; the neuron stays occupied until its issued lease ends.
    Terminated,
    /// Cancelled before any Apply was prepared.
    Cancelled,
    /// A definite rejection proves that no rights were delivered.
    Rejected,
    /// Rights ended and occupancy released exactly once.
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
    /// Zero before successful Apply; fixed to the original E afterwards.
    pub committed_until_ms: u64,
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
