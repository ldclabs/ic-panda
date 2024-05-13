use candid::{CandidType, Nat};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

#[derive(CandidType, Clone, Deserialize)]
pub struct LoadModelInput {
    pub config_id: u32,
    pub tokenizer_id: u32,
    pub model_id: u32,
}

#[derive(CandidType, Clone, Deserialize)]
pub struct ChatInput {
    pub prompt: String,
    pub messages: Option<Vec<String>>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub challenge: Option<ByteBuf>,
}

#[derive(CandidType, Clone, Serialize)]
pub struct ChatOutput {
    pub message: String,
    pub tokens: u32,
}

#[derive(CandidType, Clone, Serialize)]
pub struct FileMetadataOutput {
    pub id: u32,
    pub name: String,
    pub size: u32,
    pub content_type: String,
    pub created_at: Nat, // unix timestamp in seconds
    pub updated_at: Nat, // unix timestamp in seconds
    pub chunks: u32,
    pub filled_size: u32,
    pub hash: Option<ByteBuf>,
}
