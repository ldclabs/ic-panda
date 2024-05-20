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
fn admin_load_model(args: types::LoadModelInput) -> Result<types::LoadModelOutput, String> {
    let _ = store::unload_ai();

    let config_json = store::fs::get_full_chunks(args.config_id)?;
    let tokenizer_json = store::fs::get_full_chunks(args.tokenizer_id)?;
    let model_safetensors = store::fs::get_full_chunks(args.model_id)?;
    let mut output = types::LoadModelOutput {
        load_file_instructions: ic_cdk::api::performance_counter(1),
        load_mode_instructions: 0,
        total_instructions: 0,
    };

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

    output.load_mode_instructions =
        ic_cdk::api::performance_counter(1) - output.load_file_instructions;

    store::state::with_mut(|s| {
        s.ai_config = args.config_id;
        s.ai_tokenizer = args.tokenizer_id;
        s.ai_model = args.model_id;
    });

    output.total_instructions = ic_cdk::api::performance_counter(1);
    Ok(output)
}
