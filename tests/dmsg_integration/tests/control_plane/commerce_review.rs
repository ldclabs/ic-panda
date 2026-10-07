use super::*;
use std::collections::BTreeSet;

#[test]
fn unpublished_prices_are_rejected_but_published_old_quotes_survive_updates() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let bill = offer(&f, &id, 201);
    let q: Result<CheckoutQuote> = update(
        &f.ic,
        f.commerce,
        person(1),
        "quote_checkout",
        (bill.clone(), f.ledger, account(person(1))),
    );
    let q = q.unwrap();
    assert_eq!(q.asset.price_usd_micros, 1_000_000);
    let original_amount = q.cash.amount_atomic;
    let cfg: Result<(AppRegistration, Option<ProductRegistration>)> = update(
        &f.ic,
        f.commerce,
        person(1),
        "read_integration_configuration",
        ("dmsg".to_string(), Some(bill.product_id.clone())),
    );
    let (app, product) = cfg.unwrap();
    let mut unpublished = q.asset;
    unpublished.price_usd_micros = 1_010_000;
    let forged = checkout_quote(
        f.commerce,
        bill.clone(),
        &app,
        &product.unwrap(),
        unpublished,
        account(person(1)),
        q.quoted_at_ms,
    )
    .unwrap();
    assert!(forged.cash.amount_atomic < original_amount);
    let authorization = approve(
        &f,
        &id,
        &bill,
        f.commerce,
        ApprovalPurpose::CashCheckout,
        checkout_quote_hash(&forged),
        202,
    );
    let opened: Result<CheckoutView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "open_checkout",
        (OpenCheckout {
            quote: forged,
            authorization,
        },),
    );
    assert_eq!(opened, Err(Error::PolicyStale));
    // A real earlier observation stays usable while a newer observation is published.
    let original_asset = q_asset(&f, f.ledger);
    let q = checkout_quote(
        f.commerce,
        bill.clone(),
        &app,
        &product_registration(&f, &bill),
        original_asset,
        account(person(1)),
        q.quoted_at_ms,
    )
    .unwrap();
    let publication: Result<SettlementAsset> = update(
        &f.ic,
        f.commerce,
        f.sns,
        "publish_settlement_price",
        (f.ledger, 999_000u128, 30 * MINUTE),
    );
    publication.unwrap();
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let authorization = approve(
        &f,
        &id,
        &bill,
        f.commerce,
        ApprovalPurpose::CashCheckout,
        checkout_quote_hash(&q),
        203,
    );
    let opened: Result<CheckoutView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "open_checkout",
        (OpenCheckout {
            quote: q,
            authorization,
        },),
    );
    let opened = opened.unwrap();
    assert_eq!(opened.quote.cash.amount_atomic, original_amount);
    let amount = opened.quote.cash.amount_atomic + opened.quote.cash.fee_reserve_atomic;
    let block = deposit(&f, &opened, f.ledger, 1, amount);
    assert_eq!(
        funding(&f, opened.progress.order_id, block).status,
        CheckoutStatus::Applied
    );
}

