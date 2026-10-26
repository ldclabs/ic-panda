//! Plans and membership (`docs/protocol/commerce_zh.md`): the certified
//! catalog and this account's entitlement, cash checkouts and PANDA
//! membership claims for the account's own subscription.
//!
//! A checkout is a journal fixed at its quote: the device approves exactly
//! those terms for the payer (`approve_application`), commerce
//! opens the order, and the payer funds it once. Every step is reconciled
//! by the order's ID after a lost reply; a definite canister rejection is
//! the only reason to quote again. dMsg's own checkouts use the
//! application origin `https://dmsg.net` (plan D-19).

mod prepare;
pub use prepare::{checkout_request, plan_quote};

use crate::client::Client;
use crate::codec::decode;
use crate::crypto::sign;
use crate::hold::{Hold, Unlocked};
use crate::ic::Ic;
use crate::now_ms;
use crate::payer::{Ledger, memo, payer, transfer_call};
use crate::payment::certified_leaf;
use crate::upload::hex_id;
use candid::Principal;
use dmsg_protocol::{
    approval_message, billing, commerce_v2, digest,
    integration::{
        self, validate_app, validate_application_approval, validate_billing_offer, validate_product,
    },
};
use dmsg_types::{
    Approval, CertifiedBatch, Ed25519Signature, Error, Hash, Result,
    billing::{
        Catalog, ContractSource, EntitlementView, MembershipContract, MembershipTerms, PlanId,
        RENEWAL_WINDOW_MS, ResourceLimits, StorageAddon,
    },
    ensure, ensure_valid,
    integration::{
        AppRegistration, ApplicationApproval, ApprovalPurpose, BillingOffer, ProductRegistration,
        SettlementMethod,
    },
    integration_billing::{
        CashBlock, CashTransfer, CashTransferProgress, CashTransferStatus, CashTransfersPage,
        CheckoutDepositsPage, CheckoutOperationsPage, CheckoutProgress, CheckoutQuote,
        CheckoutRequest, CheckoutStatus, CheckoutView, OpenCheckout, ProductAuthorizationRequest,
        SettlementAssetView,
    },
    integration_membership::{
        PandaApplicationTerms, PandaClaimRequest, PandaClaimStatus, PandaClaimView,
        PandaOperationsPage, PandaServiceView,
    },
    membership::{Beneficiary, Eligibility},
    user::{Capability, UserInit},
};
use icrc_ledger_types::icrc1::{account::Account, transfer::TransferArg};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

/// dMsg's own application ID and origin.
pub const DMSG_APP: &str = "dmsg";
/// The origin dMsg's own checkouts carry.
pub const DMSG_ORIGIN: &str = "https://dmsg.net";
const JOURNAL: &str = "checkout:";
const APPLICATION: &str = "dmsg/application/approve/v1";
/// A device approval is valid for at most four minutes.
const APPROVAL_MS: u64 = 240_000;

/// Fixed terms of a checkout.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Terms {
    /// Cash on one ledger.
    Cash(Box<CheckoutQuote>),
    /// A full PANDA waiver.
    Panda(Box<PandaApplicationTerms>),
}

/// Where a checkout stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Quoted; nothing approved.
    Review,
    /// The device approval is signed.
    Authorized,
    /// Sent; the order's state is read from the service.
    Unknown,
    /// The service accepted the order.
    Accepted,
}

/// A checkout's journal, sealed under the LDK.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Checkout {
    /// Order or claim ID, hex.
    pub id: String,
    /// The paying or neuron-controlling identity.
    pub actor: Principal,
    /// The application origin.
    pub origin: String,
    /// The request.
    pub request: CheckoutRequest,
    /// The quoted terms.
    pub terms: Terms,
    /// The signed device approval.
    pub approval: Option<(ApplicationApproval, Approval)>,
    /// Progress.
    pub stage: Stage,
    /// When this device quoted it, in Unix milliseconds.
    #[serde(default)]
    pub created_at_ms: Option<u64>,
}

impl Checkout {
    /// The commitment a person approves: the exact terms and payer.
    pub fn digest(&self) -> Hash {
        digest(
            "dmsg/checkout-approval/v1",
            &(&self.id, self.actor, &self.origin, &self.terms),
        )
    }

    /// The checkout for display.
    pub fn view(&self) -> Json {
        let offer = &self.request.offer;
        let terms = match &self.terms {
            Terms::Cash(q) => json!({
                "method": "Cash",
                "ledger": q.cash.ledger.to_text(),
                "asset": q.asset.asset.to_string(),
                "amount": q.cash.amount_atomic.to_string(),
                "fee_reserve": q.cash.fee_reserve_atomic.to_string(),
                "network_fee": q.asset.network_fee_atomic.to_string(),
                "decimals": q.asset.decimals,
                "funding_deadline_ms": q.cash.funding_deadline_ms,
                "activation_deadline_ms": q.cash.activation_deadline_ms,
            }),
            Terms::Panda(t) => json!({
                "method": "Panda",
                "neuron": hex::encode(*t.neuron_id),
                "required_stake_e8s": t.quote.required_stake_e8s.to_string(),
                "application_deadline_ms": t.quote.application_deadline_ms,
                "committed_until_ms": t.quote.committed_until_ms,
            }),
        };
        json!({
            "id": self.id,
            "actor": self.actor.to_text(),
            "origin": self.origin,
            "app_id": offer.app_id,
            "product_id": offer.product_id,
            "sku": offer.sku,
            "amount_usd_micros": offer.amount_usd_micros.to_string(),
            "starts_at_ms": offer.starts_at_ms,
            "expires_at_ms": offer.expires_at_ms,
            "accept_by_ms": offer.accept_by_ms,
            "terms": terms,
            "stage": self.stage,
            "created_at_ms": self.created_at_ms,
            "digest": hex::encode(*self.digest()),
        })
    }

    /// The deadline after which the terms can no longer be approved or paid.
    pub fn deadline_ms(&self) -> u64 {
        match &self.terms {
            Terms::Cash(q) => q.cash.funding_deadline_ms,
            Terms::Panda(t) => t.quote.application_deadline_ms,
        }
    }
}

/// What a cash quote's funding transfer costs the payer: the amount and fee
/// reserve the transfer sends, plus the network fee the ledger charges.
fn cash_total(amount: u128, fee_reserve: u128, network_fee: u128) -> Result<u128> {
    amount
        .checked_add(fee_reserve)
        .and_then(|total| total.checked_add(network_fee))
        .ok_or(Error::IntegrityFailed)
}

/// Whether a saved device approval still covers a checkout at `at`.
fn approval_live(application_expires_at_ms: u64, approval_expires_at: u64, at: u64) -> bool {
    at < application_expires_at_ms && at < approval_expires_at
}

/// The canisters checkouts use.
struct Services {
    home: Principal,
    commerce: Principal,
    membership: Principal,
}

fn status_name(view: &CheckoutView, at: u64) -> &'static str {
    if let Some(c) = &view.cancellation
        && c.cancelled
    {
        return if at < c.cancelled_at_ms {
            "Closing"
        } else {
            "Cancelled"
        };
    }
    match view.progress.status {
        CheckoutStatus::Reserving => "Reserving",
        CheckoutStatus::AwaitingFunding => "AwaitingFunding",
        CheckoutStatus::Applying => "Applying",
        CheckoutStatus::Applied => "Applied",
        CheckoutStatus::RefundCommitted => "RefundCommitted",
        CheckoutStatus::Rejected => "Rejected",
    }
}

fn claim_name(view: &PandaClaimView) -> String {
    format!("{:?}", view.status)
}

