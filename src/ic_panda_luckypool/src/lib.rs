use candid::{candid_method, types::number::Nat, Principal};
use ic_captcha::CaptchaBuilder;
use once_cell::sync::Lazy;
use std::{
    convert::{From, Into},
    time::Duration,
};

use icrc_ledger_types::{
    icrc1::account::Account,
    icrc1::transfer::{TransferArg, TransferError},
};

mod crypto;
mod store;
mod types;

use crypto::mac_256;
use store::{
    active_user, deactive_user, get_airdrop, set_airdrop, set_captcha_secret, with_captcha_secret,
};
use types::{AirdropClaimInput, CaptchaOutput, Challenge};

const SECOND: u64 = 1_000_000_000;
const CAPTCHA_EXPIRE: u64 = SECOND * 60 * 5;

const TOKEN_1: u64 = 100_000_000;
const AIRDROP_AMOUNT: u64 = TOKEN_1 * 10;

static ANONYMOUS: Principal = Principal::anonymous();
static TOKEN_CANISTER: Lazy<Principal> =
    Lazy::new(|| Principal::from_text("bkyz2-fmaaa-aaaaa-qaaaq-cai").expect("invalid principal"));
static CAPTCHA_BUILDER: Lazy<CaptchaBuilder> = Lazy::new(CaptchaBuilder::new);

#[ic_cdk::init]
fn init() {
    ic_cdk_timers::set_timer(Duration::from_millis(1), || {
        ic_cdk::spawn(load_captcha_secret())
    });
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    ic_cdk_timers::set_timer(Duration::from_millis(1), || {
        ic_cdk::spawn(load_captcha_secret())
    });
}

async fn load_captcha_secret() {
    // can't be used in `init` and `post_upgrade`
    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .expect("failed to get random bytes");

    set_captcha_secret(mac_256(&rr.0, b"Captcha Secret"));
    ic_cdk::api::print("Captcha secret loaded\n");
}

#[ic_cdk::query]
#[candid_method(query)]
fn debug() -> String {
    "OK".to_string()
}

#[ic_cdk::query]
#[candid_method(query)]
fn whoami() -> Principal {
    ic_cdk::caller()
}

#[ic_cdk::query]
#[candid_method(query)]
async fn airdrop() -> Result<Option<Nat>, ()> {
    let user = ic_cdk::caller();
    if user == ANONYMOUS {
        return Ok(None);
    }

    let res = get_airdrop(user);
    Ok(res.map(Nat::from))
}

#[ic_cdk::query(composite = true)]
#[candid_method(query)]
async fn lucky_pool_balance() -> Result<Nat, String> {
    token_balance_of(ic_cdk::id()).await
}

#[ic_cdk::update(guard = "is_authenticated")]
#[candid_method(update)]
async fn captcha() -> Result<CaptchaOutput, String> {
    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .map_err(|_err| "failed to get random bytes".to_string())?;

    let captcha = CAPTCHA_BUILDER.generate(&rr.0, None);
    let now = ic_cdk::api::time();
    let challenge = Challenge {
        code: captcha.text().to_lowercase(),
        time: now / SECOND,
    };

    let challenge = with_captcha_secret(|secret| challenge.sign_to_base64(secret))?;
    Ok(CaptchaOutput {
        img_base64: captcha.to_base64(0),
        challenge,
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
#[candid_method(update)]
async fn claim_airdrop(args: AirdropClaimInput) -> Result<Nat, String> {
    let expire_at = ic_cdk::api::time() - CAPTCHA_EXPIRE;
    let _ = with_captcha_secret(|secret| {
        Challenge::verify_from_base64(
            secret,
            &args.code.to_lowercase(),
            &args.challenge,
            expire_at / SECOND,
        )
    })?;

    let user = ic_cdk::caller();
    if let Some(n) = get_airdrop(user) {
        return Ok(Nat::from(n));
    }

    if !active_user(user) {
        return Err("try again later".to_string());
    }

    let _guard = scopeguard::guard((), |_| {
        deactive_user(user);
    });

    let _block_idx = token_transfer_to(user, AIRDROP_AMOUNT).await?;
    set_airdrop(user, AIRDROP_AMOUNT);
    Ok(Nat::from(AIRDROP_AMOUNT))
}

fn is_authenticated() -> Result<(), String> {
    if ic_cdk::caller() == ANONYMOUS {
        Err("anonymous user is not allowed".to_string())
    } else {
        Ok(())
    }
}

async fn token_balance_of(user: Principal) -> Result<Nat, String> {
    let (balance,) = ic_cdk::call(
        *TOKEN_CANISTER,
        "icrc1_balance_of",
        (Account {
            owner: user,
            subaccount: None,
        },),
    )
    .await
    .map_err(|err| format!("failed to get balance, error {:?}", err))?;
    Ok(balance)
}

async fn token_transfer_to(user: Principal, amount: u64) -> Result<Nat, String> {
    let (res,): (Result<Nat, TransferError>,) = ic_cdk::call(
        *TOKEN_CANISTER,
        "icrc1_transfer",
        (TransferArg {
            from_subaccount: None,
            to: Account {
                owner: user,
                subaccount: None,
            },
            fee: None,
            created_at_time: None,
            memo: None,
            amount: Nat::from(amount),
        },),
    )
    .await
    .map_err(|err| format!("failed to call icrc1_transfer, error {:?}", err))?;
    res.map_err(|err| format!("failed to transfer, error {:?}", err))
}

ic_cdk::export_candid!();
