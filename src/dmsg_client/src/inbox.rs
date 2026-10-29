//! The inbox and paid letters (PRD §9, implementation design §10).
//!
//! A recipient publishes a contact policy at the relay: closed, contacts
//! only, free or paid, with an inbox HPKE key its devices hold. The key is a
//! synced content object, so every device of the account opens letters.
//! A paid policy carries a payment offer one `PaymentOffer` device signed.
//!
//! A sender seals a letter to the recipient's key and submits it. A free
//! letter is accepted at once; a paid one gets a relay quote. The payer then
//! opens an escrow in `dmsg_payment`, funds it, and asks the relay to admit
//! the letter with the funding proof; the admission receipt settles the
//! escrow, and the relay publishes the letter once the canister decided.
//! Every step is journaled: a lost reply is reconciled by its order or
//! operation ID and never pays or admits twice. After the acceptance
//! deadline the payer recovers the funds from the canister alone.

mod operations;
pub use operations::{configure, escrow, mark, order, refund, request, revise, send};

use crate::client::Client;
use crate::cloud::{Evidence, sign_command};
use crate::codec::{MAX_RECORD, b64, canonical, decode, digest, sha256, unb64};
use crate::crypto::{HpkeEnvelope, hpke_open, hpke_public, hpke_seal, random_key, sign};
use crate::evidence::Trust;
use crate::hold::Hold;
use crate::now_ms;
use crate::payer::{memo, transfer_call};
use crate::payment::{
    Payment, Terms, Wire, WireAccount, WireQuote, WireSignedOffer, WireSignedReceipt, escrow_view,
};
use crate::upload::random_hex;
use candid::Principal;
use dmsg_types::{
    AccountId, Ed25519Signature, Error, Hash, Result, ensure, ensure_valid,
    payment::{EscrowInfo, FundsDecision, LegStatus, PaymentOffer, SignedOffer},
    profiles::delivery::OpenEscrow,
    user::Capability,
};
use icrc_ledger_types::icrc1::{account::Account, transfer::TransferArg};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use zeroize::Zeroizing;

/// Content kind of an inbox key.
pub const INBOX_KEY: &str = "inbox";
/// Largest letter text in bytes.
pub const MAX_LETTER: usize = 6000;
const JOURNAL: &str = "inbox-job:";
const LETTER: &str = "inbox-letter:";
const CURSOR: &str = "inbox-cursor";

const POLICY: &str = "dmsg/inbox/policy/v1";
const CONTACT: &str = "dmsg/inbox/contact/v1";

/// An inbox key as the account syncs it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct InboxKey {
    pub(crate) format: String,
    pub(crate) version: u64,
    #[serde(rename = "rootGeneration")]
    pub(crate) root_generation: u64,
    pub(crate) seed: String,
}

/// The object ID of an inbox key version, hex.
pub fn key_object(account: &str, version: u64) -> String {
    hex::encode(*sha256(&canonical(&("dmsg/inbox-key/1", account, version))))
}

/// The HPKE context of a letter.
pub fn letter_context<'a>(
    sender: &'a str,
    recipient: &'a str,
    order: &'a str,
    version: u64,
) -> (&'static str, &'a str, &'a str, &'a str, u64) {
    ("dmsg/inbox-envelope/1", sender, recipient, order, version)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LetterBody {
    text: String,
}

/// Who may write without paying.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContactInvitation {
    /// Invitation ID, hex.
    pub id: String,
    /// The invited account.
    pub account: String,
    /// Unix milliseconds.
    pub expires_at: u64,
}

/// A contact policy to publish.
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PolicyInput {
    /// `closed`, `contacts`, `free` or `paid`.
    pub mode: String,
    /// Accounts that write without paying.
    #[serde(default)]
    pub allow: Vec<String>,
    /// Blocked accounts; the current list when absent.
    #[serde(default)]
    pub block: Option<Vec<String>>,
    /// Single-use contact invitations; the current ones when absent.
    #[serde(default)]
    pub invitations: Option<Vec<ContactInvitation>>,
    /// Pending letters at once; the current number, else 100, when absent.
    #[serde(default)]
    pub capacity: Option<u64>,
    /// The net amount a paid letter pays, in ledger units, decimal.
    #[serde(default)]
    pub price: Option<String>,
    /// Where paid letters' net amount goes; the payer when absent.
    #[serde(default)]
    pub payee: Option<String>,
}

