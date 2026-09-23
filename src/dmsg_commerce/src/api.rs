use crate::{
    model::{self, Subject},
    store::*,
};
use candid::{Nat, Principal};
use dmsg_protocol::{billing::*, membership::*, *};
use dmsg_runtime::storage::MapExt;
use dmsg_types::{billing::*, membership::*, *};
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{TransferArg, TransferError},
};

fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}

fn governance() -> Result<()> {
    ensure(
        ic_cdk::api::msg_caller() == config().init.governance,
        Error::Forbidden,
    )
}

fn valid_subject(b: &Beneficiary) -> Result<()> {
    beneficiary_account(b)?;
    ensure(
        config().init.user_homes.contains(&b.authority_canister),
        Error::Forbidden,
    )
}

fn get_subject(b: &Beneficiary) -> Result<Subject> {
    valid_subject(b)?;
    match load(b) {
        Ok(s) => Ok(s),
        Err(Error::NotFound) => {
            ensure(
                SUBJECTS.with_borrow(|t| t.len()) < config().init.max_subjects,
                Error::QuotaExceeded,
            )?;
            Ok(Subject::new(b.clone()))
        }
        Err(e) => Err(e),
    }
}

fn publish(canister_id: Principal, s: &mut Subject, at: u64) -> Result<EntitlementView> {
    let v = model::project(canister_id, s, &catalog(at), at)?;
    save(s);
    Ok(v)
}

/// Returns the verified authorization and the local callback timestamp.
async fn authorization(
    i: &MembershipIntent,
    canister_id: Principal,
) -> Result<(MembershipAuthorization, u64)> {
    valid_subject(&i.beneficiary)?;
    ensure(
        i.environment == config().init.environment
            && [canister_id, config().init.membership_canister].contains(&i.service_canister),
        Error::Forbidden,
    )?;
    let result: Result<MembershipAuthorization> = dmsg_runtime::call(
        i.beneficiary.authority_canister,
        "verify_membership_authorization",
        (i.clone(),),
    )
    .await?;
    let a = result?;
    let at = now();
    ensure(
        a.intent_digest == membership_intent_digest(i)
            && a.verified_at_ms <= at
            && at - a.verified_at_ms <= MINUTE
            && at < a.valid_until_ms,
        Error::PolicyStale,
    )?;
    Ok((a, at))
}

#[ic_cdk::init]
fn init(args: CommerceInit) {
    let at = now();
    for p in [
        args.governance,
        args.membership_canister,
        args.treasury.owner,
    ] {
        authenticated(p).expect("configuration");
    }
    assert!(
        !args.user_homes.is_empty()
            && args.user_homes.len() <= 16
            && args.max_subjects > 0
            && args.max_subjects <= 1_000_000
            && args.daily_orders > 0
            && args.daily_orders <= 100_000
    );
    model::validate_catalog(&args.catalog).expect("catalog");
    assert!(args.catalog.effective_at_ms <= at);
    save_catalog(&args.catalog);
    let ledger_fee = args.catalog.ledger_fee;
    save_config(&Config {
        init: args,
        paused: false,
        ledger_verified: false,
        day: 0,
        orders: 0,
        minute: 0,
        reads: 0,
        refreshes: 0,
        authorizations: Default::default(),
        ledger_fee,
    });
    persist_config();
    rebuild(at);
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    persist_config();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    load_catalogs();
    let mut pending = Vec::new();
    TRANSFERS.with_borrow(|t| {
        t.for_each(|_, v| {
            if v.status == MerchantTransferStatus::InFlight {
                pending.push(v);
            }
        })
    });
    for mut t in pending {
        t.status = MerchantTransferStatus::Unknown;
        save_transfer(&t);
    }
    rebuild(now());
}

#[ic_cdk::update]
async fn verify_ledger_configuration() -> Result<()> {
    governance()?;
    let c = catalog(now());
    let decimals: u8 = dmsg_runtime::call(c.ledger, "icrc1_decimals", ()).await?;
    let fee: Nat = dmsg_runtime::call(c.ledger, "icrc1_fee", ()).await?;
    let fee = dmsg_runtime::ledger::token_amount(fee)?;
    ensure(decimals == c.decimals && fee > 0, Error::IntegrityFailed)?;
    let mut cfg = config();
    cfg.ledger_verified = true;
    cfg.ledger_fee = fee;
    save_config(&cfg);
    Ok(())
}

#[ic_cdk::update]
fn schedule_policy(c: Catalog) -> Result<()> {
    let at = now();
    governance()?;
    model::validate_catalog(&c)?;
    let old = latest_catalog();
    ensure(
        c.effective_at_ms >= at.saturating_add(30 * DAY)
            && c.effective_at_ms > old.effective_at_ms
            && c.version > old.version
            && c.ledger == old.ledger
            && c.decimals == old.decimals,
        invalid("catalog notice / asset"),
    )?;
    ensure(
        !CATALOGS.with_borrow(|t| t.contains(&c.version.to_be_bytes()))
            && CATALOGS.with_borrow(|t| t.len()) < 256,
        Error::VersionConflict,
    )?;
    // A single month must never mix algorithm weight denominations.
    ensure(
        c.plans.iter().all(|p| p.weights == c.plans[0].weights)
            && (c.plans[0].weights == old.plans[0].weights
                || month_bounds(month_utc(c.effective_at_ms)?)?.0 == c.effective_at_ms),
        invalid("weight change at UTC month boundary"),
    )?;
    save_catalog(&c);
    Ok(())
}

#[ic_cdk::update]
fn refresh_catalog() -> Catalog {
    let c = catalog(now());
    CERT.with_borrow_mut(|t| t.put(catalog_key().to_vec(), &c));
    c
}

