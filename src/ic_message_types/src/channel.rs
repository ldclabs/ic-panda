use candid::{CandidType, Principal};
use ic_cose_types::{cose::encrypt0::try_decode_encrypt0, to_cbor_bytes};
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};
use std::collections::{BTreeSet, HashMap};

pub const MAX_CHANNEL_MANAGERS: usize = 5;
pub const MAX_CHANNEL_MEMBERS: usize = 100;
pub const MAX_CHANNEL_MESSAGES: u32 = 10000;
pub const MAX_USER_CHANNELS: usize = 1000;
pub const MAX_MESSAGE_SIZE: usize = 1024 * 32; // 32KB
pub const MIN_TOPUP_AMOUNT: u64 = 100_000_000; // 1 token

pub static SYS_MSG_CHANNEL_CREATE: &str = "Channel.Create";
pub static SYS_MSG_CHANNEL_TOPUP: &str = "Channel.Topup";
pub static SYS_MSG_CHANNEL_UPDATE_INFO: &str = "Channel.Update.Info";
pub static SYS_MSG_CHANNEL_ADD_MANAGER: &str = "Channel.Add.Manager";
pub static SYS_MSG_CHANNEL_ADD_MEMBER: &str = "Channel.Add.Member";
pub static SYS_MSG_CHANNEL_UPLOAD_FILE: &str = "Channel.Upload.File";

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct ChannelInfo {
    pub id: u32,
    pub canister: Principal,
    pub name: String,
    pub image: String,
    pub description: String,
    pub managers: BTreeSet<Principal>,
    pub members: BTreeSet<Principal>,
    pub dek: ByteBuf,
    pub created_at: u64,
    pub created_by: Principal,
    pub updated_at: u64,
    pub paid: u64,
    pub gas: u64,
    pub message_start: u32,
    pub latest_message_id: u32,
    pub latest_message_at: u64,
    pub latest_message_by: Principal,
    pub deleted_messages: BTreeSet<u32>,
    pub my_setting: ChannelSetting,
    pub ecdh_request: HashMap<Principal, (ByteArray<32>, Option<(ByteArray<32>, ByteBuf)>)>,
    #[serde(default)]
    pub files_state: Option<ChannelFilesState>,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct ChannelFilesState {
    pub file_storage: (Principal, u32),
    pub file_max_size: u64,
    pub files_total: u64,
    pub files_size_total: u64,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct ChannelBasicInfo {
    pub id: u32,
    pub canister: Principal,
    pub name: String,
    pub image: String,
    pub updated_at: u64,
    pub latest_message_id: u32,
    pub latest_message_at: u64,
    pub latest_message_by: Principal,
    pub paid: u64,
    pub gas: u64,
    pub my_setting: ChannelSetting,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct ChannelSetting {
    pub last_read: u32, // message id
    pub unread: u32,    // unread message count
    pub mute: bool,
    pub ecdh_pub: Option<ByteArray<32>>,
    pub ecdh_remote: Option<(ByteArray<32>, ByteBuf)>,
    pub updated_at: u64,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct Message {
    pub id: u32,
    pub kind: u8,      // 0: created by user, 1: created by system
    pub reply_to: u32, // 0 means not a reply
    pub created_at: u64,
    pub created_by: Principal,
    pub payload: ByteBuf,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct CreateChannelInput {
    pub name: String,
    pub image: String,
    pub description: String,
    pub managers: HashMap<Principal, ChannelECDHInput>,
    pub dek: ByteBuf,
    pub created_by: Principal,
    pub paid: u64,
}

impl CreateChannelInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            Err("name is empty".to_string())?;
        }
        if self.name.len() > 64 {
            Err("name is too long".to_string())?;
        }
        if self.description.len() > 256 {
            Err("description is too long".to_string())?;
        }

        if self.managers.is_empty() {
            Err("managers is empty".to_string())?;
        }
        if !self.managers.contains_key(&self.created_by) {
            Err("created_by is not in managers".to_string())?;
        }
        for v in self.managers.values() {
            v.validate()?;
        }

        try_decode_encrypt0(&self.dek)?;
        Ok(())
    }
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct ChannelKEKInput {
    pub id: u32,
    pub canister: Principal,
    pub kek: ByteBuf, // encrypted key to decrypt channel dek
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct ChannelTopupInput {
    pub id: u32,
    pub canister: Principal,
    pub payer: Principal,
    pub amount: u64,
}

impl ChannelTopupInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.amount < MIN_TOPUP_AMOUNT {
            Err("amount is too small".to_string())?;
        }
        Ok(())
    }
}

pub fn channel_kek_key(canister: &Principal, id: u32) -> ByteBuf {
    to_cbor_bytes(&(canister, id)).into()
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct UpdateChannelInput {
    pub id: u32,
    pub name: Option<String>,
    pub image: Option<String>,
    pub description: Option<String>,
}

impl UpdateChannelInput {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(ref name) = self.name {
            if name.is_empty() {
                Err("name is empty".to_string())?;
            }
            if name.len() > 64 {
                Err("name is too long".to_string())?;
            }
        }

        if let Some(ref image) = self.image {
            if !image.starts_with("http") {
                Err("invalid image url".to_string())?;
            }
        }

        if let Some(ref description) = self.description {
            if description.len() > 256 {
                Err("description is too long".to_string())?;
            }
        }
        Ok(())
    }
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct UpdateChannelStorageInput {
    pub id: u32,
    pub file_max_size: u64,
}

impl UpdateChannelStorageInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.file_max_size > 1024 * 1024 * 100 {
            Err("file_max_size is too large".to_string())?;
        }
        Ok(())
    }
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct UpdateChannelMemberInput {
    pub id: u32,
    pub member: Principal,
    pub ecdh: ChannelECDHInput,
}

impl UpdateChannelMemberInput {
    pub fn validate(&self) -> Result<(), String> {
        self.ecdh.validate()?;
        Ok(())
    }
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct ChannelECDHInput {
    pub ecdh_pub: Option<ByteArray<32>>,
    pub ecdh_remote: Option<(ByteArray<32>, ByteBuf)>,
}

impl ChannelECDHInput {
    pub fn validate(&self) -> Result<(), String> {
        if let Some((_, ref kek)) = self.ecdh_remote {
            try_decode_encrypt0(kek)?;
        }
        Ok(())
    }
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct UpdateMySettingInput {
    pub id: u32,
    pub last_read: Option<u32>, // message id
    pub mute: Option<bool>,
    pub ecdh: Option<ChannelECDHInput>,
}

impl UpdateMySettingInput {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(ref ecdh) = self.ecdh {
            ecdh.validate()?;
        }
        Ok(())
    }
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct AddMessageInput {
    pub channel: u32,
    pub payload: ByteBuf,
    pub reply_to: Option<u32>,
}

impl AddMessageInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.payload.len() > MAX_MESSAGE_SIZE {
            Err("payload is too large".to_string())?;
        }

        try_decode_encrypt0(&self.payload)?;
        Ok(())
    }
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct AddMessageOutput {
    pub id: u32,
    pub channel: u32,
    pub kind: u8,
    pub created_at: u64,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct DeleteMessageInput {
    pub channel: u32,
    pub id: u32,
}

impl DeleteMessageInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.channel < 1 {
            Err("channel is invalid".to_string())?;
        }
        if self.id < 1 {
            Err("id is invalid".to_string())?;
        }
        Ok(())
    }
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct TruncateMessageInput {
    pub channel: u32,
    pub to: u32,
}

impl TruncateMessageInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.channel < 1 {
            Err("channel is invalid".to_string())?;
        }
        if self.to < 2 {
            Err("to is invalid".to_string())?;
        }
        Ok(())
    }
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct UploadFileInput {
    pub channel: u32,
    pub size: u64,            // encrypted file size with COSE_Encrypt0
    pub content_type: String, // file content type
}

impl UploadFileInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.channel < 1 {
            Err("channel is invalid".to_string())?;
        }
        if self.size < 80 {
            Err("size is invalid".to_string())?;
        }
        Ok(())
    }
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct UploadFileOutput {
    pub id: u32,
    pub storage: (Principal, u32),
    pub access_token: ByteBuf,
    pub name: String,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct DownloadFilesToken {
    pub storage: (Principal, u32),
    pub access_token: ByteBuf,
}
