use candid::Principal;
use ic_cdk::management_canister::{canister_status, CanisterStatusArgs, CanisterStatusResult};
use ic_cose_types::format_error;

use crate::{store, types};

#[ic_cdk::query]
fn get_state() -> Result<types::StateInfo, String> {
    Ok(store::state::with(|s| types::StateInfo {
        name: s.name.clone(),
        managers: s.managers.clone(),
        ic_oss_cluster: s.ic_oss_cluster,
        ic_oss_buckets: s.ic_oss_buckets.clone(),
        profiles_total: store::profile::profiles_total(),
    }))
}

#[ic_cdk::query]
async fn get_canister_status() -> Result<CanisterStatusResult, String> {
    store::state::is_manager(&ic_cdk::api::msg_caller())?;

    let res = canister_status(&CanisterStatusArgs {
        canister_id: ic_cdk::api::canister_self(),
    })
    .await
    .map_err(format_error)?;
    Ok(res)
}

#[ic_cdk::query]
fn get_profile(user: Option<Principal>) -> Result<types::ProfileInfo, String> {
    let caller = ic_cdk::api::msg_caller();
    let user = user.unwrap_or(caller);
    store::profile::get(user, caller == user)
}
