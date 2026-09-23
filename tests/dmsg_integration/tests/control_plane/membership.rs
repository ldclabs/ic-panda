use super::*;

fn cooling(f: &Fixture, id: &AccountId, op: u8) -> (ClaimRequest, ClaimView) {
    neuron(f, 5_000_000_000_100, false);
    let r = claim_request(f, id, op);
    approve(f, id, 1, &r.authorization);
    let first: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_claim",
        (r.clone(),),
    );
    let first = first.unwrap();
    assert_eq!(first.status, ClaimStatus::CoolingDown);
    (r, first)
}

fn advance(f: &Fixture, claim: Hash) -> Result<ClaimView> {
    update(
        &f.ic,
        f.membership,
        person(1),
        "advance_application",
        (claim,),
    )
}

fn operation(f: &Fixture, claim: Hash) -> ClaimView {
    let view: Result<ClaimView> = query(&f.ic, f.membership, person(1), "get_operation", (claim,));
    view.unwrap()
}

fn upgrade(f: &Fixture) {
    f.ic.upgrade_canister(
        f.membership,
        wasm("membership"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
}

fn activate(f: &Fixture, id: &AccountId, r: &ClaimRequest, first: &ClaimView) -> ClaimView {
    f.ic.advance_time(Duration::from_millis(MIN_COOLING_MS + 1));
    approve(f, id, 1, &r.authorization);
    let active = advance(f, first.claim_id).unwrap();
    assert_eq!(active.status, ClaimStatus::Active);
    active
}

fn adapter_fixture() -> Fixture {
    Fixture::with_membership_config(|c| c.products[0].adapter = c.governance)
}

fn adapter_faults(f: &Fixture, lose: bool, unavailable: bool, reject_close: bool) {
    void(
        &f.ic,
        f.sns,
        person(1),
        "set_adapter_faults",
        (lose, unavailable, reject_close),
    );
}

fn adapter_calls(f: &Fixture) -> (u64, u64) {
    let reply =
        f.ic.query_call(
            f.sns,
            person(1),
            "adapter_calls",
            candid::encode_args(()).unwrap(),
        )
        .unwrap();
    candid::decode_args(&reply).unwrap()
}

#[test]
fn unapproved_requests_use_no_claim_capacity_and_attempt_limits_survive_upgrade() {
    let f = Fixture::with_membership_config(|c| {
        c.max_claims = 1;
        c.hourly_applications = 1;
        c.subsidy_budget = 1;
    });
    let id = f.create(1);
    neuron(&f, 5_000_000_000_100, false);
    for op in 1..=10 {
        let r = claim_request(&f, &id, op);
        let claim = claim_id(f.membership, &r.authorization);
        let rejected: Result<ClaimView> =
            update(&f.ic, f.membership, person(1), "request_claim", (r,));
        assert_eq!(rejected, Err(Error::NotFound));
        let missing: Result<ClaimView> =
            query(&f.ic, f.membership, person(1), "get_operation", (claim,));
        assert_eq!(missing, Err(Error::NotFound));
    }
    let r = claim_request(&f, &id, 221);
    let throttled: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_claim",
        (r.clone(),),
    );
    assert_eq!(throttled, Err(Error::QuotaExceeded));
    upgrade(&f);
    let throttled: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_claim",
        (r.clone(),),
    );
    assert_eq!(throttled, Err(Error::QuotaExceeded));
    f.ic.advance_time(Duration::from_millis(MINUTE + 1));
    approve(&f, &id, 1, &r.authorization);
    let calls: Vec<_> = (0..2)
        .map(|_| {
            f.ic.submit_call(
                f.membership,
                person(1),
                "request_claim",
                candid::encode_args((r.clone(),)).unwrap(),
            )
            .unwrap()
        })
        .collect();
    for call in calls {
        let result: Result<ClaimView> =
            candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap();
        match result {
            Ok(v) => assert_eq!(v.claim_id, claim_id(f.membership, &r.authorization)),
            Err(e) => assert_eq!(e, Error::Pending),
        }
    }
    assert_eq!(
        operation(&f, claim_id(f.membership, &r.authorization)).status,
        ClaimStatus::CoolingDown
    );
}

