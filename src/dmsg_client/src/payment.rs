//! The `dmsg_payment` escrow (implementation design §10, the canister's
//! README): certified payment terms and escrows, funding checks, funds
//! decisions, payouts and refunds. Refund recovery needs only this canister
//! and the payer's identity: no relay has to agree.
//!
//! The relay carries quotes, offers and admission receipts as JSON with
//! hex IDs, text Principals and decimal amounts; [`Wire`] converts them to
//! the canister's types, whose canonical CBOR is what every digest covers.

pub(crate) use crate::certified::certified_leaf;
use crate::codec::decode;
use crate::ic::Ic;
use crate::now_ms;
use candid::Principal;
use dmsg_types::{
    ensure,
    payment::{
        DeliveryFeePolicy, Deposit, EscrowInfo, PaymentConfiguration, PaymentOffer, ReceiptSigner,
        RefundQuote, SignedOffer, TransferLeg,
    },
    profiles::delivery::{AdmissionReceipt, OpenEscrow, Quote, SignedReceipt},
    AccountId, CertifiedBatch, Ed25519Signature, Error, Hash, Result,
};
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// The certified terms of a payment canister for one signer epoch and fee
/// policy version.
#[derive(Clone, Debug)]
pub struct Terms {
    /// Routing and admission configuration.
    pub config: PaymentConfiguration,
    /// The receipt signer.
    pub signer: ReceiptSigner,
    /// The fee policy.
    pub policy: DeliveryFeePolicy,
}

/// `signer/<u64be>` and `fee/<u64be>`.
fn leaf_key(prefix: &str, n: u64) -> Vec<u8> {
    [prefix.as_bytes(), n.to_be_bytes().as_slice()].concat()
}

/// A payment canister read through one agent.
pub struct Payment<'a> {
    /// The agent: the payer's for updates, any for reads.
    pub ic: &'a Ic,
    /// The canister.
    pub canister: Principal,
}

