//! Google Play facts submitted by governed verification services. Purchase
//! tokens remain off-chain; a client billing callback never grants rights.
use crate::{Hash, billing::PlanId, membership::Beneficiary};
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

/// Google test purchases use the same API, independently of business staging.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PlayEnvironment {
    /// A real Google Play purchase.
    Production,
    /// A purchase identified as a test by Google's authenticated response.
    Test,
}

/// Exact storefront selection; price and offer tokens come from Play Billing.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PlayProductKind {
    /// Auto-renewing base membership. Prepaid, installment and rental products
    /// are not silently interpreted as an annual dMsg subscription.
    Plan {
        /// Existing governed dMsg tier.
        plan: PlanId,
        /// Exact Google base plan.
        base_plan_id: String,
    },
    /// A consumable storage addon for the current Google subscription term.
    Storage {
        /// Existing governed dMsg storage product.
        product: Hash,
        /// Exact Google one-time purchase option.
        purchase_option_id: String,
    },
}

/// One allowed product/option/offer tuple. IDs do not encode a dMsg tier.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlayProduct {
    /// Exact Google product identifier.
    pub product_id: String,
    /// Null selects the base offer, never an arbitrary discount.
    pub offer_id: Option<String>,
    /// Entitlement and exact option controlled by governance.
    pub kind: PlayProductKind,
    /// Disabling new purchases retains verification of existing receipts.
    pub enabled: bool,
}

/// Independent admission policy; an Apple verifier is not implicitly admitted.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GooglePlayConfig {
    /// Authorized Google verification service principals.
    pub verifiers: Vec<Principal>,
    /// Exact Android package, including any distribution suffix.
    pub package_name: String,
    /// Primary purchase environment.
    pub environment: PlayEnvironment,
    /// Explicit production opt-in for test base subscriptions, not consumables.
    pub accept_test: bool,
    /// Governed product/option/offer mappings; no inferred SKU meaning.
    pub products: Vec<PlayProduct>,
}

impl GooglePlayConfig {
    /// Whether a newly submitted purchase may enter this environment.
    pub fn accepts(&self, environment: &PlayEnvironment) -> bool {
        self.environment == *environment
            || (self.environment == PlayEnvironment::Production
                && *environment == PlayEnvironment::Test
                && self.accept_test)
    }
}

/// Current provider state. Cancellation of renewal retains access until expiry.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PlayState {
    /// Payment is not complete; must not deliver or acknowledge.
    Pending,
    /// Current Google subscription or one-time purchase is active.
    Active,
    /// Google retains subscription access during its grace period.
    Grace,
    /// Renewal is canceled; current access still follows Google's expiry.
    Canceled,
    /// Google subscription is paused.
    Paused,
    /// Google subscription is suspended for a payment problem.
    OnHold,
    /// Subscription has ended, including immediate provider revocation.
    Expired,
    /// A pending subscription was canceled before payment.
    PendingCanceled,
    /// A one-time purchase is canceled or fully refunded.
    Refunded,
}

/// Facts obtained from authenticated Developer APIs, not client JSON or RTDN
/// event order. The verifier must re-read current state before granting access.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlayTransaction {
    /// Exact entitlement beneficiary bound by Google's obfuscated account ID.
    pub beneficiary: Beneficiary,
    /// SHA-256 of the raw Google purchase token; the token is never on-chain.
    pub token_hash: Hash,
    /// Latest successful order for subscriptions; optional for one-time products.
    pub order_id: Option<String>,
    /// Exact selected Google product ID.
    pub product_id: String,
    /// Base plan or purchase option ID, depending on the governed kind.
    pub option_id: String,
    /// Exact observed offer; absent for the base offer.
    pub offer_id: Option<String>,
    /// Stable 64-character account binding, independently recomputed here.
    pub account_binding: String,
    /// Package under which the Developer API verified the token.
    pub package_name: String,
    /// Environment from Google's test-purchase field.
    pub environment: PlayEnvironment,
    /// Current verified subscription or product state.
    pub state: PlayState,
    /// Start of the owned interval: max of the order period anchor and this
    /// token's subscription start. One-time products use completion time.
    pub purchased_at_ms: u64,
    /// Original order's billing-period anchor. A deferred replacement transfers
    /// its remaining interval without starting a new paid year. Absent for
    /// storage and earlier non-transferred observations.
    pub term_starts_at_ms: Option<u64>,
    /// Current subscription expiry from subscriptionsv2, never an order snapshot.
    pub expires_at_ms: Option<u64>,
    /// Actual predecessor token identified by Google's linkedPurchaseToken.
    pub linked_token_hash: Option<Hash>,
    /// Storage only: original base contract, bound by Google's obfuscated
    /// profile ID. Recovery must not attach an old purchase to a new term.
    pub base_contract_id: Option<Hash>,
    /// The provider identifies this token as replaced; it cannot revive later.
    pub replaced: bool,
    /// Durable observation order assigned by the verifier for this token.
    pub revision: u64,
    /// Time current Google state was read; must be fresh at canister application.
    pub checked_at_ms: u64,
}

/// Authority-side reconciliation result, readable only by configured verifiers.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlayReceipt {
    /// Stable receipt identity.
    pub transaction_id: Hash,
    /// Original, immutable recipient.
    pub beneficiary: Beneficiary,
    /// Last accepted provider observation order.
    pub revision: u64,
    /// Last actual provider check.
    pub checked_at_ms: u64,
    /// Whether this receipt ever produced its entitlement (not just a callback).
    pub delivered: bool,
    /// Current provider state.
    pub state: PlayState,
    /// Fixed storage term end, or current subscription expiry.
    pub expires_at_ms: Option<u64>,
}