#[test]
fn admission_reclaims_another_subjects_expired_budget_after_upgrade() {
    let f = Fixture::with_membership_config(|c| c.subsidy_budget = 1);
    let id = f.create(1);
    let other = f.create(2);
    let (_, first) = cooling(&f, &id, 222);
    upgrade(&f);
    f.ic.advance_time(Duration::from_millis(DAY + 1));
    let mut r = claim_request(&f, &other, 223);
    r.neuron_id = Hash::new([45; 32]);
    r.authorization.actor = person(2);
    r.authorization.action_digest = claim_action_digest(&r);
    approve(&f, &other, 2, &r.authorization);
    void(
        &f.ic,
        f.sns,
        person(1),
        "set_neuron",
        (Some(TestNeuron {
            id: Some(TestNeuronId { id: vec![45; 32] }),
            permissions: vec![TestPermission {
                principal: Some(person(2)),
                permission_type: vec![1, 2, 4, 5, 6],
            }],
            cached_neuron_stake_e8s: 5_000_000_000_100,
            neuron_fees_e8s: 100,
            dissolve_state: Some(TestDissolve::DissolveDelaySeconds(400 * 24 * 3600)),
        }),),
    );
    let next: Result<ClaimView> = update(&f.ic, f.membership, person(2), "request_claim", (r,));
    assert_eq!(next.unwrap().status, ClaimStatus::CoolingDown);
    assert_eq!(operation(&f, first.claim_id).status, ClaimStatus::Released);
    let swept: Result<u32> = update(
        &f.ic,
        f.membership,
        Principal::anonymous(),
        "sweep_expired_claims",
        (),
    );
    assert_eq!(swept, Ok(0));
}

#[test]
fn known_stake_loss_ends_the_application_and_new_stake_cools_again() {
    let f = Fixture::new();
    let id = f.create(1);
    let (r, first) = cooling(&f, &id, 224);
    f.ic.advance_time(Duration::from_millis(MIN_COOLING_MS + 1));
    approve(&f, &id, 1, &r.authorization);
    neuron(&f, 100, false);
    assert_eq!(
        advance(&f, first.claim_id),
        Err(Error::MembershipIneligible)
    );
    assert_eq!(operation(&f, first.claim_id).status, ClaimStatus::Released);
    neuron(&f, 5_000_000_000_100, false);
    assert_eq!(
        advance(&f, first.claim_id).unwrap().status,
        ClaimStatus::Released
    );
    let (_, second) = cooling(&f, &id, 225);
    assert_eq!(
        advance(&f, second.claim_id).unwrap().status,
        ClaimStatus::CoolingDown
    );
}

#[test]
fn pause_during_sns_callback_blocks_activation_but_preserves_existing_refresh() {
    let f = Fixture::new();
    let id = f.create(1);
    let (r, first) = cooling(&f, &id, 226);
    f.ic.advance_time(Duration::from_millis(MIN_COOLING_MS + 1));
    approve(&f, &id, 1, &r.authorization);
    void(
        &f.ic,
        f.sns,
        person(1),
        "pause_on_neuron_read",
        (f.membership,),
    );
    assert!(matches!(
        advance(&f, first.claim_id),
        Err(Error::Unavailable(_))
    ));
    assert_eq!(
        operation(&f, first.claim_id).status,
        ClaimStatus::CoolingDown
    );
    assert!(matches!(
        advance(&f, first.claim_id),
        Err(Error::Unavailable(_))
    ));
    let resumed: Result<()> = update(&f.ic, f.membership, f.sns, "set_admission_pause", (false,));
    resumed.unwrap();
    let active = advance(&f, first.claim_id).unwrap();
    assert_eq!(active.status, ClaimStatus::Active);
    let paused: Result<()> = update(&f.ic, f.membership, f.sns, "set_admission_pause", (true,));
    paused.unwrap();
    f.ic.advance_time(Duration::from_millis(MAX_LEASE_MS + 1));
    let refreshed: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "refresh_claim",
        (first.claim_id,),
    );
    let refreshed = refreshed.unwrap();
    assert_eq!(refreshed.eligibility, Eligibility::Eligible);
    assert!(refreshed.valid_until_ms > active.valid_until_ms);
}

