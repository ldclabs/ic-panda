use candid::Principal;
use std::collections::BTreeSet;

use crate::{api::normalize_name, is_controller, store};

fn validate_delegators(delegators: &BTreeSet<Principal>) -> Result<(), String> {
    if delegators.is_empty() {
        return Err("delegators is empty".to_string());
    }
    if delegators.len() > store::MAX_DELEGATIONS {
        return Err(format!(
            "delegators exceed the limit {}",
            store::MAX_DELEGATIONS
        ));
    }
    if delegators.contains(&Principal::anonymous()) {
        return Err("anonymous delegator is not allowed".to_string());
    }
    Ok(())
}

#[ic_cdk::update(guard = "is_controller")]
fn admin_reset_name(name: String, delegators: BTreeSet<Principal>) -> Result<(), String> {
    let legacy_input = candid::encode_args((&name, &delegators)).map_err(|e| e.to_string())?;
    crate::legacy::business(
        "admin_reset_name",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| {
            let name = normalize_name(name)?;
            validate_delegators(&delegators)?;
            store::state::reset_delegators(&name, delegators)
        },
    )
}

#[ic_cdk::update]
fn validate_admin_reset_name(
    name: String,
    delegators: BTreeSet<Principal>,
) -> Result<String, String> {
    normalize_name(name)?;
    validate_delegators(&delegators)?;

    Ok("ok".to_string())
}
