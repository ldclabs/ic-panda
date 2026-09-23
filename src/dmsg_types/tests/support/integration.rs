#![allow(dead_code)] // Shared fixtures are intentionally used in different subsets.
use candid::Principal;
use dmsg_types::{integration::*, membership::Beneficiary, *};

pub const NOW: u64 = 1_800_000_000_000;

pub fn principal(id: u8) -> Principal {
    Principal::from_slice(&[id, 1])
}

pub fn app() -> AppRegistration {
    AppRegistration {
        version: INTEGRATION_VERSION,
        environment: Environment::Local,
        app_id: "tokenlist".into(),
        config_version: 1,
        origins: vec!["http://localhost:5188".into()],
        user_homes: vec![principal(1)],
        cose_homes: vec![principal(2)],
        product_ids: vec!["tokenlist".into(), "sample".into()],
        capabilities: vec![
            AppCapability::Authenticate,
            AppCapability::Checkout,
            AppCapability::SignAction,
        ],
        profiles: vec![SigningProfile::AppActionV1],
        authentication_receiver: principal(3),
        action_authority: principal(3),
        paused: false,
    }
}

pub fn product() -> ProductRegistration {
    ProductRegistration {
        version: COMMERCE_VERSION,
        environment: Environment::Local,
        product_id: "tokenlist".into(),
        config_version: 1,
        quote_authority: principal(4),
        beneficiary_authority: principal(4),
        adapter: principal(4),
        subject_schema: "tokenlist-project-v1".into(),
        subject_size: 8,
        merchant: principal(5).into(),
        ledgers: vec![principal(6), principal(7)],
        terms_hash: Hash::new([11; 32]),
        subsidy_budget_id: Hash::new([12; 32]),
        paused: false,
    }
}

pub fn offer() -> BillingOffer {
    let p = product();
    BillingOffer {
        version: COMMERCE_VERSION,
        environment: Environment::Local,
        app_id: "tokenlist".into(),
        product_id: p.product_id.clone(),
        offer_id: Hash::new([13; 32]),
        beneficiary: Beneficiary {
            product_id: p.product_id,
            authority_canister: p.beneficiary_authority,
            subject_schema: p.subject_schema,
            subject_bytes: 42u64.to_be_bytes().to_vec().into(),
        },
        quote_authority: p.quote_authority,
        adapter: p.adapter,
        sku: "annual-standard".into(),
        product_terms_hash: p.terms_hash,
        expected_business_revision: 9,
        amount_usd_micros: 1_000_001,
        starts_at_ms: NOW + PANDA_COOLING_MS,
        expires_at_ms: NOW + 365 * DAY,
        issued_at_ms: NOW,
        accept_by_ms: NOW + OFFER_TTL_MS,
        operation_id: Hash::new([14; 32]),
        allowed_settlement_methods: vec![SettlementMethod::Cash, SettlementMethod::Panda],
    }
}

pub fn account_offer() -> BillingOffer {
    let mut offer = offer();
    offer.product_id = "sample".into();
    offer.beneficiary.product_id = offer.product_id.clone();
    offer.beneficiary.subject_schema = "sample-account-v1".into();
    offer.beneficiary.subject_bytes = vec![15; 12].into();
    offer
}

pub fn rate() -> PandaRatePolicy {
    PandaRatePolicy {
        version: COMMERCE_VERSION,
        policy_version: 1,
        environment: Environment::Local,
        product_ids: vec!["tokenlist".into(), "sample".into()],
        r_num: 7,
        r_den: 3,
        published_at_ms: NOW - POLICY_NOTICE_MS,
        effective_at_ms: NOW,
        subsidy_budget_id: product().subsidy_budget_id,
    }
}

pub fn authentication() -> AuthenticationRequest {
    AuthenticationRequest {
        version: INTEGRATION_VERSION,
        environment: Environment::Local,
        app_id: "tokenlist".into(),
        app_config_version: 1,
        origin: app().origins[0].clone(),
        receiver: principal(3),
        challenge_hash: Hash::new([16; 32]),
        session_key_hash: Hash::new([17; 32]),
        purpose: AuthenticationPurpose::Link,
        nonce: Hash::new([18; 32]),
        operation_id: Hash::new([19; 32]),
        issued_at_ms: NOW,
        expires_at_ms: NOW + AUTH_TTL_MS,
    }
}

pub fn approval() -> ApplicationApproval {
    ApplicationApproval {
        version: INTEGRATION_VERSION,
        environment: Environment::Local,
        app_id: "tokenlist".into(),
        app_config_version: 1,
        origin: app().origins[0].clone(),
        approving_account: AccountId([1; 12]),
        service: principal(8),
        beneficiary: offer().beneficiary,
        actor: principal(9),
        purpose: ApprovalPurpose::PandaSubscription,
        action_digest: Hash::new([20; 32]),
        operation_id: offer().operation_id,
        nonce: Hash::new([21; 32]),
        expires_at_ms: NOW + AUTH_TTL_MS,
    }
}

pub fn cash(ledger: Principal) -> CashQuote {
    CashQuote {
        version: COMMERCE_VERSION,
        offer_hash: dmsg_protocol::integration::billing_offer_hash(&offer()),
        ledger,
        amount_atomic: 1_000_001,
        conversion_hash: Hash::new([22; 32]),
        payer: principal(9).into(),
        deposit: icrc_ledger_types::icrc1::account::Account {
            owner: principal(8),
            subaccount: Some([23; 32]),
        },
        max_network_fee_atomic: 10_000,
        fee_reserve_atomic: 20_000,
        funding_deadline_ms: NOW + CASH_FUNDING_MS,
        activation_deadline_ms: NOW + CASH_ACTIVATION_MS,
    }
}

pub fn decision() -> ProductDecision {
    ProductDecision {
        version: COMMERCE_VERSION,
        offer: offer(),
        decision_id: Hash::new([24; 32]),
        source: SettlementSource::Panda {
            claim_id: Hash::new([25; 32]),
            quote_hash: Hash::new([26; 32]),
            committed_until_ms: offer().expires_at_ms,
            lease_until_ms: NOW + PANDA_LEASE_MS,
        },
        decided_at_ms: NOW,
        apply_by_ms: NOW + APPLICATION_TTL_MS,
    }
}
