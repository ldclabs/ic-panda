use candid::Principal;
use ic_cdk::api::management_canister::main::{
    canister_status, CanisterIdRecord, CanisterStatusResponse,
};
use ic_cose_types::format_error;
use ic_message_types::profile::UserInfo;
use std::collections::BTreeSet;

use crate::{is_authenticated, store, types};

#[ic_cdk::query]
fn get_state() -> Result<types::StateInfo, String> {
    Ok(store::state::with(|s| types::StateInfo {
        name: s.name.clone(),
        managers: s.managers.clone(),
        cose_canisters: s.cose_canisters.clone(),
        profile_canisters: s.profile_canisters.clone(),
        channel_canisters: s.channel_canisters.clone(),
        price: s.price.clone(),
        names_total: store::user::names_total(),
        users_total: store::user::users_total(),
        incoming_total: s.incoming_total,
        transfer_out_total: s.transfer_out_total,
    }))
}

#[ic_cdk::query]
async fn get_canister_status() -> Result<CanisterStatusResponse, String> {
    store::state::is_manager(&ic_cdk::caller())?;

    let (res,) = canister_status(CanisterIdRecord {
        canister_id: ic_cdk::id(),
    })
    .await
    .map_err(format_error)?;
    Ok(res)
}

#[ic_cdk::query]
fn search_username(prefix: String) -> Result<Vec<String>, String> {
    Ok(store::user::search_username(prefix))
}

#[ic_cdk::query]
fn get_by_username(username: String) -> Result<UserInfo, String> {
    store::user::get_by_username(username)
}

#[ic_cdk::query]
fn get_user(user: Option<Principal>) -> Result<UserInfo, String> {
    store::user::get(user.unwrap_or(ic_cdk::caller()))
}

#[ic_cdk::query(guard = "is_authenticated")]
fn batch_get_users(ids: BTreeSet<Principal>) -> Result<Vec<UserInfo>, String> {
    Ok(store::user::batch_get(ids))
}
