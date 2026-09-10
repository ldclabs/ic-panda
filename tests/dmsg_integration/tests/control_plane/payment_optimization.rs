use super::*;

fn upgrade(f: &Fixture) {
    f.ic.upgrade_canister(
        f.payment,
        wasm("dmsg_payment"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
}

fn certified_escrow(f: &Fixture, id: Hash) -> EscrowInfo {
    let batch: Result<CertifiedBatch> = query(
        &f.ic,
        f.payment,
        person(40),
        "get_escrow_certified",
        (vec![id],),
    );
    let batch = batch.unwrap();
    assert_eq!(batch.canister, f.payment);
    assert_eq!(batch.entries.len(), 1);
    let entry = &batch.entries[0];
    assert_eq!(entry.key.as_ref(), id.as_slice());
    let witness: ic_certification::HashTree = cbor2::from_slice(&entry.witness).unwrap();
    let cert: ic_certification::Certificate = cbor2::from_slice(&batch.certificate).unwrap();
    assert_eq!(
        cert.tree.lookup_path([
            b"canister".as_slice(),
            f.payment.as_slice(),
            b"certified_data".as_slice()
        ]),
        ic_certification::LookupResult::Found(&witness.digest()),
    );
    assert_eq!(
        witness.lookup_path([id.as_slice()]),
        ic_certification::LookupResult::Found(entry.value.as_ref().unwrap())
    );
    decode_canonical(entry.value.as_ref().unwrap()).unwrap()
}

#[test]
fn internal_refund_and_revision_writes_preserve_certified_funds() {
    let f = Fixture::new();
    let recipient = f.create(2);
    let e: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (f.order(&recipient, 2, 1),),
    );
    let e = e.unwrap();
    let block = f.fund(&e, e.quote.amount + 17);
    let funded: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "check_funding",
        (e.escrow_id, block),
    );
    let settled: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "finalize_receipt",
        (f.receipt(&funded.unwrap()),),
    );
    let settled = settled.unwrap();
    assert_eq!(certified_escrow(&f, e.escrow_id), settled);
    let refund: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "claim_deposit_refund",
        (e.escrow_id, block),
    );
    assert_eq!(refund.unwrap().amount, 7);
    assert_eq!(certified_escrow(&f, e.escrow_id), settled);
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "set_fee",
        (20u128,),
    );
    let blocked: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "process_transfer",
        (e.escrow_id, 0u64),
    );
    assert_eq!(blocked.unwrap().status, LegStatus::FeeBlocked);
    let revised: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "revise_rejected_transfer",
        (e.escrow_id, 0u64, 20u128),
    );
    let revised = revised.unwrap();
    assert_eq!(certified_escrow(&f, e.escrow_id), settled);
    upgrade(&f);
    assert_eq!(certified_escrow(&f, e.escrow_id), settled);
    let sent: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "process_transfer",
        (e.escrow_id, revised.leg_id),
    );
    assert_eq!(sent.unwrap().status, LegStatus::Succeeded);
    let paid = certified_escrow(&f, e.escrow_id);
    assert_eq!(paid.transferred, 1000);
    assert_eq!(paid.network_fees, 20);
    assert!(funds_conserved(&paid));
}

