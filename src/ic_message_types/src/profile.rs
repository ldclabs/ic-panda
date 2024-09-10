use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteArray;
use std::collections::{BTreeSet, HashMap};

pub const MAX_PROFILE_FOLLOWING: usize = 2048;
pub const MAX_PROFILE_BIO_SIZE: usize = 2048; // 2KB

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct UserInfo {
    pub id: Principal,
    pub name: String,
    pub image: String,
    pub profile_canister: Principal,
    pub cose_canister: Option<Principal>,
    pub username: Option<String>,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct ProfileInfo {
    pub id: Principal,
    pub canister: Principal,
    pub bio: String,
    pub active_at: u64,
    pub created_at: u64,
    pub following: Option<BTreeSet<Principal>>,
    pub channels: Option<HashMap<(Principal, u64), ChannelSetting>>,
    pub ecdh_pub: Option<ByteArray<32>>,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct ChannelSetting {
    pub pin: u32,
    pub alias: String,
    pub tags: BTreeSet<String>,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct UpdateProfileInput {
    pub bio: Option<String>,
    pub follow: BTreeSet<Principal>,
    pub unfollow: BTreeSet<Principal>,
    pub upsert_channels: HashMap<(Principal, u64), ChannelSetting>,
    pub remove_channels: BTreeSet<(Principal, u64)>,
}

impl UpdateProfileInput {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(bio) = &self.bio {
            if bio.len() > MAX_PROFILE_BIO_SIZE {
                return Err(format!("bio size limit exceeded: {}", bio.len()));
            }
        }
        Ok(())
    }
}
