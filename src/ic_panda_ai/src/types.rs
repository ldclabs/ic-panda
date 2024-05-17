use candid::CandidType;
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
    pub instructions: u64,
}

#[derive(CandidType, Clone, Serialize)]
pub struct LoadModelOutput {
    pub load_file_instructions: u64,
    pub load_mode_instructions: u64,
    pub total_instructions: u64,
}
