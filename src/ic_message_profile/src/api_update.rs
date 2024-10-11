use ic_cose_types::MILLISECONDS;
use serde_bytes::ByteArray;

use crate::{store, types};

#[ic_cdk::update]
fn update_profile(input: types::UpdateProfileInput) -> Result<types::ProfileInfo, String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::profile::update(caller, now_ms, input)
}

#[ic_cdk::update]
fn update_profile_ecdh_pub(ecdh_pub: ByteArray<32>) -> Result<(), String> {
    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::profile::update_profile_ecdh_pub(caller, now_ms, ecdh_pub)
}

#[ic_cdk::update]
fn update_links(links: Vec<types::Link>) -> Result<(), String> {
    if links.len() > types::MAX_PROFILE_LINKS {
        return Err("too many links".to_string());
    }
    for l in &links {
        l.validate()?;
    }
    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::profile::with_mut(caller, |p| {
        p.links = links;
        p.active_at = now_ms;
        Ok(())
    })
}

#[ic_cdk::update]
async fn upload_image_token(
    input: types::UploadImageInput,
) -> Result<types::UploadImageOutput, String> {
    input.validate()?;
    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::profile::upload_image_token(caller, now_ms, input).await
}
