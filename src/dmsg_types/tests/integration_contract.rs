use dmsg_protocol::{canonical, decode_canonical, integration::*, validate_origin};
use dmsg_types::{integration::*, *};
#[path = "support/integration.rs"]
mod fixtures;
use fixtures::*;

#[test]
fn full_interval_price_is_rounded_once_with_wide_intermediates() {
    for (amount, num, den, expected) in [
        (1_000_000, 2, 1, 200_000_000),
        (1, 1, 3, 34),
        (1_000_001, 7, 3, 233_333_567),
    ] {
        assert_eq!(required_panda_stake(amount, num, den), Ok(expected));
    }
    assert_eq!(required_panda_stake(u128::MAX, 1, 100), Ok(u128::MAX));
    assert_eq!(
        required_panda_stake(u128::MAX, 1, 1),
        Err(Error::QuotaExceeded)
    );
    assert_eq!(
        required_panda_stake(u128::MAX, u128::MAX, u128::MAX),
        Err(Error::QuotaExceeded)
    );
    assert!(required_panda_stake(0, 1, 1).is_err());
    assert!(required_panda_stake(1, 0, 1).is_err());
    assert!(required_panda_stake(1, 1, 0).is_err());
    let year = quote_panda(&offer(), &app(), &product(), &rate(), NOW).unwrap();
    let mut quarter = offer();
    quarter.expires_at_ms = NOW + 90 * DAY;
    let quarter = quote_panda(&quarter, &app(), &product(), &rate(), NOW).unwrap();
    assert_eq!(year.required_stake_e8s, quarter.required_stake_e8s);
    assert_ne!(year.committed_until_ms, quarter.committed_until_ms);
    assert_ne!(year.offer_hash, quarter.offer_hash);
}

#[test]
fn product_schema_is_not_inferred_from_id_length() {
    validate_billing_offer(&offer(), &app(), &product(), NOW).unwrap();
    let account = account_offer();
    let mut p = product();
    p.product_id = "sample".into();
    p.subject_schema = "sample-account-v1".into();
    p.subject_size = 12;
    validate_billing_offer(&account, &app(), &p, NOW).unwrap();
    assert!(validate_billing_offer(&account, &app(), &product(), NOW).is_err());
    let mut wrong = offer();
    wrong.beneficiary.subject_schema = "dmsg-account-v1".into();
    assert!(validate_billing_offer(&wrong, &app(), &product(), NOW).is_err());
    wrong = offer();
    wrong.beneficiary.authority_canister = principal(99);
    assert!(validate_billing_offer(&wrong, &app(), &product(), NOW).is_err());
    wrong = offer();
    wrong.beneficiary.subject_bytes = vec![0; 12].into();
    assert!(validate_billing_offer(&wrong, &app(), &product(), NOW).is_err());
}

#[test]
fn approvals_bind_account_product_actor_service_and_purpose_separately() {
    let a = approval();
    validate_application_approval(&a, &app(), &product(), principal(8), NOW).unwrap();
    assert_ne!(
        a.approving_account.as_slice(),
        a.beneficiary.subject_bytes.as_ref()
    );
    assert!(validate_application_approval(&a, &app(), &product(), principal(9), NOW).is_err());
    assert!(
        validate_application_approval(&a, &app(), &product(), principal(8), a.expires_at_ms)
            .is_err()
    );
    let mut altered = a.clone();
    altered.purpose = ApprovalPurpose::CashCheckout;
    assert_ne!(
        application_approval_hash(&a),
        application_approval_hash(&altered)
    );
    altered = a.clone();
    altered.actor = principal(98);
    assert_ne!(
        application_approval_hash(&a),
        application_approval_hash(&altered)
    );
    altered = a.clone();
    altered.origin.push_str(".evil.test");
    assert!(
        validate_application_approval(&altered, &app(), &product(), principal(8), NOW).is_err()
    );
}

#[test]
fn unknown_versions_units_extra_fields_and_unbounded_lists_fail() {
    let mut o = offer();
    o.version = 1;
    assert_eq!(
        validate_billing_offer(&o, &app(), &product(), NOW),
        Err(Error::UnsupportedProtocol)
    );
    o = offer();
    o.accept_by_ms *= 1_000_000;
    assert!(validate_billing_offer(&o, &app(), &product(), NOW).is_err());
    o = offer();
    o.expires_at_ms /= 1000;
    assert!(validate_billing_offer(&o, &app(), &product(), NOW).is_err());
    o = offer();
    o.allowed_settlement_methods.push(SettlementMethod::Panda);
    assert!(validate_billing_offer(&o, &app(), &product(), NOW).is_err());
    let mut value: cbor2::Value = cbor2::from_slice(&canonical(&offer())).unwrap();
    let cbor2::Value::Map(ref mut entries) = value else {
        panic!("map")
    };
    entries.push((
        cbor2::Value::Text("discount_bps".into()),
        cbor2::Value::Integer(1.into()),
    ));
    assert!(decode_canonical::<BillingOffer>(&canonical(&value)).is_err());
    let mut a = app();
    a.origins.push(a.origins[0].clone());
    assert!(validate_app(&a).is_err());
}

#[test]
fn exact_origins_do_not_accept_url_prefixes_credentials_paths_or_normalization() {
    for origin in [
        "https://example.test",
        "http://localhost:5188",
        "http://127.0.0.1:5188",
        "http://[::1]:5188",
    ] {
        validate_origin(origin, &Environment::Local).unwrap();
    }
    for origin in [
        "https://example.test/",
        "https://user@example.test",
        "https://example.test/a",
        "https://EXAMPLE.test",
        "https://example.test:443",
        "https://example.test?x=1",
        "http://example.test",
        "null",
    ] {
        assert!(
            validate_origin(origin, &Environment::Local).is_err(),
            "{origin}"
        );
    }
    assert!(validate_origin("http://localhost:5188", &Environment::Production).is_err());
}