#[ic_cdk::query]
fn get_catalog() -> Result<CertifiedBatch> {
    CERT.with_borrow(|c| c.batch(ic_cdk::api::canister_self(), vec![catalog_key().to_vec()]))
}

#[ic_cdk::query]
fn list_catalogs(after_version: Option<u64>) -> Vec<Catalog> {
    CATALOGS
        .with_borrow(|t| {
            t.page(
                after_version.map_or_else(Vec::new, |v| v.to_be_bytes().to_vec()),
                64,
            )
        })
        .into_iter()
        .map(|(_, c)| c)
        .collect()
}

#[ic_cdk::update]
fn set_admission_pause(paused: bool) -> Result<()> {
    governance()?;
    let mut c = config();
    c.paused = paused;
    save_config(&c);
    Ok(())
}

#[ic_cdk::query]
fn quote_order(request: QuoteOrder) -> Result<OrderQuote> {
    let at = now();
    let s = get_subject(&request.beneficiary)?;
    model::quote(ic_cdk::api::canister_self(), &s, catalog(at), request, at)
}

#[ic_cdk::update]
async fn open_order(input: OpenOrder) -> Result<BillingOrder> {
    let at = now();
    let canister_id = ic_cdk::api::canister_self();
    let caller = ic_cdk::api::msg_caller();
    authenticated(caller)?;
    ensure(
        caller == input.quote.request.payer.owner
            && caller == input.authorization.actor
            && input.authorization.service_canister == canister_id
            && input.authorization.beneficiary == input.quote.request.beneficiary
            && input.authorization.application_id == input.quote.request.op_id
            && input.authorization.action_digest == order_digest(&input.quote),
        Error::Forbidden,
    )?;
    let o = model::new_order(canister_id, input.clone());
    if let Ok(old) = order(o.order_id) {
        ensure(old.input == input, Error::IdempotencyConflict)?;
        return Ok(old);
    }
    ensure(
        at < o.input.quote.fund_by_ms && o.input.quote.created_at_ms <= at,
        Error::Expired,
    )?;
    let s = get_subject(&input.quote.request.beneficiary)?;
    ensure(
        model::quote(
            canister_id,
            &s,
            catalog(input.quote.created_at_ms),
            input.quote.request.clone(),
            input.quote.created_at_ms,
        )? == input.quote,
        Error::IntegrityFailed,
    )?;
    ensure(
        catalog(at).version == input.quote.catalog.version,
        Error::PolicyStale,
    )?;
    let cfg = config();
    ensure(
        !cfg.paused && cfg.ledger_verified,
        Error::Unavailable("cash admission paused or ledger unverified".into()),
    )?;
    ensure(cfg.ledger_fee <= input.quote.fee_reserve, Error::FeeBlocked)?;
    reserve_call(at, CallBudget::Authorization(caller))?;
    let (_, at) = authorization(&input.authorization, canister_id).await?;
    // Concurrent retries may both authorize; only the first callback opens an order.
    if let Ok(old) = order(o.order_id) {
        ensure(old.input == input, Error::IdempotencyConflict)?;
        return Ok(old);
    }
    let mut s = get_subject(&input.quote.request.beneficiary)?;
    let cfg = config();
    ensure(cfg.ledger_fee <= input.quote.fee_reserve, Error::FeeBlocked)?;
    ensure(
        s.business_revision == input.quote.request.expected_business_revision
            && at < input.quote.fund_by_ms
            && !cfg.paused,
        Error::VersionConflict,
    )?;
    ensure(
        catalog(at).version == input.quote.catalog.version,
        Error::PolicyStale,
    )?;
    // Prepare all fallible local work before committing admission and the order.
    model::project(canister_id, &mut s, &catalog(at), at)?;
    reserve_order(at)?;
    save_order(&o);
    save(&s);
    Ok(o)
}

#[ic_cdk::update]
async fn check_order_funding(id: Hash, block: u64) -> Result<OrderProgress> {
    check_funding(id, block).await.map(Into::into)
}

async fn check_funding(id: Hash, block: u64) -> Result<BillingOrder> {
    let canister_id = ic_cdk::api::canister_self();
    let at = now();
    let o = order(id)?;
    if let Some(d) = DEPOSITS.with_borrow(|t| t.load(&block.to_be_bytes())) {
        ensure(d.order_id == id, Error::IdempotencyConflict)?;
        return Ok(o);
    }
    ensure(at >= o.busy_until_ms, Error::Pending)?;
    reserve_call(at, CallBudget::Funds)?;
    let mut o = o;
    o.generation += 1;
    o.busy_until_ms = at + MINUTE;
    let generation = o.generation;
    save_order(&o);
    let result = dmsg_runtime::ledger::read_transfer(o.input.quote.catalog.ledger, block).await;
    let at = now();
    let mut o = order(id)?;
    ensure(o.generation == generation, Error::VersionConflict)?;
    o.busy_until_ms = 0;
    save_order(&o);
    let tx = match result {
        Ok(v) => v,
        Err(e) => return Err(e),
    };
    ensure(
        tx.to
            == Account {
                owner: canister_id,
                subaccount: Some(o.receive_subaccount.into_array()),
            }
            && tx.committed_at <= at,
        Error::IntegrityFailed,
    )?;
    ensure(
        !DEPOSITS.with_borrow(|t| t.contains(&block.to_be_bytes())),
        Error::IdempotencyConflict,
    )?;
    o.confirmed_in = o
        .confirmed_in
        .checked_add(tx.amount)
        .ok_or(Error::QuotaExceeded)?;
    let mut s = load(&o.input.quote.request.beneficiary)?;
    let q = o.input.quote.clone();
    let total = q
        .amount_atomic
        .checked_add(q.fee_reserve)
        .ok_or(Error::QuotaExceeded)?;
    let mut d = MerchantDeposit {
        order_id: id,
        block,
        from: tx.from,
        amount: tx.amount,
        refundable: tx.amount,
    };
    let can_activate = o.status == OrderStatus::AwaitingFunding
        && tx.from == q.request.payer
        && tx.amount >= total
        && tx.committed_at >= q.created_at_ms
        && tx.committed_at < q.fund_by_ms
        && at < q.activate_by_ms
        && s.business_revision == q.request.expected_business_revision;
    if can_activate {
        let mut next = s.clone();
        let mut candidate = o.clone();
        if model::activate(&mut next, &mut candidate, at).is_ok() {
            s = next;
            o = candidate;
            o.funding_block = Some(block);
            o.service_reserve = q.amount_atomic;
            o.fee_reserve = q.fee_reserve;
            d.refundable = tx.amount - total;
        } else {
            o.status = OrderStatus::RefundCommitted;
        }
    } else if o.status == OrderStatus::AwaitingFunding
        && (at >= q.activate_by_ms || s.business_revision != q.request.expected_business_revision)
    {
        o.status = OrderStatus::RefundCommitted;
    }
    o.refundable = o
        .refundable
        .checked_add(d.refundable)
        .ok_or(Error::QuotaExceeded)?;
    DEPOSITS.with_borrow_mut(|t| t.put(&block.to_be_bytes(), &d));
    save_order(&o);
    publish(canister_id, &mut s, at)?;
    Ok(o)
}

