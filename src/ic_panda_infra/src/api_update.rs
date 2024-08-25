use candid::Principal;
use ic_cose_types::validate_principals;
use std::collections::BTreeSet;

use crate::{is_controller, is_controller_or_manager, store};

#[ic_cdk::update]
fn ipc_setup(subject: Principal) -> Result<(), String> {
    Err("not implemented".to_string())
}
