//! `membership/1`: product-neutral PANDA qualification and durable benefit decisions.
//! All business timestamps are UTC Unix milliseconds; intervals are [start, end).
#![allow(missing_docs)]
use crate::{Environment, Hash};
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Beneficiary {
    pub product_id: String,
    pub authority_canister: Principal,
    pub subject_schema: String,
    pub subject_bytes: ByteBuf,
}

/// A device-approved, purpose-separated intent. Actor authentication is also required.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipIntent {
    pub application_id: Hash,
    pub environment: Environment,
    pub service_canister: Principal,
    pub beneficiary: Beneficiary,
    pub actor: Principal,
    pub action_digest: Hash,
    pub nonce: Hash,
    pub valid_until_ms: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipAuthorization {
    pub intent_digest: Hash,
    pub security_epoch: u64,
    pub verified_at_ms: u64,
    pub valid_until_ms: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Threshold {
    FixedPanda {
        atomic: u128,
    },
    AnnualPrice {
        price_cents: u64,
        r_num: u128,
        r_den: u128,
    },
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipPolicy {
    pub version: u64,
    pub product_id: String,
    pub benefit_id: Hash,
    pub threshold: Threshold,
    pub effective_at_ms: u64,
    /// Governance-approved service cost reservation, not cash revenue.
    pub subsidy_units: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProductConfig {
    pub product_id: String,
    pub adapter: Principal,
    pub authorities: Vec<Principal>,
    pub subject_schema: String,
    pub subject_size: u16,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipInit {
    pub environment: Environment,
    pub governance: Principal,
    pub sns_root: Principal,
    pub panda_ledger: Principal,
    pub products: Vec<ProductConfig>,
    pub policies: Vec<MembershipPolicy>,
    pub subsidy_budget: u64,
    pub max_claims: u64,
    pub hourly_applications: u32,
    pub cooling_ms: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TermRule {
    CalendarYear,
    Fixed {
        starts_at_ms: u64,
        expires_at_ms: u64,
    },
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ClaimChange {
    Start,
    Renew { previous_claim: Hash },
    Upgrade { previous_claim: Hash },
    Replace { previous_claim: Hash },
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ClaimRequest {
    pub authorization: MembershipIntent,
    pub neuron_id: Hash,
    pub policy_version: u64,
    pub benefit_id: Hash,
    pub expected_business_revision: u64,
    pub term: TermRule,
    pub change: ClaimChange,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Eligibility {
    Eligible,
    Ineligible,
    Unverifiable,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ClaimStatus {
    Checking,
    CoolingDown,
    Applying,
    Active,
    Closing,
    Released,
    Rejected,
}

/// Public qualification leaf deliberately excludes economic identities and raw neurons.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ClaimView {
    pub schema: u16,
    pub home_membership: Principal,
    pub claim_id: Hash,
    pub beneficiary: Beneficiary,
    pub benefit_id: Hash,
    pub policy_version: u64,
    pub status: ClaimStatus,
    pub eligibility: Eligibility,
    pub starts_at_ms: u64,
    pub expires_at_ms: u64,
    pub observed_at_ms: u64,
    pub valid_until_ms: u64,
    pub lease_revision: u64,
    pub decision_id: Option<Hash>,
    pub release_after_ms: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum DecisionKind {
    Apply,
    Close,
}

/// Immutable action delivered at least once to the registered product adapter.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipDecision {
    pub decision_id: Hash,
    pub claim_id: Hash,
    pub kind: DecisionKind,
    pub request: ClaimRequest,
    pub policy: MembershipPolicy,
    pub required_atomic: u128,
    pub starts_at_ms: u64,
    pub expires_at_ms: u64,
    pub apply_by_ms: u64,
    pub observed_at_ms: u64,
    pub qualification_until_ms: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum DecisionOutcome {
    Applied,
    Rejected,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipDecisionReceipt {
    pub decision_id: Hash,
    pub decision_digest: Hash,
    pub outcome: DecisionOutcome,
    pub contract_id: Option<Hash>,
    pub starts_at_ms: u64,
    pub expires_at_ms: u64,
    pub business_revision: u64,
    /// Product confirms it cannot grant any benefit from this claim beyond this time.
    pub commitment_until_ms: u64,
}