#[test]
fn policy_and_quote_expiry_are_checked_before_any_reservation() {
    let mut p = rate();
    p.effective_at_ms += 1;
    assert_eq!(
        quote_panda(&offer(), &app(), &product(), &p, NOW),
        Err(Error::PolicyStale)
    );
    p = rate();
    p.published_at_ms += 1;
    assert!(quote_panda(&offer(), &app(), &product(), &p, NOW).is_err());
    let mut a = app();
    a.paused = true;
    assert_eq!(
        quote_panda(&offer(), &a, &product(), &rate(), NOW),
        Err(Error::Locked)
    );
    assert!(quote_panda(&offer(), &app(), &product(), &rate(), offer().accept_by_ms).is_err());
    let mut o = offer();
    o.starts_at_ms = NOW;
    o.expires_at_ms = NOW + PANDA_COOLING_MS;
    assert_eq!(
        quote_panda(&o, &app(), &product(), &rate(), NOW),
        Err(Error::Expired)
    );
    o = offer();
    o.amount_usd_micros = 0;
    assert!(quote_panda(&o, &app(), &product(), &rate(), NOW).is_err());
}

#[test]
fn cash_asset_and_fee_terms_are_immutable_and_domain_separated() {
    let first = cash(principal(6));
    let second = cash(principal(7));
    validate_cash_quote(&first, &offer(), &product(), NOW).unwrap();
    validate_cash_quote(&second, &offer(), &product(), NOW).unwrap();
    assert_ne!(cash_quote_hash(&first), cash_quote_hash(&second));
    let mut wrong = first.clone();
    wrong.ledger = principal(99);
    assert!(validate_cash_quote(&wrong, &offer(), &product(), NOW).is_err());
    wrong = first.clone();
    wrong.fee_reserve_atomic = wrong.max_network_fee_atomic - 1;
    assert!(validate_cash_quote(&wrong, &offer(), &product(), NOW).is_err());
    wrong = first;
    wrong.amount_atomic = u128::MAX;
    assert!(validate_cash_quote(&wrong, &offer(), &product(), NOW).is_err());
    let mut p = product();
    p.environment = Environment::Production;
    assert!(validate_product(&p).is_err());
    p.ledgers = vec![
        candid::Principal::from_text(CKUSDT_LEDGER).unwrap(),
        candid::Principal::from_text(CKUSDC_LEDGER).unwrap(),
    ];
    validate_product(&p).unwrap();
}

#[test]
fn authentication_result_is_bound_to_the_entire_pending_request() {
    let request = authentication();
    validate_authentication(&request, &app(), NOW).unwrap();
    let result = AuthenticationResult {
        version: 1,
        request: request.clone(),
        account_id: AccountId([1; 12]),
        home_user: principal(1),
        security_epoch: 2,
        device_id: Hash::new([3; 32]),
        approved_at_ms: NOW,
        expires_at_ms: request.expires_at_ms,
    };
    match_authentication_result(&result, &request, principal(1), NOW, NOW).unwrap();
    let mut other = request.clone();
    other.session_key_hash = Hash::new([99; 32]);
    assert!(match_authentication_result(&result, &other, principal(1), NOW, NOW).is_err());
    other = request.clone();
    other.purpose = AuthenticationPurpose::Login;
    assert!(match_authentication_result(&result, &other, principal(1), NOW, NOW).is_err());
    assert!(match_authentication_result(&result, &request, principal(2), NOW, NOW).is_err());
    assert!(
        match_authentication_result(&result, &request, principal(1), NOW, NOW + 61_000).is_err()
    );
    assert!(match_authentication_result(
        &result,
        &request,
        principal(1),
        request.expires_at_ms,
        request.expires_at_ms
    )
    .is_err());
    assert!(
        authentication_key(&result.account_id, &request.operation_id)
            .starts_with(b"authentication/v1/")
    );
    assert_ne!(
        authentication_request_hash(&request),
        application_approval_hash(&approval())
    );
}

#[test]
fn receipt_cannot_change_unknown_operation_or_shorten_commitment() {
    let d = decision();
    let mut receipt = ProductReceipt {
        version: COMMERCE_VERSION,
        decision_id: d.decision_id,
        decision_hash: product_decision_hash(&d),
        adapter: d.offer.adapter,
        outcome: ProductOutcome::Applied {
            business_revision: 10,
            contract_id: Hash::new([27; 32]),
            committed_until_ms: d.offer.expires_at_ms,
        },
        applied_at_ms: NOW,
    };
    match_product_receipt(&receipt, &d).unwrap();
    let mut other = d.clone();
    other.offer.amount_usd_micros += 1;
    assert!(match_product_receipt(&receipt, &other).is_err());
    receipt.outcome = ProductOutcome::Applied {
        business_revision: 10,
        contract_id: Hash::new([27; 32]),
        committed_until_ms: NOW,
    };
    assert!(match_product_receipt(&receipt, &d).is_err());
    receipt.outcome = ProductOutcome::Rejected {
        reason: ProductRejection::RevisionConflict,
    };
    match_product_receipt(&receipt, &d).unwrap();
    receipt.decision_id = Hash::new([98; 32]);
    assert!(match_product_receipt(&receipt, &d).is_err());
}