impl Payment<'_> {
    /// Certified terms, the current ones when `epoch` and `fee` are `None`.
    ///
    /// # Errors
    /// Terms not naming `home` as a user home return `Error::IntegrityFailed`.
    pub async fn terms(
        &self,
        home: Principal,
        epoch: Option<u64>,
        fee: Option<u64>,
    ) -> Result<Terms> {
        let batch: Result<CertifiedBatch> = self
            .ic
            .query(self.canister, "get_configuration_certified", (epoch, fee))
            .await?;
        let batch = batch?;
        let root = self.ic.root_key();
        let now = now_ms();
        let leaf = |key: &[u8]| -> Result<Vec<u8>> {
            certified_leaf(&batch, key, self.canister, &root, now)?.ok_or(Error::IntegrityFailed)
        };
        let config: PaymentConfiguration = decode(&leaf(b"configuration")?, 65_536)?;
        let epoch = epoch.unwrap_or(config.signer_epoch);
        let signer: ReceiptSigner = decode(&leaf(&leaf_key("signer/", epoch))?, 65_536)?;
        let fee_key = batch
            .entries
            .iter()
            .find(|e| e.key.starts_with(b"fee/"))
            .map(|e| e.key.to_vec())
            .ok_or(Error::IntegrityFailed)?;
        let policy: DeliveryFeePolicy = decode(&leaf(&fee_key)?, 65_536)?;
        ensure(
            config.schema == 2
                && config.user_homes.contains(&home)
                && signer.epoch == epoch
                && fee.is_none_or(|f| policy.version == f),
            Error::IntegrityFailed,
        )?;
        Ok(Terms {
            config,
            signer,
            policy,
        })
    }

    /// An escrow from its certified leaf.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown escrow.
    pub async fn escrow(&self, id: &Hash) -> Result<EscrowInfo> {
        let batch = self.escrow_proof(id).await?;
        let value = certified_leaf(
            &batch,
            id.as_slice(),
            self.canister,
            &self.ic.root_key(),
            now_ms(),
        )?
        .ok_or(Error::NotFound)?;
        let escrow: EscrowInfo = decode(&value, 262_144)?;
        ensure(escrow.escrow_id == *id, Error::IntegrityFailed)?;
        Ok(escrow)
    }

    /// The certified batch of one escrow, as the relay takes it for proof.
    ///
    /// # Errors
    /// Query failures return `Error::Unavailable`.
    pub async fn escrow_proof(&self, id: &Hash) -> Result<CertifiedBatch> {
        let batch: Result<CertifiedBatch> = self
            .ic
            .query(self.canister, "get_escrow_certified", (vec![*id],))
            .await?;
        batch
    }

    /// The escrow `payer` opened under `op_id`, if any.
    ///
    /// # Errors
    /// Query failures return `Error::Unavailable`.
    pub async fn by_operation(&self, payer: Principal, op_id: &Hash) -> Result<Option<EscrowInfo>> {
        let found: Result<EscrowInfo> = self
            .ic
            .query(self.canister, "get_escrow_by_operation", (payer, op_id))
            .await?;
        match found {
            Ok(escrow) => Ok(Some(escrow)),
            Err(Error::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn update<
        A: candid::utils::ArgumentEncoder,
        R: candid::CandidType + serde::de::DeserializeOwned,
    >(
        &self,
        method: &str,
        args: A,
    ) -> Result<R> {
        let reply: Result<R> = self.ic.update(self.canister, method, args).await?;
        reply
    }

    /// Open an escrow; reconcile a lost reply with [`Self::by_operation`].
    ///
    /// # Errors
    /// The canister's verdict; a lost reply is `Error::ExecutionUnknown`.
    pub async fn open(&self, input: &OpenEscrow) -> Result<EscrowInfo> {
        self.update("open_escrow", (input,)).await
    }

    /// Check a funding deposit at `block`.
    ///
    /// # Errors
    /// The canister's verdict.
    pub async fn check_funding(&self, id: &Hash, block: u64) -> Result<EscrowInfo> {
        self.update("check_funding", (id, block)).await
    }

    /// Submit an admission receipt; anyone may.
    ///
    /// # Errors
    /// The canister's verdict.
    pub async fn finalize(&self, receipt: &SignedReceipt) -> Result<EscrowInfo> {
        self.update("finalize_receipt", (receipt,)).await
    }

    /// Commit a refund after the acceptance deadline; anyone may.
    ///
    /// # Errors
    /// The canister's verdict.
    pub async fn expiry_refund(&self, id: &Hash) -> Result<EscrowInfo> {
        self.update("expiry_refund", (id,)).await
    }

    /// The escrow's transfer legs.
    ///
    /// # Errors
    /// Query failures return `Error::Unavailable`.
    pub async fn transfers(&self, id: &Hash) -> Result<Vec<TransferLeg>> {
        let legs: Result<Vec<TransferLeg>> = self
            .ic
            .query(self.canister, "list_transfers", (id, None::<u64>))
            .await?;
        legs
    }

    /// The escrow's verified deposits.
    ///
    /// # Errors
    /// Query failures return `Error::Unavailable`.
    pub async fn deposits(&self, id: &Hash) -> Result<Vec<Deposit>> {
        let deposits: Result<Vec<Deposit>> = self
            .ic
            .query(self.canister, "list_deposits", (id, None::<u64>))
            .await?;
        deposits
    }

    /// Send one transfer leg; anyone may.
    ///
    /// # Errors
    /// The canister's verdict.
    pub async fn process_transfer(&self, id: &Hash, leg: u64) -> Result<TransferLeg> {
        self.update("process_transfer", (id, leg)).await
    }

    /// Preview a refund of `blocks`, with the released reserve when `reserve`.
    ///
    /// # Errors
    /// The canister's verdict.
    pub async fn quote_refund(
        &self,
        id: &Hash,
        blocks: &[u64],
        reserve: bool,
    ) -> Result<RefundQuote> {
        let quote: Result<RefundQuote> = self
            .ic
            .query(self.canister, "quote_refund", (id, blocks, reserve))
            .await?;
        quote
    }

    /// Refund `blocks`, with the released reserve when `reserve`.
    ///
    /// # Errors
    /// The canister's verdict.
    pub async fn claim_refund(
        &self,
        id: &Hash,
        blocks: &[u64],
        reserve: bool,
    ) -> Result<TransferLeg> {
        self.update("claim_refund", (id, blocks, reserve)).await
    }

    /// Settle a leg whose outcome is unknown with the ledger block it took.
    ///
    /// # Errors
    /// The canister's verdict.
    pub async fn reconcile_transfer(&self, id: &Hash, leg: u64, block: u64) -> Result<TransferLeg> {
        self.update("reconcile_transfer", (id, leg, block)).await
    }

    /// The payer's escrows after `after`.
    ///
    /// # Errors
    /// Query failures return `Error::Unavailable`.
    pub async fn mine(&self, after: Option<Hash>) -> Result<Vec<EscrowInfo>> {
        let list: Result<Vec<EscrowInfo>> = self
            .ic
            .query(self.canister, "list_my_escrows", (after,))
            .await?;
        list
    }

    /// Send the recipient and platform payouts of a settled escrow. Anyone
    /// may retry a leg later, so a failure here is only reported.
    pub async fn payouts(&self, id: &Hash) -> Vec<(u64, Result<TransferLeg>)> {
        use dmsg_types::payment::{LegKind, LegStatus};
        let Ok(legs) = self.transfers(id).await else {
            return vec![];
        };
        let mut sent = vec![];
        for leg in legs {
            if matches!(leg.kind, LegKind::Recipient | LegKind::Platform)
                && !matches!(leg.status, LegStatus::Succeeded | LegStatus::Superseded)
            {
                sent.push((leg.leg_id, self.process_transfer(id, leg.leg_id).await));
            }
        }
        sent
    }
}

/// An ICRC account as the relay writes it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WireAccount {
    /// Principal text.
    pub owner: String,
    /// Subaccount, hex.
    pub subaccount: Option<String>,
}

/// A recipient's payment offer as the relay writes it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WireOffer {
    /// Account ID text.
    pub account_id: String,
    /// Device ID, hex.
    pub device_id: String,
    /// Security epoch.
    pub security_epoch: u64,
    /// Payment canister.
    pub home_payment: String,
    /// Offer ID, hex.
    pub offer_id: String,
    /// Ledger.
    pub ledger: String,
    /// Where the net amount goes.
    pub recipient: WireAccount,
    /// Net amount, decimal.
    pub recipient_net: String,
    /// Quote scope, hex.
    pub quote_scope: String,
    /// Offer version.
    pub version: u64,
    /// Unix milliseconds.
    pub issued_at: u64,
    /// Unix milliseconds.
    pub expires_at: u64,
}

