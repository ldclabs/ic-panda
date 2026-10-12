//! Plans and membership (`docs/protocol/commerce_zh.md`): the certified
//! catalog and this account's entitlement, cash checkouts and PANDA
//! membership claims for the account's own subscription.
//!
//! A checkout is a journal fixed at its quote: the device approves exactly
//! those terms for the wallet that pays (`approve_application`), commerce
//! opens the order, and the wallet funds it once. Every step is reconciled
//! by the order's ID after a lost reply; a definite canister rejection is
//! the only reason to quote again. dMsg's own checkouts use the
//! application origin `https://dmsg.net` (plan D-19).

use crate::client::Client;
use crate::codec::decode;
use crate::crypto::sign;
use crate::ic::Ic;
use crate::now_ms;
use crate::payment::certified_leaf;
use crate::upload::hex_id;
use crate::wallet::{memo, transfer_call};
use candid::Principal;
use dmsg_protocol::{
    approval_message, billing, commerce_v2, digest,
    integration::{
        validate_app, validate_application_approval, validate_billing_offer, validate_product,
    },
};
use dmsg_types::{
    billing::{Catalog, EntitlementView},
    ensure, ensure_valid,
    integration::{
        AppRegistration, ApplicationApproval, ApprovalPurpose, BillingOffer, ProductRegistration,
        SettlementMethod,
    },
    integration_billing::{
        CashBlock, CashTransfer, CashTransferProgress, CashTransferStatus, CashTransfersPage,
        CheckoutOperationsPage, CheckoutProgress, CheckoutQuote, CheckoutRequest, CheckoutStatus,
        CheckoutView, OpenCheckout, ProductAuthorizationRequest, SettlementAssetView,
    },
    integration_membership::{
        PandaApplicationTerms, PandaClaimRequest, PandaClaimStatus, PandaClaimView,
        PandaOperationsPage,
    },
    membership::Beneficiary,
    user::{Capability, UserInit},
    Approval, CertifiedBatch, Ed25519Signature, Error, Hash, Result,
};
use icrc_ledger_types::icrc1::{account::Account, transfer::TransferArg};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};

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
    /// The paying or neuron-controlling wallet.
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
}

impl Checkout {
    /// The commitment a person approves: the exact terms and wallet.
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
                "asset": format!("{:?}", q.asset.asset),
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
            "terms": terms,
            "stage": self.stage,
            "digest": hex::encode(*self.digest()),
        })
    }
}

/// The canisters checkouts use.
struct Services {
    home: Principal,
    commerce: Principal,
    membership: Principal,
}

