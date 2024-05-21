use crate::{store, types};

#[ic_cdk::init]
fn init() {
    store::state::save();
    store::init_rand();
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    store::state::save();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    let s = store::state::load();
    store::init_rand();
    store::load_model(&types::LoadModelInput {
        config_id: s.ai_config,
        tokenizer_id: s.ai_tokenizer,
        model_id: s.ai_model,
    })
    .ok();
}
