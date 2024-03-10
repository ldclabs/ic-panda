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
async fn airdrop_state_of() -> Result<types::AirdropStateOutput, ()> {
    let caller = ic_cdk::caller();
    if caller == ANONYMOUS {
        return Ok(types::AirdropStateOutput {
            lucky_code: "".to_string(),
            claimed: Nat::from(0u64),
            claimable: Nat::from(AIRDROP_AMOUNT * TOKEN_1),
        });
    }

    match store::airdrop::state_of(&caller) {
        Some(store::AirdropState(code, claimed, claimable)) => Ok(types::AirdropStateOutput {
            lucky_code: utils::luckycode_to_string(code),
            claimed: Nat::from(claimed * TOKEN_1),
            claimable: Nat::from(claimable * TOKEN_1),
        }),
        None => Ok(types::AirdropStateOutput {
            lucky_code: "".to_string(),
            claimed: Nat::from(0u64),
            claimable: Nat::from(AIRDROP_AMOUNT * TOKEN_1),
        }),
    }
}

#[ic_cdk::query]
async fn airdrop_logs(prev: Option<Nat>, take: Option<Nat>) -> Result<Vec<types::AirdropLog>, ()> {
    let prev = prev.as_ref().map(nat_to_u64);
    let take = take.as_ref().map(nat_to_u64).unwrap_or(10).min(100) as usize;
    let logs = store::airdrop::logs(prev, take);
    Ok(logs)
}

#[ic_cdk::query]
async fn luckydraw_logs(
    prev: Option<Nat>,
    take: Option<Nat>,
) -> Result<Vec<types::LuckyDrawLog>, ()> {
    let prev = prev.as_ref().map(nat_to_u64);
    let take = take.as_ref().map(nat_to_u64).unwrap_or(10).min(100) as usize;
    let logs = store::luckydraw::logs(prev, take);
    Ok(logs)
}
