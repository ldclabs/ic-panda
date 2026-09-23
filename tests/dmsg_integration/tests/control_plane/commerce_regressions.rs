use super::*;

fn decision(f: &Fixture, id: &AccountId, op: u8) -> MembershipDecision {
    let r = claim_request(f, id, op);
    let at = time(&f.ic);
    MembershipDecision {
        decision_id: Hash::new([op; 32]),
        claim_id: Hash::new([op.wrapping_add(1); 32]),
        kind: DecisionKind::Apply,
        policy: MembershipPolicy {
            version: r.policy_version,
            product_id: "dmsg".into(),
            benefit_id: r.benefit_id,
            threshold: Threshold::AnnualPrice {
                price_cents: 1000,
                r_num: 5000,
                r_den: 1,
            },
            effective_at_ms: 0,
            subsidy_units: 1,
        },
        request: r,
        required_atomic: 5_000_000_000_000,
        starts_at_ms: at,
        expires_at_ms: next_year(at).unwrap(),
        apply_by_ms: at + 5 * MINUTE,
        observed_at_ms: at,
        qualification_until_ms: at + MAX_LEASE_MS,
    }
}

#[test]
fn sns_switch_uses_the_decision_boundary_after_delivery_delay() {
    for upgrade in [false, true] {
        let f = Fixture::commercial();
        let id = f.create(1);
        let first = decision(&f, &id, 201);
        let r: Result<MembershipDecisionReceipt> = update(
            &f.ic,
            f.commerce,
            f.membership,
            "apply_membership_decision",
            (first.clone(),),
        );
        assert_eq!(r.unwrap().outcome, DecisionOutcome::Applied);
        f.ic.advance_time(Duration::from_millis(DAY));
        let mut next = decision(&f, &id, 203);
        next.expires_at_ms = first.expires_at_ms;
        next.request.change = ClaimChange::Replace {
            previous_claim: first.claim_id,
        };
        next.request.term = TermRule::Fixed {
            starts_at_ms: next.starts_at_ms,
            expires_at_ms: first.expires_at_ms,
        };
        if upgrade {
            let mut plan = default_plans(1)[2].clone();
            plan.membership_policy_version = Some(2);
            next.request.change = ClaimChange::Upgrade {
                previous_claim: first.claim_id,
            };
            next.request.benefit_id = plan_digest(&plan);
            next.request.policy_version = 2;
            next.policy.version = 2;
            next.policy.benefit_id = next.request.benefit_id;
            next.policy.threshold = Threshold::AnnualPrice {
                price_cents: 5000,
                r_num: 5000,
                r_den: 1,
            };
            next.required_atomic = 25_000_000_000_000;
        }
        next.request.expected_business_revision = 1;
        next.request.authorization.action_digest = claim_action_digest(&next.request);
        f.ic.advance_time(Duration::from_millis(1));
        let r: Result<MembershipDecisionReceipt> = update(
            &f.ic,
            f.commerce,
            f.membership,
            "apply_membership_decision",
            (next.clone(),),
        );
        let r = r.unwrap();
        assert_eq!(r.outcome, DecisionOutcome::Applied);
        let replay: Result<MembershipDecisionReceipt> = update(
            &f.ic,
            f.commerce,
            f.membership,
            "apply_membership_decision",
            (next.clone(),),
        );
        assert_eq!(replay, Ok(r));
        let e: Result<ExecutionEntitlement> = update(
            &f.ic,
            f.commerce,
            f.user,
            "get_execution_entitlement",
            (
                beneficiary(f.user, &id),
                month_utc(time(&f.ic)).unwrap(),
                f.account_id(1, &id).created_at_ms,
            ),
        );
        let e = e.unwrap();
        assert_eq!(e.view.active_contract_id, Some(next.claim_id));
        let earlier = e
            .month
            .segments
            .iter()
            .find(|s| s.source_contract_id == Some(first.claim_id))
            .unwrap();
        let later = e
            .month
            .segments
            .iter()
            .find(|s| s.source_contract_id == Some(next.claim_id))
            .unwrap();
        assert_eq!(earlier.end_ms, later.start_ms);
        assert_eq!(later.start_ms, next.starts_at_ms);
    }
}

