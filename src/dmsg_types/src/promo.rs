//! Fixed-duration grants and forward-compatible membership display records.
use crate::{
    Hash,
    billing::{PlanId, RefundWindow, StorageAddon},
    membership::Beneficiary,
};
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

/// Governed authorization for new grants and recovery of outstanding requests.
#[derive(Clone, Debug, Default, CandidType, Deserialize, Serialize, PartialEq, Eq)]
pub struct PromoConfig {
    /// Admit new grants.
    pub enabled: bool,
    /// Principals allowed to submit authenticated grant decisions.
    pub verifiers: Vec<Principal>,
}

/// Immutable grant parameters. Both endpoints are UTC milliseconds.
#[derive(Clone, Debug, CandidType, Deserialize, Serialize, PartialEq, Eq)]
pub struct PromoGrant {
    /// Domain-separated, globally unique operation identifier.
    pub grant_id: Hash,
    /// Account and authority recorded at acceptance.
    pub beneficiary: Beneficiary,
    /// Plus, Pro or Max.
    pub plan: PlanId,
    /// Service compensation uses its existing source.
    pub compensation: bool,
    /// First reservation time, inclusive.
    pub starts_at_ms: u64,
    /// Exactly 30 or 365 days after the start, exclusive.
    pub expires_at_ms: u64,
}

/// Authoritative delivery outcome for this immutable request.
#[derive(Clone, Debug, CandidType, Deserialize, Serialize, PartialEq, Eq)]
pub enum PromoOutcome {
    /// Irrevocable non-delivery for this request ID.
    Rejected(String),
    /// The contract was issued.
    Applied,
    /// The previously issued contract ended at this time.
    Revoked(u64),
}

/// Persisted result. A duplicate request returns the current receipt.
#[derive(Clone, Debug, CandidType, Deserialize, Serialize, PartialEq, Eq)]
pub struct PromoReceipt {
    /// Identical on every replay.
    pub grant: PromoGrant,
    /// Grant lifecycle state.
    pub outcome: PromoOutcome,
    /// First observed formal purchase in the attribution window (source, UTC ms).
    #[serde(default)]
    pub conversion: Option<(String, u64)>,
    /// Business revision after the decision.
    pub business_revision: u64,
}

/// Aggregate first applications and first revocations for one combination.
#[derive(Clone, Debug, CandidType, Deserialize, Serialize, PartialEq, Eq)]
pub struct PromoCount {
    /// Plan granted.
    pub plan: PlanId,
    /// Interval in days.
    pub days: u16,
    /// Compensation accounting category.
    pub compensation: bool,
    /// All first deliveries, including revoked contracts.
    pub issued: u64,
    /// Previously issued grants revoked.
    pub revoked: u64,
}

/// Open identifiers allow older readers to display future sources.
#[derive(Clone, Debug, CandidType, Deserialize, Serialize, PartialEq, Eq)]
pub struct TermDisplayV3 {
    /// Product contract id.
    pub contract_id: Hash,
    /// Stable display tier.
    pub plan_id: String,
    /// Stable open source identifier.
    pub source_kind: String,
    /// Source receipt id if present.
    pub source_ref: Option<Hash>,
    /// Test transactions are identified independently of their source.
    pub is_sandbox: bool,
    /// Store classification for an existing client.
    pub store_environment: Option<String>,
    /// Google Play subscription reference, never a purchase token.
    pub store_token_hash: Option<Hash>,
    /// Annual term anchor.
    pub term_starts_at_ms: u64,
    /// Inclusive start.
    pub starts_at_ms: u64,
    /// Exclusive end.
    pub expires_at_ms: u64,
    /// Earlier end if revoked.
    pub terminated_at_ms: Option<u64>,
}

/// Display only. Resource admission always uses the certified lease.
#[derive(Clone, Debug, CandidType, Deserialize, Serialize)]
pub struct TermsDisplayV3 {
    /// Match to the certified entitlement before offering actions.
    pub business_revision: u64,
    /// Retained contracts.
    pub contracts: Vec<TermDisplayV3>,
    /// Unexpired paid storage.
    pub addons: Vec<StorageAddon>,
    /// Applicable first cash refund.
    pub refund: Option<RefundWindow>,
    /// Open action identifiers understood by the caller.
    pub allowed_actions: Vec<String>,
}

/// Stable textual error code for the display-only interface.
#[derive(Clone, Debug, CandidType, Deserialize, Serialize)]
pub struct TermsErrorV3 {
    /// Nonlocalized machine code.
    pub code: String,
}
