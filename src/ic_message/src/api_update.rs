use candid::Principal;
use ic_cose_types::{cose::encrypt0::try_decode_encrypt0, validate_key, MILLISECONDS};
use ic_message_types::{
    channel::{ChannelInfo, ChannelKEKInput, ChannelTopupInput, CreateChannelInput},
    profile::{UpdateKVInput, UserInfo},
};
use serde_bytes::{ByteArray, ByteBuf};

use crate::{is_authenticated, store, types};

#[ic_cdk::update(guard = "is_authenticated")]
async fn register_username(username: String, name: Option<String>) -> Result<UserInfo, String> {
    if username.len() > types::MAX_USER_SIZE {
        Err("username is too long".to_string())?;
    }
    if username.starts_with("_") {
        Err("invalid username".to_string())?;
    }
    validate_key(&username.to_ascii_lowercase())?;

    if let Some(ref name) = name {
        if name.is_empty() {
            Err("name is empty".to_string())?;
        }
        if name.len() > types::MAX_USER_NAME_SIZE {
            Err("name is too long".to_string())?;
        }
        if name != name.trim() {
            Err("name has leading or trailing spaces".to_string())?;
        }
    }

    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::user::register_username(caller, username.clone(), name.unwrap_or(username), now_ms).await
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn transfer_username(to: Principal) -> Result<(), String> {
    let caller = ic_cdk::caller();
    if caller == to {
        Err("cannot transfer to self".to_string())?;
    }

    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::user::transfer_username(caller, to, now_ms).await
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn update_my_name(name: String) -> Result<UserInfo, String> {
    if name.is_empty() {
        Err("name is empty".to_string())?;
    }
    if name.len() > types::MAX_USER_NAME_SIZE {
        Err("name is too long".to_string())?;
    }
    if name != name.trim() {
        Err("name has leading or trailing spaces".to_string())?;
    }

    let caller = ic_cdk::caller();
    store::user::update_name(caller, name).await
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn update_my_image(image: String) -> Result<(), String> {
    if !image.starts_with("https://") {
        Err("invalid image url".to_string())?;
    }

    let caller = ic_cdk::caller();
    store::user::update_image(caller, image).await
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn update_my_ecdh(ecdh_pub: ByteArray<32>, encrypted_ecdh: ByteBuf) -> Result<(), String> {
    let caller = ic_cdk::caller();
    try_decode_encrypt0(&encrypted_ecdh)?;
    store::user::update_my_ecdh(caller, ecdh_pub, encrypted_ecdh).await
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn update_my_kv(input: UpdateKVInput) -> Result<(), String> {
    let caller = ic_cdk::caller();
    store::user::update_my_kv(caller, input).await
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn create_channel(input: CreateChannelInput) -> Result<ChannelInfo, String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::channel::create_channel(caller, now_ms, input).await
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn topup_channel(input: ChannelTopupInput) -> Result<ChannelInfo, String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    store::channel::topup_channel(caller, input).await
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn save_channel_kek(input: ChannelKEKInput) -> Result<(), String> {
    let caller = ic_cdk::caller();
    store::channel::save_channel_kek(caller, input).await
}