#[test]
fn concurrent_opens_and_funding_reads_coalesce_and_can_retry() {
    let f = Fixture::new();
    let recipient = f.create(2);
    let inputs = [f.order(&recipient, 2, 1), f.order(&recipient, 2, 2)];
    let calls: Vec<_> = inputs
        .iter()
        .map(|input| {
            f.ic.submit_call(
                f.payment,
                person(40),
                "open_escrow",
                candid::encode_args((input,)).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let results: Vec<Result<EscrowInfo>> = calls
        .into_iter()
        .map(|call| candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap())
        .collect();
    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        1,
        "{results:?}"
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(Error::Pending))
            .count(),
        1
    );
    let e = results.iter().find_map(|r| r.as_ref().ok()).unwrap();
    let retried: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (inputs[results.iter().position(|r| r.is_err()).unwrap()].clone(),),
    );
    retried.unwrap();
    let block = f.fund(e, e.quote.amount);
    let calls: Vec<_> = [40, 41, 42]
        .into_iter()
        .map(|n| {
            f.ic.submit_call(
                f.payment,
                person(n),
                "check_funding",
                candid::encode_args((e.escrow_id, block)).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let results: Vec<Result<EscrowInfo>> = calls
        .into_iter()
        .map(|call| candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap())
        .collect();
    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        1,
        "{results:?}"
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(Error::Pending))
            .count(),
        2
    );
    let retried: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "check_funding",
        (e.escrow_id, block),
    );
    assert_eq!(retried.unwrap().confirmed_in, e.quote.amount);
    // An ordinary read error must also release its guard.
    for _ in 0..2 {
        let missing: Result<EscrowInfo> = update(
            &f.ic,
            f.payment,
            person(40),
            "check_funding",
            (e.escrow_id, 9999u64),
        );
        assert_eq!(missing, Err(Error::NotFound));
    }
}

#[test]
fn controller_disable_and_revocation_racing_an_open_are_enforced() {
    for revoke in [false, true] {
        let f = Fixture::new();
        f.ic.update_canister_settings(
            f.payment,
            None,
            pocket_ic::CanisterSettings {
                controllers: Some(vec![person(40)]),
                ..Default::default()
            },
        )
        .unwrap();
        let recipient = f.create(2);
        let input = f.order(&recipient, 2, 1);
        let call =
            f.ic.submit_call(
                f.payment,
                person(40),
                "open_escrow",
                candid::encode_args((input.clone(),)).unwrap(),
            )
            .unwrap();
        // Submit competing ingress messages without assuming that a controller
        // ingress and a cross-canister callback have a fixed execution order.
        let (method, args) = if revoke {
            (
                "revoke_receipt_signer",
                candid::encode_args((1u64,)).unwrap(),
            )
        } else {
            ("set_orders_enabled", candid::encode_args((false,)).unwrap())
        };
        let changed =
            f.ic.submit_call(f.payment, person(40), method, args)
                .unwrap();
        let enabled = revoke.then(|| {
            f.ic.submit_call(
                f.payment,
                person(40),
                "set_orders_enabled",
                candid::encode_args((true,)).unwrap(),
            )
            .unwrap()
        });
        let result: Result<EscrowInfo> =
            candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap();
        assert_eq!(
            result,
            Err(if revoke {
                Error::Forbidden
            } else {
                Error::Locked
            })
        );
        let changed: Result<()> = candid::decode_one(&f.ic.await_call(changed).unwrap()).unwrap();
        changed.unwrap();
        if let Some(enabled) = enabled {
            let enabled: Result<()> =
                candid::decode_one(&f.ic.await_call(enabled).unwrap()).unwrap();
            enabled.unwrap();
        }
        let missing: Result<EscrowInfo> = query(
            &f.ic,
            f.payment,
            person(40),
            "get_escrow_by_operation",
            (person(40), input.op_id),
        );
        assert_eq!(missing, Err(Error::NotFound));
    }
}

