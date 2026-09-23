//! dMsg account-product adapter for the same v2 checkout/membership services used by other products.
use crate::{api, model, store};
use candid::Principal;
use dmsg_protocol::membership::mul_div;
use dmsg_protocol::{billing::*, commerce_v2::*, integration::*, product_book::ProductBook, *};
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
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
type Memory = VirtualMemory<DefaultMemoryImpl>;
#[derive(Clone, Serialize, Deserialize)]
struct Delivered {
    decision: ProductDecision,
    receipt: ProductReceipt,
    contract: SubscriptionContract,
    topic: Hash,
}
thread_local! {
    static BOOKS:RefCell<StableBTreeMap<Vec<u8>,Stored<ProductBook>,Memory>>=RefCell::new(StableBTreeMap::init(store::memory(16)));
    static DELIVERED:RefCell<StableBTreeMap<Vec<u8>,Stored<Delivered>,Memory>>=RefCell::new(StableBTreeMap::init(store::memory(17)));
    static RECEIPTS:RefCell<StableBTreeMap<Vec<u8>,Stored<ProductReceipt>,Memory>>=RefCell::new(StableBTreeMap::init(store::memory(18)));
    static CANCELLATIONS:RefCell<StableBTreeMap<Vec<u8>,Stored<CashCancellationReceipt>,Memory>>=RefCell::new(StableBTreeMap::init(store::memory(19)));
    static RELEASED:RefCell<StableBTreeMap<Vec<u8>,Stored<Hash>,Memory>>=RefCell::new(StableBTreeMap::init(store::memory(20)));
}
fn base_key(b: &Beneficiary) -> Hash {
    digest("dmsg/product/base/v2", b)
}
fn topic(offer: &BillingOffer) -> Hash {
    if offer.sku.len() == 64 {
        digest(
            "dmsg/product/addon/v2",
            &(&offer.beneficiary, offer.operation_id),
        )
    } else {
        base_key(&offer.beneficiary)
    }
}
fn book(offer: &BillingOffer) -> Result<ProductBook> {
    let s = api::get_subject(&offer.beneficiary)?;
    Ok(BOOKS
        .with_borrow(|t| t.load(topic(offer).as_slice()))
        .unwrap_or_else(|| ProductBook::new(offer.beneficiary.clone(), s.business_revision)))
}
fn save(offer: &BillingOffer, b: &ProductBook) {
    BOOKS.with_borrow_mut(|t| t.put(topic(offer).as_slice(), b));
}
fn receipt(id: Hash) -> Option<ProductReceipt> {
    RECEIPTS.with_borrow(|t| t.load(id.as_slice()))
}
fn put_receipt(r: &ProductReceipt) {
    RECEIPTS.with_borrow_mut(|t| t.put(r.decision_id.as_slice(), r));
}
fn source_service(source: &SettlementSource) -> Principal {
    if matches!(source, SettlementSource::Cash { .. }) {
        ic_cdk::api::canister_self()
    } else {
        store::config().init.membership_canister
    }
}
fn settlement(a: &ApplicationApproval) -> Result<SettlementMethod> {
    match a.purpose {
        ApprovalPurpose::CashCheckout => Ok(SettlementMethod::Cash),
        ApprovalPurpose::PandaSubscription => Ok(SettlementMethod::Panda),
        _ => Err(Error::Forbidden),
    }
}
fn request_service(r: &ProductAuthorizationRequest) -> Result<Principal> {
    Ok(
        if settlement(&r.account_approval)? == SettlementMethod::Cash {
            ic_cdk::api::canister_self()
        } else {
            store::config().init.membership_canister
        },
    )
}
fn registration(app: &str, product: &str) -> Result<(AppRegistration, ProductRegistration)> {
    let (a, p) = crate::registrations::configuration(app, Some(product))?;
    let p = p.ok_or(Error::NotFound)?;
    ensure(
        p.quote_authority == ic_cdk::api::canister_self()
            && p.adapter == ic_cdk::api::canister_self(),
        Error::Forbidden,
    )?;
    Ok((a, p))
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

fn offer(
    app_id: String,
    b: Beneficiary,
    sku: String,
    operation_id: Hash,
    at: u64,
) -> Result<BillingOffer> {
    api::valid_subject(&b)?;
    validate_identifier(&sku)?;
    nonzero(operation_id.as_slice())?;
    let (app, product) = registration(&app_id, &b.product_id)?;
    let s = api::get_subject(&b)?;
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
    let (start, end, amount, methods) = if let Some(target) = sku.strip_prefix("upgrade-") {
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
        let next = model::plan(&catalog, &plan_id(target)?)?;
        ensure(next.price_cents > old.plan.price_cents, Error::Forbidden)?;
        (
            at,
            current.offer.expires_at_ms,
            mul_div(
                u128::from(next.price_cents - old.plan.price_cents) * 10_000,
                u128::from(current.offer.expires_at_ms - at),
                u128::from(current.offer.expires_at_ms - current.offer.starts_at_ms),
                true,
            )?,
            vec![SettlementMethod::Cash],
        )
    } else if sku.len() == 64 {
        let current = s.active(at).ok_or(Error::MembershipIneligible)?;
        ensure(
            current.eligibility == Eligibility::Eligible && at < current.qualified_until_ms,
            Error::MembershipStale,
        )?;
        let item = catalog
            .storage_products
            .iter()
            .find(|p| hex(p.product_id) == sku)
            .ok_or(Error::NotFound)?;
        (
            at,
            current.expires_at_ms,
            mul_div(
                u128::from(item.price_cents) * 10_000,
                u128::from(current.expires_at_ms - at),
                u128::from(current.expires_at_ms - current.term_starts_at_ms),
                true,
            )?,
            vec![SettlementMethod::Cash],
        )
    } else {
        let plan = model::plan(&catalog, &plan_id(&sku)?)?;
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
    };
    let value = BillingOffer {
        version: 2,
        environment: store::config().init.environment,
        app_id,
        product_id: b.product_id.clone(),
        offer_id: digest(
            "dmsg/product/offer/v2",
            &(&b, &sku, operation_id, s.business_revision, at),
        ),
        beneficiary: b,
        quote_authority: ic_cdk::api::canister_self(),
        adapter: ic_cdk::api::canister_self(),
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
    offer(
        app_id,
        beneficiary,
        sku,
        operation_id,
        nanos_to_millis(ic_cdk::api::time()),
    )
}
#[ic_cdk::update]
fn verify_billing_offer(value: BillingOffer) -> Result<()> {
    ensure(
        [
            ic_cdk::api::canister_self(),
            store::config().init.membership_canister,
        ]
        .contains(&ic_cdk::api::msg_caller()),
        Error::Forbidden,
    )?;
    let at = nanos_to_millis(ic_cdk::api::time());
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
async fn validate_request(request: &ProductAuthorizationRequest, at: u64) -> Result<()> {
    ensure(
        ic_cdk::api::msg_caller() == request_service(request)?,
        Error::Forbidden,
    )?;
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
    validate_product_request(
        request,
        request_service(request)?,
        settlement(&request.account_approval)?,
        at,
    )?;
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
    ensure(
        ic_cdk::api::msg_caller() == request_service(&request)?,
        Error::Forbidden,
    )?;
    ensure(
        RELEASED
            .with_borrow(|t| t.load(request.offer.operation_id.as_slice()))
            .is_none(),
        Error::Forbidden,
    )?;
    let mut b = book(&request.offer)?;
    if b.reservation
        .as_ref()
        .is_some_and(|r| r.request.offer == request.offer)
    {
        return b.reserve(request, until_ms, nanos_to_millis(ic_cdk::api::time()));
    }
    validate_request(&request, nanos_to_millis(ic_cdk::api::time())).await?;
    ensure(
        RELEASED
            .with_borrow(|t| t.load(request.offer.operation_id.as_slice()))
            .is_none(),
        Error::Forbidden,
    )?;
    b = book(&request.offer)?;
    let s = api::get_subject(&request.offer.beneficiary)?;
    b.business_revision = s.business_revision;
    // Cash-only upgrades retire the old cash resource view at delivery, never a PANDA commitment.
    let original_contracts = b.contracts.clone();
    if request.offer.sku.starts_with("upgrade-") {
        b.contracts.retain(|c| {
            !matches!(c.source, SubscriptionSource::Cash { .. })
                || c.offer.starts_at_ms >= request.offer.starts_at_ms
                || c.offer.expires_at_ms <= request.offer.starts_at_ms
        });
    }
    b.reserve(
        request.clone(),
        until_ms,
        nanos_to_millis(ic_cdk::api::time()),
    )?;
    b.contracts = original_contracts;
    save(&request.offer, &b);
    Ok(())
}
#[ic_cdk::update]
fn release_product_billing(request: ProductAuthorizationRequest) -> Result<()> {
    ensure(
        ic_cdk::api::msg_caller() == request_service(&request)?,
        Error::Forbidden,
    )?;
    let mut b = book(&request.offer)?;
    b.release(&request.offer)?;
    let hash = billing_offer_hash(&request.offer);
    if let Some(old) = RELEASED.with_borrow(|t| t.load(request.offer.operation_id.as_slice())) {
        ensure(old == hash, Error::IdempotencyConflict)?;
    }
    RELEASED.with_borrow_mut(|t| t.put(request.offer.operation_id.as_slice(), &hash));
    save(&request.offer, &b);
    Ok(())
}

#[ic_cdk::update]
async fn apply_product_decision(decision: ProductDecision) -> Result<ProductReceipt> {
    ensure(
        ic_cdk::api::msg_caller() == source_service(&decision.source),
        Error::Forbidden,
    )?;
    if let Some(old) = receipt(decision.decision_id) {
        ensure(
            old.decision_hash == product_decision_hash(&decision),
            Error::IdempotencyConflict,
        )?;
        return Ok(old);
    }
    let at = nanos_to_millis(ic_cdk::api::time());
    let mut b = book(&decision.offer)?;
    let mut s = api::get_subject(&decision.offer.beneficiary)?;
    b.business_revision = s.business_revision;
    if b.begin_apply(&decision, at).is_err() {
        let r = b.reject(&decision, ProductRejection::Expired, at);
        save(&decision.offer, &b);
        put_receipt(&r);
        return Ok(r);
    }
    let request = b.reservation.as_ref().expect("reserved").request.clone();
    save(&decision.offer, &b);
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
                store::config().init.membership_canister,
                "get_panda_claim_for_product",
                (*claim_id,),
            )
            .await?;
            let v = view?;
            ensure(
                v.claim_id == *claim_id
                    && v.terms.offer == decision.offer
                    && v.terms.home_membership == store::config().init.membership_canister
                    && panda_quote_hash(&v.terms.quote) == *quote_hash
                    && v.terms.quote.committed_until_ms == *committed_until_ms
                    && v.valid_until_ms >= *lease_until_ms
                    && v.eligibility == Eligibility::Eligible
                    && panda_application_hash(&v.terms) == request.account_approval.action_digest,
                Error::IntegrityFailed,
            )?;
            Ok(SubscriptionSource::PandaClaim {
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
    let at = nanos_to_millis(ic_cdk::api::time());
    if let Some(old) = receipt(decision.decision_id) {
        return Ok(old);
    }
    b = book(&decision.offer)?;
    s = api::get_subject(&decision.offer.beneficiary)?;
    b.business_revision = s.business_revision;
    if decision.offer.sku.starts_with("upgrade-") {
        b.contracts.retain(|c| {
            !matches!(c.source, SubscriptionSource::Cash { .. })
                || c.offer.starts_at_ms >= decision.offer.starts_at_ms
                || c.offer.expires_at_ms <= decision.offer.starts_at_ms
        });
    }
    let result = if active.is_err() {
        Err(Error::Forbidden)
    } else {
        source.and_then(|source| b.apply(&decision, source, at))
    };
    let value = match result {
        Ok((contract, r)) => {
            let catalog = store::catalog(decision.offer.issued_at_ms);
            if decision.offer.sku.len() == 64 {
                let item = catalog
                    .storage_products
                    .iter()
                    .find(|p| hex(p.product_id) == decision.offer.sku)
                    .ok_or(Error::NotFound)?;
                let SubscriptionSource::Cash { order_id, .. } = contract.source else {
                    return Err(Error::Forbidden);
                };
                ensure(s.addons.len() < 64, Error::QuotaExceeded)?;
                s.addons.push(StorageAddon {
                    contract_id: contract.contract_id,
                    order_id,
                    storage_bytes: item.storage_bytes,
                    starts_at_ms: contract.offer.starts_at_ms.max(at),
                    expires_at_ms: contract.offer.expires_at_ms,
                    last_issued_until_ms: 0,
                });
            } else {
                let upgrade = decision.offer.sku.strip_prefix("upgrade-");
                let plan =
                    model::plan(&catalog, &plan_id(upgrade.unwrap_or(&decision.offer.sku))?)?;
                if upgrade.is_some() {
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
                }
                s.contracts.push(MembershipContract {
                    term_starts_at_ms: contract.offer.starts_at_ms,
                    resource_pauses: vec![],
                    contract_id: contract.contract_id,
                    plan,
                    source: match contract.source {
                        SubscriptionSource::Cash { order_id, .. } => {
                            ContractSource::Cash { order_id }
                        }
                        SubscriptionSource::PandaClaim { claim_id, .. } => {
                            ContractSource::Sns { claim_id }
                        }
                        _ => return Err(Error::UnsupportedProtocol),
                    },
                    starts_at_ms: contract.offer.starts_at_ms.max(at),
                    expires_at_ms: contract.offer.expires_at_ms,
                    terminated_at_ms: None,
                    closing_at_ms: None,
                    last_issued_until_ms: 0,
                    eligibility: Eligibility::Eligible,
                    observed_at_ms: at,
                    qualified_until_ms: contract.lease_until_ms,
                    repair_deadline_ms: None,
                    unverifiable_since_ms: None,
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
            DELIVERED.with_borrow_mut(|t| {
                t.put(
                    contract.contract_id.as_slice(),
                    &Delivered {
                        topic: topic(&decision.offer),
                        decision: decision.clone(),
                        receipt: r.clone(),
                        contract: contract.clone(),
                    },
                )
            });
            store::save(&s);
            r
        }
        Err(e) => {
            b = book(&decision.offer)?;
            b.reject(
                &decision,
                if e == Error::Forbidden {
                    ProductRejection::Unauthorized
                } else if e == Error::Expired {
                    ProductRejection::Expired
                } else {
                    ProductRejection::RevisionConflict
                },
                at,
            )
        }
    };
    save(&decision.offer, &b);
    put_receipt(&value);
    Ok(value)
}
#[ic_cdk::update]
fn get_product_decision(id: Hash) -> Result<Option<ProductReceipt>> {
    ensure(
        [
            ic_cdk::api::canister_self(),
            store::config().init.membership_canister,
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
    let record = DELIVERED
        .with_borrow(|t| t.load(contract_id.as_slice()))
        .ok_or(Error::NotFound)?;
    ensure(
        product_decision_hash(&record.decision) == decision_hash,
        Error::IntegrityFailed,
    )?;
    let mut b = book(&record.decision.offer)?;
    let mut s = store::load(&b.beneficiary)?;
    b.business_revision = s.business_revision;
    let at = nanos_to_millis(ic_cdk::api::time());
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
    save(&record.decision.offer, &b);
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
    let generic=b.contracts.iter_mut().find(|c|matches!(c.source,SubscriptionSource::PandaClaim {claim_id,..} if claim_id==view.claim_id)).ok_or(Error::NotFound)?;
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
    native.eligibility = view.eligibility.clone();
    native.observed_at_ms = view.observed_at_ms;
    native.qualified_until_ms = generic.lease_until_ms;
    if native.eligibility != Eligibility::Eligible
        && !native
            .resource_pauses
            .last()
            .is_some_and(|(_, end)| end.is_none())
    {
        ensure(native.resource_pauses.len() < 128, Error::QuotaExceeded)?;
        native.resource_pauses.push((at, None));
    }
    if native.eligibility == Eligibility::Eligible {
        if let Some((_, end)) = native
            .resource_pauses
            .last_mut()
            .filter(|(_, end)| end.is_none())
        {
            *end = Some(at);
        }
    }
    store::save_subject(&s);
    BOOKS.with_borrow_mut(|t| t.put(key.as_slice(), &b));
    Ok(())
}
