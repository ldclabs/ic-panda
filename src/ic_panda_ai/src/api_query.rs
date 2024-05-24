use ic_oss_types::file::FileInfo;
use lib_panda::sha3_256;
use serde_bytes::ByteBuf;
use serde_json::json;

use crate::{ai, store, types, unwrap_trap};

#[ic_cdk::query]
fn api_version() -> u16 {
    1
}

#[ic_cdk::query]
fn state() -> Result<store::State, ()> {
    Ok(store::state::with(|r| r.clone()))
}

#[ic_cdk::query]
fn get_file_info(id: u32, _access_token: Option<ByteBuf>) -> Result<FileInfo, String> {
    match store::fs::get_file(id) {
        Some(meta) => Ok(meta.into_info(id)),
        None => Err("file not found".to_string()),
    }
}

#[ic_cdk::query]
fn list_files(
    _parent: u32,
    prev: Option<u32>,
    take: Option<u32>,
    _access_token: Option<ByteBuf>,
) -> Vec<FileInfo> {
    let max_prev = store::state::with(|s| s.file_id).saturating_add(1);
    let prev = prev.unwrap_or(max_prev).min(max_prev);
    let take = take.unwrap_or(10).min(100);
    store::fs::list_files(prev, take)
}

#[ic_cdk::query]
async fn chat(args: types::ChatInput) -> Result<types::ChatOutput, String> {
    let msg = json!([{
        "role": "system",
        "content": "You are a giant panda with human intelligence, the best friend and assistant to humans, named \"Panda Oracle\", born on the Internet Computer (ICP).",
    }, {
        "role": "user",
        "content": args.prompt,
    }]);

    let seed = args.seed.unwrap_or_else(|| {
        u64::from_be_bytes(sha3_256(ic_cdk::id().as_slice())[..8].try_into().unwrap())
    });
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

    Ok(types::ChatOutput {
        message: String::from_utf8(w).map_err(|err| format!("{:?}", err))?,
        instructions: ic_cdk::api::performance_counter(1),
        tokens,
    })
}
