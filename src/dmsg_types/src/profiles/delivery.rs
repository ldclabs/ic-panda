//! Paid delivery profile. These types are not required for statement verification.
use crate::{payment::SignedOffer, *};
use candid::{CandidType, Principal};
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
/// Relay quote binding payment, encrypted envelope and delivery limits.
/// Signed under `dmsg/quote/v1`. All amounts are integer ledger base units;
/// all timestamps are Unix milliseconds. `amount` is recipient_net plus
/// service_fee plus fee_reserve.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Quote {
    /// Relay-generated identifier of these fixed quote terms.
    pub quote_id: Hash,
    /// Payment canister authoritative for this escrow/offer.
    pub home_payment: Principal,
    /// ICRC account required as the source of the primary funding deposit.
    #[serde(with = "crate::account::account_cbor")]
    pub payer: Account,
    /// `dmsg/payment-offer/v1` commitment to the recipient authorization.
    pub offer_digest: Hash,
    /// Opaque commitment binding the quote to the offer application scope.
    pub quote_scope: Hash,
    /// ICRC ledger canister for all amounts in this contract.
    pub ledger: Principal,
    /// ICRC account receiving the quoted net payment.
    #[serde(with = "crate::account::account_cbor")]
    pub recipient: Account,
    /// Amount owed to the recipient, excluding fees, in ledger base units.
    pub recipient_net: u128,
    /// ICRC account receiving the service fee.
    #[serde(with = "crate::account::account_cbor")]
    pub platform: Account,
    /// Platform service fee in integer ledger base units.
    pub service_fee: u128,
    /// Ledger base units reserved for outgoing network fees and reserve refund.
    pub fee_reserve: u128,
    /// Total required funding: recipient_net + service_fee + fee_reserve,
    /// in integer ledger base units.
    pub amount: u128,
    /// Maximum network fee allowed by this quote, in ledger base units.
    pub max_network_fee: u128,
    /// Maximum accepted encrypted envelope size in bytes.
    pub max_bytes: u32,
    /// Required storage retention duration in milliseconds.
    pub retain_ms: u64,
    /// Commitment to the exact encrypted delivery envelope.
    pub envelope_digest: Hash,
    /// Relay signing-key epoch used to verify the quote or receipt.
    pub signer_epoch: u64,
    /// Creation time in Unix milliseconds.
    pub created_at: u64,
    /// Fixed funding deadline in Unix milliseconds.
    pub fund_by: u64,
    /// Fixed admission/settlement deadline in Unix milliseconds.
    pub accept_by: u64,
}
/// Request to open an escrow with a signed quote and recipient authorization.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OpenEscrow {
    /// Idempotency identifier; reuse only with identical operation parameters.
    pub op_id: OpId,
    /// Fixed relay quote accepted when the escrow was opened.
    pub quote: Quote,
    /// Relay Ed25519 signature over the quote digest.
    pub quote_signature: ByteBuf,
    /// Recipient authorization signed by a permitted device.
    pub offer: SignedOffer,
}
/// Relay attestation that the quoted encrypted envelope was accepted for storage.
/// Signed under `dmsg/admission-receipt/v1`; does not prove reading, a reply,
/// or completion of the on-chain settlement transfers.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AdmissionReceipt {
    /// Admission receipt protocol version; current version is 1.
    pub protocol: u16,
    /// Identifier of the relay attesting admission.
    pub relay_id: Hash,
    /// Relay signing-key epoch used to verify the quote or receipt.
    pub signer_epoch: u64,
    /// Payment canister authoritative for this escrow/offer.
    pub home_payment: Principal,
    /// 32-byte identifier of the escrow holding funds.
    pub escrow_id: Hash,
    /// `dmsg/quote/v1` commitment to the exact accepted quote.
    pub quote_digest: Hash,
    /// Commitment to the exact encrypted delivery envelope.
    pub envelope_digest: Hash,
    /// Accepted encrypted envelope size in bytes.
    pub size: u32,
    /// Contact-policy version applied at admission.
    pub policy_version: u64,
    /// Inbox encryption-key version used by the admitted envelope.
    pub inbox_key_version: u64,
    /// Relay admission sequence number.
    pub admission_seq: u64,
    /// Time the relay stored the envelope, in Unix milliseconds.
    pub stored_at: u64,
    /// Promised storage retention end in Unix milliseconds.
    pub retain_until: u64,
    /// Fixed funding deadline in Unix milliseconds.
    pub fund_by: u64,
    /// Fixed admission/settlement deadline in Unix milliseconds.
    pub accept_by: u64,
}
/// Admission receipt and the authorized relay Ed25519 signature.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SignedReceipt {
    /// Admission attestation bound to the escrow and quote.
    pub receipt: AdmissionReceipt,
    /// Relay Ed25519 signature over the `dmsg/admission-receipt/v1` digest.
    pub signature: ByteBuf,
}
