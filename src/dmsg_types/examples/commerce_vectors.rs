//! Deterministic public interoperability fixtures; every private key is a
//! fixed test seed. Run through scripts/verify-dmsg-vectors.mjs independently.
use candid::Principal;
use dmsg_protocol::*;
use dmsg_protocol::{billing::*, membership::*};
use dmsg_types::{billing::*, membership::*, payment::DeliveryFeePolicy, *};
use ed25519_dalek::SigningKey;

mod support;
use support::vector;

fn main() {
    let b = beneficiary(Principal::from_slice(&[1]), &AccountId([1; 12]));
    let intent = MembershipIntent {
        application_id: Hash::new([2; 32]),
        environment: Environment::Local,
        service_canister: Principal::from_slice(&[3]),
        beneficiary: b.clone(),
        actor: Principal::self_authenticating([4]),
        action_digest: Hash::new([5; 32]),
        nonce: Hash::new([6; 32]),
        valid_until_ms: 1_800_000_000_000,
    };
    let mut values = vec![
        vector("beneficiary", canonical(&b)),
        vector(
            "entitlement_key",
            canonical(&(1u8, "dmsg/commerce/entitlement-key/v1", &b)),
        ),
        vector(
            "catalog_key",
            canonical(&(1u8, "dmsg/commerce/catalog-key/v1", "dmsg")),
        ),
        vector(
            "membership_intent",
            canonical(&(1u8, "dmsg/commerce/intent/v1", &intent)),
        ),
        vector("plans", canonical(&default_plans(1))),
    ];
    assert_eq!(
        sha256(&canonical(&(1u8, "dmsg/commerce/entitlement-key/v1", &b))),
        entitlement_key(&b)
    );
    for (cents, num, den) in [
        (1000u64, 5000u128, 1u128),
        (1, 1, 3),
        (20000, u64::MAX as u128, u64::MAX as u128),
    ] {
        let threshold = Threshold::AnnualPrice {
            price_cents: cents,
            r_num: num,
            r_den: den,
        };
        values.push(vector(
            &format!("panda_{cents}_{num}_{den}"),
            canonical(&(cents, num, den, 8u8, required_panda(&threshold, 8).unwrap())),
        ));
    }
    for net in [0u128, 1, 399_999, 400_000, 400_001, 1_000_000, u128::MAX] {
        let fee = DeliveryFeePolicy {
            version: 1,
            effective_at_ms: 0,
            rate_bps: 500,
            minimum_atomic: 20_000,
        };
        values.push(vector(
            &format!("delivery_fee_{net}"),
            canonical(&(net, &fee, delivery_service_fee(net, &fee).unwrap())),
        ));
    }
    for month in [202402, 202609, 202612] {
        let (a, b) = month_bounds(month).unwrap();
        let segments = vec![
            MonthSegment {
                start_ms: a,
                end_ms: a + (b - a) / 2,
                monthly_units: 3,
                source_contract_id: None,
            },
            MonthSegment {
                start_ms: a + (b - a) / 2,
                end_ms: b,
                monthly_units: 50,
                source_contract_id: Some(Hash::new([9; 32])),
            },
        ];
        values.push(vector(
            &format!("month_{month}"),
            canonical(&(
                month,
                a,
                b,
                &segments,
                monthly_allowance(month, a, &segments).unwrap(),
            )),
        ));
    }
    for start in [1_709_208_000_123u64, 1_800_000_000_000] {
        values.push(vector(
            &format!("year_{start}"),
            canonical(&(start, next_year(start).unwrap())),
        ));
    }
    let q = OrderQuote {
        home_commerce: Principal::from_slice(&[3]),
        request: QuoteOrder {
            op_id: Hash::new([7; 32]),
            beneficiary: b,
            action: OrderAction::Subscribe { plan: PlanId::Plus },
            expected_business_revision: 0,
            payer: icrc_ledger_types::icrc1::account::Account {
                owner: Principal::from_slice(&[8]),
                subaccount: Some([9; 32]),
            },
        },
        catalog: Catalog {
            schema: 1,
            version: 1,
            effective_at_ms: 0,
            plans: default_plans(1),
            storage_products: vec![],
            ledger: Principal::from_slice(&[10]),
            decimals: 6,
            ledger_fee: 10,
            terms_digest: Hash::new([11; 32]),
        },
        amount_atomic: 10_000_000,
        fee_reserve: 30,
        created_at_ms: 1_800_000_000_000,
        fund_by_ms: 1_800_000_900_000,
        activate_by_ms: 1_800_002_700_000,
        term: TermRule::CalendarYear,
    };
    values.push(vector(
        "order_terms",
        canonical(&(1u8, "dmsg/commerce/order/v1", &q)),
    ));
    let quote = dmsg_types::profiles::delivery::Quote {
        quote_id: Hash::new([12; 32]),
        home_payment: Principal::from_slice(&[13]),
        payer: q.request.payer,
        offer_digest: Hash::new([14; 32]),
        quote_scope: Hash::new([15; 32]),
        ledger: q.catalog.ledger,
        recipient: icrc_ledger_types::icrc1::account::Account {
            owner: Principal::from_slice(&[16]),
            subaccount: None,
        },
        recipient_net: 1_000_000,
        platform: icrc_ledger_types::icrc1::account::Account {
            owner: Principal::from_slice(&[17]),
            subaccount: None,
        },
        service_fee: 50_000,
        fee_policy_version: 1,
        fee_reserve: 30,
        amount: 1_050_030,
        max_network_fee: 20,
        max_bytes: 8192,
        retain_ms: DAY,
        envelope_digest: Hash::new([18; 32]),
        signer_epoch: 1,
        created_at: q.created_at_ms,
        fund_by: q.fund_by_ms,
        accept_by: q.activate_by_ms,
    };
    let receipt = dmsg_types::profiles::delivery::AdmissionReceipt {
        protocol: 2,
        relay_id: Hash::new([19; 32]),
        signer_epoch: 1,
        home_payment: quote.home_payment,
        escrow_id: Hash::new([20; 32]),
        quote_digest: digest("dmsg/quote/v2", &quote),
        envelope_digest: quote.envelope_digest,
        size: 128,
        policy_version: 1,
        inbox_key_version: 1,
        admission_seq: 1,
        stored_at: quote.created_at + 1000,
        retain_until: quote.created_at + 1000 + DAY,
        fund_by: quote.fund_by,
        accept_by: quote.accept_by,
    };
    values.push(vector(
        "delivery_quote_v2",
        canonical(&(1u8, "dmsg/quote/v2", &quote)),
    ));
    values.push(vector(
        "delivery_receipt_v2",
        canonical(&(1u8, "dmsg/admission-receipt/v2", &receipt)),
    ));
    let signing = SigningKey::from_bytes(&[7; 32]);
    let public = public_cose_key(
        &dmsg_types::cose::Algorithm::Ed25519,
        &[],
        &signing.verifying_key().to_bytes(),
    )
    .unwrap();
    let fingerprint = key_thumbprint(&public).unwrap();
    let statement = Statement {
        issuer: "https://dmsg.test/u/040g2081040g2081040g".into(),
        subject: None,
        issued_at: None,
        content: StatementContent::Text("Commerce grant vector".into()),
    };
    let (_, tbs) = prepare_cose(
        &statement,
        &dmsg_types::cose::Algorithm::Ed25519,
        fingerprint.as_slice(),
    )
    .unwrap();
    let request_id = execution_request_id(&AccountId([1; 12]), 1, Hash::new([21; 32]), 0);
    let grant = dmsg_types::cose::ExecutionGrant {
        account_id: AccountId([1; 12]),
        home_user: Principal::from_slice(&[1]),
        home_cose: Principal::from_slice(&[22]),
        request_id,
        execution_sequence: 1,
        security_epoch: 1,
        device_id: Hash::new([21; 32]),
        device_sequence: 0,
        approved_at: 1_800_000_000_000,
        expires_at: 1_800_000_060_000,
        commerce: Some(CommercialReservation {
            reservation_id: request_id,
            month_utc: month_utc(1_800_000_000_000).unwrap(),
            units: 1,
            weight_policy_version: 1,
            business_revision: 1,
            lease_revision: 2,
            valid_until_ms: 1_800_000_060_000,
        }),
        kind: dmsg_types::cose::ExecutionKind::Sign {
            key: dmsg_types::cose::KeyRequest {
                purpose: dmsg_types::cose::KeyPurpose::Statement,
                algorithm: dmsg_types::cose::Algorithm::Ed25519,
                generation: 1,
            },
            to_be_signed: tbs.into(),
            public_key_fingerprint: fingerprint,
            origin: "https://example.com".into(),
        },
        max_cycles: 100_000_000_000,
    };
    values.push(vector(
        "execution_grant_v3",
        canonical(&(1u8, "dmsg/cose-execution/v3", &grant)),
    ));
    println!("{}", serde_json::to_string_pretty(&values).unwrap());
}
