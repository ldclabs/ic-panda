use candid::Nat;
use ic_oss_types::file::FileInfo;
use serde_bytes::ByteBuf;

use crate::{nat_to_u64, store};

#[ic_cdk::query]
fn api_version() -> u16 {
    1
}

#[ic_cdk::query]
fn state() -> Result<store::State, ()> {
    Ok(store::state::with(|r| r.clone()))
}

#[ic_cdk::query]
fn file_meta(id: u32) -> Result<FileInfo, String> {
    match store::fs::get_file(id) {
        Some(meta) => Ok(FileInfo {
            id,
            parent: meta.parent,
            name: meta.name,
            content_type: meta.content_type,
            size: Nat::from(meta.size),
            filled: Nat::from(meta.filled),
            created_at: Nat::from(meta.created_at),
            updated_at: Nat::from(meta.updated_at),
            chunks: meta.chunks,
            hash: meta.hash.map(ByteBuf::from),
            status: meta.status,
        }),
        None => Err("file not found".to_string()),
    }
}

#[ic_cdk::query]
fn files(prev: Option<Nat>, take: Option<Nat>) -> Vec<FileInfo> {
    let max_prev = store::state::with(|s| s.file_id).saturating_add(1) as u64;
    let prev = prev
        .as_ref()
        .map(nat_to_u64)
        .unwrap_or(max_prev)
        .min(max_prev) as u32;
    let take = take.as_ref().map(nat_to_u64).unwrap_or(10).min(100) as u32;
    store::fs::list_files(prev, take)
}
