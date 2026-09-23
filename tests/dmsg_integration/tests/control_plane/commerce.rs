use super::*;
use dmsg_protocol::{billing::*, membership::*};
use dmsg_types::{billing::*, membership::*};
use serde::Serialize;

#[path = "commerce_regressions.rs"]
mod regressions;

#[path = "membership.rs"]
mod membership_tests;

fn intent(
    f: &Fixture,
    id: &AccountId,
    actor: u8,
    op: u8,
    service: Principal,
    action: Hash,
) -> MembershipIntent {
    MembershipIntent {
        application_id: Hash::new([op; 32]),
        environment: Environment::Local,
        service_canister: service,
        beneficiary: beneficiary(f.user, id),
        actor: person(actor),
        action_digest: action,
        nonce: Hash::new([op.wrapping_add(1); 32]),
        valid_until_ms: time(&f.ic) + DAY - 1,
    }
}
fn approve(f: &Fixture, id: &AccountId, n: u8, i: &MembershipIntent) {
    f.mutate(
        n,
        id,
        AccountCommand::AuthorizeMembership { intent: i.clone() },
    )
    .unwrap();
}
fn open(f: &Fixture, id: &AccountId, op: u8, action: OrderAction) -> BillingOrder {
    let v: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (beneficiary(f.user, id),),
    );
    let revision = v.map_or(0, |v| v.business_revision);
    let q: Result<OrderQuote> = query(
        &f.ic,
        f.commerce,
        person(1),
        "quote_order",
        (QuoteOrder {
            op_id: Hash::new([op; 32]),
            beneficiary: beneficiary(f.user, id),
            action,
            expected_business_revision: revision,
            payer: account(person(1)),
        },),
    );
    let q = q.unwrap();
    let i = intent(f, id, 1, op, f.commerce, order_digest(&q));
    approve(f, id, 1, &i);
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
    o.unwrap()
}
fn deposit(f: &Fixture, o: &BillingOrder, who: u8, amount: u128) -> u64 {
    f.mint(person(who), amount + 10);
    let r: std::result::Result<Nat, TransferError> = update(
        &f.ic,
        f.ledger,
        person(who),
        "icrc1_transfer",
        (TransferArg {
            from_subaccount: None,
            to: Account {
                owner: f.commerce,
                subaccount: Some(o.receive_subaccount.into_array()),
            },
            amount: amount.into(),
            fee: Some(10u64.into()),
            memo: None,
            created_at_time: Some(millis_to_nanos(time(&f.ic)).unwrap()),
        },),
    );
    r.unwrap().0.to_string().parse().unwrap()
}
fn check(f: &Fixture, o: &BillingOrder, block: u64) -> BillingOrder {
    let r: Result<OrderProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "check_order_funding",
        (o.order_id, block),
    );
    r.unwrap();
    order_details(f, o.order_id)
}

fn order_details(f: &Fixture, id: Hash) -> BillingOrder {
    let r: Result<BillingOrder> = query(&f.ic, f.commerce, person(1), "get_operation", (id,));
    r.unwrap()
}

fn assert_money(o: &BillingOrder) {
    assert_eq!(
        o.confirmed_in,
        o.service_reserve
            + o.earned
            + o.refundable
            + o.fee_reserve
            + o.outgoing
            + o.transferred
            + o.network_fees
    );
}

