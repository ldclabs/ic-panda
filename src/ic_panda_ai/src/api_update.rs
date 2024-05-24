use candid::Nat;
use ic_oss_types::file::{
    CreateFileInput, CreateFileOutput, UpdateFileChunkInput, UpdateFileChunkOutput,
    UpdateFileInput, UpdateFileOutput, MAX_CHUNK_SIZE,
};
use serde_bytes::ByteBuf;
use serde_json::json;

use crate::{
    ai, is_authenticated, is_controller_or_manager, store, types, unwrap_trap, MILLISECONDS,
};

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

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn create_file(
    input: CreateFileInput,
    _access_token: Option<ByteBuf>,
) -> Result<CreateFileOutput, String> {
    // use trap to make the update fail.
    unwrap_trap(input.validate(), "invalid CreateFileInput");

    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    let id = unwrap_trap(
        store::fs::add_file(store::FileMetadata {
            name: input.name,
            content_type: input.content_type,
            hash: input.hash,
            created_at: now_ms,
            ..Default::default()
        }),
        "failed to add file",
    );
    let mut output = CreateFileOutput {
        id,
        chunks_crc32: Vec::new(),
        created_at: Nat::from(now_ms),
    };

    if let Some(content) = input.content {
        for (i, chunk) in content.chunks(MAX_CHUNK_SIZE as usize).enumerate() {
            let (_, crc32) = unwrap_trap(
                store::fs::update_chunk(id, i as u32, now_ms, chunk.to_vec()),
                "failed to update chunk",
            );
            output.chunks_crc32.push(crc32);
        }
    }

    Ok(output)
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn update_file_info(
    input: UpdateFileInput,
    _access_token: Option<ByteBuf>,
) -> Result<UpdateFileOutput, String> {
    unwrap_trap(input.validate(), "invalid UpdateFileInput");

    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    let res = store::fs::update_file(input.id, |metadata| {
        if let Some(name) = input.name {
            metadata.name = name;
        }
        if let Some(content_type) = input.content_type {
            metadata.content_type = content_type;
        }
        if input.hash.is_some() {
            metadata.hash = input.hash;
        }
    });

    match res {
        Some(_) => Ok(UpdateFileOutput {
            updated_at: Nat::from(now_ms),
        }),
        None => ic_cdk::trap("file not found"),
    }
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn update_file_chunk(
    input: UpdateFileChunkInput,
    _access_token: Option<ByteBuf>,
) -> Result<UpdateFileChunkOutput, String> {
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    let (_, crc32) = unwrap_trap(
        store::fs::update_chunk(
            input.id,
            input.chunk_index,
            now_ms,
            input.content.into_vec(),
        ),
        "failed to add update chunk",
    );

    Ok(UpdateFileChunkOutput {
        crc32,
        updated_at: Nat::from(now_ms),
    })
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn delete_file(id: u32, _access_token: Option<ByteBuf>) -> Result<(), String> {
    store::fs::delete_file(id).map_err(|err| ic_cdk::trap(&err))
}
