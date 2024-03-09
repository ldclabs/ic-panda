use crate::{store, utils::mac_256};
use std::time::Duration;

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

async fn load_captcha_secret() {
    // can't be used in `init` and `post_upgrade`
    let rr = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .expect("failed to get random bytes");

    store::captcha::set_secret(mac_256(&rr.0, b"Captcha Secret"));
}