#[test]
fn commerce_cash_deposits_close_fence_and_upgrade_survive_restart() {
    let f = Fixture::new();
    let id = f.create(1);
    let o = open(&f, &id, 91, OrderAction::Subscribe { plan: PlanId::Plus });
    let wrong = deposit(&f, &o, 2, 200);
    let awaiting = check(&f, &o, wrong);
    assert_eq!(awaiting.status, OrderStatus::AwaitingFunding);
    assert_money(&awaiting);
    let under = deposit(&f, &o, 1, 100);
    check(&f, &o, under);
    let block = deposit(
        &f,
        &o,
        1,
        o.input.quote.amount_atomic + o.input.quote.fee_reserve + 100,
    );
    let active = check(&f, &o, block);
    assert_eq!(active.status, OrderStatus::Active);
    assert_money(&active);
    assert_eq!(check(&f, &o, block), active);
    if let Some(dir) = std::env::var_os("DMSG_COMMERCE_FIXTURE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let order_proof: Result<CertifiedBatch> = query(
            &f.ic,
            f.commerce,
            person(1),
            "get_order_certified",
            (o.order_id,),
        );
        let entitlement: Result<CertifiedBatch> = query(
            &f.ic,
            f.commerce,
            person(1),
            "get_entitlement_batch",
            (vec![beneficiary(f.user, &id)],),
        );
        let catalog: Result<CertifiedBatch> =
            query(&f.ic, f.commerce, person(1), "get_catalog", ());
        let bytes = canonical(&(
            1u16,
            "dmsg-commerce/1",
            time(&f.ic),
            ByteBuf::from(f.ic.root_key().unwrap()),
            f.user,
            f.commerce,
            id.clone(),
            order_proof.unwrap(),
            entitlement.unwrap(),
            catalog.unwrap(),
        ));
        std::fs::write(dir.join("cash-active.cbor"), bytes).unwrap();
    }
    let repeated = deposit(&f, &o, 1, 500);
    let after = check(&f, &o, repeated);
    assert_eq!(after.activated_contract_id, active.activated_contract_id);
    assert_money(&after);
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
    assert!(e.is_ok());
    let i = intent(&f, &id, 1, 92, f.commerce, refund_digest(o.order_id));
    approve(&f, &id, 1, &i);
    let closing: Result<BillingOrder> = update(
        &f.ic,
        f.commerce,
        person(1),
        "request_refund",
        (o.order_id, i),
    );
    let closing = closing.unwrap();
    assert_eq!(closing.status, OrderStatus::Closing);
    assert!(closing.close_effective_at_ms.unwrap() > time(&f.ic));
    let early: Result<OrderProgress> = update(
        &f.ic,
        f.commerce,
        person(1),
        "reconcile_order",
        (o.order_id,),
    );
    assert_eq!(early, Err(Error::Pending));
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    f.ic.advance_time(Duration::from_millis(MAX_LEASE_MS + 1));
    let refunded: Result<OrderProgress> = update(
        &f.ic,
        f.commerce,
        person(1),
        "reconcile_order",
        (o.order_id,),
    );
    refunded.unwrap();
    let refunded = order_details(&f, o.order_id);
    assert_eq!(refunded.status, OrderStatus::RefundCommitted);
    assert_money(&refunded);
    assert!(refunded.refunded_principal < o.input.quote.amount_atomic);
    let leg: Result<TransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "claim_deposit_refund",
        (o.order_id, block),
    );
    let leg = leg.unwrap();
    let detail: Result<MerchantTransfer> = query(
        &f.ic,
        f.commerce,
        person(1),
        "get_transfer",
        (o.order_id, leg.transfer_id),
    );
    assert_eq!(detail.unwrap().to, account(person(1)));
    void(&f.ic, f.ledger, person(1), "lose_next_response", ());
    let unknown: Result<TransferProgress> = update(
        &f.ic,
        f.commerce,
        person(1),
        "process_transfer",
        (o.order_id, leg.transfer_id),
    );
    assert_eq!(unknown, Err(Error::ExecutionUnknown));
    let paid: Result<TransferProgress> = update(
        &f.ic,
        f.commerce,
        person(1),
        "process_transfer",
        (o.order_id, leg.transfer_id),
    );
    assert_eq!(paid.unwrap().status, MerchantTransferStatus::Succeeded);

    let rejected_leg: Result<TransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "claim_deposit_refund",
        (o.order_id, wrong),
    );
    let rejected_leg = rejected_leg.unwrap();
    void(&f.ic, f.ledger, person(2), "reject_next_transfers", (1u32,));
    let rejected: Result<TransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "process_transfer",
        (o.order_id, rejected_leg.transfer_id),
    );
    assert_eq!(rejected.unwrap().status, MerchantTransferStatus::Rejected);
    let replacement: Result<MerchantTransfer> = update(
        &f.ic,
        f.commerce,
        person(2),
        "revise_rejected_transfer",
        (o.order_id, rejected_leg.transfer_id, 10u128),
    );
    let replacement = replacement.unwrap();
    let superseded: Result<TransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "process_transfer",
        (o.order_id, rejected_leg.transfer_id),
    );
    assert_eq!(superseded, Err(Error::VersionConflict));
    let replacement_paid: Result<TransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "process_transfer",
        (o.order_id, replacement.transfer_id),
    );
    assert_eq!(
        replacement_paid.unwrap().status,
        MerchantTransferStatus::Succeeded
    );
    let view: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (beneficiary(f.user, &id),),
    );
    assert_eq!(view.unwrap().plan_snapshot.plan_id, PlanId::Free);
    let cert: Result<CertifiedBatch> = query(
        &f.ic,
        f.commerce,
        person(1),
        "get_order_certified",
        (o.order_id,),
    );
    assert!(!cert.unwrap().certificate.is_empty());
}

