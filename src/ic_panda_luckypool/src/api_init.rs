use crate::{ecdsa, store, utils::mac_256};
use std::time::Duration;

#[ic_cdk::init]
fn init() {
    store::state::save();

    ic_cdk_timers::set_timer(Duration::from_nanos(0), || ic_cdk::spawn(init_secret()));
}

#[ic_cdk::pre_upgrade]
pub fn pre_upgrade() {
    store::state::save();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    store::state::load();

    ic_cdk_timers::set_timer(Duration::from_nanos(0), || ic_cdk::spawn(init_secret()));
}

async fn init_secret() {
    // can't be used in `init` and `post_upgrade`
    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .expect("failed to get random bytes");

    store::captcha::set_secret(mac_256(&rr.0, b"CAPTCHA_SECRET"));

    let token = ecdsa::sign_token(ic_cdk::api::id().as_slice().to_vec())
        .await
        .expect("failed to sign token");
    let pub_key = ecdsa::sign_token_key()
        .await
        .expect("failed to get public key");
    store::access_token::set_token(token, pub_key);
}