/// `dmsg/inbox/policy/v1`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct PolicyPayload {
    expected_version: u64,
    prev_hash: Option<String>,
    mode: String,
    block: Vec<String>,
    allow: Vec<String>,
    invitations: Vec<ContactInvitation>,
    capacity: u64,
    inbox_key_version: u64,
    inbox_hpke_pub: String,
    root_generation: u64,
    key_descriptor: CoseRef,
    offer: Option<WireSignedOffer>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct CoseRef {
    cose_sign1: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyDescriptor {
    inbox_key_version: u64,
    inbox_hpke_pub: String,
    root_generation: u64,
}

/// `dmsg/inbox/contact/v1`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContactPayload {
    /// The recipient.
    pub recipient: String,
    /// Order ID, hex.
    pub order_id: String,
    /// The recipient's key version.
    pub inbox_key_version: u64,
    /// The sealed letter, base64url.
    pub ciphertext: String,
    /// A contact invitation.
    pub invitation: Option<String>,
    /// The ledger account that pays.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payer: Option<WireAccount>,
}

/// A signed relay command in flight.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Operation {
    action: String,
    path: String,
    payload: Json,
    request_id: String,
    cose: String,
    deadline: u64,
    result: Option<Json>,
}

/// A letter this account sent.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Outgoing {
    recipient: String,
    text_digest: String,
    payload: ContactPayload,
}

/// An escrow being opened.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct EscrowJob {
    input: OpenEscrow,
    escrow: Option<Hash>,
}

/// A letter as the recipient reads it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Letter {
    /// Order ID.
    pub order_id: String,
    /// The sender.
    pub sender: String,
    /// `visible` or `aborted`.
    pub state: String,
    /// The text; empty for an aborted order.
    pub text: String,
    /// Unix milliseconds.
    pub created_at: u64,
    /// Read.
    pub read: bool,
    /// Archived.
    pub archived: bool,
    /// The cose of the letter, to recognize a cached one.
    pub signed: Option<String>,
    /// Why an order was aborted.
    pub abort_reason: Option<String>,
}

/// Most pages of letters [`list`] reads before it gives up.
pub const INBOX_PAGES: usize = 10_000;

impl Client {
    /// The own user home's configuration.
    pub(crate) async fn home_config(&mut self) -> Result<dmsg_types::user::UserInit> {
        let home = self.account()?.home;
        self.reader().await?.query(home, "user_config", ()).await
    }

    fn inbox_job<T: serde::de::DeserializeOwned + Serialize>(
        &self,
        key: &str,
    ) -> Result<Option<T>> {
        self.workspace
            .private(self.keys()?, &format!("{JOURNAL}{key}"))
    }

