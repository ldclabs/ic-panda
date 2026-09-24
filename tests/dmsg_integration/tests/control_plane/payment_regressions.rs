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

fn signed(mut input: OpenEscrow) -> OpenEscrow {
    input.quote_signature = key(50)
        .sign(digest("dmsg/quote/v2", &input.quote).as_slice())
        .to_bytes()
        .into();
    input
}

fn configuration(f: &Fixture, version: Option<u64>) -> (PaymentConfiguration, DeliveryFeePolicy) {
    let proof: Result<CertifiedBatch> = query(
        &f.ic,
        f.payment,
        person(99),
        "get_configuration_certified",
        (None::<u64>, version),
    );
    let proof = proof.unwrap();
    let certificate: ic_certification::Certificate = cbor2::from_slice(&proof.certificate).unwrap();
    let root = match certificate.tree.lookup_path([
        b"canister".as_slice(),
        f.payment.as_slice(),
        b"certified_data".as_slice(),
    ]) {
        ic_certification::LookupResult::Found(root) => root,
        _ => panic!("missing certified data"),
    };
    for entry in &proof.entries {
        let witness: ic_certification::HashTree = cbor2::from_slice(&entry.witness).unwrap();
        assert_eq!(witness.digest().as_slice(), root);
        assert_eq!(
            witness.lookup_path([entry.key.as_ref()]),
            ic_certification::LookupResult::Found(entry.value.as_ref().unwrap().as_ref())
        );
    }
    let config = &proof.entries[0].value.as_ref().unwrap();
    let policy = &proof.entries[2].value.as_ref().unwrap();
    (
        cbor2::from_slice(config).unwrap(),
        cbor2::from_slice(policy).unwrap(),
    )
}

fn fund(f: &Fixture, e: &EscrowInfo, fee: u64) -> EscrowInfo {
    f.mint(e.payer_principal, e.quote.amount + u128::from(fee));
    let sent: std::result::Result<Nat, TransferError> = update(
        &f.ic,
        f.ledger,
        e.payer_principal,
        "icrc1_transfer",
        (TransferArg {
            from_subaccount: e.quote.payer.subaccount,
            to: Account {
                owner: f.payment,
                subaccount: Some(e.subaccount.into_array()),
            },
            amount: e.quote.amount.into(),
            fee: Some(fee.into()),
            memo: None,
            created_at_time: Some(millis_to_nanos(time(&f.ic)).unwrap()),
        },),
    );
    let block = dmsg_runtime::ledger::block_index(sent.unwrap()).unwrap();
    let checked: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(99),
        "check_funding",
        (e.escrow_id, block),
    );
    checked.unwrap()
}

#[test]
fn current_fee_certificates_follow_time_without_an_order_and_survive_upgrade() {
    let f = Fixture::new();
    let now = time(&f.ic);
    let mut policies = Vec::new();
    for version in 2..=256 {
        let p = DeliveryFeePolicy {
            version,
            effective_at_ms: now + 31 * DAY + version * MINUTE,
            rate_bps: 750,
            minimum_atomic: u128::from(version),
        };
        let r: Result<()> = update(
            &f.ic,
            f.payment,
            Principal::from_slice(&[90]),
            "schedule_fee_policy",
            (p.clone(),),
        );
        r.unwrap();
        policies.push(p);
    }
    assert_eq!(configuration(&f, None).1.version, 1);
    let p = policies[30].clone();
    f.ic.advance_time(Duration::from_millis(p.effective_at_ms - time(&f.ic) - 1));
    f.ic.tick();
    assert_eq!(configuration(&f, None).1.version, p.version - 1);
    f.ic.advance_time(Duration::from_millis(1));
    f.ic.tick();
    assert_eq!(configuration(&f, None).1, p);
    let current: DeliveryFeePolicy = query(&f.ic, f.payment, person(99), "get_fee_policy", ());
    assert_eq!(current, p);
    assert_eq!(configuration(&f, Some(1)).1.version, 1);
    upgrade(&f);
    assert_eq!(configuration(&f, None).1, p);
}

