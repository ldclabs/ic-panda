use candid::{Nat, Principal};
use num_traits::cast::ToPrimitive;
use serde_bytes::ByteBuf;
use std::collections::BTreeSet;

mod ai;
mod api_admin;
mod api_init;
mod api_query;
mod api_update;
mod store;
mod types;

use ic_oss_types::file::*;

const MILLISECONDS: u64 = 1_000_000;

static ANONYMOUS: Principal = Principal::anonymous();

// "dwv6s-6aaaa-aaaaq-aacta-cai" ICPanda DAO canister id
static DAO_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 0, 0, 166, 1, 1]);

pub fn unwrap_trap<T, E: std::fmt::Debug>(res: Result<T, E>, msg: &str) -> T {
    match res {
        Ok(v) => v,
        Err(err) => ic_cdk::trap(&format!("{}, {:?}", msg, err)),
    }
}

fn nat_to_u64(nat: &Nat) -> u64 {
    nat.0.to_u64().unwrap_or(0)
}

fn is_controller() -> Result<(), String> {
    let caller = ic_cdk::caller();
    if caller == DAO_CANISTER || ic_cdk::api::is_controller(&caller) {
        Ok(())
    } else {
        Err("user is not a controller".to_string())
    }
}

fn is_controller_or_manager() -> Result<(), String> {
    let caller = ic_cdk::caller();
    if caller == DAO_CANISTER
        || ic_cdk::api::is_controller(&caller)
        || store::state::is_manager(&caller)
    {
        Ok(())
    } else {
        Err("user is not a controller or manager".to_string())
    }
}

fn is_authenticated() -> Result<(), String> {
    if ic_cdk::caller() == ANONYMOUS {
        Err("anonymous user is not allowed".to_string())
    } else {
        Ok(())
    }
}

ic_cdk::export_candid!();