/// An order's state for display.
fn order_view(state: &Order) -> Json {
    let now = now_ms();
    match state {
        Order::Cash(view) => json!({
            "id": hex::encode(*view.progress.order_id),
            "method": "Cash",
            "status": status_name(view, now),
            "receipt": view.receipt.is_some(),
            "sku": view.quote.offer.sku,
            "amount": view.quote.cash.amount_atomic.to_string(),
            "asset": view.quote.asset.asset.to_string(),
            "decimals": view.quote.asset.decimals,
            "starts_at_ms": view.quote.offer.starts_at_ms,
            "expires_at_ms": view.cancellation.as_ref()
                .filter(|c| c.cancelled)
                .map_or(view.quote.offer.expires_at_ms, |c| c.cancelled_at_ms.min(view.quote.offer.expires_at_ms)),
            "funding_deadline_ms": view.quote.cash.funding_deadline_ms,
            "applied_at_ms": view.receipt.as_ref().map(|r| r.applied_at_ms),
            "service_reserve": view.service_reserve_atomic.to_string(),
        }),
        Order::Panda(view) => json!({
            "id": hex::encode(*view.claim_id),
            "method": "Panda",
            "status": claim_name(view),
            "eligibility": format!("{:?}", view.eligibility),
            "actor": view.terms.actor.to_text(),
            "neuron": hex::encode(*view.terms.neuron_id),
        }),
    }
}

/// An order as the service reports it.
pub enum Order {
    /// A cash order.
    Cash(Box<CheckoutView>),
    /// A PANDA claim.
    Panda(Box<PandaClaimView>),
}

impl Order {
    /// The view's canonical CBOR.
    pub fn canonical(&self) -> Vec<u8> {
        match self {
            Order::Cash(view) => crate::codec::canonical(view),
            Order::Panda(view) => crate::codec::canonical(view),
        }
    }
}

impl Client {
    async fn services(&mut self) -> Result<Services> {
        services(&self.hold()).await
    }

    fn beneficiary(&self) -> Result<Beneficiary> {
        let account = self.account()?;
        Ok(billing::beneficiary(account.home, &account.id))
    }

    /// The certified catalog.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for an uncertified answer.
    pub async fn plan_catalog(&mut self) -> Result<Catalog> {
        catalog(&self.hold()).await
    }

    /// The certified registrations of dMsg's own application and product.
    async fn registrations(
        &mut self,
        commerce: Principal,
        product: &str,
    ) -> Result<(AppRegistration, ProductRegistration)> {
        prepare::registrations(&self.hold(), commerce, product).await
    }

    /// The payer's agent for a checkout `actor`.
    async fn actor(&mut self, actor: Principal) -> Result<Ic> {
        let (payer, ic) = self.payer().await?;
        ensure(payer == actor, Error::Forbidden)?;
        Ok(ic)
    }

    /// Refuse a cash checkout the payer cannot fund now. The balance on its
    /// ledger must cover the amount, the fee reserve and the network fee the
    /// funding transfer costs; a PANDA checkout moves no cash.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` when the payer changed since the quote,
    /// `Error::Unavailable` when the ledger cannot answer, the client's
    /// `INSUFFICIENT_FUNDS` code when the balance is short, and
    /// `Error::IntegrityFailed` when the fixed terms do not add up.
    pub(crate) async fn checkout_funds(&mut self, job: &Checkout) -> Result<()> {
        prepare::funds(&self.hold(), job).await
    }

    fn checkout(&self, id: &str) -> Result<Checkout> {
        self.workspace
            .private(self.keys()?, &format!("{JOURNAL}{id}"))?
            .ok_or(Error::NotFound)
    }

