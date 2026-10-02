use super::*;

fn counters(f: &Fixture) -> (u64, u64, bool, u64, u64) {
    candid::decode_args(
        &f.ic
            .query_call(
                f.sns,
                person(1),
                "membership_fixture_counts",
                candid::encode_args(()).unwrap(),
            )
            .unwrap(),
    )
    .unwrap()
}

fn failure(f: &Fixture, error: Option<Error>) {
    void(
        &f.ic,
        f.sns,
        person(1),
        "membership_fixture_failure",
        (error, None::<Error>),
    );
}

fn mock_request(f: &Fixture, id: AccountId) -> PandaClaimRequest {
    let at = time(&f.ic);
    let product = ProductRegistration {
        version: 2,
        environment: Environment::Local,
        product_id: "mock".into(),
        config_version: 1,
        quote_authority: f.sns,
        beneficiary_authority: f.sns,
        adapter: f.sns,
        subject_schema: "fixture-account-v1".into(),
        subject_size: 12,
        merchant: account(person(60)),
        ledgers: vec![f.ledger, f.ledger2],
        terms_hash: Hash::new([101; 32]),
        paused: false,
    };
    let registered: Result<()> = update(
        &f.ic,
        f.commerce,
        f.sns,
        "register_integration_product",
        (product.clone(),),
    );
    registered.unwrap();
    let app = AppRegistration {
        version: 1,
        environment: Environment::Local,
        app_id: "mock".into(),
        config_version: 1,
        origins: vec!["https://dmsg.test".into()],
        user_homes: vec![f.user],
        cose_homes: vec![f.cose],
        product_ids: vec!["mock".into()],
        capabilities: vec![AppCapability::Checkout],
        profiles: vec![],
        authentication_receiver: f.sns,
        action_authority: f.sns,
        paused: false,
    };
    let registered: Result<()> =
        update(&f.ic, f.commerce, f.sns, "register_integration_app", (app,));
    registered.unwrap();
    let scheduled: Result<PandaRatePolicy> = update(
        &f.ic,
        f.membership,
        f.sns,
        "schedule_panda_rate",
        (PandaRatePolicy {
            version: 2,
            environment: Environment::Local,
            policy_version: 2,
            product_ids: vec!["mock".into()],
            r_num: 5000,
            r_den: 1,
            published_at_ms: at - POLICY_NOTICE_MS,
            effective_at_ms: at,
        },),
    );
    scheduled.unwrap();
    let bill = BillingOffer {
        version: 2,
        environment: Environment::Local,
        app_id: "mock".into(),
        product_id: "mock".into(),
        offer_id: Hash::new([210; 32]),
        operation_id: Hash::new([211; 32]),
        beneficiary: Beneficiary {
            product_id: "mock".into(),
            authority_canister: f.sns,
            subject_schema: product.subject_schema,
            subject_bytes: id.to_vec().into(),
        },
        quote_authority: f.sns,
        adapter: f.sns,
        sku: "annual".into(),
        product_terms_hash: product.terms_hash,
        expected_business_revision: 1,
        amount_usd_micros: 10_000_000,
        starts_at_ms: at,
        expires_at_ms: at + 365 * DAY,
        issued_at_ms: at,
        accept_by_ms: at + OFFER_TTL_MS,
        allowed_settlement_methods: vec![SettlementMethod::Panda],
    };
    void(
        &f.ic,
        f.sns,
        person(1),
        "configure_membership_fixture",
        (f.membership, bill.clone()),
    );
    let terms: Result<PandaApplicationTerms> = update(
        &f.ic,
        f.membership,
        person(1),
        "quote_panda_subscription",
        (bill.clone(), f.user, id, Hash::new([44; 32])),
    );
    let terms = terms.unwrap();
    let authorization = approve(
        f,
        &id,
        &bill,
        f.membership,
        ApprovalPurpose::PandaSubscription,
        panda_application_hash(&terms),
        212,
    );
    PandaClaimRequest {
        terms,
        authorization,
    }
}

