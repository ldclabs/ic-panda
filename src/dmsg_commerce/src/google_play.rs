//! Google Play adapter. Only governed verifier principals submit API facts;
//! neither Android callbacks nor RTDN event types are entitlement authority.
use crate::{api, model, store};
use dmsg_protocol::{billing::beneficiary_account, digest, google_play::*};
use dmsg_runtime::{
    admin::{Validation, validation},
    storage::{MapExt, Stored},
};
use dmsg_types::{billing::*, google_play::*, membership::Eligibility, *};
use ic_stable_structures::{
    DefaultMemoryImpl, StableBTreeMap, StableCell, memory_manager::VirtualMemory,
};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeSet};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Serialize, Deserialize)]
struct Observation {
    transaction: PlayTransaction,
    kind: PlayProductKind,
    fingerprint: Hash,
    superseded_at: Option<u64>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Receipt {
    transaction: PlayTransaction,
    kind: PlayProductKind,
    delivered: bool,
    storage_expires: Option<u64>,
    base_term_expires: Option<u64>,
    replaced_at: Option<u64>,
}

thread_local! {
    static CONFIG: RefCell<StableCell<Stored<Option<GooglePlayConfig>>, Memory>> = RefCell::new(StableCell::init(store::memory(38), Stored(None)));
    static TOKENS: RefCell<StableBTreeMap<Vec<u8>, Stored<Observation>, Memory>> = RefCell::new(StableBTreeMap::init(store::memory(39)));
    static RECEIPTS: RefCell<StableBTreeMap<Vec<u8>, Stored<Receipt>, Memory>> = RefCell::new(StableBTreeMap::init(store::memory(40)));
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn option(kind: &PlayProductKind) -> &str {
    match kind {
        PlayProductKind::Plan { base_plan_id, .. } => base_plan_id,
        PlayProductKind::Storage {
            purchase_option_id, ..
        } => purchase_option_id,
    }
}

fn validate_config(config: &GooglePlayConfig, environment: &Environment) -> Result<()> {
    ensure_valid(
        !config.verifiers.is_empty() && config.verifiers.len() <= 8,
        "verifiers",
    )?;
    let mut verifiers = BTreeSet::new();
    for verifier in &config.verifiers {
        dmsg_protocol::authenticated(*verifier)?;
        ensure_valid(verifiers.insert(*verifier), "duplicate verifier")?;
    }
    ensure_valid(
        valid_id(&config.package_name) && config.package_name.contains('.'),
        "package",
    )?;
    ensure(
        *environment != Environment::Production
            || config.environment == PlayEnvironment::Production,
        Error::Forbidden,
    )?;
    ensure_valid(config.products.len() <= 128, "products")?;
    let mut products = BTreeSet::new();
    for product in &config.products {
        ensure_valid(
            valid_id(&product.product_id)
                && valid_id(option(&product.kind))
                && product.offer_id.as_ref().is_none_or(|id| valid_id(id)),
            "product selection",
        )?;
        ensure_valid(
            products.insert((
                &product.product_id,
                option(&product.kind),
                &product.offer_id,
            )),
            "duplicate product selection",
        )?;
        ensure_valid(
            !matches!(
                product.kind,
                PlayProductKind::Plan {
                    plan: PlanId::Free,
                    ..
                }
            ),
            "paid plan",
        )?;
    }
    Ok(())
}

#[ic_cdk::update]
fn set_google_play_config(config: GooglePlayConfig) -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    store::check_admin(caller)?;
    validate_config(&config, &store::config(|c| c.environment.clone()))?;
    CONFIG.with_borrow_mut(|c| c.set(Stored(Some(config))));
    Ok(())
}

#[ic_cdk::query]
fn validate_set_google_play_config(config: GooglePlayConfig) -> Validation {
    validation(validate_config(&config, &store::config(|c| c.environment.clone())).map(|()| {
        format!("Set Google Play config for {} ({:?}, accept test: {}): {} verifier(s), {} product selection(s).", config.package_name, config.environment, config.accept_test, config.verifiers.len(), config.products.len())
    }))
}

#[ic_cdk::query]
fn get_google_play_config() -> Option<GooglePlayConfig> {
    CONFIG.with_borrow(|c| c.get().0.clone())
}

fn fingerprint(transaction: &PlayTransaction) -> Hash {
    let mut facts = transaction.clone();
    facts.revision = 0;
    facts.checked_at_ms = 0;
    digest("dmsg/google-play/observation/v1", &facts)
}

fn eligible(state: &PlayState) -> bool {
    matches!(
        state,
        PlayState::Active | PlayState::Grace | PlayState::Canceled
    )
}

fn validate_transaction(
    t: &PlayTransaction,
    config: &GooglePlayConfig,
    previous: Option<&Observation>,
    at: u64,
) -> Result<PlayProductKind> {
    ensure(t.package_name == config.package_name, Error::Forbidden)?;
    ensure(
        config.accepts(&t.environment)
            || previous.is_some_and(|p| {
                p.transaction.beneficiary == t.beneficiary
                    && p.transaction.environment == t.environment
            }),
        Error::Forbidden,
    )?;
    ensure_valid(
        t.revision > 0
            && t.checked_at_ms <= at.saturating_add(60_000)
            && at.saturating_sub(t.checked_at_ms) <= 60_000,
        "observation",
    )?;
    ensure_valid(
        t.purchased_at_ms <= t.checked_at_ms.saturating_add(60_000)
            && t.order_id.as_ref().is_none_or(|id| valid_id(id))
            && t.linked_token_hash != Some(t.token_hash),
        "purchase",
    )?;
    ensure(
        t.account_binding
            == account_token(
                t.beneficiary.authority_canister,
                &beneficiary_account(&t.beneficiary)?,
            ),
        Error::Forbidden,
    )?;
    let kind = config
        .products
        .iter()
        .find(|p| {
            p.product_id == t.product_id
                && option(&p.kind) == t.option_id
                && p.offer_id == t.offer_id
        })
        .map(|p| p.kind.clone())
        .ok_or(Error::UnsupportedProtocol)?;
    // enabled controls new checkout preparation, not whether an already paid
    // and verified receipt may be recovered after that switch changes.
    match &kind {
        PlayProductKind::Plan { .. } => {
            ensure_valid(t.base_contract_id.is_none(), "subscription base")?;
            ensure_valid(
                t.order_id.is_some()
                    && t.expires_at_ms.is_some_and(|e| e >= t.purchased_at_ms)
                    && t.term_starts_at_ms
                        .is_none_or(|start| start <= t.purchased_at_ms),
                "subscription period",
            )?;
            ensure(
                !matches!(
                    t.state,
                    PlayState::Pending | PlayState::PendingCanceled | PlayState::Refunded
                ),
                Error::Forbidden,
            )?;
        }
        PlayProductKind::Storage { .. } => {
            ensure_valid(
                t.base_contract_id.is_some()
                    && t.term_starts_at_ms.is_none()
                    && t.expires_at_ms.is_none()
                    && t.linked_token_hash.is_none()
                    && !t.replaced,
                "storage purchase",
            )?;
            ensure(
                matches!(t.state, PlayState::Active | PlayState::Refunded),
                Error::Forbidden,
            )?;
            ensure(
                !(config.environment == PlayEnvironment::Production
                    && t.environment == PlayEnvironment::Test),
                Error::UnsupportedProtocol,
            )?;
        }
    }
    if let Some(previous) = previous {
        let old = &previous.transaction;
        ensure(old.beneficiary == t.beneficiary, Error::Forbidden)?;
        ensure(
            std::mem::discriminant(&previous.kind) == std::mem::discriminant(&kind),
            Error::IdempotencyConflict,
        )?;
        ensure(
            t.revision >= old.revision
                && t.checked_at_ms >= old.checked_at_ms
                && t.purchased_at_ms >= old.purchased_at_ms,
            Error::VersionConflict,
        )?;
        if t.revision == old.revision {
            ensure(
                previous.fingerprint == fingerprint(t),
                Error::IdempotencyConflict,
            )?;
        }
        ensure(!old.replaced || t.replaced, Error::VersionConflict)?;
        ensure(
            previous.superseded_at.is_none() || t.replaced,
            Error::VersionConflict,
        )?;
        if matches!(kind, PlayProductKind::Storage { .. }) {
            ensure(
                old.state != PlayState::Refunded || t.state == PlayState::Refunded,
                Error::VersionConflict,
            )?;
        }
    }
    Ok(kind)
}

/// Apply a single verified observation to an in-memory subject. All fallible
/// validation and projection finish before any stable receipt is committed.
fn apply(
    s: &mut model::Subject,
    t: &PlayTransaction,
    kind: &PlayProductKind,
    prior: Option<&Receipt>,
    parent: Option<&Receipt>,
    catalog: &Catalog,
    at: u64,
) -> Result<(Receipt, Vec<Hash>)> {
    let id = transaction_id(t, kind);
    if let Some(old) = prior {
        ensure(
            old.transaction.beneficiary == t.beneficiary,
            Error::Forbidden,
        )?;
        ensure(
            old.kind == *kind
                && old.transaction.product_id == t.product_id
                && old.transaction.option_id == t.option_id
                && old.transaction.offer_id == t.offer_id
                && old.transaction.purchased_at_ms == t.purchased_at_ms
                && old
                    .transaction
                    .term_starts_at_ms
                    .unwrap_or(old.transaction.purchased_at_ms)
                    == t.term_starts_at_ms.unwrap_or(t.purchased_at_ms)
                && old.transaction.base_contract_id == t.base_contract_id,
            Error::IdempotencyConflict,
        )?;
    }
    let mut receipt = prior.cloned().unwrap_or(Receipt {
        transaction: t.clone(),
        kind: kind.clone(),
        delivered: false,
        storage_expires: None,
        base_term_expires: None,
        replaced_at: None,
    });
    let mut replaced = vec![];
    match kind {
        PlayProductKind::Plan { plan, .. } => {
            let expires = t
                .expires_at_ms
                .ok_or_else(|| invalid("subscription expiry"))?;
            let active = eligible(&t.state) && !t.replaced && expires > at;
            // Replacement truncates Google's old token, not an independently
            // purchased storage term. Retain the prior endpoint before updating it.
            if receipt.base_term_expires.is_none() && receipt.delivered {
                receipt.base_term_expires = receipt.transaction.expires_at_ms;
            }
            if active {
                receipt.base_term_expires =
                    Some(receipt.base_term_expires.unwrap_or(0).max(expires));
            }
            if prior.is_none() && active && t.environment == PlayEnvironment::Production {
                ensure(s.active(t.purchased_at_ms).is_none_or(|c| {
                    c.source.is_sandbox() || matches!(&c.source, ContractSource::GooglePlay { token_hash, environment, .. }
                        if *environment == t.environment && (*token_hash == t.token_hash || Some(*token_hash) == t.linked_token_hash))
                }), Error::Forbidden)?;
            }
            // Only an actual paid/owned replacement can end the predecessor.
            // PENDING observations are refused before reaching this transition.
            for old in &mut s.contracts {
                if old.contract_id != id
                    && old.starts_at_ms <= t.purchased_at_ms
                    && matches!(&old.source, ContractSource::GooglePlay { token_hash, environment, .. }
                        if *environment == t.environment && (*token_hash == t.token_hash || Some(*token_hash) == t.linked_token_hash))
                {
                    old.terminated_at_ms = Some(
                        old.terminated_at_ms
                            .unwrap_or(u64::MAX)
                            .min(t.purchased_at_ms),
                    );
                    replaced.push(old.contract_id);
                }
            }
            if t.replaced {
                receipt.replaced_at = Some(receipt.replaced_at.unwrap_or(u64::MAX).min(at));
            }
            if let Some(existing) = s.contracts.iter_mut().find(|c| c.contract_id == id) {
                existing.expires_at_ms = expires;
                existing.observed_at_ms = t.checked_at_ms;
                existing.qualified_until_ms =
                    expires.min(t.checked_at_ms.saturating_add(model::REFUNDABLE_LEASE_MS));
                if let Some(cut) = receipt.replaced_at {
                    existing.terminated_at_ms =
                        Some(existing.terminated_at_ms.unwrap_or(u64::MAX).min(cut));
                }
                model::set_eligibility(
                    existing,
                    if active {
                        Eligibility::Eligible
                    } else {
                        Eligibility::Ineligible
                    },
                    if active {
                        t.checked_at_ms
                    } else {
                        expires.min(t.checked_at_ms).max(t.purchased_at_ms)
                    },
                )?;
                receipt.delivered |= active && existing.terminated_at_ms.is_none_or(|end| end > at);
            } else if active && receipt.replaced_at.is_none_or(|cut| cut > at) {
                s.contracts.push(MembershipContract {
                    term_starts_at_ms: t.term_starts_at_ms.unwrap_or(t.purchased_at_ms),
                    // A pruned, previously closed receipt cannot backfill the gap
                    // when a later verified provider observation restores it.
                    resource_pauses: if prior.is_some() {
                        vec![(t.purchased_at_ms, Some(t.checked_at_ms))]
                    } else {
                        vec![]
                    },
                    contract_id: id,
                    plan: model::plan(catalog, plan)?,
                    source: ContractSource::GooglePlay {
                        transaction_id: id,
                        token_hash: t.token_hash,
                        environment: t.environment.clone(),
                    },
                    starts_at_ms: t.purchased_at_ms,
                    expires_at_ms: expires,
                    terminated_at_ms: receipt.replaced_at,
                    eligibility: Eligibility::Eligible,
                    observed_at_ms: t.checked_at_ms,
                    qualified_until_ms: expires
                        .min(t.checked_at_ms.saturating_add(model::REFUNDABLE_LEASE_MS)),
                });
                receipt.delivered = true;
                s.contracts.sort_by_key(|c| c.starts_at_ms);
            }
        }
        PlayProductKind::Storage { product, .. } => {
            if let Some(existing) = s.addons.iter_mut().find(|a| a.contract_id == id) {
                if t.state == PlayState::Refunded {
                    existing.expires_at_ms = existing.expires_at_ms.min(t.checked_at_ms);
                } else {
                    s.store_qualifications
                        .get_or_insert_with(Default::default)
                        .insert(
                            id,
                            (
                                t.checked_at_ms,
                                existing.expires_at_ms.min(
                                    t.checked_at_ms.saturating_add(model::REFUNDABLE_LEASE_MS),
                                ),
                            ),
                        );
                }
            } else if !receipt.delivered && t.state == PlayState::Active {
                let parent = parent.ok_or(Error::NotFound)?;
                ensure(
                    parent.delivered
                        && matches!(parent.kind, PlayProductKind::Plan { .. })
                        && parent.transaction.beneficiary == t.beneficiary
                        && parent.transaction.environment == t.environment
                        && Some(transaction_id(&parent.transaction, &parent.kind))
                            == t.base_contract_id
                        && parent.transaction.purchased_at_ms <= t.purchased_at_ms
                        && parent.replaced_at.is_none_or(|end| t.purchased_at_ms < end),
                    Error::Forbidden,
                )?;
                let expires = parent
                    .base_term_expires
                    .or(parent.transaction.expires_at_ms)
                    .ok_or(Error::IntegrityFailed)?;
                ensure(t.purchased_at_ms < expires, Error::Forbidden)?;
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
                        expires_at_ms: expires,
                        last_issued_until_ms: 0,
                    },
                    at,
                )?;
                s.store_qualifications
                    .get_or_insert_with(Default::default)
                    .insert(
                        id,
                        (
                            t.checked_at_ms,
                            expires.min(t.checked_at_ms.saturating_add(model::REFUNDABLE_LEASE_MS)),
                        ),
                    );
                receipt.delivered = true;
                receipt.storage_expires = Some(expires);
            }
        }
    }
    receipt.transaction = t.clone();
    Ok((receipt, replaced))
}

