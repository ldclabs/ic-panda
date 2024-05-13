use candid::Nat;
use serde_bytes::ByteBuf;

use crate::{nat_to_u64, store, types};

#[ic_cdk::query]
fn api_version() -> u16 {
    1
}

#[ic_cdk::query]
fn state() -> Result<store::State, ()> {
    Ok(store::state::with(|r| r.clone()))
}

#[ic_cdk::query]
async fn file_meta(id: u32) -> Result<types::FileMetadataOutput, String> {
    match store::fs::get_file(id) {
        Some(meta) => Ok(types::FileMetadataOutput {
            id,
            name: meta.name,
            size: meta.size as u32,
            content_type: meta.content_type,
            created_at: Nat::from(meta.created_at),
            updated_at: Nat::from(meta.updated_at),
            chunks: meta.chunks,
            filled_size: meta.filled_size as u32,
            hash: meta.hash.map(ByteBuf::from),
        }),
        None => Err("file not found".to_string()),
    }
}

#[ic_cdk::query]
async fn files(prev: Option<Nat>, take: Option<Nat>) -> Vec<types::FileMetadataOutput> {
    let max_prev = store::state::with(|s| s.file_id).saturating_add(1) as u64;
    let prev = prev
        .as_ref()
        .map(nat_to_u64)
        .unwrap_or(max_prev)
        .min(max_prev) as u32;
    let take = take.as_ref().map(nat_to_u64).unwrap_or(10).min(100) as u32;
    store::fs::list_files(prev, take)
}