#[test]
fn temporary_reservation_failures_retry_with_limits_and_archive_without_replaying() {
    let f = Fixture::commercial();
    let account = f.create(1);
    neuron(&f, 100_000_000_000_100, false);
    let request = mock_request(&f, account);
    let id = panda_claim_id(&request.terms);
    let unavailable = Error::Unavailable("temporary product authority".into());
    failure(&f, Some(unavailable.clone()));
    let result: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_panda_claim",
        (request.clone(),),
    );
    assert_eq!(result, Err(unavailable.clone()));
    let view: Result<PandaClaimView> =
        query(&f.ic, f.membership, person(1), "get_panda_claim", (id,));
    assert_eq!(view.unwrap().status, PandaClaimStatus::Checking);
    for error in [
        Error::ExecutionUnknown,
        Error::Pending,
        Error::QuotaExceeded,
    ] {
        failure(&f, Some(error.clone()));
        let retry: Result<PandaClaimView> = update(
            &f.ic,
            f.membership,
            person(1),
            "reconcile_panda_claim",
            (id,),
        );
        assert_eq!(retry, Err(error));
    }
    failure(&f, Some(unavailable.clone()));
    for _ in 4..10 {
        let retry: Result<PandaClaimView> = update(
            &f.ic,
            f.membership,
            person(1),
            "reconcile_panda_claim",
            (id,),
        );
        assert_eq!(retry, Err(unavailable.clone()));
    }
    let limited: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "reconcile_panda_claim",
        (id,),
    );
    assert_eq!(limited, Err(Error::QuotaExceeded));
    assert_eq!(counters(&f).0, 10);
    f.ic.advance_time(Duration::from_millis(MINUTE));
    failure(&f, None);
    void(
        &f.ic,
        f.sns,
        person(1),
        "membership_fixture_callback",
        (id, false),
    );
    let paused: Result<()> = update(&f.ic, f.membership, f.sns, "set_admission_pause", (true,));
    paused.unwrap();
    let recovered: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "reconcile_panda_claim",
        (id,),
    );
    assert_eq!(recovered.unwrap().status, PandaClaimStatus::CoolingDown);
    assert_eq!(counters(&f).0, 11);
    assert!(counters(&f).2);
    let cancelled: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "cancel_panda_application",
        (id,),
    );
    assert_eq!(cancelled.unwrap().status, PandaClaimStatus::Cancelled);
    let before = counters(&f);
    for _ in 0..12 {
        let result: Result<PandaClaimView> = update(
            &f.ic,
            f.membership,
            person(1),
            "reconcile_panda_claim",
            (id,),
        );
        assert_eq!(result.unwrap().status, PandaClaimStatus::Cancelled);
    }
    assert_eq!(counters(&f), before);
    f.ic.advance_time(Duration::from_millis(32 * DAY));
    let swept: Result<u32> = update(
        &f.ic,
        f.membership,
        person(1),
        "sweep_panda_commitments",
        (),
    );
    swept.unwrap();
    f.ic.upgrade_canister(
        f.membership,
        wasm("membership"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let expired: Result<PandaClaimView> =
        query(&f.ic, f.membership, person(1), "get_panda_claim", (id,));
    assert_eq!(expired, Err(Error::ResultExpired));
    let retry: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_panda_claim",
        (request.clone(),),
    );
    assert_eq!(retry, Err(Error::ResultExpired));
    let mut changed = request;
    changed.terms.neuron_id = Hash::new([77; 32]);
    let conflict: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_panda_claim",
        (changed,),
    );
    assert_eq!(conflict, Err(Error::IdempotencyConflict));
    let page: Result<PandaOperationsPage> = query(
        &f.ic,
        f.membership,
        person(1),
        "panda_operations",
        (None::<Hash>, 32u16),
    );
    assert!(page.unwrap().claims.is_empty());
}

#[test]
fn cancellation_during_reservation_ignores_late_errors_and_unlocks_recovery() {
    let f = Fixture::commercial();
    let account = f.create(1);
    let request = mock_request(&f, account);
    let id = panda_claim_id(&request.terms);
    failure(&f, Some(Error::Unavailable("late failure".into())));
    void(
        &f.ic,
        f.sns,
        person(1),
        "membership_fixture_callback",
        (id, true),
    );
    let result: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_panda_claim",
        (request,),
    );
    assert_eq!(result.unwrap().status, PandaClaimStatus::Cancelled);
    assert!(counters(&f).2);
    let recovered: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "reconcile_panda_claim",
        (id,),
    );
    assert_eq!(recovered.unwrap().status, PandaClaimStatus::Cancelled);
    assert_eq!(counters(&f).1, 1);
    assert_eq!(counters(&f).4, 0);
}

