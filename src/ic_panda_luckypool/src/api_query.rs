use candid::{Nat, Principal};
use lib_panda::Cryptogram;

use crate::{nat_to_u64, store, types, utils, ANONYMOUS, SECOND, TOKEN_1};

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
async fn name_of(owner: Option<Principal>) -> Result<Option<types::NameOutput>, ()> {
    let owner = owner.unwrap_or(ic_cdk::caller());
    if owner == ANONYMOUS {
        return Ok(None);
    }

    match store::airdrop::state_of(&owner) {
        Some(store::AirdropState(code, _, _)) => {
            Ok(store::naming::get(&code).map(|n| types::NameOutput {
                code: utils::luckycode_to_string(code),
                name: n.0,
                created_at: n.1,
                pledge_amount: Nat::from(n.2 as u64 * TOKEN_1),
                yearly_rental: Nat::from(n.3 as u64 * TOKEN_1),
            }))
        }
        None => Ok(None),
    }
}

#[ic_cdk::query]
async fn name_lookup(name: String) -> Result<Option<types::NameOutput>, ()> {
    if name.len() > 64 {
        return Ok(None);
    }

    Ok(
        store::naming::get_by_name(&name).map(|(code, n)| types::NameOutput {
            code: utils::luckycode_to_string(code),
            name: n.0,
            created_at: n.1,
            pledge_amount: Nat::from(n.2 as u64 * TOKEN_1),
            yearly_rental: Nat::from(n.3 as u64 * TOKEN_1),
        }),
    )
}

#[ic_cdk::query]
async fn airdrop_state_of(owner: Option<Principal>) -> Result<types::AirdropStateOutput, ()> {
    let owner = owner.unwrap_or(ic_cdk::caller());
    let (airdrop_amount, _) = store::state::airdrop_amount_balance();
    if owner == ANONYMOUS {
        return Ok(types::AirdropStateOutput {
            lucky_code: None,
            claimed: Nat::from(0u64),
            claimable: Nat::from(airdrop_amount * TOKEN_1),
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
            claimable: Nat::from(airdrop_amount * TOKEN_1),
        }),
    }
}

#[ic_cdk::query]
async fn airdrop_logs(prev: Option<Nat>, take: Option<Nat>) -> Vec<types::AirdropLog> {
    let prev = prev.as_ref().map(nat_to_u64);
    let take = take.as_ref().map(nat_to_u64).unwrap_or(10).min(1000) as usize;
    store::airdrop::logs(prev, take)
}

#[ic_cdk::query]
async fn luckydraw_logs(prev: Option<Nat>, take: Option<Nat>) -> Vec<types::LuckyDrawLog> {
    let prev = prev.as_ref().map(nat_to_u64);
    let take = take.as_ref().map(nat_to_u64).unwrap_or(10).min(1000) as usize;
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
        Some(store::AirdropState(code, _, _)) => store::prize::list_airdrop(code)
            .0
            .into_iter()
            .map(|p| (code, p.0 .0, p.0 .1, p.0 .2, p.0 .3, p.1))
            .collect(),
    }
}

#[ic_cdk::query]
async fn airdrop_codes_of(owner: Principal) -> Vec<types::AirdropCodeOutput> {
    if owner == ANONYMOUS {
        return vec![];
    }
    match store::airdrop::state_of(&owner) {
        None => vec![],
        Some(store::AirdropState(code, _, _)) => {
            if code == 0 {
                return vec![];
            }

            let issuer = utils::luckycode_to_string(code);
            let name = store::naming::get(&code).map(|n| n.0);
            let key = *store::keys::AIRDROP_KEY;
            store::prize::list_airdrop(code)
                .0
                .into_iter()
                .map(|p| {
                    let prize = store::Prize(code, p.0 .0, p.0 .1, p.0 .2, p.0 .3);

                    types::AirdropCodeOutput {
                        issuer: issuer.clone(),
                        issued_at: prize.1 as u64 * 60,
                        expire: prize.2 as u64 * 60,
                        amount: prize.3 as u64 * TOKEN_1,
                        quantity: prize.4,
                        filled: p.1,
                        name: name.clone(),
                        code: Some(prize.encode(&key, None)),
                    }
                })
                .collect()
        }
    }
}

#[ic_cdk::query]
async fn prize_info(
    code: String,
    recipient: Option<Principal>,
) -> Result<types::PrizeOutput, String> {
    let key = *store::keys::PRIZE_KEY;
    let prize = code.strip_prefix("PRIZE:").unwrap_or(code.as_str());
    let (prize, _) = store::Prize::try_decode(&key, recipient, prize)?;
    let info = store::prize::get_info(&prize).ok_or("prize not found")?;
    let name = store::naming::get(&prize.0).map(|n| n.0);
    Ok(types::PrizeOutput::from(&prize, &info, name, Some(code)))
}

#[ic_cdk::query]
async fn prize_claim_logs(
    owner: Principal,
    prev: Option<Nat>,
    take: Option<Nat>,
) -> Vec<types::PrizeClaimLog> {
    let prev = prev.as_ref().map(nat_to_u64);
    let take = take.as_ref().map(nat_to_u64).unwrap_or(10).min(1000) as usize;
    if owner == ANONYMOUS {
        return vec![];
    }
    match store::airdrop::state_of(&owner) {
        None => vec![],
        Some(store::AirdropState(code, _, _)) => store::prize::claim_logs(code, prev, take),
    }
}

#[ic_cdk::query]
async fn prize_issue_logs(owner: Principal, prev_ts: Option<Nat>) -> Vec<types::PrizeOutput> {
    if owner == ANONYMOUS {
        return vec![];
    }

    let now = ic_cdk::api::time() / SECOND;
    let prev_ts = prev_ts.as_ref().map(nat_to_u64).unwrap_or(now);
    let prev_ts = if prev_ts == 0 || prev_ts > now {
        now
    } else {
        prev_ts
    };
    match store::airdrop::state_of(&owner) {
        None => vec![],
        Some(store::AirdropState(code, _, _)) => {
            store::prize::issue_logs(code, (prev_ts / 60) as u32)
        }
    }
}
