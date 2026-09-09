//! Paid delivery profile. These types are not required for statement verification.
use crate::{payment::SignedOffer, *};
use candid::{CandidType, Principal};
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Quote {
    pub quote_id: Hash,
    pub home_payment: Principal,
    #[serde(with = "crate::account::account_cbor")]
    pub payer: Account,
    pub offer_digest: Hash,
    pub quote_scope: Hash,
    pub ledger: Principal,
    #[serde(with = "crate::account::account_cbor")]
    pub recipient: Account,
    pub recipient_net: u128,
    #[serde(with = "crate::account::account_cbor")]
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