#[ic_cdk::update]
fn apply_google_play_transaction(transaction: PlayTransaction) -> Result<EntitlementView> {
    let caller = ic_cdk::api::msg_caller();
    let home = ic_cdk::api::canister_self();
    let at = api::now();
    let config = CONFIG
        .with_borrow(|c| c.get().0.clone())
        .ok_or(Error::Forbidden)?;
    ensure(config.verifiers.contains(&caller), Error::Forbidden)?;
    ensure(
        !store::config(|c| c.paused) || !eligible(&transaction.state) || transaction.replaced,
        Error::Forbidden,
    )?;
    let token = token_id(
        &transaction.environment,
        &transaction.package_name,
        &transaction.token_hash,
    );
    let previous = TOKENS.with_borrow(|m| m.load(token.as_slice()));
    let kind = validate_transaction(&transaction, &config, previous.as_ref(), at)?;
    let linked = transaction
        .linked_token_hash
        .map(|hash| {
            let key = token_id(&transaction.environment, &transaction.package_name, &hash);
            TOKENS
                .with_borrow(|m| m.load(key.as_slice()))
                .ok_or(Error::NotFound)
        })
        .transpose()?;
    if let Some(linked) = &linked {
        ensure(
            linked.transaction.beneficiary == transaction.beneficiary
                && matches!(linked.kind, PlayProductKind::Plan { .. }),
            Error::Forbidden,
        )?;
    }
    let id = transaction_id(&transaction, &kind);
    let prior = RECEIPTS.with_borrow(|m| m.load(id.as_slice()));
    let facts = fingerprint(&transaction);
    let mut subject = api::get_subject(&transaction.beneficiary)?;
    if previous.as_ref().is_some_and(|p| {
        p.transaction.revision == transaction.revision
            && p.transaction.checked_at_ms == transaction.checked_at_ms
            && p.fingerprint == facts
    }) {
        return subject.view.ok_or(Error::NotFound);
    }
    let catalog = store::catalog(at);
    let parent = transaction
        .base_contract_id
        .and_then(|id| RECEIPTS.with_borrow(|m| m.load(id.as_slice())));
    let (receipt, mut replaced) = apply(
        &mut subject,
        &transaction,
        &kind,
        prior.as_ref(),
        parent.as_ref(),
        &catalog,
        at,
    )?;
    // Keep replacement bounds even if the subject's old monthly terms were pruned.
    if matches!(kind, PlayProductKind::Plan { .. }) {
        for old in previous.iter().chain(linked.iter()) {
            let old_id = transaction_id(&old.transaction, &old.kind);
            if old_id != id {
                replaced.push(old_id);
            }
        }
    }
    if previous.as_ref().is_none_or(|p| p.fingerprint != facts) {
        subject.business_revision = subject
            .business_revision
            .checked_add(1)
            .ok_or(Error::QuotaExceeded)?;
    }
    let view = model::project(home, &mut subject, &catalog, at, store::next_catalog_at(at))?;
    // No fallible operation or await follows this atomic message's commit point.
    for old_id in replaced {
        if let Some(mut old) = RECEIPTS.with_borrow(|m| m.load(old_id.as_slice())) {
            old.replaced_at = Some(
                old.replaced_at
                    .unwrap_or(u64::MAX)
                    .min(transaction.purchased_at_ms),
            );
            RECEIPTS.with_borrow_mut(|m| m.put(old_id.as_slice(), &old));
        }
    }
    RECEIPTS.with_borrow_mut(|m| m.put(id.as_slice(), &receipt));
    if let Some(mut linked) = linked {
        linked.superseded_at = Some(
            linked
                .superseded_at
                .unwrap_or(u64::MAX)
                .min(transaction.purchased_at_ms),
        );
        let key = token_id(
            &linked.transaction.environment,
            &linked.transaction.package_name,
            &linked.transaction.token_hash,
        );
        TOKENS.with_borrow_mut(|m| m.put(key.as_slice(), &linked));
    }
    let superseded_at = previous.and_then(|p| p.superseded_at);
    TOKENS.with_borrow_mut(|m| {
        m.put(
            token.as_slice(),
            &Observation {
                transaction,
                kind,
                fingerprint: facts,
                superseded_at,
            },
        )
    });
    store::save(&subject);
    store::publish_certification(at);
    Ok(view)
}

