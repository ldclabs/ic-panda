//! Apple payment adapter. Only governed verifier principals submit facts after
//! validating Apple JWS/certificates. Rights remain certified by this canister.
use crate::{api, model, store};
use dmsg_protocol::{app_store::account_token, billing::beneficiary_account, digest};
use dmsg_runtime::{
    admin::{self, Validation, hex, validation},
    storage::{MapExt, Stored},
};
use dmsg_types::{
    app_store::*,
    billing::*,
    membership::{Beneficiary, Eligibility},
    *,
};
use ic_stable_structures::{
    DefaultMemoryImpl, StableBTreeMap, StableCell, memory_manager::VirtualMemory,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

type Memory = VirtualMemory<DefaultMemoryImpl>;
#[derive(Clone, Serialize, Deserialize)]
struct Observation {
    transaction: StoreTransaction,
    fingerprint: Hash,
}
thread_local! {
    static CONFIG: RefCell<StableCell<Stored<Option<AppStoreConfig>>, Memory>> = RefCell::new(StableCell::init(store::memory(35), Stored(None)));
    static TRANSACTIONS: RefCell<StableBTreeMap<Vec<u8>, Stored<Observation>, Memory>> = RefCell::new(StableBTreeMap::init(store::memory(36)));
    static OWNERS: RefCell<StableBTreeMap<Vec<u8>, Stored<Beneficiary>, Memory>> = RefCell::new(StableBTreeMap::init(store::memory(37)));
}

fn validate_config(config: &AppStoreConfig, environment: &Environment) -> Result<()> {
    ensure_valid(
        !config.verifiers.is_empty() && config.verifiers.len() <= 8,
        "verifiers",
    )?;
    for verifier in &config.verifiers {
        dmsg_protocol::authenticated(*verifier)?;
    }
    ensure_valid(
        !config.bundle_id.is_empty() && config.bundle_id.len() <= 200 && config.app_apple_id > 0,
        "application",
    )?;
    ensure(
        *environment != Environment::Production
            || config.environment == StoreEnvironment::Production,
        Error::Forbidden,
    )?;
    ensure_valid(config.products.len() <= 128, "products")?;
    let mut seen = std::collections::BTreeSet::new();
    for product in &config.products {
        ensure_valid(
            !product.product_id.is_empty()
                && product.product_id.len() <= 200
                && seen.insert(&product.product_id),
            "product ID",
        )?;
        ensure_valid(
            !matches!(product.kind, StoreProductKind::Plan(PlanId::Free)),
            "paid plan",
        )?;
    }
    Ok(())
}

#[ic_cdk::update]
fn set_app_store_config(config: AppStoreConfig) -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    store::check_admin(caller)?;
    validate_config(&config, &store::config(|c| c.environment.clone()))?;
    CONFIG.with_borrow_mut(|c| c.set(Stored(Some(config))));
    Ok(())
}

#[ic_cdk::query]
fn validate_set_app_store_config(config: AppStoreConfig) -> Validation {
    validation(
        validate_config(&config, &store::config(|c| c.environment.clone())).map(|()| {
            let verifiers: Vec<String> = config.verifiers.iter().map(|v| v.to_text()).collect();
            let products: Vec<String> = config
                .products
                .iter()
                .map(|p| match &p.kind {
                    StoreProductKind::Plan(plan) => format!("{} => {plan:?} plan", p.product_id),
                    StoreProductKind::Storage(id) => {
                        format!("{} => storage {}", p.product_id, hex(id.as_slice()))
                    }
                })
                .collect();
            format!(
                "Set App Store config for {} (Apple ID {}, {:?}, accept sandbox: {}): verifiers [{}], products [{}].{}",
                config.bundle_id,
                config.app_apple_id,
                config.environment,
                config.accept_sandbox.unwrap_or(false),
                verifiers.join(", "),
                products.join(", "),
                admin::unchanged(
                    CONFIG.with_borrow(|c| c.get().0.as_ref() != Some(&config)),
                    "Already set"
                ),
            )
        }),
    )
}

#[ic_cdk::query]
fn get_app_store_config() -> Option<AppStoreConfig> {
    CONFIG.with_borrow(|c| c.get().0.clone())
}