    fn save_inbox_job<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{JOURNAL}{key}"), value)
    }

    /// Send a relay command once, reconciled by its request ID; a command
    /// that can no longer commit is signed again under the same ID.
    async fn inbox_post(
        &mut self,
        key: &str,
        path: &str,
        action: &str,
        payload: Json,
        repeat: bool,
    ) -> Result<Json> {
        operations::post(&self.hold(), key, path, action, payload, repeat).await
    }

    /// The inbox key of `version`, created on first use under the current
    /// root; returns its public key.
    fn inbox_key(&mut self, version: u64) -> Result<[u8; 32]> {
        let account = self.account()?.id.to_string();
        let generation = self.roots()?.generation;
        let id = key_object(&account, version);
        let key = match self.current(&id) {
            Ok((_, record)) => {
                let key: InboxKey =
                    decode(&record.open(self.environment(), self.roots()?)?, MAX_RECORD)?;
                key
            }
            Err(Error::NotFound) => {
                let key = InboxKey {
                    format: "dmsg-inbox-key/1".into(),
                    version,
                    root_generation: generation,
                    seed: b64(&Zeroizing::new(random_key())[..]),
                };
                let payload = Zeroizing::new(canonical(&key));
                self.write(INBOX_KEY, id, None, false, &payload, None)?;
                key
            }
            Err(error) => return Err(error),
        };
        ensure(
            key.format == "dmsg-inbox-key/1" && key.version == version,
            Error::IntegrityFailed,
        )?;
        ensure(key.root_generation == generation, Error::RekeyRequired)?;
        let seed: [u8; 32] = unb64(&key.seed)?
            .try_into()
            .map_err(|_| Error::IntegrityFailed)?;
        Ok(hpke_public(&Zeroizing::new(seed)))
    }

    /// The inbox seeds of `version`: the synced key and any conflict copy.
    fn inbox_seeds(&self, version: u64) -> Result<Vec<Zeroizing<[u8; 32]>>> {
        let account = self.account()?.id.to_string();
        let id = key_object(&account, version);
        let mut records = vec![self.current(&id)?.1];
        for copy in self.workspace.store.conflicts(INBOX_KEY)? {
            if copy.item == id {
                records.push(crate::content::record(&copy)?);
            }
        }
        records
            .iter()
            .map(|record| {
                let key: InboxKey =
                    decode(&record.open(self.environment(), self.roots()?)?, MAX_RECORD)?;
                let seed: [u8; 32] = unb64(&key.seed)?
                    .try_into()
                    .map_err(|_| Error::IntegrityFailed)?;
                Ok(Zeroizing::new(seed))
            })
            .collect()
    }

    /// The payment canister of this account's home and its certified terms.
    async fn payment_terms(
        &mut self,
        epoch: Option<u64>,
        fee: Option<u64>,
    ) -> Result<(Principal, Terms)> {
        let canister = self.home_config().await?.payment_canister;
        let home = self.account()?.home;
        let ic = self.reader().await?;
        let terms = Payment { ic: &ic, canister }
            .terms(home, epoch, fee)
            .await?;
        Ok((canister, terms))
    }

    /// This account's contact policy at the relay.
    ///
    /// # Errors
    /// Relay errors as returned.
    pub async fn inbox_policy(&mut self) -> Result<Option<Json>> {
        policy(&self.hold()).await
    }

    /// Publish a contact policy with a fresh inbox key; a paid policy also
    /// carries a payment offer this device signs, which needs `PaymentOffer`.
    /// An identical policy already in force is returned as it is.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a paid policy on a device without
    /// `PaymentOffer` and `Error::VersionConflict` when another device
    /// changed the policy meanwhile.
    pub async fn inbox_configure(&mut self, input: &PolicyInput) -> Result<Json> {
        configure(&self.hold(), input).await
    }

    /// Write to `recipient`: a free letter is delivered at once, a paid one
    /// returns the relay's quote for [`Self::inbox_pay`]. Repeating an order
    /// returns where it stands.
    ///
    /// # Errors
    /// Returns `Error::IdempotencyConflict` when an order ID is reused for
    /// another letter.
    pub async fn inbox_send(
        &mut self,
        recipient: &AccountId,
        text: &str,
        order: Option<&str>,
        invitation: Option<&str>,
        pay: bool,
    ) -> Result<Json> {
        send(&self.hold(), recipient, text, order, invitation, pay).await
    }

    /// An order as the relay holds it; the sender's view is checked
    /// against the letter it sent.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown order.
    pub async fn inbox_order(&mut self, recipient: &AccountId, order: &str) -> Result<Json> {
        self::order(&self.hold(), recipient, order).await
    }

    /// Letters this account sent, by order.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn inbox_outgoing(&self) -> Result<Vec<Json>> {
        self.keys()?;
        let prefix = format!("{JOURNAL}outgoing:");
        let mut list = vec![];
        for key in self.workspace.store.sealed_keys(&prefix)? {
            let outgoing: Outgoing = self
                .workspace
                .private(self.keys()?, &key)?
                .ok_or(Error::IntegrityFailed)?;
            list.push(json!({
                "order_id": &key[prefix.len()..],
                "recipient": outgoing.recipient,
                "paid": outgoing.payload.payer.is_some(),
            }));
        }
        Ok(list)
    }

    /// The commitment to a quoted letter's terms a person approves.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an order without a quote.
    pub(crate) async fn letter_digest(
        &mut self,
        recipient: &AccountId,
        order: &str,
    ) -> Result<(Hash, WireQuote)> {
        let value = self.inbox_order(recipient, order).await?;
        ensure(!value["quote"].is_null(), Error::NotFound)?;
        let quote: WireQuote =
            serde_json::from_value(value["quote"].clone()).map_err(|_| Error::IntegrityFailed)?;
        let digest = digest(
            "dmsg/letter-approval/v1",
            &(recipient.to_string(), order, &quote),
        );
        Ok((digest, quote))
    }

    /// Pay for a quoted letter: open the escrow, fund it from the payer and
    /// have the relay admit the letter; the admission receipt then settles
    /// the escrow. Each step resumes where a lost reply left it.
    ///
    /// # Errors
    /// Returns `Error::Expired` past the funding deadline and
    /// `Error::ExecutionUnknown` while a step's outcome is unknown.
    pub(crate) async fn inbox_pay(&mut self, recipient: &AccountId, order: &str) -> Result<Json> {
        let value = self.inbox_order(recipient, order).await?;
        if matches!(value["state"].as_str(), Some("visible" | "aborted"))
            && value["escrow"].is_null()
        {
            return Ok(json!({"order": value}));
        }
        let escrow = self.inbox_open_escrow(&value).await?;
        let escrow = self.inbox_fund(escrow).await?;
        self.inbox_admit(recipient, &value, escrow).await
    }

    async fn inbox_open_escrow(&mut self, order: &Json) -> Result<EscrowInfo> {
        let order_id = order["order_id"]
            .as_str()
            .ok_or(Error::IntegrityFailed)?
            .to_string();
        let local: Outgoing = self
            .inbox_job(&format!("outgoing:{order_id}"))?
            .ok_or(Error::NotFound)?;
        let wire: WireQuote =
            serde_json::from_value(order["quote"].clone()).map_err(|_| Error::IntegrityFailed)?;
        let quote = Wire::quote(&wire)?;
        let offer: SignedOffer = Wire::offer(
            &serde_json::from_value(order["offer"].clone()).map_err(|_| Error::IntegrityFailed)?,
        )?;
        let signature: Ed25519Signature = Ed25519Signature::new(
            unb64(order["quote_signature"].as_str().unwrap_or_default())?
                .try_into()
                .map_err(|_| Error::IntegrityFailed)?,
        );
        let (canister, terms) = self
            .payment_terms(Some(quote.signer_epoch), Some(quote.fee_policy_version))
            .await?;
        let (payer, ic) = self.payer().await?;
        let service =
            dmsg_protocol::billing::delivery_service_fee(quote.recipient_net, &terms.policy)?;
        let ciphertext = unb64(order["ciphertext"].as_str().unwrap_or_default())?;
        ensure(
            order["recipient"] == local.recipient.as_str()
                && order["sender"] == self.account()?.id.to_string().as_str()
                && order["ciphertext"] == local.payload.ciphertext.as_str()
                && local
                    .payload
                    .payer
                    .as_ref()
                    .is_some_and(|p| *p == wire.payer)
                && quote.payer.owner == payer
                && quote.home_payment == canister
                && quote.envelope_digest == sha256(&ciphertext)
                && terms.signer.valid_from <= quote.created_at
                && quote.accept_by < terms.signer.valid_until
                && Some(quote.amount)
                    == quote
                        .recipient_net
                        .checked_add(quote.service_fee)
                        .and_then(|a| a.checked_add(quote.fee_reserve))
                && quote.max_network_fee == terms.config.max_fee
                && quote.fee_reserve
                    >= quote.max_network_fee * if quote.service_fee > 0 { 2 } else { 1 }
                && quote.fee_reserve <= quote.max_network_fee * 3
                && quote.service_fee == service
                && quote.ledger == terms.config.ledger
                && offer.offer.account_id.to_string() == local.recipient
                && quote.home_payment == offer.offer.home_payment
                && quote.ledger == offer.offer.ledger
                && quote.recipient == offer.offer.recipient
                && quote.recipient_net == offer.offer.recipient_net
                && quote.quote_scope == offer.offer.quote_scope
                && digest("dmsg/payment-offer/v1", &offer.offer) == quote.offer_digest
                && dmsg_protocol::verify(
                    &terms.signer.public_key,
                    digest("dmsg/quote/v2", &quote).as_slice(),
                    signature.as_slice(),
                )
                .is_ok(),
            Error::IntegrityFailed,
        )?;
        let op_id = crate::upload::hex_id(&order_id)?;
        let input = OpenEscrow {
            op_id,
            quote,
            quote_signature: signature,
            offer,
        };
        let key = format!("escrow:{order_id}");
        let mut job = match self.inbox_job::<EscrowJob>(&key)? {
            Some(job) => {
                ensure(job.input == input, Error::IdempotencyConflict)?;
                job
            }
            None => {
                let job = EscrowJob {
                    input,
                    escrow: None,
                };
                self.save_inbox_job(&key, &job)?;
                job
            }
        };
        let payment = Payment { ic: &ic, canister };
        if let Some(found) = payment.by_operation(payer, &op_id).await? {
            let escrow = payment.escrow(&found.escrow_id).await?;
            ensure(
                escrow.escrow_id == digest("dmsg/escrow-id/v1", &(canister, payer, op_id))
                    && escrow.quote == job.input.quote,
                Error::IntegrityFailed,
            )?;
            return Ok(escrow);
        }
        // Only a new escrow must satisfy the mutable current policy. An
        // existing escrow keeps the terms already accepted for this operation,
        // even while new orders are disabled or a signer is revoked.
        let (current_canister, current) = self
            .payment_terms(
                Some(job.input.quote.signer_epoch),
                Some(job.input.quote.fee_policy_version),
            )
            .await?;
        ensure(current_canister == canister, Error::IntegrityFailed)?;
        ensure(current.config.enabled, Error::Locked)?;
        ensure(!current.signer.revoked, Error::Forbidden)?;
        ensure(
            job.input.quote.platform == current.config.platform,
            Error::IntegrityFailed,
        )?;
        let opened = payment.open(&job.input).await?;
        job.escrow = Some(opened.escrow_id);
        self.save_inbox_job(&key, &job)?;
        payment.escrow(&opened.escrow_id).await
    }

    /// Fund an escrow from the payer, once.
    async fn inbox_fund(&mut self, escrow: EscrowInfo) -> Result<EscrowInfo> {
        if escrow.funded_at.is_some() || escrow.decision != FundsDecision::Pending {
            return Ok(escrow);
        }
        let quote = escrow.quote.clone();
        let (_, terms) = self
            .payment_terms(Some(quote.signer_epoch), Some(quote.fee_policy_version))
            .await?;
        let fee = terms.config.ledger_fee;
        ensure(fee <= quote.max_network_fee, Error::FeeBlocked)?;
        let id = escrow.escrow_id;
        let create = || {
            ensure(quote.fund_by > now_ms(), Error::Expired)?;
            transfer_call(&TransferArg {
                from_subaccount: quote.payer.subaccount,
                to: Account {
                    owner: quote.home_payment,
                    subaccount: Some(*escrow.subaccount),
                },
                fee: Some(fee.into()),
                created_at_time: Some(now_ms() * 1_000_000),
                memo: Some(memo(digest("dmsg/delivery/funding/v1", &id).as_slice())),
                amount: quote.amount.into(),
            })
        };
        let block = self
            .pay(
                &format!("delivery-funding:{}", hex::encode(*id)),
                quote.ledger,
                fee,
                create,
            )
            .await?;
        let (_, ic) = self.payer().await?;
        let payment = Payment {
            ic: &ic,
            canister: quote.home_payment,
        };
        payment.check_funding(&id, block).await
    }

    /// Have the relay admit a funded letter, then settle the escrow with
    /// the admission receipt and let the relay publish it.
    async fn inbox_admit(
        &mut self,
        recipient: &AccountId,
        order: &Json,
        escrow: EscrowInfo,
    ) -> Result<Json> {
        let order_id = order["order_id"]
            .as_str()
            .ok_or(Error::IntegrityFailed)?
            .to_string();
        let (_, ic) = self.payer().await?;
        let canister = escrow.quote.home_payment;
        let payment = Payment { ic: &ic, canister };
        let proof = self.escrow_proof(&payment, &escrow.escrow_id).await?;
        // An admission is unique per order: read a committed receipt before
        // refreshing a time-limited funds proof after a lost reply.
        let mut result = self.inbox_order(recipient, &order_id).await?;
        if result["receipt"].is_null() {
            result = self
                .inbox_post(
                    &format!(
                        "admit:{order_id}:{}",
                        hex::encode(*sha256(&canonical(&proof)))
                    ),
                    &format!("/v1/inboxes/{recipient}/admit"),
                    "dmsg/inbox/admit/v1",
                    json!({"order_id": order_id, "proof": proof}),
                    false,
                )
                .await?;
        }
        ensure(!result["receipt"].is_null(), Error::ExecutionUnknown)?;
        let wire: WireSignedReceipt = serde_json::from_value(result["receipt"].clone())
            .map_err(|_| Error::IntegrityFailed)?;
        let receipt = Wire::receipt(&wire)?;
        let (_, terms) = self
            .payment_terms(
                Some(receipt.receipt.signer_epoch),
                Some(escrow.quote.fee_policy_version),
            )
            .await?;
        ensure(
            dmsg_protocol::verify(
                &terms.signer.public_key,
                digest("dmsg/admission-receipt/v2", &receipt.receipt).as_slice(),
                receipt.signature.as_slice(),
            )
            .is_ok()
                && receipt.receipt.quote_digest == digest("dmsg/quote/v2", &escrow.quote)
                && order["envelope_digest"]
                    == hex::encode(*receipt.receipt.envelope_digest).as_str(),
            Error::IntegrityFailed,
        )?;
        let finalized = match payment.finalize(&receipt).await {
            Ok(escrow) => escrow,
            // Already decided: read the decision.
            Err(Error::VersionConflict) => payment.escrow(&escrow.escrow_id).await?,
            Err(error) => return Err(error),
        };
        let proof = self.escrow_proof(&payment, &finalized.escrow_id).await?;
        let state = self.cloud_state().await?;
        let published = self
            .cloud
            .post_raw(
                &self.signer(&state)?,
                &format!("/v1/inboxes/{recipient}/orders/{order_id}/reconcile"),
                &proof,
                now_ms(),
            )
            .await?;
        let _ = payment.payouts(&finalized.escrow_id).await;
        let escrow = payment.escrow(&finalized.escrow_id).await?;
        Ok(json!({"order": published, "escrow": escrow_view(&escrow)}))
    }

    /// The relay's proof form of an escrow's certified leaf.
    async fn escrow_proof(&self, payment: &Payment<'_>, id: &Hash) -> Result<Json> {
        let batch = payment.escrow_proof(id).await?;
        let value = crate::payment::certified_leaf(
            &batch,
            id.as_slice(),
            payment.canister,
            &payment.ic.root_key(),
            now_ms(),
        )?
        .ok_or(Error::NotFound)?;
        let entry = batch
            .entries
            .iter()
            .find(|e| e.key.as_ref() == id.as_slice())
            .ok_or(Error::IntegrityFailed)?;
        Ok(json!({
            "canister": payment.canister.to_text(),
            "certificate": b64(&batch.certificate),
            "escrow_id": hex::encode(**id),
            "value": b64(&value),
            "witness": b64(&entry.witness),
        }))
    }

    /// The escrow of an order this account paid, from the canister.
    ///
    /// # Errors
    /// Returns `Error::NotFound` before an escrow was opened.
    pub async fn inbox_escrow(&mut self, order: &str) -> Result<Json> {
        escrow(&self.hold(), order).await
    }

    /// An escrow's transfer legs: payouts, refunds and their revisions.
    ///
    /// # Errors
    /// Query failures return `Error::Unavailable`.
    pub async fn payment_transfers(&mut self, escrow: &Hash) -> Result<Json> {
        let ic = self.reader().await?;
        let canister = self.home_config().await?.payment_canister;
        let payment = Payment { ic: &ic, canister };
        let legs = payment.transfers(escrow).await?;
        Ok(json!(legs.iter().map(leg_view).collect::<Vec<_>>()))
    }

    /// Send a refused leg of an escrow the payer paid or receives from
    /// again, at the fee the ledger asked for (`FeeBlocked`) or unchanged
    /// (`Rejected`); the escrow's approved fee ceiling still holds.
    ///
    /// # Errors
    /// Returns `Error::VersionConflict` for a leg that was not refused.
    pub async fn payment_revise(&mut self, escrow: &Hash, leg: u64) -> Result<Json> {
        revise(&self.hold(), escrow, leg).await
    }

    /// Recover what an order's escrow holds for the payer, in one refund
    /// it then sends: past the acceptance deadline the whole payment
    /// (committing the refund if nobody did), after settlement the unused
    /// fee reserve once every payout succeeded, and any deposit the escrow
    /// did not take (paid twice, too little or too late). Needs no relay.
    ///
    /// # Errors
    /// Returns `Error::Pending` while nothing can be claimed yet: before
    /// the deadline, or while payouts are under way.
    pub async fn inbox_refund(&mut self, order: &str) -> Result<Json> {
        refund(&self.hold(), order).await
    }

    /// Verify and commit a page and its cursor together. An empty page can
    /// still advance past orders that are not visible to the recipient.
    fn inbox_page_settle(&mut self, after: u64, page: &Json, trust: &Trust) -> Result<Option<u64>> {
        let me = self.account()?.id.to_string();
        let start = after;
        let mut after = after;
        let now = now_ms();
        ensure(
            self.workspace.store.meta::<u64>(CURSOR)?.unwrap_or(0) == start,
            Error::VersionConflict,
        )?;
        let entries = page["entries"].as_array().ok_or(Error::IntegrityFailed)?;
        let next = page["next_cursor"].as_u64().ok_or(Error::IntegrityFailed)?;
        ensure(next >= after, Error::IntegrityFailed)?;
        let mut changed = vec![];
        for row in entries {
            let cursor = row["cursor"].as_u64().ok_or(Error::IntegrityFailed)?;
            ensure(cursor > after && cursor <= next, Error::IntegrityFailed)?;
            let value = &row["value"];
            let order = value["order_id"]
                .as_str()
                .ok_or(Error::IntegrityFailed)?
                .to_string();
            ensure(
                crate::channel_log::is_hash(&order)
                    && value["recipient"] == me.as_str()
                    && matches!(value["state"].as_str(), Some("visible" | "aborted")),
                Error::Forbidden,
            )?;
            let cached: Option<Letter> = self
                .workspace
                .private(self.keys()?, &format!("{LETTER}{order}"))?;
            let letter = match value["state"].as_str() {
                // Relay management metadata, never an authenticated letter.
                Some("aborted") => {
                    ensure(value.get("ciphertext").is_none(), Error::IntegrityFailed)?;
                    Letter {
                        order_id: order.clone(),
                        sender: value["sender"].as_str().unwrap_or_default().into(),
                        state: "aborted".into(),
                        text: String::new(),
                        created_at: value["created_at"].as_u64().unwrap_or_default(),
                        read: value["read"] == true,
                        archived: value["archived"] == true,
                        signed: None,
                        abort_reason: value["abort_reason"].as_str().map(str::to_string),
                    }
                }
                _ => {
                    let cose = value["signed"]["cose_sign1"]
                        .as_str()
                        .ok_or(Error::IntegrityFailed)?
                        .to_string();
                    let text = match cached
                        .as_ref()
                        .filter(|l| l.signed.as_deref() == Some(&cose))
                    {
                        Some(letter) => letter.text.clone(),
                        None => {
                            let evidence: Evidence =
                                serde_json::from_value(value["evidence"].clone())
                                    .map_err(|_| Error::IntegrityFailed)?;
                            let created =
                                value["created_at"].as_u64().ok_or(Error::IntegrityFailed)?;
                            let verified: crate::evidence::Verified<ContactPayload> =
                                trust.command(&cose, &evidence, created, Some(CONTACT), now)?;
                            let payload = &verified.body.payload;
                            ensure(
                                value["sender"] == verified.account.to_string().as_str()
                                    && payload.order_id == order
                                    && payload.recipient == me
                                    && value["ciphertext"] == payload.ciphertext.as_str(),
                                Error::IntegrityFailed,
                            )?;
                            let envelope: HpkeEnvelope =
                                decode(&unb64(&payload.ciphertext)?, 16_384)?;
                            let sender = verified.account.to_string();
                            let context =
                                letter_context(&sender, &me, &order, payload.inbox_key_version);
                            let mut opened = None;
                            for seed in self.inbox_seeds(payload.inbox_key_version)? {
                                if let Ok(plain) = hpke_open(&seed, &envelope, &context) {
                                    opened = Some(plain);
                                    break;
                                }
                            }
                            let plain = opened.ok_or(Error::IntegrityFailed)?;
                            let body: LetterBody = decode(&plain, 16_384)?;
                            body.text
                        }
                    };
                    Letter {
                        order_id: order.clone(),
                        sender: value["sender"].as_str().unwrap_or_default().into(),
                        state: "visible".into(),
                        text,
                        created_at: value["created_at"].as_u64().unwrap_or_default(),
                        read: value["read"] == true,
                        archived: value["archived"] == true,
                        signed: Some(cose),
                        abort_reason: None,
                    }
                }
            };
            if cached.as_ref() != Some(&letter) {
                changed.push((letter.clone(), cursor));
            }
            after = cursor;
        }
        // Only what changed is written, in one go.
        let keys = self.keys()?;
        self.workspace.store.atomic(|_| {
            for (letter, cursor) in &changed {
                let name = format!("{LETTER}{}", letter.order_id);
                self.workspace.put_private(keys, &name, letter)?;
                self.project_inbox_notification(
                    letter,
                    *cursor,
                    now,
                    !self
                        .workspace
                        .store
                        .meta::<bool>("notification:inbox_initialized")?
                        .unwrap_or(false),
                )?;
            }
            self.workspace.store.set_meta(CURSOR, &next)
        })?;
        if next == start {
            return Ok(None);
        }
        Ok(Some(next))
    }

    pub(crate) fn project_inbox_notification(
        &self,
        letter: &Letter,
        cursor: u64,
        now: u64,
        baseline: bool,
    ) -> Result<()> {
        use crate::notifications::{self, Item, Target};
        let account = self.account()?.id.to_string();
        let target = Target::Inbox(Some(letter.order_id.clone()));
        let old = self
            .workspace
            .store
            .notification_item(&notifications::item_id(&account, &target))?;
        // Inbox management changes do not create new letters. Preserve the first
        // accepted cursor even if a later page reports read/archive metadata.
        let mut item = Item::new(
            account.clone(),
            target,
            old.as_ref().map_or(cursor.max(1), |item| item.version),
            now,
        );
        item.group = notifications::item_id(&account, &Target::Inbox(None));
        item.open = letter.state == "visible"
            && !letter.read
            && !letter.archived
            && letter.sender != account;
        item.unread = Some(u64::from(item.open));
        item.quiet = baseline && old.is_none();
        notifications::upsert(&self.workspace.store, item)?;
        Ok(())
    }

    /// Previously verified letters on this device, available offline.
    /// # Errors
    /// Returns `Error::Locked` while locked and errors for corrupt local data.
    pub fn inbox_cached(&self) -> Result<Vec<Letter>> {
        let keys = self.keys()?;
        self.workspace
            .store
            .sealed_keys(LETTER)?
            .iter()
            .map(|name| {
                self.workspace
                    .private(keys, name)?
                    .ok_or(Error::IntegrityFailed)
            })
            .collect()
    }

    /// The last fetched contact policy, sealed locally.
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn inbox_cached_policy(&self) -> Result<Option<Json>> {
        Ok(self
            .cache_read(crate::cache::Kind::Inbox, "")?
            .and_then(|s| (!s.data.is_null()).then_some(s.data)))
    }

    /// Mark a letter read or archived; archiving reopens contact for its
    /// sender.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown order.
    pub async fn inbox_mark(&mut self, order: &str, read: bool, archived: bool) -> Result<Json> {
        mark(&self.hold(), order, read, archived).await
    }
}

