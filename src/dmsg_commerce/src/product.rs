//! dMsg account-product adapter for the same v2 checkout/membership services used by other products.
use crate::{
    api::{self, now},
    model::{self, Subject},
    store,
};
use candid::Principal;
use dmsg_protocol::membership::mul_div;
use dmsg_protocol::{
    billing::*,
    commerce_v2::*,
    integration::*,
    product_book::{rejected, ProductBook},
    *,
};
use dmsg_runtime::{
    call,
    storage::{MapExt, Stored},
};
use dmsg_types::{
    billing::*,
    integration::*,
    integration_billing::*,
    integration_membership::*,
    membership::{Beneficiary, Eligibility},
    *,
};
use ic_stable_structures::{memory_manager::VirtualMemory, DefaultMemoryImpl, StableBTreeMap};
use std::cell::RefCell;

type Memory = VirtualMemory<DefaultMemoryImpl>;

thread_local! {
    static BOOKS: RefCell<StableBTreeMap<Vec<u8>, Stored<ProductBook>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(16)));
    // Delivered decisions by contract ID; cancellation finds the book from the offer.
    static DELIVERED: RefCell<StableBTreeMap<Vec<u8>, Stored<ProductDecision>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(17)));
    static RECEIPTS: RefCell<StableBTreeMap<Vec<u8>, Stored<ProductReceipt>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(18)));
    static CANCELLATIONS: RefCell<StableBTreeMap<Vec<u8>, Stored<CashCancellationReceipt>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(19)));
    static RELEASED: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(20)));
}

/// An annual plan, a cash upgrade of the current plan, or a storage add-on by product ID hex.
enum Sku<'a> {
    Plan(PlanId),
    Upgrade(PlanId),
    Storage(&'a str),
}

impl<'a> Sku<'a> {
    fn parse(sku: &'a str) -> Result<Self> {
        if let Some(target) = sku.strip_prefix("upgrade-") {
            Ok(Self::Upgrade(plan_id(target)?))
        } else if sku.len() == 64 {
            Ok(Self::Storage(sku))
        } else {
            Ok(Self::Plan(plan_id(sku)?))
        }
    }
}

fn plan_id(sku: &str) -> Result<PlanId> {
    match sku {
        "plus" => Ok(PlanId::Plus),
        "pro" => Ok(PlanId::Pro),
        "max" => Ok(PlanId::Max),
        _ => Err(Error::UnsupportedProtocol),
    }
}

fn hex(id: Hash) -> String {
    id.as_slice().iter().map(|b| format!("{b:02x}")).collect()
}

fn storage_product<'c>(catalog: &'c Catalog, sku: &str) -> Result<&'c StorageProduct> {
    catalog
        .storage_products
        .iter()
        .find(|p| hex(p.product_id) == sku)
        .ok_or(Error::NotFound)
}

/// Annual USD cents for the rest of a term `[term_start, end)`, in USD micros rounded up.
fn prorate(annual_cents: u64, term_start: u64, end: u64, at: u64) -> Result<u128> {
    mul_div(
        u128::from(annual_cents) * 10_000,
        u128::from(end - at),
        u128::from(end - term_start),
        true,
    )
}

fn base_key(b: &Beneficiary) -> Hash {
    digest("dmsg/product/base/v2", b)
}

fn topic(offer: &BillingOffer) -> Hash {
    if matches!(Sku::parse(&offer.sku), Ok(Sku::Storage(_))) {
        digest(
            "dmsg/product/addon/v2",
            &(&offer.beneficiary, offer.operation_id),
        )
    } else {
        base_key(&offer.beneficiary)
    }
}

/// The offer's product book. Every book of a subject follows the subject's business revision.
fn book(offer: &BillingOffer, s: &Subject) -> ProductBook {
    let mut b = BOOKS
        .with_borrow(|t| t.load(topic(offer).as_slice()))
        .unwrap_or_else(|| ProductBook::new(offer.beneficiary.clone(), 0));
    b.business_revision = s.business_revision;
    b
}

fn save(offer: &BillingOffer, b: &ProductBook) {
    BOOKS.with_borrow_mut(|t| t.put(topic(offer).as_slice(), b));
}