#[test]
fn scheduled_weights_compare_against_the_preceding_catalog() {
    let f = Fixture::commercial();
    let catalogs: Vec<Catalog> = query(
        &f.ic,
        f.commerce,
        person(1),
        "list_catalogs",
        (None::<u64>,),
    );
    let mut second = catalogs[0].clone();
    let (_, boundary) = month_bounds(month_utc(time(&f.ic) + 40 * DAY).unwrap()).unwrap();
    second.version = 2;
    second.effective_at_ms = boundary;
    for p in &mut second.plans {
        p.catalog_version = 2;
        p.weights = ExecutionWeights {
            version: 2,
            ed25519: 2,
            ecdsa_secp256k1: 2,
        };
    }
    let r: Result<()> = update(&f.ic, f.commerce, f.sns, "schedule_policy", (second,));
    r.unwrap();
    let mut third = catalogs[0].clone();
    third.version = 3;
    third.effective_at_ms = boundary + DAY;
    for p in &mut third.plans {
        p.catalog_version = 3;
    }
    let r: Result<()> = update(&f.ic, f.commerce, f.sns, "schedule_policy", (third,));
    assert!(matches!(r, Err(Error::InvalidInput(_))));
}

#[test]
fn fee_increase_within_reserve_is_repaired_without_policy_notice() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let o = open(&f, &id, 210, OrderAction::Subscribe { plan: PlanId::Plus });
    let block = deposit(&f, &o, 2, 200);
    check(&f, &o, block);
    let t: Result<TransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "claim_deposit_refund",
        (o.order_id, block),
    );
    let t = t.unwrap();
    let _: () = update(&f.ic, f.ledger, person(1), "set_fee", (20u128,));
    let rejected: Result<TransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "process_transfer",
        (o.order_id, t.transfer_id),
    );
    assert_eq!(rejected.unwrap().status, MerchantTransferStatus::Rejected);
    assert!(20 <= o.input.quote.fee_reserve);
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let detail: Result<MerchantTransfer> = query(
        &f.ic,
        f.commerce,
        person(2),
        "get_transfer",
        (o.order_id, t.transfer_id),
    );
    assert!(
        matches!(detail.unwrap().last_error, Some(TransferError::BadFee { expected_fee }) if expected_fee == 20u64)
    );

    let repaired: Result<MerchantTransfer> = update(
        &f.ic,
        f.commerce,
        person(2),
        "revise_rejected_transfer",
        (o.order_id, t.transfer_id, 20u128),
    );
    let repaired = repaired.unwrap();
    assert_eq!(repaired.fee, 20);
    assert_eq!(repaired.amount, 180);
    let sent: Result<TransferProgress> = update(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "process_transfer",
        (o.order_id, repaired.transfer_id),
    );
    assert_eq!(sent.unwrap().status, MerchantTransferStatus::Succeeded);
    assert_money(&order_details(&f, o.order_id));

    void(&f.ic, f.ledger, person(1), "set_fee", (40u128,));
    let verified: Result<()> = update(&f.ic, f.commerce, f.sns, "verify_ledger_configuration", ());
    verified.unwrap();
    let mut request = o.input.quote.request.clone();
    request.op_id = Hash::new([239; 32]);
    let q: Result<OrderQuote> = query(&f.ic, f.commerce, person(1), "quote_order", (request,));
    let q = q.unwrap();
    let i = intent(&f, &id, 1, 239, f.commerce, order_digest(&q));
    let blocked: Result<BillingOrder> = update(
        &f.ic,
        f.commerce,
        person(1),
        "open_order",
        (dmsg_types::billing::OpenOrder {
            quote: q,
            authorization: i,
        },),
    );
    assert_eq!(blocked, Err(Error::FeeBlocked));
}

#[test]
fn public_advancement_returns_no_private_order_or_transfer_fields() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let o = open(&f, &id, 212, OrderAction::Subscribe { plan: PlanId::Plus });
    let q: Result<BillingOrder> = query(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "get_operation",
        (o.order_id,),
    );
    assert_eq!(q, Err(Error::Forbidden));
    let raw =
        f.ic.update_call(
            f.commerce,
            Principal::anonymous(),
            "reconcile_order",
            candid::encode_args((o.order_id,)).unwrap(),
        )
        .unwrap();
    let r: Result<OrderProgress> = candid::decode_one(&raw).unwrap();
    assert_eq!(r.unwrap().status, o.status);
    assert!(candid::decode_one::<Result<BillingOrder>>(&raw).is_err());
    let block = deposit(&f, &o, 2, 200);
    check(&f, &o, block);
    let raw =
        f.ic.update_call(
            f.commerce,
            Principal::anonymous(),
            "claim_deposit_refund",
            candid::encode_args((o.order_id, block)).unwrap(),
        )
        .unwrap();
    let r: Result<TransferProgress> = candid::decode_one(&raw).unwrap();
    let r = r.unwrap();
    assert!(candid::decode_one::<Result<MerchantTransfer>>(&raw).is_err());
    let private: Result<MerchantTransfer> = query(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "get_transfer",
        (o.order_id, r.transfer_id),
    );
    assert_eq!(private, Err(Error::Forbidden));
    let original_payer: Result<MerchantTransfer> = query(
        &f.ic,
        f.commerce,
        person(2),
        "get_transfer",
        (o.order_id, r.transfer_id),
    );
    assert_eq!(original_payer.unwrap().to.owner, person(2));
}

