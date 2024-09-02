use candid::Principal;
use ic_cose_types::{validate_principals, MILLISECONDS};
use serde_bytes::ByteArray;
use std::collections::BTreeSet;

use crate::{is_controller, store};

#[ic_cdk::update(guard = "is_controller")]
fn admin_add_managers(mut args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    store::state::with_mut(|r| {
        r.managers.append(&mut args);
        Ok(())
    })
}

#[ic_cdk::update(guard = "is_controller")]
fn admin_remove_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    store::state::with_mut(|r| {
        r.managers.retain(|p| !args.contains(p));
        Ok(())
    })
}

#[ic_cdk::update]
fn admin_upsert_profile(user: Principal, channel: Option<(Principal, u64)>) -> Result<(), String> {
    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::state::is_manager(&caller)?;
    store::profile::upsert(user, now_ms, channel)
}

#[ic_cdk::update]
fn admin_update_profile_ecdh_pub(user: Principal, ecdh_pub: ByteArray<32>) -> Result<(), String> {
    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::state::is_manager(&caller)?;
    store::profile::admin_update_profile_ecdh_pub(user, now_ms, ecdh_pub)
}

#[ic_cdk::update]
fn validate_admin_add_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    Ok(())
}

#[ic_cdk::update]
fn validate_admin_remove_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    Ok(())
}