#[test]
fn heap_configuration_and_failed_call_budgets_survive_upgrade() {
    let f = Fixture::new();
    let recipient = f.create(2);
    let opened: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (f.order(&recipient, 2, 1),),
    );
    let e = opened.unwrap();
    let mut invalid = f.order(&recipient, 2, 2);
    invalid.offer.signature = vec![0; 64].into();
    let disabled: Result<()> = update(
        &f.ic,
        f.payment,
        Principal::anonymous(),
        "set_orders_enabled",
        (false,),
    );
    disabled.unwrap();
    upgrade(&f);
    let disabled: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (invalid.clone(),),
    );
    assert_eq!(disabled, Err(Error::Locked));
    let enabled: Result<()> = update(
        &f.ic,
        f.payment,
        Principal::anonymous(),
        "set_orders_enabled",
        (true,),
    );
    enabled.unwrap();
    // Even rejected offer checks consume the global attempt budget. Failed
    // calls must release their in-flight guard, and upgrades cannot reset it.
    for _ in 1..100 {
        let rejected: Result<EscrowInfo> = update(
            &f.ic,
            f.payment,
            person(40),
            "open_escrow",
            (invalid.clone(),),
        );
        assert!(
            matches!(rejected, Err(Error::Forbidden | Error::IntegrityFailed)),
            "{rejected:?}"
        );
    }
    upgrade(&f);
    let limited: Result<EscrowInfo> =
        update(&f.ic, f.payment, person(40), "open_escrow", (invalid,));
    assert_eq!(limited, Err(Error::QuotaExceeded));
    for _ in 0..400 {
        let missing: Result<EscrowInfo> = update(
            &f.ic,
            f.payment,
            person(40),
            "check_funding",
            (e.escrow_id, 9999u64),
        );
        assert_eq!(missing, Err(Error::NotFound));
    }
    upgrade(&f);
    let limited: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "check_funding",
        (e.escrow_id, 9999u64),
    );
    assert_eq!(limited, Err(Error::QuotaExceeded));
    f.ic.advance_time(Duration::from_secs(60));
    let reset: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "check_funding",
        (e.escrow_id, 9999u64),
    );
    assert_eq!(reset, Err(Error::NotFound));
}

#[test]
fn concurrent_reconciliation_completes_a_lost_transfer_once() {
    let f = Fixture::new();
    let recipient = f.create(2);
    let opened: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (f.order(&recipient, 2, 1),),
    );
    let e = opened.unwrap();
    let block = f.fund(&e, e.quote.amount);
    let funded: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "check_funding",
        (e.escrow_id, block),
    );
    let settled: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "finalize_receipt",
        (f.receipt(&funded.unwrap()),),
    );
    settled.unwrap();
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "lose_next_response",
        (),
    );
    let lost: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "process_transfer",
        (e.escrow_id, 0u64),
    );
    assert_eq!(lost, Err(Error::ExecutionUnknown));
    // The fault-injecting ledger appended precisely one outgoing block.
    let calls: Vec<_> = [40, 41, 42]
        .into_iter()
        .map(|n| {
            f.ic.submit_call(
                f.payment,
                person(n),
                "reconcile_transfer",
                candid::encode_args((e.escrow_id, 0u64, block + 1)).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let results: Vec<Result<TransferLeg>> = calls
        .into_iter()
        .map(|call| candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap())
        .collect();
    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        1,
        "{results:?}"
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(Error::Pending))
            .count(),
        2
    );
    let paid = certified_escrow(&f, e.escrow_id);
    assert_eq!(paid.transferred, 1000);
    assert_eq!(paid.network_fees, 10);
    for method in ["process_transfer", "reconcile_transfer"] {
        let args = if method == "process_transfer" {
            candid::encode_args((e.escrow_id, 0u64)).unwrap()
        } else {
            candid::encode_args((e.escrow_id, 0u64, block + 1)).unwrap()
        };
        let retry: Result<TransferLeg> = candid::decode_one(
            &f.ic
                .update_call(f.payment, person(40), method, args)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(retry.unwrap().status, LegStatus::Succeeded);
        assert_eq!(certified_escrow(&f, e.escrow_id), paid);
    }
}

#[test]
fn reserve_claim_survives_upgrade_without_changing_the_certified_balance() {
    let f = Fixture::new();
    let recipient = f.create(2);
    let mut input = f.order(&recipient, 2, 1);
    input.quote.fee_reserve = 60;
    input.quote.amount = 1160;
    input.quote_signature = key(50)
        .sign(digest("dmsg/quote/v1", &input.quote).as_slice())
        .to_bytes()
        .to_vec()
        .into();
    let opened: Result<EscrowInfo> = update(&f.ic, f.payment, person(40), "open_escrow", (input,));
    let e = opened.unwrap();
    let block = f.fund(&e, e.quote.amount);
    let funded: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "check_funding",
        (e.escrow_id, block),
    );
    let settled: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "finalize_receipt",
        (f.receipt(&funded.unwrap()),),
    );
    settled.unwrap();
    for leg in 0..2u64 {
        let paid: Result<TransferLeg> = update(
            &f.ic,
            f.payment,
            person(40),
            "process_transfer",
            (e.escrow_id, leg),
        );
        assert_eq!(paid.unwrap().status, LegStatus::Succeeded);
    }
    let before = certified_escrow(&f, e.escrow_id);
    let reserve: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "claim_fee_reserve",
        (e.escrow_id,),
    );
    let reserve = reserve.unwrap();
    assert_eq!(reserve.amount, 30);
    assert_eq!(certified_escrow(&f, e.escrow_id), before);
    upgrade(&f);
    assert_eq!(certified_escrow(&f, e.escrow_id), before);
    let duplicate: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "claim_fee_reserve",
        (e.escrow_id,),
    );
    assert_eq!(duplicate, Err(Error::FeeBlocked));
    let paid: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "process_transfer",
        (e.escrow_id, reserve.leg_id),
    );
    assert_eq!(paid.unwrap().status, LegStatus::Succeeded);
    let done = certified_escrow(&f, e.escrow_id);
    assert_eq!(done.liabilities, 0);
    assert!(funds_conserved(&done));
}

