use crate::{
    icp_transfer_to, is_authenticated, is_controller, store, token_balance_of, types, ANONYMOUS,
    DAO_CANISTER, ICP_1, ICP_CANISTER, SECOND, TRANS_FEE,
};
use base64::{engine::general_purpose, Engine};
use candid::{Nat, Principal};
use std::collections::BTreeSet;

#[ic_cdk::update(guard = "is_controller")]
async fn admin_collect_icp(amount: Nat) -> Result<(), String> {
    icp_transfer_to(DAO_CANISTER, amount)
        .await
        .map_err(|err| format!("failed to collect ICP, {}", err))?;
    Ok(())
}

#[ic_cdk::update]
async fn validate_admin_collect_icp(amount: Nat) -> Result<(), String> {
    if amount < ICP_1 {
        return Err("amount must be at least 1 ICP".to_string());
    }

    let balance = token_balance_of(ICP_CANISTER, ic_cdk::id())
        .await
        .unwrap_or(Nat::from(0u64));

    if amount + TRANS_FEE > balance {
        return Err(format!("insufficient ICP balance: {}", balance));
    }

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

#[ic_cdk::update]
fn validate_admin_set_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    if args.is_empty() {
        return Err("managers cannot be empty".to_string());
    }
    if args.contains(&ANONYMOUS) {
        return Err("anonymous user is not allowed".to_string());
    }
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
fn manager_update_airdrop_amount(airdrop_amount: u64) -> Result<(), String> {
    if !store::state::is_manager(&ic_cdk::caller()) {
        return Err("user is not a manager".to_string());
    }
    if airdrop_amount > 100 {
        return Err("airdrop amount should be less than 100 tokens".to_string());
    }

    store::state::with_mut(|state| state.airdrop_amount = Some(airdrop_amount));
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

#[ic_cdk::update(guard = "is_authenticated")]
fn manager_ban_users(ids: Vec<Principal>) -> Result<(), String> {
    if !store::state::is_manager(&ic_cdk::caller()) {
        return Err("user is not a manager".to_string());
    }
    store::airdrop::ban_users(ids)
}

#[ic_cdk::query(guard = "is_authenticated")]
fn manager_get_airdrop_key() -> Result<String, String> {
    if !store::state::is_manager(&ic_cdk::caller()) {
        return Err("user is not a manager".to_string());
    }
    Ok(general_purpose::URL_SAFE_NO_PAD.encode(*store::keys::AIRDROP_KEY))
}

#[ic_cdk::update(guard = "is_authenticated")]
fn manager_add_prize(args: types::AddPrizeInput) -> Result<String, String> {
    let caller = ic_cdk::caller();
    if !store::state::is_manager(&caller) {
        return Err("user is not a manager".to_string());
    }
    let now_sec = ic_cdk::api::time() / SECOND;
    if args.expire < 10 {
        return Err("expire should be at least 10 minutes".to_string());
    }
    if args.claimable == 0 {
        return Err("claimable should be at least 1 token".to_string());
    }
    if args.quantity == 0 {
        return Err("quantity should be at least 1".to_string());
    }
    if args.expire > 60 * 24 * 30 {
        return Err("expire should be less than 60*24*30".to_string());
    }
    if args.claimable > 100_000 {
        return Err("claimable should be less than 100_000".to_string());
    }
    if args.quantity > 10_000 {
        return Err("quantity should be less than 10_000".to_string());
    }

    match store::airdrop::state_of(&caller) {
        Some(store::AirdropState(code, _, _)) => {
            if code == 0 {
                Err("user is banned".to_string())
            } else {
                match store::prize::try_add(
                    code,
                    now_sec,
                    args.expire,
                    args.claimable,
                    args.quantity,
                ) {
                    Some(cryptogram) => Ok(cryptogram),
                    None => Err("failed to add prize".to_string()),
                }
            }
        }
        None => Err("you don't have lucky code".to_string()),
    }
}