/// A cash upgrade replaces the cash term it overlaps; a PANDA commitment is never retired.
fn retire_upgraded(b: &mut ProductBook, offer: &BillingOffer) {
    if matches!(Sku::parse(&offer.sku), Ok(Sku::Upgrade(_))) {
        b.contracts.retain(|c| {
            !matches!(c.source, SubscriptionSource::Cash { .. })
                || c.offer.starts_at_ms >= offer.starts_at_ms
                || c.offer.expires_at_ms <= offer.starts_at_ms
        });
    }
}

fn receipt(id: Hash) -> Option<ProductReceipt> {
    RECEIPTS.with_borrow(|t| t.load(id.as_slice()))
}

fn put_receipt(r: &ProductReceipt) {
    RECEIPTS.with_borrow_mut(|t| t.put(r.decision_id.as_slice(), r));
}

fn released(offer: &BillingOffer) -> bool {
    RELEASED.with_borrow(|t| t.contains(offer.operation_id.as_slice()))
}

/// A definite rejection frees only this decision's reservation.
fn reject(decision: &ProductDecision, reason: ProductRejection, at: u64) -> ProductReceipt {
    let r = match BOOKS.with_borrow(|t| t.load(topic(&decision.offer).as_slice())) {
        Some(mut b) => {
            let r = b.reject(decision, reason, at);
            save(&decision.offer, &b);
            r
        }
        None => rejected(decision, reason, at),
    };
    put_receipt(&r);
    r
}

fn rejection(e: &Error) -> ProductRejection {
    match e {
        Error::Forbidden => ProductRejection::Unauthorized,
        Error::Expired => ProductRejection::Expired,
        Error::IntervalReserved => ProductRejection::IntervalReserved,
        _ => ProductRejection::RevisionConflict,
    }
}

fn service(method: SettlementMethod) -> Principal {
    match method {
        SettlementMethod::Cash => ic_cdk::api::canister_self(),
        SettlementMethod::Panda => store::config(|c| c.membership_canister),
    }
}

fn settlement(a: &ApplicationApproval) -> SettlementMethod {
    match a.purpose {
        ApprovalPurpose::CashCheckout => SettlementMethod::Cash,
        ApprovalPurpose::PandaSubscription => SettlementMethod::Panda,
    }
}

fn registration(app: &str, product: &str) -> Result<(AppRegistration, ProductRegistration)> {
    let (a, p) = crate::registrations::product_configuration(app, product)?;
    let home = ic_cdk::api::canister_self();
    ensure(
        p.quote_authority == home && p.adapter == home,
        Error::Forbidden,
    )?;
    Ok((a, p))
}

fn offer(
    app_id: String,
    b: Beneficiary,
    sku: String,
    operation_id: Hash,
    at: u64,
) -> Result<BillingOffer> {
    let s = api::get_subject(&b)?;
    validate_identifier(&sku)?;
    nonzero(operation_id.as_slice())?;
    let (app, product) = registration(&app_id, &b.product_id)?;
    let catalog = store::catalog(at);
    ensure(
        product.terms_hash == catalog.terms_digest,
        Error::PolicyStale,
    )?;
    let base = BOOKS.with_borrow(|t| t.load(base_key(&b).as_slice()));
    let current = base.as_ref().and_then(|book| {
        book.contracts.iter().rev().find(|c| {
            c.offer.starts_at_ms <= at
                && at < c.offer.expires_at_ms
                && c.status != SubscriptionStatus::Cancelled
        })
    });
    let (start, end, amount, methods) = match Sku::parse(&sku)? {
        Sku::Upgrade(target) => {
            let current = current.ok_or(Error::NotFound)?;
            ensure(
                matches!(current.source, SubscriptionSource::Cash { .. }),
                Error::Forbidden,
            )?;
            let old = s
                .contracts
                .iter()
                .find(|c| c.contract_id == current.contract_id)
                .ok_or(Error::IntegrityFailed)?;
            let next = model::plan(&catalog, &target)?;
            ensure(next.price_cents > old.plan.price_cents, Error::Forbidden)?;
            let end = current.offer.expires_at_ms;
            (
                at,
                end,
                prorate(
                    next.price_cents - old.plan.price_cents,
                    old.term_starts_at_ms,
                    end,
                    at,
                )?,
                vec![SettlementMethod::Cash],
            )
        }
        Sku::Storage(id) => {
            ensure(
                s.addons.iter().filter(|a| a.expires_at_ms > at).count() < model::MAX_ADDONS,
                Error::QuotaExceeded,
            )?;
            let current = s.active(at).ok_or(Error::MembershipIneligible)?;
            ensure(
                current.eligibility == Eligibility::Eligible && at < current.qualified_until_ms,
                Error::MembershipStale,
            )?;
            let item = storage_product(&catalog, id)?;
            (
                at,
                current.expires_at_ms,
                prorate(
                    item.price_cents,
                    current.term_starts_at_ms,
                    current.expires_at_ms,
                    at,
                )?,
                vec![SettlementMethod::Cash],
            )
        }
        Sku::Plan(id) => {
            let plan = model::plan(&catalog, &id)?;
            let start = if let Some(current) = current {
                ensure(
                    at.saturating_add(30 * DAY) >= current.offer.expires_at_ms,
                    Error::Expired,
                )?;
                current.offer.expires_at_ms
            } else {
                at
            };
            (
                start,
                next_year(start)?,
                u128::from(plan.price_cents) * 10_000,
                vec![SettlementMethod::Cash, SettlementMethod::Panda],
            )
        }
    };
    let home = ic_cdk::api::canister_self();
    let value = BillingOffer {
        version: 2,
        environment: store::config(|c| c.environment.clone()),
        app_id,
        product_id: b.product_id.clone(),
        offer_id: digest(
            "dmsg/product/offer/v2",
            &(&b, &sku, operation_id, s.business_revision, at),
        ),
        beneficiary: b,
        quote_authority: home,
        adapter: home,
        sku,
        product_terms_hash: catalog.terms_digest,
        expected_business_revision: s.business_revision,
        amount_usd_micros: amount,
        starts_at_ms: start,
        expires_at_ms: end,
        issued_at_ms: at,
        accept_by_ms: at.saturating_add(OFFER_TTL_MS).min(end),
        operation_id,
        allowed_settlement_methods: methods,
    };
    validate_billing_offer(&value, &app, &product, at)?;
    Ok(value)
}