/// This account's contact policy at the relay.
///
/// # Errors
/// Relay errors as returned.
pub async fn policy<H: Hold>(h: &H) -> Result<Option<Json>> {
    let h = operations::Unlocked::new(h).await?;
    let (me, scope) = h
        .with(|c| Ok((c.account()?.id, c.cache_scope(crate::cache::Kind::Inbox)?)))
        .await?;
    let policy = h.find(&format!("/v1/inboxes/{me}/policy")).await?;
    h.with(|c| {
        c.cache_save_checked(
            &scope,
            crate::cache::Kind::Inbox,
            "",
            &serde_json::to_value(&policy).map_err(|_| Error::IntegrityFailed)?,
            crate::now_ms(),
        )
    })
    .await?;
    Ok(policy)
}

/// Refresh letters from the last committed cursor, then read the local list.
/// A page and its cursor commit only after every visible letter is verified.
/// Each page waits on the relay without holding the client.
///
/// # Errors
/// Returns `Error::RecoveryIncomplete` when an inbox key is missing; a failed
/// page is retried on the next refresh without skipping any of its letters.
pub async fn list<H: Hold>(h: &H) -> Result<Vec<Letter>> {
    let h = operations::Unlocked::new(h).await?;
    let report = sync(&h, INBOX_PAGES).await?;
    ensure(!report.more, Error::QuotaExceeded)?;
    h.with(|c| c.inbox_cached()).await
}

