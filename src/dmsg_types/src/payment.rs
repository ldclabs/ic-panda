//! Ledger escrow contracts for optional paid delivery.
//!
//! Recipient authorization, relay admission, the funds decision and successful
//! ledger transfers are separate facts. Times are Unix milliseconds except
//! ICRC created_at_time; amounts are integer ledger base units.
use crate::profiles::delivery::Quote;
use crate::*;
use candid::{CandidType, Principal};
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// Relay Ed25519 key authorized to sign quotes and admission receipts.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ReceiptSigner {
    /// Signer-key version used to select the relay verification key.
    pub epoch: u64,
    /// 32-byte Ed25519 verification key of the authorized relay.
    pub public_key: Hash,
    /// Inclusive beginning of signer validity in Unix milliseconds.
    pub valid_from: u64,
    /// Exclusive end of signer validity in Unix milliseconds.
    pub valid_until: u64,
    /// Whether this signer has been revoked.
    pub revoked: bool,
}
/// Escrow service deployment, ledger fees, signer and admission limits.
/// All monetary values are integer ledger base units, not display tokens or cycles.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PaymentInit {
    /// User canister authoritative for this account or deployment.
    pub home_user: Principal,
    /// ICRC ledger canister for all amounts in this contract.
    pub ledger: Principal,
    /// ICRC account receiving the service fee.
    #[serde(with = "crate::account::account_cbor")]
    pub platform: Account,
    /// Platform service fee in integer ledger base units.
    pub service_fee: u128,
    /// Configured network fee in ledger base units.
    pub ledger_fee: u128,
    /// Maximum permitted network fee in ledger base units.
    pub max_fee: u128,
    /// Authorized quote/admission receipt signer.
    pub signer: ReceiptSigner,
    /// Maximum simultaneously open escrows per payer.
    pub max_open_per_payer: u32,
    /// Maximum new escrow orders per daily budget window.
    pub daily_orders: u32,
    /// Whether new payment orders are enabled.
    pub enabled: bool,
}
/// Recipient device authorization of fixed payment terms.
/// Signed under `dmsg/payment-offer/v1`; does not itself open or fund an escrow.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PaymentOffer {
    /// Stable 12-byte dMsg account identity; not a Principal or ledger account.
    pub account_id: AccountId,
    /// 32-byte device identifier, distinct from its signing public key.
    pub device_id: Hash,
    /// Account security revision used to invalidate stale approvals.
    pub security_epoch: u64,
    /// Payment canister authoritative for this escrow/offer.
    pub home_payment: Principal,
    /// Recipient-generated payment offer identifier.
    pub offer_id: Hash,
    /// ICRC ledger canister for all amounts in this contract.
    pub ledger: Principal,
    /// ICRC account receiving the quoted net payment.
    #[serde(with = "crate::account::account_cbor")]
    pub recipient: Account,
    /// Amount owed to the recipient, excluding fees, in ledger base units.
    pub recipient_net: u128,
    /// Opaque commitment binding the quote to the offer application scope.
    pub quote_scope: Hash,
    /// Revision of this record/authorization, used for stale-state checks.
    pub version: u64,
    /// Offer issuance time in Unix milliseconds (unlike Statement::issued_at).
    pub issued_at: u64,
    /// Exclusive deadline in Unix milliseconds (`now < expires_at`).
    pub expires_at: u64,
}
/// Payment offer and its device Ed25519 signature over the offer digest.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SignedOffer {
    /// Recipient authorization signed by a permitted device.
    pub offer: PaymentOffer,
    /// Device Ed25519 signature over the `dmsg/payment-offer/v1` digest.
    pub signature: ByteBuf,
}

/// Mutually exclusive escrow disposition; does not mean transfers have finished.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum FundsDecision {
    /// Neither settlement nor refund has been committed.
    Pending,
    /// Settlement selected; recipient/platform transfers may still be pending.
    SettlementCommitted,
    /// Refund selected; refund transfers may still be pending.
    RefundCommitted,
}

