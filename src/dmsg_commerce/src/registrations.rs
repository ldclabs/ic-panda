//! Governance-owned public registrations; product delivery remains a separate authority.
use crate::store;
use candid::Principal;
use dmsg_protocol::{canonical, digest, integration::*};
use dmsg_runtime::storage::{MapExt, Stored};
use dmsg_types::{integration::*, *};
use ic_stable_structures::{memory_manager::VirtualMemory, DefaultMemoryImpl, StableBTreeMap};
use std::cell::RefCell;

type Memory = VirtualMemory<DefaultMemoryImpl>;
const MAX_APPS: u64 = 64;
const MAX_PRODUCTS: u64 = 256;
thread_local! {
    static APPS: RefCell<StableBTreeMap<Vec<u8>, Stored<AppRegistration>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(8)));
    static PRODUCTS: RefCell<StableBTreeMap<Vec<u8>, Stored<ProductRegistration>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(9)));
}

fn app_key(id: &str) -> Vec<u8> {
    digest("dmsg/registration/app/v1", &id).to_vec()
}

fn product_key(id: &str) -> Vec<u8> {
    digest("dmsg/registration/product/v2", &id).to_vec()
}

fn governance(caller: Principal) -> Result<()> {
    ensure(caller == store::config().init.governance, Error::Forbidden)
}

fn check_app_update(old: Option<&AppRegistration>, next: &AppRegistration) -> Result<()> {
    validate_app(next)?;
    if let Some(old) = old {
        ensure(
            old.app_id == next.app_id
                && old.environment == next.environment
                && old.authentication_receiver == next.authentication_receiver
                && old.action_authority == next.action_authority,
            Error::IntegrityFailed,
        )?;
        ensure(
            old == next || old.config_version.checked_add(1) == Some(next.config_version),
            Error::VersionConflict,
        )
    } else {
        ensure(next.config_version == 1, Error::VersionConflict)
    }
}

fn check_product_update(
    old: Option<&ProductRegistration>,
    next: &ProductRegistration,
) -> Result<()> {
    validate_product(next)?;
    if let Some(old) = old {
        ensure(
            old.product_id == next.product_id
                && old.environment == next.environment
                && old.quote_authority == next.quote_authority
                && old.beneficiary_authority == next.beneficiary_authority
                && old.adapter == next.adapter
                && old.subject_schema == next.subject_schema
                && old.subject_size == next.subject_size,
            Error::IntegrityFailed,
        )?;
        ensure(
            old == next || old.config_version.checked_add(1) == Some(next.config_version),
            Error::VersionConflict,
        )
    } else {
        ensure(next.config_version == 1, Error::VersionConflict)
    }
}

#[ic_cdk::update]
fn register_integration_app(app: AppRegistration) -> Result<()> {
    governance(ic_cdk::api::msg_caller())?;
    ensure(
        app.environment == store::config().init.environment,
        Error::Forbidden,
    )?;
    let old = APPS.with_borrow(|t| t.load(app.app_id.as_bytes()));
    check_app_update(old.as_ref(), &app)?;
    ensure(
        old.is_some() || APPS.with_borrow(|t| t.len()) < MAX_APPS,
        Error::QuotaExceeded,
    )?;
    for id in &app.product_ids {
        PRODUCTS
            .with_borrow(|t| t.load(id.as_bytes()))
            .ok_or(Error::NotFound)?;
    }
    APPS.with_borrow_mut(|t| t.put(app.app_id.as_bytes(), &app));
    store::CERT.with_borrow_mut(|c| c.put(app_key(&app.app_id), &app));
    Ok(())
}

#[ic_cdk::update]
fn register_integration_product(product: ProductRegistration) -> Result<()> {
    governance(ic_cdk::api::msg_caller())?;
    ensure(
        product.environment == store::config().init.environment,
        Error::Forbidden,
    )?;
    let old = PRODUCTS.with_borrow(|t| t.load(product.product_id.as_bytes()));
    check_product_update(old.as_ref(), &product)?;
    ensure(
        old.is_some() || PRODUCTS.with_borrow(|t| t.len()) < MAX_PRODUCTS,
        Error::QuotaExceeded,
    )?;
    PRODUCTS.with_borrow_mut(|t| t.put(product.product_id.as_bytes(), &product));
    store::CERT.with_borrow_mut(|c| c.put(product_key(&product.product_id), &product));
    Ok(())
}

/// Replicated response used by the pinned user home during exact approval.
#[ic_cdk::update]
fn read_integration_configuration(
    app_id: String,
    product_id: Option<String>,
) -> Result<(AppRegistration, Option<ProductRegistration>)> {
    configuration(&app_id, product_id.as_deref())
}

pub(crate) fn configuration(
    app_id: &str,
    product_id: Option<&str>,
) -> Result<(AppRegistration, Option<ProductRegistration>)> {
    validate_identifier(app_id)?;
    let app = APPS
        .with_borrow(|t| t.load(app_id.as_bytes()))
        .ok_or(Error::NotFound)?;
    let product = if let Some(id) = product_id {
        validate_identifier(id)?;
        ensure(app.product_ids.iter().any(|p| p == id), Error::Forbidden)?;
        Some(
            PRODUCTS
                .with_borrow(|t| t.load(id.as_bytes()))
                .ok_or(Error::NotFound)?,
        )
    } else {
        None
    };
    Ok((app, product))
}

/// Public, authenticated discovery. A paused configuration remains discoverable.
#[ic_cdk::query]
fn integration_configuration_certificate(
    app_id: String,
    product_id: Option<String>,
) -> Result<CertifiedBatch> {
    configuration(&app_id, product_id.as_deref())?;
    let mut keys = vec![app_key(&app_id)];
    if let Some(id) = product_id {
        keys.push(product_key(&id));
    }
    store::CERT.with_borrow(|c| c.batch(ic_cdk::api::canister_self(), keys))
}

pub(crate) fn rebuild(c: &mut dmsg_runtime::Certification) {
    APPS.with_borrow(|t| {
        t.for_each(|_, app| {
            c.insert(app_key(&app.app_id), canonical(&app));
        })
    });
    PRODUCTS.with_borrow(|t| {
        t.for_each(|_, product| {
            c.insert(product_key(&product.product_id), canonical(&product));
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkout_model::fixture::base as fixtures;

    #[test]
    fn registrations_reject_identity_replacement_and_support_idempotent_pause() {
        let old = fixtures::app();
        check_app_update(None, &old).unwrap();
        check_app_update(Some(&old), &old).unwrap();
        let mut next = old.clone();
        next.paused = true;
        assert_eq!(
            check_app_update(Some(&old), &next),
            Err(Error::VersionConflict)
        );
        next.config_version += 1;
        check_app_update(Some(&old), &next).unwrap();
        next.authentication_receiver = fixtures::principal(99);
        assert_eq!(
            check_app_update(Some(&old), &next),
            Err(Error::IntegrityFailed)
        );
        let old = fixtures::product();
        let mut next = old.clone();
        next.config_version += 1;
        next.paused = true;
        check_product_update(Some(&old), &next).unwrap();
        next.subject_schema = "different-v1".into();
        assert_eq!(
            check_product_update(Some(&old), &next),
            Err(Error::IntegrityFailed)
        );
    }
}