#[test]
fn catalog_activation_keeps_lease_and_month_consistent() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let catalogs: Vec<Catalog> = query(
        &f.ic,
        f.commerce,
        person(1),
        "list_catalogs",
        (None::<u64>,),
    );
    let mut next = catalogs.last().unwrap().clone();
    next.version += 1;
    next.effective_at_ms = time(&f.ic) + 31 * DAY;
    for p in &mut next.plans {
        p.catalog_version = next.version;
        if p.plan_id == PlanId::Free {
            p.limits.monthly_execution_units = 30;
        }
    }
    let effective = next.effective_at_ms;
    let scheduled: Result<()> = update(&f.ic, f.commerce, f.sns, "schedule_policy", (next,));
    scheduled.unwrap();
    f.ic.advance_time(Duration::from_millis(effective - time(&f.ic) - 5 * MINUTE));
    let old: Result<ExecutionUsage> = update(
        &f.ic,
        f.user,
        person(1),
        "refresh_execution_entitlement",
        (id,),
    );
    let old = old.unwrap();
    // The catalog boundary ends the lease, also for a Free account without a record.
    assert!(old.valid_until_ms <= effective);
    assert_eq!(old.lease_revision, 0);
    f.ic.advance_time(Duration::from_millis(6 * MINUTE));
    let refreshed: Result<ExecutionUsage> = update(
        &f.ic,
        f.user,
        person(1),
        "refresh_execution_entitlement",
        (id,),
    );
    let refreshed = refreshed.unwrap();
    assert!(refreshed.allowed_units > old.allowed_units);
    let month = month_utc(time(&f.ic)).unwrap();
    let (start, end) = month_bounds(month).unwrap();
    let expected = (3 * u128::from(effective - start) + 30 * u128::from(end - effective))
        / u128::from(end - start);
    assert_eq!(refreshed.allowed_units, expected as u64);
    let again: Result<ExecutionUsage> = update(
        &f.ic,
        f.user,
        person(1),
        "refresh_execution_entitlement",
        (id,),
    );
    let again = again.unwrap();
    assert_eq!(
        (again.allowed_units, again.lease_revision),
        (refreshed.allowed_units, refreshed.lease_revision)
    );
    // No commerce record was created for the Free account.
    let b = dmsg_protocol::billing::beneficiary(f.user, &id);
    let batch: Result<CertifiedBatch> = query(
        &f.ic,
        f.commerce,
        person(1),
        "get_entitlement_batch",
        (vec![b],),
    );
    assert!(batch.unwrap().entries[0].value.is_none());
}

#[test]
fn merchants_read_only_their_orders() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let (o, _) = pay(&f, &id, "plus", 203);
    let merchant = o.quote.product.merchant.owner;
    let view: Result<CheckoutView> = query(
        &f.ic,
        f.commerce,
        merchant,
        "get_checkout",
        (o.progress.order_id,),
    );
    let view = view.unwrap();
    assert_eq!(view.quote, o.quote);
    assert_eq!(view.progress.status, CheckoutStatus::Applied);
    let page: Result<CheckoutOperationsPage> = query(
        &f.ic,
        f.commerce,
        merchant,
        "checkout_operations",
        (None::<Hash>, 32u16),
    );
    let page = page.unwrap();
    assert_eq!(page.orders.len(), 1);
    assert!(page.next.is_none());
    let denied: Result<CheckoutView> = query(
        &f.ic,
        f.commerce,
        person(99),
        "get_checkout",
        (o.progress.order_id,),
    );
    assert_eq!(denied, Err(Error::Forbidden));
    let page: Result<CheckoutOperationsPage> = query(
        &f.ic,
        f.commerce,
        person(99),
        "checkout_operations",
        (None::<Hash>, 32u16),
    );
    assert!(page.unwrap().orders.is_empty());
}

