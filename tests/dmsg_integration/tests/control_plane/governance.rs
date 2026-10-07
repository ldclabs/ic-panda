use super::*;
use dmsg_types::{
    billing::Catalog, integration::*, integration_billing::*, integration_membership::*,
};

type Rendered = std::result::Result<String, String>;

fn validate<A: ArgumentEncoder>(
    f: &Fixture,
    canister: Principal,
    method: &str,
    args: A,
) -> Rendered {
    query(
        &f.ic,
        canister,
        person(9),
        &format!("validate_{method}"),
        args,
    )
}

/// Run an administrative method: others are refused, governance succeeds.
fn govern<A: ArgumentEncoder + Clone>(
    f: &Fixture,
    canister: Principal,
    governance: Principal,
    method: &str,
    args: A,
) {
    assert_denied(&f.ic, canister, person(9), method, args.clone());
    let done: Result<candid::Reserved> = update(&f.ic, canister, governance, method, args);
    assert!(done.is_ok(), "{method}: {done:?}");
}

fn unchanged(rendered: Rendered) -> bool {
    rendered.unwrap().ends_with("; no change.")
}

#[test]
fn cose_and_payment_governance_run_validated_admin_operations() {
    let f = Fixture::new();
    let rendered = validate(&f, f.cose, "initialize_keys", ()).unwrap();
    assert!(rendered.starts_with("Initialize Local content-root vetKD key key_1 pinned to 0000"));
    govern(&f, f.cose, f.sns, "initialize_keys", ());
    assert!(unchanged(validate(&f, f.cose, "initialize_keys", ())));

    let governance = Principal::from_slice(&[90]);
    assert_eq!(
        validate(&f, f.payment, "set_orders_enabled", (false,)),
        Ok("Disable new payment escrows.".into())
    );
    govern(&f, f.payment, governance, "set_orders_enabled", (false,));
    assert!(unchanged(validate(
        &f,
        f.payment,
        "set_orders_enabled",
        (false,)
    )));

    assert_eq!(
        validate(&f, f.payment, "set_ledger_fee", (21u128,)),
        Err("FeeBlocked".into())
    );
    assert!(validate(&f, f.payment, "set_ledger_fee", (15u128,)).is_ok());
    govern(&f, f.payment, governance, "set_ledger_fee", (15u128,));

    let soon = DeliveryFeePolicy {
        version: 2,
        effective_at_ms: time(&f.ic) + DAY,
        rate_bps: 400,
        minimum_atomic: 100,
    };
    assert_eq!(
        validate(&f, f.payment, "schedule_fee_policy", (&soon,)),
        Err("PolicyStale".into())
    );
    let policy = DeliveryFeePolicy {
        effective_at_ms: time(&f.ic) + 31 * DAY,
        ..soon
    };
    assert!(validate(&f, f.payment, "schedule_fee_policy", (&policy,))
        .unwrap()
        .contains("version 2"));
    govern(&f, f.payment, governance, "schedule_fee_policy", (&policy,));

    let signer = ReceiptSigner {
        epoch: 2,
        public_key: key(51).verifying_key().to_bytes().into(),
        valid_from: time(&f.ic),
        valid_until: time(&f.ic) + DAY,
        revoked: false,
    };
    let stale = ReceiptSigner {
        epoch: 1,
        ..signer.clone()
    };
    assert!(validate(&f, f.payment, "rotate_receipt_signer", (&stale,)).is_err());
    assert!(validate(&f, f.payment, "rotate_receipt_signer", (&signer,))
        .unwrap()
        .contains("epoch 2"));
    govern(
        &f,
        f.payment,
        governance,
        "rotate_receipt_signer",
        (&signer,),
    );
    assert_eq!(
        validate(&f, f.payment, "revoke_receipt_signer", (9u64,)),
        Err("NotFound".into())
    );
    assert!(!unchanged(validate(
        &f,
        f.payment,
        "revoke_receipt_signer",
        (1u64,)
    )));
    govern(&f, f.payment, governance, "revoke_receipt_signer", (1u64,));
    assert!(unchanged(validate(
        &f,
        f.payment,
        "revoke_receipt_signer",
        (1u64,)
    )));
    // Revoking again would disable escrows that were re-enabled since.
    govern(&f, f.payment, governance, "set_orders_enabled", (true,));
    assert!(!unchanged(validate(
        &f,
        f.payment,
        "revoke_receipt_signer",
        (1u64,)
    )));
}

