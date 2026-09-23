#![allow(dead_code)]
#[path = "integration.rs"]
pub mod base;
use base::*;
use dmsg_protocol::commerce_v2::{
    checkout_id, checkout_quote, checkout_quote_hash, panda_application_hash,
};
use dmsg_protocol::integration::quote_panda;
use dmsg_types::integration::{
    ApprovalPurpose, ProductDecision, SettlementSource, CASH_ACTIVATION_MS,
};
use dmsg_types::integration_billing::{
    CheckoutQuote, OpenCheckout, ProductAuthorizationRequest, SettlementAsset, SettlementAssetKind,
    SubscriptionSource,
};
use dmsg_types::integration_membership::{PandaApplicationTerms, PandaClaimRequest};
use dmsg_types::{AccountId, Environment, Hash, MINUTE};
pub fn asset() -> SettlementAsset {
    SettlementAsset {
        version: 2,
        policy_version: 1,
        environment: Environment::Local,
        ledger: principal(6),
        asset: SettlementAssetKind::CkUsdc,
        decimals: 6,
        price_usd_micros: 1_000_000,
        price_observed_at_ms: NOW,
        price_valid_until_ms: NOW + 30 * MINUTE,
        network_fee_atomic: 10,
        max_network_fee_atomic: 20,
        enabled: true,
    }
}
pub fn checkout() -> CheckoutQuote {
    checkout_quote(
        principal(8),
        offer(),
        &app(),
        &product(),
        asset(),
        principal(9).into(),
        NOW,
    )
    .unwrap()
}
pub fn terms() -> PandaApplicationTerms {
    PandaApplicationTerms {
        home_membership: principal(10),
        user_home: principal(1),
        approving_account: AccountId([1; 12]),
        actor: principal(9),
        sns_governance: principal(11),
        neuron_id: Hash::new([30; 32]),
        offer: offer(),
        quote: quote_panda(&offer(), &app(), &product(), &rate(), NOW).unwrap(),
    }
}
pub fn authorization(cash: bool) -> ProductAuthorizationRequest {
    let mut a = approval();
    a.purpose = if cash {
        ApprovalPurpose::CashCheckout
    } else {
        ApprovalPurpose::PandaSubscription
    };
    a.service = if cash { principal(8) } else { principal(10) };
    a.action_digest = if cash {
        checkout_quote_hash(&checkout())
    } else {
        panda_application_hash(&terms())
    };
    ProductAuthorizationRequest {
        offer: offer(),
        account_approval: a,
        user_home: principal(1),
        approval_id: Hash::new([31; 32]),
        product_approval: None,
    }
}
pub fn open() -> OpenCheckout {
    OpenCheckout {
        quote: checkout(),
        authorization: authorization(true),
    }
}
pub fn claim() -> PandaClaimRequest {
    PandaClaimRequest {
        terms: terms(),
        authorization: authorization(false),
    }
}
pub fn cash_decision() -> ProductDecision {
    let q = checkout();
    ProductDecision {
        version: 2,
        offer: q.offer,
        decision_id: Hash::new([32; 32]),
        source: SettlementSource::Cash {
            order_id: checkout_id(principal(8), &offer()),
            ledger: q.cash.ledger,
            block_index: 0,
            amount_atomic: q.cash.amount_atomic,
        },
        decided_at_ms: NOW,
        apply_by_ms: NOW + CASH_ACTIVATION_MS,
    }
}
pub fn cash_source(d: &ProductDecision) -> SubscriptionSource {
    if let SettlementSource::Cash {
        order_id,
        ledger,
        block_index,
        amount_atomic,
    } = d.source
    {
        SubscriptionSource::Cash {
            order_id,
            ledger,
            block_index,
            amount_atomic,
        }
    } else {
        panic!("cash fixture")
    }
}
