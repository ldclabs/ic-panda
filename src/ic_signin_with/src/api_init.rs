use candid::{CandidType, Principal};
use serde::Deserialize;
use std::time::Duration;

use crate::store;

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum CanisterArgs {
    Init(InitArgs),
    Upgrade(UpgradeArgs),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    session_expires_in_ms: u64,
    governance_canister: Option<Principal>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct UpgradeArgs {
    session_expires_in_ms: Option<u64>,
    governance_canister: Option<Principal>,
}

#[ic_cdk::init]
fn init(args: Option<CanisterArgs>) {
    if let Some(CanisterArgs::Init(args)) = args {
        store::state::mutate(|s| {
            s.session_expires_in_ms = args.session_expires_in_ms;
            s.governance_canister = args.governance_canister;
        });
    } else if let Some(CanisterArgs::Upgrade(_)) = args {
        ic_cdk::trap("cannot init the canister with an Upgrade args. Please provide an Init args.");
    } else {
        store::state::mutate(|s| {
            s.session_expires_in_ms = 1000 * 3600 * 24 * 30; // default to 30 days
        });
    }

    store::state::init_certified_data();
    ic_cdk_timers::set_timer(Duration::ZERO, store::state::initialize_nonce());
}

#[ic_cdk::post_upgrade]
fn post_upgrade(args: Option<CanisterArgs>) {
    match args {
        Some(CanisterArgs::Upgrade(args)) => {
            if args.session_expires_in_ms.is_some() || args.governance_canister.is_some() {
                store::state::mutate(|s| {
                    if let Some(session_expires_in_ms) = args.session_expires_in_ms {
                        s.session_expires_in_ms = session_expires_in_ms;
                    }
                    if let Some(governance_canister) = args.governance_canister {
                        s.governance_canister = Some(governance_canister);
                    }
                });
            }
        }
        Some(CanisterArgs::Init(_)) => {
            ic_cdk::trap(
                "cannot upgrade the canister with an Init args. Please provide an Upgrade args.",
            );
        }
        _ => {}
    }

    store::state::init_certified_data();
}