#[test]
fn an_inflight_transfer_excludes_retries_reconciliation_and_replacement() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let o = open(&f, &id, f.ledger, 205);
    let wrong = deposit(&f, &o, f.ledger, 2, 1000);
    funding(&f, o.progress.order_id, wrong.clone());
    let other = deposit(&f, &o, f.ledger, 3, 1000);
    funding(&f, o.progress.order_id, other);
    let refund: Result<CashTransfer> = update(
        &f.ic,
        f.commerce,
        person(2),
        "claim_checkout_refund",
        (
            o.progress.order_id,
            f.ledger,
            vec![wrong.block_index],
            Hash::new([207; 32]),
        ),
    );
    let refund = refund.unwrap();
    void(&f.ic, f.ledger, person(1), "reject_next_transfers", (1u32,));
    void(&f.ic, f.ledger, person(1), "delay_next_response", (20u8,));
    let first =
        f.ic.submit_call(
            f.commerce,
            person(2),
            "process_checkout_transfer",
            candid::encode_args((refund.transfer_id,)).unwrap(),
        )
        .unwrap();
    for _ in 0..10 {
        f.ic.tick();
    }
    f.ic.advance_time(Duration::from_millis(MINUTE + 1));
    let second: Result<CashTransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "process_checkout_transfer",
        (refund.transfer_id,),
    );
    assert_eq!(second, Err(Error::Pending));
    let replacement: Result<CashTransfer> = update(
        &f.ic,
        f.commerce,
        person(2),
        "revise_checkout_transfer_fee",
        (refund.transfer_id, refund.fee_atomic),
    );
    assert_eq!(replacement, Err(Error::Pending));
    let reconciled: Result<CashTransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "reconcile_checkout_transfer",
        (refund.transfer_id, wrong),
    );
    assert_eq!(reconciled, Err(Error::Pending));
    let answer: Result<CashTransferProgress> =
        candid::decode_one(&f.ic.await_call(first).unwrap()).unwrap();
    assert_eq!(answer.unwrap().status, CashTransferStatus::Rejected);
    let repeated: Result<CashTransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "process_checkout_transfer",
        (refund.transfer_id,),
    );
    assert_eq!(repeated.unwrap().status, CashTransferStatus::Succeeded);
    let balance: Nat = query(
        &f.ic,
        f.ledger,
        person(2),
        "icrc1_balance_of",
        (account(person(2)),),
    );
    assert_eq!(balance, Nat::from(990u64));
    let balance: Nat = query(
        &f.ic,
        f.ledger,
        person(2),
        "icrc1_balance_of",
        (o.quote.cash.deposit,),
    );
    assert_eq!(balance, Nat::from(1000u64));
}

fn q_asset(f: &Fixture, ledger: Principal) -> SettlementAsset {
    let a: Vec<SettlementAssetView> = query(&f.ic, f.commerce, person(1), "settlement_assets", ());
    a.into_iter()
        .find(|a| a.policy.ledger == ledger)
        .unwrap()
        .policy
}

fn product_registration(f: &Fixture, bill: &BillingOffer) -> ProductRegistration {
    let cfg: Result<(AppRegistration, Option<ProductRegistration>)> = update(
        &f.ic,
        f.commerce,
        person(1),
        "read_integration_configuration",
        ("dmsg".to_string(), Some(bill.product_id.clone())),
    );
    cfg.unwrap().1.unwrap()
}

fn refund(f: &Fixture, o: &CheckoutView, who: u8, amount: u128, op: u8) -> CashTransfer {
    let block = deposit(f, o, f.ledger, who, amount);
    funding(f, o.progress.order_id, block.clone());
    let r: Result<CashTransfer> = update(
        &f.ic,
        f.commerce,
        person(who),
        "claim_checkout_refund",
        (
            o.progress.order_id,
            f.ledger,
            vec![block.block_index],
            Hash::new([op; 32]),
        ),
    );
    r.unwrap()
}

#[test]
fn upgrade_discards_only_the_live_guard_and_recovers_the_frozen_transfer() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let o = open(&f, &id, f.ledger, 210);
    let leg = refund(&f, &o, 2, 1000, 212);
    void(&f.ic, f.ledger, person(1), "delay_next_response", (60u8,));
    f.ic.submit_call(
        f.commerce,
        person(2),
        "process_checkout_transfer",
        candid::encode_args((leg.transfer_id,)).unwrap(),
    )
    .unwrap();
    for _ in 0..10 {
        f.ic.tick();
    }
    let paid: Nat = query(
        &f.ic,
        f.ledger,
        person(2),
        "icrc1_balance_of",
        (account(person(2)),),
    );
    assert_eq!(paid, Nat::from(990u64));
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let retry: Result<CashTransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "process_checkout_transfer",
        (leg.transfer_id,),
    );
    assert_eq!(retry.unwrap().status, CashTransferStatus::Succeeded);
    let saved: Result<CashTransfer> = query(
        &f.ic,
        f.commerce,
        person(2),
        "get_checkout_transfer",
        (leg.transfer_id,),
    );
    let saved = saved.unwrap();
    assert_eq!(saved.created_at_time_ns, leg.created_at_time_ns);
    assert_eq!(saved.memo, leg.memo);
    assert_eq!(saved.amount_atomic, leg.amount_atomic);
    let paid: Nat = query(
        &f.ic,
        f.ledger,
        person(2),
        "icrc1_balance_of",
        (account(person(2)),),
    );
    assert_eq!(paid, Nat::from(990u64));
}

