//! Independent account-product reference adapter. Its IDs and ownership have no dMsg-account meaning.
use candid::{CandidType, Principal};
use dmsg_protocol::{commerce_v2::*, integration::*, product_book::*, *};
use dmsg_runtime::{
    call,
    storage::{MapExt, Stored},
};
use dmsg_types::{
    integration::*,
    integration_billing::*,
    integration_membership::*,
    membership::{Beneficiary, Eligibility},
    *,
};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap};
type Memory = VirtualMemory<DefaultMemoryImpl>;
#[derive(Clone, CandidType, Serialize, Deserialize)]
pub struct Config {
    pub admin: Principal,
    pub commerce: Principal,
    pub membership: Principal,
    pub environment: Environment,
    pub app_id: String,
    pub product_id: String,
    pub terms_hash: Hash,
    pub annual_usd_micros: u128,
}

#[derive(Clone, CandidType, Serialize, Deserialize)]
pub struct Prepared {
    pub offer: BillingOffer,
    pub approval: ProductApproval,
}

#[derive(Clone, Serialize, Deserialize)]
struct Subject {
    owner: Principal,
    book: ProductBook,
    offers: BTreeMap<Hash, Prepared>,
}

thread_local! {
 static MEM:RefCell<MemoryManager<DefaultMemoryImpl>>=RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
 static CONFIG:RefCell<StableCell<Stored<Option<Config>>,Memory>>=RefCell::new(StableCell::init(memory(0),Stored(None)));
 static SUBJECTS:RefCell<StableBTreeMap<Vec<u8>,Stored<Subject>,Memory>>=RefCell::new(StableBTreeMap::init(memory(1)));
 static RECEIPTS:RefCell<StableBTreeMap<Vec<u8>,Stored<ProductReceipt>,Memory>>=RefCell::new(StableBTreeMap::init(memory(2)));
 static CANCELLED:RefCell<StableBTreeMap<Vec<u8>,Stored<CashCancellationReceipt>,Memory>>=RefCell::new(StableBTreeMap::init(memory(3)));
 static RELEASED:RefCell<StableBTreeMap<Vec<u8>,Stored<Hash>,Memory>>=RefCell::new(StableBTreeMap::init(memory(4)));
 static LOSE_ACK:RefCell<bool>=const {RefCell::new(false)};
}

fn memory(id: u8) -> Memory {
    MEM.with_borrow(|m| m.get(MemoryId::new(id)))
}

fn config() -> Config {
    CONFIG.with_borrow(|c| c.get().0.clone().expect("initialized"))
}

fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}

fn subject(b: &Beneficiary) -> Result<Subject> {
    let c = config();
    ensure(
        b.product_id == c.product_id
            && b.authority_canister == ic_cdk::api::canister_self()
            && b.subject_schema == "sample-account-v1"
            && b.subject_bytes.len() == 12,
        Error::Forbidden,
    )?;
    SUBJECTS
        .with_borrow(|t| t.load(&b.subject_bytes))
        .ok_or(Error::NotFound)
}

fn save(s: &Subject) {
    SUBJECTS.with_borrow_mut(|t| t.put(&s.book.beneficiary.subject_bytes, s));
}

fn service(m: &SettlementMethod) -> Principal {
    if *m == SettlementMethod::Cash {
        config().commerce
    } else {
        config().membership
    }
}

fn method(a: &ApplicationApproval) -> SettlementMethod {
    match a.purpose {
        ApprovalPurpose::CashCheckout => SettlementMethod::Cash,
        ApprovalPurpose::PandaSubscription => SettlementMethod::Panda,
    }
}

fn receipt(id: Hash) -> Option<ProductReceipt> {
    RECEIPTS.with_borrow(|t| t.load(id.as_slice()))
}

#[ic_cdk::init]
fn init(c: Config) {
    for p in [c.admin, c.commerce, c.membership] {
        authenticated(p).expect("authenticated deployment");
    }
    validate_identifier(&c.app_id).expect("app");
    validate_identifier(&c.product_id).expect("product");
    assert!(c.annual_usd_micros > 0);
    CONFIG.with_borrow_mut(|v| v.set(Stored(Some(c))));
}

#[ic_cdk::update]
fn assign_account(id: AccountId, owner: Principal) -> Result<()> {
    let c = config();
    ensure(ic_cdk::api::msg_caller() == c.admin, Error::Forbidden)?;
    authenticated(owner)?;
    if let Some(mut s) = SUBJECTS.with_borrow(|t| t.load(id.as_slice())) {
        s.owner = owner;
        s.book.bump()?;
        save(&s);
    } else {
        ensure(
            SUBJECTS.with_borrow(|t| t.len()) < 100,
            Error::QuotaExceeded,
        )?;
        let b = Beneficiary {
            product_id: c.product_id,
            authority_canister: ic_cdk::api::canister_self(),
            subject_schema: "sample-account-v1".into(),
            subject_bytes: id.to_vec().into(),
        };
        save(&Subject {
            owner,
            book: ProductBook::new(b, 1),
            offers: BTreeMap::new(),
        });
    }
    Ok(())
}

