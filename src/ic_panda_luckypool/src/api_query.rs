use crate::{nat_to_u64, store, types, utils, AIRDROP_AMOUNT, ANONYMOUS, TOKEN_1};
use candid::{Nat, Principal};

#[ic_cdk::query]
fn api_version() -> u16 {
    1
}

#[ic_cdk::query]
fn whoami() -> Result<Principal, ()> {
    Ok(ic_cdk::caller())
}

#[ic_cdk::query]
fn state() -> Result<store::State, ()> {
    Ok(store::state::with(|r| r.clone()))
}

#[ic_cdk::query]
async fn airdrop_state_of(owner: Option<Principal>) -> Result<types::AirdropStateOutput, ()> {
    let owner = owner.unwrap_or(ic_cdk::caller());
    if owner == ANONYMOUS {
        return Ok(types::AirdropStateOutput {
            lucky_code: None,
            claimed: Nat::from(0u64),
            claimable: Nat::from(AIRDROP_AMOUNT * TOKEN_1),
        });
    }

    match store::airdrop::state_of(&owner) {
        Some(store::AirdropState(code, claimed, claimable)) => Ok(types::AirdropStateOutput {
            lucky_code: Some(utils::luckycode_to_string(code)),
            claimed: Nat::from(claimed),
            claimable: Nat::from(claimable),
        }),
        None => Ok(types::AirdropStateOutput {
            lucky_code: None,
            claimed: Nat::from(0u64),
            claimable: Nat::from(AIRDROP_AMOUNT * TOKEN_1),
        }),
    }
}

#[ic_cdk::query]
async fn airdrop_logs(prev: Option<Nat>, take: Option<Nat>) -> Vec<types::AirdropLog> {
    let prev = prev.as_ref().map(nat_to_u64);
    let take = take.as_ref().map(nat_to_u64).unwrap_or(10).min(100) as usize;
    store::airdrop::logs(prev, take)
}

#[ic_cdk::query]
async fn luckydraw_logs(prev: Option<Nat>, take: Option<Nat>) -> Vec<types::LuckyDrawLog> {
    let prev = prev.as_ref().map(nat_to_u64);
    let take = take.as_ref().map(nat_to_u64).unwrap_or(10).min(100) as usize;
    store::luckydraw::logs(prev, take, None)
}

#[ic_cdk::query]
async fn my_luckydraw_logs(prev: Option<Nat>, take: Option<Nat>) -> Vec<types::LuckyDrawLog> {
    let prev = prev.as_ref().map(nat_to_u64);
    let take = take.as_ref().map(nat_to_u64).unwrap_or(10).min(100) as usize;
    store::luckydraw::logs(prev, take, Some(ic_cdk::caller()))
}

#[ic_cdk::query]
async fn notifications() -> Vec<types::Notification> {
    store::notification::list()
}

// (Issuer code, Issue time, Expire, Claimable amount, Quantity, Filled quantity)
#[ic_cdk::query]
async fn prizes_of(owner: Option<Principal>) -> Vec<(u32, u32, u16, u32, u16, u16)> {
    let owner = owner.unwrap_or(ic_cdk::caller());
    if owner == ANONYMOUS {
        return vec![];
    }
    match store::airdrop::state_of(&owner) {
        None => vec![],
        Some(store::AirdropState(code, _, _)) => store::prize::list(code)
            .0
            .into_iter()
            .map(|p| (code, p.0 .0, p.0 .1, p.0 .2, p.0 .3, p.1))
            .collect(),
    }
}