#[ic_cdk::update]
fn prepare_account_subscription(
    app_id: String,
    beneficiary: Beneficiary,
    sku: String,
    operation_id: Hash,
) -> Result<BillingOffer> {
    offer(app_id, beneficiary, sku, operation_id, now())
}

#[ic_cdk::update]
fn verify_billing_offer(value: BillingOffer) -> Result<()> {
    ensure(
        [
            ic_cdk::api::canister_self(),
            store::config(|c| c.membership_canister),
        ]
        .contains(&ic_cdk::api::msg_caller()),
        Error::Forbidden,
    )?;
    let at = now();
    ensure(
        at < value.accept_by_ms
            && offer(
                value.app_id.clone(),
                value.beneficiary.clone(),
                value.sku.clone(),
                value.operation_id,
                value.issued_at_ms,
            )? == value,
        Error::VersionConflict,
    )
}

/// The entry has already matched its caller to `service`.
async fn validate_request(
    request: &ProductAuthorizationRequest,
    service: Principal,
    at: u64,
) -> Result<()> {
    let expected = offer(
        request.offer.app_id.clone(),
        request.offer.beneficiary.clone(),
        request.offer.sku.clone(),
        request.offer.operation_id,
        request.offer.issued_at_ms,
    )?;
    ensure(
        expected == request.offer
            && request.offer.issued_at_ms <= at
            && at < request.offer.expires_at_ms,
        Error::VersionConflict,
    )?;
    // The pinned settlement service has already verified the exact device proof.
    // The product adapter cannot consume it: it only rechecks its own beneficiary.
    validate_product_request(request, service, settlement(&request.account_approval), at)?;
    ensure(
        request.user_home == request.offer.beneficiary.authority_canister
            && request.offer.beneficiary.subject_bytes.as_ref()
                == request.account_approval.approving_account.as_slice(),
        Error::Forbidden,
    )?;
    let answer: Result<()> = call(
        request.offer.beneficiary.authority_canister,
        "verify_product_account",
        (
            request.offer.app_id.clone(),
            request.offer.beneficiary.clone(),
        ),
    )
    .await?;
    answer
}

