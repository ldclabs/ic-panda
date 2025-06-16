use candid::Principal;
use ic_cose_types::{validate_principals, MILLISECONDS};
use std::collections::BTreeSet;

use crate::{is_controller, store, types};

#[ic_cdk::update(guard = "is_controller")]
fn admin_add_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    let mut args = args;
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

#[ic_cdk::update(guard = "is_controller")]
fn admin_add_canister(kind: types::CanisterKind, id: Principal) -> Result<(), String> {
    validate_admin_add_canister(kind, id)?;
    store::state::with_mut(|s| {
        match kind {
            types::CanisterKind::OssCluster => {
                s.ic_oss_cluster = Some(id);
            }
            types::CanisterKind::OssBucket => {
                s.ic_oss_buckets.push(id);
            }
        }
        Ok(())
    })
}

#[ic_cdk::update]
fn admin_create_channel(input: types::CreateChannelInput) -> Result<types::ChannelInfo, String> {
    input.validate()?;

    let caller = ic_cdk::api::msg_caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::state::is_manager(&caller)?;
    store::channel::create(caller, input, now_ms)
}

#[ic_cdk::update]
fn admin_topup_channel(input: types::ChannelTopupInput) -> Result<types::ChannelInfo, String> {
    let caller = ic_cdk::api::msg_caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::state::is_manager(&caller)?;
    store::channel::topup(input.payer, input.id, input.amount, now_ms)
}

#[ic_cdk::update]
fn validate_admin_add_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    Ok(())
}

#[ic_cdk::update]
fn validate2_admin_add_managers(args: BTreeSet<Principal>) -> Result<String, String> {
    validate_principals(&args)?;
    Ok("ok".to_string())
}

#[ic_cdk::update]
fn validate_admin_remove_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    Ok(())
}

#[ic_cdk::update]
fn validate2_admin_remove_managers(args: BTreeSet<Principal>) -> Result<String, String> {
    validate_principals(&args)?;
    Ok("ok".to_string())
}

#[ic_cdk::update]
fn validate_admin_add_canister(kind: types::CanisterKind, id: Principal) -> Result<String, String> {
    store::state::with(|s| {
        match kind {
            types::CanisterKind::OssCluster => {
                if s.ic_oss_cluster.is_some() {
                    Err("OSS cluster canister is already added".to_string())?;
                }
            }
            types::CanisterKind::OssBucket => {
                if s.ic_oss_buckets.contains(&id) {
                    Err("OSS bucket canister is already added".to_string())?;
                }
            }
        }
        Ok("ok".to_string())
    })
}