#[derive(CandidType, Serialize, Deserialize, Clone)]
struct TestNeuronId {
    id: Vec<u8>,
}
#[derive(CandidType, Serialize, Deserialize, Clone)]
struct TestPermission {
    principal: Option<Principal>,
    permission_type: Vec<i32>,
}
#[derive(CandidType, Serialize, Deserialize, Clone)]
enum TestDissolve {
    DissolveDelaySeconds(u64),
    WhenDissolvedTimestampSeconds(u64),
}
#[derive(CandidType, Serialize, Deserialize, Clone)]
struct TestNeuron {
    id: Option<TestNeuronId>,
    permissions: Vec<TestPermission>,
    cached_neuron_stake_e8s: u64,
    neuron_fees_e8s: u64,
    dissolve_state: Option<TestDissolve>,
}
fn neuron(f: &Fixture, stake: u64, other: bool) {
    let mut permissions = vec![TestPermission {
        principal: Some(person(1)),
        permission_type: vec![1, 2, 4, 5, 6],
    }];
    if other {
        permissions.push(TestPermission {
            principal: Some(person(2)),
            permission_type: vec![2],
        });
    }
    void(
        &f.ic,
        f.sns,
        person(1),
        "set_neuron",
        (Some(TestNeuron {
            id: Some(TestNeuronId { id: vec![44; 32] }),
            permissions,
            cached_neuron_stake_e8s: stake,
            neuron_fees_e8s: 100,
            dissolve_state: Some(TestDissolve::DissolveDelaySeconds(400 * 24 * 3600)),
        }),),
    );
}
fn claim_request(f: &Fixture, id: &AccountId, op: u8) -> ClaimRequest {
    let mut plans = default_plans(1);
    plans[1].membership_policy_version = Some(1);
    let mut r = ClaimRequest {
        authorization: intent(f, id, 1, op, f.membership, Hash::new([0; 32])),
        neuron_id: Hash::new([44; 32]),
        policy_version: 1,
        benefit_id: plan_digest(&plans[1]),
        expected_business_revision: 0,
        term: TermRule::CalendarYear,
        change: ClaimChange::Start,
    };
    r.authorization.action_digest = claim_action_digest(&r);
    r
}
#[test]
fn membership_cooling_renewed_device_approval_exclusivity_and_close() {
    let f = Fixture::new();
    let id = f.create(1);
    let other = f.create(2);
    neuron(&f, 5_000_000_000_100, false);
    let r = claim_request(&f, &id, 93);
    approve(&f, &id, 1, &r.authorization);
    let first: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_claim",
        (r.clone(),),
    );
    let first = first.unwrap();
    assert_eq!(first.status, ClaimStatus::CoolingDown);
    let mut r2 = claim_request(&f, &other, 94);
    r2.authorization.actor = person(2);
    approve(&f, &other, 2, &r2.authorization);
    let collision: Result<ClaimView> =
        update(&f.ic, f.membership, person(2), "request_claim", (r2,));
    assert_eq!(collision, Err(Error::VersionConflict));
    f.ic.advance_time(Duration::from_millis(MIN_COOLING_MS + 1));
    let expired: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "advance_application",
        (first.claim_id,),
    );
    assert_eq!(expired, Err(Error::Expired));
    approve(&f, &id, 1, &r.authorization);
    let active: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "advance_application",
        (first.claim_id,),
    );
    let active = active.unwrap();
    assert_eq!(active.status, ClaimStatus::Active);
    assert!(active.lease_revision > first.lease_revision);
    assert_eq!(
        active.expires_at_ms,
        next_year(active.starts_at_ms).unwrap()
    );
    if let Some(dir) = std::env::var_os("DMSG_COMMERCE_FIXTURE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let proof: Result<CertifiedBatch> = query(
            &f.ic,
            f.membership,
            person(1),
            "get_claim_certified",
            (vec![active.claim_id],),
        );
        let policy: Result<CertifiedBatch> = query(
            &f.ic,
            f.membership,
            person(1),
            "get_policy_certified",
            (vec![1u64],),
        );
        std::fs::write(
            dir.join("sns-active.cbor"),
            canonical(&(
                1u16,
                "membership/1",
                time(&f.ic),
                ByteBuf::from(f.ic.root_key().unwrap()),
                f.membership,
                id.clone(),
                proof.unwrap(),
                policy.unwrap(),
            )),
        )
        .unwrap();
    }
    let v: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (beneficiary(f.user, &id),),
    );
    assert_eq!(v.unwrap().plan_snapshot.plan_id, PlanId::Plus);
    f.ic.upgrade_canister(
        f.membership,
        wasm("membership"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    f.ic.advance_time(Duration::from_millis(MAX_LEASE_MS + 1));
    neuron(&f, 5_000_000_000_100, true);
    let v: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (beneficiary(f.user, &id),),
    );
    let repair = v.unwrap();
    assert_eq!(repair.source_status, SourceStatus::RepairRequired);
    assert!(repair.repair_deadline_ms.is_some());
    // Rechecking known loss of control must not reset the repair clock.
    f.ic.advance_time(Duration::from_millis(MAX_LEASE_MS + 1));
    let v: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (beneficiary(f.user, &id),),
    );
    assert_eq!(v.unwrap().repair_deadline_ms, repair.repair_deadline_ms);
    let i = intent(
        &f,
        &id,
        1,
        95,
        f.membership,
        close_claim_digest(active.claim_id),
    );
    approve(&f, &id, 1, &i);
    let closing: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_change",
        (active.claim_id, i),
    );
    let closing = closing.unwrap();
    assert_eq!(closing.status, ClaimStatus::Closing);
    assert!(closing.lease_revision > active.lease_revision);
    assert!(closing.release_after_ms < active.expires_at_ms);
    f.ic.advance_time(Duration::from_millis(MAX_LEASE_MS + 1));
    let closed: Result<ClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "refresh_claim",
        (active.claim_id,),
    );
    assert_eq!(closed.unwrap().status, ClaimStatus::Released);
}

