use crate::commerce_v2::{
    asset_available, cash_amount, check_quoted_asset, contract, entitlement_until,
    observe_contract, validate_asset,
};
use crate::integration::{panda_quote_hash, product_decision_hash};
use crate::product_book::ProductBook;
use dmsg_types::integration::{
    ProductDecision, ProductRejection, SettlementSource, PANDA_COOLING_MS, PANDA_LEASE_MS,
};
use dmsg_types::integration_billing::{SubscriptionSource, SubscriptionStatus};
use dmsg_types::membership::Eligibility;
use dmsg_types::{Environment, Error, Hash, DAY, MINUTE};
#[path = "../../dmsg_types/tests/support/commerce.rs"]
mod f;
use f::base::*;
#[test]
fn cash_price_is_exact_and_admission_never_infers_a_dollar_peg() {
    let mut a = f::asset();
    assert_eq!(cash_amount(1_000_001, &a).unwrap(), 1_000_001);
    a.price_usd_micros = 999_999;
    assert_eq!(cash_amount(1_000_001, &a).unwrap(), 1_000_003);
    a.price_usd_micros = 989_999;
    assert!(asset_available(&a, NOW).is_err());
    a = f::asset();
    assert!(asset_available(&a, a.price_valid_until_ms).is_err());
    a.environment = Environment::Production;
    assert!(validate_asset(&a).is_err());
    a = f::asset();
    assert!(cash_amount(u128::MAX, &a).is_ok());
    a.price_usd_micros = 1;
    assert_eq!(cash_amount(u128::MAX, &a), Err(Error::QuotaExceeded));
}

#[test]
fn a_new_price_observation_keeps_live_quotes_but_other_term_changes_do_not() {
    let quoted = f::asset();
    let mut current = quoted.clone();
    current.policy_version += 1;
    current.price_usd_micros = 999_000;
    current.price_observed_at_ms = NOW + MINUTE;
    current.price_valid_until_ms = NOW + 31 * MINUTE;
    check_quoted_asset(&current, &quoted, NOW + MINUTE).unwrap();
    assert_eq!(
        check_quoted_asset(&current, &quoted, quoted.price_valid_until_ms),
        Err(Error::PolicyStale)
    );
    let mut fee = current.clone();
    fee.network_fee_atomic += 1;
    assert_eq!(
        check_quoted_asset(&fee, &quoted, NOW + MINUTE),
        Err(Error::PolicyStale)
    );
    current.enabled = false;
    assert_eq!(
        check_quoted_asset(&current, &quoted, NOW + MINUTE),
        Err(Error::PolicyStale)
    );
}

#[test]
fn one_product_interval_excludes_competing_cash_and_panda_and_retains_unknown_apply() {
    let mut book = ProductBook::new(offer().beneficiary, 9);
    let auth = f::authorization(true);
    book.reserve(auth.clone(), NOW + DAY, NOW).unwrap();
    let before = book.clone();
    let mut competing = f::authorization(false);
    competing.offer.operation_id = Hash::new([44; 32]);
    assert_eq!(book.reserve(competing, NOW + DAY, NOW), Err(Error::Pending));
    assert_eq!(book, before);
    let d = f::cash_decision();
    book.begin_apply(&d, NOW).unwrap();
    book.prune(NOW + 2 * DAY);
    assert!(book.reservation.is_some());
    assert_eq!(book.release(&auth.offer), Err(Error::ExecutionUnknown));
    // Adapter supplies a definite terminal receipt after it establishes non-delivery.
    book.reject(&d, ProductRejection::Expired, NOW + 2 * DAY);
    assert!(book.reservation.is_none());
}

#[test]
fn cash_delivery_and_unstarted_cancellation_advance_only_business_revision() {
    let mut b = ProductBook::new(offer().beneficiary, 9);
    b.reserve(f::authorization(true), NOW + DAY, NOW).unwrap();
    let d = f::cash_decision();
    b.begin_apply(&d, NOW).unwrap();
    let (c, r) = b.apply(&d, f::cash_source(&d), NOW).unwrap();
    assert_eq!(b.business_revision, 10);
    assert_eq!(r.decision_hash, product_decision_hash(&d));
    let order = if let SubscriptionSource::Cash { order_id, .. } = c.source {
        order_id
    } else {
        unreachable!()
    };
    assert!(
        b.cancel_cash(order, c.contract_id, r.decision_hash, NOW)
            .unwrap()
            .cancelled
    );
    assert_eq!(b.business_revision, 11);
    let mut b = ProductBook::new(offer().beneficiary, 9);
    b.reserve(f::authorization(true), NOW + DAY, NOW).unwrap();
    b.begin_apply(&d, NOW).unwrap();
    let (c, r) = b.apply(&d, f::cash_source(&d), NOW).unwrap();
    assert!(
        !b.cancel_cash(order, c.contract_id, r.decision_hash, c.offer.starts_at_ms)
            .unwrap()
            .cancelled
    );
}

#[test]
fn qualification_leases_do_not_bump_business_revision_or_revive_terminated_rights() {
    let terms = f::terms();
    let at = NOW + PANDA_COOLING_MS;
    let d = ProductDecision {
        version: 2,
        offer: terms.offer.clone(),
        decision_id: Hash::new([45; 32]),
        source: SettlementSource::Panda {
            claim_id: Hash::new([46; 32]),
            quote_hash: panda_quote_hash(&terms.quote),
            committed_until_ms: terms.offer.expires_at_ms,
            lease_until_ms: at + PANDA_LEASE_MS,
        },
        decided_at_ms: at,
        apply_by_ms: at + 5 * MINUTE,
    };
    let mut c = contract(
        &d,
        SubscriptionSource::PandaClaim {
            claim_id: Hash::new([46; 32]),
            quote: terms.quote,
        },
        10,
        at,
    )
    .unwrap();
    let end = c.offer.expires_at_ms;
    assert_eq!(entitlement_until(&mut c, at), Some(at + PANDA_LEASE_MS));
    assert_eq!(entitlement_until(&mut c, at + PANDA_LEASE_MS), None);
    observe_contract(&mut c, Eligibility::Ineligible, at + 1, at + 1, at + 1).unwrap();
    assert_eq!(entitlement_until(&mut c, at + 1), None);
    observe_contract(
        &mut c,
        Eligibility::Unverifiable,
        at + DAY,
        at + DAY,
        at + DAY,
    )
    .unwrap();
    let elapsed = c.repair_elapsed_ms;
    observe_contract(
        &mut c,
        Eligibility::Ineligible,
        at + 10 * DAY,
        at + 10 * DAY,
        at + 10 * DAY,
    )
    .unwrap();
    assert_eq!(c.repair_elapsed_ms, elapsed);
    observe_contract(
        &mut c,
        Eligibility::Ineligible,
        at + 18 * DAY,
        at + 18 * DAY,
        at + 18 * DAY,
    )
    .unwrap();
    assert_eq!(c.status, SubscriptionStatus::Terminated);
    observe_contract(
        &mut c,
        Eligibility::Eligible,
        at + 19 * DAY,
        at + 19 * DAY + PANDA_LEASE_MS,
        at + 19 * DAY,
    )
    .unwrap();
    assert_eq!(c.status, SubscriptionStatus::Terminated);
    assert_eq!(c.offer.expires_at_ms, end);
    assert_eq!(c.business_revision, 10);
}
