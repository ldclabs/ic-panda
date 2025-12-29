use std::time::Duration;

use crate::store;

#[ic_cdk::init]
fn init() {
    store::state::save();

    ic_cdk_timers::set_timer(Duration::from_nanos(0), store::keys::load());
}

#[ic_cdk::pre_upgrade]
pub fn pre_upgrade() {
    store::keys::save();
    store::state::save();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    store::state::load();

    ic_cdk_timers::set_timer(Duration::from_nanos(0), store::keys::load());

    // canister_global_timer can not support unbounded type!!!
    ic_cdk_timers::set_timer_interval(Duration::from_secs(2 * 60), || async {
        store::prize::handle_refund_jobs();
    });
}