#[test]
fn commercial_authorization_does_not_add_auth_and_governance_is_fixed() {
    let f = Fixture::new();
    let id = f.create(1);
    let q: Result<OrderQuote> = query(
        &f.ic,
        f.commerce,
        person(1),
        "quote_order",
        (QuoteOrder {
            op_id: Hash::new([96; 32]),
            beneficiary: beneficiary(f.user, &id),
            action: OrderAction::Subscribe { plan: PlanId::Plus },
            expected_business_revision: 0,
            payer: account(person(2)),
        },),
    );
    let q = q.unwrap();
    let i = intent(&f, &id, 2, 96, f.commerce, order_digest(&q));
    let unapproved: Result<BillingOrder> = update(
        &f.ic,
        f.commerce,
        person(2),
        "open_order",
        (dmsg_types::billing::OpenOrder {
            quote: q.clone(),
            authorization: i.clone(),
        },),
    );
    assert_eq!(unapproved, Err(Error::NotFound));
    approve(&f, &id, 1, &i);
    assert_eq!(f.account_id(1, &id).auth_bindings, vec![person(1)]);
    let approved: Result<BillingOrder> = update(
        &f.ic,
        f.commerce,
        person(2),
        "open_order",
        (dmsg_types::billing::OpenOrder {
            quote: q,
            authorization: i,
        },),
    );
    assert!(approved.is_ok());
    let denied: Result<()> = update(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "set_admission_pause",
        (true,),
    );
    assert_eq!(denied, Err(Error::Forbidden));
    let denied: Result<()> = update(
        &f.ic,
        f.membership,
        Principal::anonymous(),
        "set_admission_pause",
        (true,),
    );
    assert_eq!(denied, Err(Error::Forbidden));

    let catalogs: Vec<Catalog> = query(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "list_catalogs",
        (None::<u64>,),
    );
    let mut version_three = catalogs[0].clone();
    version_three.version = 3;
    version_three.effective_at_ms = time(&f.ic) + 31 * DAY;
    for plan in &mut version_three.plans {
        plan.catalog_version = 3;
    }
    let scheduled: Result<()> = update(
        &f.ic,
        f.commerce,
        f.sns,
        "schedule_policy",
        (version_three.clone(),),
    );
    scheduled.unwrap();
    let mut version_two = version_three;
    version_two.version = 2;
    version_two.effective_at_ms = time(&f.ic) + 61 * DAY;
    for plan in &mut version_two.plans {
        plan.catalog_version = 2;
    }
    let out_of_order: Result<()> =
        update(&f.ic, f.commerce, f.sns, "schedule_policy", (version_two,));
    assert!(matches!(out_of_order, Err(Error::InvalidInput(_))));

    let fee_governance = Principal::from_slice(&[90]);
    let current_fee: DeliveryFeePolicy = query(
        &f.ic,
        f.payment,
        Principal::anonymous(),
        "get_fee_policy",
        (),
    );
    let mut fee_three = current_fee.clone();
    fee_three.version = 3;
    fee_three.effective_at_ms = time(&f.ic) + 31 * DAY;
    let scheduled: Result<()> = update(
        &f.ic,
        f.payment,
        fee_governance,
        "schedule_fee_policy",
        (fee_three.clone(),),
    );
    scheduled.unwrap();
    let mut fee_two = fee_three;
    fee_two.version = 2;
    fee_two.effective_at_ms = time(&f.ic) + 61 * DAY;
    let out_of_order: Result<()> = update(
        &f.ic,
        f.payment,
        fee_governance,
        "schedule_fee_policy",
        (fee_two,),
    );
    assert_eq!(out_of_order, Err(Error::PolicyStale));
}

