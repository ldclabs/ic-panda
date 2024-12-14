use candid::Principal;
use std::collections::BTreeSet;

use crate::{is_controller, store};

#[ic_cdk::update(guard = "is_controller")]
fn admin_reset_name(name: String, delegators: BTreeSet<Principal>) -> Result<(), String> {
    let name = name.to_ascii_lowercase();
    store::state::reset_delegators(&name, delegators)
}

#[ic_cdk::update]
fn validate_admin_reset_name(
    name: String,
    delegators: BTreeSet<Principal>,
) -> Result<String, String> {
    if name.is_empty() {
        return Err("name is empty".to_string());
    }
    if delegators.is_empty() {
        return Err("delegators is empty".to_string());
    }

    Ok("ok".to_string())
}
