use super::*;
use dmsg_types::{app_store::*, promo::*};

fn config(f: &Fixture, enabled: bool) {
    let result: Result<()> = update(
        &f.ic,
        f.commerce,
        f.sns,
        "set_promo_config",
        (PromoConfig {
            enabled,
            verifiers: vec![person(91)],
        },),
    );
    result.unwrap();
}

fn grant(f: &Fixture, account: &AccountId, seq: u8, plan: PlanId, days: u64) -> PromoGrant {
    let at = time(&f.ic);
    PromoGrant {
        grant_id: Hash::new([seq; 32]),
        beneficiary: beneficiary(f.user, account),
        plan,
        compensation: false,
        starts_at_ms: at,
        expires_at_ms: at + days * DAY,
    }
}

fn apply(f: &Fixture, g: &PromoGrant) -> Result<PromoReceipt> {
    update(&f.ic, f.commerce, person(91), "apply_promo_grant", (g,))
}

fn terms(f: &Fixture, g: &PromoGrant) -> TermsDisplayV3 {
    let r: std::result::Result<TermsDisplayV3, TermsErrorV3> = query(
        &f.ic,
        f.commerce,
        person(1),
        "get_membership_terms_v3",
        (&g.beneficiary,),
    );
    r.unwrap()
}

#[test]
fn fixed_grants_replay_pause_revoke_six_products_and_legacy_readers() {
    let f = Fixture::commercial();
    config(&f, true);
    let invalid_account = f.create(20);
    for invalid in [
        grant(&f, &invalid_account, 180, PlanId::Free, 30),
        grant(&f, &invalid_account, 181, PlanId::Plus, 14),
        grant(&f, &invalid_account, 182, PlanId::Max, 366),
    ] {
        assert!(matches!(apply(&f, &invalid), Err(Error::InvalidInput(_))));
    }
    for (i, (plan, days)) in [PlanId::Plus, PlanId::Pro, PlanId::Max]
        .into_iter()
        .flat_map(|p| [(p.clone(), 30), (p, 365)])
        .enumerate()
    {
        let a = f.create(i as u8 + 1);
        let g = grant(&f, &a, i as u8 + 1, plan.clone(), days);
        let forbidden: Result<PromoReceipt> =
            update(&f.ic, f.commerce, person(1), "apply_promo_grant", (&g,));
        assert_eq!(forbidden, Err(Error::Forbidden));
        let r = apply(&f, &g).unwrap();
        assert_eq!(r.outcome, PromoOutcome::Applied);
        assert_eq!(apply(&f, &g).unwrap(), r);
        assert_eq!(terms(&f, &g).contracts[0].source_kind, "promo");
        for method in ["get_membership_terms", "get_membership_terms_v2"] {
            let legacy: Result<MembershipTerms> =
                query(&f.ic, f.commerce, person(1), method, (&g.beneficiary,));
            assert_eq!(legacy, Err(Error::UnsupportedProtocol));
        }
        let v: Result<EntitlementView> = update(
            &f.ic,
            f.commerce,
            person(1),
            "refresh_entitlement",
            (&g.beneficiary,),
        );
        let v = v.unwrap();
        assert_eq!(v.plan_snapshot.plan_id, plan);
        assert!(v.valid_until_ms <= time(&f.ic) + DAY);
        assert_eq!(
            apply(
                &f,
                &PromoGrant {
                    compensation: true,
                    ..g.clone()
                }
            ),
            Err(Error::IdempotencyConflict)
        );
        let gift: Result<u64> = update(
            &f.ic,
            f.commerce,
            person(91),
            "check_guest_pass_issuer",
            (&g.beneficiary,),
        );
        assert_eq!(gift, Err(Error::MembershipIneligible));
        config(&f, false);
        assert_eq!(apply(&f, &g).unwrap(), r);
        let bad = grant(&f, &a, 100 + i as u8, plan, 30);
        let denied = apply(&f, &bad).unwrap();
        assert!(matches!(denied.outcome, PromoOutcome::Rejected(_)));
        let end: Result<PromoReceipt> = update(
            &f.ic,
            f.commerce,
            f.sns,
            "revoke_promo_grant",
            (g.grant_id,),
        );
        let end = end.unwrap();
        assert!(matches!(end.outcome, PromoOutcome::Revoked(_)));
        config(&f, true);
        assert_eq!(apply(&f, &g).unwrap(), end);
        assert_eq!(apply(&f, &bad).unwrap(), denied);
    }
    let counts: Vec<PromoCount> = query(&f.ic, f.commerce, person(1), "promo_stats", ());
    assert_eq!(counts.len(), 6);
    assert!(counts.iter().all(|r| r.issued == 1 && r.revoked == 1));
}