/// A signed offer as the relay writes it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WireSignedOffer {
    /// The offer.
    pub offer: WireOffer,
    /// The device signature, base64url.
    pub signature: String,
}

/// A relay quote as the relay writes it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)]
pub struct WireQuote {
    pub quote_id: String,
    pub home_payment: String,
    pub payer: WireAccount,
    pub offer_digest: String,
    pub quote_scope: String,
    pub ledger: String,
    pub recipient: WireAccount,
    pub recipient_net: String,
    pub platform: WireAccount,
    pub fee_policy_version: u64,
    pub service_fee: String,
    pub fee_reserve: String,
    pub amount: String,
    pub max_network_fee: String,
    pub max_bytes: u32,
    pub retain_ms: u64,
    pub envelope_digest: String,
    pub signer_epoch: u64,
    pub created_at: u64,
    pub fund_by: u64,
    pub accept_by: u64,
}

/// An admission receipt as the relay writes it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)]
pub struct WireReceipt {
    pub protocol: u16,
    pub relay_id: String,
    pub signer_epoch: u64,
    pub home_payment: String,
    pub escrow_id: String,
    pub quote_digest: String,
    pub envelope_digest: String,
    pub size: u32,
    pub policy_version: u64,
    pub inbox_key_version: u64,
    pub admission_seq: u64,
    pub stored_at: u64,
    pub retain_until: u64,
    pub fund_by: u64,
    pub accept_by: u64,
}

/// A signed receipt as the relay writes it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WireSignedReceipt {
    /// The receipt.
    pub receipt: WireReceipt,
    /// The relay signature, base64url.
    pub signature: String,
}

/// Conversions between relay JSON and canister types.
pub struct Wire;

fn principal(text: &str) -> Result<Principal> {
    Principal::from_text(text).map_err(|_| Error::IntegrityFailed)
}

fn hash(text: &str) -> Result<Hash> {
    let bytes: [u8; 32] = hex::decode(text)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or(Error::IntegrityFailed)?;
    Ok(Hash::new(bytes))
}

fn amount(text: &str) -> Result<u128> {
    ensure(
        !text.is_empty()
            && text.len() <= 39
            && text.bytes().all(|b| b.is_ascii_digit())
            && (text == "0" || !text.starts_with('0')),
        Error::IntegrityFailed,
    )?;
    text.parse().map_err(|_| Error::IntegrityFailed)
}

fn signature(text: &str) -> Result<Ed25519Signature> {
    let bytes: [u8; 64] = crate::codec::unb64(text)?
        .try_into()
        .map_err(|_| Error::IntegrityFailed)?;
    Ok(Ed25519Signature::new(bytes))
}

fn account(wire: &WireAccount) -> Result<Account> {
    Ok(Account {
        owner: principal(&wire.owner)?,
        subaccount: wire
            .subaccount
            .as_deref()
            .map(hash)
            .transpose()?
            .map(|h| *h),
    })
}

fn wire_account(account: &Account) -> WireAccount {
    WireAccount {
        owner: account.owner.to_text(),
        subaccount: account.subaccount.map(hex::encode),
    }
}

