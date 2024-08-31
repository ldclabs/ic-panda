use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub use ic_message_types::channel::*;

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct StateInfo {
    pub name: String,
    pub managers: BTreeSet<Principal>,
    pub channel_id: u32,
    pub channels_total: u64,
    pub messages_total: u64,
    pub incoming_gas: u128,
    pub burned_gas: u128,
}