#[test]
fn one_caller_cannot_exhaust_other_callers_funding_and_refund_budget() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let o = open(&f, &id, f.ledger, 215);
    let missing = CashBlock {
        ledger: f.ledger,
        block_index: u128::from(u64::MAX),
    };
    for _ in 0..40 {
        let r: Result<CheckoutProgress> = update(
            &f.ic,
            f.commerce,
            person(90),
            "check_checkout_funding",
            (o.progress.order_id, missing.clone()),
        );
        assert_eq!(r, Err(Error::NotFound));
    }
    let denied: Result<CheckoutProgress> = update(
        &f.ic,
        f.commerce,
        person(90),
        "check_checkout_funding",
        (o.progress.order_id, missing),
    );
    assert_eq!(denied, Err(Error::QuotaExceeded));
    // Local allocation does not need another ledger call or another caller's permission.
    let r = refund(&f, &o, 90, 1000, 217);
    let paid: Result<CashTransferProgress> = update(
        &f.ic,
        f.commerce,
        person(91),
        "process_checkout_transfer",
        (r.transfer_id,),
    );
    assert_eq!(paid.unwrap().status, CashTransferStatus::Succeeded);
}

#[test]
fn cold_history_preserves_replay_reads_and_late_original_source_refunds() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let bill = offer(&f, &id, 220);
    let quote: Result<CheckoutQuote> = update(
        &f.ic,
        f.commerce,
        person(1),
        "quote_checkout",
        (bill.clone(), f.ledger, account(person(1))),
    );
    let quote = quote.unwrap();
    let authorization = approve(
        &f,
        &id,
        &bill,
        f.commerce,
        ApprovalPurpose::CashCheckout,
        checkout_quote_hash(&quote),
        221,
    );
    let input = OpenCheckout {
        quote,
        authorization,
    };
    let opened: Result<CheckoutView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "open_checkout",
        (input.clone(),),
    );
    let o = opened.unwrap();
    let leg = refund(&f, &o, 2, 1000, 222);
    let moved: Result<CashTransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "process_checkout_transfer",
        (leg.transfer_id,),
    );
    assert_eq!(moved.unwrap().status, CashTransferStatus::Succeeded);
    let cancelled: Result<CheckoutProgress> = update(
        &f.ic,
        f.commerce,
        person(1),
        "cancel_checkout",
        (o.progress.order_id,),
    );
    assert_eq!(cancelled.unwrap().status, CheckoutStatus::RefundCommitted);
    f.ic.advance_time(Duration::from_millis(32 * DAY));
    let swept: CheckoutHistorySweep =
        update(&f.ic, f.commerce, person(99), "sweep_checkout_history", ());
    assert_eq!(swept.orders, 1);
    assert_eq!(swept.transfers, 1);
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    // Cold orders stay readable by their readers.
    let cold: Result<CheckoutView> = query(
        &f.ic,
        f.commerce,
        person(1),
        "get_checkout",
        (o.progress.order_id,),
    );
    assert_eq!(
        cold.unwrap().progress.status,
        CheckoutStatus::RefundCommitted
    );
    let old: Result<CashTransfer> = query(
        &f.ic,
        f.commerce,
        person(2),
        "get_checkout_transfer",
        (leg.transfer_id,),
    );
    assert_eq!(old.unwrap().status, CashTransferStatus::Succeeded);
    let page: Result<CheckoutOperationsPage> = query(
        &f.ic,
        f.commerce,
        o.quote.product.merchant.owner,
        "checkout_operations",
        (None::<Hash>, 32u16),
    );
    assert_eq!(page.unwrap().orders.len(), 1);
    let replay: Result<CheckoutView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "open_checkout",
        (input.clone(),),
    );
    assert_eq!(
        replay.unwrap().progress.status,
        CheckoutStatus::RefundCommitted
    );
    let mut changed = input;
    changed.quote.cash.amount_atomic += 1;
    let denied: Result<CheckoutView> =
        update(&f.ic, f.commerce, person(1), "open_checkout", (changed,));
    assert_eq!(denied, Err(Error::IdempotencyConflict));
    // A sender can still recover a new deposit to an old receiving subaccount.
    let late = refund(&f, &o, 3, 1000, 223);
    let moved: Result<CashTransferProgress> = update(
        &f.ic,
        f.commerce,
        person(3),
        "process_checkout_transfer",
        (late.transfer_id,),
    );
    assert_eq!(moved.unwrap().status, CashTransferStatus::Succeeded);
    let balance: Nat = query(
        &f.ic,
        f.ledger,
        person(3),
        "icrc1_balance_of",
        (account(person(3)),),
    );
    assert_eq!(balance, Nat::from(990u64));
}

