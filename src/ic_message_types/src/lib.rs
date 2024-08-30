use candid::Principal;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteArray;

pub mod channel;
pub mod profile;

#[derive(Clone, Deserialize, Serialize)]
pub struct NameBlock {
    #[serde(rename = "h")]
    pub height: u64,
    #[serde(rename = "p")]
    pub phash: ByteArray<32>,
    #[serde(rename = "n")]
    pub name: String,
    #[serde(rename = "u")]
    pub user: Principal,
    #[serde(rename = "f")]
    pub from: Option<Principal>,
    #[serde(rename = "v")]
    pub value: u64,
    #[serde(rename = "t")]
    pub timestamp: u64, // milliseconds
}