#[ic_cdk::update]
async fn reserve_product_billing(
    request: ProductAuthorizationRequest,
    until_ms: u64,
) -> Result<()> {
    let service = service(settlement(&request.account_approval));
    ensure(ic_cdk::api::msg_caller() == service, Error::Forbidden)?;
    ensure(!released(&request.offer), Error::Forbidden)?;
    let at = now();
    let mut b = book(
        &request.offer,
        &api::get_subject(&request.offer.beneficiary)?,
    );
    if b.reservation
        .as_ref()
        .is_some_and(|r| r.request.offer == request.offer)
    {
        return b.reserve(request, until_ms, at);
    }
    validate_request(&request, service, at).await?;
    let at = now();
    ensure(!released(&request.offer), Error::Forbidden)?;
    let mut b = book(
        &request.offer,
        &api::get_subject(&request.offer.beneficiary)?,
    );
    // Check the interval as it will be after delivery, but keep the live contracts until then.
    let contracts = b.contracts.clone();
    retire_upgraded(&mut b, &request.offer);
    b.reserve(request.clone(), until_ms, at)?;
    b.contracts = contracts;
    save(&request.offer, &b);
    Ok(())
}

#[ic_cdk::update]
fn release_product_billing(request: ProductAuthorizationRequest) -> Result<()> {
    ensure(
        ic_cdk::api::msg_caller() == service(settlement(&request.account_approval)),
        Error::Forbidden,
    )?;
    let mut b = book(
        &request.offer,
        &api::get_subject(&request.offer.beneficiary)?,
    );
    b.release(&request.offer)?;
    let hash = billing_offer_hash(&request.offer);
    if let Some(old) = RELEASED.with_borrow(|t| t.load(request.offer.operation_id.as_slice())) {
        ensure(old == hash, Error::IdempotencyConflict)?;
    }
    RELEASED.with_borrow_mut(|t| t.put(request.offer.operation_id.as_slice(), &hash));
    save(&request.offer, &b);
    Ok(())
}

/// Only a failed call to an authority leaves the decision unknown. Every other failure
/// is a durable rejection, so the settlement service can refund without waiting.
#[ic_cdk::update]
async fn apply_product_decision(decision: ProductDecision) -> Result<ProductReceipt> {
    let method = match decision.source {
        SettlementSource::Cash { .. } => SettlementMethod::Cash,
        SettlementSource::Panda { .. } => SettlementMethod::Panda,
    };
    ensure(
        ic_cdk::api::msg_caller() == service(method),
        Error::Forbidden,
    )?;
    if let Some(old) = receipt(decision.decision_id) {
        ensure(
            old.decision_hash == product_decision_hash(&decision),
            Error::IdempotencyConflict,
        )?;
        return Ok(old);
    }
    let at = now();
    let begun = api::get_subject(&decision.offer.beneficiary).and_then(|s| {
        let mut b = book(&decision.offer, &s);
        b.begin_apply(&decision, at)?;
        save(&decision.offer, &b);
        Ok(b.reservation.expect("reserved").request)
    });
    let request = match begun {
        Ok(request) => request,
        Err(_) => return Ok(reject(&decision, ProductRejection::Expired, at)),
    };
    let source = match &decision.source {
        SettlementSource::Cash {
            order_id,
            ledger,
            block_index,
            amount_atomic,
        } => Ok(SubscriptionSource::Cash {
            order_id: *order_id,
            ledger: *ledger,
            block_index: *block_index,
            amount_atomic: *amount_atomic,
        }),
        SettlementSource::Panda {
            claim_id,
            quote_hash,
            committed_until_ms,
            lease_until_ms,
        } => {
            let view: Result<PandaClaimView> = call(
                store::config(|c| c.membership_canister),
                "get_panda_claim_for_product",
                (*claim_id,),
            )
            .await?;
            let v = view?;
            ensure(
                v.claim_id == *claim_id
                    && v.terms.offer == decision.offer
                    && v.terms.home_membership == store::config(|c| c.membership_canister)
                    && panda_quote_hash(&v.terms.quote) == *quote_hash
                    && v.terms.quote.committed_until_ms == *committed_until_ms
                    && v.valid_until_ms >= *lease_until_ms
                    && v.eligibility == Eligibility::Eligible
                    && panda_application_hash(&v.terms) == request.account_approval.action_digest,
                Error::IntegrityFailed,
            )
            .map(|()| SubscriptionSource::PandaClaim {
                claim_id: *claim_id,
                quote: v.terms.quote,
            })
        }
    };
    let active: Result<()> = call(
        decision.offer.beneficiary.authority_canister,
        "verify_product_account",
        (
            decision.offer.app_id.clone(),
            decision.offer.beneficiary.clone(),
        ),
    )
    .await?;
    let at = now();
    if let Some(old) = receipt(decision.decision_id) {
        return Ok(old);
    }
    let result = active
        .map_err(|_| Error::Forbidden)
        .and(source)
        .and_then(|source| deliver(&decision, source, at));
    Ok(match result {
        Ok(r) => r,
        Err(e) => reject(&decision, rejection(&e), at),
    })
}

