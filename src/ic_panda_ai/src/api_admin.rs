use candid::Principal;
use std::collections::BTreeSet;

use crate::{ai, is_controller, is_controller_or_manager, store, types, ANONYMOUS};

#[ic_cdk::update(guard = "is_controller")]
fn admin_set_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    store::state::with_mut(|r| {
        r.managers = args;
    });
    Ok(())
}

#[ic_cdk::update]
fn validate_admin_set_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    if args.is_empty() {
        return Err("managers cannot be empty".to_string());
    }
    if args.contains(&ANONYMOUS) {
        return Err("anonymous user is not allowed".to_string());
    }
    Ok(())
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
async fn admin_load_model(args: types::LoadModelInput) -> Result<(), String> {
    let config_json = store::fs::get_full_chunks(args.config_id)?;
    let tokenizer_json = store::fs::get_full_chunks(args.tokenizer_id)?;
    let model_safetensors = store::fs::get_full_chunks(args.model_id)?;

    store::load_ai(
        &ai::Args {
            temperature: Some(0.618),
            top_p: Some(0.382),
            seed: 299792458,
            sample_len: 1024,
            repeat_penalty: 1.1,
            repeat_last_n: 64,
        },
        &config_json,
        &tokenizer_json,
        model_safetensors,
    )?;

    store::state::with_mut(|s| {
        s.ai_config = args.config_id;
        s.ai_tokenizer = args.tokenizer_id;
        s.ai_model = args.model_id;
    });

    Ok(())
}
