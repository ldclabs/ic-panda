use crate::{
    icp_transfer_from, icp_transfer_to, is_authenticated, nat_to_u64, store, token_balance_of,
    token_transfer_to, types, utils, AIRDROP_AMOUNT, ICP_1, SECOND, TOKEN_1, TRANS_FEE,
};
use candid::Nat;
use ic_captcha::CaptchaBuilder;
use once_cell::sync::Lazy;

const CAPTCHA_EXPIRE_SEC: u64 = 60 * 5;
const LUCKIEST_AIRDROP_AMOUNT: u64 = 100_000;
const LOWEST_LUCKYDRAW_BALANCE: u64 = 500;

static CAPTCHA_BUILDER: Lazy<CaptchaBuilder> = Lazy::new(CaptchaBuilder::new);

#[ic_cdk::update(guard = "is_authenticated")]
async fn captcha() -> Result<types::CaptchaOutput, String> {
    let caller = ic_cdk::caller();
    if !store::user::active(caller) {
        return Err("try again later".to_string());
    }
    let _guard = scopeguard::guard((), |_| {
        store::user::deactive(caller);
    });

    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .map_err(|_err| "failed to get random bytes".to_string())?;

    let captcha = CAPTCHA_BUILDER.generate(&rr.0, None);
    let now_sec = ic_cdk::api::time() / SECOND;
    let challenge = types::ChallengeCode {
        code: captcha.text().to_lowercase(),
    };

    let challenge = store::captcha::with_secret(|secret| challenge.sign_to_base64(secret, now_sec));
    Ok(types::CaptchaOutput {
        img_base64: captcha.to_base64(0),
        challenge,
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn airdrop(args: types::AirdropClaimInput) -> Result<types::AirdropStateOutput, String> {
    let now_sec = ic_cdk::api::time() / SECOND;
    let expire_at = now_sec - CAPTCHA_EXPIRE_SEC;
    let challenge = types::ChallengeCode {
        code: args.code.to_lowercase(),
    };

    store::captcha::with_secret(|secret| {
        challenge.verify_from_base64(secret, expire_at, &args.challenge)
    })?;

    let caller = ic_cdk::caller();
    if let Some(store::AirdropState(code, claimed, claimable)) = store::airdrop::state_of(&caller) {
        return Ok(types::AirdropStateOutput {
            lucky_code: Some(utils::luckycode_to_string(code)),
            claimed: Nat::from(claimed),
            claimable: Nat::from(claimable),
        });
    }

    if store::state::airdrop_balance() < AIRDROP_AMOUNT * TOKEN_1 + TRANS_FEE {
        return Err("airdrop pool is empty".to_string());
    }

    if !store::user::active(caller) {
        return Err("try again later".to_string());
    }

    let _guard = scopeguard::guard((), |_| {
        store::user::deactive(caller);
    });

    let referrer = args
        .lucky_code
        .and_then(|s| store::luckycode::get_by_string(&s));
    let claimed = if referrer.is_some() {
        (AIRDROP_AMOUNT + AIRDROP_AMOUNT / 2) * TOKEN_1
    } else {
        AIRDROP_AMOUNT * TOKEN_1
    };

    // don't need to check balance, because the token canister will reject the transfer if the balance is insufficient
    let _block_idx = token_transfer_to(caller, Nat::from(claimed)).await?;
    let caller_code = store::luckycode::new_from(caller, args.challenge.as_bytes());
    let log = store::airdrop::insert(caller, referrer, now_sec, claimed, caller_code)?;
    store::state::with_mut(|r| {
        r.airdrop_balance = r.airdrop_balance.saturating_sub(claimed + TRANS_FEE);
        r.total_airdrop = r.total_airdrop.saturating_add(claimed + TRANS_FEE);
        r.total_airdrop_count += 1;
        r.latest_airdrop_logs.insert(0, log);
        if r.latest_airdrop_logs.len() > 10 {
            r.latest_airdrop_logs.truncate(10);
        }
    });

    Ok(types::AirdropStateOutput {
        lucky_code: Some(utils::luckycode_to_string(caller_code)),
        claimed: Nat::from(claimed),
        claimable: Nat::from(0u32),
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

    let now_sec = ic_cdk::api::time() / SECOND;

    match store::airdrop::state_of(&caller) {
        None => Err("no claimable tokens to harvest".to_string()),
        Some(store::AirdropState(_, _, claimable)) => {
            let amount = nat_to_u64(&args.amount);
            if amount > claimable {
                return Err("insufficient claimable tokens to harvest".to_string());
            }

            let _block_idx = token_transfer_to(caller, args.amount).await?;
            let (state, log) = store::airdrop::harvest(caller, now_sec, amount)?;
            store::state::with_mut(|r| {
                r.airdrop_balance = r.airdrop_balance.saturating_sub(amount + TRANS_FEE);
                r.total_airdrop = r.total_airdrop.saturating_add(amount + TRANS_FEE);
                r.total_airdrop_count += 1;
                r.latest_airdrop_logs.insert(0, log);
                if r.latest_airdrop_logs.len() > 10 {
                    r.latest_airdrop_logs.truncate(10);
                }
            });

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
    if args.icp < 1 || args.icp > 10 {
        return Err("invalid icp amount, should be in [1, 10]".to_string());
    }

    let caller = ic_cdk::caller();
    if !store::user::active(caller) {
        return Err("try again later".to_string());
    }
    let _guard = scopeguard::guard((), |_| {
        store::user::deactive(caller);
    });

    let now_sec = ic_cdk::api::time() / SECOND;
    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .map_err(|_err| "failed to get random bytes".to_string())?;
    let (x, amount) = luckydraw_amount(&utils::mac_256(&rr.0, b"ICPanda"));
    let is_luckiest = amount == LUCKIEST_AIRDROP_AMOUNT * TOKEN_1;
    let icp = args.icp as u64 * ICP_1;
    let amount = args.icp as u64 * amount;

    let balance = token_balance_of(ic_cdk::id()).await?;
    let lowest_balance = LOWEST_LUCKYDRAW_BALANCE * TOKEN_1 * args.icp as u64 + TRANS_FEE;
    if balance < lowest_balance {
        return Err("insufficient token balance for luckydraw".to_string());
    }

    let _ = icp_transfer_from(caller, Nat::from(icp - TRANS_FEE)).await?;
    let balance = token_balance_of(ic_cdk::id())
        .await
        .unwrap_or(Nat::from(0u64));
    let draw_amount = if balance >= lowest_balance {
        let balance = nat_to_u64(&balance).saturating_sub(TRANS_FEE);
        let draw_amount = if balance < amount { balance } else { amount };
        match token_transfer_to(caller, Nat::from(draw_amount)).await {
            Ok(_) => draw_amount,
            Err(_) => 0,
        }
    } else {
        0
    };

    if draw_amount > 0 {
        let log = store::luckydraw::insert(caller, now_sec, draw_amount, icp, x)?;
        store::state::with_mut(|r| {
            r.total_luckydraw = r.total_luckydraw.saturating_add(draw_amount + TRANS_FEE);
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
        Ok(types::LuckyDrawOutput {
            amount: Nat::from(draw_amount),
            random: x,
            luckypool_empty: draw_amount < amount,
        })
    } else {
        // refund ICP when failed to transfer tokens
        let _ = icp_transfer_to(caller, Nat::from(icp - TRANS_FEE - TRANS_FEE))
            .await
            .map_err(|err| format!("failed to refund ICP, {}", err))?;
        Err("insufficient token balance for luckydraw".to_string())
    }
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