fn activate_channel(f: &Fixture, account: &AccountId, channel: &str) {
    match channel {
        "cash" => {
            pay(f, account, "pro", 160);
        }
        "sns" => {
            activate_panda(f, account, 130);
        }
        "google_play" => {
            google_play::configure(f);
            let t = google_play::transaction(f, account);
            google_play::deliver(f, &t).unwrap();
        }
        "app_store" | "sandbox" => {
            let cfg = AppStoreConfig {
                verifiers: vec![person(90)],
                bundle_id: "net.dmsg.ios".into(),
                app_apple_id: 6827050680,
                environment: StoreEnvironment::Production,
                accept_sandbox: Some(true),
                products: vec![StoreProduct {
                    product_id: "annual".into(),
                    kind: StoreProductKind::Plan(PlanId::Plus),
                }],
            };
            let ok: Result<()> = update(&f.ic, f.commerce, f.sns, "set_app_store_config", (&cfg,));
            ok.unwrap();
            let at = time(&f.ic);
            let t = StoreTransaction {
                beneficiary: beneficiary(f.user, account),
                transaction_id: "1001".into(),
                original_transaction_id: "1000".into(),
                product_id: "annual".into(),
                app_account_token: dmsg_protocol::app_store::account_token(f.user, account),
                bundle_id: cfg.bundle_id,
                environment: if channel == "sandbox" {
                    StoreEnvironment::Sandbox
                } else {
                    StoreEnvironment::Production
                },
                purchased_at_ms: at,
                expires_at_ms: Some(at + 365 * DAY),
                revoked_at_ms: None,
                is_upgraded: None,
                signed_at_ms: at,
                checked_at_ms: at,
            };
            let view: Result<EntitlementView> = update(
                &f.ic,
                f.commerce,
                person(90),
                "apply_app_store_transaction",
                (t,),
            );
            view.unwrap();
        }
        _ => unreachable!(),
    }
}

#[test]
fn promotional_members_convert_through_each_real_payment_adapter() {
    for channel in ["cash", "sns", "app_store", "google_play"] {
        let f = Fixture::commercial();
        let account = f.create(1);
        config(&f, true);
        let g = grant(&f, &account, 70, PlanId::Plus, 30);
        assert_eq!(apply(&f, &g).unwrap().outcome, PromoOutcome::Applied);
        let before = terms(&f, &g);
        assert!(!before.allowed_actions.iter().any(|a| a == "storage"));
        let storage: Result<BillingOffer> = update(
            &f.ic,
            f.commerce,
            person(1),
            "prepare_account_subscription",
            ("dmsg", &g.beneficiary, "78".repeat(32), Hash::new([88; 32])),
        );
        assert_eq!(storage, Err(Error::MembershipIneligible));
        assert!(before.allowed_actions.iter().any(|a| a == channel));
        activate_channel(&f, &account, channel);
        let after = terms(&f, &g);
        let current: Result<EntitlementView> = update(
            &f.ic,
            f.commerce,
            person(1),
            "refresh_entitlement",
            (&g.beneficiary,),
        );
        let id = current.unwrap().active_contract_id.unwrap();
        assert_eq!(
            after
                .contracts
                .iter()
                .find(|c| c.contract_id == id)
                .unwrap()
                .source_kind,
            channel
        );
        assert_eq!(after.refund.is_some(), channel == "cash");
        let saved: Result<Option<PromoReceipt>> = update(
            &f.ic,
            f.commerce,
            person(91),
            "get_promo_receipt",
            (g.grant_id,),
        );
        assert_eq!(saved.unwrap().unwrap().conversion.unwrap().0, channel);
        let issuer: Result<u64> = update(
            &f.ic,
            f.commerce,
            person(91),
            "check_guest_pass_issuer",
            (&g.beneficiary,),
        );
        assert!(issuer.unwrap() > time(&f.ic));
    }
}

