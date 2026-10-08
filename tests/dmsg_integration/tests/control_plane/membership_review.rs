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
        beneficiary_authorities: vec![f.sns],
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
        product_ids: vec!["mock".into()],
        capabilities: vec![AppCapability::Checkout],
        profiles: vec![],
        authentication_receiver: f.sns,
        action_authority: f.sns,
        action_schema: None,
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
    neuron(&f, 100_000_000_000_100);
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
    // The reentrant cancellation returned its durable view; the release it could
    // not make beside the in-flight reservation follows once that call returns.
    assert!(!counters(&f).2);
    assert_eq!(counters(&f).1, 1);
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
    neuron(&f, 100_000_000_000_100);
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
    neuron(&f, 0);
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
    void(&f.ic, f.sns, person(1), "set_decimals", (6u8,));
    f.ic.advance_time(Duration::from_millis(PANDA_LEASE_MS + 1));
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
    void(&f.ic, f.sns, person(1), "set_decimals", (8u8,));
    let results = run();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results
        .iter()
        .all(|r| r.is_ok() || *r == Err(Error::Pending)));
    assert_eq!(counters(&f).3, start + 3);
}

#[test]
fn a_neuron_read_returning_after_the_sns_verification_expired_issues_no_lease() {
    let f = Fixture::commercial();
    let account = f.create(1);
    neuron(&f, 100_000_000_000_100);
    // Verify at a known time, then apply two minutes before that verification expires.
    f.ic.advance_time(Duration::from_millis(PANDA_LEASE_MS + 1));
    let verified: Result<()> = update(
        &f.ic,
        f.membership,
        person(1),
        "verify_sns_configuration",
        (),
    );
    verified.unwrap();
    f.ic.advance_time(Duration::from_millis(PANDA_LEASE_MS - 2 * MINUTE));
    let bill = offer(&f, &account, 230);
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
        231,
    );
    void(&f.ic, f.sns, person(1), "delay_neuron_read", (6u8,));
    let reads = counters(&f).4;
    let pending =
        f.ic.submit_call(
            f.membership,
            person(1),
            "request_panda_claim",
            candid::encode_args((PandaClaimRequest {
                terms,
                authorization,
            },))
            .unwrap(),
        )
        .unwrap();
    for _ in 0..100 {
        if counters(&f).4 > reads {
            break;
        }
        f.ic.tick();
    }
    // The read started under a fresh verification; it returns after the verification expired.
    assert_eq!(counters(&f).4, reads + 1);
    f.ic.advance_time(Duration::from_millis(3 * MINUTE));
    let claim =
        candid::decode_one::<Result<PandaClaimView>>(&f.ic.await_call(pending).unwrap()).unwrap();
    let claim = claim.unwrap();
    assert_eq!(claim.status, PandaClaimStatus::Checking);
    assert_eq!(claim.eligibility, Eligibility::Unverifiable);
    assert!(claim.cooling_until_ms.is_none());
    assert!(claim.valid_until_ms <= time(&f.ic));
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

