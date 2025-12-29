use candid::{CandidType, Principal};
use serde::Deserialize;
use std::{collections::BTreeSet, time::Duration};

use crate::{store, types};

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ChainArgs {
    Init(InitArgs),
    Upgrade(UpgradeArgs),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    name: String,
    managers: BTreeSet<Principal>,
    schnorr_key_name: String, // Use "dfx_test_key" for local replica and "test_key_1" for a testing key for testnet and mainnet
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct UpgradeArgs {
    name: Option<String>,
    managers: Option<BTreeSet<Principal>>,
    schnorr_key_name: Option<String>, // Use "dfx_test_key" for local replica and "test_key_1" for a testing key for testnet and mainnet
}

#[ic_cdk::init]
fn init(args: Option<ChainArgs>) {
    store::state::with_mut(|s| {
        s.price = types::Price {
            channel: 1000 * types::TOKEN_1,
            name_l1: 1_000_000 * types::TOKEN_1,
            name_l2: 200_000 * types::TOKEN_1,
            name_l3: 50_000 * types::TOKEN_1,
            name_l5: 20_000 * types::TOKEN_1,
            name_l7: 5000 * types::TOKEN_1,
        };
        s.schnorr_key_name = "dfx_test_key".to_string();
    });

    match args {
        None => {}
        Some(ChainArgs::Init(args)) => {
            store::state::with_mut(|s| {
                s.name = args.name;
                s.managers = args.managers;
                s.schnorr_key_name = args.schnorr_key_name;
            });
        }
        Some(ChainArgs::Upgrade(_)) => {
            ic_cdk::trap(
                "cannot initialize the canister with an Upgrade args. Please provide an Init args.",
            );
        }
    }

    ic_cdk_timers::set_timer(Duration::from_secs(0), store::state::try_init_public_key());
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
                if let Some(schnorr_key_name) = args.schnorr_key_name {
                    s.schnorr_key_name = schnorr_key_name;
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

    store::state::with_mut(|s| {
        if s.latest_usernames.is_empty() && !s.short_usernames.is_empty() {
            s.latest_usernames
                .extend(s.short_usernames.iter().take(20).cloned());
        }
    });
}
