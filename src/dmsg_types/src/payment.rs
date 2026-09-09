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
    #[serde(with = "crate::ledger::account_cbor")]
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
    pub subject: SubjectId,
    pub device_id: Hash,
    pub security_epoch: u64,
    pub home_payment: Principal,
    pub offer_id: Hash,
    pub ledger: Principal,
    #[serde(with = "crate::ledger::account_cbor")]
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
pub struct Quote {
    pub quote_id: Hash,
    pub home_payment: Principal,
    #[serde(with = "crate::ledger::account_cbor")]
    pub payer: Account,
    pub offer_digest: Hash,
    pub quote_scope: Hash,
    pub ledger: Principal,
    #[serde(with = "crate::ledger::account_cbor")]
    pub recipient: Account,
    pub recipient_net: u128,
    #[serde(with = "crate::ledger::account_cbor")]
    pub platform: Account,
    pub service_fee: u128,
    pub fee_reserve: u128,
    pub amount: u128,
    pub max_network_fee: u128,
    pub max_bytes: u32,
    pub retain_ms: u64,
    pub envelope_digest: Hash,
    pub signer_epoch: u64,
    pub created_at: u64,
    pub fund_by: u64,
    pub accept_by: u64,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OpenEscrow {
    pub op_id: OpId,
    pub quote: Quote,
    pub quote_signature: ByteBuf,
    pub offer: SignedOffer,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AdmissionReceipt {
    pub protocol: u16,
    pub relay_id: Hash,
    pub signer_epoch: u64,
    pub home_payment: Principal,
    pub escrow_id: Hash,
    pub quote_digest: Hash,
    pub envelope_digest: Hash,
    pub size: u32,
    pub policy_version: u64,
    pub inbox_key_version: u64,
    pub admission_seq: u64,
    pub stored_at: u64,
    pub retain_until: u64,
    pub fund_by: u64,
    pub accept_by: u64,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SignedReceipt {
    pub receipt: AdmissionReceipt,
    pub signature: ByteBuf,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum FundsDecision {
    Pending,
    SettlementCommitted,
    RefundCommitted,
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Escrow {
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
    pub primary_remaining: u128,
    pub next_leg: u64,
    pub pending_payouts: u32,
}
impl Escrow {
    pub fn conserved(&self) -> bool {
        self.liabilities
            .checked_add(self.transferred)
            .and_then(|v| v.checked_add(self.network_fees))
            == Some(self.confirmed_in)
    }
}
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Deposit {
    pub block: u64,
    #[serde(with = "crate::ledger::account_cbor")]
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
    #[serde(with = "crate::ledger::account_cbor")]
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