/// Loads host-built images written by membership's `capacity_image` from
/// DMSG_MEMBERSHIP_IMAGE_DIR and measures the upgrade, hourly refreshes and
/// sweeping to zero at that size.
#[test]
#[ignore = "100k/1M-claim capacity; build the images with membership capacity_image first"]
fn membership_capacity_profile() {
    use dmsg_types::membership::MembershipStats;
    let dir = PathBuf::from(std::env::var_os("DMSG_MEMBERSHIP_IMAGE_DIR").expect("image dir"));
    // Matches membership::capacity: fixture time, actors, neurons and 64 due records of each kind.
    let at = 1_800_000_000_000u64;
    let actor = |i: u64| Principal::from_slice(&digest("membership capacity actor", &i)[..20]);
    let neuron_id = |i: u64| digest("membership capacity neuron", &i);
    for claims in [100_000u64, 1_000_000] {
        let Ok(file) = std::fs::File::open(dir.join(format!("membership-{claims}.bin"))) else {
            continue;
        };
        let image_bytes = file.metadata().unwrap().len();
        let ic = PocketIcBuilder::new().with_application_subnet().build();
        ic.set_time(pocket_ic::Time::from_nanos_since_unix_epoch(
            millis_to_nanos(at + MINUTE).unwrap(),
        ));
        let sns = ic.create_canister();
        assert_eq!(
            sns.to_text(),
            "xp3jw-ot777-77777-aaaaa-cai",
            "update membership::capacity::SNS"
        );
        let membership = ic.create_canister();
        for id in [sns, membership] {
            ic.add_cycles(id, 100_000_000_000_000_000);
        }
        ic.install_canister(
            sns,
            wasm("dmsg_test_sns"),
            candid::encode_args(()).unwrap(),
            None,
        );
        // An empty module, so nothing runs against the image before the upgrade.
        ic.install_canister(membership, b"\0asm\x01\0\0\0".to_vec(), vec![], None);
        // Streamed and compressed, so a multi-GiB image fits one upload.
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::copy(&mut std::io::BufReader::new(file), &mut gzip).unwrap();
        ic.set_stable_memory(
            membership,
            gzip.finish().unwrap(),
            pocket_ic::common::rest::BlobCompression::Gzip,
        );
        let cycles = ic.cycle_balance(membership);
        ic.upgrade_canister(
            membership,
            wasm("membership"),
            candid::encode_args(()).unwrap(),
            None,
        )
        .unwrap();
        let upgrade_cycles = cycles - ic.cycle_balance(membership);
        let upgrade = ic
            .fetch_canister_logs(membership, Principal::anonymous())
            .unwrap()
            .into_iter()
            .map(|l| String::from_utf8_lossy(&l.content).into_owned())
            .find(|l| l.contains("membership_upgrade"))
            .unwrap();
        let stats: MembershipStats = query(&ic, membership, person(1), "membership_stats", ());
        assert_eq!(
            (stats.live_claims, stats.full_claims, stats.tombstones),
            (claims, claims + 64, 0)
        );
        // Every lease has ended; an owner's actor renews it with a fresh neuron read.
        ic.set_time(pocket_ic::Time::from_nanos_since_unix_epoch(
            millis_to_nanos(at + 32 * DAY).unwrap(),
        ));
        let refresh = |i: u64| -> u128 {
            void(
                &ic,
                sns,
                person(1),
                "set_neuron",
                (Some(TestNeuron {
                    id: Some(TestNeuronId {
                        id: neuron_id(i).to_vec(),
                    }),
                    permissions: vec![TestPermission {
                        principal: Some(actor(i)),
                        permission_type: vec![3, 4],
                    }],
                    cached_neuron_stake_e8s: 1_000_000_000_000_000_000,
                    neuron_fees_e8s: 0,
                    dissolve_state: Some(TestDissolve::DissolveDelaySeconds(400 * 86_400)),
                }),),
            );
            let page: Result<PandaOperationsPage> = query(
                &ic,
                membership,
                actor(i),
                "panda_operations",
                (None::<Hash>, 1u16),
            );
            let id = page.unwrap().claims[0].claim_id;
            let cycles = ic.cycle_balance(membership);
            let view: Result<PandaClaimView> =
                update(&ic, membership, actor(i), "refresh_panda_claim", (id,));
            let view = view.unwrap();
            assert_eq!(view.eligibility, Eligibility::Eligible);
            assert!(view.valid_until_ms > at + 32 * DAY);
            cycles - ic.cycle_balance(membership)
        };
        // The first refresh also verifies the SNS configuration, once per hour.
        let verify_refresh_cycles = refresh(1_000);
        let refresh_cycles = refresh(claims - 7);
        let mut sweeps = vec![];
        loop {
            let cycles = ic.cycle_balance(membership);
            let swept: Result<u32> =
                update(&ic, membership, person(1), "sweep_panda_commitments", ());
            let swept = swept.unwrap();
            if swept == 0 {
                break;
            }
            sweeps.push((swept, cycles - ic.cycle_balance(membership)));
        }
        // 32 releases and 32 compactions per call: two full calls.
        assert_eq!(sweeps.iter().map(|s| s.0).collect::<Vec<_>>(), [64, 64]);
        let stats: MembershipStats = query(&ic, membership, person(1), "membership_stats", ());
        assert_eq!(
            (stats.live_claims, stats.full_claims, stats.tombstones),
            (claims - 64, claims, 64)
        );
        println!("{upgrade}");
        println!(
            "membership_capacity claims={claims} image_bytes={image_bytes} upgrade_cycles={upgrade_cycles} verify_refresh_cycles={verify_refresh_cycles} refresh_cycles={refresh_cycles} sweep64_cycles={} stable_pages={}",
            sweeps.iter().map(|s| s.1.to_string()).collect::<Vec<_>>().join("/"),
            stats.stable_pages,
        );
    }
}