#[test]
fn transfer_fee_cannot_consume_principal_beyond_the_frozen_reserve() {
    let f = Fixture::new();
    let id = f.create(1);
    let o = open(&f, &id, 107, OrderAction::Subscribe { plan: PlanId::Plus });
    let refundable = deposit(&f, &o, 2, 200);
    check(&f, &o, refundable);

    let catalogs: Vec<Catalog> = query(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "list_catalogs",
        (None::<u64>,),
    );
    let mut next = catalogs[0].clone();
    next.version = 2;
    next.effective_at_ms = time(&f.ic) + 31 * DAY;
    next.ledger_fee = o.input.quote.fee_reserve + 1;
    for plan in &mut next.plans {
        plan.catalog_version = 2;
    }
    let scheduled: Result<()> = update(&f.ic, f.commerce, f.sns, "schedule_policy", (next,));
    scheduled.unwrap();
    f.ic.advance_time(Duration::from_millis(31 * DAY));
    void(
        &f.ic,
        f.ledger,
        person(1),
        "set_fee",
        (o.input.quote.fee_reserve + 1,),
    );
    let verified: Result<()> = update(&f.ic, f.commerce, f.sns, "verify_ledger_configuration", ());
    verified.unwrap();

    let blocked: Result<TransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "claim_deposit_refund",
        (o.order_id, refundable),
    );
    assert_eq!(blocked, Err(Error::FeeBlocked));
}