    fn save_checkout(&self, job: &Checkout) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{JOURNAL}{}", job.id), job)
    }

    /// Quote this account's own subscription to `sku` (such as `plus`):
    /// cash on the ledger `selection`, or a PANDA waiver backed by the
    /// neuron `selection` (hex), paid or controlled by the payer. Returns
    /// the journal to approve.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` without `FormalApprove` on this device.
    pub async fn plan_quote(
        &mut self,
        sku: &str,
        method: SettlementMethod,
        selection: &str,
    ) -> Result<Checkout> {
        plan_quote(&self.hold(), sku, method, selection).await
    }

    /// Fix the terms of a web application's checkout from `origin` for the
    /// payer: cash on the ledger `selection`, or a PANDA waiver backed by
    /// the neuron `selection` (hex).
    pub(crate) async fn quote_request(
        &mut self,
        request: CheckoutRequest,
        origin: &str,
        selection: &str,
    ) -> Result<Checkout> {
        prepare::quote_request(&self.hold(), request, origin, selection).await
    }

    /// A checkout journal.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown checkout.
    pub fn plan_checkout(&self, id: &str) -> Result<Checkout> {
        self.checkout(id)
    }

    /// Every checkout journal.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn plan_checkouts(&self) -> Result<Vec<Checkout>> {
        let mut jobs = self
            .workspace
            .store
            .sealed_keys(JOURNAL)?
            .iter()
            .map(|key| {
                self.workspace
                    .private(self.keys()?, key)?
                    .ok_or(Error::IntegrityFailed)
            })
            .collect::<Result<Vec<Checkout>>>()?;
        // Newest first; journals from before the field come last.
        jobs.sort_by_key(|job| std::cmp::Reverse(job.created_at_ms));
        Ok(jobs)
    }

    /// Forget a checkout nothing was sent for: a quote nobody approved, or an
    /// approved one whose deadline passed before it reached the service.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` once the service may hold the order.
    pub fn plan_discard(&self, id: &str) -> Result<()> {
        let job = self.checkout(id)?;
        ensure(
            job.stage == Stage::Review
                || (job.stage == Stage::Authorized && now_ms() >= job.deadline_ms()),
            Error::Forbidden,
        )?;
        self.workspace.delete_private(&format!("{JOURNAL}{id}"))
    }

    /// The order's state at the service, checked against the journal.
    ///
    /// # Errors
    /// Returns `Error::NotFound` before the service has the order.
    pub async fn plan_order(&mut self, id: &str) -> Result<Order> {
        order(&self.hold(), id).await
    }

    /// The order's state for display.
    ///
    /// # Errors
    /// As [`Self::plan_order`].
    pub async fn plan_order_view(&mut self, id: &str) -> Result<Json> {
        let order = self.plan_order(id).await?;
        Ok(order_view(&order))
    }

    /// Sign the device approval of a checkout's exact terms, once; with
    /// `renew`, sign a new one when the saved approval no longer covers the
    /// checkout.
    async fn authorize_checkout(
        &mut self,
        job: &mut Checkout,
        services: &Services,
        renew: bool,
    ) -> Result<()> {
        let now = now_ms();
        let saved = job
            .approval
            .as_ref()
            .is_some_and(|(application, approval)| {
                approval_live(application.expires_at_ms, approval.expires_at, now)
            });
        if saved || (job.approval.is_some() && !renew) {
            return Ok(());
        }
        let (app, product) = self
            .registrations(services.commerce, &job.request.offer.product_id)
            .await?;
        let now = now_ms();
        let (service, purpose, action_digest, deadline) = match &job.terms {
            Terms::Cash(q) => (
                services.commerce,
                ApprovalPurpose::CashCheckout,
                commerce_v2::checkout_quote_hash(q),
                q.cash.funding_deadline_ms,
            ),
            Terms::Panda(t) => (
                services.membership,
                ApprovalPurpose::PandaSubscription,
                commerce_v2::panda_application_hash(t),
                t.quote.application_deadline_ms,
            ),
        };
        ensure(
            now < deadline && (renew || now < job.request.offer.accept_by_ms),
            Error::Expired,
        )?;
        let account = self.account()?.clone();
        let state = self.refresh(account.home, &account.id).await?;
        let device = state.active(&self.workspace.meta.device_id)?;
        ensure(
            device
                .input
                .capabilities
                .contains(&Capability::FormalApprove),
            Error::Forbidden,
        )?;
        let application = ApplicationApproval {
            version: 1,
            environment: app.environment.clone(),
            app_id: app.app_id.clone(),
            app_config_version: app.config_version,
            origin: job.origin.clone(),
            approving_account: job.request.approving_account,
            service,
            beneficiary: job.request.offer.beneficiary.clone(),
            actor: job.actor,
            purpose,
            action_digest,
            operation_id: job.request.offer.operation_id,
            nonce: Hash::new(crate::crypto::random_id()),
            expires_at_ms: (now + APPROVAL_MS).min(deadline),
        };
        validate_application_approval(&application, &app, &product, service, now)?;
        let mut approval = Approval {
            device_id: self.workspace.meta.device_id,
            security_epoch: state.info.security_epoch,
            sequence: device.next_sequence,
            request_id: Hash::new(crate::crypto::random_id()),
            expires_at: application.expires_at_ms,
            signature: Ed25519Signature::new([0; 64]),
            hardware: None,
        };
        let message = approval_message(
            account.home,
            &account.id,
            APPLICATION,
            &application,
            &approval,
        );
        approval.signature = Ed25519Signature::new(sign(&self.keys()?.signing, message.as_slice()));
        approval.hardware = self.hardware_proof(
            &state.info,
            dmsg_protocol::application_purpose(&application.purpose),
            message,
        )?;
        job.approval = Some((application, approval));
        // A renewed approval advances an order the service may already hold;
        // only the first signature moves the journal out of review.
        if job.stage == Stage::Review {
            job.stage = Stage::Authorized;
        }
        self.save_checkout(job)
    }

    /// Approve and open a checkout, then fund a cash order from the payer.
    /// A retry reads the service's state first and repeats only what is
    /// missing; the saved approval is sent again, and a PANDA advance renews
    /// it only after the saved one expired.
    ///
    /// # Errors
    /// Returns `Error::ExecutionUnknown` while a step's outcome is unknown.
    pub(crate) async fn plan_execute(&mut self, id: &str) -> Result<Json> {
        let mut job = self.checkout(id)?;
        let services = self.services().await?;
        if job.stage != Stage::Review {
            match self.plan_order(id).await {
                // A checking PANDA application is progressed here, so the
                // approval completes without the person reconciling it.
                Ok(Order::Panda(view)) if view.status == PandaClaimStatus::Checking => {
                    if view.eligibility == Eligibility::Eligible {
                        return self.plan_advance(id).await;
                    }
                    let view = self.reconcile_claim(id, job.actor).await?;
                    ensure(view.status != PandaClaimStatus::Checking, Error::Pending)?;
                    return Ok(order_view(&Order::Panda(Box::new(view))));
                }
                Ok(order) => return self.plan_fund(&job, order).await,
                Err(Error::NotFound) => {}
                Err(error) => return Err(error),
            }
        }
        self.authorize_checkout(&mut job, &services, false).await?;
        let (application, approval) = job.approval.clone().ok_or(Error::IntegrityFailed)?;
        let login = self.session()?.ic.clone();
        let approved: Result<Hash> = login
            .update(
                services.home,
                "approve_application",
                (&application, &approval),
            )
            .await?;
        approved?;
        job.stage = Stage::Unknown;
        self.save_checkout(&job)?;
        let authorization = ProductAuthorizationRequest {
            offer: job.request.offer.clone(),
            account_approval: application,
            user_home: services.home,
            approval_id: approval.request_id,
            product_approval: job.request.product_approval.clone(),
        };
        let ic = self.actor(job.actor).await?;
        match &job.terms {
            Terms::Cash(quote) => {
                let opened: Result<CheckoutView> = ic
                    .update(
                        services.commerce,
                        "open_checkout",
                        (OpenCheckout {
                            quote: (**quote).clone(),
                            authorization,
                        },),
                    )
                    .await?;
                opened?;
            }
            Terms::Panda(terms) => {
                let requested: Result<PandaClaimView> = ic
                    .update(
                        services.membership,
                        "request_panda_claim",
                        (PandaClaimRequest {
                            terms: (**terms).clone(),
                            authorization,
                        },),
                    )
                    .await?;
                // The claim is saved before a pending answer; keep the
                // approval running and let the next pass reconcile it.
                let view = match requested {
                    Ok(view) => view,
                    Err(Error::Pending) => return Err(Error::Pending),
                    Err(error) => return Err(error),
                };
                job.stage = Stage::Accepted;
                self.save_checkout(&job)?;
                if view.status == PandaClaimStatus::Checking
                    && view.eligibility == Eligibility::Eligible
                {
                    return self.plan_advance(id).await;
                }
                return Ok(order_view(&Order::Panda(Box::new(view))));
            }
        }
        job.stage = Stage::Accepted;
        self.save_checkout(&job)?;
        let order = self.plan_order(id).await?;
        self.plan_fund(&job, order).await
    }

    /// Fund a cash order awaiting funds, once, and report its state.
    async fn plan_fund(&mut self, job: &Checkout, order: Order) -> Result<Json> {
        let Order::Cash(view) = &order else {
            return Ok(order_view(&order));
        };
        if view.progress.status != CheckoutStatus::AwaitingFunding {
            return Ok(order_view(&order));
        }
        let quote = view.quote.clone();
        let cash = quote.cash.clone();
        let fee = quote.asset.network_fee_atomic;
        ensure(fee <= quote.asset.max_network_fee_atomic, Error::FeeBlocked)?;
        let order_id = view.progress.order_id;
        let create = || {
            ensure(cash.funding_deadline_ms > now_ms(), Error::Expired)?;
            transfer_call(&TransferArg {
                from_subaccount: cash.payer.subaccount,
                to: cash.deposit,
                fee: Some(fee.into()),
                created_at_time: Some(now_ms() * 1_000_000),
                memo: Some(memo(
                    digest("dmsg/checkout/funding/v2", &order_id).as_slice(),
                )),
                amount: (cash.amount_atomic + cash.fee_reserve_atomic).into(),
            })
        };
        let block = self
            .pay(
                &format!("checkout-funding:{}", hex::encode(*order_id)),
                cash.ledger,
                fee,
                create,
            )
            .await?;
        let services = self.services().await?;
        let ic = self.actor(job.actor).await?;
        let funded: Result<CheckoutProgress> = ic
            .update(
                services.commerce,
                "check_checkout_funding",
                (
                    order_id,
                    CashBlock {
                        ledger: cash.ledger,
                        block_index: u128::from(block),
                    },
                ),
            )
            .await?;
        funded?;
        self.plan_order_view(&job.id).await
    }

    /// Ask membership to finish a checking PANDA claim's qualification.
    /// `Error::Pending` means another SNS or product check is still running.
    async fn reconcile_claim(&mut self, id: &str, actor: Principal) -> Result<PandaClaimView> {
        let services = self.services().await?;
        let ic = self.actor(actor).await?;
        let view: Result<PandaClaimView> = ic
            .update(services.membership, "reconcile_panda_claim", (hex_id(id)?,))
            .await?;
        view
    }

    /// Apply a qualified PANDA claim immediately, reusing the device approval
    /// of the same terms when it is still valid and signing a fresh one after
    /// it expired.
    ///
    /// An unknown reply may mean the durable approval or Apply already
    /// succeeded; both are idempotent, so one immediate retry settles the
    /// common transport loss before the approval is left for reconciliation.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a cash order.
    pub(crate) async fn plan_advance(&mut self, id: &str) -> Result<Json> {
        match self.plan_advance_once(id).await {
            Err(Error::ExecutionUnknown) => self.plan_advance_once(id).await,
            result => result,
        }
    }

    async fn plan_advance_once(&mut self, id: &str) -> Result<Json> {
        let mut job = self.checkout(id)?;
        let Terms::Panda(_) = &job.terms else {
            return Err(Error::Forbidden);
        };
        let services = self.services().await?;
        if let Order::Panda(view) = self.plan_order(id).await? {
            if view.status == PandaClaimStatus::Applying {
                let view = self.reconcile_claim(id, job.actor).await?;
                return Ok(order_view(&Order::Panda(Box::new(view))));
            }
            if view.status != PandaClaimStatus::Checking {
                return Ok(order_view(&Order::Panda(view)));
            }
        }
        self.authorize_checkout(&mut job, &services, true).await?;
        let (application, approval) = job.approval.clone().ok_or(Error::IntegrityFailed)?;
        let login = self.session()?.ic.clone();
        let approved: Result<Hash> = login
            .update(
                services.home,
                "approve_application",
                (&application, &approval),
            )
            .await?;
        approved?;
        let authorization = ProductAuthorizationRequest {
            offer: job.request.offer.clone(),
            account_approval: application,
            user_home: services.home,
            approval_id: approval.request_id,
            product_approval: job.request.product_approval.clone(),
        };
        let ic = self.actor(job.actor).await?;
        let advanced: Result<PandaClaimView> = ic
            .update(
                services.membership,
                "advance_panda_claim",
                (hex_id(id)?, authorization),
            )
            .await?;
        // The claim is saved before a pending answer; keep the approval
        // running and let the next pass retry the application.
        match advanced {
            Ok(_) => {}
            Err(Error::Pending) => return Err(Error::Pending),
            Err(error) => return Err(error),
        }
        self.plan_order_view(id).await
    }
}

