use candid::Principal;
use serde_bytes::ByteArray;

use crate::{store, types};

#[ic_cdk::update]
fn update_profile(input: types::UpdateProfileInput) -> Result<types::ProfileInfo, String> {
    let legacy_input = candid::encode_args((&input,)).map_err(|e| e.to_string())?;
    crate::legacy::business(
        "update_profile",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| {
            input.validate()?;

            let caller = _legacy_caller;
            let now_ms = _legacy_now_ms;
            store::profile::update(caller, now_ms, input)
        },
    )
}

#[ic_cdk::update]
fn update_profile_ecdh_pub(ecdh_pub: ByteArray<32>) -> Result<(), String> {
    let legacy_input = candid::encode_args((&ecdh_pub,)).map_err(|e| e.to_string())?;
    crate::legacy::business(
        "update_profile_ecdh_pub",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| {
            let caller = _legacy_caller;
            let now_ms = _legacy_now_ms;
            store::profile::update_profile_ecdh_pub(caller, now_ms, ecdh_pub)
        },
    )
}

#[ic_cdk::update]
fn update_links(links: Vec<types::Link>) -> Result<(), String> {
    let legacy_input = candid::encode_args((&links,)).map_err(|e| e.to_string())?;
    crate::legacy::business(
        "update_links",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| {
            if links.len() > types::MAX_PROFILE_LINKS {
                return Err("too many links".to_string());
            }
            for l in &links {
                l.validate()?;
            }
            let caller = _legacy_caller;
            let now_ms = _legacy_now_ms;
            store::profile::with_mut(caller, |p| {
                p.links = links;
                p.active_at = now_ms;
                Ok(())
            })
        },
    )
}

#[ic_cdk::update]
fn update_tokens(tokens: Vec<Principal>) -> Result<(), String> {
    let legacy_input = candid::encode_args((&tokens,)).map_err(|e| e.to_string())?;
    crate::legacy::business(
        "update_tokens",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| {
            if tokens.len() > types::MAX_PROFILE_TOKENS {
                return Err("too many tokens".to_string());
            }
            let mut c = tokens.clone();
            c.dedup();
            if c.len() != tokens.len() {
                return Err("duplicate tokens".to_string());
            }

            for t in &tokens {
                if t.as_slice().len() != 10 {
                    return Err("invalid token canister ID".to_string());
                }
            }

            let caller = _legacy_caller;
            let now_ms = _legacy_now_ms;
            store::profile::with_mut(caller, |p| {
                p.tokens = tokens;
                p.active_at = now_ms;
                Ok(())
            })
        },
    )
}

#[ic_cdk::update]
async fn upload_image_token(
    input: types::UploadImageInput,
) -> Result<types::UploadImageOutput, String> {
    let legacy_input = candid::encode_args((&input,)).map_err(|e| e.to_string())?;
    crate::legacy::business_async(
        "upload_image_token",
        legacy_input,
        |_legacy_caller, _legacy_now_ms| async move {
            input.validate()?;
            let caller = _legacy_caller;
            let now_ms = _legacy_now_ms;
            store::profile::upload_image_token(caller, now_ms, input).await
        },
    )
    .await
}
