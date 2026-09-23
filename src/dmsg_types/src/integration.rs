//! Third-party contracts. Values alone are never evidence of authorization.
use crate::{membership::Beneficiary, AccountId, Environment, Hash};
use candid::{CandidType, Principal};
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};

/// External integration envelope version.
pub const INTEGRATION_VERSION: u16 = 1;
/// Generic checkout and irrevocable membership contract version.
pub const COMMERCE_VERSION: u16 = 2;
/// Browser transport; no downgrade to the document-only bridge.
pub const EXTENSION_PROTOCOL: &str = "dmsg-extension/4";
/// Dedicated authentication media type (a certified leaf, not a COSE document).
pub const AUTHENTICATION_TYPE: &str = "application/vnd.dmsg.authentication+cbor;v=1";
/// Dedicated application action COSE profile.
pub const APP_ACTION_TYPE: &str = "application/vnd.dmsg.app-action+cose;v=1";
/// Maximum lifetime of an external authentication request and proof.
pub const AUTH_TTL_MS: u64 = 5 * crate::MINUTE;
/// Maximum session derived from a dMsg proof without fresh authentication.
pub const MAX_SESSION_MS: u64 = 24 * 60 * crate::MINUTE;
/// Maximum acceptance window of an unaccepted business offer.
pub const OFFER_TTL_MS: u64 = 15 * crate::MINUTE;
/// Maximum cash funding interval from the accepted quote.
pub const CASH_FUNDING_MS: u64 = 30 * crate::MINUTE;
/// Maximum initial activation interval; Unknown still requires reconciliation.
pub const CASH_ACTIVATION_MS: u64 = 24 * 60 * crate::MINUTE;
/// Maximum PANDA application interval, also bounded by the contract end.
pub const APPLICATION_TTL_MS: u64 = 24 * 60 * crate::MINUTE;
/// Minimum cooling includes a five-minute margin beyond the one-hour lease.
pub const PANDA_COOLING_MS: u64 = 65 * crate::MINUTE;
/// Maximum live qualification lease, always truncated to the contract end.
pub const PANDA_LEASE_MS: u64 = 60 * crate::MINUTE;
/// Advance notice required for ordinary PANDA rate changes.
pub const POLICY_NOTICE_MS: u64 = 30 * crate::DAY;
/// Maximum retained in-flight external operations per account.
pub const MAX_ACCOUNT_OPERATIONS: usize = 32;
/// Maximum successful external approvals per account per UTC hour.
pub const MAX_HOURLY_APPROVALS: u32 = 60;
/// Maximum origins and product bindings per application.
pub const MAX_APP_BINDINGS: usize = 16;
/// User-selected cash asset identities. Metadata is verified separately at deployment.
pub const CKUSDT_LEDGER: &str = "cngnf-vqaaa-aaaar-qag4q-cai";
/// User-selected cash asset identity; never identified by symbol alone.
pub const CKUSDC_LEDGER: &str = "xevnm-gaaaa-aaaar-qafnq-cai";

/// Explicit external capability; never grants access to secrets or wallet keys.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum AppCapability {
    /// Account authentication only.
    Authenticate,
    /// One of the three existing closed document profiles.
    SignDocument,
    /// A registered, typed product action.
    SignAction,
    /// Exact business offer approval.
    Checkout,
}

/// Closed signing profiles that an application may request.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SigningProfile {
    /// Existing raw UTF-8 text statement profile.
    TextStatementV1,
    /// Existing SHA-256 digest statement profile.
    DigestStatementV1,
    /// Existing text-plus-file statement profile.
    FileStatementV1,
    /// Typed application action profile.
    AppActionV1,
}

/// Governance-pinned application identity and browser boundary.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AppRegistration {
    /// Wire version, currently one.
    pub version: u16,
    /// Deployment separation.
    pub environment: Environment,
    /// Stable application identifier.
    pub app_id: String,
    /// Monotonically increasing configuration version.
    pub config_version: u64,
    /// Canonical exact browser origins, never URL prefixes.
    pub origins: Vec<String>,
    /// Pinned user canisters allowed to authenticate accounts.
    pub user_homes: Vec<Principal>,
    /// Pinned execution canisters allowed to sign artifacts.
    pub cose_homes: Vec<Principal>,
    /// Product IDs allowed for this application.
    pub product_ids: Vec<String>,
    /// Explicit capability allowlist.
    pub capabilities: Vec<AppCapability>,
    /// Closed signing profile allowlist, empty for an authentication-only app.
    pub profiles: Vec<SigningProfile>,
    /// Receiver of external account authentication proofs.
    pub authentication_receiver: Principal,
    /// Product authority that confirms prepared actions and signer-account linkage.
    pub action_authority: Principal,
    /// Stops new approvals; does not erase accepted commitments.
    pub paused: bool,
}

