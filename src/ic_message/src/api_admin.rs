use candid::{Nat, Principal};
use ic_cose_types::validate_principals;
use icrc_ledger_types::icrc1::account::Account;
use num_traits::cast::ToPrimitive;
use std::collections::BTreeSet;

use crate::{is_controller, store, token_transfer_to, types};

#[ic_cdk::update(guard = "is_controller")]
fn admin_add_managers(mut args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    store::state::with_mut(|s| {
        s.managers.append(&mut args);
        Ok(())
    })
}

#[ic_cdk::update(guard = "is_controller")]
fn admin_remove_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    store::state::with_mut(|s| {
        s.managers.retain(|p| !args.contains(p));
        Ok(())
    })
}

#[ic_cdk::update(guard = "is_controller")]
fn admin_add_canister(kind: types::CanisterKind, id: Principal) -> Result<(), String> {
    validate_admin_add_canister(kind, id)?;
    store::state::with_mut(|s| {
        match kind {
            types::CanisterKind::Cose => {
                s.cose_canisters.push(id);
            }
            types::CanisterKind::Profile => {
                s.profile_canisters.push(id);
            }
            types::CanisterKind::Channel => {
                s.channel_canisters.push(id);
            }
        }
        Ok(())
    })
}

#[ic_cdk::update(guard = "is_controller")]
fn admin_update_price(args: types::UpdatePriceInput) -> Result<(), String> {
    validate_admin_update_price(args.clone())?;
    store::state::with_mut(|s| {
        if let Some(price) = args.channel {
            s.price.channel = price;
        }
        if let Some(price) = args.name_l7 {
            s.price.name_l7 = price;
        }
        if let Some(price) = args.name_l5 {
            s.price.name_l5 = price;
        }
        if let Some(price) = args.name_l3 {
            s.price.name_l3 = price;
        }
        if let Some(price) = args.name_l2 {
            s.price.name_l2 = price;
        }
        if let Some(price) = args.name_l1 {
            s.price.name_l1 = price;
        }
        Ok(())
    })
}

#[ic_cdk::update(guard = "is_controller")]
async fn admin_collect_token(user: Account, amount: Nat) -> Result<(), String> {
    let amount64 = amount.0.to_u64().unwrap_or_default();
    token_transfer_to(user, amount, "COLLECT".to_string())
        .await
        .map_err(|err| format!("failed to collect token, {}", err))?;

    store::state::with_mut(|s| {
        s.transfer_out_total += amount64 as u128;
    });
    Ok(())
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
fn validate_admin_add_canister(kind: types::CanisterKind, id: Principal) -> Result<(), String> {
    store::state::with(|s| {
        match kind {
            types::CanisterKind::Cose => {
                if s.cose_canisters.contains(&id) {
                    Err("Cose canister is already added".to_string())?;
                }
            }
            types::CanisterKind::Profile => {
                if s.profile_canisters.contains(&id) {
                    Err("Profile canister is already added".to_string())?;
                }
            }
            types::CanisterKind::Channel => {
                if s.channel_canisters.contains(&id) || s.matured_channel_canisters.contains(&id) {
                    Err("Channel canister is already added".to_string())?;
                }
            }
        }
        Ok(())
    })
}

#[ic_cdk::update]
fn validate2_admin_add_canister(
    kind: types::CanisterKind,
    id: Principal,
) -> Result<String, String> {
    validate_admin_add_canister(kind, id)?;
    Ok("ok".to_string())
}

#[ic_cdk::update]
fn validate_admin_update_price(args: types::UpdatePriceInput) -> Result<(), String> {
    args.validate()?;
    store::state::with(|s| {
        if let Some(price) = args.name_l5 {
            if price <= args.name_l7.unwrap_or(s.price.name_l7) {
                Err("name_l5 must be greater than name_l7".to_string())?;
            }
        }
        if let Some(price) = args.name_l3 {
            if price <= args.name_l5.unwrap_or(s.price.name_l5) {
                Err("name_l3 must be greater than name_l5".to_string())?;
            }
        }
        if let Some(price) = args.name_l2 {
            if price <= args.name_l3.unwrap_or(s.price.name_l3) {
                Err("name_l2 must be greater than name_l3".to_string())?;
            }
        }
        if let Some(price) = args.name_l1 {
            if price <= args.name_l2.unwrap_or(s.price.name_l2) {
                Err("name_l1 must be greater than name_l2".to_string())?;
            }
        }
        Ok::<(), String>(())
    })?;
    Ok(())
}
#[ic_cdk::update]
fn validate2_admin_update_price(args: types::UpdatePriceInput) -> Result<String, String> {
    validate_admin_update_price(args)?;
    Ok("ok".to_string())
}

#[ic_cdk::update]
fn validate_admin_collect_token(_user: Account, amount: Nat) -> Result<(), String> {
    if amount < types::TOKEN_1 {
        Err("amount must be at least 1 token".to_string())?;
    }

    Ok(())
}

#[ic_cdk::update]
fn validate2_admin_collect_token(user: Account, amount: Nat) -> Result<String, String> {
    validate_admin_collect_token(user, amount)?;
    Ok("ok".to_string())
}
