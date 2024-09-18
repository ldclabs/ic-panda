use candid::{CandidType, Principal};
use serde::Deserialize;
use std::collections::BTreeSet;

use crate::store;

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ChainArgs {
    Init(InitArgs),
    Upgrade(UpgradeArgs),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    name: String,
    managers: BTreeSet<Principal>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct UpgradeArgs {
    name: Option<String>,
    managers: Option<BTreeSet<Principal>>,
}

#[ic_cdk::init]
fn init(args: Option<ChainArgs>) {
    match args {
        None => {}
        Some(ChainArgs::Init(args)) => {
            store::state::with_mut(|s| {
                s.name = args.name;
                s.managers = args.managers;
            });
        }
        Some(ChainArgs::Upgrade(_)) => {
            ic_cdk::trap(
                "cannot initialize the canister with an Upgrade args. Please provide an Init args.",
            );
        }
    }
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    store::state::save();
}

#[ic_cdk::post_upgrade]
fn post_upgrade(args: Option<ChainArgs>) {
    store::state::load();

    match args {
        Some(ChainArgs::Upgrade(args)) => {
            store::state::with_mut(|s| {
                if let Some(name) = args.name {
                    s.name = name;
                }
                if let Some(managers) = args.managers {
                    s.managers = managers;
                }
            });
        }
        Some(ChainArgs::Init(_)) => {
            ic_cdk::trap(
                "cannot upgrade the canister with an Init args. Please provide an Upgrade args.",
            );
        }
        _ => {}
    }
}
