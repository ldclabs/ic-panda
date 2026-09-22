use candid::Principal;
use ic_cose_types::{cose::encrypt0::try_decode_encrypt0, validate_str};
use ic_message_types::{
    channel::{ChannelInfo, ChannelKEKInput, ChannelTopupInput, CreateChannelInput},
    profile::{UpdateKVInput, UserInfo},
};
use serde_bytes::{ByteArray, ByteBuf};

use crate::{is_authenticated, store, types};

#[ic_cdk::update(guard = "is_authenticated")]
async fn register_username(username: String, name: Option<String>) -> Result<UserInfo, String> {
    let legacy_input = candid::encode_args((&username, &name)).map_err(|e| e.to_string())?;
    crate::legacy::business_async(
        "register_username",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| async move {
            if username.len() > types::MAX_USER_NAME_SIZE {
                Err("username is too long".to_string())?;
            }
            if username.starts_with("_") {
                Err("invalid username".to_string())?;
            }

            validate_str(&username.to_ascii_lowercase())?;

            if let Some(ref name) = name {
                if name.is_empty() {
                    Err("name is empty".to_string())?;
                }
                if name.len() > types::MAX_DISPLAY_NAME_SIZE {
                    Err("name is too long".to_string())?;
                }
                if name != name.trim() {
                    Err("name has leading or trailing spaces".to_string())?;
                }
            }

            let caller = _legacy_caller;
            let now_ms = _legacy_now_ms;
            store::user::register_username(
                caller,
                username.clone(),
                name.unwrap_or(username),
                now_ms,
            )
            .await
        },
    )
    .await
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn transfer_username(to: Principal) -> Result<(), String> {
    let legacy_input = candid::encode_args((&to,)).map_err(|e| e.to_string())?;
    crate::legacy::business_async(
        "transfer_username",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| async move {
            let caller = _legacy_caller;
            if caller == to {
                Err("cannot transfer to self".to_string())?;
            }

            let now_ms = _legacy_now_ms;
            store::user::transfer_username(caller, to, now_ms).await
        },
    )
    .await
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn update_my_name(name: String) -> Result<UserInfo, String> {
    let legacy_input = candid::encode_args((&name,)).map_err(|e| e.to_string())?;
    crate::legacy::business_async(
        "update_my_name",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| async move {
            if name.is_empty() {
                Err("name is empty".to_string())?;
            }
            if name.len() > types::MAX_DISPLAY_NAME_SIZE {
                Err("name is too long".to_string())?;
            }
            if name != name.trim() {
                Err("name has leading or trailing spaces".to_string())?;
            }

            let caller = _legacy_caller;
            store::user::update_name(caller, name).await
        },
    )
    .await
}

#[ic_cdk::update(guard = "is_authenticated")]
fn update_my_username(username: String) -> Result<UserInfo, String> {
    let legacy_input = candid::encode_args((&username,)).map_err(|e| e.to_string())?;
    crate::legacy::business(
        "update_my_username",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| {
            if username.len() > types::MAX_USER_NAME_SIZE {
                Err("username is too long".to_string())?;
            }
            if username.starts_with("_") {
                Err("invalid username".to_string())?;
            }
            validate_str(&username.to_ascii_lowercase())?;

            let caller = _legacy_caller;
            store::user::update_username(caller, username)
        },
    )
}

#[ic_cdk::update(guard = "is_authenticated")]
fn update_my_image(image: String) -> Result<(), String> {
    let legacy_input = candid::encode_args((&image,)).map_err(|e| e.to_string())?;
    crate::legacy::business(
        "update_my_image",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| {
            if !image.starts_with("http") {
                Err("invalid image url".to_string())?;
            }

            let caller = _legacy_caller;
            store::user::update_image(caller, image)
        },
    )
}

// DEPRECATED
#[ic_cdk::update(guard = "is_authenticated")]
async fn update_my_ecdh(ecdh_pub: ByteArray<32>, encrypted_ecdh: ByteBuf) -> Result<(), String> {
    let legacy_input =
        candid::encode_args((&ecdh_pub, &encrypted_ecdh)).map_err(|e| e.to_string())?;
    crate::legacy::business_async(
        "update_my_ecdh",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| async move {
            let caller = _legacy_caller;
            try_decode_encrypt0(&encrypted_ecdh)?;
            store::user::update_my_ecdh(caller, ecdh_pub, encrypted_ecdh).await
        },
    )
    .await
}

// DEPRECATED
#[ic_cdk::update(guard = "is_authenticated")]
async fn update_my_kv(input: UpdateKVInput) -> Result<(), String> {
    let legacy_input = candid::encode_args((&input,)).map_err(|e| e.to_string())?;
    crate::legacy::business_async(
        "update_my_kv",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| async move {
            let caller = _legacy_caller;
            store::user::update_my_kv(caller, input).await
        },
    )
    .await
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn create_channel(input: CreateChannelInput) -> Result<ChannelInfo, String> {
    let legacy_input = candid::encode_args((&input,)).map_err(|e| e.to_string())?;
    crate::legacy::business_async(
        "create_channel",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| async move {
            let caller = _legacy_caller;
            let mut input = input;
            input.created_by = caller;
            input.validate()?;

            let now_ms = _legacy_now_ms;
            store::channel::create_channel(caller, now_ms, input).await
        },
    )
    .await
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn topup_channel(input: ChannelTopupInput) -> Result<ChannelInfo, String> {
    let legacy_input = candid::encode_args((&input,)).map_err(|e| e.to_string())?;
    crate::legacy::business_async(
        "topup_channel",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| async move {
            input.validate()?;

            let caller = _legacy_caller;
            store::channel::topup_channel(caller, input).await
        },
    )
    .await
}

// DEPRECATED
#[ic_cdk::update(guard = "is_authenticated")]
async fn save_channel_kek(input: ChannelKEKInput) -> Result<(), String> {
    let legacy_input = candid::encode_args((&input,)).map_err(|e| e.to_string())?;
    crate::legacy::business_async(
        "save_channel_kek",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| async move {
            let caller = _legacy_caller;
            store::channel::save_channel_kek(caller, input).await
        },
    )
    .await
}
