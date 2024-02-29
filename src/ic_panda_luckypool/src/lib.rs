use base64::{engine::general_purpose, Engine};
use candid::Principal;
use ic_captcha::CaptchaBuilder;
use once_cell::sync::Lazy;
use sha3::{Digest, Sha3_256};

mod store;
mod types;

use store::{with_state, with_state_mut};
use types::{AirdropClaimArg, Challenge};

static ANONYMOUS: Principal = Principal::anonymous();
static CAPTCHA_BUILDER: Lazy<CaptchaBuilder> = Lazy::new(CaptchaBuilder::new);

#[ic_cdk::query]
fn whoami() -> Principal {
    ic_cdk::caller()
}

#[ic_cdk::update]
async fn reset_secret() -> Result<(), String> {
    if !ic_cdk::api::is_controller(&ic_cdk::caller()) {
        return Err("only the controller can reset secret".to_string());
    }

    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .expect("failed to get random bytes");

    let captcha_secret = next_seed(&rr.0, b"Captcha Secret");
    with_state_mut(|state| {
        state.captcha_secret = captcha_secret;
    });

    Ok(())
}

#[ic_cdk::update]
async fn get_captcha() -> Result<Challenge, String> {
    let user = ic_cdk::caller();
    if user == ANONYMOUS {
        return Err("anonymous user is not allowed".to_string());
    }

    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .expect("failed to get random bytes");
    let captcha = CAPTCHA_BUILDER.generate(&rr.0, None);
    let challenge = with_state(|state| {
        next_seed(
            &state.captcha_secret,
            captcha.text().to_lowercase().as_bytes(),
        )
    });

    Ok(Challenge {
        img_base64: captcha.to_base64(0),
        challenge: general_purpose::URL_SAFE_NO_PAD.encode(&challenge[0..16]),
    })
}

#[ic_cdk::update]
async fn airdrop_claim(arg: AirdropClaimArg) -> Result<(), String> {
    let user = ic_cdk::caller();
    if user == ANONYMOUS {
        return Err("anonymous user is not allowed".to_string());
    }

    let challenge = general_purpose::URL_SAFE_NO_PAD
        .decode(arg.challenge.as_bytes())
        .map_err(|_err| "invalid challenge".to_string())?;

    let challenge2 =
        with_state(|state| next_seed(&state.captcha_secret, arg.code.to_lowercase().as_bytes()));
    if challenge.as_slice() != &challenge2[0..16] {
        return Err("invalid captcha code".to_string());
    }

    // TODO: claim airdrop

    Ok(())
}

fn next_seed(seed: &[u8], add: &[u8]) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(seed);
    hasher.update(add);
    hasher.finalize().into()
}

ic_cdk::export_candid!();