pub(crate) fn transaction_environment(id: &Hash) -> Option<StoreEnvironment> {
    TRANSACTIONS.with_borrow(|m| m.load(id.as_slice()).map(|o| o.transaction.environment))
}

fn validate_transaction(
    t: &StoreTransaction,
    config: &AppStoreConfig,
    at: u64,
) -> Result<StoreProductKind> {
    ensure(
        t.bundle_id == config.bundle_id
            && (config.accepts(&t.environment)
                || (config.environment == StoreEnvironment::Production
                    && t.environment == StoreEnvironment::Sandbox
                    && OWNERS
                        .with_borrow(|m| {
                            m.load(
                                digest(
                                    "dmsg/app-store/original/v1",
                                    &(&t.environment, &t.original_transaction_id),
                                )
                                .as_slice(),
                            )
                        })
                        .is_some_and(|owner| owner == t.beneficiary))),
        Error::Forbidden,
    )?;
    for id in [&t.transaction_id, &t.original_transaction_id] {
        ensure_valid(
            !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_digit()),
            "transaction ID",
        )?;
    }
    ensure_valid(
        t.checked_at_ms <= at.saturating_add(60_000)
            && at.saturating_sub(t.checked_at_ms) <= 60_000,
        "status observation",
    )?;
    let account = beneficiary_account(&t.beneficiary)?;
    ensure(
        t.app_account_token.to_ascii_lowercase()
            == account_token(t.beneficiary.authority_canister, &account),
        Error::Forbidden,
    )?;
    ensure_valid(
        t.purchased_at_ms <= t.signed_at_ms && t.signed_at_ms <= at.saturating_add(60_000),
        "transaction time",
    )?;
    ensure_valid(
        t.expires_at_ms.is_none_or(|e| e > t.purchased_at_ms)
            && t.revoked_at_ms
                .is_none_or(|r| r >= t.purchased_at_ms && r <= t.signed_at_ms),
        "transaction interval",
    )?;
    let kind = config
        .products
        .iter()
        .find(|p| p.product_id == t.product_id)
        .map(|p| p.kind.clone())
        .ok_or(Error::UnsupportedProtocol)?;
    // This production opt-in is for annual memberships, not consumable credit.
    ensure(
        !(config.environment == StoreEnvironment::Production
            && t.environment == StoreEnvironment::Sandbox
            && matches!(kind, StoreProductKind::Storage(_))),
        Error::UnsupportedProtocol,
    )?;
    Ok(kind)
}