#[test]
fn unknown_transfers_and_their_orders_are_not_archived() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let o = open(&f, &id, f.ledger, 225);
    let leg = refund(&f, &o, 2, 1000, 227);
    void(&f.ic, f.ledger, person(1), "lose_next_response", ());
    let lost: Result<CashTransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "process_checkout_transfer",
        (leg.transfer_id,),
    );
    assert_eq!(lost, Err(Error::ExecutionUnknown));
    let cancelled: Result<CheckoutProgress> = update(
        &f.ic,
        f.commerce,
        person(1),
        "cancel_checkout",
        (o.progress.order_id,),
    );
    cancelled.unwrap();
    f.ic.advance_time(Duration::from_millis(32 * DAY));
    let swept: CheckoutHistorySweep =
        update(&f.ic, f.commerce, person(99), "sweep_checkout_history", ());
    assert_eq!(
        swept,
        CheckoutHistorySweep {
            orders: 0,
            transfers: 0
        }
    );
    // The unknown transfer stays live and readable by its recipient.
    let live: Result<CashTransfer> = query(
        &f.ic,
        f.commerce,
        person(2),
        "get_checkout_transfer",
        (leg.transfer_id,),
    );
    assert_eq!(live.unwrap().transfer_id, leg.transfer_id);
}