#[test]
fn network_fee_maintenance_preserves_old_legs_and_admits_funded_new_orders() {
    let f = Fixture::new();
    let recipient = f.create(2);
    let r: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (f.order(&recipient, 2, 1),),
    );
    let old = fund(&f, &r.unwrap(), 10);
    let r: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(99),
        "finalize_receipt",
        (f.receipt(&old),),
    );
    r.unwrap();
    let before: Result<TransferLeg> = query(
        &f.ic,
        f.payment,
        person(99),
        "get_transfer",
        (old.escrow_id, 0u64),
    );
    for (caller, fee, error) in [
        (person(99), 20u128, Error::Forbidden),
        (Principal::anonymous(), 21, Error::FeeBlocked),
    ] {
        let r: Result<()> = update(&f.ic, f.payment, caller, "set_ledger_fee", (fee,));
        assert_eq!(r, Err(error));
    }
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "set_fee",
        (20u128,),
    );
    let updated: Result<()> = update(
        &f.ic,
        f.payment,
        Principal::anonymous(),
        "set_ledger_fee",
        (20u128,),
    );
    updated.unwrap();
    upgrade(&f);
    assert_eq!(configuration(&f, None).0.ledger_fee, 20);
    let after: Result<TransferLeg> = query(
        &f.ic,
        f.payment,
        person(99),
        "get_transfer",
        (old.escrow_id, 0u64),
    );
    assert_eq!(before, after);
    let stale = f.order(&recipient, 2, 2);
    let denied: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (stale.clone(),),
    );
    assert_eq!(denied, Err(Error::FeeBlocked));
    let mut fresh = stale;
    fresh.quote.fee_reserve = 60;
    fresh.quote.amount = 1160;
    let r: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (signed(fresh),),
    );
    let e = fund(&f, &r.unwrap(), 20);
    let r: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(99),
        "finalize_receipt",
        (f.receipt(&e),),
    );
    r.unwrap();
    for leg in 0..2u64 {
        let paid: Result<TransferLeg> = update(
            &f.ic,
            f.payment,
            person(99),
            "process_transfer",
            (e.escrow_id, leg),
        );
        let paid = paid.unwrap();
        assert_eq!(
            (paid.status, paid.fee, paid.last_failure),
            (LegStatus::Succeeded, 20, None)
        );
    }
    let status = f.ic.canister_status(f.payment, None).unwrap();
    assert_eq!(
        status.memory_metrics.stable_memory_size,
        Nat::from(11 * 1024 * 1024 + 65536u64)
    );
}

#[test]
fn network_fee_changes_during_offer_verification_are_rechecked() {
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
    let opening =
        f.ic.submit_call(
            f.payment,
            person(40),
            "open_escrow",
            candid::encode_args((input.clone(),)).unwrap(),
        )
        .unwrap();
    let changed =
        f.ic.submit_call(
            f.payment,
            person(40),
            "set_ledger_fee",
            candid::encode_args((20u128,)).unwrap(),
        )
        .unwrap();
    let opened: Result<EscrowInfo> =
        candid::decode_one(&f.ic.await_call(opening).unwrap()).unwrap();
    assert_eq!(opened, Err(Error::FeeBlocked));
    let changed: Result<()> = candid::decode_one(&f.ic.await_call(changed).unwrap()).unwrap();
    changed.unwrap();
    let missing: Result<EscrowInfo> = query(
        &f.ic,
        f.payment,
        person(40),
        "get_escrow_by_operation",
        (person(40), input.op_id),
    );
    assert_eq!(missing, Err(Error::NotFound));
}

#[test]
fn failed_authorizations_have_separate_budgets_and_daily_admission_is_atomic() {
    let f = Fixture::with_order_limit(vec![Algorithm::Ed25519, Algorithm::VetKdBls12381], true, 1);
    let recipient = f.create(2);
    let input = f.order(&recipient, 2, 1);
    let mut invalid = input.clone();
    invalid.offer.signature = [0; 64].into();
    let before = configuration(&f, None);
    for _ in 0..10 {
        let denied: Result<EscrowInfo> = update(
            &f.ic,
            f.payment,
            person(40),
            "open_escrow",
            (invalid.clone(),),
        );
        assert_eq!(denied, Err(Error::IntegrityFailed));
    }
    assert_eq!(configuration(&f, None), before);
    upgrade(&f);
    let limited: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (input.clone(),),
    );
    assert_eq!(limited, Err(Error::QuotaExceeded));
    f.ic.advance_time(Duration::from_millis(MINUTE));
    let mut other = input.clone();
    other.op_id = Hash::new([2; 32]);
    other.quote.quote_id = Hash::new([2; 32]);
    other.quote.payer = account(person(41));
    let pending: Vec<_> = [(person(40), input.clone()), (person(41), signed(other))]
        .into_iter()
        .map(|(payer, input)| {
            f.ic.submit_call(
                f.payment,
                payer,
                "open_escrow",
                candid::encode_args((input,)).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let results: Vec<Result<EscrowInfo>> = pending
        .into_iter()
        .map(|call| candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap())
        .collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(Error::QuotaExceeded))
            .count(),
        1
    );
    let e = results.into_iter().find_map(|r| r.ok()).unwrap();
    // A full opening budget does not prevent funding or refunds, including after upgrade.
    upgrade(&f);
    let funded = fund(&f, &e, 10);
    assert!(funded.funding_ref.is_some());
    let replay: Result<EscrowInfo> =
        update(&f.ic, f.payment, e.payer_principal, "open_escrow", (input,));
    if e.payer_principal == person(40) {
        assert_eq!(replay.unwrap(), funded);
    }
}