#[test]
fn granted_plus_supplies_device_month_and_wallet_limits() {
    use dmsg_types::wallet::{ActivationQuote, HubInit, PlanQuota};
    let f = Fixture::commercial();
    let account = f.create(1);
    let hub = super::super::wallet::setup(&f);
    let hub_config: HubInit = query(&f.ic, hub, person(1), "get_hub_config", ());
    let mut pricing = hub_config.pricing;
    pricing.quotas.push(PlanQuota {
        plan: PlanId::Plus,
        wallets: 1,
    });
    let updated: Result<()> = update(&f.ic, hub, f.sns, "admin_set_pricing", (pricing,));
    updated.unwrap();

    let enroll = |n| device_limits::enroll(&f, &device_limits::enrollment(&f, &account, n));
    enroll(2).unwrap();
    assert_eq!(enroll(3), Err(Error::QuotaExceeded));
    config(&f, true);
    let g = grant(&f, &account, 84, PlanId::Plus, 30);
    apply(&f, &g).unwrap();
    enroll(3).unwrap();
    assert_eq!(enroll(4), Err(Error::QuotaExceeded));

    let usage: Result<ExecutionUsage> = update(
        &f.ic,
        f.user,
        person(1),
        "refresh_execution_entitlement",
        (account,),
    );
    let usage = usage.unwrap();
    let created = f.account_id(1, &account).created_at_ms;
    let bundle: Result<ExecutionEntitlement> = update(
        &f.ic,
        f.commerce,
        f.user,
        "get_execution_entitlement",
        (&g.beneficiary, usage.month_utc, created),
    );
    let bundle = bundle.unwrap();
    let source = bundle.view.active_contract_id.unwrap();
    let segment = bundle
        .month
        .segments
        .iter()
        .find(|s| s.source_contract_id == Some(source))
        .unwrap();
    let (_, month_end) = month_bounds(usage.month_utc).unwrap();
    assert_eq!(
        (segment.start_ms, segment.end_ms),
        (g.starts_at_ms, month_end.min(g.expires_at_ms))
    );
    assert_eq!(
        segment.monthly_units,
        bundle.view.effective_limits.monthly_execution_units
    );
    assert_eq!(
        usage.allowed_units,
        monthly_allowance(usage.month_utc, created, &bundle.month.segments).unwrap()
    );
    assert!(usage.allowed_units < segment.monthly_units);
    let quote: Result<ActivationQuote> =
        update(&f.ic, hub, person(1), "quote_activation", (account,));
    let quote = quote.unwrap();
    assert_eq!((quote.plan, quote.remaining), (PlanId::Plus, 1));
}

