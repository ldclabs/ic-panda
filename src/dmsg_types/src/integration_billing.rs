//! Operational v2 checkout, product authorization and shared subscription contracts.
//! All money is integer atomic units or explicitly named USD micro units.
use crate::{integration::*, membership::Eligibility, AccountId, Environment, Hash};
use candid::{CandidType, Principal};
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};

/// Initially supported stable assets, bound to fixed ledger IDs outside Local.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SettlementAssetKind {
    /// ckUSDT on ICP.
    CkUsdt,
    /// ckUSDC on ICP.
    CkUsdc,
}

/// Governance-approved asset/price snapshot. Pausing admission never removes refund routing.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SettlementAsset {
    /// Protocol version two.
    pub version: u16,
    /// Immutable revision for this ledger.
    pub policy_version: u64,
    /// Deployment domain.
    pub environment: Environment,
    /// Ledger identity, not a symbol.
    pub ledger: Principal,
    /// Declared kind, checked against the configured ledger identity.
    pub asset: SettlementAssetKind,
    /// Verified atomic precision; initial assets require six decimals.
    pub decimals: u16,
    /// Explicit reference USD micro price for one full token.
    pub price_usd_micros: u128,
    /// Time of the trusted price observation.
    pub price_observed_at_ms: u64,
    /// Exclusive validity end of that price observation.
    pub price_valid_until_ms: u64,
    /// Latest verified ledger network fee.
    pub network_fee_atomic: u128,
    /// Per-transfer cap accepted for new orders.
    pub max_network_fee_atomic: u128,
    /// New-quote admission only.
    pub enabled: bool,
}

/// Explicit product permission; not a claim that the payer has a project role.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProductApproval {
    /// Protocol version two.
    pub version: u16,
    /// Product-owned approval identity.
    pub approval_id: Hash,
    /// Exact bill accepted by the product operator.
    pub offer_hash: Hash,
    /// Operator whose product permissions must remain valid at delivery.
    pub operator: Principal,
    /// Accepted settlement method.
    pub method: SettlementMethod,
    /// Approval time.
    pub approved_at_ms: u64,
    /// Exclusive application deadline, never extended on retry.
    pub expires_at_ms: u64,
}

/// Inputs authenticated by checkout/membership before asking the product authority.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProductAuthorizationRequest {
    /// Exact authoritative bill.
    pub offer: BillingOffer,
    /// dMsg device-approved account and economic actor.
    pub account_approval: ApplicationApproval,
    /// Registered dMsg user home that verified the approval.
    pub user_home: Principal,
    /// Exact device approval record.
    pub approval_id: Hash,
    /// Explicit product approval; a personal-account adapter may derive owner consent from account_approval.
    pub product_approval: Option<ProductApproval>,
}

/// Fresh replicated product authorization; no interval is reserved by this read.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProductAuthorization {
    /// Digest of all authorization inputs.
    pub request_hash: Hash,
    /// Authenticated product operator, separate from the economic actor.
    pub operator: Principal,
    /// Check time.
    pub verified_at_ms: u64,
    /// Exclusive freshness boundary.
    pub valid_until_ms: u64,
}

/// Fixed cash terms, including independently verified asset policy.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CheckoutQuote {
    /// Frozen registered merchant, subject schema and adapter configuration.
    pub product: ProductRegistration,
    /// Product-authoritative USD bill.
    pub offer: BillingOffer,
    /// Immutable one-ledger payment and refund terms.
    pub cash: CashQuote,
    /// Accepted conversion and network-fee policy.
    pub asset: SettlementAsset,
    /// Time used to compute the fixed funding/activation deadlines.
    pub quoted_at_ms: u64,
}

/// Opening never transfers funds. Transfer only after an accepted order exists.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OpenCheckout {
    /// Accepted immutable quote.
    pub quote: CheckoutQuote,
    /// Account and product permission input.
    pub authorization: ProductAuthorizationRequest,
}

/// Same operation identity is retained through every uncertain result.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum CheckoutStatus {
    /// Product interval reservation is being reconciled; do not fund yet.
    Reserving,
    /// Order and its interval are accepted; the payer may transfer once.
    AwaitingFunding,
    /// Funds and delivery decision committed; adapter acknowledgement is pending.
    Applying,
    /// Product applied rights and a durable receipt.
    Applied,
    /// Definitive nondelivery or unstarted cancellation; original deposits are refundable.
    RefundCommitted,
    /// No order permission was granted.
    Rejected,
}

