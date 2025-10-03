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
    if let Some(CanisterArgs::Init(args)) = args {
        store::state::with_mut(|s| {
            s.allowed_origins = args.allowed_origins;
        });
    }

    store::state::init_http_certified_data();
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    store::state::save();
}

#[ic_cdk::post_upgrade]
fn post_upgrade(args: Option<CanisterArgs>) {
    store::state::load();

    match args {
        Some(CanisterArgs::Upgrade(args)) => {
            store::state::with_mut(|s| {
                if let Some(origins) = args.allowed_origins {
                    s.allowed_origins = origins;
                }
            });
        }
        Some(CanisterArgs::Init(_)) => {
            ic_cdk::trap(
                "cannot upgrade the canister with an Init args. Please provide an Upgrade args.",
            );
        }
        _ => {}
    }

    store::state::init_http_certified_data();
}
