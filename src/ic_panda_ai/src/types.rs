use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::collections::BTreeSet;

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
    pub seed: Option<u64>,
}

#[derive(CandidType, Clone, Serialize)]
pub struct ChatOutput {
    pub message: String,
    pub tokens: u32,
    pub instructions: u64,
}

#[derive(CandidType, Clone, Serialize)]
pub struct LoadModelOutput {
    pub load_file_instructions: u64,
    pub load_mode_instructions: u64,
    pub total_instructions: u64,
}

#[derive(CandidType, Clone, Serialize)]
pub struct StateInfo {
    pub chat_count: u64,
    pub ai_config: u32,
    pub ai_tokenizer: u32,
    pub ai_model: u32,
    pub file_id: u32,
    pub max_file_size: u64,
    pub visibility: u8, // 0: private; 1: public
    pub total_files: u64,
    pub total_chunks: u64,
    pub managers: BTreeSet<Principal>,
}