#[test]
fn transfer_rejection_reasons_survive_upgrade_and_clear_on_success() {
    let f = Fixture::new();
    let recipient = f.create(2);
    let r: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (f.order(&recipient, 2, 1),),
    );
    let e = fund(&f, &r.unwrap(), 10);
    let r: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(99),
        "finalize_receipt",
        (f.receipt(&e),),
    );
    r.unwrap();
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "reject_next_transfers",
        (1u32,),
    );
    let failed: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(99),
        "process_transfer",
        (e.escrow_id, 0u64),
    );
    let failed = failed.unwrap();
    assert_eq!(
        failed.last_failure,
        Some(TransferFailure::TemporarilyUnavailable)
    );
    upgrade(&f);
    let restored: Result<TransferLeg> = query(
        &f.ic,
        f.payment,
        person(99),
        "get_transfer",
        (e.escrow_id, 0u64),
    );
    assert_eq!(restored.unwrap(), failed);
    let paid: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(99),
        "process_transfer",
        (e.escrow_id, 0u64),
    );
    let paid = paid.unwrap();
    assert_eq!(
        (paid.status, paid.last_failure),
        (LegStatus::Succeeded, None)
    );
}

#[test]
fn global_authorization_limit_survives_upgrade_and_leaves_ledger_budget_available() {
    let f = Fixture::new();
    let recipient = f.create(2);
    let input = f.order(&recipient, 2, 1);
    let opened: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (input.clone(),),
    );
    let opened = opened.unwrap();
    for n in 0..199u64 {
        let payer = Principal::self_authenticating(n.to_be_bytes());
        let mut bad = input.clone();
        bad.op_id = digest("test/auth-budget", &n);
        bad.quote.quote_id = bad.op_id;
        bad.quote.payer = account(payer);
        bad.offer.signature = [0; 64].into();
        let denied: Result<EscrowInfo> =
            update(&f.ic, f.payment, payer, "open_escrow", (signed(bad),));
        assert_eq!(denied, Err(Error::IntegrityFailed));
    }
    upgrade(&f);
    let mut valid = input;
    valid.op_id = Hash::new([9; 32]);
    valid.quote.quote_id = valid.op_id;
    valid.quote.payer = account(person(41));
    let limited: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(41),
        "open_escrow",
        (signed(valid),),
    );
    assert_eq!(limited, Err(Error::QuotaExceeded));
    assert!(fund(&f, &opened, 10).funding_ref.is_some());
}