/// What the maintenance pages list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Records {
    /// Checkout orders with their per-ledger balances.
    Orders,
    /// Outgoing cash transfers.
    Transfers,
    /// PANDA claims and their eligibility leases.
    Claims,
}

/// A certified leaf of a commerce or membership batch `ic` reads, decoded.
async fn certified_by<
    T: serde::de::DeserializeOwned + Serialize,
    A: candid::utils::ArgumentEncoder,
>(
    ic: &Ic,
    canister: Principal,
    method: &str,
    args: A,
    key: &[u8],
) -> Result<Option<T>> {
    let batch: Result<CertifiedBatch> = ic.query(canister, method, args).await?;
    let batch = batch?;
    certified_leaf(&batch, key, canister, &ic.root_key(), now_ms())?
        .map(|value| decode(&value, 1 << 20))
        .transpose()
}

/// The canisters of this account's user home, read without the client.
async fn services<H: Hold>(h: &H) -> Result<Services> {
    let home = h.with(|c| Ok(c.account()?.home)).await?;
    let init: UserInit = h.reader().await?.query(home, "user_config", ()).await?;
    Ok(Services {
        home,
        commerce: init.commerce_canister,
        membership: init.membership_canister,
    })
}

/// The certified catalog, read without the client.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for an uncertified answer.
pub async fn catalog<H: Hold>(h: &H) -> Result<Catalog> {
    let services = services(h).await?;
    catalog_by(&h.anonymous().await?, services.commerce).await
}

async fn catalog_by(ic: &Ic, commerce: Principal) -> Result<Catalog> {
    certified_by(
        ic,
        commerce,
        "get_catalog",
        (),
        billing::catalog_key().as_slice(),
    )
    .await?
    .ok_or(Error::IntegrityFailed)
}

/// The settlement assets checkouts may pay with, read without the client.
///
/// # Errors
/// Query failures return `Error::Unavailable`.
pub async fn plan_assets<H: Hold>(h: &H) -> Result<Json> {
    let services = services(h).await?;
    let assets: Vec<SettlementAssetView> = h
        .anonymous()
        .await?
        .query(services.commerce, "settlement_assets", ())
        .await?;
    Ok(Json::Array(
        assets
            .iter()
            .filter(|a| a.ledger_verified && a.policy.enabled)
            .map(|a| {
                json!({
                    "ledger": a.policy.ledger.to_text(),
                    "asset": a.policy.asset.to_string(),
                    "decimals": a.policy.decimals,
                    "network_fee": a.policy.network_fee_atomic.to_string(),
                })
            })
            .collect(),
    ))
}

/// Resource limits for display.
fn limits_view(l: &ResourceLimits) -> Json {
    json!({
        "storage_bytes": l.storage_bytes,
        "active_channels": l.active_channels,
        "monthly_execution_units": l.monthly_execution_units,
    })
}

/// The certified catalog for display: the plans with their limits and the
/// storage products, by product ID hex.
pub fn catalog_view(c: &Catalog) -> Json {
    json!({
        "version": c.version,
        "effective_at_ms": c.effective_at_ms,
        "plans": c.plans.iter().map(|p| json!({
            "plan_id": format!("{:?}", p.plan_id),
            "price_cents": p.price_cents,
            "limits": limits_view(&p.limits),
            "terms_version": p.terms_version,
        })).collect::<Vec<_>>(),
        "storage_products": c.storage_products.iter().map(|p| json!({
            "product_id": hex::encode(*p.product_id),
            "storage_bytes": p.storage_bytes,
            "price_cents": p.price_cents,
        })).collect::<Vec<_>>(),
    })
}

/// When a contract's rights end: its term, or an earlier termination.
fn contract_end(c: &MembershipContract) -> u64 {
    c.expires_at_ms.min(c.terminated_at_ms.unwrap_or(u64::MAX))
}

/// A base contract for display.
fn contract_view(c: &MembershipContract) -> Json {
    let (method, source) = match &c.source {
        ContractSource::Cash { order_id } => ("Cash", hex::encode(**order_id)),
        ContractSource::Sns { claim_id } => ("Panda", hex::encode(**claim_id)),
        ContractSource::AppStore { transaction_id } => ("AppStore", hex::encode(**transaction_id)),
        ContractSource::Compensation => ("Compensation", String::new()),
    };
    json!({
        "contract": hex::encode(*c.contract_id),
        "plan": format!("{:?}", c.plan.plan_id),
        "method": method,
        "source": source,
        "term_starts_at_ms": c.term_starts_at_ms,
        "starts_at_ms": c.starts_at_ms,
        "expires_at_ms": c.expires_at_ms,
        "terminated_at_ms": c.terminated_at_ms,
        "ends_at_ms": contract_end(c),
    })
}

/// The base contract in force now: the newest one that has started, as
/// commerce's own projection picks it. An early refund keeps its contract
/// until the lease it already issued runs out, so a purchase made after it
/// is the one in force.
fn in_force<'a>(live: &[&'a MembershipContract], now: u64) -> Option<&'a MembershipContract> {
    live.iter().rev().find(|c| c.starts_at_ms <= now).copied()
}

/// The one renewal queued after the term in force: contracts come oldest
/// first, so it is the first that has not started.
fn queued<'a>(live: &[&'a MembershipContract], now: u64) -> Option<&'a MembershipContract> {
    live.iter().find(|c| c.starts_at_ms > now).copied()
}

