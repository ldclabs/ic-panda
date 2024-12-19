use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteArray;
use std::collections::BTreeSet;

pub const TOKEN_1: u64 = 100_000_000;
pub const TOKEN_FEE: u64 = 10_000; // 0.0001 token
pub const MIN_NAME_PRICE: u64 = TOKEN_1;
pub const MAX_DISPLAY_NAME_SIZE: usize = 32;
pub const MAX_USER_NAME_SIZE: usize = 20;

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct StateInfo {
    pub name: String,
    pub managers: BTreeSet<Principal>,
    pub cose_canisters: Vec<Principal>,
    pub profile_canisters: Vec<Principal>,
    pub channel_canisters: Vec<Principal>,
    pub matured_channel_canisters: BTreeSet<Principal>,
    pub price: Price,
    pub names_total: u64,
    pub users_total: u64,
    pub incoming_total: u128,
    pub transfer_out_total: u128,
    pub next_block_height: u64,
    pub next_block_phash: ByteArray<32>,
    pub latest_usernames: Vec<String>,
}

#[derive(CandidType, Clone, Debug, Default, Deserialize, Serialize)]
pub struct Price {
    pub channel: u64, // price to create a channel
    pub name_l1: u64, // price to register a namespace in COSE service
    pub name_l2: u64,
    pub name_l3: u64,
    pub name_l5: u64,
    pub name_l7: u64,
}

impl Price {
    pub fn get(&self, level: usize) -> u64 {
        match level {
            1 => self.name_l1,
            2 => self.name_l2,
            3 | 4 => self.name_l3,
            5 | 6 => self.name_l5,
            _ => self.name_l7,
        }
    }
}

#[derive(CandidType, Copy, Clone, Debug, Deserialize, Serialize)]
pub enum CanisterKind {
    Cose,
    Profile,
    Channel,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct UpdatePriceInput {
    pub channel: Option<u64>,
    pub name_l1: Option<u64>, // price to register a namespace in COSE service
    pub name_l2: Option<u64>,
    pub name_l3: Option<u64>,
    pub name_l5: Option<u64>,
    pub name_l7: Option<u64>,
}

impl UpdatePriceInput {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(price) = self.name_l1 {
            if price < MIN_NAME_PRICE {
                return Err(format!("name price too low: {}", price));
            }
        }
        if let Some(price) = self.name_l2 {
            if price < MIN_NAME_PRICE {
                return Err(format!("name price too low: {}", price));
            }
        }
        if let Some(price) = self.name_l3 {
            if price < MIN_NAME_PRICE {
                return Err(format!("name price too low: {}", price));
            }
        }
        if let Some(price) = self.name_l5 {
            if price < MIN_NAME_PRICE {
                return Err(format!("name price too low: {}", price));
            }
        }
        if let Some(price) = self.name_l7 {
            if price < MIN_NAME_PRICE {
                return Err(format!("name price too low: {}", price));
            }
        }
        Ok(())
    }
}