#[test]
fn commerce_and_membership_governance_run_validated_admin_operations() {
    let f = Fixture::new();
    let (app, product): (AppRegistration, Option<ProductRegistration>) = {
        let r: Result<(AppRegistration, Option<ProductRegistration>)> = update(
            &f.ic,
            f.commerce,
            person(9),
            "read_integration_configuration",
            ("dmsg", Some("dmsg")),
        );
        r.unwrap()
    };
    let product = product.unwrap();
    assert!(unchanged(validate(
        &f,
        f.commerce,
        "register_integration_app",
        (&app,)
    )));
    assert!(unchanged(validate(
        &f,
        f.commerce,
        "register_integration_product",
        (&product,)
    )));
    let paused = ProductRegistration {
        config_version: 2,
        paused: true,
        ..product.clone()
    };
    let rendered = validate(&f, f.commerce, "register_integration_product", (&paused,)).unwrap();
    assert!(rendered.contains("config version 2") && rendered.contains("paused true"));
    let moved = ProductRegistration {
        adapter: person(9),
        ..paused.clone()
    };
    assert_eq!(
        validate(&f, f.commerce, "register_integration_product", (&moved,)),
        Err("IntegrityFailed".into())
    );
    govern(
        &f,
        f.commerce,
        f.sns,
        "register_integration_product",
        (&paused,),
    );
    let app = AppRegistration {
        config_version: 2,
        paused: true,
        ..app
    };
    assert!(!unchanged(validate(
        &f,
        f.commerce,
        "register_integration_app",
        (&app,)
    )));
    govern(&f, f.commerce, f.sns, "register_integration_app", (&app,));

    let assets: Vec<SettlementAssetView> =
        query(&f.ic, f.commerce, person(9), "settlement_assets", ());
    let asset = assets[0].policy.clone();
    assert!(unchanged(validate(
        &f,
        f.commerce,
        "register_settlement_asset",
        (&asset,)
    )));
    let disabled = SettlementAsset {
        policy_version: asset.policy_version + 1,
        enabled: false,
        ..asset.clone()
    };
    assert!(
        validate(&f, f.commerce, "register_settlement_asset", (&disabled,))
            .unwrap()
            .contains("enabled false")
    );
    govern(
        &f,
        f.commerce,
        f.sns,
        "register_settlement_asset",
        (&disabled,),
    );
    assert_eq!(
        validate(
            &f,
            f.commerce,
            "verify_settlement_asset",
            (person(9), None::<u128>)
        ),
        Err("NotFound".into())
    );
    assert!(validate(
        &f,
        f.commerce,
        "verify_settlement_asset",
        (asset.ledger, None::<u128>)
    )
    .is_ok());
    govern(
        &f,
        f.commerce,
        f.sns,
        "verify_settlement_asset",
        (asset.ledger, None::<u128>),
    );
    assert!(validate(
        &f,
        f.commerce,
        "publish_settlement_price",
        (asset.ledger, 0u128, MINUTE)
    )
    .is_err());
    assert!(validate(
        &f,
        f.commerce,
        "publish_settlement_price",
        (asset.ledger, 999_000u128, MINUTE)
    )
    .unwrap()
    .contains("999000 USD micros"));
    govern(
        &f,
        f.commerce,
        f.sns,
        "publish_settlement_price",
        (asset.ledger, 999_000u128, MINUTE),
    );
    assert_eq!(
        validate(
            &f,
            f.commerce,
            "set_settlement_price_authority",
            (Principal::anonymous(),)
        ),
        Err("AuthRequired".into())
    );
    govern(
        &f,
        f.commerce,
        f.sns,
        "set_settlement_price_authority",
        (person(8),),
    );
    assert!(unchanged(validate(
        &f,
        f.commerce,
        "set_settlement_price_authority",
        (person(8),)
    )));
    // The price authority publishes without governance.
    let published: Result<SettlementAsset> = update(
        &f.ic,
        f.commerce,
        person(8),
        "publish_settlement_price",
        (asset.ledger, 1_000_000u128, MINUTE),
    );
    let published = published.unwrap();
    // A proposal written before those publications still applies its terms: it
    // keeps the latest price and takes the next version.
    let enabled = SettlementAsset {
        enabled: true,
        ..disabled.clone()
    };
    assert!(
        validate(&f, f.commerce, "register_settlement_asset", (&enabled,))
            .unwrap()
            .contains("next policy version")
    );
    govern(
        &f,
        f.commerce,
        f.sns,
        "register_settlement_asset",
        (&enabled,),
    );
    let assets: Vec<SettlementAssetView> =
        query(&f.ic, f.commerce, person(9), "settlement_assets", ());
    let current = &assets[0].policy;
    assert!(current.enabled);
    assert_eq!(current.policy_version, published.policy_version + 1);
    assert_eq!(
        (current.price_usd_micros, current.price_observed_at_ms),
        (published.price_usd_micros, published.price_observed_at_ms)
    );

    let limits: dmsg_types::billing::CommerceLimits =
        query(&f.ic, f.commerce, person(9), "get_commerce_limits", ());
    assert!(unchanged(validate(
        &f,
        f.commerce,
        "admin_set_limits",
        (&limits,)
    )));
    let raised = dmsg_types::billing::CommerceLimits {
        max_subjects: 2_000_000,
        max_hot_orders: 1_000_000,
        max_orders: 5_000_000,
        ..limits.clone()
    };
    assert!(validate(&f, f.commerce, "admin_set_limits", (&raised,))
        .unwrap()
        .contains("2000000 subjects"));
    for invalid in [
        dmsg_types::billing::CommerceLimits {
            max_hot_orders: 6_000_000,
            ..raised.clone()
        },
        dmsg_types::billing::CommerceLimits {
            calls_per_caller: raised.calls_per_minute + 1,
            ..raised.clone()
        },
    ] {
        assert_eq!(
            validate(&f, f.commerce, "admin_set_limits", (&invalid,)),
            Err("InvalidInput(\"commerce limits\")".into())
        );
    }
    govern(&f, f.commerce, f.sns, "admin_set_limits", (&raised,));
    let current: dmsg_types::billing::CommerceLimits =
        query(&f.ic, f.commerce, person(9), "get_commerce_limits", ());
    assert_eq!(current, raised);

    let catalogs: Vec<Catalog> = query(
        &f.ic,
        f.commerce,
        person(9),
        "list_catalogs",
        (None::<u64>,),
    );
    let mut catalog = catalogs[0].clone();
    catalog.version += 1;
    for plan in &mut catalog.plans {
        plan.catalog_version = catalog.version;
    }
    assert!(validate(&f, f.commerce, "schedule_policy", (&catalog,)).is_err());
    catalog.effective_at_ms = time(&f.ic) + 31 * DAY;
    assert!(validate(&f, f.commerce, "schedule_policy", (&catalog,))
        .unwrap()
        .contains("catalog version 2"));
    govern(&f, f.commerce, f.sns, "schedule_policy", (&catalog,));
    assert_eq!(
        validate(&f, f.commerce, "set_admission_pause", (true,)),
        Ok("Pause new cash checkouts.".into())
    );
    govern(&f, f.commerce, f.sns, "set_admission_pause", (true,));
    assert!(unchanged(validate(
        &f,
        f.commerce,
        "set_admission_pause",
        (true,)
    )));

    let m = f.membership;
    assert_eq!(
        validate(&f, m, "set_admission_pause", (true,)),
        Ok("Pause new PANDA applications.".into())
    );
    govern(&f, m, f.sns, "set_admission_pause", (true,));
    assert!(unchanged(validate(&f, m, "set_admission_pause", (true,))));
    let home = CommerceHome {
        user_home: f.user,
        commerce_canister: f.commerce,
    };
    let service = PandaServiceConfig {
        commerce_homes: vec![home.clone()],
        max_claims: 1000,
        hourly_applications: 100,
        cooling_ms: PANDA_COOLING_MS,
        qualifications_per_minute: 200,
    };
    assert!(unchanged(validate(
        &f,
        m,
        "configure_panda_service",
        (&service,)
    )));
    // A home keeps its commerce; another home's commerce can only be appended.
    let other = PandaServiceConfig {
        commerce_homes: vec![CommerceHome {
            commerce_canister: person(9),
            ..home.clone()
        }],
        ..service.clone()
    };
    assert_eq!(
        validate(&f, m, "configure_panda_service", (&other,)),
        Err("IntegrityFailed".into())
    );
    let duplicate = PandaServiceConfig {
        commerce_homes: vec![home.clone(), home.clone()],
        ..service.clone()
    };
    assert_eq!(
        validate(&f, m, "configure_panda_service", (&duplicate,)),
        Err("InvalidInput(\"commerce homes\")".into())
    );
    let unbounded = PandaServiceConfig {
        qualifications_per_minute: 0,
        ..service.clone()
    };
    assert_eq!(
        validate(&f, m, "configure_panda_service", (&unbounded,)),
        Err("InvalidInput(\"PANDA limits\")".into())
    );
    let raised = PandaServiceConfig {
        commerce_homes: vec![
            home,
            CommerceHome {
                user_home: person(8),
                commerce_canister: person(9),
            },
        ],
        hourly_applications: 200,
        qualifications_per_minute: 400,
        ..service
    };
    let rendered = validate(&f, m, "configure_panda_service", (&raised,)).unwrap();
    assert!(rendered.contains(&format!("{} -> {}", person(8), person(9))));
    assert!(rendered.contains("400 SNS reads per minute"));
    assert!(!unchanged(Ok(rendered)));
    govern(&f, m, f.sns, "configure_panda_service", (&raised,));
    let rate = PandaRatePolicy {
        version: 2,
        policy_version: 1,
        environment: Environment::Local,
        product_ids: vec!["dmsg".into(), "sample".into()],
        r_num: 5000,
        r_den: 1,
        published_at_ms: 0,
        effective_at_ms: POLICY_NOTICE_MS,
    };
    assert!(unchanged(validate(&f, m, "schedule_panda_rate", (&rate,))));
    let next = PandaRatePolicy {
        policy_version: 2,
        r_num: 6000,
        effective_at_ms: time(&f.ic) + POLICY_NOTICE_MS,
        ..rate
    };
    assert!(validate(&f, m, "schedule_panda_rate", (&next,))
        .unwrap()
        .contains("6000/1"));
    govern(&f, m, f.sns, "schedule_panda_rate", (&next,));
    assert!(validate(
        &f,
        m,
        "set_sns_governance_module_hash",
        (Hash::new([0; 32]),)
    )
    .is_err());
    assert!(validate(
        &f,
        m,
        "set_sns_governance_module_hash",
        (Hash::new([7; 32]),)
    )
    .unwrap()
    .contains("0707"));
    govern(
        &f,
        m,
        f.sns,
        "set_sns_governance_module_hash",
        (Hash::new([7; 32]),),
    );
}

#[test]
fn user_governance_adjusts_account_limits() {
    let f = Fixture::new();
    f.create(1);
    let limits =
        |max: u64, daily: u32| validate(&f, f.user, "admin_set_account_limits", (max, daily));
    assert!(limits(0, 10).is_err());
    assert!(limits(1, 10_001).is_err());
    assert!(limits(1, 10).unwrap().contains("existing accounts: 1"));
    govern(&f, f.user, f.sns, "admin_set_account_limits", (1u64, 10u32));
    let full: Result<AccountId> = update(
        &f.ic,
        f.user,
        person(2),
        "create_account",
        (f.create_input(2),),
    );
    assert_eq!(full, Err(Error::QuotaExceeded));
    govern(&f, f.user, f.sns, "admin_set_account_limits", (2u64, 10u32));
    assert!(unchanged(limits(2, 10)));
    f.create(2);
}