/// What each paid plan's card offers this account now: a first
/// subscription, a renewal or a switch from the end of the current term
/// (within [`RENEWAL_WINDOW_MS`] of it), an upgrade for the rest of a cash
/// term, or why none is possible yet. A term that was refunded early ends
/// with its lease; a new subscription starts at once. Each offer names the
/// term end (`ends_at_ms`) that a PANDA neuron's lock must cover.
fn offers(
    catalog: &Catalog,
    current: Option<&MembershipContract>,
    next: Option<&MembershipContract>,
    now: u64,
) -> Result<Vec<Json>> {
    let price = |plan: &PlanId| {
        catalog
            .plans
            .iter()
            .find(|p| p.plan_id == *plan)
            .map(|p| p.price_cents)
    };
    let current = current.filter(|c| c.terminated_at_ms.is_none());
    catalog
        .plans
        .iter()
        .filter(|p| p.price_cents > 0)
        .map(|p| -> Result<Json> {
            let plan = format!("{:?}", p.plan_id);
            let sku = plan.to_ascii_lowercase();
            let offer = |action: &str,
                         sku: String,
                         starts: u64,
                         ends: u64,
                         methods: &[&str]|
             -> Result<Json> {
                Ok(json!({
                    "plan": plan,
                    "action": action,
                    "sku": sku,
                    "starts_at_ms": starts,
                    "ends_at_ms": ends,
                    "methods": methods,
                }))
            };
            let wait = |reason: &str, from: Option<u64>| -> Result<Json> {
                Ok(json!({
                    "plan": plan,
                    "action": "unavailable",
                    "reason": reason,
                    "available_at_ms": from,
                }))
            };
            let Some(c) = current else {
                return offer(
                    "subscribe",
                    sku,
                    now,
                    billing::next_year(now)?,
                    &["Cash", "Panda"],
                );
            };
            let same = c.plan.plan_id == p.plan_id;
            let higher = price(&c.plan.plan_id).is_some_and(|cents| p.price_cents > cents);
            if higher && matches!(c.source, ContractSource::Cash { .. }) {
                return offer(
                    "upgrade",
                    format!("upgrade-{sku}"),
                    now,
                    c.expires_at_ms,
                    &["Cash"],
                );
            }
            if next.is_some() {
                return wait("RENEWAL_QUEUED", None);
            }
            let from = c.expires_at_ms.saturating_sub(RENEWAL_WINDOW_MS);
            if now < from {
                let reason = match (same, higher) {
                    (true, _) => "RENEWAL_WINDOW",
                    // Only a cash term is upgraded midway.
                    (false, true) => "UPGRADE_CASH_ONLY",
                    (false, false) => "SWITCH_AT_RENEWAL",
                };
                return wait(reason, Some(from));
            }
            let action = if same { "renew" } else { "switch" };
            offer(
                action,
                sku,
                c.expires_at_ms,
                billing::next_year(c.expires_at_ms)?,
                &["Cash", "Panda"],
            )
        })
        .collect()
}

fn addon_view(a: &StorageAddon) -> Json {
    json!({
        "contract": hex::encode(*a.contract_id),
        "order": hex::encode(*a.order_id),
        "storage_bytes": a.storage_bytes,
        "starts_at_ms": a.starts_at_ms,
        "expires_at_ms": a.expires_at_ms,
    })
}

/// This account's entitlement and terms, refreshed first with `refresh`; a
/// certified absence means the catalog's Free plan. The terms come from an
/// uncertified query and are only shown: the current and the next base
/// contract, when a renewal may be bought, what each plan's card offers,
/// whether storage packs may be bought and the first purchase's refund
/// window. With `refresh` the relay is asked to read the entitlement again,
/// so its limits follow a purchase at once; `synced` says whether it did.
/// The IC is waited on without the client.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for an uncertified answer.
pub async fn plan_status<H: Hold>(h: &H, refresh: bool) -> Result<Json> {
    let beneficiary = h.with(|c| c.beneficiary()).await?;
    let commerce = services(h).await?.commerce;
    let ic = h.anonymous().await?;
    if refresh {
        let refreshed: Result<EntitlementView> = ic
            .update(commerce, "refresh_entitlement", (&beneficiary,))
            .await?;
        match refreshed {
            Ok(_) | Err(Error::NotFound) => {}
            Err(error) => return Err(error),
        }
    }
    // Independent certified entitlement, contract terms and catalog reads
    // share the discovered commerce canister and agent.
    let beneficiaries = vec![&beneficiary];
    let key = billing::entitlement_key(&beneficiary);
    let (view, terms, catalog): (Option<EntitlementView>, Result<MembershipTerms>, Catalog) = futures_util::try_join!(
        certified_by(
            &ic,
            commerce,
            "get_entitlement_batch",
            (beneficiaries,),
            key.as_slice()
        ),
        ic.query(commerce, "get_membership_terms", (&beneficiary,)),
        catalog_by(&ic, commerce)
    )?;
    let terms = terms?;
    let mut status = match view {
        Some(view) => json!({
            "plan": format!("{:?}", view.plan_snapshot.plan_id),
            "source": format!("{:?}", view.source_status),
            "eligibility": format!("{:?}", view.eligibility_status),
            "limits": limits_view(&view.effective_limits),
            "addons": view.addons.iter().map(addon_view).collect::<Vec<_>>(),
            "lease_until_ms": view.valid_until_ms,
            "next_change_at_ms": view.next_limit_change_at_ms,
            "repair_deadline_ms": view.repair_deadline_ms,
            "service_terminated_at_ms": view.service_terminated_at_ms,
        }),
        None => {
            let free = catalog
                .plans
                .iter()
                .find(|p| p.plan_id == PlanId::Free)
                .ok_or(Error::IntegrityFailed)?;
            json!({
                "plan": "Free",
                "source": "Free",
                "limits": limits_view(&free.limits),
                "addons": [],
            })
        }
    };
    let now = now_ms();
    let live: Vec<&MembershipContract> = terms
        .contracts
        .iter()
        .filter(|c| contract_end(c) > now)
        .collect();
    let current = in_force(&live, now);
    let next = queued(&live, now);
    status["current"] = current.map_or(Json::Null, contract_view);
    status["next"] = next.map_or(Json::Null, contract_view);
    status["offers"] = offers(&catalog, current, next, now)?.into();
    // Storage packs extend a paid membership that qualifies now, to its end.
    status["storage_available"] = (current.is_some_and(|c| c.terminated_at_ms.is_none())
        && status["eligibility"] == "Eligible")
        .into();
    // Only a term running to its end is continued; one renewal at a time.
    status["renew_from_ms"] = match (current, next) {
        (Some(c), None) if c.terminated_at_ms.is_none() => {
            c.expires_at_ms.saturating_sub(RENEWAL_WINDOW_MS).into()
        }
        _ => Json::Null,
    };
    status["refund"] = terms.refund.map_or(Json::Null, |w| {
        json!({
            "order": hex::encode(*w.order_id),
            "contract": hex::encode(*w.contract_id),
            "until_ms": w.until_ms,
        })
    });
    if refresh {
        status["synced"] = relay_refresh(h).await.is_ok().into();
    }
    Ok(status)
}

/// The account's base plan from its certified entitlement; Free without a
/// record. Unlike [`plan_status`] this reads no catalog or terms, so a caller
/// that only needs the tier does not wait for the whole plan page.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for an uncertified answer.
pub async fn plan<H: Hold>(h: &H) -> Result<PlanId> {
    let beneficiary = h.with(|c| c.beneficiary()).await?;
    let commerce = services(h).await?.commerce;
    let view: Option<EntitlementView> = certified_by(
        &h.anonymous().await?,
        commerce,
        "get_entitlement_batch",
        (vec![&beneficiary],),
        billing::entitlement_key(&beneficiary).as_slice(),
    )
    .await?;
    Ok(view.map_or(PlanId::Free, |v| v.plan_snapshot.plan_id))
}

/// Ask the relay to read this account's entitlement again, so its storage
/// and channel limits follow a purchase or refund now rather than within
/// the hour.
///
/// # Errors
/// Relay failures.
pub async fn relay_refresh<H: Hold>(h: &H) -> Result<()> {
    let base = h.with(|c| c.base()).await?;
    let path = format!("{base}/membership/refresh");
    h.send(|c| c.signed_post_empty(&path)).await.map(|_| ())
}

/// Acknowledge the relay's over-limit notice `id`; the export window it
/// guarantees starts now at the latest.
///
/// # Errors
/// Relay failures; `Error::VersionConflict` for a notice no longer open.
pub async fn notice_ack<H: Hold>(h: &H, id: &str) -> Result<Json> {
    ensure(
        id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()),
        dmsg_types::invalid("notice"),
    )?;
    let base = h.with(|c| c.base()).await?;
    h.post(
        &format!("{base}/membership/notice-ack"),
        "dmsg/membership/notice-ack/v1",
        &json!({"notice_id": id.to_ascii_lowercase()}),
        &Hash::new(crate::crypto::random_id()),
    )
    .await
}

