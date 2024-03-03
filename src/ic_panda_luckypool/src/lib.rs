use candid::{candid_method, Nat, Principal};
use ic_captcha::CaptchaBuilder;
use num_traits::cast::ToPrimitive;
use once_cell::sync::Lazy;
use std::{
    convert::{From, Into},
    time::Duration,
};

use icrc_ledger_types::{
    icrc1::account::Account,
    icrc1::transfer::{TransferArg, TransferError},
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};

mod crypto;
mod store;
mod types;

use crypto::mac_256;

const SECOND: u64 = 1_000_000_000;
const CAPTCHA_EXPIRE_SEC: u64 = 60 * 5;

const TRANS_FEE: u64 = 10_000;
const TOKEN_1: u64 = 100_000_000;
const ICP_1: u64 = ic_ledger_types::Tokens::SUBDIVIDABLE_BY;
const AIRDROP_AMOUNT: u64 = TOKEN_1 * 10;
const LOWEST_LUCKYDRAW_BALANCE: u64 = TOKEN_1 * 100;

// 344693032001 from b"PANDA"
const LUCKYDRAW_DIVISOR: u64 = u64::from_be_bytes([0, 0, 0, b'P', b'A', b'N', b'D', b'A']);

static ANONYMOUS: Principal = Principal::anonymous();
static ICP_CANISTER: Principal = ic_ledger_types::MAINNET_LEDGER_CANISTER_ID;
static TOKEN_CANISTER: Lazy<Principal> =
    Lazy::new(|| Principal::from_text("bkyz2-fmaaa-aaaaa-qaaaq-cai").expect("invalid principal")); // TODO: update token canister id
static DAO_CANISTER: Lazy<Principal> =
    Lazy::new(|| Principal::from_text("a7cug-2qaaa-aaaap-ab3la-cai").expect("invalid principal")); // TODO: update dao canister id
static CAPTCHA_BUILDER: Lazy<CaptchaBuilder> = Lazy::new(CaptchaBuilder::new);

#[ic_cdk::init]
fn init() {
    ic_cdk_timers::set_timer(Duration::from_nanos(0), || {
        ic_cdk::spawn(load_captcha_secret())
    });
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    ic_cdk_timers::set_timer(Duration::from_nanos(0), || {
        ic_cdk::spawn(load_captcha_secret())
    });
}

#[ic_cdk::query]
#[candid_method]
fn debug() -> String {
    "OK".to_string()
}

#[ic_cdk::query]
#[candid_method]
fn whoami() -> Result<Principal, ()> {
    Ok(ic_cdk::caller())
}

#[ic_cdk::query]
#[candid_method]
fn state() -> Result<store::State, ()> {
    Ok(store::state::get())
}

#[ic_cdk::query(composite = true)]
#[candid_method]
async fn lucky_pool_balance() -> Result<Nat, String> {
    token_balance_of(ic_cdk::id()).await
}

#[ic_cdk::update(guard = "is_authenticated")]
#[candid_method]
async fn captcha() -> Result<types::CaptchaOutput, String> {
    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .map_err(|_err| "failed to get random bytes".to_string())?;

    let captcha = CAPTCHA_BUILDER.generate(&rr.0, None);
    let now = ic_cdk::api::time();
    let challenge = types::Challenge {
        code: captcha.text().to_lowercase(),
        time: now / SECOND,
    };

    let challenge = store::captcha::with_secret(|secret| challenge.sign_to_base64(secret))?;
    Ok(types::CaptchaOutput {
        img_base64: captcha.to_base64(0),
        challenge,
    })
}

#[ic_cdk::query]
#[candid_method]
async fn airdrop_status() -> Result<bool, ()> {
    let user = ic_cdk::caller();
    if user == ANONYMOUS {
        return Ok(false);
    }

    Ok(store::airdrop::has(user))
}

#[ic_cdk::query]
#[candid_method]
async fn airdrop_total() -> Result<u64, ()> {
    Ok(store::airdrop::total())
}

#[ic_cdk::query]
#[candid_method]
async fn luckydraw_total() -> Result<u64, ()> {
    Ok(store::luckydraw::total())
}

#[ic_cdk::query]
#[candid_method]
async fn airdrop_logs(args: types::LogsInput) -> Result<types::LogsOutput<store::AirdropLog>, ()> {
    let res = store::airdrop::logs(10, args.index);
    Ok(types::LogsOutput {
        next_index: res.1,
        logs: res.0,
    })
}