// Run the identical host test against each build via DMSG_WASM_DIR. Measure
// only payment's balance delta, excluding user/ledger execution and token fees.
fn measured<R>(f: &Fixture, label: &str, run: impl FnOnce() -> R) -> R {
    let before = f.ic.cycle_balance(f.payment);
    let result = run();
    println!(
        "payment_cycles method={label} cycles={}",
        before - f.ic.cycle_balance(f.payment)
    );
    result
}

#[test]
#[ignore = "cycles comparison for dmsg_payment builds"]
fn payment_cycles_profile() {
    let f = Fixture::new();
    let recipient = f.create(2);
    // Repeat the same lifecycle eight times and report per-method medians;
    // keep first-allocation costs visible in the raw output.
    for nonce in 1..=8 {
        let input = f.order(&recipient, 2, nonce);
        let e: Result<EscrowInfo> = measured(&f, "open_escrow", || {
            update(&f.ic, f.payment, person(40), "open_escrow", (input,))
        });
        let e = e.unwrap();
        let block = f.fund(&e, e.quote.amount + 17);
        let funded: Result<EscrowInfo> = measured(&f, "check_funding", || {
            update(
                &f.ic,
                f.payment,
                person(40),
                "check_funding",
                (e.escrow_id, block),
            )
        });
        let funded = funded.unwrap();
        let decided: Result<EscrowInfo> = measured(&f, "finalize_receipt", || {
            update(
                &f.ic,
                f.payment,
                person(99),
                "finalize_receipt",
                (f.receipt(&funded),),
            )
        });
        decided.unwrap();
        for leg_id in 0..2u64 {
            let sent: Result<TransferLeg> = measured(&f, "process_transfer", || {
                update(
                    &f.ic,
                    f.payment,
                    person(99),
                    "process_transfer",
                    (e.escrow_id, leg_id),
                )
            });
            assert_eq!(sent.unwrap().status, LegStatus::Succeeded);
        }
        let refund: Result<TransferLeg> = measured(&f, "claim_deposit_refund", || {
            update(
                &f.ic,
                f.payment,
                person(99),
                "claim_deposit_refund",
                (e.escrow_id, block),
            )
        });
        let refund = refund.unwrap();
        let sent: Result<TransferLeg> = measured(&f, "process_refund", || {
            update(
                &f.ic,
                f.payment,
                person(99),
                "process_transfer",
                (e.escrow_id, refund.leg_id),
            )
        });
        assert_eq!(sent.unwrap().status, LegStatus::Succeeded);
        let done: Result<EscrowInfo> =
            query(&f.ic, f.payment, person(99), "get_escrow", (e.escrow_id,));
        assert!(funds_conserved(&done.unwrap()));
    }
    measured(&f, "upgrade_8_escrows", || {
        f.ic.upgrade_canister(
            f.payment,
            wasm("dmsg_payment"),
            candid::encode_args(()).unwrap(),
            None,
        )
        .unwrap();
    });
}
