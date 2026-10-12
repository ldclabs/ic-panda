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

use crate::client::Client;
use crate::cloud::{sign_command, Evidence};
use crate::codec::{b64, canonical, decode, digest, sha256, unb64, MAX_RECORD};
use crate::crypto::{hpke_open, hpke_public, hpke_seal, random_key, sign, HpkeEnvelope};
use crate::now_ms;
use crate::payment::{
    escrow_view, Payment, Terms, Wire, WireAccount, WireQuote, WireSignedOffer, WireSignedReceipt,
};
use crate::upload::random_hex;
use crate::wallet::{memo, transfer_call};
use candid::Principal;
use dmsg_types::{
    ensure, ensure_valid,
    payment::{EscrowInfo, FundsDecision, PaymentOffer, SignedOffer},
    profiles::delivery::OpenEscrow,
    user::Capability,
    AccountId, Ed25519Signature, Error, Hash, Result,
};
use icrc_ledger_types::icrc1::{account::Account, transfer::TransferArg};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use zeroize::Zeroizing;

/// Content kind of an inbox key.
pub const INBOX_KEY: &str = "inbox";
/// Largest letter text in bytes.
pub const MAX_LETTER: usize = 6000;
const JOURNAL: &str = "inbox-job:";
const LETTER: &str = "inbox-letter:";
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
    /// Where paid letters' net amount goes; the wallet when absent.
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
        let mut job: Option<Operation> = self.inbox_job(key)?;
        if repeat && job.as_ref().is_some_and(|j| j.result.is_some()) {
            job = None;
        }
        if let Some(job) = &job {
            ensure(
                job.payload == payload && job.path == path,
                Error::IdempotencyConflict,
            )?;
            if let Some(result) = &job.result {
                return Ok(result.clone());
            }
        }
        let state = self.cloud_state().await?;
        let mut job = match job {
            Some(job) if job.deadline > now_ms() + 5_000 => job,
            previous => {
                let signer = self.signer(&state)?;
                let request_id = previous.map_or_else(random_hex, |j| j.request_id);
                let deadline = signer.deadline(now_ms())?;
                let cose = sign_command(&signer, action, &payload, &request_id, deadline);
                let job = Operation {
                    action: action.into(),
                    path: path.into(),
                    payload,
                    request_id,
                    cose,
                    deadline,
                    result: None,
                };
                self.save_inbox_job(key, &job)?;
                job
            }
        };
        let result = self
            .cloud
            .post_signed(
                &self.signer(&state)?,
                &job.path,
                &job.cose,
                &job.request_id,
                job.deadline,
                now_ms(),
            )
            .await?;
        job.result = Some(result.clone());
        self.save_inbox_job(key, &job)?;
        Ok(result)
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
        let me = self.account()?.id;
        self.relay_find(&format!("/v1/inboxes/{me}/policy")).await
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
        ensure_valid(
            matches!(input.mode.as_str(), "closed" | "contacts" | "free" | "paid"),
            "mode",
        )?;
        for account in input.allow.iter().chain(input.block.iter().flatten()) {
            account
                .parse::<AccountId>()
                .map_err(|_| dmsg_types::invalid("account"))?;
        }
        let state = self.cloud_state().await?;
        let generation = self.roots()?.generation;
        ensure(
            state.snapshot.vault_write_state == dmsg_types::user::VaultWriteState::Ready
                && state.snapshot.content_root_generation == generation,
            Error::RekeyRequired,
        )?;
        let payee = match &input.payee {
            Some(text) => Principal::from_text(text).map_err(|_| dmsg_types::invalid("payee"))?,
            None if input.mode == "paid" => self.wallet().await?.0,
            None => Principal::anonymous(),
        };
        let price = match (&input.mode[..], &input.price) {
            ("paid", Some(price)) => {
                let price: u128 = price.parse().map_err(|_| dmsg_types::invalid("price"))?;
                ensure_valid(price > 0, "price")?;
                Some(price)
            }
            ("paid", None) => return Err(dmsg_types::invalid("price")),
            _ => None,
        };
        let me = self.account()?.id.to_string();
        let old = self.inbox_policy().await?;
        let matches = |policy: &Json| {
            policy["mode"] == input.mode.as_str()
                && policy["allow"] == json!(input.allow)
                && input
                    .block
                    .as_ref()
                    .is_none_or(|b| policy["block"] == json!(b))
                && input
                    .invitations
                    .as_ref()
                    .is_none_or(|i| policy["invitations"] == json!(i))
                && input
                    .capacity
                    .is_none_or(|c| policy["capacity"].as_u64() == Some(c))
                && (input.mode != "paid"
                    || (policy["offer"]["offer"]["recipient"]["owner"] == payee.to_text().as_str()
                        && policy["offer"]["offer"]["recipient_net"]
                            == price.unwrap_or(0).to_string().as_str()
                        && policy["offer"]["offer"]["expires_at"]
                            .as_u64()
                            .is_some_and(|t| t > now_ms() + 3_600_000)))
        };
        if let Some(old) = &old {
            if matches(old)
                && old["security_epoch"].as_u64() == Some(state.snapshot.security_epoch)
                && old["root_generation"].as_u64() == Some(generation)
            {
                return Ok(old.clone());
            }
        }
        let expected = old
            .as_ref()
            .and_then(|o| o["version"].as_u64())
            .unwrap_or(0);
        let key = format!("policy:{expected}");
        // The exact policy signed before a lost reply, while it can still commit.
        if let Some(job) = self.inbox_job::<Operation>(&key)? {
            if job.result.is_none() && job.deadline > now_ms() + 5_000 {
                let payload: PolicyPayload = serde_json::from_value(job.payload.clone())
                    .map_err(|_| Error::IntegrityFailed)?;
                ensure(
                    matches(&serde_json::to_value(&payload).unwrap_or_default()),
                    Error::IdempotencyConflict,
                )?;
                return self
                    .inbox_post(
                        &key,
                        &format!("/v1/inboxes/{me}/policy"),
                        POLICY,
                        job.payload,
                        false,
                    )
                    .await;
            }
        }
        let version = old
            .as_ref()
            .and_then(|o| o["inbox_key_version"].as_u64())
            .unwrap_or(0)
            + 1;
        let public = self.inbox_key(version)?;
        let signer = self.signer(&state)?;
        let deadline = signer.deadline(now_ms())?;
        let descriptor = sign_command(
            &signer,
            "dmsg/inbox/key/v1",
            &KeyDescriptor {
                inbox_key_version: version,
                inbox_hpke_pub: hex::encode(public),
                root_generation: generation,
            },
            &random_hex(),
            deadline,
        );
        let offer = match price {
            Some(net) => {
                ensure(
                    state
                        .active(&self.workspace.meta.device_id)?
                        .input
                        .capabilities
                        .contains(&Capability::PaymentOffer),
                    Error::Forbidden,
                )?;
                let (canister, terms) = self.payment_terms(None, None).await?;
                let now = now_ms();
                let offer = PaymentOffer {
                    account_id: self.account()?.id,
                    device_id: self.workspace.meta.device_id,
                    security_epoch: state.snapshot.security_epoch,
                    home_payment: canister,
                    offer_id: Hash::new(crate::crypto::random_id()),
                    ledger: terms.config.ledger,
                    recipient: Account {
                        owner: payee,
                        subaccount: None,
                    },
                    recipient_net: net,
                    quote_scope: Hash::new(crate::crypto::random_id()),
                    version: expected + 1,
                    issued_at: now,
                    expires_at: now + dmsg_types::DAY,
                };
                let signature = sign(
                    &self.keys()?.signing,
                    digest("dmsg/payment-offer/v1", &offer).as_slice(),
                );
                Some(WireSignedOffer {
                    offer: Wire::wire_offer(&offer),
                    signature: b64(&signature),
                })
            }
            None => None,
        };
        let payload = PolicyPayload {
            expected_version: expected,
            prev_hash: old
                .as_ref()
                .and_then(|o| o["hash"].as_str())
                .map(str::to_string),
            mode: input.mode.clone(),
            block: input.block.clone().unwrap_or_else(|| {
                old.as_ref()
                    .and_then(|o| serde_json::from_value(o["block"].clone()).ok())
                    .unwrap_or_default()
            }),
            allow: input.allow.clone(),
            invitations: input.invitations.clone().unwrap_or_else(|| {
                old.as_ref()
                    .and_then(|o| serde_json::from_value(o["invitations"].clone()).ok())
                    .unwrap_or_default()
            }),
            capacity: input.capacity.unwrap_or_else(|| {
                old.as_ref()
                    .and_then(|o| o["capacity"].as_u64())
                    .unwrap_or(100)
            }),
            inbox_key_version: version,
            inbox_hpke_pub: hex::encode(public),
            root_generation: generation,
            key_descriptor: CoseRef {
                cose_sign1: descriptor,
            },
            offer,
        };
        // A new signature replaces an expired one under a fresh key; the
        // relay's version check keeps the late original from applying too.
        if self.inbox_job::<Operation>(&key)?.is_some() {
            let key = format!("{key}:{}", random_hex());
            return self
                .inbox_post(
                    &key,
                    &format!("/v1/inboxes/{me}/policy"),
                    POLICY,
                    serde_json::to_value(&payload).expect("json"),
                    false,
                )
                .await;
        }
        self.inbox_post(
            &key,
            &format!("/v1/inboxes/{me}/policy"),
            POLICY,
            serde_json::to_value(&payload).expect("json"),
            false,
        )
        .await
    }

    /// `recipient`'s public policy, checked: its key descriptor against the
    /// recipient's certified devices and security epoch, its offer against
    /// a `PaymentOffer` device.
    async fn recipient_policy(&mut self, recipient: &AccountId) -> Result<Json> {
        let security = self.peer_security(recipient).await?;
        let policy = self
            .cloud
            .public(&format!("/public/inboxes/{recipient}/policy"))
            .await?
            .ok_or(Error::NotFound)?;
        let cose = policy["key_descriptor"]["cose_sign1"]
            .as_str()
            .ok_or(Error::IntegrityFailed)?;
        let (device, descriptor): (Hash, KeyDescriptor) = crate::cloud::verify_signed_command(
            cose,
            &security.snapshot.issuer,
            &security.devices,
            "dmsg/inbox/key/v1",
        )?;
        ensure(
            security.active(&device).is_ok()
                && policy["security_epoch"].as_u64() == Some(security.snapshot.security_epoch),
            Error::PolicyStale,
        )?;
        ensure(
            policy["inbox_hpke_pub"] == descriptor.inbox_hpke_pub.as_str()
                && policy["inbox_key_version"].as_u64() == Some(descriptor.inbox_key_version)
                && policy["root_generation"].as_u64() == Some(descriptor.root_generation),
            Error::IntegrityFailed,
        )?;
        if !policy["offer"].is_null() {
            let wire: WireSignedOffer = serde_json::from_value(policy["offer"].clone())
                .map_err(|_| Error::IntegrityFailed)?;
            let offer = Wire::offer(&wire)?;
            let author = security
                .active(&offer.offer.device_id)
                .ok()
                .filter(|d| d.input.capabilities.contains(&Capability::PaymentOffer))
                .ok_or(Error::AuthRequired)?;
            dmsg_protocol::verify(
                &author.input.signing_pub,
                digest("dmsg/payment-offer/v1", &offer.offer).as_slice(),
                offer.signature.as_slice(),
            )
            .map_err(|_| Error::AuthRequired)?;
            ensure(offer.offer.expires_at > now_ms(), Error::AuthRequired)?;
        }
        Ok(policy)
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
        let order = order.map_or_else(random_hex, str::to_string);
        ensure_valid(crate::channel_log::is_hash(&order), "order")?;
        ensure_valid(!text.is_empty() && text.len() <= MAX_LETTER, "text")?;
        ensure_valid(
            invitation.is_none_or(crate::channel_log::is_hash),
            "invitation",
        )?;
        let payer = match pay {
            true => Some(WireAccount {
                owner: self.wallet().await?.0.to_text(),
                subaccount: None,
            }),
            false => None,
        };
        let key = format!("outgoing:{order}");
        let text_digest = hex::encode(*sha256(text.as_bytes()));
        let path = format!("/v1/inboxes/{recipient}/contacts");
        if let Some(prior) = self.inbox_job::<Outgoing>(&key)? {
            ensure(
                prior.recipient == recipient.to_string()
                    && prior.text_digest == text_digest
                    && prior.payload.invitation.as_deref() == invitation
                    && prior.payload.payer == payer,
                Error::IdempotencyConflict,
            )?;
            return match self.inbox_order(recipient, &order).await {
                Err(Error::NotFound) => {
                    self.inbox_post(
                        &format!("contact:{order}"),
                        &path,
                        CONTACT,
                        serde_json::to_value(&prior.payload).expect("json"),
                        false,
                    )
                    .await
                }
                other => other,
            };
        }
        let policy = self.recipient_policy(recipient).await?;
        let version = policy["inbox_key_version"]
            .as_u64()
            .ok_or(Error::IntegrityFailed)?;
        let public: [u8; 32] = hex::decode(policy["inbox_hpke_pub"].as_str().unwrap_or_default())
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or(Error::IntegrityFailed)?;
        let me = self.account()?.id.to_string();
        let recipient_text = recipient.to_string();
        let sealed = hpke_seal(
            &public,
            &canonical(&LetterBody { text: text.into() }),
            &letter_context(&me, &recipient_text, &order, version),
        )?;
        let payload = ContactPayload {
            recipient: recipient_text.clone(),
            order_id: order.clone(),
            inbox_key_version: version,
            ciphertext: b64(&canonical(&sealed)),
            invitation: invitation.map(str::to_string),
            payer,
        };
        self.save_inbox_job(
            &key,
            &Outgoing {
                recipient: recipient_text,
                text_digest,
                payload: payload.clone(),
            },
        )?;
        self.inbox_post(
            &format!("contact:{order}"),
            &path,
            CONTACT,
            serde_json::to_value(&payload).expect("json"),
            false,
        )
        .await
    }

    /// An order as the relay holds it; the sender's view is checked
    /// against the letter it sent.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown order.
    pub async fn inbox_order(&mut self, recipient: &AccountId, order: &str) -> Result<Json> {
        let value = self
            .relay_get(&format!("/v1/inboxes/{recipient}/orders/{order}"))
            .await?;
        if let Some(prior) = self.inbox_job::<Outgoing>(&format!("outgoing:{order}"))? {
            ensure(
                value["order_id"] == order
                    && value["recipient"] == prior.recipient.as_str()
                    && value["ciphertext"] == prior.payload.ciphertext.as_str()
                    && value["sender"] == self.account()?.id.to_string().as_str(),
                Error::IntegrityFailed,
            )?;
        }
        Ok(value)
    }

    /// Letters this account sent, by order.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn inbox_outgoing(&self) -> Result<Vec<Json>> {
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

    /// Pay for a quoted letter: open the escrow, fund it from the wallet and
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
        let (payer, ic) = self.wallet().await?;
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
                && terms.config.enabled
                && !terms.signer.revoked
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
                && quote.platform == terms.config.platform
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
            return payment.escrow(&found.escrow_id).await;
        }
        let opened = payment.open(&job.input).await?;
        job.escrow = Some(opened.escrow_id);
        self.save_inbox_job(&key, &job)?;
        payment.escrow(&opened.escrow_id).await
    }

    /// Fund an escrow from the wallet, once.
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
        let (_, ic) = self.wallet().await?;
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
        let (_, ic) = self.wallet().await?;
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
        let (payer, ic) = self.wallet().await?;
        let canister = self.home_config().await?.payment_canister;
        let payment = Payment { ic: &ic, canister };
        let op_id = crate::upload::hex_id(order)?;
        let escrow = payment
            .by_operation(payer, &op_id)
            .await?
            .ok_or(Error::NotFound)?;
        let escrow = payment.escrow(&escrow.escrow_id).await?;
        let transfers = payment.transfers(&escrow.escrow_id).await?;
        Ok(json!({
            "escrow": escrow_view(&escrow),
            "transfers": transfers.iter().map(|t| json!({
                "leg_id": t.leg_id,
                "kind": format!("{:?}", t.kind),
                "amount": t.amount.to_string(),
                "status": format!("{:?}", t.status),
                "block": t.block,
            })).collect::<Vec<_>>(),
        }))
    }

    /// Recover the funds of an order past its acceptance deadline: commit
    /// the refund if nobody did, then send it to the original funding
    /// account. Needs no relay.
    ///
    /// # Errors
    /// Returns `Error::Pending` before the deadline.
    pub async fn inbox_refund(&mut self, order: &str) -> Result<Json> {
        let (payer, ic) = self.wallet().await?;
        let canister = self.home_config().await?.payment_canister;
        let payment = Payment { ic: &ic, canister };
        let op_id = crate::upload::hex_id(order)?;
        let escrow = payment
            .by_operation(payer, &op_id)
            .await?
            .ok_or(Error::NotFound)?;
        let mut escrow = payment.escrow(&escrow.escrow_id).await?;
        if escrow.decision == FundsDecision::Pending {
            ensure(now_ms() >= escrow.quote.accept_by, Error::Pending)?;
            escrow = payment.expiry_refund(&escrow.escrow_id).await?;
        }
        ensure(
            escrow.decision == FundsDecision::RefundCommitted,
            Error::Forbidden,
        )?;
        let deposits = payment.deposits(&escrow.escrow_id).await?;
        let blocks: Vec<u64> = deposits
            .iter()
            .filter(|d| d.refundable > 0 && d.from.owner == payer)
            .map(|d| d.block)
            .collect();
        let leg = match blocks.is_empty() {
            true => None,
            false => Some(
                payment
                    .claim_refund(&escrow.escrow_id, &blocks, true)
                    .await?,
            ),
        };
        if let Some(leg) = &leg {
            let _ = payment
                .process_transfer(&escrow.escrow_id, leg.leg_id)
                .await;
        }
        let escrow = payment.escrow(&escrow.escrow_id).await?;
        Ok(json!({"escrow": escrow_view(&escrow), "refund_leg": leg.map(|l| l.leg_id)}))
    }

    /// Letters to this account, newest state of each, opened with the
    /// synced inbox keys; each is checked against the sender's evidence.
    ///
    /// # Errors
    /// Returns `Error::RecoveryIncomplete` when an inbox key is missing.
    pub async fn inbox_list(&mut self) -> Result<Vec<Letter>> {
        let me = self.account()?.id.to_string();
        let trust = self.trust().await?;
        let mut letters: std::collections::BTreeMap<String, Letter> =
            std::collections::BTreeMap::new();
        let mut after = 0;
        for _ in 0..10_000 {
            let page = self
                .relay_get(&format!("/v1/inboxes/{me}?after={after}&limit=20"))
                .await?;
            let entries = page["entries"].as_array().ok_or(Error::IntegrityFailed)?;
            let next = page["next_cursor"].as_u64().ok_or(Error::IntegrityFailed)?;
            ensure(next >= after, Error::IntegrityFailed)?;
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
                        let cached: Option<Letter> = self
                            .workspace
                            .private(self.keys()?, &format!("{LETTER}{order}"))?;
                        let text = match cached.filter(|l| l.signed.as_deref() == Some(&cose)) {
                            Some(letter) => letter.text,
                            None => {
                                let evidence: Evidence =
                                    serde_json::from_value(value["evidence"].clone())
                                        .map_err(|_| Error::IntegrityFailed)?;
                                let created =
                                    value["created_at"].as_u64().ok_or(Error::IntegrityFailed)?;
                                let verified: crate::evidence::Verified<ContactPayload> = trust
                                    .command(&cose, &evidence, created, Some(CONTACT), now_ms())?;
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
                self.workspace
                    .put_private(self.keys()?, &format!("{LETTER}{order}"), &letter)?;
                letters.insert(order, letter);
                after = cursor;
            }
            if next == after || entries.is_empty() {
                return Ok(letters.into_values().collect());
            }
            after = next;
        }
        Err(Error::QuotaExceeded)
    }

    /// Mark a letter read or archived; archiving reopens contact for its
    /// sender.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown order.
    pub async fn inbox_mark(&mut self, order: &str, read: bool, archived: bool) -> Result<Json> {
        ensure_valid(crate::channel_log::is_hash(order), "order")?;
        let me = self.account()?.id;
        self.inbox_post(
            &format!("mark:{order}:{read}:{archived}"),
            &format!("/v1/inboxes/{me}/mark"),
            "dmsg/inbox/mark/v1",
            json!({"order_id": order, "read": read, "archived": archived, "reopen_contact": archived}),
            true,
        )
        .await
    }
}

#[cfg(test)]
#[path = "inbox_tests.rs"]
mod tests;
