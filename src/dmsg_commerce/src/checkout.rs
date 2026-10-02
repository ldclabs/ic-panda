//! Generic v2 merchant checkout. A product adapter owns delivery; only this book owns funds.
use crate::{
    api::now,
    calls::CallGuard,
    checkout_model::{self as model, Order},
    checkout_store::*,
    registrations, store,
};
use candid::{CandidType, Nat, Principal};
use dmsg_protocol::{commerce_v2::*, integration::*, *};
use dmsg_runtime::{
    call, call_classified,
    ledger::{read_transfer, token_amount},
};
use dmsg_types::{integration::*, integration_billing::*, *};
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{TransferArg, TransferError},
};
use serde::Deserialize;

fn configuration(offer: &BillingOffer) -> Result<(AppRegistration, ProductRegistration)> {
    registrations::product_configuration(&offer.app_id, &offer.product_id)
}

#[ic_cdk::update]
fn register_settlement_asset(policy: SettlementAsset) -> Result<()> {
    store::check_governance(ic_cdk::api::msg_caller())?;
    validate_asset(&policy)?;
    ensure(
        store::config(|c| c.environment == policy.environment),
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
            policy.policy_version == 1 && assets().len() < 2,
            Error::QuotaExceeded,
        )?;
        ensure(
            assets().iter().all(|a| a.policy.asset != policy.asset),
            Error::IdempotencyConflict,
        )?;
    }
    let verified = old.as_ref().is_some_and(|o| o.verified);
    let fee = old.as_ref().map_or(policy.network_fee_atomic, |o| o.fee);
    save_asset(
        &Asset {
            fee,
            policy,
            verified,
        },
        now(),
    )
}

#[derive(CandidType, Deserialize)]
struct Standard {
    name: String,
    url: String,
}