#[test]
fn activation_rechecks_a_neuron_observed_just_before_cooling_ended() {
    let f = Fixture::commercial();
    let account = f.create(1);
    neuron(&f, 100_000_000_000_100, false);
    let bill = offer(&f, &account, 220);
    let terms: Result<PandaApplicationTerms> = update(
        &f.ic,
        f.membership,
        person(1),
        "quote_panda_subscription",
        (bill.clone(), f.user, account, Hash::new([44; 32])),
    );
    let terms = terms.unwrap();
    let authorization = approve(
        &f,
        &account,
        &bill,
        f.membership,
        ApprovalPurpose::PandaSubscription,
        panda_application_hash(&terms),
        221,
    );
    let result: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_panda_claim",
        (PandaClaimRequest {
            terms: terms.clone(),
            authorization,
        },),
    );
    let cooling = result.unwrap();
    let ready = cooling.cooling_until_ms.unwrap();
    f.ic.advance_time(Duration::from_millis(ready - time(&f.ic) - 30_000));
    let checked: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "reconcile_panda_claim",
        (cooling.claim_id,),
    );
    assert!(checked.unwrap().observed_at_ms < ready);
    let reads = counters(&f).4;
    neuron(&f, 0, false);
    f.ic.advance_time(Duration::from_millis(ready - time(&f.ic) + 1));
    let fresh = approve(
        &f,
        &account,
        &bill,
        f.membership,
        ApprovalPurpose::PandaSubscription,
        panda_application_hash(&terms),
        222,
    );
    let advanced: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "advance_panda_claim",
        (cooling.claim_id, fresh),
    );
    assert_eq!(advanced, Err(Error::MembershipIneligible));
    assert_eq!(counters(&f).4, reads + 1);
    let rejected: Result<PandaClaimView> = query(
        &f.ic,
        f.membership,
        person(1),
        "get_panda_claim",
        (cooling.claim_id,),
    );
    assert_eq!(rejected.unwrap().status, PandaClaimStatus::Rejected);
}

#[test]
fn concurrent_sns_configuration_checks_share_one_call_and_retry_after_failure() {
    let f = Fixture::commercial();
    let start = counters(&f).3;
    f.ic.advance_time(Duration::from_millis(PANDA_LEASE_MS + 1));
    let run = || {
        let calls: Vec<_> = (0..16)
            .map(|_| {
                f.ic.submit_call(
                    f.membership,
                    person(1),
                    "verify_sns_configuration",
                    candid::encode_args(()).unwrap(),
                )
                .unwrap()
            })
            .collect();
        calls
            .into_iter()
            .map(|id| candid::decode_one::<Result<()>>(&f.ic.await_call(id).unwrap()).unwrap())
            .collect::<Vec<_>>()
    };
    let results = run();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results
        .iter()
        .all(|r| r.is_ok() || *r == Err(Error::Pending)));
    assert_eq!(counters(&f).3, start + 1);
    let pin: Result<()> = update(
        &f.ic,
        f.membership,
        f.sns,
        "set_sns_governance_module_hash",
        (Hash::new([33; 32]),),
    );
    pin.unwrap();
    let results = run();
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(Error::UnsupportedProtocol))
            .count(),
        1
    );
    assert!(results
        .iter()
        .all(|r| *r == Err(Error::UnsupportedProtocol) || *r == Err(Error::Pending)));
    assert_eq!(counters(&f).3, start + 2);
    let hash =
        f.ic.canister_status(f.sns, None)
            .unwrap()
            .module_hash
            .unwrap();
    f.ic.set_controllers(f.sns, None, vec![f.sns]).unwrap();
    let pin: Result<()> = update(
        &f.ic,
        f.membership,
        f.sns,
        "set_sns_governance_module_hash",
        (Hash::new(hash.try_into().unwrap()),),
    );
    pin.unwrap();
    let results = run();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results
        .iter()
        .all(|r| r.is_ok() || *r == Err(Error::Pending)));
    assert_eq!(counters(&f).3, start + 3);
}

