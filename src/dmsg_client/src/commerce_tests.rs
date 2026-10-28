use super::*;

#[path = "../../dmsg_types/tests/support/commerce.rs"]
mod fixture;

const DAY_MS: u64 = 86_400_000;
const NOW: u64 = 1_000 * DAY_MS;

#[test]
fn benefit_list_uses_tier_devices_and_the_hubs_actual_wallet_quotas() {
    use dmsg_types::wallet::PlanQuota;
    let catalog = Catalog {
        schema: 1,
        version: 1,
        effective_at_ms: 0,
        plans: dmsg_protocol::billing::default_plans(1),
        storage_products: vec![],
        terms_digest: Hash::new([9; 32]),
    };
    // Governance may change quotas or enable Free; the display must not use
    // the launch proposal's 0/1/3/10 in place of the actual configuration.
    let quotas = vec![
        PlanQuota {
            plan: PlanId::Free,
            wallets: 1,
        },
        PlanQuota {
            plan: PlanId::Pro,
            wallets: 7,
        },
        PlanQuota {
            plan: PlanId::Max,
            wallets: 12,
        },
    ];
    let view = catalog_benefits(&catalog, Some(&quotas));
    for (index, (devices, wallets)) in [(2, 1), (3, 0), (5, 7), (10, 12)].into_iter().enumerate() {
        assert_eq!(view["plans"][index]["limits"]["active_devices"], devices);
        assert_eq!(view["plans"][index]["limits"]["team_wallets"], wallets);
        assert_eq!(
            view["plans"][index]["limits"]["storage_bytes"],
            catalog.plans[index].limits.storage_bytes
        );
    }
    let unavailable = catalog_view(&catalog);
    assert!(
        unavailable["plans"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["limits"]["team_wallets"].is_null())
    );
    let empty = catalog_benefits(&catalog, Some(&[]));
    assert!(
        empty["plans"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["limits"]["team_wallets"] == 0)
    );
}

#[test]
fn cash_views_display_symbols_without_renaming_the_wire_asset_variants() {
    use dmsg_types::integration_billing::SettlementAssetKind;
    for (asset, symbol, wire) in [
        (SettlementAssetKind::CkUsdc, "ckUSDC", "CkUsdc"),
        (SettlementAssetKind::CkUsdt, "ckUSDT", "CkUsdt"),
    ] {
        let mut quote = fixture::checkout();
        quote.asset.asset = asset;
        assert_eq!(serde_json::to_value(&quote.asset.asset).unwrap(), wire);
        let checkout = Checkout {
            id: "sample".into(),
            actor: quote.cash.payer.owner,
            origin: DMSG_ORIGIN.into(),
            request: CheckoutRequest {
                offer: quote.offer.clone(),
                approving_account: dmsg_types::AccountId([1; 12]),
                product_approval: None,
                method: SettlementMethod::Cash,
            },
            terms: Terms::Cash(Box::new(quote.clone())),
            approval: None,
            stage: Stage::Review,
            created_at_ms: None,
        };
        assert_eq!(checkout.view()["terms"]["asset"], symbol);
        let order = Order::Cash(Box::new(CheckoutView {
            progress: CheckoutProgress {
                order_id: Hash::new([1; 32]),
                status: CheckoutStatus::Applied,
                decision_id: None,
            },
            quote,
            receipt: None,
            cancellation: None,
            outgoing_atomic: 0,
            service_reserve_atomic: 0,
            fee_reserve_atomic: 0,
        }));
        assert_eq!(order_view(&order)["asset"], symbol);
    }
}

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

/// Apple renews an App Store term itself: inside the renewal window a cash
/// term is offered a renewal, an App Store term no renewal, switch or upgrade.
#[test]
fn an_app_store_term_offers_no_other_renewal_or_switch() {
    let catalog = Catalog {
        schema: 1,
        version: 1,
        effective_at_ms: 0,
        plans: dmsg_protocol::billing::default_plans(1),
        storage_products: vec![],
        terms_digest: Hash::new([9; 32]),
    };
    let cash = contract(11, PlanId::Plus, NOW - 360 * DAY_MS, NOW + 5 * DAY_MS);
    let renew = offers(&catalog, Some(&cash), None, NOW).unwrap();
    assert!(renew.iter().any(|o| o["action"] == "renew"));
    let mut apple = cash;
    apple.source = ContractSource::AppStore {
        transaction_id: Hash::new([12; 32]),
    };
    let managed = offers(&catalog, Some(&apple), None, NOW).unwrap();
    assert_eq!(managed.len(), 3);
    assert!(managed.iter().all(|o| o["action"] == "unavailable"
        && o["reason"] == "APP_STORE_MANAGED"
        && o["available_at_ms"].is_null()));
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

#[test]
fn an_early_refund_reports_when_the_term_closes() {
    use dmsg_types::integration_billing::CashCancellationReceipt;
    let stop = now_ms() + 60_000;
    let view = CheckoutView {
        progress: CheckoutProgress {
            order_id: Hash::new([1; 32]),
            status: CheckoutStatus::Applied,
            decision_id: None,
        },
        quote: fixture::checkout(),
        receipt: None,
        cancellation: Some(CashCancellationReceipt {
            order_id: Hash::new([1; 32]),
            contract_id: Hash::new([2; 32]),
            decision_hash: Hash::new([3; 32]),
            cancelled: true,
            cancelled_at_ms: stop,
            business_revision: 2,
        }),
        outgoing_atomic: 100,
        service_reserve_atomic: 10,
        fee_reserve_atomic: 0,
    };
    assert_eq!(status_name(&view, stop - 1), "Closing");
    assert_eq!(status_name(&view, stop), "Cancelled");
    let output = order_view(&Order::Cash(Box::new(view.clone())));
    assert_eq!(output["expires_at_ms"], stop);
    let mut refused = view;
    refused.cancellation.as_mut().unwrap().cancelled = false;
    assert_eq!(status_name(&refused, stop), "Applied");
}
