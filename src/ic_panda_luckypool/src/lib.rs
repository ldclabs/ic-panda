use candid::{candid_method, Principal};
use ic_captcha::CaptchaBuilder;
use once_cell::sync::Lazy;

mod crypto;
mod store;
mod types;

use crypto::mac_256;
use store::{with_state, with_state_mut};
use types::{AirdropClaimInput, CaptchaOutput, Challenge};

const SECOND: u64 = 1_000_000_000;
const CAPTCHA_EXPIRE: u64 = SECOND * 60 * 5;

static ANONYMOUS: Principal = Principal::anonymous();
static CAPTCHA_BUILDER: Lazy<CaptchaBuilder> = Lazy::new(CaptchaBuilder::new);

#[ic_cdk::query]
#[candid_method(query)]
fn whoami() -> Principal {
    ic_cdk::caller()
}

#[ic_cdk::update]
#[candid_method(update)]
async fn reset_secret() -> Result<(), String> {
    if !ic_cdk::api::is_controller(&ic_cdk::caller()) {
        return Err("only the controller can reset secret".to_string());
    }

    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .map_err(|_err| "failed to get random bytes".to_string())?;

    let captcha_secret = mac_256(&rr.0, b"Captcha Secret");
    with_state_mut(|state| {
        state.captcha_secret = captcha_secret;
    });

    Ok(())
}

#[ic_cdk::update]
#[candid_method(update)]
async fn get_captcha() -> Result<CaptchaOutput, String> {
    let user = ic_cdk::caller();
    if user == ANONYMOUS {
        return Err("anonymous user is not allowed".to_string());
    }

    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .map_err(|_err| "failed to get random bytes".to_string())?;

    let captcha = CAPTCHA_BUILDER.generate(&rr.0, None);
    let now = ic_cdk::api::time();
    let challenge = Challenge {
        code: captcha.text().to_lowercase(),
        time: now / SECOND,
    };

    let challenge = with_state(|state| challenge.sign_to_base64(&state.captcha_secret))?;
    Ok(CaptchaOutput {
        img_base64: captcha.to_base64(0),
        challenge,
    })
}

#[ic_cdk::update]
#[candid_method(update)]
async fn airdrop_claim(args: AirdropClaimInput) -> Result<(), String> {
    let user = ic_cdk::caller();
    if user == ANONYMOUS {
        return Err("anonymous user is not allowed".to_string());
    }

    let expire_at = ic_cdk::api::time() - CAPTCHA_EXPIRE;
    let _ = with_state(|state| {
        Challenge::verify_from_base64(
            &state.captcha_secret,
            &args.code.to_lowercase(),
            &args.challenge,
            expire_at / SECOND,
        )
    })?;

    // TODO: claim airdrop

    Ok(())
}

ic_cdk::export_candid!();