/// The order `id`'s state at the service: checked against this device's
/// journal when it has one, otherwise read as the payer's cash order.
///
/// # Errors
/// Returns `Error::NotFound` before the service has the order.
pub(crate) async fn order<H: Hold>(h: &H, id: &str) -> Result<Order> {
    let job = h.with(|c| Ok(c.checkout(id).ok())).await?;
    let services = services(h).await?;
    let (payer, ic) = payer(h).await?;
    let raw = hex_id(id)?;
    if let Some(job) = &job {
        ensure(job.actor == payer, Error::Forbidden)?;
    }
    match job.as_ref().map(|j| &j.terms) {
        Some(Terms::Panda(terms)) => {
            // The certificate is the normal read. A stale or incompletely
            // migrated leaf must not hide the live claim from its actor.
            let certified: Result<Option<PandaClaimView>> = certified_by(
                &ic,
                services.membership,
                "panda_claim_certificate",
                (raw,),
                digest("dmsg/panda/claim-certificate/v2", &raw).as_slice(),
            )
            .await;
            let view = match certified {
                Ok(Some(view)) => view,
                Ok(None) | Err(Error::IntegrityFailed) => {
                    let direct: Result<PandaClaimView> = ic
                        .query(services.membership, "get_panda_claim", (raw,))
                        .await?;
                    direct?
                }
                Err(error) => return Err(error),
            };
            ensure(view.terms == **terms, Error::IntegrityFailed)?;
            Ok(Order::Panda(Box::new(view)))
        }
        quoted => {
            let view: Result<CheckoutView> =
                ic.query(services.commerce, "get_checkout", (raw,)).await?;
            let view = view?;
            if let Some(Terms::Cash(quote)) = quoted {
                ensure(view.quote == **quote, Error::IntegrityFailed)?;
            }
            Ok(Order::Cash(Box::new(view)))
        }
    }
}

/// The order `id`'s state for display, read without the client.
///
/// # Errors
/// As [`order`].
pub async fn plan_order_view<H: Hold>(h: &H, id: &str) -> Result<Json> {
    Ok(order_view(&order(h, id).await?))
}

/// Whether this device quoted `id` as a PANDA claim.
async fn is_panda<H: Hold>(h: &H, id: &str) -> Result<bool> {
    h.with(|c| {
        Ok(c.checkout(id)
            .is_ok_and(|job| matches!(job.terms, Terms::Panda(_))))
    })
    .await
}

/// Cancel an order: one not yet paid or applied, a renewal that has not
/// started, or the first purchase within its refund window, whose rights
/// stop once the lease already issued runs out. What becomes refundable is
/// claimed back to the payer and sent at once.
///
/// # Errors
/// The service's verdict; a started term otherwise is `Error::Forbidden`.
pub async fn plan_cancel<H: Hold>(h: &H, id: &str) -> Result<Json> {
    let services = services(h).await?;
    let (payer, ic) = payer(h).await?;
    let raw = hex_id(id)?;
    let mut refunds = Vec::new();
    if is_panda(h, id).await? {
        let cancelled: Result<PandaClaimView> = ic
            .update(services.membership, "cancel_panda_application", (raw,))
            .await?;
        cancelled?;
    } else {
        let cancelled: Result<CheckoutProgress> = ic
            .update(services.commerce, "cancel_checkout", (raw,))
            .await?;
        cancelled?;
        refunds = refund_deposits(&ic, services.commerce, raw, payer, None).await?;
    }
    let mut view = plan_order_view(h, id).await?;
    view["refunds"] = refunds.into();
    // The relay follows the shorter entitlement now; it rereads within the
    // hour otherwise.
    let _ = relay_refresh(h).await;
    Ok(view)
}

/// What cancelling the first purchase `order` would refund now: the price
/// it has not earned when its rights stop, at the end of the lease already
/// issued, less the refund transfer's network fee.
///
/// # Errors
/// Returns `Error::Forbidden` when the order is not the refundable one.
pub async fn plan_refund_quote<H: Hold>(h: &H, order: &str) -> Result<Json> {
    let status = plan_status(h, false).await?;
    ensure(
        status["refund"]["order"].as_str() == Some(order),
        Error::Forbidden,
    )?;
    let services = services(h).await?;
    let (_, ic) = payer(h).await?;
    let view: Result<CheckoutView> = ic
        .query(services.commerce, "get_checkout", (hex_id(order)?,))
        .await?;
    let view = view?;
    let applied = view.receipt.as_ref().ok_or(Error::NotFound)?.applied_at_ms;
    let stop = status["lease_until_ms"]
        .as_u64()
        .unwrap_or_default()
        .max(now_ms());
    let amount = view.quote.cash.amount_atomic;
    let refund = amount - commerce_v2::earned_atomic(amount, &view.quote.offer, applied, stop)?;
    let fee = view.quote.asset.network_fee_atomic;
    Ok(json!({
        "order": order,
        "refund": refund.to_string(),
        "received": refund.saturating_sub(fee).to_string(),
        "fee": fee.to_string(),
        "asset": view.quote.asset.asset.to_string(),
        "decimals": view.quote.asset.decimals,
        "stops_at_ms": stop,
        "until_ms": status["refund"]["until_ms"],
    }))
}