#[ic_cdk::update]
async fn request_refund(id: Hash, intent: MembershipIntent) -> Result<BillingOrder> {
    let canister_id = ic_cdk::api::canister_self();
    let caller = ic_cdk::api::msg_caller();
    let o = order(id)?;
    ensure(
        caller == o.input.quote.request.payer.owner
            && intent.actor == caller
            && intent.beneficiary == o.input.quote.request.beneficiary
            && intent.service_canister == canister_id
            && intent.action_digest == refund_digest(id),
        Error::Forbidden,
    )?;
    if matches!(
        o.status,
        OrderStatus::Closing | OrderStatus::RefundCommitted
    ) {
        return Ok(o);
    }
    let (_, at) = authorization(&intent, canister_id).await?;
    let mut o = order(id)?;
    let mut s = load(&o.input.quote.request.beneficiary)?;
    if o.status == OrderStatus::AwaitingFunding {
        o.status = OrderStatus::Cancelled;
        save_order(&o);
        return Ok(o);
    }
    ensure(o.status == OrderStatus::Active, Error::VersionConflict)?;
    let target = s
        .contracts
        .iter()
        .find(|c| Some(c.contract_id) == o.activated_contract_id)
        .cloned()
        .ok_or(Error::UnsupportedProtocol)?;
    let future = at < target.starts_at_ms;
    ensure(
        future
            || (s.first_cash_order == Some(id)
                && !s.self_refund_used
                && at < target.starts_at_ms.saturating_add(7 * DAY)),
        Error::Forbidden,
    )?;
    let related: Vec<_> = s
        .contracts
        .iter()
        .filter(|c| {
            c.term_starts_at_ms == target.term_starts_at_ms
                && c.expires_at_ms == target.expires_at_ms
                && matches!(c.source, ContractSource::Cash { .. })
        })
        .map(|c| c.contract_id)
        .collect();
    let stop = s
        .contracts
        .iter()
        .filter(|c| related.contains(&c.contract_id))
        .map(|c| c.last_issued_until_ms)
        .max()
        .unwrap_or(0)
        .max(at);
    let mut orders = Vec::new();
    for c in &s.contracts {
        if related.contains(&c.contract_id) {
            if let ContractSource::Cash { order_id } = c.source {
                let mut old = order(order_id)?;
                ensure(old.status == OrderStatus::Active, Error::VersionConflict)?;
                old.status = OrderStatus::Closing;
                old.close_effective_at_ms = Some(stop);
                orders.push(old);
            }
        }
    }
    for c in &mut s.contracts {
        if related.contains(&c.contract_id) {
            c.closing_at_ms = Some(stop);
        }
    }
    if !future {
        s.self_refund_used = true;
    }
    for old in orders {
        save_order(&old);
        if old.order_id == id {
            o = old;
        }
    }

    s.bump();
    publish(canister_id, &mut s, at)?;
    Ok(o)
}

#[ic_cdk::update]
fn reconcile_order(id: Hash) -> Result<OrderProgress> {
    reconcile(id, ic_cdk::api::canister_self(), now()).map(Into::into)
}

fn reconcile(id: Hash, canister_id: Principal, at: u64) -> Result<BillingOrder> {
    let mut o = order(id)?;
    let original = o.clone();
    let mut s = load(&o.input.quote.request.beneficiary)?;
    if o.status == OrderStatus::AwaitingFunding && at >= o.input.quote.activate_by_ms {
        o.status = OrderStatus::Cancelled;
    }
    if o.status == OrderStatus::Closing {
        let stop = o.close_effective_at_ms.ok_or(Error::IntegrityFailed)?;
        ensure(at >= stop, Error::Pending)?;
        let c = s
            .contracts
            .iter_mut()
            .find(|c| Some(c.contract_id) == o.activated_contract_id)
            .ok_or(Error::NotFound)?;
        let refund = model::refund_principal(&o, c, stop)?;
        c.terminated_at_ms = Some(c.terminated_at_ms.unwrap_or(stop).min(stop));
        o.service_reserve -= refund;
        o.refunded_principal += refund;
        let total = refund
            .checked_add(o.fee_reserve)
            .ok_or(Error::QuotaExceeded)?;
        o.fee_reserve = 0;
        o.refundable = o
            .refundable
            .checked_add(total)
            .ok_or(Error::QuotaExceeded)?;
        let block = o.funding_block.ok_or(Error::IntegrityFailed)?;
        let mut d = DEPOSITS
            .with_borrow(|t| t.load(&block.to_be_bytes()))
            .ok_or(Error::NotFound)?;
        d.refundable = d
            .refundable
            .checked_add(total)
            .ok_or(Error::QuotaExceeded)?;
        DEPOSITS.with_borrow_mut(|t| t.put(&block.to_be_bytes(), &d));
        o.status = OrderStatus::RefundCommitted;
        s.bump();
    }
    model::accrue(&mut o, &s, at)?;
    if o != original {
        save_order(&o);
    }
    if !s
        .view
        .as_ref()
        .is_some_and(|v| v.business_revision == s.business_revision && at < v.valid_until_ms)
    {
        publish(canister_id, &mut s, at)?;
    }
    Ok(o)
}

