use super::*;

const DAY_MS: u64 = 86_400_000;
const NOW: u64 = 1_000 * DAY_MS;

fn contract(seed: u8, plan: PlanId, starts_at_ms: u64, expires_at_ms: u64) -> MembershipContract {
    MembershipContract {
        term_starts_at_ms: starts_at_ms,
        resource_pauses: vec![],
        contract_id: Hash::new([seed; 32]),
        plan: dmsg_protocol::billing::default_plans(1)
            .into_iter()
            .find(|p| p.plan_id == plan)
            .expect("catalog plan"),
        source: ContractSource::Cash {
            order_id: Hash::new([seed.wrapping_add(1); 32]),
        },
        starts_at_ms,
        expires_at_ms,
        terminated_at_ms: None,
        eligibility: dmsg_types::membership::Eligibility::Eligible,
        observed_at_ms: starts_at_ms,
        qualified_until_ms: expires_at_ms,
    }
}

/// A refunded first purchase keeps its contract until its lease runs out; a
/// purchase made after it is the one in force, as commerce projects it.
#[test]
fn the_newest_started_contract_is_in_force() {
    let mut refunded = contract(1, PlanId::Plus, NOW - 10 * DAY_MS, NOW + 355 * DAY_MS);
    refunded.terminated_at_ms = Some(NOW + 60_000);
    let bought = contract(3, PlanId::Max, NOW - 60_000, NOW + 365 * DAY_MS);
    let contracts = [&refunded, &bought];
    assert_eq!(
        in_force(&contracts, NOW).map(|c| c.contract_id),
        Some(bought.contract_id)
    );
    assert_eq!(queued(&contracts, NOW).map(|c| c.contract_id), None);
}

/// A renewal queued after the running term is next, and the running term
/// stays in force until it ends.
#[test]
fn a_queued_renewal_follows_the_term_in_force() {
    let running = contract(5, PlanId::Plus, NOW - 360 * DAY_MS, NOW + 5 * DAY_MS);
    let renewal = contract(7, PlanId::Plus, NOW + 5 * DAY_MS, NOW + 370 * DAY_MS);
    let contracts = [&running, &renewal];
    assert_eq!(
        in_force(&contracts, NOW).map(|c| c.contract_id),
        Some(running.contract_id)
    );
    assert_eq!(
        queued(&contracts, NOW).map(|c| c.contract_id),
        Some(renewal.contract_id)
    );
}

/// Nothing in force before the first term starts, and nothing queued after
/// the last one started.
#[test]
fn a_term_that_has_not_started_is_only_queued() {
    let future = contract(9, PlanId::Pro, NOW + DAY_MS, NOW + 366 * DAY_MS);
    let contracts = [&future];
    assert_eq!(in_force(&contracts, NOW), None);
    assert_eq!(
        queued(&contracts, NOW).map(|c| c.contract_id),
        Some(future.contract_id)
    );
}

/// A funding transfer sends the amount and its fee reserve, and the ledger
/// charges the network fee on top; an order needing more than a `u128`
/// cannot be funded.
#[test]
fn a_cash_checkout_needs_its_amount_reserve_and_network_fee() {
    assert_eq!(cash_total(1_000, 200, 3), Ok(1_203));
    assert_eq!(cash_total(u128::MAX, 1, 0), Err(Error::IntegrityFailed));
}

/// A saved approval covers the checkout up to, but not at, its deadline; both
/// the application and the device signature must still be live.
#[test]
fn a_saved_approval_covers_only_its_remaining_window() {
    assert!(approval_live(NOW + 1, NOW + 1, NOW));
    assert!(!approval_live(NOW, NOW + 1, NOW));
    assert!(!approval_live(NOW + 1, NOW, NOW));
}