/// Verified ledger deposit and its remaining refundable allocation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Deposit {
    /// Verified deposit ledger transaction index.
    pub block: u64,
    /// Source ICRC account verified from the ledger deposit.
    #[serde(with = "crate::account::account_cbor")]
    pub from: Account,
    /// Verified incoming amount in integer ledger base units.
    pub amount: u128,
    /// Time the deposit was recorded, in Unix milliseconds.
    pub committed_at: u64,
    /// Deposit allocation available for refund before the refund network fee.
    pub refundable: u128,
}
/// One outgoing ledger transfer status, independent of the escrow decision.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum LegStatus {
    /// Transfer is prepared but has no confirmed ledger result.
    Pending,
    /// Ledger transfer call is in flight.
    InFlight,
    /// Ledger outcome is uncertain; reconcile this escrow/leg before retrying.
    Unknown,
    /// The approved fee or available fee reserve cannot support the transfer.
    FeeBlocked,
    /// Ledger confirmed this transfer.
    Succeeded,
    /// Ledger definitively rejected this transfer.
    Rejected,
    /// A fee revision replaced this leg; do not execute it again.
    Superseded,
}
/// Destination/purpose of an escrow transfer.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum LegKind {
    /// Net payment to the recipient.
    Recipient,
    /// Platform service-fee payment.
    Platform,
    /// Refund of a specific deposit allocation.
    Refund {
        /// Ledger block identifying the deposit to refund.
        funding_block: u64,
    },
    /// Return unused primary funding fee reserve to the payer.
    ReserveRefund,
}
/// One outgoing ICRC transfer with stable deduplication parameters.
/// Reconcile unknown outcomes before retrying; preserve memo and created_at_time.
/// Amounts and fees are integer ledger base units.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TransferLeg {
    /// 32-byte identifier of the escrow holding funds.
    pub escrow_id: Hash,
    /// Transfer identifier within the escrow.
    pub leg_id: u64,
    /// Beneficiary or refund allocation paid by this transfer.
    pub kind: LegKind,
    /// ICRC destination of this outgoing transfer.
    #[serde(with = "crate::account::account_cbor")]
    pub to: Account,
    /// Outgoing amount excluding the network fee, in ledger base units.
    pub amount: u128,
    /// Ledger network fee in integer base units.
    pub fee: u128,
    /// Fixed 32-byte ledger memo used for transfer correlation/deduplication.
    pub memo: Hash,
    /// Fixed ICRC transfer timestamp in Unix nanoseconds, preserved on retries.
    pub created_at_time: u64,
    /// Ledger execution state of this particular transfer leg.
    pub status: LegStatus,
    /// Confirmed ledger transaction index, if available.
    pub block: Option<u64>,
    /// Network fee reported by the ledger after a BadFee response, if available.
    #[serde(default)]
    pub expected_fee: Option<u128>,
    /// Fee-repricing revision of this transfer.
    #[serde(default)]
    pub revision: u64,
    /// Previous transfer leg superseded by this revision, if any.
    #[serde(default)]
    pub replaces: Option<u64>,
    /// Commitment to the preceding transfer revision; zero for an initial leg.
    #[serde(default)]
    pub history_digest: Hash,
}
/// Maximum retained chain of transfer fee revisions (8); superseded ancestors may be pruned.
pub const TRANSFER_HISTORY_LIMIT: usize = 8;
/// Public escrow accounting view and certified leaf payload.
/// The certified path is the single raw escrow ID. `confirmed_in` equals
/// `liabilities + transferred + network_fees`; a committed decision does not
/// prove successful payout. This view is not the internal storage schema.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EscrowInfo {
    /// 32-byte identifier of the escrow holding funds.
    pub escrow_id: Hash,
    /// Authenticated Principal that opened the escrow.
    pub payer_principal: Principal,
    /// Idempotency identifier; reuse only with identical operation parameters.
    pub op_id: OpId,
    /// Fixed relay quote accepted when the escrow was opened.
    pub quote: Quote,
    /// `dmsg/quote/v1` commitment to the exact accepted quote.
    pub quote_digest: Hash,
    /// 32-byte escrow deposit subaccount owned by the payment canister.
    pub subaccount: Hash,
    /// Ledger block selected as the primary funding deposit, if any.
    pub funding_ref: Option<u64>,
    /// Primary funding time in Unix milliseconds, if funded.
    pub funded_at: Option<u64>,
    /// Irreversible choice of settlement or refund, separate from payout progress.
    pub decision: FundsDecision,
    /// Commitment to the admitted receipt chosen for settlement, if any.
    pub receipt_digest: Option<Hash>,
    /// Revision of this record/authorization, used for stale-state checks.
    pub version: u64,
    /// Total verified deposits in integer ledger base units.
    pub confirmed_in: u128,
    /// Funds still owed/reserved by the escrow in ledger base units.
    pub liabilities: u128,
    /// Total successful outgoing transfer amounts, excluding network fees, in base units.
    pub transferred: u128,
    /// Total successful outgoing network fees in ledger base units.
    pub network_fees: u128,
}
