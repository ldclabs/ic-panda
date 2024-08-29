use ic_cose_types::MILLISECONDS;

use crate::{store, types};

#[ic_cdk::update]
fn update_profile(input: types::UpdateProfileInput) -> Result<types::ProfileInfo, String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::profile::update(caller, now_ms, input)
}