#[test]
fn recorded_cose_failure_releases_the_commercial_reservation() {
    let f = Fixture::commercial();
    let id = f.create(1);
    f.recoverable(1, &id);
    let initial: Result<ExecutionUsage> = update(
        &f.ic,
        f.user,
        person(1),
        "refresh_execution_entitlement",
        (&id,),
    );
    let initial = initial.unwrap();

    let initialized: Result<KeyState> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    initialized.unwrap();

    let public = key(7).verifying_key().to_bytes();
    let fingerprint =
        key_thumbprint(&public_cose_key(&Algorithm::Ed25519, &[], &public).unwrap()).unwrap();
    let state = f.account_id(1, &id);
    let device = &state.devices[&Hash::new([1; 32])];
    let mut request = SignRequest {
        account_id: id.clone(),
        key: SigningKeyRef {
            algorithm: SigningAlgorithm::Ed25519,
            kid: fingerprint.to_vec().into(),
            public_key_fingerprint: fingerprint,
        },
        statement: Statement {
            issuer: account_issuer(NAMESPACE, &id).unwrap(),
            subject: None,
            issued_at: None,
            content: StatementContent::Text("known rejection".into()),
        },
        origin: "https://example.com".into(),
        max_cycles: 100_000_000_000,
        approval: Approval {
            device_id: Hash::new([1; 32]),
            security_epoch: state.security_epoch,
            sequence: device.next_sequence,
            request_id: execution_request_id(
                &id,
                state.security_epoch,
                Hash::new([1; 32]),
                device.next_sequence,
            ),
            expires_at: time(&f.ic) + MINUTE,
            signature: ByteBuf::new(),
        },
    };
    request.approval.signature = key(1)
        .sign(
            request
                .clone()
                .into_execution()
                .unwrap()
                .approval_message(f.user)
                .as_slice(),
        )
        .to_bytes()
        .to_vec()
        .into();
    let rejected: Result<ExecutionResult> = update(&f.ic, f.user, person(1), "sign", (request,));
    assert!(matches!(
        rejected.unwrap().outcome,
        ExecutionOutcome::Failed(Error::IntegrityFailed)
    ));
    let usage: Result<ExecutionUsage> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_usage",
        (&id, initial.month_utc),
    );
    let usage = usage.unwrap();
    assert_eq!(usage.held_units, 0);
    assert_eq!(usage.charged_units, 0);
}

