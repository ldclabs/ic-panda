use crate::{store, types, AIRDROP_AMOUNT, ANONYMOUS};
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
    Ok(store::state::get())
}

#[ic_cdk::query]
async fn airdrop_balance() -> Result<Nat, ()> {
    let user = ic_cdk::caller();
    if user == ANONYMOUS {
        return Ok(Nat::from(0u64));
    }

    Ok(if store::airdrop::has(user) {
        Nat::from(AIRDROP_AMOUNT)
    } else {
        Nat::from(0u64)
    })
}

#[ic_cdk::query]
async fn airdrop_total() -> Result<Nat, ()> {
    Ok(Nat::from(store::airdrop::total()))
}

#[ic_cdk::query]
async fn luckydraw_total() -> Result<Nat, ()> {
    Ok(Nat::from(store::luckydraw::total()))
}

#[ic_cdk::query]
async fn airdrop_logs(args: types::LogsInput) -> Result<types::LogsOutput<store::AirdropLog>, ()> {
    let res = store::airdrop::logs(10, args.index);
    Ok(types::LogsOutput {
        next_index: res.1,
        logs: res.0,
    })
}

#[ic_cdk::query]
async fn luckydraw_logs(
    args: types::LogsInput,
) -> Result<types::LogsOutput<store::LuckyDrawLog>, ()> {
    let res = store::luckydraw::logs(10, args.index);
    Ok(types::LogsOutput {
        next_index: res.1,
        logs: res.0,
    })
}