/// Claim back the deposits to order `raw` at `blocks`, or every refundable
/// one the payer made, at most 32 per refund on one ledger, and send each
/// refund to its source. A refund the service refuses, such as one smaller
/// than the network fee, is reported in its place.
async fn refund_deposits(
    ic: &Ic,
    commerce: Principal,
    raw: Hash,
    payer: Principal,
    blocks: Option<&[u64]>,
) -> Result<Vec<Json>> {
    let mut selected: std::collections::BTreeMap<Principal, Vec<u128>> = Default::default();
    let mut after = None;
    loop {
        let page: Result<CheckoutDepositsPage> = ic
            .query(commerce, "checkout_deposits", (raw, after, 128u16))
            .await?;
        let page = page?;
        for d in page.deposits {
            let chosen = match blocks {
                Some(blocks) => blocks.iter().any(|b| u128::from(*b) == d.block.block_index),
                None => d.from.owner == payer && d.refundable_atomic > 0,
            };
            if chosen {
                selected
                    .entry(d.block.ledger)
                    .or_default()
                    .push(d.block.block_index);
            }
        }
        match page.next {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    let mut refunds = Vec::new();
    for (ledger, mut blocks) in selected {
        blocks.sort_unstable();
        blocks.dedup();
        for group in blocks.chunks(32) {
            let operation = digest("dmsg/refund-selection/v2", &(raw, ledger, group));
            let claimed: Result<CashTransfer> = ic
                .update(
                    commerce,
                    "claim_checkout_refund",
                    (raw, ledger, group.to_vec(), operation),
                )
                .await?;
            let transfer = match claimed {
                Ok(transfer) => transfer,
                Err(error) => {
                    refunds.push(json!({
                        "ledger": ledger.to_text(),
                        "error": crate::error_code(&error),
                    }));
                    continue;
                }
            };
            let sent: Result<Result<CashTransferProgress>> = ic
                .update(
                    commerce,
                    "process_checkout_transfer",
                    (transfer.transfer_id,),
                )
                .await;
            refunds.push(json!({
                "transfer_id": hex::encode(*transfer.transfer_id),
                "ledger": ledger.to_text(),
                "amount": transfer.amount_atomic.to_string(),
                "status": match sent.and_then(|r| r) {
                    Ok(progress) => format!("{:?}", progress.status),
                    Err(error) => crate::error_code(&error),
                },
            }));
        }
    }
    Ok(refunds)
}

/// Refund the deposits to order `id` the service refunds: those at
/// `blocks`, or every refundable one the payer made, each sent back to its
/// source.
///
/// # Errors
/// The service's verdict.
pub async fn plan_refund<H: Hold>(h: &H, id: &str, blocks: &[u64]) -> Result<Json> {
    ensure_valid(blocks.len() <= 32, "blocks")?;
    let services = services(h).await?;
    let (payer, ic) = payer(h).await?;
    let selection = (!blocks.is_empty()).then_some(blocks);
    Ok(Json::Array(
        refund_deposits(&ic, services.commerce, hex_id(id)?, payer, selection).await?,
    ))
}

/// Advance an order whose delivery is being applied.
///
/// # Errors
/// The service's verdict.
pub async fn plan_reconcile<H: Hold>(h: &H, id: &str) -> Result<Json> {
    let services = services(h).await?;
    let (_, ic) = payer(h).await?;
    let raw = hex_id(id)?;
    if is_panda(h, id).await? {
        let view: Result<PandaClaimView> = ic
            .update(services.membership, "reconcile_panda_claim", (raw,))
            .await?;
        // The update already carries the trusted view. Do not turn a stale
        // certificate into a failed reconcile after the claim settled.
        return Ok(order_view(&Order::Panda(Box::new(view?))));
    }
    let r: Result<CheckoutProgress> = ic
        .update(services.commerce, "reconcile_checkout", (raw,))
        .await?;
    r?;
    plan_order_view(h, id).await
}

#[derive(candid::CandidType, Deserialize)]
struct NeuronId {
    id: serde_bytes::ByteBuf,
}

#[derive(candid::CandidType, Deserialize)]
struct ListNeurons {
    of_principal: Option<Principal>,
    limit: u32,
    start_page_at: Option<NeuronId>,
}

#[derive(candid::CandidType, Deserialize)]
enum DissolveState {
    DissolveDelaySeconds(u64),
    WhenDissolvedTimestampSeconds(u64),
}

/// What the client reads of an SNS neuron; the SNS sends more fields.
#[derive(candid::CandidType, Deserialize)]
struct Neuron {
    id: Option<NeuronId>,
    cached_neuron_stake_e8s: u64,
    neuron_fees_e8s: u64,
    dissolve_state: Option<DissolveState>,
}

#[derive(candid::CandidType, Deserialize)]
struct ListNeuronsResponse {
    neurons: Vec<Neuron>,
}

/// The PANDA path before a quote: the SNS whose neurons qualify, the
/// current rate and what each plan needs staked, and the payer's neurons on
/// that SNS with the stake and the lock that count.
/// The payer must be a principal of the neuron, for example a hotkey.
///
/// # Errors
/// Returns `Error::AuthRequired` without a payer.
pub async fn plan_panda<H: Hold>(h: &H) -> Result<Json> {
    let services = services(h).await?;
    let ic = h.anonymous().await?;
    let (service, catalog, (payer, _)): (PandaServiceView, Catalog, _) = futures_util::try_join!(
        ic.query(services.membership, "panda_service", ()),
        catalog_by(&ic, services.commerce),
        payer(h)
    )?;
    let now = now_ms();
    let rate = service
        .rates
        .iter()
        .filter(|r| r.effective_at_ms <= now && r.product_ids.iter().any(|p| p == DMSG_APP))
        .max_by_key(|r| r.effective_at_ms);
    let plans = catalog
        .plans
        .iter()
        .filter(|p| p.price_cents > 0)
        .map(|p| {
            let required = rate
                .map(|r| {
                    integration::required_panda_stake(
                        u128::from(p.price_cents) * 10_000,
                        r.r_num,
                        r.r_den,
                    )
                })
                .transpose()?;
            Ok(json!({
                "plan_id": format!("{:?}", p.plan_id),
                "required_stake_e8s": required.map(|r| r.to_string()),
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    let listed: Result<ListNeuronsResponse> = ic
        .query(
            service.governance,
            "list_neurons",
            (ListNeurons {
                of_principal: Some(payer),
                limit: 100,
                start_page_at: None,
            },),
        )
        .await;
    let now = now_ms();
    let mut view = json!({
        "governance": service.governance.to_text(),
        "ledger": service.panda_ledger.to_text(),
        "paused": service.paused,
        "rate": rate.map(|r| json!({
            "policy_version": r.policy_version,
            "r_num": r.r_num.to_string(),
            "r_den": r.r_den.to_string(),
            "effective_at_ms": r.effective_at_ms,
        })),
        "plans": plans,
        "payer": payer.to_text(),
    });
    match listed {
        Ok(listed) => {
            let neurons: Vec<Json> = listed
                .neurons
                .iter()
                .filter_map(|n| {
                    let id = n.id.as_ref()?;
                    // A neuron that is not dissolving unlocks no earlier than its delay from now.
                    let (dissolving, unlock_at_ms, dissolve_delay_seconds) = match n.dissolve_state
                    {
                        Some(DissolveState::DissolveDelaySeconds(delay)) => (
                            false,
                            now.saturating_add(delay.saturating_mul(1000)),
                            Some(delay),
                        ),
                        Some(DissolveState::WhenDissolvedTimestampSeconds(at)) => {
                            (true, at.saturating_mul(1000), None)
                        }
                        None => (true, now, None),
                    };
                    Some(json!({
                        "id": hex::encode(&id.id),
                        "stake_e8s": n
                            .cached_neuron_stake_e8s
                            .saturating_sub(n.neuron_fees_e8s)
                            .to_string(),
                        "dissolving": dissolving,
                        "unlock_at_ms": unlock_at_ms,
                        "dissolve_delay_seconds": dissolve_delay_seconds,
                    }))
                })
                .collect();
            view["neurons"] = neurons.into();
        }
        Err(error) => view["neurons_error"] = crate::error_code(&error).into(),
    }
    Ok(view)
}

/// One page of `records` visible to the payer, after `after`: as payer,
/// merchant, adapter or operator. Read without the client.
///
/// # Errors
/// The service's verdict.
pub async fn commerce_records<H: Hold>(
    h: &H,
    records: Records,
    after: Option<Hash>,
) -> Result<Json> {
    let services = services(h).await?;
    let (caller, ic) = payer(h).await?;
    let units = |n: u128| n.to_string();
    Ok(match records {
        Records::Orders => {
            let page: Result<CheckoutOperationsPage> = ic
                .query(services.commerce, "checkout_operations", (after, 16u16))
                .await?;
            let page = page?;
            json!({
                "orders": page.orders.iter().map(|row| json!({
                    "order_id": hex::encode(*row.order.progress.order_id),
                    "product_id": row.order.quote.offer.product_id,
                    "sku": row.order.quote.offer.sku,
                    "status": format!("{:?}", row.order.progress.status),
                    "receipt": row.order.receipt.is_some(),
                    "asset": row.order.quote.asset.asset.to_string(),
                    "decimals": row.order.quote.asset.decimals,
                    "balances": row.balances.iter().map(|b| json!({
                        "ledger": b.ledger.to_text(),
                        "incoming": units(b.incoming_atomic),
                        "refundable": units(b.refundable_atomic),
                        "service_reserve": units(b.service_reserve_atomic),
                        "fee_reserve": units(b.fee_reserve_atomic),
                        "outgoing": units(b.outgoing_atomic),
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "next": page.next.map(|h| hex::encode(*h)),
            })
        }
        Records::Transfers => {
            let page: Result<CashTransfersPage> = ic
                .query(services.commerce, "checkout_transfers", (after, 16u16))
                .await?;
            let page = page?;
            json!({
                "transfers": page.transfers.iter().map(|t| json!({
                    "transfer_id": hex::encode(*t.transfer_id),
                    "order_id": hex::encode(*t.order_id),
                    "ledger": t.ledger.to_text(),
                    "to": t.to.owner.to_text(),
                    "amount": units(t.amount_atomic),
                    "fee": units(t.fee_atomic),
                    "max_fee": units(t.max_fee_atomic),
                    "expected_fee": t.expected_fee_atomic.map(units),
                    "status": format!("{:?}", t.status),
                    "error_code": t.error_code,
                    "block": t.block_index.map(units),
                    // Only the recipient revises a refused transfer, within its fee limit.
                    "revisable": t.status == CashTransferStatus::Rejected
                        && t.to.owner == caller
                        && t.expected_fee_atomic.is_some_and(|f| f <= t.max_fee_atomic),
                })).collect::<Vec<_>>(),
                "next": page.next.map(|h| hex::encode(*h)),
            })
        }
        Records::Claims => {
            let page: Result<PandaOperationsPage> = ic
                .query(services.membership, "panda_operations", (after, 16u16))
                .await?;
            let page = page?;
            json!({
                "claims": page.claims.iter().map(|c| json!({
                    "claim_id": hex::encode(*c.claim_id),
                    "product_id": c.terms.offer.product_id,
                    "sku": c.terms.offer.sku,
                    "status": format!("{:?}", c.status),
                    "eligibility": format!("{:?}", c.eligibility),
                    "observed_at_ms": c.observed_at_ms,
                    "valid_until_ms": c.valid_until_ms,
                    "committed_until_ms": c.committed_until_ms,
                    "amount_usd_micros": units(c.terms.offer.amount_usd_micros),
                    // A live claim refreshes its lease; others reconcile.
                    "refreshable": matches!(c.status, PandaClaimStatus::Active | PandaClaimStatus::Terminated),
                })).collect::<Vec<_>>(),
                "next": page.next.map(|h| hex::encode(*h)),
            })
        }
    })
}

/// Read or replay an order's original decision after a lost reply.
///
/// # Errors
/// The service's verdict.
pub async fn commerce_reconcile_order<H: Hold>(h: &H, id: &Hash) -> Result<Json> {
    let services = services(h).await?;
    let (_, ic) = payer(h).await?;
    let progress: Result<CheckoutProgress> = ic
        .update(services.commerce, "reconcile_checkout", (id,))
        .await?;
    let progress = progress?;
    Ok(
        json!({"order_id": hex::encode(*progress.order_id), "status": format!("{:?}", progress.status)}),
    )
}

/// Send a transfer again with its original memo, time, amount and fee.
///
/// # Errors
/// The service's verdict.
pub async fn commerce_process_transfer<H: Hold>(h: &H, id: &Hash) -> Result<Json> {
    let services = services(h).await?;
    let (_, ic) = payer(h).await?;
    let progress: Result<CashTransferProgress> = ic
        .update(services.commerce, "process_checkout_transfer", (id,))
        .await?;
    let progress = progress?;
    Ok(json!({
        "transfer_id": hex::encode(*progress.transfer_id),
        "status": format!("{:?}", progress.status),
        "block": progress.block_index.map(|b| b.to_string()),
    }))
}

/// Replace a transfer the ledger refused with one at the fee it asked
/// for, as its recipient; the total obligation does not grow.
///
/// # Errors
/// The service's verdict.
pub async fn commerce_revise_transfer<H: Hold>(h: &H, id: &Hash, fee: u128) -> Result<Json> {
    let services = services(h).await?;
    let (_, ic) = payer(h).await?;
    let revised: Result<CashTransfer> = ic
        .update(services.commerce, "revise_checkout_transfer_fee", (id, fee))
        .await?;
    let revised = revised?;
    Ok(json!({
        "transfer_id": hex::encode(*revised.transfer_id),
        "replaces": revised.replaces.map(|h| hex::encode(*h)),
        "status": format!("{:?}", revised.status),
    }))
}

/// Refresh a live PANDA claim's eligibility, or reconcile any other.
///
/// # Errors
/// The service's verdict.
pub async fn commerce_claim<H: Hold>(h: &H, id: &Hash, refresh: bool) -> Result<Json> {
    let services = services(h).await?;
    let (_, ic) = payer(h).await?;
    let method = match refresh {
        true => "refresh_panda_claim",
        false => "reconcile_panda_claim",
    };
    let view: Result<PandaClaimView> = ic.update(services.membership, method, (id,)).await?;
    let view = view?;
    Ok(json!({
        "claim_id": hex::encode(*view.claim_id),
        "status": format!("{:?}", view.status),
        "eligibility": format!("{:?}", view.eligibility),
        "valid_until_ms": view.valid_until_ms,
    }))
}

/// Release or compact expired PANDA commitments; anyone may.
///
/// # Errors
/// The service's verdict.
pub async fn commerce_sweep<H: Hold>(h: &H) -> Result<u32> {
    let services = services(h).await?;
    let ic = h.reader().await?;
    let swept: Result<u32> = ic
        .update(services.membership, "sweep_panda_commitments", ())
        .await?;
    swept
}

#[cfg(test)]
#[path = "commerce_tests.rs"]
mod tests;

/// Governed StoreKit products and the account-bound purchase token. Prices are
/// obtained from StoreKit, never inferred from the cash catalogue.
pub async fn app_store_products<H: Hold>(h: &H) -> Result<Json> {
    let beneficiary = h.with(|c| c.beneficiary()).await?;
    let commerce = services(h).await?.commerce;
    let ic = h.anonymous().await?;
    let configured: Option<dmsg_types::app_store::AppStoreConfig> =
        ic.query(commerce, "get_app_store_config", ()).await?;
    let account = dmsg_protocol::billing::beneficiary_account(&beneficiary)?;
    let token = dmsg_protocol::app_store::account_token(beneficiary.authority_canister, &account);
    Ok(
        json!({"configuration":configured,"account_token":token,"home":beneficiary.authority_canister.to_text(),"account":account.to_string()}),
    )
}

/// Recheck the supported plan transition before presenting Apple's payment UI.
/// This is a preflight, not proof of a purchase or a durable entitlement.
/// # Errors
/// Unconfigured product, stale certification, or unavailable commerce.
pub async fn app_store_purchase_check<H: Hold>(h: &H, product: &str) -> Result<Json> {
    let commerce = services(h).await?.commerce;
    let ic = h.anonymous().await?;
    let config: Option<dmsg_types::app_store::AppStoreConfig> =
        ic.query(commerce, "get_app_store_config", ()).await?;
    let config = config.ok_or(Error::UnsupportedProtocol)?;
    let kind = &config
        .products
        .iter()
        .find(|p| p.product_id == product)
        .ok_or(Error::UnsupportedProtocol)?
        .kind;
    let status = plan_status(h, true).await?;
    let reason = app_store_purchase_reason(kind, &status);
    Ok(
        json!({"eligible":reason.is_none(),"reason":reason,"environment":config.environment,"kind":if matches!(kind,dmsg_types::app_store::StoreProductKind::Plan(_)){"subscription"}else{"storage"}}),
    )
}

fn app_store_purchase_reason(
    kind: &dmsg_types::app_store::StoreProductKind,
    status: &Json,
) -> Option<&'static str> {
    if status["source"] == "Unverifiable" {
        return Some("unavailable");
    }
    let current = &status["current"];
    if !current.is_null() && current["method"] != "AppStore" {
        return Some("external_plan");
    }
    if matches!(kind, dmsg_types::app_store::StoreProductKind::Storage(_))
        && (current.is_null() || status["source"] != "Active")
    {
        return Some("subscription_required");
    }
    None
}

#[cfg(test)]
mod app_store_purchase_tests {
    use super::*;
    #[test]
    fn no_payment_ui_for_unsupported_source_or_storage_without_a_live_base() {
        use dmsg_types::app_store::StoreProductKind::{Plan, Storage};
        let plan = Plan(PlanId::Plus);
        let storage = Storage(Hash::new([1; 32]));
        let free = json!({"source":"Free","current":null});
        assert_eq!(app_store_purchase_reason(&plan, &free), None);
        assert_eq!(
            app_store_purchase_reason(&storage, &free),
            Some("subscription_required")
        );
        assert_eq!(
            app_store_purchase_reason(
                &plan,
                &json!({"source":"Active","current":{"method":"Cash"}})
            ),
            Some("external_plan")
        );
        assert_eq!(
            app_store_purchase_reason(
                &storage,
                &json!({"source":"Active","current":{"method":"AppStore"}})
            ),
            None
        );
        assert_eq!(
            app_store_purchase_reason(
                &plan,
                &json!({"source":"Unverifiable","current":{"method":"AppStore"}})
            ),
            Some("unavailable")
        );
    }
}