/// Safe public progress without disclosing private product bill/account data.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CheckoutProgress {
    /// Original order identity.
    pub order_id: Hash,
    /// Current state, not inferred from a wallet reply.
    pub status: CheckoutStatus,
    /// Immutable delivery identity when prepared.
    pub decision_id: Option<Hash>,
}

/// Owner/adapter view, used for recovery and the billing centre.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CheckoutView {
    /// Identity and public state.
    pub progress: CheckoutProgress,
    /// Original fixed terms.
    pub quote: CheckoutQuote,
    /// Product receipt if obtained.
    pub receipt: Option<ProductReceipt>,
    /// Cumulative allocated transfers for the selected asset, including their fees.
    pub outgoing_atomic: u128,
    /// Amount of price still reserved, never withdrawable before earned.
    pub service_reserve_atomic: u128,
    /// Remaining customer-funded network reserve.
    pub fee_reserve_atomic: u128,
}

/// A ledger-scoped block reference.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CashBlock {
    /// Authenticated ledger source.
    pub ledger: Principal,
    /// Native ledger block index; checked before narrowing in an adapter.
    pub block_index: u128,
}

/// Result of registering a real deposit, including wrong-source/late/excess deposits.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CheckoutDeposit {
    /// Original order receiving these funds.
    pub order_id: Hash,
    /// Ledger-scoped identity, never a global bare block number.
    pub block: CashBlock,
    /// Original source account; refunds cannot substitute another destination.
    #[serde(with = "crate::account::account_cbor")]
    pub from: Account,
    /// Actual amount received.
    pub amount_atomic: u128,
    /// Still-unallocated refund obligation to the original source.
    pub refundable_atomic: u128,
}

/// Network transfer lifecycle. Unknown is never replaced with another memo/time.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum CashTransferStatus {
    /// Durable frozen transfer before dispatch.
    Pending,
    /// Call may still execute.
    InFlight,
    /// Original transfer must be retried or matched to a trusted block.
    Unknown,
    /// Known ledger rejection; no prior unknown attempt exists.
    Rejected,
    /// Actual block confirmed or ledger Duplicate returned.
    Succeeded,
    /// Replaced only after a known rejection; never dispatch this leg again.
    Superseded,
}

/// Each outgoing leg owns its ledger, source, destination, fee, memo and timestamp.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CashTransfer {
    /// Globally unique immutable transfer identity.
    pub transfer_id: Hash,
    /// Source order, never another merchant's pool.
    pub order_id: Hash,
    /// Immutable ledger.
    pub ledger: Principal,
    /// Source order subaccount.
    pub source_subaccount: Hash,
    /// Approved immutable destination.
    #[serde(with = "crate::account::account_cbor")]
    pub to: Account,
    /// Net amount to deliver.
    pub amount_atomic: u128,
    /// Frozen fee for this exact attempt.
    pub fee_atomic: u128,
    /// Limit accepted by the responsible payer/merchant.
    pub max_fee_atomic: u128,
    /// Stable ledger deduplication memo.
    pub memo: Hash,
    /// Stable sender time in nanoseconds, not the funding proof's ledger timestamp.
    pub created_at_time_ns: u64,
    /// Actual transfer state.
    pub status: CashTransferStatus,
    /// Confirmed block when succeeded.
    pub block_index: Option<u128>,
    /// Ledger-reported replacement fee, not permission to use it.
    pub expected_fee_atomic: Option<u128>,
    /// Stable diagnostic code without private payloads.
    pub error_code: Option<String>,
    /// A known-rejected predecessor, if any.
    pub replaces: Option<Hash>,
    /// A replacement leg prevents replay of this one.
    pub replaced_by: Option<Hash>,
}

/// Common product contract source: included service is neither cash nor a PANDA payment.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SubscriptionSource {
    /// Product-funded base term.
    Included,
    /// External cash order and ledger provenance.
    Cash {
        /// Original checkout order.
        order_id: Hash,
        /// Selected ledger.
        ledger: Principal,
        /// Authenticated funding block.
        block_index: u128,
        /// Product price excluding network reserves.
        amount_atomic: u128,
    },
    /// Full fee waiver; no withdrawable/refundable cash balance is created.
    PandaClaim {
        /// Shared qualification record.
        claim_id: Hash,
        /// Exact full-waiver quotation and rate snapshot.
        quote: PandaQuote,
    },
}

/// Entitlement termination does not release an accepted PANDA commitment.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SubscriptionStatus {
    /// Contract may issue rights inside its interval and source lease.
    Active,
    /// Known ineligibility; new PANDA rights paused while repair is possible.
    Repairing,
    /// Qualification cannot be verified; repair clock is paused.
    Unverifiable,
    /// Rights ended permanently for this contract; original commitment remains.
    Terminated,
    /// Natural end reached.
    Expired,
    /// Only an unstarted cash term may be cancelled.
    Cancelled,
}