fn new_transfer(
    o: &mut BillingOrder,
    to: Account,
    total: u128,
    fee: u128,
    at: u64,
) -> Result<MerchantTransfer> {
    ensure(
        fee <= o.input.quote.fee_reserve && total > fee,
        Error::FeeBlocked,
    )?;
    let n = o.next_transfer;
    o.next_transfer = o.next_transfer.checked_add(1).ok_or(Error::QuotaExceeded)?;
    o.outgoing = o.outgoing.checked_add(total).ok_or(Error::QuotaExceeded)?;
    Ok(MerchantTransfer {
        replaces: None,
        replaced_by: None,
        order_id: o.order_id,
        transfer_id: n,
        to,
        amount: total - fee,
        fee,
        memo: digest("dmsg/commerce/transfer/v1", &(o.order_id, n)),
        created_at_time_ns: millis_to_nanos(at)?,
        status: MerchantTransferStatus::Pending,
        block: None,
        last_error: None,
    })
}

#[ic_cdk::update]
fn claim_deposit_refund(id: Hash, block: u64) -> Result<TransferProgress> {
    claim_refund(id, block).map(Into::into)
}

fn claim_refund(id: Hash, block: u64) -> Result<MerchantTransfer> {
    let mut o = order(id)?;
    let mut d = DEPOSITS
        .with_borrow(|t| t.load(&block.to_be_bytes()))
        .ok_or(Error::NotFound)?;
    ensure(d.order_id == id, Error::IntegrityFailed)?;
    let leg = new_transfer(&mut o, d.from, d.refundable, config().ledger_fee, now())?;
    o.refundable = o
        .refundable
        .checked_sub(d.refundable)
        .ok_or(Error::IntegrityFailed)?;
    d.refundable = 0;
    DEPOSITS.with_borrow_mut(|t| t.put(&block.to_be_bytes(), &d));
    save_transfer(&leg);
    save_order(&o);
    Ok(leg)
}

#[ic_cdk::update]
fn collect_revenue(id: Hash) -> Result<MerchantTransfer> {
    governance()?;
    let at = now();
    reconcile(id, ic_cdk::api::canister_self(), at)?;
    let mut o = order(id)?;
    let total = o.earned;
    let leg = new_transfer(
        &mut o,
        config().init.treasury,
        total,
        config().ledger_fee,
        at,
    )?;
    o.earned = 0;
    save_transfer(&leg);
    save_order(&o);
    Ok(leg)
}

#[ic_cdk::update]
fn claim_fee_reserve(id: Hash) -> Result<TransferProgress> {
    claim_fees(id).map(Into::into)
}

fn claim_fees(id: Hash) -> Result<MerchantTransfer> {
    let at = now();
    let mut o = reconcile(id, ic_cdk::api::canister_self(), at)?;
    let s = load(&o.input.quote.request.beneficiary)?;
    ensure(
        o.service_reserve == 0
            && o.outgoing == 0
            && o.earned == 0
            && s.contracts
                .iter()
                .filter(|c| Some(c.contract_id) == o.activated_contract_id)
                .all(|c| at >= c.expires_at_ms),
        Error::Pending,
    )?;
    let fee_reserve = o.fee_reserve;
    let to = o.input.quote.request.payer;
    let leg = new_transfer(&mut o, to, fee_reserve, config().ledger_fee, at)?;
    o.fee_reserve = 0;
    save_transfer(&leg);
    save_order(&o);
    Ok(leg)
}

fn complete(id: Hash, n: u64, block: u64) -> Result<MerchantTransfer> {
    let mut t = transfer(id, n)?;
    if t.status == MerchantTransferStatus::Succeeded {
        ensure(t.block == Some(block), Error::IntegrityFailed)?;
        return Ok(t);
    }
    ensure(
        matches!(
            t.status,
            MerchantTransferStatus::InFlight | MerchantTransferStatus::Unknown
        ),
        Error::VersionConflict,
    )?;
    if let Some(old) = OUTGOING.with_borrow(|m| m.load(&block.to_be_bytes())) {
        ensure(old == key(id, n), Error::IdempotencyConflict)?;
    }
    let mut o = order(id)?;
    let total = t.amount.checked_add(t.fee).ok_or(Error::IntegrityFailed)?;
    o.outgoing = o
        .outgoing
        .checked_sub(total)
        .ok_or(Error::IntegrityFailed)?;
    o.transferred = o
        .transferred
        .checked_add(t.amount)
        .ok_or(Error::IntegrityFailed)?;
    o.network_fees = o
        .network_fees
        .checked_add(t.fee)
        .ok_or(Error::IntegrityFailed)?;
    t.status = MerchantTransferStatus::Succeeded;
    t.block = Some(block);
    OUTGOING.with_borrow_mut(|m| m.put(&block.to_be_bytes(), &key(id, n)));
    save_transfer(&t);
    save_order(&o);
    Ok(t)
}