fn apply(
    s: &mut model::Subject,
    t: &StoreTransaction,
    kind: &StoreProductKind,
    catalog: &Catalog,
    at: u64,
) -> Result<()> {
    let id = digest(
        "dmsg/app-store/transaction/v1",
        &(&t.environment, &t.transaction_id),
    );
    if let Some(existing) = s.contracts.iter_mut().find(|c| c.contract_id == id) {
        ensure(
            existing.starts_at_ms == t.purchased_at_ms,
            Error::IdempotencyConflict,
        )?;
        // A refund never reopens a term an upgrade already ended.
        if let Some(revoked) = t.revoked_at_ms {
            existing.terminated_at_ms = Some(
                existing
                    .terminated_at_ms
                    .map_or(revoked, |end| end.min(revoked)),
            );
        }
        existing.expires_at_ms = t
            .expires_at_ms
            .ok_or_else(|| dmsg_types::invalid("subscription expiry"))?;
        existing.observed_at_ms = t.checked_at_ms;
        existing.qualified_until_ms = existing
            .expires_at_ms
            .min(t.checked_at_ms.saturating_add(60 * MINUTE));
        return Ok(());
    }
    if let Some(existing) = s.addons.iter_mut().find(|a| a.contract_id == id) {
        if let Some(revoked) = t.revoked_at_ms {
            existing.expires_at_ms = existing.expires_at_ms.min(revoked);
        }
        return Ok(());
    }
    if t.revoked_at_ms.is_some() || t.expires_at_ms.is_some_and(|e| e <= at) {
        return Ok(());
    }
    match kind {
        StoreProductKind::Plan(plan) => {
            let expiry = t
                .expires_at_ms
                .ok_or_else(|| dmsg_types::invalid("subscription expiry"))?;
            ensure_valid(
                expiry - t.purchased_at_ms <= 400 * DAY,
                "subscription interval",
            )?;
            ensure(
                t.environment == StoreEnvironment::Sandbox
                    || s.active(t.purchased_at_ms)
                        .is_none_or(|c| matches!(c.source, ContractSource::AppStore { .. })),
                Error::Forbidden,
            )?;
            // StoreKit upgrades replace only overlapping Apple subscriptions.
            for old in &mut s.contracts {
                if matches!(old.source, ContractSource::AppStore { ref environment, .. } if environment.as_ref() == Some(&t.environment))
                    && old.starts_at_ms < t.purchased_at_ms
                    && model::end(old) > t.purchased_at_ms
                {
                    old.terminated_at_ms = Some(t.purchased_at_ms);
                }
            }
            let replaced_at = s
                .contracts
                .iter()
                .filter(|c| {
                    matches!(c.source, ContractSource::AppStore { ref environment, .. } if environment.as_ref() == Some(&t.environment))
                        && c.starts_at_ms > t.purchased_at_ms
                })
                .map(|c| c.starts_at_ms)
                .min();
            s.contracts.push(MembershipContract {
                term_starts_at_ms: t.purchased_at_ms,
                resource_pauses: vec![],
                contract_id: id,
                plan: model::plan(catalog, plan)?,
                source: ContractSource::AppStore {
                    transaction_id: id,
                    environment: Some(t.environment.clone()),
                },
                starts_at_ms: t.purchased_at_ms,
                expires_at_ms: expiry,
                terminated_at_ms: replaced_at,
                eligibility: Eligibility::Eligible,
                observed_at_ms: t.checked_at_ms,
                qualified_until_ms: expiry.min(t.checked_at_ms.saturating_add(60 * MINUTE)),
            });
            s.contracts.sort_by_key(|c| c.starts_at_ms);
        }
        StoreProductKind::Storage(product) => {
            let current = s.active(at).ok_or(Error::Forbidden)?;
            ensure(
                matches!(current.source, ContractSource::AppStore { ref environment, .. } if environment.as_ref() == Some(&t.environment)),
                Error::Forbidden,
            )?;
            let expiry = model::end(current).min(t.expires_at_ms.unwrap_or(u64::MAX));
            let product = catalog
                .storage_products
                .iter()
                .find(|p| p.product_id == *product)
                .ok_or(Error::NotFound)?;
            model::add_storage(
                s,
                StorageAddon {
                    contract_id: id,
                    order_id: id,
                    storage_bytes: product.storage_bytes,
                    starts_at_ms: t.purchased_at_ms,
                    expires_at_ms: expiry,
                    last_issued_until_ms: 0,
                },
                at,
            )?;
        }
    }
    Ok(())
}