/// Shared immutable contract and mutable short entitlement projection.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SubscriptionContract {
    /// Immutable identity.
    pub contract_id: Hash,
    /// Product bill and interval.
    pub offer: BillingOffer,
    /// Actual settlement or included-service provenance.
    pub source: SubscriptionSource,
    /// Terminal authority decision that created it, absent for included terms.
    pub decision_id: Option<Hash>,
    /// Atomic delivery time, distinct from interval start.
    pub applied_at_ms: u64,
    /// Product business revision when applied.
    pub business_revision: u64,
    /// Qualification refresh revision, never a business CAS revision.
    pub lease_revision: u64,
    /// Current contract state.
    pub status: SubscriptionStatus,
    /// Most recent qualified lease end.
    pub lease_until_ms: u64,
    /// Maximum rights already issued; never decreases.
    pub max_issued_until_ms: u64,
    /// Latest trusted qualification observation.
    pub qualification: Eligibility,
    /// Latest observation used for the repair clock.
    pub observed_at_ms: u64,
    /// Accumulated known-ineligible time; Unverifiable intervals do not add time.
    pub repair_elapsed_ms: u64,
}

/// Durable permission to cancel exactly one unstarted cash term.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CashCancellationReceipt {
    /// Checkout order to refund.
    pub order_id: Hash,
    /// Product contract cancelled atomically.
    pub contract_id: Hash,
    /// Full original decision commitment.
    pub decision_hash: Hash,
    /// Time of the cancellation.
    pub cancelled_at_ms: u64,
    /// False is a durable refusal, not an unknown cancellation.
    pub cancelled: bool,
    /// Product's new business revision.
    pub business_revision: u64,
}

/// Public transfer advancement does not disclose payer or merchant account details.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CashTransferProgress {
    /// Original immutable leg.
    pub transfer_id: Hash,
    /// Authoritative progress.
    pub status: CashTransferStatus,
    /// Ledger block when known.
    pub block_index: Option<u128>,
}

impl From<CashTransfer> for CashTransferProgress {
    fn from(t: CashTransfer) -> Self {
        Self {
            transfer_id: t.transfer_id,
            status: t.status,
            block_index: t.block_index,
        }
    }
}

/// Browser request carries product terms, never a wallet key or an inferred subject.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CheckoutRequest {
    /// Authoritative bill, verified again by the service.
    pub offer: BillingOffer,
    /// Expected dMsg workspace account; independent from beneficiary.
    pub approving_account: AccountId,
    /// Product permission from the initiating operator, if required by its adapter.
    pub product_approval: Option<ProductApproval>,
    /// Explicit full cash payment or full PANDA waiver.
    pub method: SettlementMethod,
}

/// Asset discovery remains available while quotes are paused.
#[derive(Clone, CandidType, Serialize, Deserialize)]
pub struct SettlementAssetView {
    /// Exact current asset policy.
    pub policy: SettlementAsset,
    /// Metadata/block format has been verified for this policy.
    pub ledger_verified: bool,
}

/// Exact per-order accounting in one ledger; amounts in distinct ledgers are never added.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CheckoutLedgerBalance {
    /// Asset identity.
    pub ledger: Principal,
    /// Confirmed incoming amount.
    pub incoming_atomic: u128,
    /// Obligations to original deposit sources.
    pub refundable_atomic: u128,
    /// Product price not yet allocated to a merchant transfer.
    pub service_reserve_atomic: u128,
    /// Customer-funded network fee reserve.
    pub fee_reserve_atomic: u128,
    /// All allocated outgoing obligations, including their exact fees.
    pub outgoing_atomic: u128,
}

/// Private operational observation; delivery continues to require the service's durable receipt.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CheckoutOperationAudit {
    /// Original order and receipt.
    pub order: CheckoutView,
    /// Independently conserved balances.
    pub balances: Vec<CheckoutLedgerBalance>,
}

/// Bounded owner/merchant/governance scan. Cursor is the last scanned key, not the last match.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CheckoutOperationsPage {
    /// Accessible orders.
    pub orders: Vec<CheckoutOperationAudit>,
    /// Continue after this key; null means the scan reached the end.
    pub next: Option<Hash>,
}

/// Outgoing legs visible to their recipient or the corresponding order reader.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CashTransfersPage {
    /// Exact immutable transfer arguments and state.
    pub transfers: Vec<CashTransfer>,
    /// Last scanned key, including inaccessible rows.
    pub next: Option<Hash>,
}