/// Product identity, subject schema and settlement boundary.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProductRegistration {
    /// Commerce version, currently two.
    pub version: u16,
    /// Deployment separation.
    pub environment: Environment,
    /// Stable product identifier.
    pub product_id: String,
    /// Monotonically increasing configuration version.
    pub config_version: u64,
    /// Canister supplying authoritative offers.
    pub quote_authority: Principal,
    /// Canister supplying beneficiary permissions.
    pub beneficiary_authority: Principal,
    /// Canister applying product decisions.
    pub adapter: Principal,
    /// Fixed interpretation of subject bytes.
    pub subject_schema: String,
    /// Exact subject byte length.
    pub subject_size: u16,
    /// Merchant settlement identity, separate from the beneficiary.
    #[serde(with = "crate::account::account_cbor")]
    pub merchant: Account,
    /// Enabled ledgers, explicitly configured even in local fixtures.
    pub ledgers: Vec<Principal>,
    /// Current product terms commitment.
    pub terms_hash: Hash,
    /// Governance budget namespace, not a cash balance.
    pub subsidy_budget_id: Hash,
    /// Stops new commitments only.
    pub paused: bool,
}

/// Authentication purpose prevents login/link/reauthentication replay.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum AuthenticationPurpose {
    /// Sign in or register a local product account.
    Login,
    /// Link to an already authenticated product account.
    Link,
    /// Confirm recent control for a sensitive product action.
    Reauthenticate,
}

/// Exact browser authentication request approved by a dMsg device.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthenticationRequest {
    /// Authentication profile version.
    pub version: u16,
    /// Deployment separation.
    pub environment: Environment,
    /// Registered application.
    pub app_id: String,
    /// Exact registration version used at approval time.
    pub app_config_version: u64,
    /// Actual extension-verified origin.
    pub origin: String,
    /// Pinned product authentication receiver.
    pub receiver: Principal,
    /// Product challenge commitment, including any local account identity.
    pub challenge_hash: Hash,
    /// SHA-256 of the product's temporary session public-key DER.
    pub session_key_hash: Hash,
    /// Purpose checked against the local pending challenge.
    pub purpose: AuthenticationPurpose,
    /// Fresh product-generated nonce.
    pub nonce: Hash,
    /// Stable operation identity used during recovery.
    pub operation_id: Hash,
    /// Inclusive request time in Unix milliseconds.
    pub issued_at_ms: u64,
    /// Exclusive authentication deadline in Unix milliseconds.
    pub expires_at_ms: u64,
}

/// Dedicated certified authentication leaf; never substitute SecuritySnapshot.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthenticationResult {
    /// Authentication profile version.
    pub version: u16,
    /// Exact approved request.
    pub request: AuthenticationRequest,
    /// Stable dMsg subject, distinct from the product's local account.
    pub account_id: AccountId,
    /// Expected IC certificate authority.
    pub home_user: Principal,
    /// Account epoch consumed at approval.
    pub security_epoch: u64,
    /// Approved device identity; not its secret key.
    pub device_id: Hash,
    /// Approval time in Unix milliseconds.
    pub approved_at_ms: u64,
    /// Exclusive validity deadline, never later than the request deadline.
    pub expires_at_ms: u64,
}

/// Mutually exclusive settlement sources. PANDA always covers the entire offer.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SettlementMethod {
    /// One explicitly enabled ledger.
    Cash,
    /// Full subscription waiver without any cash transfer.
    Panda,
}

