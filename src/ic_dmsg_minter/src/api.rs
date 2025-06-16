use candid::{CandidType, Principal};
use serde::Deserialize;
use std::collections::BTreeSet;

use crate::store;

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum MinterArgs {
    Init(InitArgs),
    Upgrade(UpgradeArgs),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    preparers: BTreeSet<Principal>,
    committers: BTreeSet<Principal>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct UpgradeArgs {
    preparers: Option<BTreeSet<Principal>>,
    committers: Option<BTreeSet<Principal>>,
}

#[ic_cdk::init]
fn init(args: Option<MinterArgs>) {
    match args {
        None => {}
        Some(MinterArgs::Init(args)) => {
            store::state::with_mut(|s| {
                s.preparers = args.preparers;
                s.committers = args.committers;
            });
        }
        Some(MinterArgs::Upgrade(_)) => {
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
fn post_upgrade(args: Option<MinterArgs>) {
    store::state::load();

    match args {
        Some(MinterArgs::Upgrade(args)) => {
            store::state::with_mut(|s| {
                if let Some(mut preparers) = args.preparers {
                    s.preparers.append(&mut preparers);
                }
                if let Some(mut committers) = args.committers {
                    s.committers.append(&mut committers);
                }
            });
        }
        Some(MinterArgs::Init(_)) => {
            ic_cdk::trap(
                "cannot upgrade the canister with an Init args. Please provide an Upgrade args.",
            );
        }
        _ => {}
    }
}

#[ic_cdk::query]
fn get_state() -> store::State {
    store::state::with(|s| s.clone())
}

#[ic_cdk::update]
fn try_prepare(miner: Principal, payer: Principal) -> bool {
    if store::state::with(|s| !s.preparers.contains(&ic_cdk::api::msg_caller())) {
        ic_cdk::trap("unauthorized");
    }

    store::minter::try_prepare(store::Linker(miner, payer))
}

#[ic_cdk::update]
async fn try_commit(miner: Principal, payer: Principal) -> Option<u64> {
    if store::state::with(|s| !s.committers.contains(&ic_cdk::api::msg_caller())) {
        ic_cdk::trap("unauthorized");
    }

    let now_ms = ic_cdk::api::time() / 1_000_000;
    store::minter::try_commit(store::Linker(miner, payer), now_ms).await
}

#[ic_cdk::query]
async fn get_block(height: u64) -> Option<store::LinkLog> {
    store::minter::get_log(height)
}

#[ic_cdk::query]
async fn list_blocks(prev: Option<u64>, take: Option<usize>) -> Vec<store::LinkLog> {
    let take = take.unwrap_or(10).min(100);
    store::minter::list_logs(prev, take)
}