#[test]
fn unverifiable_refresh_has_a_persistent_cooldown_and_can_recover() {
    let f = Fixture::commercial();
    let id = f.create(1);
    neuron(&f, 5_000_000_000_100, false);
    let request = claim_request(&f, &id, 214);
    approve(&f, &id, 1, &request.authorization);
    let first: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_claim",
        (request.clone(),),
    );
    let first = first.unwrap();
    f.ic.advance_time(Duration::from_millis(MIN_COOLING_MS + 1));
    approve(&f, &id, 1, &request.authorization);
    let active: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "advance_application",
        (first.claim_id,),
    );
    assert_eq!(active.unwrap().status, ClaimStatus::Active);
    f.ic.advance_time(Duration::from_millis(MAX_LEASE_MS + 1));
    // get_neuron now returns no matching fact, so membership is unverifiable.
    let _: () = update(&f.ic, f.sns, person(1), "set_neuron", (None::<TestNeuron>,));
    let v1: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (beneficiary(f.user, &id),),
    );
    let v1 = v1.unwrap();
    assert_eq!(v1.source_status, SourceStatus::Unverifiable);
    let before = f.ic.cycle_balance(f.commerce);
    let v2: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (beneficiary(f.user, &id),),
    );
    let v2 = v2.unwrap();
    println!(
        "Immediate unknown retry: lease {} -> {}, commerce cycles {}",
        v1.lease_revision,
        v2.lease_revision,
        before - f.ic.cycle_balance(f.commerce)
    );
    assert_eq!(v2, v1);
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let cached: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (beneficiary(f.user, &id),),
    );
    assert_eq!(cached.unwrap(), v1);
    neuron(&f, 5_000_000_000_100, false);
    f.ic.advance_time(Duration::from_millis(MINUTE + 1));
    let recovered: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (beneficiary(f.user, &id),),
    );
    assert_eq!(recovered.unwrap().source_status, SourceStatus::Active);
}

