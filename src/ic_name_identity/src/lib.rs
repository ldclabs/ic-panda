use candid::Principal;
use ic_auth_types::{SignInResponse, SignedDelegation};
use serde_bytes::ByteBuf;
use std::collections::BTreeSet;

mod api;
mod api_admin;
mod api_init;
mod store;
mod types;

use api_init::ChainArgs;
use types::{Delegator, NameAccount};

// "nscli-qiaaa-aaaaj-qa4pa-cai" ICPanda Message canister id
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

ic_cdk::export_candid!();