#[ic_cdk::update]
fn prepare_billing_offer(
    id: AccountId,
    operation: Hash,
    method: SettlementMethod,
) -> Result<Prepared> {
    let c = config();
    let mut s = SUBJECTS
        .with_borrow(|t| t.load(id.as_slice()))
        .ok_or(Error::NotFound)?;
    ensure(s.owner == ic_cdk::api::msg_caller(), Error::Forbidden)?;
    if let Some(old) = s.offers.get(&operation) {
        ensure(old.approval.method == method, Error::IdempotencyConflict)?;
        return Ok(old.clone());
    }
    let at = now();
    s.book.prune(at);
    s.offers.retain(|_, v| v.approval.expires_at_ms > at);
    ensure(s.offers.len() < 32, Error::QuotaExceeded)?;
    nonzero(operation.as_slice())?;
    let start = s
        .book
        .contracts
        .iter()
        .map(|v| v.offer.expires_at_ms)
        .max()
        .unwrap_or(at)
        .max(at);
    let end = dmsg_protocol::billing::next_year(start)?;
    let offer = BillingOffer {
        version: 2,
        environment: c.environment,
        app_id: c.app_id,
        product_id: c.product_id,
        offer_id: digest(
            "sample/offer/v2",
            &(&id, operation, s.book.business_revision),
        ),
        beneficiary: s.book.beneficiary.clone(),
        quote_authority: ic_cdk::api::canister_self(),
        adapter: ic_cdk::api::canister_self(),
        sku: "annual".into(),
        product_terms_hash: c.terms_hash,
        expected_business_revision: s.book.business_revision,
        amount_usd_micros: c.annual_usd_micros,
        starts_at_ms: start,
        expires_at_ms: end,
        issued_at_ms: at,
        accept_by_ms: at + OFFER_TTL_MS,
        operation_id: operation,
        allowed_settlement_methods: vec![SettlementMethod::Cash, SettlementMethod::Panda],
    };
    let approval = ProductApproval {
        version: 2,
        approval_id: digest("sample/approval/v2", &(&offer, &method, s.owner)),
        offer_hash: billing_offer_hash(&offer),
        operator: s.owner,
        method,
        approved_at_ms: at,
        expires_at_ms: at + APPLICATION_TTL_MS,
    };
    let result = Prepared { offer, approval };
    s.offers.insert(operation, result.clone());
    save(&s);
    Ok(result)
}

fn check(request: &ProductAuthorizationRequest, at: u64) -> Result<Subject> {
    let m = method(&request.account_approval);
    ensure(ic_cdk::api::msg_caller() == service(&m), Error::Forbidden)?;
    validate_product_request(request, service(&m), m.clone(), at)?;
    let s = subject(&request.offer.beneficiary)?;
    let expected = s
        .offers
        .get(&request.offer.operation_id)
        .ok_or(Error::NotFound)?;
    ensure(
        expected.offer == request.offer
            && request.product_approval.as_ref() == Some(&expected.approval)
            && expected.approval.operator == s.owner
            && request.offer.expected_business_revision == s.book.business_revision,
        Error::VersionConflict,
    )?;
    check_operator_approval(&expected.approval, &request.offer, m, at)?;
    Ok(s)
}

#[ic_cdk::update]
fn verify_billing_offer(offer: BillingOffer) -> Result<()> {
    ensure(
        [config().commerce, config().membership].contains(&ic_cdk::api::msg_caller()),
        Error::Forbidden,
    )?;
    let s = subject(&offer.beneficiary)?;
    let v = s.offers.get(&offer.operation_id).ok_or(Error::NotFound)?;
    ensure(
        v.offer == offer
            && offer.expected_business_revision == s.book.business_revision
            && v.approval.operator == s.owner
            && now() < offer.accept_by_ms,
        Error::PolicyStale,
    )
}

#[ic_cdk::update]
fn authorize_product_billing(request: ProductAuthorizationRequest) -> Result<ProductAuthorization> {
    let at = now();
    let s = check(&request, at)?;
    Ok(ProductAuthorization {
        request_hash: product_authorization_hash(&request),
        operator: s.owner,
        verified_at_ms: at,
        valid_until_ms: (at + MINUTE).min(request.account_approval.expires_at_ms),
    })
}

