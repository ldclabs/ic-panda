use crate::{icp_transfer_to, is_authenticated, is_controller, store, types, DAO_CANISTER};
use candid::{Nat, Principal};
use std::collections::BTreeSet;

#[ic_cdk::update(guard = "is_controller")]
async fn admin_collect_icp(amount: Nat) -> Result<(), String> {
    icp_transfer_to(DAO_CANISTER, amount)
        .await
        .map_err(|err| format!("failed to collect ICP, {}", err))?;
    Ok(())
}

// Set the managers.
#[ic_cdk::update(guard = "is_controller")]
fn admin_set_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    store::state::with_mut(|r| {
        r.managers = Some(args);
    });
    Ok(())
}

#[ic_cdk::update(guard = "is_authenticated")]
fn manager_update_airdrop_balance(airdrop_balance: u64) -> Result<(), String> {
    if !store::state::is_manager(&ic_cdk::caller()) {
        return Err("user is not a manager".to_string());
    }
    store::state::with_mut(|state| state.airdrop_balance = airdrop_balance);
    Ok(())
}

#[ic_cdk::update(guard = "is_authenticated")]
fn manager_add_notification(args: types::Notification) -> Result<(), String> {
    if !store::state::is_manager(&ic_cdk::caller()) {
        return Err("user is not a manager".to_string());
    }
    store::notification::add(args);
    Ok(())
}

#[ic_cdk::update(guard = "is_authenticated")]
fn manager_remove_notifications(ids: Vec<u8>) -> Result<(), String> {
    if !store::state::is_manager(&ic_cdk::caller()) {
        return Err("user is not a manager".to_string());
    }
    store::notification::remove(ids);
    Ok(())
}