/// Product-authoritative offer for one exact half-open subscription interval.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BillingOffer {
    /// Commerce version, currently two.
    pub version: u16,
    /// Deployment separation.
    pub environment: Environment,
    /// Registered application.
    pub app_id: String,
    /// Registered product.
    pub product_id: String,
    /// Product-issued quote identifier.
    pub offer_id: Hash,
    /// Typed recipient of product rights.
    pub beneficiary: Beneficiary,
    /// Registered authoritative quote canister.
    pub quote_authority: Principal,
    /// Registered product delivery canister.
    pub adapter: Principal,
    /// Opaque product SKU, never interpreted by checkout.
    pub sku: String,
    /// Exact product terms accepted by the user.
    pub product_terms_hash: Hash,
    /// Product CAS revision, separate from qualification lease revision.
    pub expected_business_revision: u64,
    /// Entire interval price in USD micro units, not an annualized price.
    pub amount_usd_micros: u128,
    /// Inclusive subscription start in Unix milliseconds.
    pub starts_at_ms: u64,
    /// Exclusive immutable end in Unix milliseconds.
    pub expires_at_ms: u64,
    /// Quote issuance in Unix milliseconds.
    pub issued_at_ms: u64,
    /// Exclusive initial acceptance deadline; not an activation deadline.
    pub accept_by_ms: u64,
    /// Operation identity fixed across retries.
    pub operation_id: Hash,
    /// Explicit enabled settlement sources, without duplicates.
    pub allowed_settlement_methods: Vec<SettlementMethod>,
}

/// Immutable governance conversion policy; not a market-price oracle.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PandaRatePolicy {
    /// Commerce version.
    pub version: u16,
    /// Immutable policy revision.
    pub policy_version: u64,
    /// Deployment separation.
    pub environment: Environment,
    /// Products eligible for this policy.
    pub product_ids: Vec<String>,
    /// Positive PANDA/USD numerator.
    pub r_num: u128,
    /// Positive PANDA/USD denominator.
    pub r_den: u128,
    /// Policy publication time in Unix milliseconds.
    pub published_at_ms: u64,
    /// Inclusive policy activation time, after the notice interval.
    pub effective_at_ms: u64,
    /// Governance budget namespace, never a ledger balance.
    pub subsidy_budget_id: Hash,
}

/// Full waiver quotation with an immutable commitment end.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PandaQuote {
    /// Time used to freeze the application deadline.
    pub quoted_at_ms: u64,
    /// Commerce version.
    pub version: u16,
    /// Complete authoritative offer commitment.
    pub offer_hash: Hash,
    /// Immutable rate policy.
    pub policy: PandaRatePolicy,
    /// Computed stake in eight-decimal PANDA atomic units.
    pub required_stake_e8s: u128,
    /// Service subsidy obligation in USD micro units, not cash revenue.
    pub subsidy_usd_micros: u128,
    /// Latest activation time, bounded by offer end and application lifetime.
    pub application_deadline_ms: u64,
    /// Original offer expiry, cannot be reduced after successful Apply.
    pub committed_until_ms: u64,
}

/// Fixed single-asset cash terms; no floating point or implicit peg.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CashQuote {
    /// Commerce version.
    pub version: u16,
    /// Complete authoritative offer commitment.
    pub offer_hash: Hash,
    /// Immutable selected ledger.
    pub ledger: Principal,
    /// Product price converted using the accepted asset policy.
    pub amount_atomic: u128,
    /// Commitment to the actual conversion rate / depeg checks.
    pub conversion_hash: Hash,
    /// Approved funding account.
    #[serde(with = "crate::account::account_cbor")]
    pub payer: Account,
    /// Exact per-order receiving account.
    #[serde(with = "crate::account::account_cbor")]
    pub deposit: Account,
    /// Approved per-transfer fee ceiling.
    pub max_network_fee_atomic: u128,
    /// Separately identified fee reserve.
    pub fee_reserve_atomic: u128,
    /// Exclusive funding deadline.
    pub funding_deadline_ms: u64,
    /// Exclusive initial activation deadline, not a release rule for Unknown.
    pub activation_deadline_ms: u64,
}

/// Exact purpose for account approval, distinct from product authorization.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ApprovalPurpose {
    /// Cash order approval.
    CashCheckout,
    /// Full subscription waiver approval.
    PandaSubscription,
    /// Typed application action approval.
    AppAction,
}