#[test]
fn failed_authorization_creates_no_order_and_success_is_counted_once() {
    let f = Fixture::with_order_limit(vec![Algorithm::Ed25519, Algorithm::VetKdBls12381], false, 1);
    let id = f.create(1);
    let q: Result<OrderQuote> = query(
        &f.ic,
        f.commerce,
        person(1),
        "quote_order",
        (QuoteOrder {
            op_id: Hash::new([208; 32]),
            beneficiary: beneficiary(f.user, &id),
            action: OrderAction::Subscribe { plan: PlanId::Plus },
            expected_business_revision: 0,
            payer: account(person(1)),
        },),
    );
    let q = q.unwrap();
    let i = intent(&f, &id, 1, 208, f.commerce, order_digest(&q));
    let input = dmsg_types::billing::OpenOrder {
        quote: q,
        authorization: i.clone(),
    };
    for _ in 0..3 {
        let r: Result<BillingOrder> =
            update(&f.ic, f.commerce, person(1), "open_order", (input.clone(),));
        assert!(r.is_err());
    }
    let order_id = digest(
        "dmsg/commerce/order-id/v1",
        &(f.commerce, person(1), Hash::new([208; 32])),
    );
    let saved: Result<BillingOrder> =
        query(&f.ic, f.commerce, person(1), "get_operation", (order_id,));
    assert_eq!(saved, Err(Error::NotFound));
    approve(&f, &id, 1, &i);
    let calls = (0..4)
        .map(|_| {
            f.ic.submit_call(
                f.commerce,
                person(1),
                "open_order",
                candid::encode_args((input.clone(),)).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let replies = calls
        .into_iter()
        .map(|call| {
            let reply: Result<BillingOrder> =
                candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap();
            reply.unwrap()
        })
        .collect::<Vec<_>>();
    let opened = replies[0].clone();
    assert!(replies.iter().all(|o| o == &opened));
    for _ in 0..3 {
        let replay: Result<BillingOrder> =
            update(&f.ic, f.commerce, person(1), "open_order", (input.clone(),));
        assert_eq!(replay, Ok(opened.clone()));
    }
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let mut next = input.clone();
    next.quote.request.op_id = Hash::new([209; 32]);
    next.authorization = intent(&f, &id, 1, 209, f.commerce, order_digest(&next.quote));
    approve(&f, &id, 1, &next.authorization);
    let limited: Result<BillingOrder> = update(&f.ic, f.commerce, person(1), "open_order", (next,));
    assert_eq!(limited, Err(Error::QuotaExceeded));
    f.ic.advance_time(Duration::from_millis(46 * MINUTE));
    let cancelled: Result<OrderProgress> =
        update(&f.ic, f.commerce, person(1), "reconcile_order", (order_id,));
    assert_eq!(cancelled.unwrap().status, OrderStatus::Cancelled);
}

#[test]
fn storage_requires_paid_membership_and_prorates_to_its_endpoint() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let request = QuoteOrder {
        op_id: Hash::new([220; 32]),
        beneficiary: beneficiary(f.user, &id),
        action: OrderAction::Storage {
            product_id: Hash::new([120; 32]),
        },
        expected_business_revision: 0,
        payer: account(person(1)),
    };
    let free: Result<OrderQuote> = query(&f.ic, f.commerce, person(1), "quote_order", (request,));
    assert_eq!(free, Err(Error::MembershipIneligible));
    let base = open(&f, &id, 221, OrderAction::Subscribe { plan: PlanId::Plus });
    let block = deposit(
        &f,
        &base,
        1,
        base.input.quote.amount_atomic + base.input.quote.fee_reserve,
    );
    check(&f, &base, block);
    let v: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (beneficiary(f.user, &id),),
    );
    let v = v.unwrap();
    let end = v.next_limit_change_at_ms.unwrap();
    let start = v.issued_at_ms;
    f.ic.advance_time(Duration::from_millis((end - start) / 2));
    let o = open(
        &f,
        &id,
        222,
        OrderAction::Storage {
            product_id: Hash::new([120; 32]),
        },
    );
    assert_eq!(o.input.quote.amount_atomic, 500_000);
    let block = deposit(
        &f,
        &o,
        1,
        o.input.quote.amount_atomic + o.input.quote.fee_reserve,
    );
    check(&f, &o, block);
    let v: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (beneficiary(f.user, &id),),
    );
    assert_eq!(v.unwrap().addons[0].expires_at_ms, end);
}

#[test]
fn catalogs_are_paginated_and_cache_survives_upgrade() {
    let f = Fixture::commercial();
    let catalogs: Vec<Catalog> = query(
        &f.ic,
        f.commerce,
        person(1),
        "list_catalogs",
        (None::<u64>,),
    );
    let base = catalogs[0].clone();
    let at = time(&f.ic);
    for version in 2..=66 {
        let mut c = base.clone();
        c.version = version;
        c.effective_at_ms = at + (30 + version) * DAY;
        for p in &mut c.plans {
            p.catalog_version = version;
        }
        let r: Result<()> = update(&f.ic, f.commerce, f.sns, "schedule_policy", (c,));
        r.unwrap();
    }
    let first: Vec<Catalog> = query(
        &f.ic,
        f.commerce,
        person(1),
        "list_catalogs",
        (None::<u64>,),
    );
    let second: Vec<Catalog> = query(
        &f.ic,
        f.commerce,
        person(1),
        "list_catalogs",
        (Some(first.last().unwrap().version),),
    );
    assert_eq!(first.len(), 64);
    assert_eq!(
        second.iter().map(|c| c.version).collect::<Vec<_>>(),
        vec![65, 66]
    );
    f.ic.advance_time(Duration::from_millis(33 * DAY));
    let before: Catalog = update(&f.ic, f.commerce, person(1), "refresh_catalog", ());
    assert_eq!(before.version, 3);
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let after: Catalog = update(&f.ic, f.commerce, person(1), "refresh_catalog", ());
    assert_eq!(before, after);
}