#[test]
fn execution_monthly_units_cannot_be_reset_by_refund_or_upgrade() {
    let f = Fixture::commercial();
    let id = f.create(1);
    f.recoverable(1, &id);
    let keys: Result<dmsg_types::cose::KeyState> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    keys.unwrap();
    let initial: Result<ExecutionUsage> = update(
        &f.ic,
        f.user,
        person(1),
        "refresh_execution_entitlement",
        (&id,),
    );
    let initial = initial.unwrap();
    assert!(initial.allowed_units <= 3);
    let signing_key = f.key_ref(&id, SigningPurpose::Statement, SigningAlgorithm::Ed25519);
    for _ in 0..initial.allowed_units {
        let request = user_tests::statement_request(&f, &id, signing_key.clone(), 100_000_000_000);
        let result: Result<ExecutionResult> = update(&f.ic, f.user, person(1), "sign", (request,));
        assert!(matches!(
            result.unwrap().outcome,
            ExecutionOutcome::Completed(_)
        ));
    }
    let exhausted = user_tests::statement_request(&f, &id, signing_key.clone(), 100_000_000_000);
    let result: Result<ExecutionResult> = update(&f.ic, f.user, person(1), "sign", (exhausted,));
    assert_eq!(result, Err(Error::QuotaExceeded));
    let o = open(&f, &id, 97, OrderAction::Subscribe { plan: PlanId::Plus });
    let block = deposit(
        &f,
        &o,
        1,
        o.input.quote.amount_atomic + o.input.quote.fee_reserve,
    );
    check(&f, &o, block);
    // Explicit refresh observes the purchase immediately, despite the cached Free lease.
    let u: Result<ExecutionUsage> = update(
        &f.ic,
        f.user,
        person(1),
        "refresh_execution_entitlement",
        (&id,),
    );
    let u = u.unwrap();
    assert!(u.allowed_units > initial.allowed_units && u.allowed_units <= 10);
    let signing_key = f.key_ref(&id, SigningPurpose::Statement, SigningAlgorithm::Ed25519);
    assert_eq!(u.charged_units, initial.allowed_units);
    let mut last = None;
    for index in u.charged_units..=u.allowed_units {
        if index == 8 {
            f.ic.advance_time(Duration::from_millis(DAY));
        }
        let s = f.account_id(1, &id);
        let device = &s.devices[&Hash::new([1; 32])];
        let mut request = SignRequest {
            account_id: id.clone(),
            key: signing_key.clone(),
            statement: Statement {
                issuer: account_issuer(NAMESPACE, &id).unwrap(),
                subject: None,
                issued_at: None,
                content: StatementContent::Text(format!("commercial signature {index}")),
            },
            origin: "https://example.com".into(),
            max_cycles: 100_000_000_000,
            approval: Approval {
                device_id: Hash::new([1; 32]),
                security_epoch: s.security_epoch,
                sequence: device.next_sequence,
                request_id: execution_request_id(
                    &id,
                    s.security_epoch,
                    Hash::new([1; 32]),
                    device.next_sequence,
                ),
                expires_at: time(&f.ic) + MINUTE,
                signature: ByteBuf::new(),
            },
        };
        request.approval.signature = key(1)
            .sign(
                request
                    .clone()
                    .into_execution()
                    .unwrap()
                    .approval_message(f.user)
                    .as_slice(),
            )
            .to_bytes()
            .to_vec()
            .into();
        let result: Result<ExecutionResult> =
            update(&f.ic, f.user, person(1), "sign", (request.clone(),));
        if index == u.allowed_units {
            assert_eq!(result, Err(Error::QuotaExceeded));
        } else {
            let result = result.unwrap();
            assert!(
                matches!(result.outcome, ExecutionOutcome::Completed(_)),
                "{result:?}"
            );
            last = Some(request);
        }
    }
    let repeated: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "sign", (last.unwrap(),));
    assert!(repeated.is_ok());
    let used: Result<ExecutionUsage> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_usage",
        (&id, u.month_utc),
    );
    let used = used.unwrap();
    assert_eq!(used.charged_units, u.allowed_units);
    assert_eq!(used.held_units, 0);
    let refund = intent(&f, &id, 1, 98, f.commerce, refund_digest(o.order_id));
    approve(&f, &id, 1, &refund);
    let r: Result<BillingOrder> = update(
        &f.ic,
        f.commerce,
        person(1),
        "request_refund",
        (o.order_id, refund),
    );
    r.unwrap();
    f.ic.advance_time(Duration::from_millis(MAX_LEASE_MS + 1));
    let r: Result<OrderProgress> = update(
        &f.ic,
        f.commerce,
        person(1),
        "reconcile_order",
        (o.order_id,),
    );
    r.unwrap();
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let after: Result<ExecutionUsage> = update(
        &f.ic,
        f.user,
        person(1),
        "refresh_execution_entitlement",
        (&id,),
    );
    let after = after.unwrap();
    assert_eq!(after.charged_units, used.charged_units);
    assert!(after.allowed_units <= used.allowed_units);
    // Root administration remains available when paid signing is exhausted.
    f.mutate(
        1,
        &id,
        AccountCommand::ReserveRoot {
            expected_generation: 0,
            op_id: Hash::new([100; 32]),
        },
    )
    .unwrap();
    let cert: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_usage_certified",
        (&id, u.month_utc),
    );
    certified_value(&f, cert.unwrap(), usage_key(&id, u.month_utc).as_slice());
}

