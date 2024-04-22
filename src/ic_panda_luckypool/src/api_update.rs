use candid::Nat;
use lib_panda::{mac_256, ChallengeState, Cryptogram, Ed25519Message, VerifyingKey};
use serde_bytes::ByteBuf;

use crate::{
    icp_transfer_from, icp_transfer_to, is_authenticated, nat_to_u64, store, token_balance_of,
    token_transfer_from, token_transfer_to, types, utils, ICP_1, SECOND, TOKEN_1, TOKEN_CANISTER,
    TRANS_FEE,
};

const LUCKIEST_AIRDROP_AMOUNT: u64 = 100_000;
const NAMING_DEPOSIT_TOKENS: u32 = 3_000;

#[ic_cdk::update(guard = "is_authenticated")]
async fn captcha() -> Result<types::CaptchaOutput, String> {
    Err("deprecated".to_string())
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn airdrop(args: types::AirdropClaimInput) -> Result<types::AirdropStateOutput, String> {
    let caller = ic_cdk::caller();
    let key = *store::keys::AIRDROP_KEY;
    let now_sec = ic_cdk::api::time() / SECOND;
    let prize = if !args.challenge.is_empty() {
        let pk = store::keys::with_challenge_pub_key(VerifyingKey::from_bytes)
            .map_err(|_| "failed to get the public key of the challenge".to_string())?;
        let state = ChallengeState::verify_from(&pk, &args.challenge)?;
        if !state.is_valid(&caller, &state.0 .1, now_sec) {
            return Err("invalid xauth challenge or expired".to_string());
        }
        if !store::xauth::try_set(state.0 .1, caller, now_sec) {
            return Err("xauth user id exists".to_string());
        }

        None
    } else {
        match store::Prize::decode(&key, Some(caller), &args.code) {
            Ok(prize) => {
                // should be issued by the system
                if !prize.is_valid_system(now_sec) {
                    return Err("invalid airdrop code or expired".to_string());
                }
                None
            }
            Err(_) => match store::Prize::decode(&key, None, &args.code) {
                Ok(prize) => {
                    // should be issued by the user
                    if !prize.is_valid(now_sec) || prize.3 != 0 || prize.0 == 0 {
                        return Err("invalid airdrop code or expired".to_string());
                    }
                    Some(prize)
                }
                Err(_) => return Err("invalid airdrop code".to_string()),
            },
        }
    };

    if let Some(store::AirdropState(code, claimed, claimable)) = store::airdrop::state_of(&caller) {
        return Ok(types::AirdropStateOutput {
            lucky_code: Some(utils::luckycode_to_string(code)),
            claimed: Nat::from(claimed),
            claimable: Nat::from(claimable),
        });
    }

    let (airdrop_amount, airdrop_balance) = store::state::airdrop_amount_balance();
    if airdrop_balance < airdrop_amount * TOKEN_1 + TRANS_FEE {
        return Err("insufficient airdrop balance".to_string());
    }

    if !store::user::active(caller) {
        return Err("try again later".to_string());
    }

    let _guard = scopeguard::guard((), |_| {
        store::user::deactive(caller);
    });

    let lucky_code = if let Some(ref p) = prize {
        Some(utils::luckycode_to_string(p.0))
    } else {
        args.lucky_code
    };
    let referrer = lucky_code.and_then(|s| store::luckycode::get_by_string(&s));
    let (claimable, rebate_bonus) = if referrer.is_some() {
        let rebate_bonus = (airdrop_amount / 2) * TOKEN_1;
        (airdrop_amount * TOKEN_1 + rebate_bonus, rebate_bonus)
    } else {
        (airdrop_amount * TOKEN_1, 0)
    };

    // issued by users and try to claim airdrop
    if let Some(prize) = prize {
        store::prize::claim_airdrop(caller, prize)?;
    }

    let caller_code = store::luckycode::new_from(caller);
    let log = store::airdrop::insert(
        caller,
        referrer,
        now_sec,
        claimable,
        rebate_bonus,
        caller_code,
    )?;

    store::state::with_mut(|r| {
        let cost = claimable + rebate_bonus + TRANS_FEE;
        r.airdrop_balance = r.airdrop_balance.saturating_sub(cost);
        r.total_airdrop = r.total_airdrop.saturating_add(cost);
        r.total_airdrop_count += 1;
        r.latest_airdrop_logs.insert(0, log);
        if r.latest_airdrop_logs.len() > 10 {
            r.latest_airdrop_logs.truncate(10);
        }
    });

    Ok(types::AirdropStateOutput {
        lucky_code: Some(utils::luckycode_to_string(caller_code)),
        claimed: Nat::from(0u64),
        claimable: Nat::from(claimable),
    })
}

// Deprecated!
#[ic_cdk::update(guard = "is_authenticated")]
async fn prize(_: String) -> Result<types::AirdropStateOutput, String> {
    Err("can not claim prize".to_string())
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn claim_prize(args: types::ClaimPrizeInput) -> Result<types::ClaimPrizeOutput, String> {
    let caller = ic_cdk::caller();
    let key = *store::keys::PRIZE_KEY;
    let cryptogram = args
        .code
        .strip_prefix("PRIZE:")
        .unwrap_or(args.code.as_str());
    let (prize, prize_data) = store::Prize::try_decode(&key, Some(caller), cryptogram)?;
    let now_ms = ic_cdk::api::time() / 1000000;
    let now_sec = now_ms / 1000;
    if !prize.is_valid(now_sec) {
        return Err("invalid prize code or expired".to_string());
    }
    if prize.0 == 0 || prize.3 == 0 {
        return Err("invalid prize code".to_string());
    }

    let pk = store::keys::with_challenge_pub_key(VerifyingKey::from_bytes)
        .map_err(|_| "failed to get the public key of the challenge".to_string())?;
    let token: ChallengeState<ByteBuf> = ChallengeState::verify(&pk, &args.challenge)?;
    if !token.is_valid(&caller, &prize_data, now_sec) {
        return Err("invalid challenge token or expired".to_string());
    }

    if !store::user::active(caller) {
        return Err("try again later".to_string());
    }

    let _guard = scopeguard::guard((), |_| {
        store::user::deactive(caller);
    });

    let store::AirdropState(caller_code, _, claimable) = store::airdrop::state_of(&caller)
        .ok_or("get your lucky code through airdrop to claim prize".to_string())?;
    if caller_code == 0 {
        return Err("user is banned".to_string());
    }
    if claimable < TOKEN_1 * 10 {
        let balance = token_balance_of(TOKEN_CANISTER, caller)
            .await
            .unwrap_or(Nat::from(0u64));
        if (claimable + balance) < TOKEN_1 * 10 {
            return Err("the balance must be more than 10 tokens to claim prize.".to_string());
        }
    }

    let (amount, avg) =
        store::prize::claim(caller_code, prize, claimable / TOKEN_1, now_sec, now_ms % 5)?;
    let state = store::airdrop::deposit(caller, amount)?;
    store::state::with_mut(|r| {
        r.total_prize = Some(r.total_prize.unwrap_or_default().saturating_add(amount));
        r.total_prize_count = Some(r.total_prize_count.unwrap_or_default() + 1);
    });

    Ok(types::ClaimPrizeOutput {
        state: types::AirdropStateOutput {
            lucky_code: Some(utils::luckycode_to_string(caller_code)),
            claimed: Nat::from(state.1),
            claimable: Nat::from(state.2),
        },
        claimed: Nat::from(amount),
        average: Nat::from(avg),
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn harvest(args: types::AirdropHarvestInput) -> Result<types::AirdropStateOutput, String> {
    let caller = ic_cdk::caller();
    if !store::user::active(caller) {
        return Err("try again later".to_string());
    }
    let _guard = scopeguard::guard((), |_| {
        store::user::deactive(caller);
    });

    match store::airdrop::state_of(&caller) {
        None => Err("no lucky code".to_string()),
        Some(store::AirdropState(caller_code, _, claimable)) => {
            if caller_code == 0 {
                return Err("user is banned".to_string());
            }

            let amount = nat_to_u64(&args.amount);
            if amount < TOKEN_1 {
                return Err("amount must be at least 1 token".to_string());
            }

            if amount > claimable {
                return Err("insufficient lucky balance to transfer".to_string());
            }

            let state = store::airdrop::withdraw(caller, amount)?;
            if let Err(err) = token_transfer_to(caller, args.amount, "WITHDRAW".to_string()).await {
                ic_cdk::trap(&format!("failed to transfer tokens, {}", err));
            }

            Ok(types::AirdropStateOutput {
                lucky_code: Some(utils::luckycode_to_string(state.0)),
                claimed: Nat::from(state.1),
                claimable: Nat::from(state.2),
            })
        }
    }
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn luckydraw(args: types::LuckyDrawInput) -> Result<types::LuckyDrawOutput, String> {
    let icp01 = if args.icp == 0 {
        args.amount.map_or(0, |v| nat_to_u64(&v) * 10 / TOKEN_1)
    } else {
        args.icp as u64 * 10
    };

    if !(1..=1000).contains(&icp01) {
        return Err("invalid icp amount, should be in [0.1, 100]".to_string());
    }
    if store::state::with(|r| r.total_luckydraw) >= 420000000 * TOKEN_1 {
        return Err("The lucky draw pool has been drawn empty.".to_string());
    }

    let caller = ic_cdk::caller();
    if !store::user::active(caller) {
        return Err("try again later".to_string());
    }
    let _guard = scopeguard::guard((), |_| {
        store::user::deactive(caller);
    });

    let state = store::airdrop::state_of(&caller);
    if let Some(ref state) = state {
        if state.0 == 0 {
            return Err("user is banned".to_string());
        }
    }

    let now_sec = ic_cdk::api::time() / SECOND;
    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .map_err(|_err| "failed to get random bytes".to_string())?;
    let (x, amount) = luckydraw_amount(&mac_256(&rr.0, b"ICPanda"));
    let is_luckiest = amount == LUCKIEST_AIRDROP_AMOUNT * TOKEN_1;
    let icp = icp01 * ICP_1 / 10;
    let amount = icp01 * amount / 10;

    let _ = icp_transfer_from(caller, Nat::from(icp - TRANS_FEE), "LUCKYDRAW".to_string()).await?;
    let res: Result<u32, String> = {
        match state {
            None => {
                let (airdrop_amount, _) = store::state::airdrop_amount_balance();
                let caller_code = store::luckycode::new_from(caller);
                store::airdrop::insert(
                    caller,
                    None,
                    now_sec,
                    airdrop_amount * TOKEN_1,
                    0,
                    caller_code,
                )
                .and_then(|_| store::airdrop::deposit(caller, amount).map(|_| caller_code))
            }
            Some(store::AirdropState(caller_code, _, _)) => {
                store::airdrop::deposit(caller, amount).map(|_| caller_code)
            }
        }
    };

    match res {
        Ok(caller_code) => {
            let log = store::luckydraw::insert(caller, now_sec, amount, icp, x)?;
            store::state::with_mut(|r| {
                r.total_luckydraw = r.total_luckydraw.saturating_add(amount + TRANS_FEE);
                r.total_luckydraw_icp = r.total_luckydraw_icp.saturating_add(icp - TRANS_FEE);
                r.total_luckydraw_count += 1;
                r.latest_luckydraw_logs.insert(0, log.clone());
                if r.latest_luckydraw_logs.len() > 10 {
                    r.latest_luckydraw_logs.truncate(10);
                }
                if is_luckiest {
                    r.luckiest_luckydraw_logs.insert(0, log);
                    if r.luckiest_luckydraw_logs.len() > 3 {
                        r.luckiest_luckydraw_logs.truncate(3);
                    }
                }
            });

            let airdrop_cryptogram =
                store::prize::try_add_airdrop(caller_code, now_sec, 4320, 0, (icp01 as u16) * 5);

            Ok(types::LuckyDrawOutput {
                amount: Nat::from(amount),
                random: x,
                luckypool_empty: false,
                prize_cryptogram: None,
                airdrop_cryptogram,
            })
        }
        Err(err) => {
            // refund ICP when failed to transfer tokens
            let _ = icp_transfer_to(
                caller,
                Nat::from(icp - TRANS_FEE - TRANS_FEE),
                "LUCKYDRAW:REFUND".to_string(),
            )
            .await
            .map_err(|err| format!("failed to refund ICP, {}", err))?;
            Err(format!("failed to draw PANDA, ICP refunded, {}", err))
        }
    }
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn add_prize(args: types::AddPrizeInputV2) -> Result<types::PrizeOutput, String> {
    args.validate()?;
    let prize_subsidy =
        store::state::with(|r| r.prize_subsidy.clone()).ok_or("can not add prize currently.")?;

    let caller = ic_cdk::caller();
    if !store::user::active(caller) {
        return Err("try again later".to_string());
    }
    let _guard = scopeguard::guard((), |_| {
        store::user::deactive(caller);
    });

    let store::AirdropState(caller_code, _, _) = store::airdrop::state_of(&caller)
        .ok_or("you don't have lucky code to add prize".to_string())?;
    if caller_code == 0 {
        return Err("user is banned".to_string());
    }
    let now_sec = ic_cdk::api::time() / SECOND;
    let kind = args.kind.unwrap_or_default();
    let subsidy = if kind == 1 {
        prize_subsidy.subsidy(args.total_amount, args.quantity)
    } else {
        0
    };
    let payment = (args.total_amount - subsidy) as u64 * TOKEN_1 + prize_subsidy.0;
    let prize = store::Prize(
        caller_code,
        (now_sec / 60) as u32,
        args.expire,
        args.total_amount,
        args.quantity,
    );
    let info = store::PrizeInfo(kind, prize_subsidy.0, subsidy, 0, 0, 0, args.memo);

    if !store::prize::add(prize.clone(), info.clone()) {
        return Err("failed to add prize".to_string());
    }
    if let Err(err) = token_transfer_from(caller, Nat::from(payment), "PRIZE:ADD".to_string()).await
    {
        store::prize::clear_failed(&prize);
        return Err(err);
    }

    store::prize::add_refund_job(prize.clone())?;
    store::state::with_mut(|r| {
        r.total_prizes_count = Some(r.total_prizes_count.unwrap_or_default().saturating_add(1));
        if let Some(prize_subsidy) = r.prize_subsidy.as_mut() {
            prize_subsidy.5 = prize_subsidy.5.saturating_sub(1);
        }
    });

    let code = prize.encode(&(*store::keys::PRIZE_KEY), args.recipient);
    let name = store::naming::get(&caller_code).map(|n| n.0);
    Ok(types::PrizeOutput::from(&prize, &info, name, Some(code)))
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn register_name(args: types::NameInput) -> Result<types::NameOutput, String> {
    args.validate()?;
    let caller = ic_cdk::caller();
    if !store::user::active(caller) {
        return Err("try again later".to_string());
    }
    let _guard = scopeguard::guard((), |_| {
        store::user::deactive(caller);
    });

    let store::AirdropState(caller_code, _, _) = store::airdrop::state_of(&caller)
        .ok_or("you don't have lucky code to register name".to_string())?;
    if caller_code == 0 {
        return Err("user is banned".to_string());
    }
    let now_sec = ic_cdk::api::time() / SECOND;
    if let Some(name) = store::naming::get(&caller_code) {
        return Err(format!("you have registered a name: {}", name.0));
    }

    let name_state = store::NamingState(
        args.name.clone(),
        now_sec,
        NAMING_DEPOSIT_TOKENS,
        NAMING_DEPOSIT_TOKENS / 10,
    );
    if store::naming::try_set_name(caller_code, name_state.clone()) {
        if let Err(err) = token_transfer_from(
            caller,
            Nat::from(NAMING_DEPOSIT_TOKENS as u64 * TOKEN_1),
            "NAME:REG".to_string(),
        )
        .await
        {
            store::naming::remove_name(caller_code, &args.name);
            return Err(err);
        }
    } else {
        return Err("failed to register name".to_string());
    }

    Ok(types::NameOutput {
        code: utils::luckycode_to_string(caller_code),
        name: name_state.0,
        created_at: name_state.1,
        deposit: Nat::from(name_state.2 as u64 * TOKEN_1),
        annual_fee: Nat::from(name_state.3 as u64 * TOKEN_1),
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn update_name(args: types::NameInput) -> Result<types::NameOutput, String> {
    args.validate()?;
    let old = args.old_name.ok_or("old name is required".to_string())?;

    let caller = ic_cdk::caller();
    if !store::user::active(caller) {
        return Err("try again later".to_string());
    }
    let _guard = scopeguard::guard((), |_| {
        store::user::deactive(caller);
    });

    let store::AirdropState(caller_code, _, _) = store::airdrop::state_of(&caller)
        .ok_or("you don't have lucky code to update name".to_string())?;
    if caller_code == 0 {
        return Err("user is banned".to_string());
    }
    let mut name_state = store::naming::get(&caller_code).ok_or("no name to update".to_string())?;

    name_state.0 = args.name.clone();
    if !store::naming::try_update_name(caller_code, &old, name_state.clone()) {
        return Err("failed to update name".to_string());
    }

    Ok(types::NameOutput {
        code: utils::luckycode_to_string(caller_code),
        name: name_state.0,
        created_at: name_state.1,
        deposit: Nat::from(name_state.2 as u64 * TOKEN_1),
        annual_fee: Nat::from(name_state.3 as u64 * TOKEN_1),
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn unregister_name(args: types::NameInput) -> Result<Nat, String> {
    args.validate()?;
    let caller = ic_cdk::caller();
    if !store::user::active(caller) {
        return Err("try again later".to_string());
    }
    let _guard = scopeguard::guard((), |_| {
        store::user::deactive(caller);
    });

    let store::AirdropState(caller_code, _, _) = store::airdrop::state_of(&caller)
        .ok_or("you don't have lucky code to unregister name".to_string())?;
    if caller_code == 0 {
        return Err("user is banned".to_string());
    }
    let now_sec = ic_cdk::api::time() / SECOND;
    let name_state = store::naming::get(&caller_code).ok_or("no name to unregister".to_string())?;
    if name_state.0 != args.name {
        return Err("name does not match".to_string());
    }
    if !store::naming::remove_name(caller_code, &args.name) {
        return Err("failed to unregister name".to_string());
    }
    let du = now_sec - name_state.1;
    let y = 3600 * 24 * 365;
    let r = du % y;
    let n = (du / y + if r > 3600 * 24 * 7 { 1 } else { 0 }) as u32;
    let refund = if name_state.2 > name_state.3 * n {
        (name_state.2 - n * name_state.3) as u64 * TOKEN_1
    } else {
        0
    };
    if refund > 0 {
        let _ = token_transfer_to(caller, Nat::from(refund), "NAME:UNREG".to_string())
            .await
            .map_err(|err| format!("failed to refund, {}", err))?;
    }

    Ok(Nat::from(refund))
}

// 344693032001 from b"PANDA"
const LUCKYDRAW_DIVISOR: u64 = u64::from_be_bytes([0, 0, 0, b'P', b'A', b'N', b'D', b'A']);

fn luckydraw_amount(random: &[u8]) -> (u64, u64) {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&random[0..8]);
    let x = u64::from_be_bytes(bytes);
    let x = x % LUCKYDRAW_DIVISOR;
    let amount = match x / TOKEN_1 {
        v if v <= 5 => LUCKIEST_AIRDROP_AMOUNT,
        v if v <= 1000 => 1000,
        v => v,
    };

    (x, amount * TOKEN_1)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_luckydraw_amount() {
        assert_eq!(LUCKYDRAW_DIVISOR, 344693032001);

        let rt = luckydraw_amount(vec![0u8; 8].as_slice());
        assert_eq!(rt.0, 0);
        assert_eq!(rt.1, 100000 * TOKEN_1);

        let rt = luckydraw_amount(vec![0, 0, 0, 0, 29, 205, 101, 0].as_slice());
        assert_eq!(rt.0, TOKEN_1 * 5);
        assert_eq!(rt.1, 100000 * TOKEN_1);

        let rt = luckydraw_amount(vec![0, 0, 0, 2, 84, 11, 228, 0].as_slice());
        assert_eq!(rt.0, TOKEN_1 * 100);
        assert_eq!(rt.1, 1000 * TOKEN_1);

        let rt = luckydraw_amount(vec![0, 0, 0, 23, 72, 118, 232, 0].as_slice());
        assert_eq!(rt.0, TOKEN_1 * 1000);
        assert_eq!(rt.1, 1000 * TOKEN_1);

        let rt = luckydraw_amount(vec![0, 0, 0, 46, 144, 237, 208, 0].as_slice());
        assert_eq!(rt.0, TOKEN_1 * 2000);
        assert_eq!(rt.1, 2000 * TOKEN_1);

        let rt = luckydraw_amount(vec![0, 0, 0, 80, 59, 194, 182, 0].as_slice());
        assert_eq!(rt.0, TOKEN_1 * 3446);
        assert_eq!(rt.1, 3446 * TOKEN_1);

        let rt = luckydraw_amount(vec![0, 0, 0, 80, 65, 184, 151, 0].as_slice());
        assert_eq!(rt.0, TOKEN_1 * 3447u64 - LUCKYDRAW_DIVISOR);
        assert_eq!(rt.1, 100000 * TOKEN_1);
    }
}