#[test]
fn repeated_product_close_never_revives_released_claims() {
    let f = Fixture::new();
    let id = f.create(1);
    let (r, first) = cooling(&f, &id, 227);
    let active = activate(&f, &id, &r, &first);
    let i = intent(
        &f,
        &id,
        1,
        228,
        f.membership,
        close_claim_digest(active.claim_id),
    );
    approve(&f, &id, 1, &i);
    let closing: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_change",
        (active.claim_id, i.clone()),
    );
    assert_eq!(closing.unwrap().status, ClaimStatus::Closing);
    f.ic.advance_time(Duration::from_millis(MAX_LEASE_MS + 1));
    let released: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "refresh_claim",
        (active.claim_id,),
    );
    let released = released.unwrap();
    assert_eq!(released.status, ClaimStatus::Released);
    upgrade(&f);
    for _ in 0..2 {
        let replay: Result<ClaimView> = update(
            &f.ic,
            f.commerce,
            person(1),
            "release_replaced_claim",
            (beneficiary(f.user, &id), active.claim_id),
        );
        assert_eq!(replay.unwrap(), released);
    }
    let replay: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_change",
        (active.claim_id, i),
    );
    assert_eq!(replay.unwrap(), released);
}

#[test]
fn first_delivery_skips_lookup_and_lost_apply_and_close_acks_reconcile_after_upgrade() {
    let f = adapter_fixture();
    let id = f.create(1);
    let (_, first) = cooling(&f, &id, 229);
    f.ic.advance_time(Duration::from_millis(MIN_COOLING_MS + 1));
    adapter_faults(&f, true, false, false);
    assert_eq!(advance(&f, first.claim_id), Err(Error::ExecutionUnknown));
    assert_eq!(adapter_calls(&f), (1, 0));
    assert_eq!(operation(&f, first.claim_id).status, ClaimStatus::Applying);
    f.ic.advance_time(Duration::from_millis(DAY + 1));
    upgrade(&f);
    let swept: Result<u32> = update(&f.ic, f.membership, person(1), "sweep_expired_claims", ());
    assert_eq!(swept, Ok(0));
    adapter_faults(&f, false, true, false);
    let unavailable: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "reconcile_claim",
        (first.claim_id,),
    );
    assert!(matches!(unavailable, Err(Error::Unavailable(_))));
    adapter_faults(&f, false, false, false);
    let recovered: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "reconcile_claim",
        (first.claim_id,),
    );
    assert_eq!(recovered.unwrap().status, ClaimStatus::Active);
    assert_eq!(adapter_calls(&f), (1, 2));
    adapter_faults(&f, true, false, false);
    let i = intent(
        &f,
        &id,
        1,
        230,
        f.membership,
        close_claim_digest(first.claim_id),
    );
    let close: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_change",
        (first.claim_id, i),
    );
    assert_eq!(close, Err(Error::ExecutionUnknown));
    assert_eq!(adapter_calls(&f), (2, 2));
    f.ic.advance_time(Duration::from_millis(MAX_LEASE_MS + 1));
    upgrade(&f);
    let swept: Result<u32> = update(&f.ic, f.membership, person(1), "sweep_expired_claims", ());
    assert_eq!(swept, Ok(0));
    let closed: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "reconcile_claim",
        (first.claim_id,),
    );
    assert_eq!(closed.unwrap().status, ClaimStatus::Closing);
    let released: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "reconcile_claim",
        (first.claim_id,),
    );
    assert_eq!(released.unwrap().status, ClaimStatus::Released);
    assert_eq!(adapter_calls(&f), (2, 3));
}

#[test]
fn rejected_close_can_be_reauthorized_with_a_new_immutable_decision() {
    let f = adapter_fixture();
    let id = f.create(1);
    let (r, first) = cooling(&f, &id, 231);
    let active = activate(&f, &id, &r, &first);
    adapter_faults(&f, false, false, true);
    let i = intent(
        &f,
        &id,
        1,
        232,
        f.membership,
        close_claim_digest(active.claim_id),
    );
    let rejected: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_change",
        (active.claim_id, i),
    );
    let rejected = rejected.unwrap();
    assert_eq!(rejected.status, ClaimStatus::Active);
    adapter_faults(&f, false, false, false);
    let i = intent(
        &f,
        &id,
        1,
        233,
        f.membership,
        close_claim_digest(active.claim_id),
    );
    let closed: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_change",
        (active.claim_id, i),
    );
    let closed = closed.unwrap();
    assert_eq!(closed.status, ClaimStatus::Closing);
    assert_ne!(closed.decision_id, rejected.decision_id);
    assert_eq!(adapter_calls(&f), (3, 0));
}

