use candid::{utils::ArgumentEncoder, Principal};
use ic_cdk::management_canister::CanisterStatusResult;
use ic_cose_types::ANONYMOUS;
use std::collections::BTreeSet;

mod api_admin;
mod api_init;
mod api_query;
mod api_update;
mod store;
mod types;

use crate::api_init::ChainArgs;

// "dwv6s-6aaaa-aaaaq-aacta-cai" ICPanda DAO canister id
static DAO_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 0, 0, 166, 1, 1]);
// "ql553-iqaaa-aaaap-anuyq-cai" dMsg minter canister id
static MINTER_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 1, 224, 109, 49, 1, 1]);

fn is_controller() -> Result<(), String> {
    let caller = ic_cdk::api::msg_caller();
    if caller == DAO_CANISTER || ic_cdk::api::is_controller(&caller) {
        Ok(())
    } else {
        Err("user is not a controller".to_string())
    }
}

fn is_authenticated() -> Result<(), String> {
    if ic_cdk::api::msg_caller() == ANONYMOUS {
        Err("anonymous user is not allowed".to_string())
    } else {
        Ok(())
    }
}

async fn call<In, Out>(id: Principal, method: &str, args: In, cycles: u128) -> Result<Out, String>
where
    In: ArgumentEncoder + Send,
    Out: candid::CandidType + for<'a> candid::Deserialize<'a>,
{
    let res = ic_cdk::call::Call::bounded_wait(id, method)
        .with_args(&args)
        .with_cycles(cycles)
        .await
        .map_err(|err| format!("failed to call {} on {:?}, error: {:?}", method, &id, err))?;
    res.candid().map_err(|err| {
        format!(
            "failed to decode response from {} on {:?}, error: {:?}",
            method, &id, err
        )
    })
}

ic_cdk::export_candid!();
