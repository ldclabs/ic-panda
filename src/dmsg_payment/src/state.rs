use candid::Principal;
use dmsg_types::{payment::*, profiles::delivery::*, *};
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
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
impl Escrow {
    pub fn info(&self) -> EscrowInfo {
        EscrowInfo {
            escrow_id: self.escrow_id,
            payer_principal: self.payer_principal,
            op_id: self.op_id,
            quote: self.quote.clone(),
            quote_digest: self.quote_digest,
            subaccount: self.subaccount,
            funding_ref: self.funding_ref,
            funded_at: self.funded_at,
            decision: self.decision.clone(),
            receipt_digest: self.receipt_digest,
            version: self.version,
            confirmed_in: self.confirmed_in,
            liabilities: self.liabilities,
            transferred: self.transferred,
            network_fees: self.network_fees,
        }
    }
}