#[test]
fn conflicting_policy_dates_are_rejected_and_effective_index_survives_upgrade() {
    let f = adapter_fixture();
    let id = f.create(1);
    let old: Result<MembershipPolicy> =
        query(&f.ic, f.membership, person(1), "get_policy", (1u64,));
    let mut next = old.unwrap();
    next.version = 4;
    next.effective_at_ms = time(&f.ic) + 31 * DAY;
    let result: Result<()> = update(
        &f.ic,
        f.membership,
        f.sns,
        "schedule_policy",
        (next.clone(),),
    );
    result.unwrap();
    next.version = 5;
    let conflict: Result<()> = update(&f.ic, f.membership, f.sns, "schedule_policy", (next,));
    assert_eq!(conflict, Err(Error::VersionConflict));
    upgrade(&f);
    f.ic.advance_time(Duration::from_millis(31 * DAY + 1));
    let mut r = claim_request(&f, &id, 234);
    let old: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_claim",
        (r.clone(),),
    );
    assert_eq!(old, Err(Error::PolicyStale));
    r.policy_version = 4;
    r.authorization.action_digest = claim_action_digest(&r);
    neuron(&f, 5_000_000_000_100, false);
    let admitted: Result<ClaimView> = update(&f.ic, f.membership, person(1), "request_claim", (r,));
    assert_eq!(admitted.unwrap().status, ClaimStatus::CoolingDown);
}

#[test]
#[should_panic(expected = "duplicate product")]
fn initialization_rejects_duplicate_product_ids() {
    Fixture::with_membership_config(|c| c.products.push(c.products[0].clone()));
}

#[test]
#[should_panic(expected = "product: AuthRequired")]
fn initialization_rejects_anonymous_adapter() {
    Fixture::with_membership_config(|c| c.products[0].adapter = Principal::anonymous());
}

#[test]
#[ignore = "membership cycles and upgrade growth profile"]
fn membership_cycles_profile() {
    for history in [0u8, 8, 32] {
        let f = Fixture::new();
        let id = f.create(1);
        for op in 1..=history {
            let (_, first) = cooling(&f, &id, op);
            let cancelled: Result<ClaimView> = update(
                &f.ic,
                f.membership,
                person(1),
                "cancel_application",
                (first.claim_id,),
            );
            assert_eq!(cancelled.unwrap().status, ClaimStatus::Released);
            f.ic.advance_time(Duration::from_millis(MINUTE + 1));
        }
        neuron(&f, 5_000_000_000_100, false);
        let request = claim_request(&f, &id, 245);
        approve(&f, &id, 1, &request.authorization);
        let before = f.ic.cycle_balance(f.membership);
        let first: Result<ClaimView> = update(
            &f.ic,
            f.membership,
            person(1),
            "request_claim",
            (request.clone(),),
        );
        let first = first.unwrap();
        let admission_cycles = before - f.ic.cycle_balance(f.membership);
        f.ic.advance_time(Duration::from_millis(MIN_COOLING_MS + 1));
        approve(&f, &id, 1, &request.authorization);
        let before = f.ic.cycle_balance(f.membership);
        let active = advance(&f, first.claim_id).unwrap();
        assert_eq!(active.status, ClaimStatus::Active);
        let activation_cycles = before - f.ic.cycle_balance(f.membership);
        f.ic.advance_time(Duration::from_millis(MAX_LEASE_MS + 1));
        let before = f.ic.cycle_balance(f.membership);
        let fresh: Result<ClaimView> = update(
            &f.ic,
            f.membership,
            person(1),
            "refresh_claim",
            (first.claim_id,),
        );
        let fresh = fresh.unwrap();
        let refresh_cycles = before - f.ic.cycle_balance(f.membership);
        let before = f.ic.cycle_balance(f.membership);
        let same: Result<ClaimView> = update(
            &f.ic,
            f.membership,
            person(1),
            "refresh_claim",
            (first.claim_id,),
        );
        assert_eq!(same.unwrap(), fresh);
        let cached_cycles = before - f.ic.cycle_balance(f.membership);
        let before = f.ic.cycle_balance(f.membership);
        upgrade(&f);
        let upgrade_cycles = before - f.ic.cycle_balance(f.membership);
        assert_eq!(operation(&f, first.claim_id), fresh);
        println!("membership_growth history={history} admission_cycles={admission_cycles} activation_cycles={activation_cycles} refresh_cycles={refresh_cycles} cached_cycles={cached_cycles} upgrade_cycles={upgrade_cycles} stable_bytes={}", f.ic.get_stable_memory(f.membership).len());
    }
}