#[ic_cdk::update]
async fn verify_settlement_asset(ledger: Principal, sample_transfer: Option<u128>) -> Result<()> {
    store::check_governance(ic_cdk::api::msg_caller())?;
    let previous = asset(ledger)?;
    if previous.policy.environment != Environment::Local {
        ensure_valid(
            sample_transfer.is_some(),
            "a verified transfer block is required",
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
    save_asset(&current, now())
}

#[ic_cdk::query]
pub(crate) fn settlement_assets() -> Vec<SettlementAssetView> {
    asset_views()
}

fn quotable_asset(ledger: Principal) -> Result<Asset> {
    let a = asset(ledger)?;
    ensure(
        a.verified && a.fee == a.policy.network_fee_atomic,
        Error::Unavailable("ledger/fee is not verified".into()),
    )?;
    Ok(a)
}

#[ic_cdk::update]
async fn quote_checkout(
    offer: BillingOffer,
    ledger: Principal,
    payer: Account,
) -> Result<CheckoutQuote> {
    let (app, product) = configuration(&offer)?;
    let at = now();
    validate_billing_offer(&offer, &app, &product, at)?;
    ensure(!store::config(|c| c.paused), Error::Locked)?;
    quotable_asset(ledger)?;
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
    let at = now();
    let (current, registration) = configuration(&offer)?;
    ensure(
        current == app && registration == product && !store::config(|c| c.paused),
        Error::PolicyStale,
    )?;
    let a = quotable_asset(ledger)?;
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

/// Returns the time read after the authority replies.
async fn authorize(
    input: &OpenCheckout,
    caller: Principal,
    home: Principal,
    at: u64,
) -> Result<u64> {
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
    let a = check_price(&quote.asset, at)?;
    ensure(
        a.verified && a.fee == a.policy.network_fee_atomic && !store::config(|c| c.paused),
        Error::PolicyStale,
    )?;
    // The product and account authorities are independent; ask both at once.
    let (product_auth, approved) = futures::future::join(
        call::<_, Result<ProductAuthorization>>(
            product.beneficiary_authority,
            "authorize_product_billing",
            (auth.clone(),),
        ),
        call::<_, Result<ApplicationAuthorization>>(
            auth.user_home,
            "verify_application_authorization",
            (auth.approval_id, auth.account_approval.clone()),
        ),
    )
    .await;
    let product_auth = product_auth??;
    let approved = approved??;
    let at = now();
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
            && !store::config(|c| c.paused),
        Error::PolicyStale,
    )?;
    let a = check_price(&quote.asset, at)?;
    ensure(
        a.verified && a.fee == a.policy.network_fee_atomic,
        Error::PolicyStale,
    )?;
    Ok(at)
}

#[ic_cdk::update]
async fn open_checkout(input: OpenCheckout) -> Result<CheckoutView> {
    let caller = ic_cdk::api::msg_caller();
    let home = ic_cdk::api::canister_self();
    authenticated(caller)?;
    let id = checkout_id(home, &input.quote.offer);
    if let Ok(old) = order(id) {
        read_access(&old, caller)?;
        ensure(old.matches_input(&input), Error::IdempotencyConflict)?;
        return Ok(old.view());
    }
    let at = now();
    ensure(canonical(&input).len() <= MAX_PAYLOAD, Error::QuotaExceeded)?;
    check_capacity()?;
    store::reserve_call(at, store::CallBudget::Authorization(caller))?;
    let at = authorize(&input, caller, home, at).await?;
    if let Ok(old) = order(id) {
        ensure(old.matches_input(&input), Error::IdempotencyConflict)?;
        return Ok(old.view());
    }
    check_capacity()?;
    store::reserve_order(at)?;
    let mut o = Order::new(home, input);
    save(&mut o, at);
    reserve(id, caller, at).await?;
    Ok(order(id)?.view())
}

async fn reserve(id: Hash, caller: Principal, at: u64) -> Result<()> {
    let old = order(id)?;
    if old.status != CheckoutStatus::Reserving {
        return Ok(());
    }
    let _call = CallGuard::product(id)?;
    store::reserve_call(at, store::CallBudget::Funds(caller))?;
    let result: Result<Result<()>> = call(
        old.quote.product.adapter,
        "reserve_product_billing",
        (
            old.authorization.clone().ok_or(Error::IntegrityFailed)?,
            old.quote.cash.activation_deadline_ms,
        ),
    )
    .await;
    let at = now();
    let mut current = order(id)?;
    if current.status != CheckoutStatus::Reserving {
        return Ok(());
    }
    match result {
        Ok(Ok(())) => {
            current.status = if at < current.quote.cash.funding_deadline_ms {
                CheckoutStatus::AwaitingFunding
            } else {
                CheckoutStatus::RefundCommitted
            }
        }
        // The adapter could not reach its own authority; the reservation may still succeed.
        Ok(Err(e @ (Error::Unavailable(_) | Error::ExecutionUnknown))) => return Err(e),
        Ok(Err(_)) => current.status = CheckoutStatus::Rejected,
        Err(_) => return Err(Error::ExecutionUnknown),
    }
    save(&mut current, at);
    Ok(())
}

/// Safe progress can be advanced without revealing the payer or private bill.
#[ic_cdk::update]
async fn reconcile_checkout(id: Hash) -> Result<CheckoutProgress> {
    let caller = ic_cdk::api::msg_caller();
    let at = now();
    let mut current = order(id)?;
    if current.status == CheckoutStatus::AwaitingFunding
        && at >= current.quote.cash.activation_deadline_ms
    {
        current.status = CheckoutStatus::RefundCommitted;
        save(&mut current, at);
    }
    let release = matches!(
        current.status,
        CheckoutStatus::RefundCommitted | CheckoutStatus::Rejected
    ) && current.decision.is_none()
        && !current.reservation_released;
    // Only an actual adapter call consumes the shared call budget.
    if current.status == CheckoutStatus::Reserving
        || current.status == CheckoutStatus::Applying
        || current.cancellation_pending
        || release
    {
        if current.status == CheckoutStatus::Reserving {
            reserve(id, caller, at).await?;
        } else if current.status == CheckoutStatus::Applying {
            deliver(id, true, caller, at).await?;
        } else if current.cancellation_pending {
            cancel(id, true, caller, at).await?;
        } else {
            release_reservation(id, caller, at).await?;
        }
    }
    Ok(order(id)?.progress())
}

#[ic_cdk::update]
async fn check_checkout_funding(id: Hash, block: CashBlock) -> Result<CheckoutProgress> {
    let caller = ic_cdk::api::msg_caller();
    if let Some(old) = block_owner(&block) {
        ensure(old == id, Error::IdempotencyConflict)?;
        return Ok(order(id)?.progress());
    }
    order(id)?;
    ensure(asset(block.ledger)?.verified, Error::UnsupportedProtocol)?;
    let index = u64::try_from(block.block_index).map_err(|_| Error::QuotaExceeded)?;
    let _call = CallGuard::funding(id)?;
    store::reserve_call(now(), store::CallBudget::Funds(caller))?;
    let tx = read_transfer(block.ledger, index).await?;
    let at = now();
    let mut current = order(id)?;
    if let Some(old) = block_owner(&block) {
        ensure(old == id, Error::IdempotencyConflict)?;
        return Ok(current.progress());
    }

    let deposit = current.deposit(block.ledger, &tx, at)?;
    let applying = current.status == CheckoutStatus::Applying;
    save_deposit(&deposit);
    save(&mut current, at);
    if applying {
        deliver(id, false, caller, at).await?;
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
        save_deposit(&d);
    }
    Ok(())
}

async fn deliver(id: Hash, reconcile: bool, caller: Principal, mut at: u64) -> Result<()> {
    let o = order(id)?;
    if o.receipt.is_some() {
        return Ok(());
    }
    let _call = CallGuard::product(id)?;
    let decision = o.decision.clone().ok_or(Error::NotFound)?;
    let adapter = o.quote.product.adapter;
    let receipt = if reconcile {
        store::reserve_call(at, store::CallBudget::Funds(caller))?;
        let found: Result<Option<ProductReceipt>> =
            call(adapter, "get_product_decision", (decision.decision_id,)).await?;
        at = now();
        found?
    } else {
        None
    };
    let receipt = if let Some(value) = receipt {
        value
    } else {
        store::reserve_call(at, store::CallBudget::Funds(caller))?;
        let result: Result<ProductReceipt> =
            call(adapter, "apply_product_decision", (decision.clone(),)).await?;
        at = now();
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
    save(&mut current, at);
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
fn checkout_deposits(id: Hash, after: Option<Hash>, take: u16) -> Result<CheckoutDepositsPage> {
    read_access(&order(id)?, ic_cdk::api::msg_caller())?;
    deposits(id, after, take)
}

#[ic_cdk::update]
async fn cancel_checkout(id: Hash) -> Result<CheckoutProgress> {
    let caller = ic_cdk::api::msg_caller();
    let mut o = order(id)?;
    ensure(caller == o.quote.cash.payer.owner, Error::Forbidden)?;
    let at = now();
    match o.status {
        CheckoutStatus::AwaitingFunding | CheckoutStatus::Reserving => {
            o.status = CheckoutStatus::RefundCommitted;
            save(&mut o, at);
            release_reservation(id, caller, at).await?;
        }
        CheckoutStatus::Applied => {
            if o.begin_cancellation(at)? {
                save_state(&o);
                cancel(id, false, caller, at).await?;
            }
        }
        CheckoutStatus::RefundCommitted | CheckoutStatus::Rejected => {}
        CheckoutStatus::Applying => return Err(Error::ExecutionUnknown),
    }
    Ok(order(id)?.progress())
}

async fn cancel(id: Hash, reconcile: bool, caller: Principal, mut at: u64) -> Result<()> {
    let o = order(id)?;
    if o.cancellation.is_some() {
        return Ok(());
    }
    let _call = CallGuard::product(id)?;
    let receipt = o.receipt.as_ref().ok_or(Error::NotFound)?;
    let contract_id = match receipt.outcome {
        ProductOutcome::Applied { contract_id, .. } => contract_id,
        _ => return Err(Error::Forbidden),
    };
    let decision = o.decision.as_ref().ok_or(Error::NotFound)?;
    let hash = product_decision_hash(decision);
    let found = if reconcile {
        store::reserve_call(at, store::CallBudget::Funds(caller))?;
        let result: Result<Option<CashCancellationReceipt>> =
            call(o.quote.product.adapter, "get_cash_cancellation", (id,)).await?;
        at = now();
        result?
    } else {
        None
    };
    let result = if let Some(value) = found {
        value
    } else {
        store::reserve_call(at, store::CallBudget::Funds(caller))?;
        let value: Result<CashCancellationReceipt> = call(
            o.quote.product.adapter,
            "cancel_cash_contract",
            (id, contract_id, hash),
        )
        .await?;
        at = now();
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
            result.cancelled_at_ms < current.quote.offer.starts_at_ms
                && current.earned_allocated == 0,
            Error::IntegrityFailed,
        )?;
        current.status = CheckoutStatus::RefundCommitted;
        restore_refund(&mut current)?;
    }
    current.cancellation_pending = false;
    current.cancellation = Some(result);
    save(&mut current, at);
    Ok(())
}

async fn release_reservation(id: Hash, caller: Principal, at: u64) -> Result<()> {
    let o = order(id)?;
    ensure(
        matches!(
            o.status,
            CheckoutStatus::RefundCommitted | CheckoutStatus::Rejected
        ) && o.decision.is_none(),
        Error::Forbidden,
    )?;
    if o.reservation_released {
        return Ok(());
    }
    let _call = CallGuard::product(id)?;
    store::reserve_call(at, store::CallBudget::Funds(caller))?;
    let result: Result<()> = call(
        o.quote.product.adapter,
        "release_product_billing",
        (o.authorization.clone().ok_or(Error::IntegrityFailed)?,),
    )
    .await?;
    result?;
    let at = now();
    let mut current = order(id)?;
    current.reservation_released = true;
    save(&mut current, at);
    Ok(())
}

#[ic_cdk::update]
fn claim_checkout_refund(
    id: Hash,
    ledger: Principal,
    blocks: Vec<u128>,
    operation_id: Hash,
) -> Result<CashTransfer> {
    ensure_valid(
        !blocks.is_empty() && blocks.len() <= 32 && blocks.windows(2).all(|w| w[0] < w[1]),
        "ordered refund blocks",
    )?;
    nonzero(operation_id.as_slice())?;
    let refund_id = digest(
        "dmsg/checkout/refund-operation/v2",
        &(id, ledger, &blocks, operation_id),
    );
    let caller = ic_cdk::api::msg_caller();
    if let Ok(old) = transfer(refund_id) {
        ensure(
            old.view.to.owner == caller || read_access(&order(id)?, caller).is_ok(),
            Error::Forbidden,
        )?;
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
    ensure(
        destination.is_some_and(|a| a.owner == caller) || caller == o.quote.cash.payer.owner,
        Error::Forbidden,
    )?;
    let a = asset(ledger)?;
    let at = now();
    let balance = o.balances.get_mut(&ledger).ok_or(Error::NotFound)?;
    balance.refundable = balance
        .refundable
        .checked_sub(total)
        .ok_or(Error::IntegrityFailed)?;
    let cap = if ledger == o.quote.cash.ledger {
        o.quote.cash.max_network_fee_atomic
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
        save_deposit(&d);
    }
    save_transfer(&mut leg, at);
    save(&mut o, at);
    Ok(leg.view)
}

#[ic_cdk::update]
fn collect_checkout_revenue(id: Hash) -> Result<CashTransfer> {
    let mut o = order(id)?;
    ensure(
        ic_cdk::api::msg_caller() == o.quote.product.merchant.owner,
        Error::Forbidden,
    )?;
    let at = now();
    let earned = o.earned(at)?;
    let available = earned
        .checked_sub(o.earned_allocated)
        .ok_or(Error::IntegrityFailed)?;
    ensure(available > 0, Error::NotFound)?;
    let ledger = o.quote.cash.ledger;
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
    let merchant = o.quote.product.merchant;
    let cap = o.quote.cash.max_network_fee_atomic;
    let mut leg = model::transfer(&mut o, ledger, merchant, debit, a.fee, cap, at)?;
    o.earned_allocated = earned;
    ensure(o.conserved(), Error::IntegrityFailed)?;
    save_transfer(&mut leg, at);
    save(&mut o, at);
    Ok(leg.view)
}

#[ic_cdk::update]
fn claim_checkout_fee_reserve(id: Hash) -> Result<CashTransfer> {
    let at = now();
    let mut o = order(id)?;
    ensure(
        ic_cdk::api::msg_caller() == o.quote.cash.payer.owner,
        Error::Forbidden,
    )?;
    ensure(
        o.status == CheckoutStatus::Applied && o.earned_allocated == o.quote.cash.amount_atomic,
        Error::Pending,
    )?;
    let ledger = o.quote.cash.ledger;
    let a = asset(ledger)?;
    let balance = o.balances.get_mut(&ledger).ok_or(Error::NotFound)?;
    let total = balance.fees;
    balance.fees = 0;
    let payer = o.quote.cash.payer;
    let cap = o.quote.cash.max_network_fee_atomic;
    let mut leg = model::transfer(&mut o, ledger, payer, total, a.fee, cap, at)?;
    ensure(o.conserved(), Error::IntegrityFailed)?;
    save_transfer(&mut leg, at);
    save(&mut o, at);
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

fn complete_transfer(id: Hash, block: u128, at: u64) -> Result<CashTransfer> {
    let mut current = transfer(id)?;
    if current.view.status == CashTransferStatus::Succeeded {
        return Ok(current.view);
    }
    ensure(
        current.view.status != CashTransferStatus::Superseded,
        Error::VersionConflict,
    )?;
    let mut o = order(current.view.order_id)?;
    o.pending_transfers = o
        .pending_transfers
        .checked_sub(1)
        .ok_or(Error::IntegrityFailed)?;
    current.view.status = CashTransferStatus::Succeeded;
    current.view.block_index = Some(block);
    current.view.error_code = None;
    current.view.expected_fee_atomic = None;
    save_transfer(&mut current, at);
    save(&mut o, at);
    Ok(current.view)
}

#[ic_cdk::update]
async fn process_checkout_transfer(id: Hash) -> Result<CashTransferProgress> {
    dispatch_transfer(id).await.map(Into::into)
}

async fn dispatch_transfer(id: Hash) -> Result<CashTransfer> {
    let at = now();
    let mut t = transfer(id)?;
    if t.view.status == CashTransferStatus::Succeeded {
        return Ok(t.view);
    }
    ensure(
        t.view.status != CashTransferStatus::Superseded,
        Error::VersionConflict,
    )?;
    let _call = CallGuard::transfer(id)?;
    let was_unknown = matches!(
        t.view.status,
        CashTransferStatus::Unknown | CashTransferStatus::InFlight
    );
    store::reserve_call(at, store::CallBudget::Funds(ic_cdk::api::msg_caller()))?;
    t.view.status = CashTransferStatus::InFlight;
    save_transfer(&mut t, at);
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
    let at = now();
    let mut current = transfer(id)?;
    let outcome = model::transfer_result(&mut current, was_unknown, result);
    if let Ok(Some(block)) = outcome {
        return complete_transfer(id, block, at);
    }
    if let Some(fee) = current.view.expected_fee_atomic {
        observe_fee(current.view.ledger, fee)?;
    }
    save_transfer(&mut current, at);
    outcome.map(|_| current.view)
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
    let _call = CallGuard::transfer(id)?;
    store::reserve_call(now(), store::CallBudget::Funds(ic_cdk::api::msg_caller()))?;
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
    complete_transfer(id, block.block_index, now())
}

/// The recipient may reissue a definitely rejected leg with a fresh ledger timestamp,
/// keeping its fee or adopting the ledger's expected fee within the original cap.
#[ic_cdk::update]
fn revise_checkout_transfer_fee(id: Hash, new_fee: u128) -> Result<CashTransfer> {
    let at = now();
    let _call = CallGuard::transfer(id)?;
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
            && (new_fee == old.view.fee_atomic || old.view.expected_fee_atomic == Some(new_fee)),
        Error::FeeBlocked,
    )?;
    let mut o = order(old.view.order_id)?;
    let new_id = o.next_transfer_id()?;
    let mut t = old.clone();
    t.view.transfer_id = new_id;
    t.view.memo = new_id;
    t.view.fee_atomic = new_fee;
    t.view.amount_atomic = total - new_fee;
    t.view.created_at_time_ns = millis_to_nanos(at)?;
    t.view.status = CashTransferStatus::Pending;
    t.view.replaces = Some(id);
    t.view.replaced_by = None;
    t.view.error_code = None;
    t.view.expected_fee_atomic = None;
    old.view.replaced_by = Some(new_id);
    old.view.status = CashTransferStatus::Superseded;
    // The original obligation is already allocated; its total debit is unchanged.
    save_transfer(&mut t, at);
    save_transfer(&mut old, at);
    save_state(&o);
    Ok(t.view)
}

#[ic_cdk::query]
fn settlement_assets_certificate() -> Result<CertifiedBatch> {
    store::CERT.with_borrow(|c| c.batch(ic_cdk::api::canister_self(), vec![assets_key()]))
}

#[ic_cdk::query]
fn checkout_certificate(id: Hash) -> Result<CertifiedBatch> {
    let o = order(id)?;
    read_access(&o, ic_cdk::api::msg_caller())?;
    order_certificate_available(id)?;
    store::CERT.with_borrow(|c| c.batch(ic_cdk::api::canister_self(), vec![order_key(id)]))
}

#[ic_cdk::query]
fn checkout_transfer_certificate(id: Hash) -> Result<CertifiedBatch> {
    get_checkout_transfer(id)?;
    transfer_certificate_available(id)?;
    store::CERT.with_borrow(|c| c.batch(ic_cdk::api::canister_self(), vec![transfer_key(id)]))
}

#[ic_cdk::update]
fn set_settlement_price_authority(authority: Principal) -> Result<()> {
    store::check_governance(ic_cdk::api::msg_caller())?;
    authenticated(authority)?;
    set_price_authority(authority);
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
        caller == store::config(|c| c.governance) || price_authority() == Some(caller),
        Error::Forbidden,
    )?;
    ensure_valid(
        price_usd_micros > 0 && valid_for_ms > 0 && valid_for_ms <= PRICE_WINDOW_MS,
        "price observation",
    )?;
    let at = now();
    let mut a = asset(ledger)?;
    a.policy.policy_version = a
        .policy
        .policy_version
        .checked_add(1)
        .ok_or(Error::QuotaExceeded)?;
    a.policy.price_usd_micros = price_usd_micros;
    a.policy.price_observed_at_ms = at;
    a.policy.price_valid_until_ms = at.checked_add(valid_for_ms).ok_or(Error::QuotaExceeded)?;
    save_asset(&a, at)?;
    Ok(a.policy)
}

/// Consensus read for the pinned product adapter during Apply. Query replies are not delivery authority.
#[ic_cdk::update]
fn get_checkout_for_product(id: Hash) -> Result<CheckoutView> {
    let value = order(id)?;
    ensure(
        ic_cdk::api::msg_caller() == value.quote.product.adapter,
        Error::Forbidden,
    )?;
    Ok(value.view())
}

/// Private operations centre; quoting and monitoring never reserve an interval.
#[ic_cdk::query]
fn checkout_operations(after: Option<Hash>, take: u16) -> Result<CheckoutOperationsPage> {
    operations(ic_cdk::api::msg_caller(), after, take)
}

#[ic_cdk::query]
fn checkout_transfers(after: Option<Hash>, take: u16) -> Result<CashTransfersPage> {
    transfers(ic_cdk::api::msg_caller(), after, take)
}

/// Bounded maintenance; funds, original-source refunds and idempotency survive archival.
#[ic_cdk::update]
fn sweep_checkout_history() -> CheckoutHistorySweep {
    sweep(now())
}