#[ic_cdk::query]
#[candid_method]
async fn luckydraw_logs(
    args: types::LogsInput,
) -> Result<types::LogsOutput<store::LuckyDrawLog>, ()> {
    let res = store::luckydraw::logs(10, args.index);
    Ok(types::LogsOutput {
        next_index: res.1,
        logs: res.0,
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
#[candid_method]
async fn claim_airdrop(args: types::AirdropClaimInput) -> Result<Nat, String> {
    let now = ic_cdk::api::time() / SECOND;
    let expire_at = now - CAPTCHA_EXPIRE_SEC;
    let _ = store::captcha::with_secret(|secret| {
        types::Challenge::verify_from_base64(
            secret,
            &args.code.to_lowercase(),
            &args.challenge,
            expire_at,
        )
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
    ic_cdk::api::print(format!("claim_airdrop: {:?}\n", _res));

    Ok(Nat::from(AIRDROP_AMOUNT))
}

#[ic_cdk::update(guard = "is_authenticated")]
#[candid_method]
async fn luckydraw(args: types::LuckyDrawInput) -> Result<Nat, String> {
    if args.icp < 1 || args.icp > 10 {
        return Err("invalid icp amount".to_string());
    }

    let user = ic_cdk::caller();
    if !store::user::active(user) {
        return Err("try again later".to_string());
    }
    let _guard = scopeguard::guard((), |_| {
        store::user::deactive(user);
    });

    let balance = token_balance_of(ic_cdk::id()).await?;
    if balance < LOWEST_LUCKYDRAW_BALANCE * args.icp as u64 + TRANS_FEE {
        return Err("insufficient token balance for luckydraw".to_string());
    }

    let now = ic_cdk::api::time() / SECOND;
    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .map_err(|_err| "failed to get random bytes".to_string())?;
    let icp = args.icp as u64 * ICP_1;
    let (x, amount) = luckydraw_amount(&rr.0);

    let _ = icp_transfer_from(user, Nat::from(icp)).await?;
    let balance = token_balance_of(ic_cdk::id()).await?;
    let balance: u64 = balance.0.to_u64().unwrap_or(0).saturating_sub(TRANS_FEE);
    let amount = args.icp as u64 * amount;
    let amount = if balance < amount { balance } else { amount };
    if amount > 0 {
        let _ = token_transfer_to(user, Nat::from(amount)).await?;
    }

    let _res = store::luckydraw::update(user, now, amount, icp, x);
    // TODO: trace error logs
    ic_cdk::api::print(format!("claim_airdrop: {:?}\n", _res));

    Ok(Nat::from(amount))
}

fn is_authenticated() -> Result<(), String> {
    if ic_cdk::caller() == ANONYMOUS {
        Err("anonymous user is not allowed".to_string())
    } else {
        Ok(())
    }
}

fn luckydraw_amount(rnd32: &[u8]) -> (u64, u64) {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&rnd32[0..8]);
    let x = u64::from_be_bytes(bytes);
    let a = x % LUCKYDRAW_DIVISOR;
    let amount = match a / TOKEN_1 {
        v if v <= 5 => 100000 * TOKEN_1,
        v if v <= 1000 => 1000 * TOKEN_1,
        v => v * TOKEN_1,
    };
    (x, amount)
}

async fn load_captcha_secret() {
    // can't be used in `init` and `post_upgrade`
    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .expect("failed to get random bytes");

    store::captcha::set_secret(mac_256(&rr.0, b"Captcha Secret"));
    ic_cdk::api::print("Captcha secret loaded\n");
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
    .map_err(|err| format!("failed to get balance, error: {:?}", err))?;
    Ok(balance)
}

async fn token_transfer_to(user: Principal, amount: Nat) -> Result<Nat, String> {
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
            amount,
        },),
    )
    .await
    .map_err(|err| format!("failed to call icrc1_transfer, error: {:?}", err))?;
    res.map_err(|err| format!("failed to transfer tokens, error: {:?}", err))
}

async fn icp_transfer_from(user: Principal, amount: Nat) -> Result<Nat, String> {
    let (res,): (Result<Nat, TransferFromError>,) = ic_cdk::call(
        ICP_CANISTER,
        "icrc2_transfer_from",
        (TransferFromArgs {
            spender_subaccount: None,
            from: Account {
                owner: user,
                subaccount: None,
            },
            to: Account {
                owner: *DAO_CANISTER,
                subaccount: None,
            },
            fee: None,
            created_at_time: None,
            memo: None,
            amount,
        },),
    )
    .await
    .map_err(|err| format!("failed to call icrc2_transfer_from, error: {:?}", err))?;
    res.map_err(|err| format!("failed to transfer ICP from user, error: {:?}", err))
}

ic_cdk::export_candid!();