// Real entry-point admissions, unique payers and a shared recipient. This measures
// quote/order/index growth; funded transfer history is covered by separate tests.
#[test]
#[ignore = "payment capacity measurement at 1,000 and 10,000 escrows"]
fn payment_scale_profile() {
    let f = Fixture::with_order_limit(
        vec![Algorithm::Ed25519, Algorithm::VetKdBls12381],
        true,
        100_000,
    );
    let recipient = f.create(2);
    let template = f.order(&recipient, 2, 1);
    let mut samples = Vec::new();
    for batch in 0..500u64 {
        if batch % 5 == 0 {
            f.ic.advance_time(Duration::from_millis(MINUTE));
            f.ic.tick();
        }
        let at = time(&f.ic);
        let mut offer = template.offer.clone();
        offer.offer.issued_at = at;
        offer.offer.expires_at = at + 15 * MINUTE;
        offer.signature = key(2)
            .sign(digest("dmsg/payment-offer/v1", &offer.offer).as_slice())
            .to_bytes()
            .into();
        let mut pending = Vec::new();
        for j in 0..20u64 {
            let n = batch * 20 + j + 1;
            let payer = Principal::self_authenticating(n.to_be_bytes());
            let mut input = template.clone();
            input.op_id = digest("test/payment-growth", &n);
            input.offer = offer.clone();
            input.quote.quote_id = input.op_id;
            input.quote.offer_digest = digest("dmsg/payment-offer/v1", &input.offer.offer);
            input.quote.payer = account(payer);
            input.quote.created_at = at;
            input.quote.fund_by = at + 15 * MINUTE;
            input.quote.accept_by = at + 45 * MINUTE;
            pending.push(
                f.ic.submit_call(
                    f.payment,
                    payer,
                    "open_escrow",
                    candid::encode_args((signed(input),)).unwrap(),
                )
                .unwrap(),
            );
        }
        let mut last = None;
        for call in pending {
            let opened: Result<EscrowInfo> =
                candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap();
            last = Some(opened.unwrap());
        }
        let count = (batch + 1) * 20;
        if [1_000, 10_000].contains(&count) {
            samples.push(last.unwrap());
            let before = f.ic.cycle_balance(f.payment);
            upgrade(&f);
            let cycles = before - f.ic.cycle_balance(f.payment);
            let status = f.ic.canister_status(f.payment, None).unwrap();
            for expected in &samples {
                let actual: Result<EscrowInfo> = query(
                    &f.ic,
                    f.payment,
                    person(99),
                    "get_escrow",
                    (expected.escrow_id,),
                );
                assert_eq!(actual.unwrap(), *expected);
                let proof: Result<CertifiedBatch> = query(
                    &f.ic,
                    f.payment,
                    person(99),
                    "get_escrow_certified",
                    (vec![expected.escrow_id],),
                );
                let proof = proof.unwrap();
                let certificate: ic_certification::Certificate =
                    cbor2::from_slice(&proof.certificate).unwrap();
                let witness: ic_certification::HashTree =
                    cbor2::from_slice(&proof.entries[0].witness).unwrap();
                assert_eq!(
                    certificate.tree.lookup_path([
                        b"canister".as_slice(),
                        f.payment.as_slice(),
                        b"certified_data".as_slice()
                    ]),
                    ic_certification::LookupResult::Found(witness.digest().as_slice())
                );
                assert_eq!(
                    proof.entries[0].value.as_ref().unwrap().as_ref(),
                    canonical(expected)
                );
            }
            println!("payment_growth orders={count} upgrade_cycles={cycles} stable_bytes={} wasm_bytes={}", status.memory_metrics.stable_memory_size, status.memory_metrics.wasm_memory_size);
        }
    }
}

#[test]
fn fee_policy_switch_rejects_old_quotes_but_preserves_accepted_settlement_terms() {
    let f = Fixture::new();
    let recipient = f.create(2);
    let effective_at = time(&f.ic) + 31 * DAY;
    let next = DeliveryFeePolicy {
        version: 2,
        effective_at_ms: effective_at,
        rate_bps: 500,
        minimum_atomic: 150,
    };
    let r: Result<()> = update(
        &f.ic,
        f.payment,
        Principal::from_slice(&[90]),
        "schedule_fee_policy",
        (next,),
    );
    r.unwrap();
    let signer = ReceiptSigner {
        epoch: 2,
        public_key: key(50).verifying_key().to_bytes().into(),
        valid_from: time(&f.ic),
        valid_until: effective_at + DAY,
        revoked: false,
    };
    let r: Result<()> = update(
        &f.ic,
        f.payment,
        Principal::anonymous(),
        "rotate_receipt_signer",
        (signer,),
    );
    r.unwrap();
    f.ic.advance_time(Duration::from_millis(effective_at - time(&f.ic) - MINUTE));
    f.ic.tick();
    let mut input = f.order(&recipient, 2, 1);
    input.quote.signer_epoch = 2;
    let r: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (signed(input),),
    );
    let e = fund(&f, &r.unwrap(), 10);
    let mut stale = f.order(&recipient, 2, 2);
    stale.quote.signer_epoch = 2;
    f.ic.advance_time(Duration::from_millis(MINUTE));
    let denied: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (signed(stale),),
    );
    assert_eq!(denied, Err(Error::PolicyStale));
    let mut receipt = f.receipt(&e);
    receipt.receipt.signer_epoch = 2;
    receipt.signature = key(50)
        .sign(digest("dmsg/admission-receipt/v2", &receipt.receipt).as_slice())
        .to_bytes()
        .into();
    let r: Result<EscrowInfo> =
        update(&f.ic, f.payment, person(99), "finalize_receipt", (receipt,));
    assert_eq!(r.unwrap().quote.service_fee, 100);
    let platform: Result<TransferLeg> = query(
        &f.ic,
        f.payment,
        person(99),
        "get_transfer",
        (e.escrow_id, 1u64),
    );
    assert_eq!(platform.unwrap().amount, 100);
}