impl Wire {
    /// A relay offer as the canister's type.
    ///
    /// # Errors
    /// Malformed fields return `Error::IntegrityFailed`.
    pub fn offer(wire: &WireSignedOffer) -> Result<SignedOffer> {
        let o = &wire.offer;
        Ok(SignedOffer {
            offer: PaymentOffer {
                account_id: AccountId::from_str(&o.account_id)
                    .map_err(|_| Error::IntegrityFailed)?,
                device_id: hash(&o.device_id)?,
                security_epoch: o.security_epoch,
                home_payment: principal(&o.home_payment)?,
                offer_id: hash(&o.offer_id)?,
                ledger: principal(&o.ledger)?,
                recipient: account(&o.recipient)?,
                recipient_net: amount(&o.recipient_net)?,
                quote_scope: hash(&o.quote_scope)?,
                version: o.version,
                issued_at: o.issued_at,
                expires_at: o.expires_at,
            },
            signature: signature(&wire.signature)?,
        })
    }

    /// An offer as the relay takes it.
    pub fn wire_offer(offer: &PaymentOffer) -> WireOffer {
        WireOffer {
            account_id: offer.account_id.to_string(),
            device_id: hex::encode(*offer.device_id),
            security_epoch: offer.security_epoch,
            home_payment: offer.home_payment.to_text(),
            offer_id: hex::encode(*offer.offer_id),
            ledger: offer.ledger.to_text(),
            recipient: wire_account(&offer.recipient),
            recipient_net: offer.recipient_net.to_string(),
            quote_scope: hex::encode(*offer.quote_scope),
            version: offer.version,
            issued_at: offer.issued_at,
            expires_at: offer.expires_at,
        }
    }

    /// A relay quote as the canister's type.
    ///
    /// # Errors
    /// Malformed fields return `Error::IntegrityFailed`.
    pub fn quote(q: &WireQuote) -> Result<Quote> {
        Ok(Quote {
            quote_id: hash(&q.quote_id)?,
            home_payment: principal(&q.home_payment)?,
            payer: account(&q.payer)?,
            offer_digest: hash(&q.offer_digest)?,
            quote_scope: hash(&q.quote_scope)?,
            ledger: principal(&q.ledger)?,
            recipient: account(&q.recipient)?,
            recipient_net: amount(&q.recipient_net)?,
            platform: account(&q.platform)?,
            service_fee: amount(&q.service_fee)?,
            fee_policy_version: q.fee_policy_version,
            fee_reserve: amount(&q.fee_reserve)?,
            amount: amount(&q.amount)?,
            max_network_fee: amount(&q.max_network_fee)?,
            max_bytes: q.max_bytes,
            retain_ms: q.retain_ms,
            envelope_digest: hash(&q.envelope_digest)?,
            signer_epoch: q.signer_epoch,
            created_at: q.created_at,
            fund_by: q.fund_by,
            accept_by: q.accept_by,
        })
    }

    /// A relay receipt as the canister's type.
    ///
    /// # Errors
    /// Malformed fields return `Error::IntegrityFailed`.
    pub fn receipt(wire: &WireSignedReceipt) -> Result<SignedReceipt> {
        let r = &wire.receipt;
        Ok(SignedReceipt {
            receipt: AdmissionReceipt {
                protocol: r.protocol,
                relay_id: hash(&r.relay_id)?,
                signer_epoch: r.signer_epoch,
                home_payment: principal(&r.home_payment)?,
                escrow_id: hash(&r.escrow_id)?,
                quote_digest: hash(&r.quote_digest)?,
                envelope_digest: hash(&r.envelope_digest)?,
                size: r.size,
                policy_version: r.policy_version,
                inbox_key_version: r.inbox_key_version,
                admission_seq: r.admission_seq,
                stored_at: r.stored_at,
                retain_until: r.retain_until,
                fund_by: r.fund_by,
                accept_by: r.accept_by,
            },
            signature: signature(&wire.signature)?,
        })
    }
}

/// An escrow for display: amounts as decimal text.
pub fn escrow_view(escrow: &EscrowInfo) -> serde_json::Value {
    use dmsg_types::payment::FundsDecision;
    let q = &escrow.quote;
    serde_json::json!({
        "escrow_id": hex::encode(*escrow.escrow_id),
        "payer": escrow.payer_principal.to_text(),
        "ledger": q.ledger.to_text(),
        "amount": q.amount.to_string(),
        "recipient_net": q.recipient_net.to_string(),
        "service_fee": q.service_fee.to_string(),
        "fee_reserve": q.fee_reserve.to_string(),
        "fund_by": q.fund_by,
        "accept_by": q.accept_by,
        "funded_at": escrow.funded_at,
        "funding_block": escrow.funding_ref,
        "decision": match escrow.decision {
            FundsDecision::Pending => "Pending",
            FundsDecision::SettlementCommitted => "SettlementCommitted",
            FundsDecision::RefundCommitted => "RefundCommitted",
        },
        "confirmed_in": escrow.confirmed_in.to_string(),
        "liabilities": escrow.liabilities.to_string(),
        "transferred": escrow.transferred.to_string(),
        "version": escrow.version,
    })
}