#[test]
fn nonlocal_rate_retry_and_pruned_version_identity_survive_upgrade() {
    let ic = PocketIcBuilder::new().with_application_subnet().build();
    let membership = ic.create_canister();
    ic.add_cycles(membership, 10_000_000_000_000);
    ic.install_canister(
        membership,
        wasm("membership"),
        candid::encode_args((MembershipInit {
            environment: Environment::Staging,
            governance: person(1),
            sns_root: person(2),
            panda_ledger: person(3),
            expected_governance_module_hash: Some(Hash::new([1; 32])),
        },))
        .unwrap(),
        None,
    );
    let at = time(&ic);
    let mut policy = PandaRatePolicy {
        version: 2,
        environment: Environment::Staging,
        policy_version: 1,
        product_ids: vec!["dmsg".into()],
        r_num: 5000,
        r_den: 1,
        published_at_ms: at,
        effective_at_ms: at + POLICY_NOTICE_MS + MINUTE,
    };
    let first: Result<PandaRatePolicy> = update(
        &ic,
        membership,
        person(1),
        "schedule_panda_rate",
        (policy.clone(),),
    );
    let first = first.unwrap();
    ic.advance_time(Duration::from_secs(1));
    let retry: Result<PandaRatePolicy> = update(
        &ic,
        membership,
        person(1),
        "schedule_panda_rate",
        (policy.clone(),),
    );
    assert_eq!(retry.unwrap(), first);
    policy.policy_version = 2;
    policy.effective_at_ms += MINUTE;
    let second: Result<PandaRatePolicy> = update(
        &ic,
        membership,
        person(1),
        "schedule_panda_rate",
        (policy.clone(),),
    );
    let second = second.unwrap();
    ic.advance_time(Duration::from_millis(POLICY_NOTICE_MS + 3 * MINUTE));
    policy.policy_version = 3;
    policy.effective_at_ms = time(&ic) + POLICY_NOTICE_MS + MINUTE;
    let third: Result<PandaRatePolicy> = update(
        &ic,
        membership,
        person(1),
        "schedule_panda_rate",
        (policy.clone(),),
    );
    third.unwrap();
    ic.upgrade_canister(
        membership,
        wasm("membership"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let retry: Result<PandaRatePolicy> = update(
        &ic,
        membership,
        person(1),
        "schedule_panda_rate",
        (second.clone(),),
    );
    assert_eq!(retry.unwrap(), second);
    policy.policy_version = 1;
    policy.r_num += 1;
    let reused: Result<PandaRatePolicy> =
        update(&ic, membership, person(1), "schedule_panda_rate", (policy,));
    assert_eq!(reused, Err(Error::VersionConflict));
}

#[test]
#[ignore = "membership-only cycles for 16 concurrent SNS configuration checks"]
fn membership_verification_profile() {
    let mut variants = vec![("current", wasm("membership"))];
    if let Some(path) = std::env::var_os("DMSG_MEMBERSHIP_BASELINE_WASM") {
        variants.push(("baseline", std::fs::read(path).unwrap()));
    }
    for (name, module) in variants {
        let ic = PocketIcBuilder::new().with_application_subnet().build();
        let membership = ic.create_canister();
        let sns = ic.create_canister();
        for id in [membership, sns] {
            ic.add_cycles(id, 10_000_000_000_000);
        }
        ic.install_canister(
            sns,
            wasm("dmsg_test_sns"),
            candid::encode_args(()).unwrap(),
            None,
        );
        ic.install_canister(
            membership,
            module,
            candid::encode_args((MembershipInit {
                environment: Environment::Local,
                governance: sns,
                sns_root: sns,
                panda_ledger: sns,
                expected_governance_module_hash: None,
            },))
            .unwrap(),
            None,
        );
        let before = ic.cycle_balance(membership);
        let calls: Vec<_> = (0..16)
            .map(|_| {
                ic.submit_call(
                    membership,
                    person(1),
                    "verify_sns_configuration",
                    candid::encode_args(()).unwrap(),
                )
                .unwrap()
            })
            .collect();
        let mut pending = 0;
        for id in calls {
            let result: Result<()> = candid::decode_one(&ic.await_call(id).unwrap()).unwrap();
            assert!(result.is_ok() || result == Err(Error::Pending));
            pending += u32::from(result == Err(Error::Pending));
        }
        let burst = before - ic.cycle_balance(membership);
        for _ in 0..pending {
            let retry: Result<()> =
                update(&ic, membership, person(1), "verify_sns_configuration", ());
            retry.unwrap();
        }
        let spent = before - ic.cycle_balance(membership);
        let counts: (u64, u64, bool, u64, u64) = candid::decode_args(
            &ic.query_call(
                sns,
                person(1),
                "membership_fixture_counts",
                candid::encode_args(()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        println!(
            "MEMBERSHIP verification={name} concurrent=16 sns_checks={} pending={pending} burst_cycles={burst} completed_cycles={spent}",
            counts.3
        );
        if name == "current" {
            assert_eq!(counts.3, 1);
        }
    }
}