#[ic_cdk::update]
async fn process_transfer(id: Hash, n: u64) -> Result<TransferProgress> {
    dispatch_transfer(id, n).await.map(Into::into)
}

async fn dispatch_transfer(id: Hash, n: u64) -> Result<MerchantTransfer> {
    let mut t = transfer(id, n)?;
    if t.status == MerchantTransferStatus::Succeeded {
        return Ok(t);
    }
    ensure(
        matches!(
            t.status,
            MerchantTransferStatus::Pending
                | MerchantTransferStatus::Unknown
                | MerchantTransferStatus::Rejected
        ),
        if t.status == MerchantTransferStatus::InFlight {
            Error::Pending
        } else {
            Error::VersionConflict
        },
    )?;
    reserve_call(now(), CallBudget::Funds)?;
    let unknown = t.status == MerchantTransferStatus::Unknown;
    let o = order(id)?;
    t.status = MerchantTransferStatus::InFlight;
    save_transfer(&t);
    let result: std::result::Result<
        std::result::Result<Nat, TransferError>,
        dmsg_runtime::CallFailure,
    > = dmsg_runtime::call_classified(
        o.input.quote.catalog.ledger,
        "icrc1_transfer",
        (TransferArg {
            from_subaccount: Some(o.receive_subaccount.into_array()),
            to: t.to,
            fee: Some(t.fee.into()),
            created_at_time: Some(t.created_at_time_ns),
            memo: Some(t.memo.to_vec().into()),
            amount: t.amount.into(),
        },),
    )
    .await;
    if transfer(id, n)?.status == MerchantTransferStatus::Succeeded {
        return transfer(id, n);
    }
    match result {
        Ok(Ok(b)) => complete(id, n, dmsg_runtime::ledger::block_index(b)?),
        Ok(Err(TransferError::Duplicate { duplicate_of })) => {
            complete(id, n, dmsg_runtime::ledger::block_index(duplicate_of)?)
        }
        Ok(Err(error)) => {
            if let TransferError::BadFee { expected_fee } = &error {
                let fee = dmsg_runtime::ledger::token_amount(expected_fee.clone())?;
                ensure(fee > 0, Error::IntegrityFailed)?;
                let mut cfg = config();
                cfg.ledger_fee = fee;
                save_config(&cfg);
            }
            t.last_error = Some(error);
            t.status = if unknown {
                MerchantTransferStatus::Unknown
            } else {
                MerchantTransferStatus::Rejected
            };
            save_transfer(&t);
            Ok(t)
        }
        Err(failure) => {
            let unknown = failure.preserves_unknown(unknown);
            t.status = if unknown {
                MerchantTransferStatus::Unknown
            } else {
                MerchantTransferStatus::Rejected
            };
            save_transfer(&t);
            Err(if unknown {
                Error::ExecutionUnknown
            } else {
                failure.into()
            })
        }
    }
}

#[ic_cdk::update]
async fn reconcile_transfer(id: Hash, n: u64, block: u64) -> Result<TransferProgress> {
    verify_transfer(id, n, block).await.map(Into::into)
}

async fn verify_transfer(id: Hash, n: u64, block: u64) -> Result<MerchantTransfer> {
    let t = transfer(id, n)?;
    if t.status == MerchantTransferStatus::Succeeded {
        return Ok(t);
    }
    ensure(
        matches!(
            t.status,
            MerchantTransferStatus::InFlight | MerchantTransferStatus::Unknown
        ),
        Error::VersionConflict,
    )?;
    let o = order(id)?;
    reserve_call(now(), CallBudget::Funds)?;
    let tx = dmsg_runtime::ledger::read_transfer(o.input.quote.catalog.ledger, block).await?;
    ensure(
        tx.from
            == Account {
                owner: ic_cdk::api::canister_self(),
                subaccount: Some(o.receive_subaccount.into_array()),
            }
            && tx.to == t.to
            && tx.amount == t.amount
            && tx.fee == Some(t.fee)
            && tx.memo == Some(t.memo.to_vec())
            && tx.created_at_time == Some(t.created_at_time_ns),
        Error::IntegrityFailed,
    )?;
    complete(id, n, block)
}

#[ic_cdk::update]
fn revise_rejected_transfer(id: Hash, n: u64, fee: u128) -> Result<MerchantTransfer> {
    let at = now();
    let caller = ic_cdk::api::msg_caller();
    let mut t = transfer(id, n)?;
    let o = order(id)?;
    ensure(
        caller == t.to.owner
            || caller == o.input.quote.request.payer.owner
            || caller == config().init.governance,
        Error::Forbidden,
    )?;
    if let Some(next) = t.replaced_by {
        let next = transfer(id, next)?;
        ensure(next.fee == fee, Error::IdempotencyConflict)?;
        return Ok(next);
    }
    ensure(
        t.status == MerchantTransferStatus::Rejected
            && fee
                == t.last_error
                    .as_ref()
                    .and_then(|e| match e {
                        TransferError::BadFee { expected_fee } => {
                            dmsg_runtime::ledger::token_amount(expected_fee.clone()).ok()
                        }
                        _ => None,
                    })
                    .unwrap_or(config().ledger_fee)
            && fee <= o.input.quote.fee_reserve,
        Error::VersionConflict,
    )?;
    let total = t.amount.checked_add(t.fee).ok_or(Error::QuotaExceeded)?;
    ensure(total > fee, Error::FeeBlocked)?;
    let mut o = o;
    o.outgoing = o
        .outgoing
        .checked_sub(total)
        .ok_or(Error::IntegrityFailed)?;
    let mut replacement = new_transfer(&mut o, t.to, total, fee, at)?;
    replacement.replaces = Some(n);
    t.status = MerchantTransferStatus::Superseded;
    t.replaced_by = Some(replacement.transfer_id);
    save_transfer(&t);
    save_transfer(&replacement);
    save_order(&o);
    Ok(replacement)
}

