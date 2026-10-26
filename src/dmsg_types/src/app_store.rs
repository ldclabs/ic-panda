//! StoreKit receipts are verified by explicitly configured merchant services.
//! These types carry verified facts, never a client assertion of paid status.
use crate::{Hash, billing::PlanId, membership::Beneficiary};
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

/// Apple's transaction environment; sandbox rights never enter production.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum StoreEnvironment {
    /// Transactions signed for App Store distribution.
    Production,
    /// Apple sandbox transactions for nonproduction deployments.
    Sandbox,
}

/// A governed mapping from an Apple product to an existing dMsg catalogue item.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum StoreProductKind {
    /// Annual base subscription.
    Plan(PlanId),
    /// Storage for the active subscription term.
    Storage(Hash),
}

/// One App Store Connect product ID, independent of the application's SKU.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StoreProduct {
    /// Exact Apple product identifier.
    pub product_id: String,
    /// Existing dMsg product to grant.
    pub kind: StoreProductKind,
}

/// Merchant-verification policy set by governance.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AppStoreConfig {
    /// Principals of the server-side Apple-verification service.
    pub verifiers: Vec<Principal>,
    /// Signed application bundle ID.
    pub bundle_id: String,
    /// App Store Connect application Apple ID.
    pub app_apple_id: u64,
    /// One deployment environment.
    pub environment: StoreEnvironment,
    /// No unknown product can mint an entitlement.
    pub products: Vec<StoreProduct>,
}

/// Facts extracted from a cryptographically verified Apple transaction.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StoreTransaction {
    /// Rights recipient; the account token must bind the same beneficiary.
    pub beneficiary: Beneficiary,
    /// Immutable Apple transaction ID, retained across retries.
    pub transaction_id: String,
    /// Original subscription transaction ID, bound to one beneficiary forever.
    pub original_transaction_id: String,
    /// Exact configured product ID.
    pub product_id: String,
    /// StoreKit appAccountToken in UUID form.
    pub app_account_token: String,
    /// Signed bundle ID.
    pub bundle_id: String,
    /// Signed Apple environment.
    pub environment: StoreEnvironment,
    /// Apple purchase time in Unix milliseconds.
    pub purchased_at_ms: u64,
    /// Apple subscription expiry; absent only for a configured storage product.
    pub expires_at_ms: Option<u64>,
    /// Verified refund or revocation time.
    pub revoked_at_ms: Option<u64>,
    /// Apple's signedDate, used to reject older observations.
    pub signed_at_ms: u64,
    /// Time the trusted verifier refreshed current status through Apple's server API.
    pub checked_at_ms: u64,
}