/// Run the same sample against each build using DMSG_WASM_DIR. The baseline
/// deliberately skips the new archive API; its heap still contains all 32 orders.
#[test]
#[ignore = "explicit commerce cycles/stable-memory/upgrade sample"]
fn commerce_cost_profile() {
    let f = Fixture::commercial();
    let id = f.create(1);
    for i in 0..32u8 {
        f.ic.advance_time(Duration::from_millis(MINUTE));
        if i % 10 == 0 {
            reprice(&f);
        }
        let before = f.ic.cycle_balance(f.commerce);
        let o = open(&f, &id, f.ledger, 20 + i * 2);
        let open_cycles = before - f.ic.cycle_balance(f.commerce);
        let cancelled: Result<CheckoutProgress> = update(
            &f.ic,
            f.commerce,
            person(1),
            "cancel_checkout",
            (o.progress.order_id,),
        );
        cancelled.unwrap();
        if [0, 7, 31].contains(&i) {
            let before = f.ic.cycle_balance(f.commerce);
            let page: Result<CheckoutOperationsPage> = update(
                &f.ic,
                f.commerce,
                person(99),
                "checkout_operations",
                (None::<Hash>, 32u16),
            );
            assert!(page.unwrap().orders.is_empty());
            let query_cycles = before - f.ic.cycle_balance(f.commerce);
            let metrics =
                f.ic.canister_status(f.commerce, None)
                    .unwrap()
                    .memory_metrics;
            println!("commerce orders={} open_scenario_cycles={open_cycles} unrelated_reader_cycles={query_cycles} stable_bytes={} heap_bytes={}", i + 1, metrics.stable_memory_size, metrics.wasm_memory_size);
        }
    }
    let before = f.ic.cycle_balance(f.commerce);
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let upgrade_cycles = before - f.ic.cycle_balance(f.commerce);
    let metrics =
        f.ic.canister_status(f.commerce, None)
            .unwrap()
            .memory_metrics;
    println!(
        "commerce hot_upgrade_orders=32 cycles={upgrade_cycles} stable_bytes={} heap_bytes={}",
        metrics.stable_memory_size, metrics.wasm_memory_size
    );
    if std::env::var_os("DMSG_COMMERCE_BASELINE").is_none() {
        f.ic.advance_time(Duration::from_millis(32 * DAY));
        let swept: CheckoutHistorySweep =
            update(&f.ic, f.commerce, person(99), "sweep_checkout_history", ());
        assert_eq!(swept.orders, 32);
        let before = f.ic.cycle_balance(f.commerce);
        f.ic.upgrade_canister(
            f.commerce,
            wasm("dmsg_commerce"),
            candid::encode_args(()).unwrap(),
            None,
        )
        .unwrap();
        let cycles = before - f.ic.cycle_balance(f.commerce);
        let metrics =
            f.ic.canister_status(f.commerce, None)
                .unwrap()
                .memory_metrics;
        println!(
            "commerce cold_upgrade_orders=32 cycles={cycles} stable_bytes={} heap_bytes={}",
            metrics.stable_memory_size, metrics.wasm_memory_size
        );
    }
}

#[test]
fn deposit_pagination_returns_every_source_through_the_public_interface() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let o = open(&f, &id, f.ledger, 230);
    let mut expected = BTreeSet::new();
    for who in [2, 3, 4] {
        let b = deposit(&f, &o, f.ledger, who, 1000);
        expected.insert(b.block_index);
        funding(&f, o.progress.order_id, b);
    }
    let first: Result<CheckoutDepositsPage> = query(
        &f.ic,
        f.commerce,
        o.quote.product.merchant.owner,
        "checkout_deposits",
        (o.progress.order_id, None::<Hash>, 2u16),
    );
    let first = first.unwrap();
    assert_eq!(first.deposits.len(), 2);
    assert!(first.next.is_some());
    let second: Result<CheckoutDepositsPage> = query(
        &f.ic,
        f.commerce,
        person(1),
        "checkout_deposits",
        (o.progress.order_id, first.next, 2u16),
    );
    let second = second.unwrap();
    assert_eq!(second.deposits.len(), 1);
    assert!(second.next.is_none());
    assert_eq!(
        expected,
        first
            .deposits
            .into_iter()
            .chain(second.deposits)
            .map(|d| d.block.block_index)
            .collect()
    );
}