#[ic_cdk::update]
fn apply_app_store_transaction(transaction: StoreTransaction) -> Result<EntitlementView> {
    let caller = ic_cdk::api::msg_caller();
    let home = ic_cdk::api::canister_self();
    let at = api::now();
    let config = CONFIG
        .with_borrow(|c| c.get().0.clone())
        .ok_or(Error::Forbidden)?;
    ensure(config.verifiers.contains(&caller), Error::Forbidden)?;
    ensure(
        !store::config(|c| c.paused) || transaction.revoked_at_ms.is_some(),
        Error::Forbidden,
    )?;
    let kind = validate_transaction(&transaction, &config, at)?;
    let mut subject = api::get_subject(&transaction.beneficiary)?;
    let key = digest(
        "dmsg/app-store/transaction/v1",
        &(&transaction.environment, &transaction.transaction_id),
    );
    let original = digest(
        "dmsg/app-store/original/v1",
        &(
            &transaction.environment,
            &transaction.original_transaction_id,
        ),
    );
    let mut apple_facts = transaction.clone();
    apple_facts.checked_at_ms = 0;
    let fingerprint = digest("dmsg/app-store/observation/v1", &apple_facts);
    let mut business_changed = true;
    if let Some(owner) = OWNERS.with_borrow(|m| m.load(original.as_slice())) {
        ensure(owner == transaction.beneficiary, Error::Forbidden)?;
    }
    if let Some(previous) = TRANSACTIONS.with_borrow(|m| m.load(key.as_slice())) {
        let old = previous.transaction;
        ensure(
            old.beneficiary == transaction.beneficiary
                && old.original_transaction_id == transaction.original_transaction_id
                && old.product_id == transaction.product_id
                && old.purchased_at_ms == transaction.purchased_at_ms,
            Error::IdempotencyConflict,
        )?;
        ensure(
            transaction.signed_at_ms >= old.signed_at_ms,
            Error::VersionConflict,
        )?;
        if transaction.signed_at_ms == old.signed_at_ms {
            ensure(
                previous.fingerprint == fingerprint,
                Error::IdempotencyConflict,
            )?;
            ensure(
                transaction.checked_at_ms >= old.checked_at_ms,
                Error::VersionConflict,
            )?;
            if transaction.checked_at_ms == old.checked_at_ms {
                return subject.view.ok_or(Error::NotFound);
            }
            business_changed = false;
        }
        ensure(
            old.revoked_at_ms.is_none() || old.revoked_at_ms == transaction.revoked_at_ms,
            Error::VersionConflict,
        )?;
    }
    apply(&mut subject, &transaction, &kind, &store::catalog(at), at)?;
    if business_changed {
        subject.business_revision = subject
            .business_revision
            .checked_add(1)
            .ok_or(Error::QuotaExceeded)?;
    }
    let view = model::project(
        home,
        &mut subject,
        &store::catalog(at),
        at,
        store::next_catalog_at(at),
    )?;
    // No await splits these durable changes.
    OWNERS.with_borrow_mut(|m| m.put(original.as_slice(), &transaction.beneficiary));
    TRANSACTIONS.with_borrow_mut(|m| {
        m.put(
            key.as_slice(),
            &Observation {
                transaction,
                fingerprint,
            },
        )
    });
    store::save(&subject);
    store::publish_certification(at);
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use candid::Principal;
    use dmsg_protocol::billing::{beneficiary, default_plans};
    const NOW: u64 = 1_800_000_000_000;

    fn catalog() -> Catalog {
        Catalog {
            schema: 1,
            version: 1,
            effective_at_ms: 0,
            plans: default_plans(1),
            storage_products: vec![],
            terms_digest: Hash::new([9; 32]),
        }
    }

    fn transaction(id: &str, start: u64) -> StoreTransaction {
        let home = Principal::from_slice(&[1]);
        let account = AccountId([2; 12]);
        StoreTransaction {
            beneficiary: beneficiary(home, &account),
            transaction_id: id.into(),
            original_transaction_id: "100".into(),
            product_id: "test.plus.annual".into(),
            app_account_token: account_token(home, &account),
            bundle_id: "net.dmsg.ios".into(),
            environment: StoreEnvironment::Sandbox,
            purchased_at_ms: start,
            expires_at_ms: Some(start + 365 * DAY),
            revoked_at_ms: None,
            signed_at_ms: NOW,
            checked_at_ms: NOW,
        }
    }

    #[test]
    fn receipts_bind_beneficiary_environment_product_and_fresh_status() {
        let t = transaction("101", NOW - DAY);
        let config = AppStoreConfig {
            verifiers: vec![Principal::from_slice(&[4])],
            bundle_id: t.bundle_id.clone(),
            app_apple_id: 6827050680,
            environment: StoreEnvironment::Sandbox,
            accept_sandbox: None,
            products: vec![StoreProduct {
                product_id: t.product_id.clone(),
                kind: StoreProductKind::Plan(PlanId::Plus),
            }],
        };
        assert!(validate_transaction(&t, &config, NOW).is_ok());
        let mut wrong = t.clone();
        wrong.beneficiary = beneficiary(Principal::from_slice(&[5]), &AccountId([2; 12]));
        assert!(validate_transaction(&wrong, &config, NOW).is_err());
        wrong = t.clone();
        wrong.environment = StoreEnvironment::Production;
        assert!(validate_transaction(&wrong, &config, NOW).is_err());
        wrong = t.clone();
        wrong.product_id = "unconfigured".into();
        assert!(validate_transaction(&wrong, &config, NOW).is_err());
        wrong = t.clone();
        wrong.checked_at_ms -= 61_000;
        assert!(validate_transaction(&wrong, &config, NOW).is_err());
        assert!(validate_config(&config, &Environment::Production).is_err());
    }

    #[test]
    fn legacy_contract_environment_is_recovered_from_the_recorded_transaction() {
        let t = transaction("101", NOW - DAY);
        let mut s = model::Subject::new(t.beneficiary.clone());
        apply(
            &mut s,
            &t,
            &StoreProductKind::Plan(PlanId::Plus),
            &catalog(),
            NOW,
        )
        .unwrap();
        let id = s.contracts[0].contract_id;
        if let ContractSource::AppStore { environment, .. } = &mut s.contracts[0].source {
            *environment = None;
        }
        TRANSACTIONS.with_borrow_mut(|m| {
            m.put(
                id.as_slice(),
                &Observation {
                    transaction: t.clone(),
                    fingerprint: Hash::new([0; 32]),
                },
            )
        });
        store::save_subject(&s);
        assert!(
            store::load(&t.beneficiary).unwrap().contracts[0]
                .source
                .is_sandbox()
        );
        // A missing fact must not look like an absent subject, which would be replaced.
        TRANSACTIONS.with_borrow_mut(|m| m.delete(id.as_slice()));
        assert_eq!(store::load(&t.beneficiary), Err(Error::IntegrityFailed));
    }

    #[test]
    fn production_priority_and_same_environment_upgrades_preserve_both_histories() {
        let mut sandbox = transaction("101", NOW - DAY);
        let mut paid = transaction("101", NOW - DAY / 2);
        paid.environment = StoreEnvironment::Production;
        let mut s = model::Subject::new(paid.beneficiary.clone());
        let c = catalog();
        apply(
            &mut s,
            &sandbox,
            &StoreProductKind::Plan(PlanId::Max),
            &c,
            NOW,
        )
        .unwrap();
        apply(
            &mut s,
            &paid,
            &StoreProductKind::Plan(PlanId::Plus),
            &c,
            NOW,
        )
        .unwrap();
        assert_eq!(s.contracts.len(), 2); // The same Apple ID in two environments is not a duplicate.
        assert_eq!(s.active(NOW).unwrap().plan.plan_id, PlanId::Plus);
        let production_id = s.active(NOW).unwrap().contract_id;
        s.created_at_ms = Some(NOW - DAY);
        let paid_end = model::end(s.active(NOW).unwrap());
        sandbox.transaction_id = "102".into();
        sandbox.purchased_at_ms = NOW - MINUTE;
        apply(
            &mut s,
            &sandbox,
            &StoreProductKind::Plan(PlanId::Pro),
            &c,
            NOW,
        )
        .unwrap();
        assert_eq!(s.active(NOW).unwrap().contract_id, production_id);
        assert_eq!(model::end(s.active(NOW).unwrap()), paid_end);
        // Both resource projection and month accounting must use the formal source.
        let view = model::project(Principal::from_slice(&[1]), &mut s, &c, NOW, None).unwrap();
        assert_eq!(view.plan_snapshot.plan_id, PlanId::Plus);
        let month = model::month(
            &s,
            std::slice::from_ref(&c),
            dmsg_protocol::billing::month_utc(NOW).unwrap(),
        )
        .unwrap();
        assert_eq!(
            month
                .segments
                .iter()
                .find(|segment| segment.start_ms <= NOW && NOW < segment.end_ms)
                .unwrap()
                .source_contract_id,
            Some(production_id)
        );
        sandbox.revoked_at_ms = Some(NOW);
        apply(
            &mut s,
            &sandbox,
            &StoreProductKind::Plan(PlanId::Pro),
            &c,
            NOW,
        )
        .unwrap();
        assert_eq!(s.active(NOW).unwrap().contract_id, production_id);
        paid.revoked_at_ms = Some(NOW);
        apply(
            &mut s,
            &paid,
            &StoreProductKind::Plan(PlanId::Plus),
            &c,
            NOW,
        )
        .unwrap();
        assert!(s.active(NOW).is_none());
    }

    #[test]
    fn sandbox_renewals_use_apple_expiry_and_never_mint_an_annual_term() {
        let mut first = transaction("101", NOW);
        first.expires_at_ms = Some(NOW + MINUTE);
        let mut s = model::Subject::new(first.beneficiary.clone());
        apply(
            &mut s,
            &first,
            &StoreProductKind::Plan(PlanId::Plus),
            &catalog(),
            NOW,
        )
        .unwrap();
        assert!(s.active(NOW + MINUTE).is_none());
        let mut renewal = transaction("102", NOW + MINUTE);
        renewal.expires_at_ms = Some(NOW + 2 * MINUTE);
        apply(
            &mut s,
            &renewal,
            &StoreProductKind::Plan(PlanId::Plus),
            &catalog(),
            NOW + MINUTE,
        )
        .unwrap();
        assert_eq!(
            s.active(NOW + MINUTE).unwrap().expires_at_ms,
            NOW + 2 * MINUTE
        );
        assert!(s.active(NOW + 2 * MINUTE).is_none());
    }

    #[test]
    fn repeat_does_not_double_grant_and_new_status_extends_or_revokes() {
        let mut t = transaction("101", NOW - DAY);
        let mut subject = model::Subject::new(t.beneficiary.clone());
        let kind = StoreProductKind::Plan(PlanId::Plus);
        apply(&mut subject, &t, &kind, &catalog(), NOW).unwrap();
        apply(&mut subject, &t, &kind, &catalog(), NOW).unwrap();
        assert_eq!(subject.contracts.len(), 1);
        t.expires_at_ms = t.expires_at_ms.map(|at| at + DAY);
        t.checked_at_ms += MINUTE;
        apply(&mut subject, &t, &kind, &catalog(), NOW + MINUTE).unwrap();
        assert_eq!(subject.contracts[0].expires_at_ms, t.expires_at_ms.unwrap());
        t.revoked_at_ms = Some(NOW);
        apply(&mut subject, &t, &kind, &catalog(), NOW + MINUTE).unwrap();
        assert!(subject.active(NOW + MINUTE).is_none());
    }

    #[test]
    fn late_delivery_cannot_reactivate_a_replaced_subscription_after_refund() {
        let new = transaction("102", NOW - MINUTE);
        let old = transaction("101", NOW - DAY);
        let mut subject = model::Subject::new(new.beneficiary.clone());
        apply(
            &mut subject,
            &new,
            &StoreProductKind::Plan(PlanId::Pro),
            &catalog(),
            NOW,
        )
        .unwrap();
        apply(
            &mut subject,
            &old,
            &StoreProductKind::Plan(PlanId::Plus),
            &catalog(),
            NOW,
        )
        .unwrap();
        assert_eq!(model::end(&subject.contracts[0]), new.purchased_at_ms);
        let mut revoked = new;
        revoked.revoked_at_ms = Some(NOW);
        apply(
            &mut subject,
            &revoked,
            &StoreProductKind::Plan(PlanId::Pro),
            &catalog(),
            NOW,
        )
        .unwrap();
        assert!(subject.active(NOW).is_none());
    }

    #[test]
    fn refund_after_upgrade_does_not_reopen_the_replaced_term() {
        let old = transaction("101", NOW - DAY);
        let new = transaction("102", NOW - MINUTE);
        let mut subject = model::Subject::new(old.beneficiary.clone());
        let plus = StoreProductKind::Plan(PlanId::Plus);
        apply(&mut subject, &old, &plus, &catalog(), NOW).unwrap();
        apply(
            &mut subject,
            &new,
            &StoreProductKind::Plan(PlanId::Pro),
            &catalog(),
            NOW,
        )
        .unwrap();
        let mut refunded = old;
        refunded.revoked_at_ms = Some(NOW);
        apply(&mut subject, &refunded, &plus, &catalog(), NOW).unwrap();
        assert_eq!(model::end(&subject.contracts[0]), new.purchased_at_ms);
    }

    #[test]
    fn apple_rights_have_short_leases_and_expire_without_status_refresh() {
        let t = transaction("101", NOW - DAY);
        let mut subject = model::Subject::new(t.beneficiary.clone());
        apply(
            &mut subject,
            &t,
            &StoreProductKind::Plan(PlanId::Plus),
            &catalog(),
            NOW,
        )
        .unwrap();
        let view = model::project(
            Principal::from_slice(&[6]),
            &mut subject,
            &catalog(),
            NOW,
            None,
        )
        .unwrap();
        assert_eq!(view.source_status, SourceStatus::Active);
        assert!(view.valid_until_ms <= NOW + 60 * MINUTE);
        let view = model::project(
            Principal::from_slice(&[6]),
            &mut subject,
            &catalog(),
            NOW + 60 * MINUTE,
            None,
        )
        .unwrap();
        assert_eq!(view.source_status, SourceStatus::Unverifiable);
    }
}
