use crate::profiles::delivery::Quote;
use crate::*;
use candid::{CandidType, Principal};
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ReceiptSigner {
    pub epoch: u64,
    pub public_key: Hash,
    pub valid_from: u64,
    pub valid_until: u64,
    pub revoked: bool,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PaymentInit {
    pub home_user: Principal,
    pub ledger: Principal,
    #[serde(with = "crate::account::account_cbor")]
    pub platform: Account,
    pub service_fee: u128,
    pub ledger_fee: u128,
    pub max_fee: u128,
    pub signer: ReceiptSigner,
    pub max_open_per_payer: u32,
    pub daily_orders: u32,
    pub enabled: bool,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PaymentOffer {
    pub account_id: AccountId,
    pub device_id: Hash,
    pub security_epoch: u64,
    pub home_payment: Principal,
    pub offer_id: Hash,
    pub ledger: Principal,
    #[serde(with = "crate::account::account_cbor")]
    pub recipient: Account,
    pub recipient_net: u128,
    pub quote_scope: Hash,
    pub version: u64,
    pub issued_at: u64,
    pub expires_at: u64,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SignedOffer {
    pub offer: PaymentOffer,
    pub signature: ByteBuf,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum FundsDecision {
    Pending,
    SettlementCommitted,
    RefundCommitted,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Deposit {
    pub block: u64,
    #[serde(with = "crate::account::account_cbor")]
    pub from: Account,
    pub amount: u128,
    pub committed_at: u64,
    pub refundable: u128,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum LegStatus {
    Pending,
    InFlight,
    Unknown,
    FeeBlocked,
    Succeeded,
    Rejected,
    Superseded,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum LegKind {
    Recipient,
    Platform,
    Refund { funding_block: u64 },
    ReserveRefund,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TransferLeg {
    pub escrow_id: Hash,
    pub leg_id: u64,
    pub kind: LegKind,
    #[serde(with = "crate::account::account_cbor")]
    pub to: Account,
    pub amount: u128,
    pub fee: u128,
    pub memo: Hash,
    /// Fixed ICRC transfer timestamp in Unix nanoseconds, preserved on retries.
    pub created_at_time: u64,
    pub status: LegStatus,
    pub block: Option<u64>,
    #[serde(default)]
    pub expected_fee: Option<u128>,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub replaces: Option<u64>,
    #[serde(default)]
    pub history_digest: Hash,
}
pub const TRANSFER_HISTORY_LIMIT: usize = 8;
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EscrowInfo {
    pub escrow_id: Hash,
    pub payer_principal: Principal,
    pub op_id: OpId,
    pub quote: Quote,
    pub quote_digest: Hash,
    pub subaccount: Hash,
    pub funding_ref: Option<u64>,
    pub funded_at: Option<u64>,
    pub decision: FundsDecision,
    pub receipt_digest: Option<Hash>,
    pub version: u64,
    pub confirmed_in: u128,
    pub liabilities: u128,
    pub transferred: u128,
    pub network_fees: u128,
}