/// Loads host-built images written by dmsg_commerce's `capacity_image` from
/// DMSG_COMMERCE_IMAGE_DIR and measures upgrades, certified reads, execution
/// entitlement reads and lease renewal at that size.
#[test]
#[ignore = "100k/1M-subject capacity; build the images with dmsg_commerce capacity_image first"]
fn commerce_capacity_profile() {
    let dir = PathBuf::from(std::env::var_os("DMSG_COMMERCE_IMAGE_DIR").expect("image dir"));
    // Matches dmsg_commerce::capacity: fixture time, user home and accounts.
    let at = 1_800_000_000_000u64;
    let home = Principal::from_slice(&[92, 1]);
    let account = |i: u64| AccountId(digest("capacity subject", &i)[..12].try_into().unwrap());
    for subjects in [100_000u64, 1_000_000] {
        let Ok(image) = std::fs::read(dir.join(format!("commerce-{subjects}.bin"))) else {
            continue;
        };
        let ic = PocketIcBuilder::new().with_application_subnet().build();
        ic.set_time(pocket_ic::Time::from_nanos_since_unix_epoch(
            millis_to_nanos(at + MINUTE).unwrap(),
        ));
        let commerce = ic.create_canister();
        ic.add_cycles(commerce, 100_000_000_000_000_000);
        // An empty module has no pre_upgrade that could write over the image.
        ic.install_canister(commerce, b"\0asm\x01\0\0\0".to_vec(), vec![], None);
        let image_bytes = image.len();
        // Compressed, so a multi-GiB image fits one upload.
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gzip, &image).unwrap();
        drop(image);
        ic.set_stable_memory(
            commerce,
            gzip.finish().unwrap(),
            pocket_ic::common::rest::BlobCompression::Gzip,
        );
        let cycles = ic.cycle_balance(commerce);
        ic.upgrade_canister(
            commerce,
            wasm("dmsg_commerce"),
            candid::encode_args(()).unwrap(),
            None,
        )
        .unwrap();
        let upgrade_cycles = cycles - ic.cycle_balance(commerce);
        let upgrade = ic
            .fetch_canister_logs(commerce, Principal::anonymous())
            .unwrap()
            .into_iter()
            .map(|l| String::from_utf8_lossy(&l.content).into_owned())
            .find(|l| l.contains("commerce_upgrade"))
            .unwrap();
        let spent = |method: &str, sender: Principal, args: Vec<u8>| -> (u128, Vec<u8>) {
            let cycles = ic.cycle_balance(commerce);
            let reply = ic.update_call(commerce, sender, method, args).unwrap();
            (cycles - ic.cycle_balance(commerce), reply)
        };
        let b = |i: u64| beneficiary(home, &account(i));
        let batch: Vec<_> = (0..64).map(|n| b(n * (subjects / 64))).collect();
        let began = std::time::Instant::now();
        let certified: Result<CertifiedBatch> =
            query(&ic, commerce, person(1), "get_entitlement_batch", (batch,));
        let elapsed = began.elapsed();
        let certified = certified.unwrap();
        assert!(certified.entries.iter().all(|e| e.value.is_some()));
        let batch_bytes = candid::encode_one(Ok::<_, Error>(&certified))
            .unwrap()
            .len();
        let month = month_utc(at + MINUTE).unwrap();
        let entitlement = |i: u64| candid::encode_args((b(i), month, at - 60 * DAY)).unwrap();
        let (paid_cycles, reply) = spent("get_execution_entitlement", home, entitlement(7));
        let paid: Result<ExecutionEntitlement> = candid::decode_one(&reply).unwrap();
        assert_eq!(paid.unwrap().view.plan_snapshot.plan_id, PlanId::Plus);
        let (free_cycles, reply) =
            spent("get_execution_entitlement", home, entitlement(subjects + 7));
        let free: Result<ExecutionEntitlement> = candid::decode_one(&reply).unwrap();
        assert_eq!(free.unwrap().view.lease_revision, 0);
        // Renewal inside the window re-projects and recertifies one subject.
        ic.set_time(pocket_ic::Time::from_nanos_since_unix_epoch(
            millis_to_nanos(at + 30 * DAY - 5 * MINUTE).unwrap(),
        ));
        let (renew_cycles, reply) = spent(
            "refresh_entitlement",
            person(1),
            candid::encode_args((b(11),)).unwrap(),
        );
        let renewed: Result<EntitlementView> = candid::decode_one(&reply).unwrap();
        assert!(renewed.unwrap().valid_until_ms > at + 30 * DAY);
        let status = ic.canister_status(commerce, None).unwrap();
        println!("{upgrade}");
        println!(
            "commerce_capacity subjects={subjects} image_bytes={image_bytes} upgrade_cycles={upgrade_cycles} renew_cycles={renew_cycles} paid_entitlement_cycles={paid_cycles} free_entitlement_cycles={free_cycles} batch64_bytes={batch_bytes} host_query_ms={} heap_bytes={} stable_bytes={}",
            elapsed.as_millis(),
            status.memory_metrics.wasm_memory_size,
            status.memory_metrics.stable_memory_size,
        );
    }
}
