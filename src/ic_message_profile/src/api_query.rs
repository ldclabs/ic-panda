use candid::Principal;
use ic_cdk::api::management_canister::main::{
    canister_status, CanisterIdRecord, CanisterStatusResponse,
};
use ic_cose_types::format_error;

use crate::{is_authenticated, store, types};

#[ic_cdk::query]
fn get_state() -> Result<types::StateInfo, String> {
    Ok(store::state::with(|s| types::StateInfo {
        name: s.name.clone(),
        managers: s.managers.clone(),
        profiles_total: store::profile::profiles_total(),
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

#[ic_cdk::query(guard = "is_authenticated")]
fn get_profile(user: Option<Principal>) -> Result<types::ProfileInfo, String> {
    let caller = ic_cdk::caller();
    let user = user.unwrap_or(caller);
    store::profile::get(user, caller == user)
}
