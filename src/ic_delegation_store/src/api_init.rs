use candid::CandidType;
use serde::Deserialize;

use crate::store;

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum CanisterArgs {
    Init(InitArgs),
    Upgrade(UpgradeArgs),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    allowed_origins: Vec<String>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct UpgradeArgs {
    allowed_origins: Option<Vec<String>>,
}

#[ic_cdk::init]
fn init(args: Option<CanisterArgs>) {
    match args {
        Some(CanisterArgs::Init(args)) => set_allowed_origins(args.allowed_origins),
        Some(CanisterArgs::Upgrade(_)) => {
            ic_cdk::trap("cannot initialize the canister with Upgrade args")
        }
        None => {}
    }

    store::state::init_certified_data();
}

#[ic_cdk::post_upgrade]
fn post_upgrade(args: Option<CanisterArgs>) {
    match args {
        Some(CanisterArgs::Upgrade(args)) => {
            if let Some(origins) = args.allowed_origins {
                set_allowed_origins(origins);
            } else {
                normalize_allowed_origins();
            }
        }
        Some(CanisterArgs::Init(_)) => {
            ic_cdk::trap("cannot upgrade the canister with Init args");
        }
        None => normalize_allowed_origins(),
    }

    store::state::init_certified_data();
}

fn set_allowed_origins(origins: Vec<String>) {
    store::config::set_allowed_origins(origins)
        .unwrap_or_else(|err| ic_cdk::trap(format!("invalid allowed_origins: {err}")));
}

fn normalize_allowed_origins() {
    store::config::normalize_allowed_origins()
        .unwrap_or_else(|err| ic_cdk::trap(format!("invalid persisted allowed_origins: {err}")));
}
