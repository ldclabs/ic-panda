use serde_json::json;

use crate::{ai, is_authenticated, store, types, unwrap_trap};

#[ic_cdk::update(guard = "is_authenticated")]
async fn update_chat(args: types::ChatInput) -> Result<types::ChatOutput, String> {
    let msg = json!([{
        "role": "system",
        "content": "You are a giant panda with human intelligence, the best friend and assistant to humans, named \"Panda Oracle\", born on the Internet Computer (ICP).",
    }, {
        "role": "user",
        "content": args.prompt,
    }]);

    let seed = args.seed.unwrap_or_else(ic_cdk::api::time);
    let sample_len = args.max_tokens.unwrap_or(1024).min(4096) as usize;
    let mut w = Vec::new();
    let tokens = unwrap_trap(
        store::run_ai(
            &ai::Args {
                temperature: Some(0.618),
                top_p: Some(0.382),
                seed,
                sample_len,
                repeat_penalty: 1.1,
                repeat_last_n: 64,
            },
            &unwrap_trap(serde_json::to_string(&msg), "failed to serialize prompt"),
            sample_len,
            &mut w,
        ),
        "failed to run AI",
    );

    store::state::with_mut(|s| {
        s.chat_count = s.chat_count.saturating_add(1);
    });

    Ok(types::ChatOutput {
        message: String::from_utf8(w).map_err(|err| format!("{:?}", err))?,
        instructions: ic_cdk::api::performance_counter(1),
        tokens,
    })
}
