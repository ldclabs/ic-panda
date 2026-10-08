mod api;
#[cfg(test)]
mod capacity;
mod claim;
mod claims;
#[cfg(test)]
#[path = "../../dmsg_types/tests/support/commerce.rs"]
mod fixture;
mod rate;
mod sns;
mod store;
use candid::Principal;
use dmsg_runtime::admin::Validation;
use dmsg_types::{
    integration::*, integration_billing::*, integration_membership::*, membership::*, *,
};
ic_cdk::export_candid!();