#[test]
fn refunded_first_purchase_restores_remaining_max_experience() {
    let f = Fixture::commercial();
    let account = f.create(1);
    config(&f, true);
    let g = grant(&f, &account, 83, PlanId::Max, 30);
    apply(&f, &g).unwrap();
    let base = pay(&f, &account, "plus", 140).0;
    let refresh = || {
        let result: Result<EntitlementView> = update(
            &f.ic,
            f.commerce,
            person(1),
            "refresh_entitlement",
            (&g.beneficiary,),
        );
        result.unwrap()
    };
    let paid = refresh();
    assert_eq!(paid.plan_snapshot.plan_id, PlanId::Plus);
    let after = terms(&f, &g);
    assert!(after.allowed_actions.iter().any(|a| a == "storage"));
    assert_eq!(after.refund.unwrap().order_id, base.progress.order_id);
    let storage = offer_sku(&f, &account, &"78".repeat(32), 155);
    assert!(storage.expires_at_ms > g.expires_at_ms);
    let cancelled: Result<CheckoutProgress> = update(
        &f.ic,
        f.commerce,
        person(1),
        "cancel_checkout",
        (base.progress.order_id,),
    );
    assert_eq!(cancelled.unwrap().status, CheckoutStatus::Applied);
    let stop = status(&f, base.progress.order_id)
        .cancellation
        .unwrap()
        .cancelled_at_ms;
    f.ic.advance_time(Duration::from_millis(stop.saturating_sub(time(&f.ic))));
    assert_eq!(refresh().plan_snapshot.plan_id, PlanId::Max);
    let original = terms(&f, &g)
        .contracts
        .into_iter()
        .find(|c| c.source_kind == "promo")
        .unwrap();
    assert_eq!(original.expires_at_ms, g.expires_at_ms);
    f.ic.advance_time(Duration::from_millis(g.expires_at_ms - time(&f.ic)));
    assert_eq!(refresh().plan_snapshot.plan_id, PlanId::Free);
}

#[test]
#[ignore = "requires DMSG_PRE_PROMO_WASM containing a released commerce module"]
fn pre_promo_sources_survive_upgrade_and_new_receipts_persist() {
    let old = std::fs::read(std::env::var_os("DMSG_PRE_PROMO_WASM").unwrap()).unwrap();
    let current = wasm("dmsg_commerce");
    assert_ne!(dmsg_protocol::sha256(&old), dmsg_protocol::sha256(&current));
    for channel in ["cash", "sns", "app_store", "google_play", "sandbox"] {
        let f = Fixture::with_modules(
            false,
            100,
            MAX_PAYMENT_ESCROWS,
            |_| {},
            [
                wasm("dmsg_user"),
                wasm("dmsg_directory"),
                wasm("dmsg_handle"),
                old.clone(),
            ],
        );
        let existing = f.create(1);
        activate_channel(&f, &existing, channel);
        let b = beneficiary(f.user, &existing);
        let read = |method: &str| -> Result<MembershipTerms> {
            query(&f.ic, f.commerce, person(1), method, (&b,))
        };
        let v1 = read("get_membership_terms");
        let v2 = read("get_membership_terms_v2").unwrap();
        assert_eq!(v2.contracts.len(), 1);
        let upgrade = || {
            f.ic.upgrade_canister(
                f.commerce,
                current.clone(),
                candid::encode_args(()).unwrap(),
                None,
            )
            .unwrap()
        };
        upgrade();
        assert_eq!(read("get_membership_terms"), v1);
        assert_eq!(read("get_membership_terms_v2").unwrap(), v2);
        let display: std::result::Result<TermsDisplayV3, TermsErrorV3> = query(
            &f.ic,
            f.commerce,
            person(1),
            "get_membership_terms_v3",
            (&b,),
        );
        let contract = display.unwrap().contracts.pop().unwrap();
        assert_eq!(contract.contract_id, v2.contracts[0].contract_id);
        assert_eq!(contract.is_sandbox, channel == "sandbox");
        assert_eq!(
            contract.source_kind,
            if channel == "sandbox" {
                "app_store"
            } else {
                channel
            }
        );
        if channel == "cash" {
            config(&f, true);
            let account = f.create(2);
            let g = grant(&f, &account, 22, PlanId::Plus, 30);
            let r = apply(&f, &g).unwrap();
            let mut compensation = grant(&f, &f.create(3), 23, PlanId::Pro, 30);
            compensation.compensation = true;
            let credit = apply(&f, &compensation).unwrap();
            upgrade();
            assert_eq!(apply(&f, &g).unwrap(), r);
            assert_eq!(apply(&f, &compensation).unwrap(), credit);
            for method in ["get_membership_terms", "get_membership_terms_v2"] {
                let legacy: Result<MembershipTerms> = query(
                    &f.ic,
                    f.commerce,
                    person(1),
                    method,
                    (&compensation.beneficiary,),
                );
                assert_eq!(
                    legacy.unwrap().contracts[0].source,
                    ContractSource::Compensation
                );
            }
        }
    }
}
