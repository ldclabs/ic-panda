use candid::{CandidType, Principal};
use serde::Deserialize;

use crate::{
    api_admin::validate_config,
    store::{self, State},
};

const DEFAULT_SESSION_EXPIRES_IN_MS: u64 = 1000 * 3600 * 24 * 30;

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum CanisterArgs {
    /// The whole configuration, so a reinstall serves sign-ins at once.
    Init(State),
    Upgrade(UpgradeArgs),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct UpgradeArgs {
    session_expires_in_ms: Option<u64>,
    governance_canister: Option<Principal>,
}

#[ic_cdk::init]
fn init(args: Option<CanisterArgs>) {
    match args {
        Some(CanisterArgs::Init(state)) => {
            if let Err(err) = validate_config(&state) {
                ic_cdk::trap(format!("invalid Init args: {err}"));
            }
            store::state::mutate(|s| *s = state);
        }
        Some(CanisterArgs::Upgrade(_)) => {
            ic_cdk::trap(
                "cannot init the canister with an Upgrade args. Please provide an Init args.",
            );
        }
        None => {
            store::state::mutate(|s| s.session_expires_in_ms = DEFAULT_SESSION_EXPIRES_IN_MS);
        }
    }
}

#[ic_cdk::post_upgrade]
fn post_upgrade(args: Option<CanisterArgs>) {
    store::refuse_memory_manager_layout();
    match args {
        Some(CanisterArgs::Upgrade(args)) => {
            store::state::mutate(|s| {
                if let Some(session_expires_in_ms) = args.session_expires_in_ms {
                    s.session_expires_in_ms = session_expires_in_ms;
                }
                if let Some(governance_canister) = args.governance_canister {
                    s.governance_canister = Some(governance_canister);
                }
            });
        }
        Some(CanisterArgs::Init(_)) => {
            ic_cdk::trap(
                "cannot upgrade the canister with an Init args. Please provide an Upgrade args.",
            );
        }
        None => {}
    }
}