#[test]
fn upgrades_refund_the_same_period_and_independent_storage_survives() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let base = open(&f, &id, 101, OrderAction::Subscribe { plan: PlanId::Plus });
    let block = deposit(
        &f,
        &base,
        1,
        base.input.quote.amount_atomic + base.input.quote.fee_reserve,
    );
    check(&f, &base, block);
    let extra = open(
        &f,
        &id,
        102,
        OrderAction::Storage {
            product_id: Hash::new([120; 32]),
        },
    );
    let block = deposit(
        &f,
        &extra,
        1,
        extra.input.quote.amount_atomic + extra.input.quote.fee_reserve,
    );
    check(&f, &extra, block);
    f.ic.advance_time(Duration::from_millis(DAY));
    let upgraded = open(&f, &id, 103, OrderAction::Upgrade { plan: PlanId::Pro });
    assert!(upgraded.input.quote.amount_atomic < 40_000_000);
    let block = deposit(
        &f,
        &upgraded,
        1,
        upgraded.input.quote.amount_atomic + upgraded.input.quote.fee_reserve,
    );
    check(&f, &upgraded, block);
    let v: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (beneficiary(f.user, &id),),
    );
    assert_eq!(v.unwrap().plan_snapshot.plan_id, PlanId::Pro);
    let i = intent(&f, &id, 1, 104, f.commerce, refund_digest(base.order_id));
    approve(&f, &id, 1, &i);
    let closing: Result<BillingOrder> = update(
        &f.ic,
        f.commerce,
        person(1),
        "request_refund",
        (base.order_id, i),
    );
    closing.unwrap();
    f.ic.advance_time(Duration::from_millis(MAX_LEASE_MS + 1));
    for id in [base.order_id, upgraded.order_id] {
        let r: Result<OrderProgress> =
            update(&f.ic, f.commerce, person(1), "reconcile_order", (id,));
        r.unwrap();
        let r = order_details(&f, id);
        assert_eq!(r.status, OrderStatus::RefundCommitted);
        assert_money(&r);
    }
    let v: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (beneficiary(f.user, &id),),
    );
    let v = v.unwrap();
    assert_eq!(v.plan_snapshot.plan_id, PlanId::Free);
    assert_eq!(v.addons.len(), 1);
    assert_eq!(
        v.effective_limits.storage_bytes,
        104_857_600 + 1_073_741_824
    );
}

#[test]
fn cancellation_of_future_renewal_keeps_current_contract_and_refunds_at_most_principal() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let base = open(&f, &id, 105, OrderAction::Subscribe { plan: PlanId::Plus });
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
    let expiry = v.unwrap().next_limit_change_at_ms.unwrap();
    f.ic.advance_time(Duration::from_millis(expiry - 29 * DAY - time(&f.ic)));
    let future = open(&f, &id, 106, OrderAction::Renew { plan: PlanId::Pro });
    let block = deposit(
        &f,
        &future,
        1,
        future.input.quote.amount_atomic + future.input.quote.fee_reserve,
    );
    check(&f, &future, block);
    let i = intent(&f, &id, 1, 107, f.commerce, refund_digest(future.order_id));
    approve(&f, &id, 1, &i);
    let r: Result<BillingOrder> = update(
        &f.ic,
        f.commerce,
        person(1),
        "request_refund",
        (future.order_id, i),
    );
    r.unwrap();
    let r: Result<OrderProgress> = update(
        &f.ic,
        f.commerce,
        person(1),
        "reconcile_order",
        (future.order_id,),
    );
    r.unwrap();
    let r = order_details(&f, future.order_id);
    assert_eq!(r.refunded_principal, future.input.quote.amount_atomic);
    assert_money(&r);
    let v: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (beneficiary(f.user, &id),),
    );
    assert_eq!(v.unwrap().plan_snapshot.plan_id, PlanId::Plus);
}

#[path = "external_integration.rs"]
mod external_integration;