/// Approved dMsg account and product beneficiary are deliberately separate.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ApplicationApproval {
    /// Integration version.
    pub version: u16,
    /// Deployment separation.
    pub environment: Environment,
    /// Registered application.
    pub app_id: String,
    /// Exact application configuration revision.
    pub app_config_version: u64,
    /// Extension-verified browser origin.
    pub origin: String,
    /// dMsg account whose device approves this action.
    pub approving_account: AccountId,
    /// Exact service allowed to consume this approval.
    pub service: Principal,
    /// Product-defined rights recipient; no inferred AccountId conversion.
    pub beneficiary: Beneficiary,
    /// Economic actor, distinct from both account IDs and payer accounts.
    pub actor: Principal,
    /// Purpose-separated authorization.
    pub purpose: ApprovalPurpose,
    /// Complete cash/PANDA quotation or application action digest.
    pub action_digest: Hash,
    /// Idempotency identifier.
    pub operation_id: Hash,
    /// Fresh challenge.
    pub nonce: Hash,
    /// Exclusive validity deadline in Unix milliseconds.
    pub expires_at_ms: u64,
}

/// Paid/waived provenance. PANDA never fabricates a ledger payment reference.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SettlementSource {
    /// Confirmed receipt of a single selected asset.
    Cash {
        /// Checkout order identity.
        order_id: Hash,
        /// Selected ledger, also scopes block deduplication.
        ledger: Principal,
        /// Authentic ICRC block index.
        block_index: u128,
        /// Actual product price, excluding the separately tracked fee reserve.
        amount_atomic: u128,
    },
    /// Full waiver requiring a live qualification lease.
    Panda {
        /// Shared membership claim identity.
        claim_id: Hash,
        /// Exact accepted quotation.
        quote_hash: Hash,
        /// Immutable commitment, even if benefit later terminates.
        committed_until_ms: u64,
        /// Current exclusive lease end, bounded by contract end.
        lease_until_ms: u64,
    },
}

/// Durable product delivery decision, persisted before the external Apply call.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProductDecision {
    /// Commerce version.
    pub version: u16,
    /// Complete accepted business offer.
    pub offer: BillingOffer,
    /// Permanent decision identity; never replaced after an unknown response.
    pub decision_id: Hash,
    /// Typed source verified against the fixed service caller.
    pub source: SettlementSource,
    /// Decision preparation time in Unix milliseconds.
    pub decided_at_ms: u64,
    /// Exclusive delivery deadline; unknown delivery never implies a refund.
    pub apply_by_ms: u64,
}

/// Terminal Apply result. Unknown is the absence of an authenticated receipt.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ProductOutcome {
    /// Rights and the receipt were committed atomically.
    Applied {
        /// Product business revision after Apply.
        business_revision: u64,
        /// Immutable contract identity.
        contract_id: Hash,
        /// Earliest permissible commitment release time.
        committed_until_ms: u64,
    },
    /// Known rejection, persisted and idempotently retrievable.
    Rejected {
        /// Stable typed reason without sensitive free-form diagnostics.
        reason: ProductRejection,
    },
}

/// Expected terminal product rejections.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ProductRejection {
    /// Business revision changed.
    RevisionConflict,
    /// Product permission is no longer valid.
    Unauthorized,
    /// Delivery deadline or permitted interval has passed.
    Expired,
    /// Another operation already owns the same interval.
    IntervalReserved,
    /// Authoritative quote or product terms no longer match.
    OfferMismatch,
}

/// Authenticated product receipt, not an untrusted browser success flag.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProductReceipt {
    /// Commerce version.
    pub version: u16,
    /// Original immutable decision identity.
    pub decision_id: Hash,
    /// Complete decision commitment; catches same-ID/different-bytes retries.
    pub decision_hash: Hash,
    /// Fixed registered delivery authority.
    pub adapter: Principal,
    /// Terminal delivery result.
    pub outcome: ProductOutcome,
    /// Atomic receipt creation time in Unix milliseconds.
    pub applied_at_ms: u64,
}

/// Fresh read-only revalidation of an exact stored application approval.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ApplicationAuthorization {
    /// Device approval request ID, distinct from the product operation ID.
    pub approval_id: Hash,
    /// Exact ApplicationApproval commitment.
    pub approval_hash: Hash,
    /// Current account security epoch.
    pub security_epoch: u64,
    /// Verification time in Unix milliseconds.
    pub verified_at_ms: u64,
    /// Exclusive deadline bounded by both application and device approval.
    pub valid_until_ms: u64,
}
