use candid::{CandidType, Nat};
use serde::Deserialize;
use serde_bytes::ByteBuf;
use std::collections::HashMap;

use crate::{is_controller_or_manager, nat_to_u64, store, MILLISECONDS};

// Compatible API with ic-asserts, @dfinity/assets

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct StoreArg {
    pub key: String,
    pub content_type: String,
    pub content_encoding: String, // not used
    pub content: ByteBuf,
    pub sha256: Option<ByteBuf>,
    pub aliased: Option<bool>, // not used
}

fn unwrap_hash(v: Option<ByteBuf>) -> Option<[u8; 32]> {
    v.and_then(|v| {
        if v.len() == 32 {
            let mut hash = [0; 32];
            hash.copy_from_slice(&v[..]);
            Some(hash)
        } else {
            None
        }
    })
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn store(arg: StoreArg) {
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    let id = store::fs::add_file(store::FileMetadata {
        name: arg.key,
        content_type: arg.content_type,
        hash: unwrap_hash(arg.sha256),
        created_at: now_ms,
        ..Default::default()
    })
    .map_err(|err| ic_cdk::trap(&format!("failed to add metadata: {}", err)))
    .unwrap();

    let _ = store::fs::append_chunk(id, now_ms, arg.content.into_vec())
        .map_err(|err| ic_cdk::trap(&format!("failed to add content: {}", err)));
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct CreateBatchResponse {
    pub batch_id: Nat,
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn create_batch() -> CreateBatchResponse {
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    let id = store::fs::add_file(store::FileMetadata {
        created_at: now_ms,
        ..Default::default()
    })
    .map_err(|err| ic_cdk::trap(&format!("failed to add metadata: {}", err)))
    .unwrap();

    CreateBatchResponse {
        batch_id: Nat::from(id as u64),
    }
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct CreateChunkArg {
    pub batch_id: Nat,
    pub content: ByteBuf,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct CreateChunkResponse {
    pub chunk_id: Nat,
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn create_chunk(arg: CreateChunkArg) -> CreateChunkResponse {
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    let (chunk_id, _) = store::fs::append_chunk(
        nat_to_u64(&arg.batch_id) as u32,
        now_ms,
        arg.content.into_vec(),
    )
    .map_err(|err| ic_cdk::trap(&format!("failed to add content: {}", err)))
    .unwrap();

    CreateChunkResponse {
        chunk_id: Nat::from(chunk_id as u64),
    }
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum BatchOperation {
    CreateAsset(CreateAssetArguments),
    SetAssetContent(SetAssetContentArguments),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct CreateAssetArguments {
    pub key: String,
    pub content_type: String,
    pub max_age: Option<u64>,                     // not used
    pub headers: Option<HashMap<String, String>>, // not used
    pub enable_aliasing: Option<bool>,            // not used
    pub allow_raw_access: Option<bool>,           // not used
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct SetAssetContentArguments {
    pub key: String,
    pub content_encoding: String, // not used
    pub chunk_ids: Vec<Nat>,      // not used
    pub sha256: Option<ByteBuf>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct CommitBatchArguments {
    pub batch_id: Nat,
    pub operations: Vec<BatchOperation>,
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn commit_batch(arg: CommitBatchArguments) {
    let now_ms = ic_cdk::api::time() / MILLISECONDS;

    let res = store::fs::update_file(nat_to_u64(&arg.batch_id) as u32, |meta| {
        meta.updated_at = now_ms;
        for op in arg.operations {
            match op {
                BatchOperation::CreateAsset(args) => {
                    if !args.key.is_empty() && meta.name.is_empty() {
                        meta.name = args.key;
                        meta.content_type = args.content_type;
                    }
                }
                BatchOperation::SetAssetContent(args) => {
                    if meta.hash.is_none() {
                        if let Some(hash) = unwrap_hash(args.sha256) {
                            meta.hash = Some(hash);
                        }
                    }
                }
            }
        }
    });
    if res.is_none() {
        ic_cdk::trap("batch not found");
    }
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct DeleteBatchArguments {
    pub batch_id: Nat,
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn delete_batch(arg: DeleteBatchArguments) {
    let _ = store::fs::delete_file(nat_to_u64(&arg.batch_id) as u32);
}