/// One bounded background pass; each page retains the existing atomic verifier.
#[derive(Clone, Debug, Serialize)]
pub struct SyncReport {
    /// Last committed cursor, lossless across JSON.
    #[serde(with = "crate::notifications::decimal")]
    pub cursor: u64,
    /// Verified rows committed in this pass, deduplicated by order ID.
    pub changed: Vec<String>,
    /// The pass reached its page budget and should yield before continuing.
    pub more: bool,
}

/// Synchronize at most `max_pages` without a client lock across network waits.
/// Errors leave the failed page and its cursor uncommitted.
pub async fn sync<H: Hold>(h: &H, max_pages: usize) -> Result<SyncReport> {
    ensure(
        max_pages > 0 && max_pages <= INBOX_PAGES,
        Error::QuotaExceeded,
    )?;
    let h = operations::Unlocked::new(h).await?;
    let (me, mut after) = h
        .with(|c| {
            Ok((
                c.account()?.id,
                c.workspace.store.meta::<u64>(CURSOR)?.unwrap_or(0),
            ))
        })
        .await?;
    let trust = h.trust().await?;
    let mut changed = std::collections::BTreeSet::new();
    for _ in 0..max_pages {
        let page = h
            .get(&format!("/v1/inboxes/{me}?after={after}&limit=20"))
            .await?;
        match h
            .with(|c| c.inbox_page_settle(after, &page, &trust))
            .await?
        {
            Some(next) => {
                after = next;
                for row in page["entries"].as_array().ok_or(Error::IntegrityFailed)? {
                    if let Some(id) = row["value"]["order_id"].as_str() {
                        changed.insert(id.to_string());
                    }
                }
            }
            None => {
                h.with(|c| {
                    c.workspace
                        .store
                        .set_meta("notification:inbox_initialized", &true)
                })
                .await?;
                return Ok(SyncReport {
                    cursor: after,
                    changed: changed.into_iter().collect(),
                    more: false,
                });
            }
        }
    }
    Ok(SyncReport {
        cursor: after,
        changed: changed.into_iter().collect(),
        more: true,
    })
}

#[cfg(test)]
#[path = "inbox_tests.rs"]
mod tests;

/// A transfer leg for display.
pub(crate) fn leg_view(t: &dmsg_types::payment::TransferLeg) -> Json {
    json!({
        "escrow_id": hex::encode(*t.escrow_id),
        "leg_id": t.leg_id,
        "kind": format!("{:?}", t.kind),
        "to": t.to.owner.to_text(),
        "amount": t.amount.to_string(),
        "fee": t.fee.to_string(),
        "expected_fee": t.expected_fee.map(|f| f.to_string()),
        "status": format!("{:?}", t.status),
        "block": t.block,
        "revision": t.revision,
    })
}
