use candid::Principal;
use ic_cose_types::validate_principals;
use std::collections::BTreeSet;

use crate::{is_controller, store, types};

#[ic_cdk::update(guard = "is_controller")]
fn admin_add_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    let legacy_input = candid::encode_args((&args,)).map_err(|e| e.to_string())?;
    crate::legacy::business(
        "admin_add_managers",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| {
            validate_principals(&args)?;
            let mut args = args;
            store::state::with_mut(|r| {
                r.managers.append(&mut args);
                Ok(())
            })
        },
    )
}

#[ic_cdk::update(guard = "is_controller")]
fn admin_remove_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    let legacy_input = candid::encode_args((&args,)).map_err(|e| e.to_string())?;
    crate::legacy::business(
        "admin_remove_managers",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| {
            validate_principals(&args)?;
            store::state::with_mut(|r| {
                r.managers.retain(|p| !args.contains(p));
                Ok(())
            })
        },
    )
}

#[ic_cdk::update(guard = "is_controller")]
fn admin_add_canister(kind: types::CanisterKind, id: Principal) -> Result<(), String> {
    let legacy_input = candid::encode_args((&kind, &id)).map_err(|e| e.to_string())?;
    crate::legacy::business(
        "admin_add_canister",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| {
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
        },
    )
}

#[ic_cdk::update]
fn admin_create_channel(input: types::CreateChannelInput) -> Result<types::ChannelInfo, String> {
    let legacy_input = candid::encode_args((&input,)).map_err(|e| e.to_string())?;
    crate::legacy::business(
        "admin_create_channel",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| {
            input.validate()?;

            let caller = _legacy_caller;
            let now_ms = _legacy_now_ms;
            store::state::is_manager(&caller)?;
            store::channel::create(caller, input, now_ms)
        },
    )
}

#[ic_cdk::update]
fn admin_topup_channel(input: types::ChannelTopupInput) -> Result<types::ChannelInfo, String> {
    let legacy_input = candid::encode_args((&input,)).map_err(|e| e.to_string())?;
    crate::legacy::business(
        "admin_topup_channel",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| {
            let caller = _legacy_caller;
            let now_ms = _legacy_now_ms;
            store::state::is_manager(&caller)?;
            store::channel::topup(input.payer, input.id, input.amount, now_ms)
        },
    )
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
