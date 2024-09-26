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