/// Returns the view and the current timestamp, refreshed if a remote call was needed.
async fn refresh(
    b: Beneficiary,
    canister_id: Principal,
    mut at: u64,
) -> Result<(EntitlementView, u64)> {
    valid_subject(&b)?;
    let mut s = load(&b)?;
    if let Some(v) = &s.view {
        if at < v.valid_until_ms
            && at.saturating_add(MINUTE) < v.valid_until_ms
            && v.business_revision == s.business_revision
        {
            return Ok((v.clone(), at));
        }
    }
    let source = s.active(at).and_then(|c| match c.source {
        ContractSource::Sns { claim_id } if c.closing_at_ms.is_none() => {
            Some((c.contract_id, claim_id))
        }
        _ => None,
    });
    if let Some((contract, id)) = source {
        ensure(at >= s.busy_until_ms, Error::Pending)?;
        if at < s.retry_after_ms {
            return s
                .view
                .clone()
                .map(|v| (v, at))
                .ok_or(Error::MembershipStale);
        }
        reserve_call(at, CallBudget::Refresh)?;
        s.generation += 1;
        let generation = s.generation;
        s.busy_until_ms = at + MINUTE;
        save_subject(&s);
        let response: Result<Result<ClaimView>> = dmsg_runtime::call(
            config().init.membership_canister,
            "get_claim_for_consumer",
            (id,),
        )
        .await;
        at = now();
        s = load(&b)?;
        ensure(s.generation == generation, Error::VersionConflict)?;
        s.busy_until_ms = 0;
        let c = s
            .contracts
            .iter_mut()
            .find(|c| c.contract_id == contract)
            .ok_or(Error::NotFound)?;
        let previous = c.eligibility.clone();
        let observed = at;
        let eligibility = match response {
            Ok(Ok(v))
                if v.claim_id == id
                    && v.beneficiary == b
                    && v.home_membership == config().init.membership_canister
                    && v.status == ClaimStatus::Active =>
            {
                c.observed_at_ms = v.observed_at_ms;
                c.qualified_until_ms = v.valid_until_ms;
                v.eligibility
            }
            _ => Eligibility::Unverifiable,
        };
        match eligibility {
            Eligibility::Unverifiable => {
                c.unverifiable_since_ms.get_or_insert(observed);
            }
            Eligibility::Ineligible => {
                if !c
                    .resource_pauses
                    .last()
                    .is_some_and(|(_, end)| end.is_none())
                {
                    ensure(c.resource_pauses.len() < 32, Error::QuotaExceeded)?;
                    c.resource_pauses
                        .push((observed.max(c.last_issued_until_ms), None));
                }
                if let (Some(since), Some(deadline)) =
                    (c.unverifiable_since_ms.take(), c.repair_deadline_ms)
                {
                    c.repair_deadline_ms = Some(
                        deadline
                            .saturating_add(observed.saturating_sub(since))
                            .min(c.expires_at_ms),
                    );
                }
                c.repair_deadline_ms
                    .get_or_insert(observed.saturating_add(7 * DAY).min(c.expires_at_ms));
            }
            Eligibility::Eligible => {
                if let Some((start, end)) = c
                    .resource_pauses
                    .last_mut()
                    .filter(|(_, end)| end.is_none())
                {
                    *end = Some(observed.max(*start));
                }
                c.repair_deadline_ms = None;
                c.unverifiable_since_ms = None;
            }
        }
        c.eligibility = eligibility.clone();
        s.retry_after_ms = if eligibility == Eligibility::Unverifiable {
            at.saturating_add(MINUTE)
        } else {
            0
        };
        if previous != eligibility {
            s.bump();
        }
    }
    Ok((publish(canister_id, &mut s, at)?, at))
}

#[ic_cdk::update]
async fn refresh_entitlement(b: Beneficiary) -> Result<EntitlementView> {
    refresh(b, ic_cdk::api::canister_self(), now())
        .await
        .map(|(view, _)| view)
}

#[ic_cdk::query]
fn get_entitlement_batch(subjects: Vec<Beneficiary>) -> Result<CertifiedBatch> {
    for b in &subjects {
        valid_subject(b)?;
    }
    CERT.with_borrow(|c| {
        c.batch(
            ic_cdk::api::canister_self(),
            subjects
                .iter()
                .map(|b| entitlement_key(b).to_vec())
                .collect(),
        )
    })
}

#[ic_cdk::update]
async fn get_execution_entitlement(
    b: Beneficiary,
    month: u32,
    account_created_at_ms: u64,
) -> Result<ExecutionEntitlement> {
    let at = now();
    valid_subject(&b)?;
    ensure(
        ic_cdk::api::msg_caller() == b.authority_canister
            && account_created_at_ms <= at
            && month == month_utc(at)?,
        Error::Forbidden,
    )?;
    let mut s = get_subject(&b)?;
    if let Some(created) = s.created_at_ms {
        ensure(created == account_created_at_ms, Error::IntegrityFailed)?;
    } else {
        s.created_at_ms = Some(account_created_at_ms);
        save(&s);
    }
    let (view, at) = refresh(b.clone(), ic_cdk::api::canister_self(), at).await?;
    let s = load(&b)?;
    let month = model::month(&s, &catalog(at), month)?;
    Ok(ExecutionEntitlement { view, month })
}

