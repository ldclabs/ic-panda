use crate::store;

#[ic_cdk::init]
fn init() {
    store::state::save();
    store::init_rand();
}

#[ic_cdk::pre_upgrade]
pub fn pre_upgrade() {
    store::state::save();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    store::state::load();
    store::init_rand();
}