/// Build the contract, resources and receipt, then commit them together.
fn deliver(
    decision: &ProductDecision,
    source: SubscriptionSource,
    at: u64,
) -> Result<ProductReceipt> {
    let offer = &decision.offer;
    let mut s = api::get_subject(&offer.beneficiary)?;
    let mut b = book(offer, &s);
    retire_upgraded(&mut b, offer);
    let (contract, r) = b.apply(decision, source, at)?;
    let catalog = store::catalog(offer.issued_at_ms);
    let plan = match Sku::parse(&offer.sku)? {
        Sku::Storage(id) => {
            let item = storage_product(&catalog, id)?;
            let SubscriptionSource::Cash { order_id, .. } = contract.source else {
                return Err(Error::Forbidden);
            };
            model::add_storage(
                &mut s,
                StorageAddon {
                    contract_id: contract.contract_id,
                    order_id,
                    storage_bytes: item.storage_bytes,
                    starts_at_ms: contract.offer.starts_at_ms.max(at),
                    expires_at_ms: contract.offer.expires_at_ms,
                    last_issued_until_ms: 0,
                },
                at,
            )?;
            None
        }
        Sku::Upgrade(id) => {
            let old = s
                .contracts
                .iter_mut()
                .rev()
                .find(|c| c.starts_at_ms <= at && at < model::end(c))
                .ok_or(Error::NotFound)?;
            ensure(
                matches!(old.source, ContractSource::Cash { .. }),
                Error::Forbidden,
            )?;
            old.terminated_at_ms = Some(at);
            // The upgrade keeps the annual anchor, so later prorations use the full term.
            Some((id, old.term_starts_at_ms))
        }
        Sku::Plan(id) => Some((id, contract.offer.starts_at_ms)),
    };
    if let Some((id, term_starts_at_ms)) = plan {
        s.contracts.push(MembershipContract {
            term_starts_at_ms,
            resource_pauses: vec![],
            contract_id: contract.contract_id,
            plan: model::plan(&catalog, &id)?,
            source: match &contract.source {
                SubscriptionSource::Cash { order_id, .. } => ContractSource::Cash {
                    order_id: *order_id,
                },
                SubscriptionSource::PandaClaim { claim_id, .. } => ContractSource::Sns {
                    claim_id: *claim_id,
                },
                _ => return Err(Error::UnsupportedProtocol),
            },
            starts_at_ms: contract.offer.starts_at_ms.max(at),
            expires_at_ms: contract.offer.expires_at_ms,
            terminated_at_ms: None,
            eligibility: Eligibility::Eligible,
            observed_at_ms: at,
            qualified_until_ms: contract.lease_until_ms,
            repair_deadline_ms: None,
        });
    }
    s.business_revision = b.business_revision;
    // Complete the fallible projection before writing the contract and receipt.
    model::project(
        ic_cdk::api::canister_self(),
        &mut s,
        &store::catalog(at),
        at,
    )?;
    DELIVERED.with_borrow_mut(|t| t.put(contract.contract_id.as_slice(), decision));
    store::save(&s);
    save(offer, &b);
    put_receipt(&r);
    Ok(r)
}

#[ic_cdk::update]
fn get_product_decision(id: Hash) -> Result<Option<ProductReceipt>> {
    ensure(
        [
            ic_cdk::api::canister_self(),
            store::config(|c| c.membership_canister),
        ]
        .contains(&ic_cdk::api::msg_caller()),
        Error::Forbidden,
    )?;
    Ok(receipt(id))
}

