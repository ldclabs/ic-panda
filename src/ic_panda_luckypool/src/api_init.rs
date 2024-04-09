use crate::store;
use std::time::Duration;

#[ic_cdk::init]
fn init() {
    store::state::save();

    ic_cdk_timers::set_timer(Duration::from_nanos(0), || {
        ic_cdk::spawn(store::keys::load())
    });
}

#[ic_cdk::pre_upgrade]
pub fn pre_upgrade() {
    store::keys::save();
    store::state::save();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    store::state::load();

    ic_cdk_timers::set_timer(Duration::from_nanos(0), || {
        ic_cdk::spawn(store::keys::load())
    });
}
