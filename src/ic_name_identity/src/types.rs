use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

#[derive(CandidType, Clone, Deserialize, Serialize)]
pub struct NameAccount {
    pub name: String,
    pub account: Principal,
}

#[derive(CandidType, Clone, Deserialize, Serialize)]
pub struct Delegator {
    pub owner: Principal,
    pub sign_in_at: u64, // milliseconds since epoch
    pub role: i8,        // -1: suspend; 0: member; 1: owner
}