#[ic_cdk::update]
async fn authorize_membership_intent(request: ClaimRequest) -> Result<MembershipAuthorization> {
    let caller = ic_cdk::api::msg_caller();
    ensure(
        caller == config().init.membership_canister
            && request.authorization.service_canister == caller
            && request.authorization.action_digest == claim_action_digest(&request),
        Error::Forbidden,
    )?;
    validate_claim(
        &get_subject(&request.authorization.beneficiary)?,
        &request,
        now(),
    )?;
    let (a, at) = authorization(&request.authorization, ic_cdk::api::canister_self()).await?;
    validate_claim(
        &get_subject(&request.authorization.beneficiary)?,
        &request,
        at,
    )?;
    Ok(a)
}

fn validate_claim(s: &Subject, r: &ClaimRequest, at: u64) -> Result<PlanVersion> {
    ensure(
        s.business_revision == r.expected_business_revision,
        Error::VersionConflict,
    )?;
    let c = catalog(at);
    let p = if let ClaimChange::Replace { previous_claim } = r.change {
        let old = s
            .contracts
            .iter()
            .find(|c| matches!(c.source, ContractSource::Sns { claim_id } if claim_id == previous_claim))
            .ok_or(Error::NotFound)?;
        ensure(
            plan_digest(&old.plan) == r.benefit_id
                && old.plan.membership_policy_version == Some(r.policy_version),
            Error::PolicyStale,
        )?;
        old.plan.clone()
    } else {
        c.plans
            .into_iter()
            .find(|p| {
                plan_digest(p) == r.benefit_id
                    && p.membership_policy_version == Some(r.policy_version)
                    && p.price_cents > 0
            })
            .ok_or(Error::PolicyStale)?
    };
    let current = s.active(at);
    match (&r.change, &r.term) {
        (ClaimChange::Start, TermRule::CalendarYear) => ensure(
            current.is_none()
                && !s
                    .contracts
                    .iter()
                    .any(|c| c.starts_at_ms > at && model::end(c) > c.starts_at_ms),
            Error::VersionConflict,
        )?,
        (
            ClaimChange::Start,
            TermRule::Fixed {
                starts_at_ms,
                expires_at_ms,
            },
        ) => {
            let old = current.ok_or(Error::NotFound)?;
            ensure(
                matches!(old.source, ContractSource::Cash { .. })
                    && *starts_at_ms == old.expires_at_ms
                    && *expires_at_ms == next_year(old.expires_at_ms)?
                    && at.saturating_add(30 * DAY) >= *starts_at_ms
                    && !s
                        .contracts
                        .iter()
                        .any(|c| c.starts_at_ms >= *starts_at_ms && model::end(c) > c.starts_at_ms),
                Error::VersionConflict,
            )?;
        }
        (
            ClaimChange::Renew { previous_claim },
            TermRule::Fixed {
                starts_at_ms,
                expires_at_ms,
            },
        ) => {
            let old = s
                .contracts
                .iter()
                .find(|c| {
                    matches!(c.source, ContractSource::Sns { claim_id } if claim_id == *previous_claim)
                })
                .ok_or(Error::NotFound)?;
            ensure(
                old.closing_at_ms.is_none()
                    && old.terminated_at_ms.is_none()
                    && at < *expires_at_ms,
                Error::VersionConflict,
            )?;
            ensure(
                matches!(old.source, ContractSource::Sns { claim_id } if claim_id == *previous_claim)
                    && *starts_at_ms == old.expires_at_ms
                    && *expires_at_ms == next_year(old.expires_at_ms)?
                    && at + 30 * DAY >= *starts_at_ms
                    && !s
                        .contracts
                        .iter()
                        .any(|c| c.starts_at_ms >= *starts_at_ms && model::end(c) > c.starts_at_ms),
                Error::VersionConflict,
            )?;
        }
        (
            ClaimChange::Upgrade { previous_claim } | ClaimChange::Replace { previous_claim },
            TermRule::Fixed {
                starts_at_ms,
                expires_at_ms,
            },
        ) => {
            let old = current.ok_or(Error::NotFound)?;
            ensure(
                matches!(old.source, ContractSource::Sns { claim_id } if claim_id == *previous_claim)
                    && *starts_at_ms >= old.starts_at_ms
                    && *starts_at_ms <= at
                    && *expires_at_ms == old.expires_at_ms
                    && (p.price_cents > old.plan.price_cents
                        || (matches!(r.change, ClaimChange::Replace { .. }) && p == old.plan)),
                Error::VersionConflict,
            )?;
        }
        _ => return Err(Error::UnsupportedProtocol),
    }
    Ok(p)
}

#[ic_cdk::update]
async fn authorize_membership_close(
    id: Hash,
    intent: MembershipIntent,
) -> Result<MembershipAuthorization> {
    let caller = ic_cdk::api::msg_caller();
    ensure(
        caller == config().init.membership_canister
            && intent.service_canister == caller
            && intent.action_digest == close_claim_digest(id),
        Error::Forbidden,
    )?;
    authorization(&intent, ic_cdk::api::canister_self())
        .await
        .map(|(authorization, _)| authorization)
}