#[test]
#[ignore = "commerce growth and upgrade profile; local measurements, not production capacity"]
fn commerce_upgrade_profile() {
    let f = Fixture::with_order_limit(
        vec![Algorithm::Ed25519, Algorithm::VetKdBls12381],
        false,
        100_000,
    );
    let id = f.create(1);
    for n in 1..=1000u64 {
        if n % 10 == 1 {
            f.ic.advance_time(Duration::from_millis(16 * MINUTE));
            f.ic.tick();
        }
        let q: Result<OrderQuote> = query(
            &f.ic,
            f.commerce,
            person(1),
            "quote_order",
            (QuoteOrder {
                op_id: digest("test/commerce-growth", &n),
                beneficiary: beneficiary(f.user, &id),
                action: OrderAction::Subscribe { plan: PlanId::Plus },
                expected_business_revision: 0,
                payer: account(person(1)),
            },),
        );
        let q = q.unwrap();
        let mut i = intent(&f, &id, 1, 1, f.commerce, order_digest(&q));
        i.application_id = q.request.op_id;
        approve(&f, &id, 1, &i);
        let o: Result<BillingOrder> = update(
            &f.ic,
            f.commerce,
            person(1),
            "open_order",
            (dmsg_types::billing::OpenOrder {
                quote: q,
                authorization: i,
            },),
        );
        let o = o.unwrap();
        let block = deposit(&f, &o, 2, 200);
        check(&f, &o, block);
        let t: Result<TransferProgress> = update(
            &f.ic,
            f.commerce,
            person(2),
            "claim_deposit_refund",
            (o.order_id, block),
        );
        let t = t.unwrap();
        let sent: Result<TransferProgress> = update(
            &f.ic,
            f.commerce,
            person(2),
            "process_transfer",
            (o.order_id, t.transfer_id),
        );
        assert_eq!(sent.unwrap().status, MerchantTransferStatus::Succeeded);
        if [1, 100, 1000].contains(&n) {
            let before = f.ic.cycle_balance(f.commerce);
            let r: Result<OrderProgress> = update(
                &f.ic,
                f.commerce,
                person(1),
                "reconcile_order",
                (o.order_id,),
            );
            r.unwrap();
            let no_change_cycles = before - f.ic.cycle_balance(f.commerce);
            let before = f.ic.cycle_balance(f.commerce);
            f.ic.upgrade_canister(
                f.commerce,
                wasm("dmsg_commerce"),
                candid::encode_args(()).unwrap(),
                None,
            )
            .unwrap();
            let upgrade_cycles = before - f.ic.cycle_balance(f.commerce);
            let stable_bytes = f.ic.get_stable_memory(f.commerce).len();
            assert_money(&order_details(&f, o.order_id));
            let proof: Result<CertifiedBatch> = query(
                &f.ic,
                f.commerce,
                person(1),
                "get_order_certified",
                (o.order_id,),
            );
            assert!(!proof.unwrap().certificate.is_empty());
            println!("commerce_growth orders={n} deposits={n} transfers={n} upgrade_cycles={upgrade_cycles} no_change_cycles={no_change_cycles} stable_bytes={stable_bytes}");
        }
    }
}

#[test]
fn authorization_attempt_limits_survive_upgrade_without_blocking_funds() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let opened = open(&f, &id, 230, OrderAction::Subscribe { plan: PlanId::Plus });
    let mut input = opened.input.clone();
    input.quote.request.op_id = Hash::new([231; 32]);
    input.authorization = intent(&f, &id, 1, 231, f.commerce, order_digest(&input.quote));
    for _ in 0..9 {
        let r: Result<BillingOrder> =
            update(&f.ic, f.commerce, person(1), "open_order", (input.clone(),));
        assert_eq!(r, Err(Error::NotFound));
    }
    let r: Result<BillingOrder> =
        update(&f.ic, f.commerce, person(1), "open_order", (input.clone(),));
    assert_eq!(r, Err(Error::QuotaExceeded));
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let r: Result<BillingOrder> =
        update(&f.ic, f.commerce, person(1), "open_order", (input.clone(),));
    assert_eq!(r, Err(Error::QuotaExceeded));
    let block = deposit(&f, &opened, 2, 200);
    assert_eq!(check(&f, &opened, block).refundable, 200);
    f.ic.advance_time(Duration::from_millis(MINUTE + 1));
    let r: Result<BillingOrder> = update(&f.ic, f.commerce, person(1), "open_order", (input,));
    assert_eq!(r, Err(Error::NotFound));
}