fn status_name(view: &CheckoutView) -> &'static str {
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
    match state {
        Order::Cash(view) => json!({
            "id": hex::encode(*view.progress.order_id),
            "method": "Cash",
            "status": status_name(view),
            "receipt": view.receipt.is_some(),
        }),
        Order::Panda(view) => json!({
            "id": hex::encode(*view.claim_id),
            "method": "Panda",
            "status": claim_name(view),
            "eligibility": format!("{:?}", view.eligibility),
            "cooling_until_ms": view.cooling_until_ms,
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
        let init: UserInit = self.home_config().await?;
        Ok(Services {
            home: self.account()?.home,
            commerce: init.commerce_canister,
            membership: init.membership_canister,
        })
    }

    fn beneficiary(&self) -> Result<Beneficiary> {
        let account = self.account()?;
        Ok(billing::beneficiary(account.home, &account.id))
    }

    /// A certified leaf of a commerce or membership batch, decoded; read
    /// anonymously unless `ic` is given.
    async fn certified<
        T: serde::de::DeserializeOwned + Serialize,
        A: candid::utils::ArgumentEncoder,
    >(
        &mut self,
        ic: Option<Ic>,
        canister: Principal,
        method: &str,
        args: A,
        key: &[u8],
    ) -> Result<Option<T>> {
        let ic = match ic {
            Some(ic) => ic,
            None => self.anonymous().await?,
        };
        let batch: Result<CertifiedBatch> = ic.query(canister, method, args).await?;
        let batch = batch?;
        certified_leaf(&batch, key, canister, &ic.root_key(), now_ms())?
            .map(|value| decode(&value, 1 << 20))
            .transpose()
    }

    /// The certified catalog.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for an uncertified answer.
    pub async fn plan_catalog(&mut self) -> Result<Catalog> {
        let services = self.services().await?;
        self.certified(
            None,
            services.commerce,
            "get_catalog",
            (),
            billing::catalog_key().as_slice(),
        )
        .await?
        .ok_or(Error::IntegrityFailed)
    }

    /// The settlement assets checkouts may pay with.
    ///
    /// # Errors
    /// Query failures return `Error::Unavailable`.
    pub async fn plan_assets(&mut self) -> Result<Json> {
        let services = self.services().await?;
        let assets: Vec<SettlementAssetView> = self
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
                        "asset": format!("{:?}", a.policy.asset),
                        "decimals": a.policy.decimals,
                        "network_fee": a.policy.network_fee_atomic.to_string(),
                    })
                })
                .collect(),
        ))
    }

    /// This account's entitlement, refreshed first with `refresh`; a
    /// certified absence means the catalog's Free plan.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for an uncertified answer.
    pub async fn plan_status(&mut self, refresh: bool) -> Result<Json> {
        let services = self.services().await?;
        let beneficiary = self.beneficiary()?;
        if refresh {
            let ic = self.anonymous().await?;
            let refreshed: Result<Result<EntitlementView>> = ic
                .update(services.commerce, "refresh_entitlement", (&beneficiary,))
                .await;
            match refreshed {
                Ok(Ok(_)) | Ok(Err(Error::NotFound)) => {}
                Ok(Err(error)) => return Err(error),
                Err(error) => return Err(error),
            }
        }
        let view: Option<EntitlementView> = self
            .certified(
                None,
                services.commerce,
                "get_entitlement_batch",
                (vec![&beneficiary],),
                billing::entitlement_key(&beneficiary).as_slice(),
            )
            .await?;
        Ok(match view {
            Some(view) => json!({
                "plan": format!("{:?}", view.plan_snapshot.plan_id),
                "source": format!("{:?}", view.source_status),
                "eligibility": format!("{:?}", view.eligibility_status),
                "valid_until_ms": view.valid_until_ms,
                "storage_bytes": view.effective_limits.storage_bytes,
                "effective_stop_at_ms": view.effective_stop_at_ms,
            }),
            None => {
                let catalog = self.plan_catalog().await?;
                let free = catalog
                    .plans
                    .iter()
                    .find(|p| p.plan_id == dmsg_types::billing::PlanId::Free)
                    .ok_or(Error::IntegrityFailed)?;
                json!({
                    "plan": "Free",
                    "source": "Free",
                    "storage_bytes": free.limits.storage_bytes,
                })
            }
        })
    }

    /// The certified registrations of dMsg's own application and product.
    async fn registrations(
        &mut self,
        commerce: Principal,
        product: &str,
    ) -> Result<(AppRegistration, ProductRegistration)> {
        let ic = self.anonymous().await?;
        let batch: Result<CertifiedBatch> = ic
            .query(
                commerce,
                "integration_configuration_certificate",
                (DMSG_APP, Some(product)),
            )
            .await?;
        let batch = batch?;
        let root = ic.root_key();
        let now = now_ms();
        let app_key = digest("dmsg/registration/app/v1", &DMSG_APP).to_vec();
        let product_key = digest("dmsg/registration/product/v2", &product).to_vec();
        let app: AppRegistration = decode(
            &certified_leaf(&batch, &app_key, commerce, &root, now)?.ok_or(Error::NotFound)?,
            1 << 20,
        )?;
        let registered: ProductRegistration = decode(
            &certified_leaf(&batch, &product_key, commerce, &root, now)?.ok_or(Error::NotFound)?,
            1 << 20,
        )?;
        validate_app(&app)?;
        validate_product(&registered)?;
        ensure(
            app.app_id == DMSG_APP
                && app.origins.iter().any(|o| o == DMSG_ORIGIN)
                && app.environment == self.config.environment,
            Error::IntegrityFailed,
        )?;
        Ok((app, registered))
    }

    /// The wallet's agent for a checkout `actor`.
    async fn actor(&mut self, actor: Principal) -> Result<Ic> {
        let (payer, ic) = self.wallet().await?;
        ensure(payer == actor, Error::Forbidden)?;
        Ok(ic)
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
    /// neuron `selection` (hex), paid or controlled by the wallet. Returns
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
        let services = self.services().await?;
        let state = self.cloud_state().await?;
        ensure(
            state
                .active(&self.workspace.meta.device_id)?
                .input
                .capabilities
                .contains(&Capability::FormalApprove),
            Error::Forbidden,
        )?;
        let (actor, ic) = self.wallet().await?;
        let beneficiary = self.beneficiary()?;
        let operation = Hash::new(crate::crypto::random_id());
        let offer: Result<BillingOffer> = ic
            .update(
                services.commerce,
                "prepare_account_subscription",
                (DMSG_APP, &beneficiary, sku, operation),
            )
            .await?;
        let request = CheckoutRequest {
            offer: offer?,
            approving_account: self.account()?.id,
            product_approval: None,
            method,
        };
        self.quote(request, DMSG_ORIGIN, selection, actor, &ic, &services)
            .await
    }

    /// Fix the terms of a web application's checkout from `origin` for the
    /// wallet: cash on the ledger `selection`, or a PANDA waiver backed by
    /// the neuron `selection` (hex).
    pub(crate) async fn quote_request(
        &mut self,
        request: CheckoutRequest,
        origin: &str,
        selection: &str,
    ) -> Result<Checkout> {
        let services = self.services().await?;
        let (actor, ic) = self.wallet().await?;
        self.quote(request, origin, selection, actor, &ic, &services)
            .await
    }

    /// Fix the terms of `request` for `actor`.
    async fn quote(
        &mut self,
        request: CheckoutRequest,
        origin: &str,
        selection: &str,
        actor: Principal,
        ic: &Ic,
        services: &Services,
    ) -> Result<Checkout> {
        ensure(
            request.approving_account == self.account()?.id,
            Error::InvalidInput("ACCOUNT_MISMATCH".into()),
        )?;
        let (app, product) = self
            .registrations(services.commerce, &request.offer.product_id)
            .await?;
        let now = now_ms();
        validate_billing_offer(&request.offer, &app, &product, now)?;
        ensure(
            request
                .offer
                .allowed_settlement_methods
                .contains(&request.method)
                && app.origins.iter().any(|o| o == origin),
            Error::Forbidden,
        )?;
        let terms = match request.method {
            SettlementMethod::Cash => {
                let ledger =
                    Principal::from_text(selection).map_err(|_| dmsg_types::invalid("ledger"))?;
                let payer = Account {
                    owner: actor,
                    subaccount: None,
                };
                let quote: Result<CheckoutQuote> = ic
                    .update(
                        services.commerce,
                        "quote_checkout",
                        (&request.offer, ledger, payer),
                    )
                    .await?;
                let quote = quote?;
                let assets: Vec<SettlementAssetView> = self
                    .anonymous()
                    .await?
                    .query(services.commerce, "settlement_assets", ())
                    .await?;
                ensure(
                    assets
                        .iter()
                        .any(|a| a.ledger_verified && a.policy == quote.asset),
                    Error::PolicyStale,
                )?;
                // The service's terms must be exactly those it would compute.
                let expected = commerce_v2::checkout_quote(
                    services.commerce,
                    request.offer.clone(),
                    &app,
                    &product,
                    quote.asset.clone(),
                    payer,
                    quote.quoted_at_ms,
                )?;
                ensure(expected == quote, Error::IntegrityFailed)?;
                Terms::Cash(Box::new(quote))
            }
            SettlementMethod::Panda => {
                let neuron = hex_id(selection).map_err(|_| dmsg_types::invalid("neuron"))?;
                let terms: Result<PandaApplicationTerms> = ic
                    .update(
                        services.membership,
                        "quote_panda_subscription",
                        (
                            &request.offer,
                            services.home,
                            request.approving_account,
                            neuron,
                        ),
                    )
                    .await?;
                let terms = terms?;
                ensure(
                    terms.actor == actor
                        && terms.neuron_id == neuron
                        && terms.user_home == services.home
                        && terms.approving_account == request.approving_account
                        && terms.offer == request.offer,
                    Error::IntegrityFailed,
                )?;
                commerce_v2::validate_panda_terms(
                    &terms,
                    &app,
                    &product,
                    services.membership,
                    terms.sns_governance,
                    now_ms(),
                    true,
                )?;
                Terms::Panda(Box::new(terms))
            }
        };
        let id = match &terms {
            Terms::Cash(_) => commerce_v2::checkout_id(services.commerce, &request.offer),
            Terms::Panda(terms) => commerce_v2::panda_claim_id(terms),
        };
        let job = Checkout {
            id: hex::encode(*id),
            actor,
            origin: origin.into(),
            request,
            terms,
            approval: None,
            stage: Stage::Review,
        };
        if let Ok(old) = self.checkout(&job.id) {
            ensure(
                old.request == job.request && old.actor == job.actor && old.origin == job.origin,
                Error::IdempotencyConflict,
            )?;
            // Approved terms stay fixed; only an unapproved quote is refreshed.
            if old.stage != Stage::Review {
                return Ok(old);
            }
        }
        self.save_checkout(&job)?;
        Ok(job)
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
        self.workspace
            .store
            .sealed_keys(JOURNAL)?
            .iter()
            .map(|key| {
                self.workspace
                    .private(self.keys()?, key)?
                    .ok_or(Error::IntegrityFailed)
            })
            .collect()
    }

    /// The order's state at the service, checked against the journal.
    ///
    /// # Errors
    /// Returns `Error::NotFound` before the service has the order.
    pub async fn plan_order(&mut self, id: &str) -> Result<Order> {
        let job = self.checkout(id)?;
        let services = self.services().await?;
        let raw = hex_id(id)?;
        match &job.terms {
            Terms::Cash(quote) => {
                let ic = self.actor(job.actor).await?;
                let view: Result<CheckoutView> =
                    ic.query(services.commerce, "get_checkout", (raw,)).await?;
                let view = view?;
                ensure(view.quote == **quote, Error::IntegrityFailed)?;
                Ok(Order::Cash(Box::new(view)))
            }
            Terms::Panda(terms) => {
                // Only the claim's actor reads its certificate.
                let ic = self.actor(job.actor).await?;
                let view: PandaClaimView = self
                    .certified(
                        Some(ic),
                        services.membership,
                        "panda_claim_certificate",
                        (raw,),
                        digest("dmsg/panda/claim-certificate/v2", &raw).as_slice(),
                    )
                    .await?
                    .ok_or(Error::NotFound)?;
                ensure(view.terms == **terms, Error::IntegrityFailed)?;
                Ok(Order::Panda(Box::new(view)))
            }
        }
    }

    /// The order's state for display.
    ///
    /// # Errors
    /// As [`Self::plan_order`].
    pub async fn plan_order_view(&mut self, id: &str) -> Result<Json> {
        let order = self.plan_order(id).await?;
        Ok(order_view(&order))
    }

    /// Sign the device approval of a checkout's exact terms, once; a fresh
    /// one after a PANDA claim's cooling period with `fresh`.
    async fn authorize_checkout(
        &mut self,
        job: &mut Checkout,
        services: &Services,
        fresh: bool,
    ) -> Result<()> {
        if job.approval.is_some() && !fresh {
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
            now < deadline && (fresh || now < job.request.offer.accept_by_ms),
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
        };
        let message = approval_message(
            account.home,
            &account.id,
            APPLICATION,
            &application,
            &approval,
        );
        approval.signature = Ed25519Signature::new(sign(&self.keys()?.signing, message.as_slice()));
        job.approval = Some((application, approval));
        job.stage = Stage::Authorized;
        self.save_checkout(job)
    }

    /// Approve and open a checkout, then fund a cash order from the wallet.
    /// A retry reads the service's state first and repeats only what is
    /// missing; the saved approval is sent again, never a new one.
    ///
    /// # Errors
    /// Returns `Error::ExecutionUnknown` while a step's outcome is unknown.
    pub(crate) async fn plan_execute(&mut self, id: &str) -> Result<Json> {
        let mut job = self.checkout(id)?;
        let services = self.services().await?;
        if job.stage != Stage::Review {
            match self.plan_order(id).await {
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
                requested?;
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

    /// Advance a PANDA claim after its cooling period with a fresh device
    /// approval of the same terms.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a cash order.
    pub(crate) async fn plan_advance(&mut self, id: &str) -> Result<Json> {
        let mut job = self.checkout(id)?;
        let Terms::Panda(_) = &job.terms else {
            return Err(Error::Forbidden);
        };
        let services = self.services().await?;
        if let Order::Panda(view) = self.plan_order(id).await? {
            if view.status != PandaClaimStatus::CoolingDown {
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
        advanced?;
        self.plan_order_view(id).await
    }

    /// Cancel an order before it is applied.
    ///
    /// # Errors
    /// The service's verdict; an applied order is `Error::Forbidden`.
    pub async fn plan_cancel(&mut self, id: &str) -> Result<Json> {
        let job = self.checkout(id)?;
        let services = self.services().await?;
        let ic = self.actor(job.actor).await?;
        let raw = hex_id(id)?;
        match job.terms {
            Terms::Cash(_) => {
                let cancelled: Result<CheckoutProgress> = ic
                    .update(services.commerce, "cancel_checkout", (raw,))
                    .await?;
                cancelled?;
            }
            Terms::Panda(_) => {
                let cancelled: Result<PandaClaimView> = ic
                    .update(services.membership, "cancel_panda_application", (raw,))
                    .await?;
                cancelled?;
            }
        }
        self.plan_order_view(id).await
    }

    /// Advance an order whose delivery is being applied.
    ///
    /// # Errors
    /// The service's verdict.
    pub async fn plan_reconcile(&mut self, id: &str) -> Result<Json> {
        let job = self.checkout(id)?;
        let services = self.services().await?;
        let ic = self.actor(job.actor).await?;
        let raw = hex_id(id)?;
        match job.terms {
            Terms::Cash(_) => {
                let r: Result<CheckoutProgress> = ic
                    .update(services.commerce, "reconcile_checkout", (raw,))
                    .await?;
                r?;
            }
            Terms::Panda(_) => {
                let r: Result<PandaClaimView> = ic
                    .update(services.membership, "reconcile_panda_claim", (raw,))
                    .await?;
                r?;
            }
        }
        self.plan_order_view(id).await
    }

    /// Refund the deposits of a cash order the service refunds, to their
    /// original accounts.
    ///
    /// # Errors
    /// The service's verdict.
    pub async fn plan_refund(&mut self, id: &str, blocks: &[u64]) -> Result<Json> {
        ensure_valid(!blocks.is_empty() && blocks.len() <= 32, "blocks")?;
        let job = self.checkout(id)?;
        let Terms::Cash(quote) = &job.terms else {
            return Err(Error::Forbidden);
        };
        let services = self.services().await?;
        let ic = self.actor(job.actor).await?;
        let mut selected: Vec<u128> = blocks.iter().map(|b| u128::from(*b)).collect();
        selected.sort_unstable();
        selected.dedup();
        let raw = hex_id(id)?;
        let operation = digest(
            "dmsg/refund-selection/v2",
            &(raw, quote.cash.ledger, &selected),
        );
        let transfer: Result<dmsg_types::integration_billing::CashTransfer> = ic
            .update(
                services.commerce,
                "claim_checkout_refund",
                (raw, quote.cash.ledger, selected, operation),
            )
            .await?;
        let transfer = transfer?;
        Ok(json!({
            "transfer_id": hex::encode(*transfer.transfer_id),
            "amount": transfer.amount_atomic.to_string(),
            "status": format!("{:?}", transfer.status),
        }))
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

/// The commerce and membership records the wallet may read and repair: as
/// payer, merchant, adapter or operator. Each action repeats an original
/// operation with its frozen parameters; none creates a new payment.
impl Client {
    /// One page of `records` visible to the wallet, after `after`.
    ///
    /// # Errors
    /// The service's verdict.
    pub async fn commerce_records(
        &mut self,
        records: Records,
        after: Option<Hash>,
    ) -> Result<Json> {
        let services = self.services().await?;
        let (caller, ic) = self.wallet().await?;
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
    pub async fn commerce_reconcile_order(&mut self, id: &Hash) -> Result<Json> {
        let services = self.services().await?;
        let (_, ic) = self.wallet().await?;
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
    pub async fn commerce_process_transfer(&mut self, id: &Hash) -> Result<Json> {
        let services = self.services().await?;
        let (_, ic) = self.wallet().await?;
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
    pub async fn commerce_revise_transfer(&mut self, id: &Hash, fee: u128) -> Result<Json> {
        let services = self.services().await?;
        let (_, ic) = self.wallet().await?;
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
    pub async fn commerce_claim(&mut self, id: &Hash, refresh: bool) -> Result<Json> {
        let services = self.services().await?;
        let (_, ic) = self.wallet().await?;
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
    pub async fn commerce_sweep(&mut self) -> Result<u32> {
        let services = self.services().await?;
        let ic = self.reader().await?;
        let swept: Result<u32> = ic
            .update(services.membership, "sweep_panda_commitments", ())
            .await?;
        swept
    }
}