#[ic_cdk::update]
fn apply_membership_decision(d: MembershipDecision) -> Result<MembershipDecisionReceipt> {
    let at = now();
    ensure(
        ic_cdk::api::msg_caller() == config().init.membership_canister,
        Error::Forbidden,
    )?;
    if let Some(r) = DECISIONS.with_borrow(|t| t.load(d.decision_id.as_slice())) {
        ensure(
            r.decision_digest == decision_digest(&d),
            Error::IdempotencyConflict,
        )?;
        return Ok(r);
    }
    let mut s = get_subject(&d.request.authorization.beneficiary)?;
    let result = (|| -> Result<(Option<Hash>, u64)> {
        if d.kind == DecisionKind::Close {
            let c = s
                .contracts
                .iter_mut()
                .find(|c| matches!(c.source, ContractSource::Sns { claim_id } if claim_id == d.claim_id))
                .ok_or(Error::NotFound)?;
            let stop = at.max(c.last_issued_until_ms);
            c.closing_at_ms = Some(stop);
            c.terminated_at_ms = Some(c.terminated_at_ms.unwrap_or(stop).min(stop));
            s.bump();
            return Ok((Some(d.claim_id), stop));
        }
        ensure(
            at < d.apply_by_ms
                && at < d.qualification_until_ms
                && d.starts_at_ms < d.expires_at_ms
                && s.contracts.len() < 256,
            Error::Expired,
        )?;
        let p = validate_claim(&s, &d.request, at)?;
        ensure(
            d.policy.benefit_id == plan_digest(&p)
                && d.policy.version == d.request.policy_version
                && d.required_atomic >= required_panda(&d.policy.threshold, 8)?,
            Error::IntegrityFailed,
        )?;
        let term_starts_at_ms = match d.request.change {
            ClaimChange::Upgrade { previous_claim } | ClaimChange::Replace { previous_claim } => {
                model::replace_claim(&mut s, previous_claim, d.starts_at_ms)?
            }
            _ => d.starts_at_ms,
        };
        ensure(
            !s.contracts
                .iter()
                .any(|c| c.starts_at_ms < d.expires_at_ms && d.starts_at_ms < model::end(c)),
            Error::VersionConflict,
        )?;
        s.contracts.push(MembershipContract {
            term_starts_at_ms,
            resource_pauses: vec![],
            contract_id: d.claim_id,
            plan: p,
            source: ContractSource::Sns {
                claim_id: d.claim_id,
            },
            starts_at_ms: d.starts_at_ms,
            expires_at_ms: d.expires_at_ms,
            terminated_at_ms: None,
            closing_at_ms: None,
            last_issued_until_ms: 0,
            eligibility: Eligibility::Eligible,
            observed_at_ms: d.observed_at_ms,
            qualified_until_ms: d.qualification_until_ms,
            repair_deadline_ms: None,
            unverifiable_since_ms: None,
        });
        s.bump();
        Ok((Some(d.claim_id), d.expires_at_ms))
    })();
    let (outcome, contract_id, commitment) = match result {
        Ok((id, end)) => (DecisionOutcome::Applied, id, end),
        Err(_) => (DecisionOutcome::Rejected, None, 0),
    };
    let receipt = MembershipDecisionReceipt {
        decision_id: d.decision_id,
        decision_digest: decision_digest(&d),
        outcome: outcome.clone(),
        contract_id,
        starts_at_ms: d.starts_at_ms,
        expires_at_ms: d.expires_at_ms,
        business_revision: s.business_revision,
        commitment_until_ms: commitment,
    };
    // Persist known rejections as well, so a lost ACK cannot leave occupancy ambiguous.
    if outcome == DecisionOutcome::Applied {
        publish(ic_cdk::api::canister_self(), &mut s, at)?;
    }
    DECISIONS.with_borrow_mut(|t| t.put(d.decision_id.as_slice(), &receipt));
    Ok(receipt)
}

#[ic_cdk::update]
fn get_membership_decision(id: Hash) -> Result<Option<MembershipDecisionReceipt>> {
    ensure(
        ic_cdk::api::msg_caller() == config().init.membership_canister,
        Error::Forbidden,
    )?;
    Ok(DECISIONS.with_borrow(|t| t.load(id.as_slice())))
}

fn can_read(o: &BillingOrder, caller: Principal) -> Result<()> {
    ensure(
        caller == o.input.quote.request.payer.owner
            || caller == o.input.quote.request.beneficiary.authority_canister
            || caller == config().init.governance,
        Error::Forbidden,
    )
}

#[ic_cdk::query]
fn get_operation(id: Hash) -> Result<BillingOrder> {
    let o = order(id)?;
    can_read(&o, ic_cdk::api::msg_caller())?;
    Ok(o)
}

#[ic_cdk::query]
fn get_order_certified(id: Hash) -> Result<CertifiedBatch> {
    can_read(&order(id)?, ic_cdk::api::msg_caller())?;
    CERT.with_borrow(|c| c.batch(ic_cdk::api::canister_self(), vec![order_key(id).to_vec()]))
}

#[ic_cdk::query]
fn get_transfer(id: Hash, n: u64) -> Result<MerchantTransfer> {
    let caller = ic_cdk::api::msg_caller();
    let t = transfer(id, n)?;
    ensure(
        caller == t.to.owner || can_read(&order(id)?, caller).is_ok(),
        Error::Forbidden,
    )?;
    Ok(t)
}

#[ic_cdk::query]
fn get_deposit(block: u64) -> Result<MerchantDeposit> {
    let caller = ic_cdk::api::msg_caller();
    let d = DEPOSITS
        .with_borrow(|t| t.load(&block.to_be_bytes()))
        .ok_or(Error::NotFound)?;
    ensure(
        caller == d.from.owner || can_read(&order(d.order_id)?, caller).is_ok(),
        Error::Forbidden,
    )?;
    Ok(d)
}

#[ic_cdk::update]
async fn release_replaced_claim(b: Beneficiary, id: Hash) -> Result<ClaimView> {
    let s = load(&b)?;
    let c = s
        .contracts
        .iter()
        .find(|c| matches!(c.source, ContractSource::Sns { claim_id } if claim_id == id))
        .ok_or(Error::NotFound)?;
    ensure(
        c.terminated_at_ms.is_some() || now() >= c.expires_at_ms,
        Error::VersionConflict,
    )?;
    let result: Result<ClaimView> = dmsg_runtime::call(
        config().init.membership_canister,
        "close_for_consumer",
        (id,),
    )
    .await?;
    result
}