#[ic_cdk::update]
fn cancel_cash_contract(
    order_id: Hash,
    contract_id: Hash,
    decision_hash: Hash,
) -> Result<CashCancellationReceipt> {
    ensure(
        ic_cdk::api::msg_caller() == ic_cdk::api::canister_self(),
        Error::Forbidden,
    )?;
    if let Some(old) = CANCELLATIONS.with_borrow(|t| t.load(order_id.as_slice())) {
        ensure(
            old.contract_id == contract_id && old.decision_hash == decision_hash,
            Error::IdempotencyConflict,
        )?;
        return Ok(old);
    }
    let decision = DELIVERED
        .with_borrow(|t| t.load(contract_id.as_slice()))
        .ok_or(Error::NotFound)?;
    ensure(
        product_decision_hash(&decision) == decision_hash,
        Error::IntegrityFailed,
    )?;
    let mut s = store::load(&decision.offer.beneficiary)?;
    let mut b = book(&decision.offer, &s);
    let at = now();
    let result = b.cancel_cash(order_id, contract_id, decision_hash, at)?;
    if result.cancelled {
        s.contracts.retain(|c| c.contract_id != contract_id);
        s.addons.retain(|c| c.contract_id != contract_id);
        s.business_revision = b.business_revision;
        model::project(
            ic_cdk::api::canister_self(),
            &mut s,
            &store::catalog(at),
            at,
        )?;
        store::save(&s);
    }
    save(&decision.offer, &b);
    CANCELLATIONS.with_borrow_mut(|t| t.put(order_id.as_slice(), &result));
    Ok(result)
}

#[ic_cdk::update]
fn get_cash_cancellation(id: Hash) -> Result<Option<CashCancellationReceipt>> {
    ensure(
        ic_cdk::api::msg_caller() == ic_cdk::api::canister_self(),
        Error::Forbidden,
    )?;
    Ok(CANCELLATIONS.with_borrow(|t| t.load(id.as_slice())))
}

/// Qualification updates never change a business CAS revision or revive terminated rights.
pub(crate) fn observe(view: &PandaClaimView, at: u64) -> Result<()> {
    let mut s = store::load(&view.terms.offer.beneficiary)?;
    let key = base_key(&s.beneficiary);
    let mut b = BOOKS
        .with_borrow(|t| t.load(key.as_slice()))
        .ok_or(Error::NotFound)?;
    let generic = b
        .contracts
        .iter_mut()
        .find(|c| {
            matches!(c.source, SubscriptionSource::PandaClaim { claim_id, .. } if claim_id == view.claim_id)
        })
        .ok_or(Error::NotFound)?;
    if matches!(view.status, PandaClaimStatus::Terminated) {
        generic.status = SubscriptionStatus::Terminated;
        generic.lease_until_ms = at;
    } else if view.status == PandaClaimStatus::Released {
        generic.status = SubscriptionStatus::Expired;
        generic.lease_until_ms = generic.offer.expires_at_ms;
    } else {
        observe_contract(
            generic,
            view.eligibility.clone(),
            view.observed_at_ms,
            view.valid_until_ms,
            at,
        )?;
    }
    let native = s
        .contracts
        .iter_mut()
        .find(|c| c.contract_id == generic.contract_id)
        .ok_or(Error::NotFound)?;
    if matches!(
        generic.status,
        SubscriptionStatus::Terminated | SubscriptionStatus::Expired
    ) {
        native.terminated_at_ms.get_or_insert(at);
    }
    model::set_eligibility(native, view.eligibility.clone(), at)?;
    if view.eligibility == Eligibility::Ineligible {
        // Membership accumulates verified repair time up to this observation.
        native.repair_deadline_ms = Some(
            view.observed_at_ms
                .saturating_add(REPAIR_WINDOW_MS.saturating_sub(view.repair_elapsed_ms)),
        );
    }
    native.observed_at_ms = view.observed_at_ms;
    native.qualified_until_ms = generic.lease_until_ms;
    store::save_subject(&s);
    BOOKS.with_borrow_mut(|t| t.put(key.as_slice(), &b));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skus_select_one_product_kind() {
        assert!(matches!(Sku::parse("plus"), Ok(Sku::Plan(PlanId::Plus))));
        assert!(matches!(
            Sku::parse("upgrade-max"),
            Ok(Sku::Upgrade(PlanId::Max))
        ));
        let id = hex(Hash::new([120; 32]));
        assert!(matches!(Sku::parse(&id), Ok(Sku::Storage(s)) if s == id));
        assert!(Sku::parse("free").is_err());
        assert!(Sku::parse("upgrade-free").is_err());
    }

    #[test]
    fn prorations_use_the_full_annual_term() {
        let start = 1_000;
        let end = start + 365 * DAY;
        assert_eq!(prorate(4000, start, end, start).unwrap(), 40_000_000);
        // One month before the end of the original term, the whole-year anchor keeps
        // a second upgrade to about one twelfth of the annual difference.
        let late = end - 365 * DAY / 12;
        assert_eq!(prorate(15_000, start, end, late).unwrap(), 12_500_000);
        assert_eq!(prorate(100, start, end, end - 1).unwrap(), 1);
    }
}
