//! Generic v2 merchant checkout. A product adapter owns delivery; only this book owns funds.
use crate::{
    checkout_model::{self as model, Order, Transfer},
    registrations, store,
};
use candid::{CandidType, Nat, Principal};
use dmsg_protocol::{commerce_v2::*, integration::*, *};
use dmsg_runtime::{
    call, call_classified,
    ledger::{block_index, read_transfer, token_amount},
    storage::{MapExt, Stored},
};
use dmsg_types::{integration::*, integration_billing::*, *};
use ic_stable_structures::{
    memory_manager::VirtualMemory, DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{TransferArg, TransferError},
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

type Memory = VirtualMemory<DefaultMemoryImpl>;
#[derive(Clone, Serialize, Deserialize)]
struct Asset {
    policy: SettlementAsset,
    verified: bool,
    fee: u128,
}

thread_local! {
    static PRICE_AUTHORITY:RefCell<StableCell<Stored<Option<Principal>>,Memory>>=RefCell::new(StableCell::init(store::memory(15),Stored(None)));
    static ASSETS:RefCell<StableBTreeMap<Vec<u8>,Stored<Asset>,Memory>>=RefCell::new(StableBTreeMap::init(store::memory(10)));
    static ORDERS:RefCell<StableBTreeMap<Vec<u8>,Stored<Order>,Memory>>=RefCell::new(StableBTreeMap::init(store::memory(11)));
    static DEPOSITS:RefCell<StableBTreeMap<Vec<u8>,Stored<CheckoutDeposit>,Memory>>=RefCell::new(StableBTreeMap::init(store::memory(12)));
    static TRANSFERS:RefCell<StableBTreeMap<Vec<u8>,Stored<Transfer>,Memory>>=RefCell::new(StableBTreeMap::init(store::memory(13)));
    static BLOCKS:RefCell<StableBTreeMap<Vec<u8>,Stored<Hash>,Memory>>=RefCell::new(StableBTreeMap::init(store::memory(14)));
}

fn asset(ledger: Principal) -> Result<Asset> {
    ASSETS
        .with_borrow(|t| t.load(ledger.as_slice()))
        .ok_or(Error::NotFound)
}

fn order(id: Hash) -> Result<Order> {
    ORDERS
        .with_borrow(|t| t.load(id.as_slice()))
        .ok_or(Error::NotFound)
}

fn transfer(id: Hash) -> Result<Transfer> {
    TRANSFERS
        .with_borrow(|t| t.load(id.as_slice()))
        .ok_or(Error::NotFound)
}

fn block_key(block: &CashBlock) -> Hash {
    digest("dmsg/checkout/block/v2", block)
}

fn deposit_key(id: Hash, block: &CashBlock) -> Vec<u8> {
    [id.as_slice(), block_key(block).as_slice()].concat()
}

fn deposit(id: Hash, block: &CashBlock) -> Result<CheckoutDeposit> {
    DEPOSITS
        .with_borrow(|t| t.load(&deposit_key(id, block)))
        .ok_or(Error::NotFound)
}

fn save(o: &Order) {
    assert!(o.conserved(), "per-ledger money conservation");
    ORDERS.with_borrow_mut(|t| t.put(o.id.as_slice(), o));
    store::CERT.with_borrow_mut(|c| c.put(order_key(o.id), &o.view()));
}

fn save_transfer(t: &Transfer) {
    TRANSFERS.with_borrow_mut(|m| m.put(t.view.transfer_id.as_slice(), t));
    store::CERT.with_borrow_mut(|c| c.put(transfer_key(t.view.transfer_id), &t.view));
}

fn read_access(o: &Order, caller: Principal) -> Result<()> {
    ensure(
        caller == o.input.quote.cash.payer.owner
            || caller == o.input.quote.product.adapter
            || caller == store::config().init.governance,
        Error::Forbidden,
    )
}

fn configuration(offer: &BillingOffer) -> Result<(AppRegistration, ProductRegistration)> {
    let (a, p) = registrations::configuration(&offer.app_id, Some(&offer.product_id))?;
    Ok((a, p.ok_or(Error::NotFound)?))
}

#[ic_cdk::update]
fn register_settlement_asset(policy: SettlementAsset) -> Result<()> {
    ensure(
        ic_cdk::api::msg_caller() == store::config().init.governance,
        Error::Forbidden,
    )?;
    validate_asset(&policy)?;
    ensure(
        policy.environment == store::config().init.environment,
        Error::Forbidden,
    )?;
    let old = asset(policy.ledger).ok();
    if let Some(old) = &old {
        ensure(
            old.policy.asset == policy.asset
                && old.policy.environment == policy.environment
                && old.policy.decimals == policy.decimals,
            Error::IntegrityFailed,
        )?;
        if old.policy == policy {
            return Ok(());
        }
        ensure(
            old.policy.policy_version.checked_add(1) == Some(policy.policy_version),
            Error::VersionConflict,
        )?;
    } else {
        ensure(
            policy.policy_version == 1 && ASSETS.with_borrow(|t| t.len()) < 2,
            Error::QuotaExceeded,
        )?;
        ensure(
            ASSETS.with_borrow(|t| t.iter().all(|v| v.value().0.policy.asset != policy.asset)),
            Error::IdempotencyConflict,
        )?;
    }
    let verified = old.as_ref().is_some_and(|o| o.verified);
    let fee = old.as_ref().map_or(policy.network_fee_atomic, |o| o.fee);
    ASSETS.with_borrow_mut(|t| {
        t.put(
            policy.ledger.as_slice(),
            &Asset {
                fee,
                policy: policy.clone(),
                verified,
            },
        )
    });
    publish_assets();
    Ok(())
}

#[derive(CandidType, Deserialize)]
struct Standard {
    name: String,
    url: String,
}

#[ic_cdk::update]
async fn verify_settlement_asset(ledger: Principal, sample_transfer: Option<u128>) -> Result<()> {
    let previous = asset(ledger)?;
    if previous.policy.environment != Environment::Local {
        ensure(
            sample_transfer.is_some(),
            invalid("a verified transfer block is required"),
        )?;
    }
    let decimals: u8 = call(ledger, "icrc1_decimals", ()).await?;
    let fee: Nat = call(ledger, "icrc1_fee", ()).await?;
    let standards: Vec<Standard> = call(ledger, "icrc1_supported_standards", ()).await?;
    ensure(
        standards.len() <= 32
            && standards
                .iter()
                .all(|s| s.name.len() <= 64 && s.url.len() <= 1024)
            && ["ICRC-1", "ICRC-3"]
                .iter()
                .all(|name| standards.iter().any(|s| s.name == *name)),
        Error::UnsupportedProtocol,
    )?;
    if let Some(index) = sample_transfer {
        read_transfer(
            ledger,
            u64::try_from(index).map_err(|_| Error::QuotaExceeded)?,
        )
        .await?;
    }
    let fee = token_amount(fee)?;
    let mut current = asset(ledger)?;
    ensure(current.policy == previous.policy, Error::PolicyStale)?;
    ensure(
        u16::from(decimals) == current.policy.decimals
            && fee == current.policy.network_fee_atomic
            && fee <= current.policy.max_network_fee_atomic,
        Error::FeeBlocked,
    )?;
    current.verified = true;
    current.fee = fee;
    ASSETS.with_borrow_mut(|t| t.put(ledger.as_slice(), &current));
    publish_assets();
    Ok(())
}

#[ic_cdk::query]
fn settlement_assets() -> Vec<SettlementAssetView> {
    ASSETS.with_borrow(|t| {
        t.iter()
            .map(|v| {
                let a = v.value().0;
                SettlementAssetView {
                    policy: a.policy,
                    ledger_verified: a.verified,
                }
            })
            .collect()
    })
}

#[ic_cdk::update]
async fn quote_checkout(
    offer: BillingOffer,
    ledger: Principal,
    payer: Account,
) -> Result<CheckoutQuote> {
    let (app, product) = configuration(&offer)?;
    let at = nanos_to_millis(ic_cdk::api::time());
    validate_billing_offer(&offer, &app, &product, at)?;
    ensure(!store::config().paused, Error::Locked)?;
    store::reserve_call(
        at,
        store::CallBudget::Authorization(ic_cdk::api::msg_caller()),
    )?;
    let valid: Result<()> = call(
        product.quote_authority,
        "verify_billing_offer",
        (offer.clone(),),
    )
    .await?;
    valid?;
    let at = nanos_to_millis(ic_cdk::api::time());
    let (current, registration) = configuration(&offer)?;
    ensure(
        current == app && registration == product && !store::config().paused,
        Error::PolicyStale,
    )?;
    let a = asset(ledger)?;
    ensure(
        a.verified && a.fee == a.policy.network_fee_atomic,
        Error::Unavailable("ledger/fee is not verified".into()),
    )?;
    checkout_quote(
        ic_cdk::api::canister_self(),
        offer,
        &app,
        &product,
        a.policy,
        payer,
        at,
    )
}

async fn authorize(
    input: &OpenCheckout,
    caller: Principal,
    home: Principal,
    at: u64,
) -> Result<()> {
    let quote = &input.quote;
    let auth = &input.authorization;
    let (app, product) = configuration(&quote.offer)?;
    ensure(
        auth.offer == quote.offer
            && caller == quote.cash.payer.owner
            && caller == auth.account_approval.actor
            && auth.account_approval.action_digest == checkout_quote_hash(quote)
            && app.user_homes.contains(&auth.user_home),
        Error::Forbidden,
    )?;
    validate_product_request(auth, home, SettlementMethod::Cash, at)?;
    validate_application_approval(&auth.account_approval, &app, &product, home, at)?;
    let reconstructed = checkout_quote(
        home,
        quote.offer.clone(),
        &app,
        &product,
        quote.asset.clone(),
        quote.cash.payer,
        quote.quoted_at_ms,
    )?;
    ensure(
        reconstructed == *quote
            && quote.quoted_at_ms <= at
            && at < quote.offer.accept_by_ms
            && at < quote.cash.funding_deadline_ms,
        Error::IntegrityFailed,
    )?;
    let a = asset(quote.cash.ledger)?;
    check_quoted_asset(&a.policy, &quote.asset, at)?;
    ensure(
        a.verified && a.fee == a.policy.network_fee_atomic && !store::config().paused,
        Error::PolicyStale,
    )?;
    let product_auth: Result<ProductAuthorization> = call(
        product.beneficiary_authority,
        "authorize_product_billing",
        (auth.clone(),),
    )
    .await?;
    let product_auth = product_auth?;
    let approved: Result<ApplicationAuthorization> = call(
        auth.user_home,
        "verify_application_authorization",
        (auth.approval_id, auth.account_approval.clone()),
    )
    .await?;
    let approved = approved?;
    let at = nanos_to_millis(ic_cdk::api::time());
    check_product_authorization(auth, &product_auth, at)?;
    ensure(
        approved.approval_id == auth.approval_id
            && approved.approval_hash == application_approval_hash(&auth.account_approval)
            && approved.verified_at_ms <= at
            && at - approved.verified_at_ms <= MINUTE
            && at < approved.valid_until_ms,
        Error::PolicyStale,
    )?;
    let (current, next) = configuration(&quote.offer)?;
    ensure(
        current == app
            && next == product
            && at < quote.offer.accept_by_ms
            && at < quote.cash.funding_deadline_ms
            && !store::config().paused,
        Error::PolicyStale,
    )?;
    let a = asset(quote.cash.ledger)?;
    check_quoted_asset(&a.policy, &quote.asset, at)?;
    ensure(
        a.verified && a.fee == a.policy.network_fee_atomic,
        Error::PolicyStale,
    )
}

#[ic_cdk::update]
async fn open_checkout(input: OpenCheckout) -> Result<CheckoutView> {
    let caller = ic_cdk::api::msg_caller();
    let home = ic_cdk::api::canister_self();
    authenticated(caller)?;
    let id = checkout_id(home, &input.quote.offer);
    if let Ok(old) = order(id) {
        read_access(&old, caller)?;
        ensure(old.input == input, Error::IdempotencyConflict)?;
        return Ok(old.view());
    }
    let at = nanos_to_millis(ic_cdk::api::time());
    ensure(
        canonical(&input).len() <= MAX_PAYLOAD && ORDERS.with_borrow(|t| t.len()) < 1_000_000,
        Error::QuotaExceeded,
    )?;
    store::reserve_call(at, store::CallBudget::Authorization(caller))?;
    authorize(&input, caller, home, at).await?;
    let at = nanos_to_millis(ic_cdk::api::time());
    if let Ok(old) = order(id) {
        ensure(old.input == input, Error::IdempotencyConflict)?;
        return Ok(old.view());
    }
    store::reserve_order(at)?;
    let o = Order::new(home, input);
    save(&o);
    reserve(id).await?;
    Ok(order(id)?.view())
}

async fn reserve(id: Hash) -> Result<()> {
    let old = order(id)?;
    if old.status != CheckoutStatus::Reserving {
        return Ok(());
    }
    let result: Result<Result<()>> = call(
        old.input.quote.product.adapter,
        "reserve_product_billing",
        (
            old.input.authorization.clone(),
            old.input.quote.cash.activation_deadline_ms,
        ),
    )
    .await;
    let at = nanos_to_millis(ic_cdk::api::time());
    let mut current = order(id)?;
    if current.status != CheckoutStatus::Reserving {
        return Ok(());
    }
    match result {
        Ok(Ok(())) => {
            current.status = if at < current.input.quote.cash.funding_deadline_ms {
                CheckoutStatus::AwaitingFunding
            } else {
                CheckoutStatus::RefundCommitted
            }
        }
        Ok(Err(_)) => current.status = CheckoutStatus::Rejected,
        Err(_) => return Err(Error::ExecutionUnknown),
    }
    save(&current);
    Ok(())
}

/// Safe progress can be advanced without revealing the payer or private bill.
#[ic_cdk::update]
async fn reconcile_checkout(id: Hash) -> Result<CheckoutProgress> {
    let at = nanos_to_millis(ic_cdk::api::time());
    store::reserve_call(at, store::CallBudget::Funds)?;
    let current = order(id)?;
    if current.status == CheckoutStatus::Reserving {
        reserve(id).await?;
    } else if current.status == CheckoutStatus::Applying {
        deliver(id, true).await?;
    } else if current.cancellation_pending {
        cancel(id, true).await?;
    } else if matches!(
        current.status,
        CheckoutStatus::RefundCommitted | CheckoutStatus::Rejected
    ) && current.decision.is_none()
        && !current.reservation_released
    {
        release_reservation(id).await?;
    } else if current.status == CheckoutStatus::AwaitingFunding
        && at >= current.input.quote.cash.activation_deadline_ms
    {
        let mut next = current;
        next.status = CheckoutStatus::RefundCommitted;
        save(&next);
    }
    Ok(order(id)?.progress())
}

#[ic_cdk::update]
async fn check_checkout_funding(id: Hash, block: CashBlock) -> Result<CheckoutProgress> {
    let key = block_key(&block);
    if let Some(old) = BLOCKS.with_borrow(|t| t.load(key.as_slice())) {
        ensure(old == id, Error::IdempotencyConflict)?;
        return Ok(order(id)?.progress());
    }
    let ledger_asset = asset(block.ledger)?;
    ensure(ledger_asset.verified, Error::UnsupportedProtocol)?;
    let at = nanos_to_millis(ic_cdk::api::time());
    store::reserve_call(at, store::CallBudget::Funds)?;
    let mut original = order(id)?;
    ensure(at >= original.busy_until_ms, Error::Pending)?;
    original.generation = original
        .generation
        .checked_add(1)
        .ok_or(Error::QuotaExceeded)?;
    original.busy_until_ms = at + MINUTE;
    let generation = original.generation;
    save(&original);
    let tx = read_transfer(
        block.ledger,
        u64::try_from(block.block_index).map_err(|_| Error::QuotaExceeded)?,
    )
    .await;
    let at = nanos_to_millis(ic_cdk::api::time());
    let mut current = order(id)?;
    ensure(current.generation == generation, Error::VersionConflict)?;
    current.busy_until_ms = 0;
    save(&current);
    let tx = tx?;
    if let Some(old) = BLOCKS.with_borrow(|t| t.load(key.as_slice())) {
        ensure(old == id, Error::IdempotencyConflict)?;
        return Ok(current.progress());
    }
    let deposit = current.deposit(block.ledger, &tx, at)?;
    let applying = current.status == CheckoutStatus::Applying;
    ensure(current.conserved(), Error::IntegrityFailed)?;
    BLOCKS.with_borrow_mut(|t| t.put(key.as_slice(), &id));
    DEPOSITS.with_borrow_mut(|t| t.put(&deposit_key(id, &block), &deposit));
    save(&current);
    if applying {
        deliver(id, false).await?;
    }
    Ok(order(id)?.progress())
}

fn restore_refund(o: &mut Order) -> Result<()> {
    let amount = o.refund_price()?;
    if amount > 0 {
        let block = o.funding.as_ref().ok_or(Error::IntegrityFailed)?;
        let mut d = deposit(o.id, block)?;
        d.refundable_atomic = d
            .refundable_atomic
            .checked_add(amount)
            .ok_or(Error::QuotaExceeded)?;
        DEPOSITS.with_borrow_mut(|t| t.put(&deposit_key(o.id, block), &d));
    }
    Ok(())
}

async fn deliver(id: Hash, reconcile: bool) -> Result<()> {
    let o = order(id)?;
    if o.receipt.is_some() {
        return Ok(());
    }
    let decision = o.decision.clone().ok_or(Error::NotFound)?;
    let adapter = o.input.quote.product.adapter;
    let receipt = if reconcile {
        let found: Result<Option<ProductReceipt>> =
            call(adapter, "get_product_decision", (decision.decision_id,)).await?;
        found?
    } else {
        None
    };
    let receipt = if let Some(value) = receipt {
        value
    } else {
        let result: Result<ProductReceipt> =
            call(adapter, "apply_product_decision", (decision.clone(),)).await?;
        result?
    };
    let mut current = order(id)?;
    if current.receipt.is_some() {
        return Ok(());
    }
    ensure(
        current.decision.as_ref() == Some(&decision),
        Error::IdempotencyConflict,
    )?;
    current.accept(receipt)?;
    if current.status == CheckoutStatus::RefundCommitted {
        restore_refund(&mut current)?;
    }
    save(&current);
    Ok(())
}

#[ic_cdk::query]
fn get_checkout(id: Hash) -> Result<CheckoutView> {
    let o = order(id)?;
    read_access(&o, ic_cdk::api::msg_caller())?;
    Ok(o.view())
}

#[ic_cdk::query]
fn checkout_progress(id: Hash) -> Result<CheckoutProgress> {
    Ok(order(id)?.progress())
}

#[ic_cdk::query]
fn checkout_deposits(id: Hash) -> Result<Vec<CheckoutDeposit>> {
    let o = order(id)?;
    read_access(&o, ic_cdk::api::msg_caller())?;
    let start = [id.as_slice(), &[0; 32]].concat();
    let end = [id.as_slice(), &[255; 32]].concat();
    Ok(DEPOSITS.with_borrow(|t| {
        t.range(start..=end)
            .take(128)
            .map(|v| v.value().0)
            .collect()
    }))
}

#[ic_cdk::update]
async fn cancel_checkout(id: Hash) -> Result<CheckoutProgress> {
    let mut o = order(id)?;
    ensure(
        ic_cdk::api::msg_caller() == o.input.quote.cash.payer.owner,
        Error::Forbidden,
    )?;
    let at = nanos_to_millis(ic_cdk::api::time());
    match o.status {
        CheckoutStatus::AwaitingFunding | CheckoutStatus::Reserving => {
            o.status = CheckoutStatus::RefundCommitted;
            save(&o);
            release_reservation(id).await?;
        }
        CheckoutStatus::Applied => {
            ensure(at < o.input.quote.offer.starts_at_ms, Error::Forbidden)?;
            o.cancellation_pending = true;
            save(&o);
            cancel(id, false).await?;
        }
        CheckoutStatus::RefundCommitted | CheckoutStatus::Rejected => {}
        CheckoutStatus::Applying => return Err(Error::ExecutionUnknown),
    }
    Ok(order(id)?.progress())
}

async fn cancel(id: Hash, reconcile: bool) -> Result<()> {
    let o = order(id)?;
    if o.cancellation.is_some() {
        return Ok(());
    }
    let receipt = o.receipt.as_ref().ok_or(Error::NotFound)?;
    let contract_id = match receipt.outcome {
        ProductOutcome::Applied { contract_id, .. } => contract_id,
        _ => return Err(Error::Forbidden),
    };
    let decision = o.decision.as_ref().ok_or(Error::NotFound)?;
    let hash = product_decision_hash(decision);
    let found = if reconcile {
        let result: Result<Option<CashCancellationReceipt>> = call(
            o.input.quote.product.adapter,
            "get_cash_cancellation",
            (id,),
        )
        .await?;
        result?
    } else {
        None
    };
    let result = if let Some(value) = found {
        value
    } else {
        let value: Result<CashCancellationReceipt> = call(
            o.input.quote.product.adapter,
            "cancel_cash_contract",
            (id, contract_id, hash),
        )
        .await?;
        value?
    };
    let mut current = order(id)?;
    if current.cancellation.is_some() {
        return Ok(());
    }
    ensure(
        result.order_id == id && result.contract_id == contract_id && result.decision_hash == hash,
        Error::IntegrityFailed,
    )?;
    if result.cancelled {
        ensure(
            result.cancelled_at_ms < current.input.quote.offer.starts_at_ms
                && current.earned_allocated == 0,
            Error::IntegrityFailed,
        )?;
        current.status = CheckoutStatus::RefundCommitted;
        restore_refund(&mut current)?;
    }
    current.cancellation_pending = false;
    current.cancellation = Some(result);
    save(&current);
    Ok(())
}

async fn release_reservation(id: Hash) -> Result<()> {
    let o = order(id)?;
    ensure(
        matches!(
            o.status,
            CheckoutStatus::RefundCommitted | CheckoutStatus::Rejected
        ) && o.decision.is_none(),
        Error::Forbidden,
    )?;
    let result: Result<()> = call(
        o.input.quote.product.adapter,
        "release_product_billing",
        (o.input.authorization.clone(),),
    )
    .await?;
    result?;
    let mut current = order(id)?;
    current.reservation_released = true;
    save(&current);
    Ok(())
}

#[ic_cdk::update]
fn claim_checkout_refund(
    id: Hash,
    ledger: Principal,
    blocks: Vec<u128>,
    operation_id: Hash,
) -> Result<CashTransfer> {
    ensure(
        !blocks.is_empty() && blocks.len() <= 32 && blocks.windows(2).all(|w| w[0] < w[1]),
        invalid("ordered refund blocks"),
    )?;
    nonzero(operation_id.as_slice())?;
    let refund_id = digest(
        "dmsg/checkout/refund-operation/v2",
        &(id, ledger, &blocks, operation_id),
    );
    if let Ok(old) = transfer(refund_id) {
        return Ok(old.view);
    }
    let mut o = order(id)?;
    let mut deposits = Vec::new();
    let mut total = 0u128;
    let mut destination = None;
    for index in &blocks {
        let d = deposit(
            id,
            &CashBlock {
                ledger,
                block_index: *index,
            },
        )?;
        ensure(
            destination.is_none_or(|a| a == d.from),
            Error::IntegrityFailed,
        )?;
        destination = Some(d.from);
        total = total
            .checked_add(d.refundable_atomic)
            .ok_or(Error::QuotaExceeded)?;
        deposits.push(d);
    }
    let caller = ic_cdk::api::msg_caller();
    ensure(
        destination.is_some_and(|a| a.owner == caller) || caller == o.input.quote.cash.payer.owner,
        Error::Forbidden,
    )?;
    let a = asset(ledger)?;
    let at = nanos_to_millis(ic_cdk::api::time());
    store::reserve_call(at, store::CallBudget::Funds)?;
    let balance = o.balances.get_mut(&ledger).ok_or(Error::NotFound)?;
    balance.refundable = balance
        .refundable
        .checked_sub(total)
        .ok_or(Error::IntegrityFailed)?;
    let cap = if ledger == o.input.quote.cash.ledger {
        o.input.quote.cash.max_network_fee_atomic
    } else {
        a.policy.max_network_fee_atomic
    };
    let mut leg = model::transfer(
        &mut o,
        ledger,
        destination.ok_or(Error::NotFound)?,
        total,
        a.fee,
        cap,
        at,
    )?;
    // The operation id includes the exact selected blocks. Retrying cannot allocate them twice.
    leg.view.transfer_id = refund_id;
    leg.view.memo = refund_id;
    ensure(o.conserved(), Error::IntegrityFailed)?;
    for mut d in deposits {
        d.refundable_atomic = 0;
        DEPOSITS.with_borrow_mut(|t| t.put(&deposit_key(id, &d.block), &d));
    }
    save_transfer(&leg);
    save(&o);
    Ok(leg.view)
}

#[ic_cdk::update]
fn collect_checkout_revenue(id: Hash) -> Result<CashTransfer> {
    let mut o = order(id)?;
    ensure(
        ic_cdk::api::msg_caller() == o.input.quote.product.merchant.owner,
        Error::Forbidden,
    )?;
    let at = nanos_to_millis(ic_cdk::api::time());
    let earned = o.earned(at)?;
    let available = earned
        .checked_sub(o.earned_allocated)
        .ok_or(Error::IntegrityFailed)?;
    ensure(available > 0, Error::NotFound)?;
    let ledger = o.input.quote.cash.ledger;
    let a = asset(ledger)?;
    let balance = o.balances.get_mut(&ledger).ok_or(Error::NotFound)?;
    let subsidized = balance.fees.min(a.fee);
    let debit = available
        .checked_add(subsidized)
        .ok_or(Error::QuotaExceeded)?;
    balance.service = balance
        .service
        .checked_sub(available)
        .ok_or(Error::IntegrityFailed)?;
    balance.fees -= subsidized;
    let merchant = o.input.quote.product.merchant;
    let cap = o.input.quote.cash.max_network_fee_atomic;
    let leg = model::transfer(&mut o, ledger, merchant, debit, a.fee, cap, at)?;
    o.earned_allocated = earned;
    ensure(o.conserved(), Error::IntegrityFailed)?;
    save_transfer(&leg);
    save(&o);
    Ok(leg.view)
}

#[ic_cdk::update]
fn claim_checkout_fee_reserve(id: Hash) -> Result<CashTransfer> {
    let mut o = order(id)?;
    ensure(
        ic_cdk::api::msg_caller() == o.input.quote.cash.payer.owner,
        Error::Forbidden,
    )?;
    ensure(
        o.status == CheckoutStatus::Applied
            && o.earned_allocated == o.input.quote.cash.amount_atomic,
        Error::Pending,
    )?;
    let ledger = o.input.quote.cash.ledger;
    let a = asset(ledger)?;
    let balance = o.balances.get_mut(&ledger).ok_or(Error::NotFound)?;
    let total = balance.fees;
    balance.fees = 0;
    let payer = o.input.quote.cash.payer;
    let cap = o.input.quote.cash.max_network_fee_atomic;
    let leg = model::transfer(
        &mut o,
        ledger,
        payer,
        total,
        a.fee,
        cap,
        nanos_to_millis(ic_cdk::api::time()),
    )?;
    ensure(o.conserved(), Error::IntegrityFailed)?;
    save_transfer(&leg);
    save(&o);
    Ok(leg.view)
}

#[ic_cdk::query]
fn get_checkout_transfer(id: Hash) -> Result<CashTransfer> {
    let caller = ic_cdk::api::msg_caller();
    let t = transfer(id)?;
    let o = order(t.view.order_id)?;
    ensure(
        t.view.to.owner == caller || read_access(&o, caller).is_ok(),
        Error::Forbidden,
    )?;
    Ok(t.view)
}

fn complete_transfer(id: Hash, block: u128) -> Result<CashTransfer> {
    let mut current = transfer(id)?;
    if current.view.status == CashTransferStatus::Succeeded {
        return Ok(current.view);
    }
    ensure(
        current.view.status != CashTransferStatus::Superseded,
        Error::VersionConflict,
    )?;
    current.view.status = CashTransferStatus::Succeeded;
    current.view.block_index = Some(block);
    current.view.error_code = None;
    save_transfer(&current);
    Ok(current.view)
}

#[ic_cdk::update]
async fn process_checkout_transfer(id: Hash) -> Result<CashTransferProgress> {
    dispatch_transfer(id).await.map(Into::into)
}

async fn dispatch_transfer(id: Hash) -> Result<CashTransfer> {
    let at = nanos_to_millis(ic_cdk::api::time());
    store::reserve_call(at, store::CallBudget::Funds)?;
    let mut t = transfer(id)?;
    if t.view.status == CashTransferStatus::Succeeded {
        return Ok(t.view);
    }
    ensure(
        t.view.status != CashTransferStatus::Superseded,
        Error::VersionConflict,
    )?;
    let was_unknown = matches!(
        t.view.status,
        CashTransferStatus::Unknown | CashTransferStatus::InFlight
    );
    if t.view.status == CashTransferStatus::InFlight {
        ensure(
            at >= t.dispatched_at_ms.saturating_add(MINUTE),
            Error::Pending,
        )?;
    }
    t.view.status = CashTransferStatus::InFlight;
    t.dispatched_at_ms = at;
    save_transfer(&t);
    let v = &t.view;
    let result: std::result::Result<
        std::result::Result<Nat, TransferError>,
        dmsg_runtime::CallFailure,
    > = call_classified(
        v.ledger,
        "icrc1_transfer",
        (TransferArg {
            from_subaccount: Some(v.source_subaccount.into_array()),
            to: v.to,
            fee: Some(v.fee_atomic.into()),
            created_at_time: Some(v.created_at_time_ns),
            memo: Some(v.memo.to_vec().into()),
            amount: v.amount_atomic.into(),
        },),
    )
    .await;
    let mut current = transfer(id)?;
    if current.view.status == CashTransferStatus::Succeeded {
        return Ok(current.view);
    }
    ensure(
        current.view.status != CashTransferStatus::Superseded,
        Error::VersionConflict,
    )?;
    match result {
        Ok(Ok(index)) => complete_transfer(id, u128::from(block_index(index)?)),
        Ok(Err(TransferError::Duplicate { duplicate_of })) => {
            complete_transfer(id, u128::from(block_index(duplicate_of)?))
        }
        Ok(Err(error)) => {
            if let TransferError::BadFee { expected_fee } = error {
                let fee = token_amount(expected_fee)?;
                ensure(fee > 0, Error::IntegrityFailed)?;
                let mut a = asset(v.ledger)?;
                a.fee = fee;
                ASSETS.with_borrow_mut(|m| m.put(v.ledger.as_slice(), &a));
                publish_assets();
                current.view.expected_fee_atomic = Some(fee);
                current.view.error_code = Some("BadFee".into());
            } else {
                current.view.error_code = Some(
                    match error {
                        TransferError::TooOld => "TooOld",
                        TransferError::TemporarilyUnavailable => "TemporarilyUnavailable",
                        TransferError::InsufficientFunds { .. } => "InsufficientFunds",
                        _ => "LedgerRejected",
                    }
                    .into(),
                );
            }
            current.view.status = if was_unknown {
                CashTransferStatus::Unknown
            } else {
                CashTransferStatus::Rejected
            };
            save_transfer(&current);
            Ok(current.view)
        }
        Err(failure) => {
            let unknown = failure.preserves_unknown(was_unknown);
            current.view.status = if unknown {
                CashTransferStatus::Unknown
            } else {
                CashTransferStatus::Rejected
            };
            current.view.error_code = Some(
                if unknown {
                    "ExecutionUnknown"
                } else {
                    "NotExecuted"
                }
                .into(),
            );
            save_transfer(&current);
            Err(if unknown {
                Error::ExecutionUnknown
            } else {
                failure.into()
            })
        }
    }
}

#[ic_cdk::update]
async fn reconcile_checkout_transfer(id: Hash, block: CashBlock) -> Result<CashTransferProgress> {
    verify_transfer(id, block).await.map(Into::into)
}

async fn verify_transfer(id: Hash, block: CashBlock) -> Result<CashTransfer> {
    let t = transfer(id)?;
    ensure(block.ledger == t.view.ledger, Error::Forbidden)?;
    if t.view.status == CashTransferStatus::Succeeded {
        return Ok(t.view);
    }
    ensure(
        matches!(
            t.view.status,
            CashTransferStatus::InFlight | CashTransferStatus::Unknown
        ),
        Error::VersionConflict,
    )?;
    let at = nanos_to_millis(ic_cdk::api::time());
    store::reserve_call(at, store::CallBudget::Funds)?;
    let tx = read_transfer(
        block.ledger,
        u64::try_from(block.block_index).map_err(|_| Error::QuotaExceeded)?,
    )
    .await?;
    let v = &t.view;
    ensure(
        tx.from
            == Account {
                owner: ic_cdk::api::canister_self(),
                subaccount: Some(v.source_subaccount.into_array()),
            }
            && tx.to == v.to
            && tx.amount == v.amount_atomic
            && tx.fee == Some(v.fee_atomic)
            && tx.memo == Some(v.memo.to_vec())
            && tx.created_at_time == Some(v.created_at_time_ns),
        Error::IntegrityFailed,
    )?;
    complete_transfer(id, block.block_index)
}

#[ic_cdk::update]
fn revise_checkout_transfer_fee(id: Hash, new_fee: u128) -> Result<CashTransfer> {
    let mut old = transfer(id)?;
    ensure(
        old.view.status == CashTransferStatus::Rejected && old.view.replaced_by.is_none(),
        Error::VersionConflict,
    )?;
    ensure(
        ic_cdk::api::msg_caller() == old.view.to.owner,
        Error::Forbidden,
    )?;
    let total = old
        .view
        .amount_atomic
        .checked_add(old.view.fee_atomic)
        .ok_or(Error::QuotaExceeded)?;
    ensure(
        new_fee > 0
            && new_fee <= old.view.max_fee_atomic
            && new_fee < total
            && old.view.expected_fee_atomic == Some(new_fee),
        Error::FeeBlocked,
    )?;
    let mut o = order(old.view.order_id)?;
    let next = o.next_transfer.checked_add(1).ok_or(Error::QuotaExceeded)?;
    let new_id = digest("dmsg/checkout/transfer/v2", &(o.id, o.next_transfer));
    let mut t = old.clone();
    t.view.transfer_id = new_id;
    t.view.memo = new_id;
    t.view.fee_atomic = new_fee;
    t.view.amount_atomic = total - new_fee;
    t.view.created_at_time_ns = millis_to_nanos(nanos_to_millis(ic_cdk::api::time()))?;
    t.view.status = CashTransferStatus::Pending;
    t.view.replaces = Some(id);
    t.view.replaced_by = None;
    t.view.error_code = None;
    t.view.expected_fee_atomic = None;
    t.dispatched_at_ms = 0;
    old.view.replaced_by = Some(new_id);
    old.view.status = CashTransferStatus::Superseded;
    o.next_transfer = next;
    // The original obligation is already allocated; its total debit is unchanged.
    save_transfer(&t);
    save_transfer(&old);
    save(&o);
    Ok(t.view)
}

fn order_key(id: Hash) -> Vec<u8> {
    digest("dmsg/checkout/certificate/v2", &id).to_vec()
}

fn transfer_key(id: Hash) -> Vec<u8> {
    digest("dmsg/checkout/transfer-certificate/v2", &id).to_vec()
}

fn assets_key() -> Vec<u8> {
    digest("dmsg/settlement-assets/v2", &"supported").to_vec()
}

fn publish_assets() {
    let views = settlement_assets();
    store::CERT.with_borrow_mut(|c| c.put(assets_key(), &views));
}

#[ic_cdk::query]
fn settlement_assets_certificate() -> Result<CertifiedBatch> {
    store::CERT.with_borrow(|c| c.batch(ic_cdk::api::canister_self(), vec![assets_key()]))
}

#[ic_cdk::query]
fn checkout_certificate(id: Hash) -> Result<CertifiedBatch> {
    let o = order(id)?;
    read_access(&o, ic_cdk::api::msg_caller())?;
    store::CERT.with_borrow(|c| c.batch(ic_cdk::api::canister_self(), vec![order_key(id)]))
}

#[ic_cdk::query]
fn checkout_transfer_certificate(id: Hash) -> Result<CertifiedBatch> {
    get_checkout_transfer(id)?;
    store::CERT.with_borrow(|c| c.batch(ic_cdk::api::canister_self(), vec![transfer_key(id)]))
}

pub(crate) fn rebuild(cert: &mut dmsg_runtime::Certification) {
    ORDERS.with_borrow(|t| {
        t.for_each(|_, o| {
            cert.0.insert(order_key(o.id), canonical(&o.view()));
        })
    });
    TRANSFERS.with_borrow(|t| {
        t.for_each(|_, v| {
            cert.0
                .insert(transfer_key(v.view.transfer_id), canonical(&v.view));
        })
    });
    cert.0.insert(assets_key(), canonical(&settlement_assets()));
}

#[ic_cdk::update]
fn set_settlement_price_authority(authority: Principal) -> Result<()> {
    ensure(
        ic_cdk::api::msg_caller() == store::config().init.governance,
        Error::Forbidden,
    )?;
    authenticated(authority)?;
    PRICE_AUTHORITY.with_borrow_mut(|c| c.set(Stored(Some(authority))));
    Ok(())
}

#[ic_cdk::update]
fn publish_settlement_price(
    ledger: Principal,
    price_usd_micros: u128,
    valid_for_ms: u64,
) -> Result<SettlementAsset> {
    let caller = ic_cdk::api::msg_caller();
    ensure(
        caller == store::config().init.governance
            || PRICE_AUTHORITY.with_borrow(|c| c.get().0 == Some(caller)),
        Error::Forbidden,
    )?;
    ensure(
        price_usd_micros > 0 && valid_for_ms > 0 && valid_for_ms <= PRICE_WINDOW_MS,
        invalid("price observation"),
    )?;
    let at = nanos_to_millis(ic_cdk::api::time());
    let mut a = asset(ledger)?;
    a.policy.policy_version = a
        .policy
        .policy_version
        .checked_add(1)
        .ok_or(Error::QuotaExceeded)?;
    a.policy.price_usd_micros = price_usd_micros;
    a.policy.price_observed_at_ms = at;
    a.policy.price_valid_until_ms = at.checked_add(valid_for_ms).ok_or(Error::QuotaExceeded)?;
    ASSETS.with_borrow_mut(|t| t.put(ledger.as_slice(), &a));
    publish_assets();
    Ok(a.policy)
}

/// Consensus read for the pinned product adapter during Apply. Query replies are not delivery authority.
#[ic_cdk::update]
fn get_checkout_for_product(id: Hash) -> Result<CheckoutView> {
    let value = order(id)?;
    ensure(
        ic_cdk::api::msg_caller() == value.input.quote.product.adapter,
        Error::Forbidden,
    )?;
    Ok(value.view())
}

/// Private operations centre; quoting and monitoring never reserve an interval.
#[ic_cdk::query]
fn checkout_operations(after: Option<Hash>, take: u16) -> Result<CheckoutOperationsPage> {
    use std::ops::Bound::{Excluded, Unbounded};
    let caller = ic_cdk::api::msg_caller();
    authenticated(caller)?;
    ensure((1..=32).contains(&take), invalid("page size"))?;
    let mut orders = vec![];
    let mut next = None;
    ORDERS.with_borrow(|t| {
        let start = after.map_or(Unbounded, |id| Excluded(id.to_vec()));
        let mut iter = t.range((start, Unbounded));
        for _ in 0..512 {
            let Some(row) = iter.next() else {
                next = None;
                break;
            };
            let order = row.value().0;
            next = Some(order.id);
            if read_access(&order, caller).is_ok() {
                orders.push(CheckoutOperationAudit {
                    order: order.view(),
                    balances: order
                        .balances
                        .iter()
                        .map(|(ledger, b)| CheckoutLedgerBalance {
                            ledger: *ledger,
                            incoming_atomic: b.incoming,
                            refundable_atomic: b.refundable,
                            service_reserve_atomic: b.service,
                            fee_reserve_atomic: b.fees,
                            outgoing_atomic: b.outgoing,
                        })
                        .collect(),
                });
            }
            if orders.len() >= usize::from(take) {
                break;
            }
        }
    });
    Ok(CheckoutOperationsPage { orders, next })
}

#[ic_cdk::query]
fn checkout_transfers(after: Option<Hash>, take: u16) -> Result<CashTransfersPage> {
    use std::ops::Bound::{Excluded, Unbounded};
    let caller = ic_cdk::api::msg_caller();
    authenticated(caller)?;
    ensure((1..=32).contains(&take), invalid("page size"))?;
    let mut transfers = vec![];
    let mut next = None;
    TRANSFERS.with_borrow(|t| {
        let start = after.map_or(Unbounded, |id| Excluded(id.to_vec()));
        let mut iter = t.range((start, Unbounded));
        for _ in 0..512 {
            let Some(row) = iter.next() else {
                next = None;
                break;
            };
            let v = row.value().0.view;
            next = Some(v.transfer_id);
            if v.to.owner == caller
                || order(v.order_id).is_ok_and(|o| read_access(&o, caller).is_ok())
            {
                transfers.push(v);
            }
            if transfers.len() >= usize::from(take) {
                break;
            }
        }
    });
    Ok(CashTransfersPage { transfers, next })
}
