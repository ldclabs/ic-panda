//! Reference implementation; public records are defined by dmsg_types.
mod api;
mod calls;
#[cfg(test)]
mod capacity;
mod model;
mod stable_codec;
mod state;
mod store;

use candid::Principal;
use dmsg_runtime::admin::Validation;
use dmsg_types::{payment::*, profiles::delivery::*, *};

ic_cdk::export_candid!();
