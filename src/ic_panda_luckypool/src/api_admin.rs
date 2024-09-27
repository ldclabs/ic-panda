use base64::{engine::general_purpose, Engine};
use candid::{Nat, Principal};
use lib_panda::{bytes32_from_base64, Cryptogram};
use std::collections::BTreeSet;

use crate::{
    icp_transfer_to, is_authenticated, is_controller, store, token_balance_of, types, ANONYMOUS,
    DAO_CANISTER, ICP_1, ICP_CANISTER, SECOND, TOKEN_1, TRANS_FEE,
};

#[ic_cdk::update(guard = "is_controller")]
async fn admin_collect_icp(amount: Nat) -> Result<(), String> {
    icp_transfer_to(DAO_CANISTER, amount, "COLLECT".to_string())
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

#[ic_cdk::update]
async fn validate2_admin_collect_icp(amount: Nat) -> Result<String, String> {
    validate_admin_collect_icp(amount).await?;
    Ok("ok".to_string())
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

#[ic_cdk::update]
fn validate2_admin_set_managers(args: BTreeSet<Principal>) -> Result<String, String> {
    validate_admin_set_managers(args)?;
    Ok("ok".to_string())
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
fn manager_update_prize_subsidy(subsidy: Option<store::SysPrizeSubsidy>) -> Result<(), String> {
    if !store::state::is_manager(&ic_cdk::caller()) {
        return Err("user is not a manager".to_string());
    }
    if let Some(ref subsidy) = subsidy {
        if subsidy.0 > 100 * TOKEN_1 {
            return Err("prize creating fee should be less than 100 tokens".to_string());
        }
        if subsidy.1 < 100 {
            return Err("min quantity for subsidy should be at least 100".to_string());
        }
        if subsidy.2 < 1000 {
            return Err("min total amount tokens for subsidy should be at least 1000".to_string());
        }
        if subsidy.3 > 50 {
            return Err("subsidy ratio should be less than 50".to_string());
        }
        if subsidy.4 > 10000 {
            return Err("max subsidy tokens should be less than 10,000 tokens".to_string());
        }
        if subsidy.5 > 1000 {
            return Err("max subsidy amount should be less than 1000".to_string());
        }
    }

    store::state::with_mut(|state| state.prize_subsidy = subsidy);
    Ok(())
}

#[ic_cdk::update(guard = "is_authenticated")]
fn manager_add_prize(args: types::AddPrizeInput) -> Result<String, String> {
    args.validate()?;
    Err("deprecated".to_string())
}

#[ic_cdk::update(guard = "is_authenticated")]
fn manager_add_prize_v2(args: types::AddPrizeInputV2) -> Result<String, String> {
    args.validate()?;
    let caller = ic_cdk::caller();
    if !store::state::is_manager(&caller) {
        return Err("user is not a manager".to_string());
    }
    let _ =
        store::state::with(|r| r.prize_subsidy.clone()).ok_or("can not add prize currently.")?;
    let store::AirdropState(caller_code, _, _) = store::airdrop::state_of(&caller)
        .ok_or("you don't have lucky code to add prize".to_string())?;
    if caller_code == 0 {
        return Err("user is banned".to_string());
    }
    let now_sec = ic_cdk::api::time() / SECOND;
    let prize = store::Prize(
        caller_code,
        (now_sec / 60) as u32,
        args.expire,
        args.total_amount,
        args.quantity,
    );
    let prize_info = store::PrizeInfo(
        args.kind.unwrap_or_default(),
        0,
        args.total_amount,
        0,
        0,
        0,
        args.memo,
    );
    if !store::prize::add(prize.clone(), prize_info.clone()) {
        return Err("failed to add prize".to_string());
    }
    store::state::with_mut(|r| {
        r.total_prizes_count = Some(r.total_prizes_count.unwrap_or_default().saturating_add(1));
    });

    let code = prize.encode(&(*store::keys::PRIZE_KEY), args.recipient);
    Ok(code)
}

#[ic_cdk::update(guard = "is_authenticated")]
fn manager_set_challenge_pub_key(key: String) -> Result<(), String> {
    if !store::state::is_manager(&ic_cdk::caller()) {
        return Err("user is not a manager".to_string());
    }
    let key = bytes32_from_base64(&key)?;
    store::keys::set_challenge_pub_key(key);
    Ok(())
}
