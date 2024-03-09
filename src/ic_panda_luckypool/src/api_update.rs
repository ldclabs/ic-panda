use crate::{
    icp_transfer_from, icp_transfer_to, is_authenticated, nat_to_u64, store, token_balance_of,
    token_transfer_to, types, AIRDROP_AMOUNT, ICP_1, SECOND, TOKEN_1, TRANS_FEE,
};
use candid::Nat;
use ic_captcha::CaptchaBuilder;
use once_cell::sync::Lazy;

const CAPTCHA_EXPIRE_SEC: u64 = 60 * 5;
const LOWEST_LUCKYDRAW_BALANCE: u64 = TOKEN_1 * 100;

static CAPTCHA_BUILDER: Lazy<CaptchaBuilder> = Lazy::new(CaptchaBuilder::new);

#[ic_cdk::update(guard = "is_authenticated")]
async fn captcha() -> Result<types::CaptchaOutput, String> {
    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .map_err(|_err| "failed to get random bytes".to_string())?;

    let captcha = CAPTCHA_BUILDER.generate(&rr.0, None);
    let now = ic_cdk::api::time();
    let challenge = types::ChallengeCode {
        code: captcha.text().to_lowercase(),
    };

    let challenge =
        store::captcha::with_secret(|secret| challenge.sign_to_base64(secret, now / SECOND));
    Ok(types::CaptchaOutput {
        img_base64: captcha.to_base64(0),
        challenge,
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn airdrop(args: types::AirdropClaimInput) -> Result<Nat, String> {
    let now = ic_cdk::api::time() / SECOND;
    let expire_at = now - CAPTCHA_EXPIRE_SEC;
    let challenge = types::ChallengeCode {
        code: args.code.to_lowercase(),
    };
    store::captcha::with_secret(|secret| {
        challenge.verify_from_base64(secret, expire_at, &args.challenge)
    })?;

    let user = ic_cdk::caller();
    if store::airdrop::has(user) {
        return Ok(Nat::from(AIRDROP_AMOUNT));
    }

    if store::airdrop::balance() < AIRDROP_AMOUNT {
        return Err("airdrop pool is empty".to_string());
    }

    if !store::user::active(user) {
        return Err("try again later".to_string());
    }

    let _guard = scopeguard::guard((), |_| {
        store::user::deactive(user);
    });

    // don't need to check balance, because the token canister will reject the transfer if the balance is insufficient
    let _block_idx = token_transfer_to(user, Nat::from(AIRDROP_AMOUNT)).await?;
    let _res = store::airdrop::update(user, now, AIRDROP_AMOUNT);
    // TODO: trace error logs
    // ic_cdk::api::print(format!("airdrop: {:?}\n", _res));
    Ok(Nat::from(AIRDROP_AMOUNT))
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn luckydraw(args: types::LuckyDrawInput) -> Result<Nat, String> {
    if args.icp < 1 || args.icp > 100 {
        return Err("invalid icp amount, should be in [1, 100]".to_string());
    }

    let user = ic_cdk::caller();
    if !store::user::active(user) {
        return Err("try again later".to_string());
    }
    let _guard = scopeguard::guard((), |_| {
        store::user::deactive(user);
    });

    let now = ic_cdk::api::time() / SECOND;
    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .map_err(|_err| "failed to get random bytes".to_string())?;
    let (x, amount) = luckydraw_amount(&rr.0);
    let icp = args.icp as u64 * ICP_1;
    let amount = args.icp as u64 * amount;

    let balance = token_balance_of(ic_cdk::id()).await?;
    let lowest_balance = LOWEST_LUCKYDRAW_BALANCE * args.icp as u64 + TRANS_FEE;
    if balance < lowest_balance {
        return Err("insufficient token balance for luckydraw".to_string());
    }

    let _ = icp_transfer_from(user, Nat::from(icp)).await?;
    let balance = token_balance_of(ic_cdk::id())
        .await
        .unwrap_or(Nat::from(0u64));
    let draw_amount = if balance >= lowest_balance {
        let balance = nat_to_u64(&balance).saturating_sub(TRANS_FEE);
        let draw_amount = if balance < amount { balance } else { amount };
        match token_transfer_to(user, Nat::from(draw_amount)).await {
            Ok(_) => draw_amount,
            Err(_) => 0,
        }
    } else {
        0
    };

    if draw_amount > 0 {
        let _res = store::luckydraw::update(user, now, draw_amount, icp, x);
        // TODO: trace error logs
        // ic_cdk::api::print(format!("luckydraw: {:?}\n", _res));
        Ok(Nat::from(draw_amount))
    } else {
        // refund ICP when failed to transfer tokens
        let _ = icp_transfer_to(user, Nat::from(icp))
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
    let a = x % LUCKYDRAW_DIVISOR;
    let amount = match a / TOKEN_1 {
        v if v <= 5 => 100000,
        v if v <= 1000 => 1000,
        v => v,
    };
    (x, amount * TOKEN_1)
}