#[ic_cdk::update]
fn reserve_product_billing(request: ProductAuthorizationRequest, until: u64) -> Result<()> {
    let at = now();
    let mut s = check(&request, at)?;
    ensure(
        RELEASED
            .with_borrow(|t| t.load(request.offer.operation_id.as_slice()))
            .is_none(),
        Error::Forbidden,
    )?;
    s.book.reserve(request, until, at)?;
    save(&s);
    Ok(())
}

#[ic_cdk::update]
fn release_product_billing(request: ProductAuthorizationRequest) -> Result<()> {
    ensure(
        ic_cdk::api::msg_caller() == service(&method(&request.account_approval)),
        Error::Forbidden,
    )?;
    let mut s = subject(&request.offer.beneficiary)?;
    s.book.release(&request.offer)?;
    let hash = billing_offer_hash(&request.offer);
    if let Some(old) = RELEASED.with_borrow(|t| t.load(request.offer.operation_id.as_slice())) {
        ensure(old == hash, Error::IdempotencyConflict)?;
    }
    RELEASED.with_borrow_mut(|t| t.put(request.offer.operation_id.as_slice(), &hash));
    save(&s);
    Ok(())
}

#[ic_cdk::update]
async fn apply_product_decision(d: ProductDecision) -> Result<ProductReceipt> {
    let c = config();
    ensure(
        ic_cdk::api::msg_caller()
            == if matches!(d.source, SettlementSource::Cash { .. }) {
                c.commerce
            } else {
                c.membership
            },
        Error::Forbidden,
    )?;
    if let Some(r) = receipt(d.decision_id) {
        ensure(
            r.decision_hash == product_decision_hash(&d),
            Error::IdempotencyConflict,
        )?;
        return Ok(r);
    }
    let mut s = subject(&d.offer.beneficiary)?;
    let at = now();
    if s.book.begin_apply(&d, at).is_err() {
        let r = s.book.reject(&d, ProductRejection::Expired, at);
        save(&s);
        RECEIPTS.with_borrow_mut(|t| t.put(d.decision_id.as_slice(), &r));
        return Ok(r);
    }
    let request = s
        .book
        .reservation
        .as_ref()
        .expect("reserved")
        .request
        .clone();
    save(&s);
    let source = match &d.source {
        SettlementSource::Cash {
            order_id,
            ledger,
            block_index,
            amount_atomic,
        } => {
            let reply: Result<CheckoutView> =
                call(c.commerce, "get_checkout_for_product", (*order_id,)).await?;
            let v = reply?;
            ensure(
                v.progress.decision_id == Some(d.decision_id)
                    && v.quote.offer == d.offer
                    && checkout_quote_hash(&v.quote) == request.account_approval.action_digest,
                Error::IntegrityFailed,
            )?;
            SubscriptionSource::Cash {
                order_id: *order_id,
                ledger: *ledger,
                block_index: *block_index,
                amount_atomic: *amount_atomic,
            }
        }
        SettlementSource::Panda {
            claim_id,
            quote_hash,
            lease_until_ms,
            ..
        } => {
            let reply: Result<PandaClaimView> =
                call(c.membership, "get_panda_claim_for_product", (*claim_id,)).await?;
            let v = reply?;
            ensure(
                v.terms.offer == d.offer
                    && panda_application_hash(&v.terms) == request.account_approval.action_digest
                    && panda_quote_hash(&v.terms.quote) == *quote_hash
                    && v.eligibility == Eligibility::Eligible
                    && v.valid_until_ms >= *lease_until_ms,
                Error::IntegrityFailed,
            )?;
            SubscriptionSource::PandaClaim {
                claim_id: *claim_id,
                quote: v.terms.quote,
            }
        }
    };
    if let Some(r) = receipt(d.decision_id) {
        return Ok(r);
    }
    let at = now();
    s = subject(&d.offer.beneficiary)?;
    let result = if request
        .product_approval
        .as_ref()
        .is_some_and(|a| a.operator == s.owner)
    {
        s.book.apply(&d, source, at)
    } else {
        Err(Error::Forbidden)
    };
    let r = match result {
        Ok((_, r)) => r,
        Err(_) => s.book.reject(&d, ProductRejection::RevisionConflict, at),
    };
    save(&s);
    RECEIPTS.with_borrow_mut(|t| t.put(d.decision_id.as_slice(), &r));
    let lose = LOSE_ACK.with_borrow_mut(std::mem::take);
    if lose {
        let _: () = call(ic_cdk::api::canister_self(), "barrier", ()).await?;
        ic_cdk::trap("test lost Apply ACK after durable delivery");
    }
    Ok(r)
}

#[ic_cdk::update]
fn get_product_decision(id: Hash) -> Result<Option<ProductReceipt>> {
    ensure(
        [config().commerce, config().membership].contains(&ic_cdk::api::msg_caller()),
        Error::Forbidden,
    )?;
    Ok(receipt(id))
}

