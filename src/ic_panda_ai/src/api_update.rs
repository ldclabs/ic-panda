use crate::{is_authenticated, store, types, unwrap_trap};

use serde_json::json;

#[ic_cdk::update(guard = "is_authenticated")]
async fn chat(args: types::ChatInput) -> Result<types::ChatOutput, String> {
    let msg = json!([{
        "role": "system",
        "content": "You are a giant panda prophet with top-level intelligence, the best friend and assistant to humans.",
    },{
        "role": "user",
        "content": args.prompt,
    }]);
    let mut w = Vec::new();
    let tokens = unwrap_trap(
        store::run_ai(
            &unwrap_trap(serde_json::to_string(&msg), "failed to serialize prompt"),
            args.max_tokens.unwrap_or(1024).min(4096) as usize,
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