#[ic_cdk::query]
fn get_google_play_receipt(
    environment: PlayEnvironment,
    token_hash: Hash,
) -> Result<Option<PlayReceipt>> {
    let caller = ic_cdk::api::msg_caller();
    let config = CONFIG
        .with_borrow(|c| c.get().0.clone())
        .ok_or(Error::Forbidden)?;
    ensure(config.verifiers.contains(&caller), Error::Forbidden)?;
    let token = token_id(&environment, &config.package_name, &token_hash);
    let Some(current) = TOKENS.with_borrow(|m| m.load(token.as_slice())) else {
        return Ok(None);
    };
    let id = transaction_id(&current.transaction, &current.kind);
    let receipt = RECEIPTS
        .with_borrow(|m| m.load(id.as_slice()))
        .ok_or(Error::IntegrityFailed)?;
    Ok(Some(PlayReceipt {
        transaction_id: id,
        beneficiary: receipt.transaction.beneficiary,
        revision: current.transaction.revision,
        checked_at_ms: current.transaction.checked_at_ms,
        delivered: receipt.delivered,
        state: current.transaction.state,
        expires_at_ms: receipt
            .storage_expires
            .or(receipt.transaction.expires_at_ms),
    }))
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
            storage_products: vec![StorageProduct {
                product_id: Hash::new([6; 32]),
                storage_bytes: 1_000_000_000,
                price_cents: 100,
            }],
            terms_digest: Hash::new([9; 32]),
        }
    }

    fn kind() -> PlayProductKind {
        PlayProductKind::Plan {
            plan: PlanId::Plus,
            base_plan_id: "annual".into(),
        }
    }

    fn transaction() -> PlayTransaction {
        let home = Principal::from_slice(&[1]);
        let account = AccountId([2; 12]);
        PlayTransaction {
            beneficiary: beneficiary(home, &account),
            token_hash: Hash::new([3; 32]),
            order_id: Some("GPA.0000..0".into()),
            product_id: "dmsg.plus".into(),
            option_id: "annual".into(),
            offer_id: None,
            account_binding: account_token(home, &account),
            package_name: "net.dmsg.android".into(),
            environment: PlayEnvironment::Production,
            state: PlayState::Active,
            purchased_at_ms: NOW - DAY,
            term_starts_at_ms: Some(NOW - DAY),
            expires_at_ms: Some(NOW + 365 * DAY),
            linked_token_hash: None,
            base_contract_id: None,
            replaced: false,
            revision: 1,
            checked_at_ms: NOW,
        }
    }

    fn config() -> GooglePlayConfig {
        GooglePlayConfig {
            verifiers: vec![Principal::from_slice(&[4, 1])],
            package_name: "net.dmsg.android".into(),
            environment: PlayEnvironment::Production,
            accept_test: false,
            products: vec![
                PlayProduct {
                    product_id: "dmsg.plus".into(),
                    offer_id: None,
                    kind: kind(),
                    enabled: true,
                },
                PlayProduct {
                    product_id: "dmsg.storage".into(),
                    offer_id: None,
                    kind: storage_kind(),
                    enabled: true,
                },
            ],
        }
    }

    fn observation(t: &PlayTransaction) -> Observation {
        Observation {
            transaction: t.clone(),
            kind: kind(),
            fingerprint: fingerprint(t),
            superseded_at: None,
        }
    }

    fn storage_kind() -> PlayProductKind {
        PlayProductKind::Storage {
            product: Hash::new([6; 32]),
            purchase_option_id: "buy".into(),
        }
    }

    fn storage(base: &PlayTransaction) -> PlayTransaction {
        PlayTransaction {
            token_hash: Hash::new([5; 32]),
            order_id: None,
            product_id: "dmsg.storage".into(),
            option_id: "buy".into(),
            purchased_at_ms: NOW,
            term_starts_at_ms: None,
            expires_at_ms: None,
            base_contract_id: Some(transaction_id(base, &kind())),
            ..base.clone()
        }
    }

    fn initial() -> (model::Subject, PlayTransaction, Receipt) {
        let t = transaction();
        let mut s = model::Subject::new(t.beneficiary.clone());
        let (r, _) = apply(&mut s, &t, &kind(), None, None, &catalog(), NOW).unwrap();
        (s, t, r)
    }

    #[test]
    fn admission_binds_owner_package_exact_selection_and_current_provider_facts() {
        let t = transaction();
        let mut cfg = config();
        assert!(validate_config(&cfg, &Environment::Production).is_ok());
        assert!(validate_transaction(&t, &cfg, None, NOW).is_ok());
        for field in 0..7 {
            let mut wrong = t.clone();
            match field {
                0 => wrong.package_name.push_str(".other"),
                1 => wrong.account_binding = "0".repeat(64),
                2 => wrong.option_id = "monthly".into(),
                3 => wrong.offer_id = Some("trial".into()),
                4 => wrong.checked_at_ms = NOW - 60_001,
                5 => wrong.state = PlayState::Pending,
                _ => {
                    wrong.beneficiary =
                        beneficiary(Principal::from_slice(&[5]), &AccountId([2; 12]))
                }
            }
            assert!(
                validate_transaction(&wrong, &cfg, None, NOW).is_err(),
                "field {field}"
            );
        }
        cfg.products[0].enabled = false;
        assert!(
            validate_transaction(&t, &cfg, None, NOW).is_ok(),
            "paid receipt recovery survives disabled checkout"
        );
        cfg.verifiers.push(cfg.verifiers[0]);
        assert!(validate_config(&cfg, &Environment::Production).is_err());
    }

    #[test]
    fn revisions_cannot_rewrite_facts_reverse_time_or_revive_a_replaced_token() {
        let t = transaction();
        let mut old = observation(&t);
        let mut next = t.clone();
        next.expires_at_ms = Some(NOW + DAY);
        assert!(validate_transaction(&next, &config(), Some(&old), NOW).is_err());
        next.revision += 1;
        assert!(validate_transaction(&next, &config(), Some(&old), NOW).is_ok());
        next.checked_at_ms -= 1;
        assert!(validate_transaction(&next, &config(), Some(&old), NOW).is_err());
        next.checked_at_ms = NOW;
        old.superseded_at = Some(NOW);
        assert!(validate_transaction(&next, &config(), Some(&old), NOW).is_err());
        next.replaced = true;
        assert!(validate_transaction(&next, &config(), Some(&old), NOW).is_ok());
    }

    #[test]
    fn canceled_renewal_retains_access_but_hold_and_restoration_exclude_the_gap() {
        let (mut s, mut t, r) = initial();
        t.state = PlayState::Canceled;
        let (r, _) = apply(&mut s, &t, &kind(), Some(&r), None, &catalog(), NOW).unwrap();
        let view =
            model::project(Principal::from_slice(&[9]), &mut s, &catalog(), NOW, None).unwrap();
        assert_eq!(view.plan_snapshot.plan_id, PlanId::Plus);
        assert_eq!(view.source_status, SourceStatus::Active);
        t.state = PlayState::OnHold;
        t.checked_at_ms += MINUTE;
        let (r, _) = apply(
            &mut s,
            &t,
            &kind(),
            Some(&r),
            None,
            &catalog(),
            t.checked_at_ms,
        )
        .unwrap();
        let view = model::project(
            Principal::from_slice(&[9]),
            &mut s,
            &catalog(),
            t.checked_at_ms,
            None,
        )
        .unwrap();
        assert_eq!(view.source_status, SourceStatus::Suspended);
        t.state = PlayState::Active;
        t.checked_at_ms += MINUTE;
        apply(
            &mut s,
            &t,
            &kind(),
            Some(&r),
            None,
            &catalog(),
            t.checked_at_ms,
        )
        .unwrap();
        assert_eq!(
            s.contracts[0].resource_pauses,
            vec![(NOW + MINUTE, Some(NOW + 2 * MINUTE))]
        );
        assert_eq!(s.contracts[0].eligibility, Eligibility::Eligible);
    }

    #[test]
    fn renewal_uses_exact_google_period_and_terminates_previous_receipt() {
        let (mut s, mut next, _) = initial();
        let first_id = s.contracts[0].contract_id;
        next.order_id = Some("GPA.0000..1".into());
        next.purchased_at_ms = NOW;
        next.term_starts_at_ms = Some(NOW);
        next.expires_at_ms = Some(NOW + 3 * DAY); // accelerated test/deferral, not an assumed year
        let (_, replaced) = apply(&mut s, &next, &kind(), None, None, &catalog(), NOW).unwrap();
        assert_eq!(replaced, vec![first_id]);
        assert_eq!(model::end(&s.contracts[0]), NOW);
        assert_eq!(s.active(NOW).unwrap().expires_at_ms, NOW + 3 * DAY);
        assert_eq!(s.active(NOW).unwrap().term_starts_at_ms, NOW);
    }

    #[test]
    fn formal_cash_and_apple_rights_remain_above_test_play() {
        for apple in [false, true] {
            let (mut s, mut t, _) = initial();
            s.contracts[0].source = if apple {
                ContractSource::AppStore {
                    transaction_id: Hash::new([9; 32]),
                    environment: Some(dmsg_types::app_store::StoreEnvironment::Production),
                }
            } else {
                ContractSource::Cash {
                    order_id: Hash::new([9; 32]),
                }
            };
            t.token_hash = Hash::new([8; 32]);
            t.purchased_at_ms = NOW;
            assert!(apply(&mut s, &t, &kind(), None, None, &catalog(), NOW).is_err());
            t.environment = PlayEnvironment::Test;
            let max = PlayProductKind::Plan {
                plan: PlanId::Max,
                base_plan_id: "annual".into(),
            };
            apply(&mut s, &t, &max, None, None, &catalog(), NOW).unwrap();
            assert_eq!(s.active(NOW).unwrap().plan.plan_id, PlanId::Plus);
        }
        let mut cfg = config();
        cfg.accept_test = true;
        let (_, mut t, _) = initial();
        t.environment = PlayEnvironment::Test;
        assert!(validate_transaction(&t, &cfg, None, NOW).is_ok());
        let addon = storage(&t);
        assert_eq!(
            validate_transaction(&addon, &cfg, None, NOW),
            Err(Error::UnsupportedProtocol)
        );
    }

    #[test]
    fn storage_recovery_uses_original_base_even_after_the_next_period_started() {
        let (mut s, base, parent) = initial();
        let addon = storage(&base);
        let mut renewal = base.clone();
        renewal.order_id = Some("GPA.0000..1".into());
        renewal.purchased_at_ms = NOW + DAY;
        renewal.term_starts_at_ms = Some(NOW + DAY);
        renewal.expires_at_ms = Some(NOW + 730 * DAY);
        apply(&mut s, &renewal, &kind(), None, None, &catalog(), NOW + DAY).unwrap();
        let (receipt, _) = apply(
            &mut s,
            &addon,
            &storage_kind(),
            None,
            Some(&parent),
            &catalog(),
            NOW + DAY,
        )
        .unwrap();
        assert!(receipt.delivered);
        assert_eq!(s.addons[0].expires_at_ms, base.expires_at_ms.unwrap());
        let mut supplied_wrong_parent = parent.clone();
        supplied_wrong_parent.transaction = renewal;
        let mut empty = model::Subject::new(base.beneficiary.clone());
        assert!(
            apply(
                &mut empty,
                &addon,
                &storage_kind(),
                None,
                Some(&supplied_wrong_parent),
                &catalog(),
                NOW
            )
            .is_err()
        );
    }

    #[test]
    fn storage_is_idempotent_after_late_order_id_and_expired_terms_are_pruned() {
        let (mut s, base, parent) = initial();
        let mut addon = storage(&base);
        let (r, _) = apply(
            &mut s,
            &addon,
            &storage_kind(),
            None,
            Some(&parent),
            &catalog(),
            NOW,
        )
        .unwrap();
        addon.order_id = Some("GPA.late-order".into());
        let (r, _) = apply(
            &mut s,
            &addon,
            &storage_kind(),
            Some(&r),
            None,
            &catalog(),
            NOW,
        )
        .unwrap();
        assert_eq!(s.addons.len(), 1);
        model::prune(&mut s, base.expires_at_ms.unwrap() + DAY).unwrap();
        assert!(s.addons.is_empty());
        assert!(s.store_qualifications.as_ref().unwrap().is_empty());
        apply(
            &mut s,
            &addon,
            &storage_kind(),
            Some(&r),
            None,
            &catalog(),
            NOW + 400 * DAY,
        )
        .unwrap();
        assert!(s.addons.is_empty());
    }

    #[test]
    fn independent_storage_requires_fresh_proof_and_refund_is_terminal() {
        let (mut s, base, parent) = initial();
        let mut addon = storage(&base);
        let (r, _) = apply(
            &mut s,
            &addon,
            &storage_kind(),
            None,
            Some(&parent),
            &catalog(),
            NOW,
        )
        .unwrap();
        s.contracts.clear(); // independent addon survives base ending
        let view =
            model::project(Principal::from_slice(&[9]), &mut s, &catalog(), NOW, None).unwrap();
        assert_eq!(view.addons.len(), 1);
        let stale = model::project(
            Principal::from_slice(&[9]),
            &mut s,
            &catalog(),
            NOW + model::REFUNDABLE_LEASE_MS,
            None,
        )
        .unwrap();
        assert_eq!(stale.source_status, SourceStatus::Unverifiable);
        addon.state = PlayState::Refunded;
        addon.checked_at_ms = NOW + MINUTE;
        apply(
            &mut s,
            &addon,
            &storage_kind(),
            Some(&r),
            None,
            &catalog(),
            addon.checked_at_ms,
        )
        .unwrap();
        model::prune(&mut s, addon.checked_at_ms).unwrap();
        assert!(s.addons.is_empty());
        let previous = Observation {
            transaction: addon.clone(),
            kind: storage_kind(),
            fingerprint: fingerprint(&addon),
            superseded_at: None,
        };
        addon.state = PlayState::Active;
        addon.revision += 1;
        assert!(
            validate_transaction(&addon, &config(), Some(&previous), addon.checked_at_ms).is_err()
        );
    }

    #[test]
    fn deferred_transfer_preserves_billing_anchor_and_only_moves_remaining_service() {
        let (mut subject, old, _) = initial();
        let mut transfer = old.clone();
        transfer.token_hash = Hash::new([8; 32]);
        transfer.linked_token_hash = Some(old.token_hash);
        transfer.purchased_at_ms = NOW;
        transfer.checked_at_ms = NOW;
        let (receipt, _) = apply(
            &mut subject,
            &transfer,
            &kind(),
            None,
            None,
            &catalog(),
            NOW,
        )
        .unwrap();
        assert!(receipt.delivered);
        assert_eq!(model::end(&subject.contracts[0]), NOW);
        let current = subject.active(NOW).unwrap();
        assert_eq!(current.starts_at_ms, NOW);
        assert_eq!(current.term_starts_at_ms, old.purchased_at_ms);
        assert_eq!(current.expires_at_ms, old.expires_at_ms.unwrap());
        assert_eq!(current.plan.plan_id, PlanId::Plus);
        // First renewal of the transferred token may move to the selected new SKU.
        let mut renewal = transfer.clone();
        renewal.order_id = Some("GPA.changed..0".into());
        renewal.product_id = "dmsg.max".into();
        renewal.purchased_at_ms = old.expires_at_ms.unwrap();
        renewal.term_starts_at_ms = Some(renewal.purchased_at_ms);
        renewal.checked_at_ms = renewal.purchased_at_ms;
        renewal.expires_at_ms = Some(renewal.purchased_at_ms + 365 * DAY);
        let max = PlayProductKind::Plan {
            plan: PlanId::Max,
            base_plan_id: "annual".into(),
        };
        apply(
            &mut subject,
            &renewal,
            &max,
            None,
            None,
            &catalog(),
            renewal.checked_at_ms,
        )
        .unwrap();
        assert_eq!(
            subject.active(renewal.checked_at_ms).unwrap().plan.plan_id,
            PlanId::Max
        );
    }

    #[test]
    fn delayed_storage_delivery_retains_paid_term_after_google_truncates_the_old_token() {
        let (mut subject, mut base, parent) = initial();
        let original_end = base.expires_at_ms.unwrap();
        let mut addon = storage(&base);
        base.expires_at_ms = Some(NOW + MINUTE);
        base.checked_at_ms = NOW + MINUTE;
        base.state = PlayState::Expired;
        base.replaced = true;
        let (parent, _) = apply(
            &mut subject,
            &base,
            &kind(),
            Some(&parent),
            None,
            &catalog(),
            base.checked_at_ms,
        )
        .unwrap();
        assert_eq!(parent.base_term_expires, Some(original_end));
        addon.checked_at_ms = NOW + 2 * MINUTE;
        apply(
            &mut subject,
            &addon,
            &storage_kind(),
            None,
            Some(&parent),
            &catalog(),
            addon.checked_at_ms,
        )
        .unwrap();
        assert_eq!(subject.addons[0].expires_at_ms, original_end);
    }
}
