use candid::{CandidType, Principal};
use ic_oss_types::file::CHUNK_SIZE;
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub const MAX_PROFILE_FOLLOWING: usize = 2048;
pub const MAX_PROFILE_BIO_SIZE: usize = 2048; // 2KB
pub const MAX_PROFILE_LINKS: usize = 100;
pub const MAX_PROFILE_TOKENS: usize = 100;
pub const MAX_PROFILE_CHANNEL_ALIAS_LEN: usize = 20;
pub const MAX_PROFILE_CHANNEL_TAGS_LEN: usize = 5;
pub const MAX_PROFILE_CHANNEL_TAG_LEN: usize = 20;

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
    pub image_file: Option<(Principal, u32)>, // image file: (ic-oss-bucket canister, file_id)
    pub links: Vec<Link>,
    pub tokens: Vec<Principal>,
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
        // Check for conflicts in follow and unfollow
        if !self.follow.is_disjoint(&self.unfollow) {
            return Err("conflicting principals in follow and unfollow".to_string());
        }

        for (channel, setting) in self.upsert_channels.iter() {
            if self.remove_channels.contains(channel) {
                return Err(format!(
                    "channel {:?} exists in both upsert and remove",
                    channel
                ));
            }
            if setting.alias.len() > MAX_PROFILE_CHANNEL_ALIAS_LEN {
                return Err(format!("channel alias too long: {}", setting.alias.len()));
            }
            if setting.tags.len() > MAX_PROFILE_CHANNEL_TAGS_LEN {
                return Err(format!("too many tags: {}", setting.tags.len()));
            }
            for tag in &setting.tags {
                if tag.len() > MAX_PROFILE_CHANNEL_TAG_LEN {
                    return Err(format!("tag too long: {}", tag.len()));
                }
            }
        }
        Ok(())
    }
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct UpdateKVInput {
    pub upsert_kv: BTreeMap<String, ByteBuf>,
    pub remove_kv: BTreeSet<String>,
}

impl UpdateKVInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.upsert_kv.is_empty() || self.remove_kv.is_empty() {
            return Err("empty upsert_kv or remove_kv".to_string());
        }
        Ok(())
    }
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct Link {
    pub title: String,
    pub uri: String,
    pub image: Option<String>,
}

impl Link {
    pub fn validate(&self) -> Result<(), String> {
        if self.title.is_empty() || self.title.trim() != self.title || self.title.len() > 128 {
            return Err("invalid title".to_string());
        }
        if self.uri.is_empty() || self.uri.trim() != self.uri || self.uri.len() > 128 {
            return Err("invalid uri".to_string());
        }
        Ok(())
    }
}

#[derive(CandidType, Clone, Debug, Default, Deserialize, Serialize)]
pub struct UploadImageInput {
    pub size: u64,            // should <= 256KB
    pub content_type: String, // "image/webp" | "image/png" | "image/jpeg" | "image/svg+xml"
}

impl UploadImageInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.size > CHUNK_SIZE as u64 {
            return Err("invalid size".to_string());
        }
        match self.content_type.as_str() {
            "image/webp" | "image/png" | "image/jpeg" | "image/svg+xml" => {}
            _ => return Err(format!("invalid content_type {}", self.content_type)),
        }
        Ok(())
    }

    pub fn filename(&self, name: String) -> String {
        match self.content_type.as_str() {
            "image/webp" => format!("{}.webp", name),
            "image/png" => format!("{}.png", name),
            "image/jpeg" => format!("{}.jpg", name),
            "image/svg+xml" => format!("{}.svg", name),
            _ => name,
        }
    }
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct UploadImageOutput {
    pub name: String,
    pub image: (Principal, u32),
    pub access_token: ByteBuf,
}
