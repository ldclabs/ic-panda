use candid::{utils::ArgumentEncoder, Principal};
use serde_bytes::ByteBuf;
use std::collections::BTreeSet;

mod api;
mod api_admin;
mod api_init;
mod store;
mod types;

use api_init::ChainArgs;
use types::{Delegator, NameAccount, SignInResponse, SignedDelegation};

// "nscli-qiaaa-aaaaj-qa4pa-cai" dMsg minter canister id
static NAMECHAIN_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 1, 48, 7, 30, 1, 1]);
// "dwv6s-6aaaa-aaaaq-aacta-cai" ICPanda DAO canister id
static DAO_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 0, 0, 166, 1, 1]);

fn is_controller() -> Result<(), String> {
    let caller = ic_cdk::api::msg_caller();
    if caller == DAO_CANISTER || ic_cdk::api::is_controller(&caller) {
        Ok(())
    } else {
        Err("caller is not a controller".to_string())
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
