use candid::Principal;
use ic_cose_types::ANONYMOUS;

use crate::{store, store::StateInfo};

#[ic_cdk::query]
fn info() -> Result<StateInfo, String> {
    Ok(store::state::info())
}

#[ic_cdk::query]
fn evm_address() -> Result<String, String> {
    let caller = msg_caller()?;
    let addr = store::state::evm_address(&caller);
    Ok(addr.to_string())
}

fn msg_caller() -> Result<Principal, String> {
    let caller = ic_cdk::api::msg_caller();
    if caller == ANONYMOUS {
        Err("anonymous user is not allowed".to_string())
    } else {
        Ok(caller)
    }
}