#[ic_cdk::update]
fn cancel_cash_contract(
    order: Hash,
    contract: Hash,
    decision_hash: Hash,
) -> Result<CashCancellationReceipt> {
    ensure(
        ic_cdk::api::msg_caller() == config().commerce,
        Error::Forbidden,
    )?;
    if let Some(r) = CANCELLED.with_borrow(|t| t.load(order.as_slice())) {
        ensure(
            r.contract_id == contract && r.decision_hash == decision_hash,
            Error::IdempotencyConflict,
        )?;
        return Ok(r);
    }
    // Sample lookup is bounded by its explicitly limited set of registered accounts.
    let mut s = SUBJECTS
        .with_borrow(|t| {
            t.iter().take(100).find_map(|e| {
                let s = e.value().0;
                s.book
                    .contracts
                    .iter()
                    .any(|c| c.contract_id == contract)
                    .then_some(s)
            })
        })
        .ok_or(Error::NotFound)?;
    let c = s
        .book
        .contracts
        .iter()
        .find(|c| c.contract_id == contract)
        .ok_or(Error::NotFound)?;
    let applied = receipt(c.decision_id.ok_or(Error::IntegrityFailed)?).ok_or(Error::NotFound)?;
    ensure(
        applied.decision_hash == decision_hash,
        Error::IntegrityFailed,
    )?;
    let r = s.book.cancel_cash(order, contract, decision_hash, now())?;
    save(&s);
    CANCELLED.with_borrow_mut(|t| t.put(order.as_slice(), &r));
    Ok(r)
}

#[ic_cdk::update]
fn get_cash_cancellation(id: Hash) -> Result<Option<CashCancellationReceipt>> {
    ensure(
        ic_cdk::api::msg_caller() == config().commerce,
        Error::Forbidden,
    )?;
    Ok(CANCELLED.with_borrow(|t| t.load(id.as_slice())))
}

#[ic_cdk::query]
fn contracts(id: AccountId) -> Result<Vec<SubscriptionContract>> {
    let s = SUBJECTS
        .with_borrow(|t| t.load(id.as_slice()))
        .ok_or(Error::NotFound)?;
    ensure(ic_cdk::api::msg_caller() == s.owner, Error::Forbidden)?;
    Ok(s.book.contracts)
}

/// Fault injection exists only in Local; it changes delivery transport, never authorization.
#[ic_cdk::update]
fn lose_next_apply_ack() -> Result<()> {
    ensure(
        config().environment == Environment::Local && config().admin == ic_cdk::api::msg_caller(),
        Error::Forbidden,
    )?;
    LOSE_ACK.with_borrow_mut(|v| *v = true);
    Ok(())
}

#[ic_cdk::update]
fn barrier() {}
ic_cdk::export_candid!();

/// The sample exposes no paid rights from an expired qualification cache.
#[ic_cdk::update]
async fn entitlement(id: AccountId) -> Result<Option<u64>> {
    let mut s = SUBJECTS
        .with_borrow(|t| t.load(id.as_slice()))
        .ok_or(Error::NotFound)?;
    ensure(ic_cdk::api::msg_caller() == s.owner, Error::Forbidden)?;
    let at = now();
    let Some(c) = s
        .book
        .contracts
        .iter()
        .find(|c| {
            c.offer.starts_at_ms <= at
                && at < c.offer.expires_at_ms
                && c.status != SubscriptionStatus::Cancelled
        })
        .cloned()
    else {
        return Ok(None);
    };
    if let SubscriptionSource::PandaClaim { claim_id, .. } = c.source {
        if c.status != SubscriptionStatus::Terminated
            && c.lease_until_ms <= at.saturating_add(MINUTE)
        {
            let answer: Result<Result<PandaClaimView>> =
                call(config().membership, "refresh_panda_claim", (claim_id,)).await;
            s = SUBJECTS
                .with_borrow(|t| t.load(id.as_slice()))
                .ok_or(Error::NotFound)?;
            if let Some(current) = s
                .book
                .contracts
                .iter_mut()
                .find(|v| v.contract_id == c.contract_id)
            {
                if let Ok(Ok(v)) = answer {
                    ensure(v.terms.offer == current.offer, Error::IntegrityFailed)?;
                    if v.status == PandaClaimStatus::Terminated {
                        current.status = SubscriptionStatus::Terminated;
                    } else {
                        observe_contract(
                            current,
                            v.eligibility,
                            v.observed_at_ms,
                            v.valid_until_ms,
                            now(),
                        )?;
                    }
                }
            }
        }
    }
    let until = s
        .book
        .contracts
        .iter_mut()
        .find(|v| v.contract_id == c.contract_id)
        .and_then(|c| entitlement_until(c, now()));
    save(&s);
    Ok(until)
}
