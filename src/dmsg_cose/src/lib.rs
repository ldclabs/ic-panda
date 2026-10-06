//! Reference implementation; public records are defined by dmsg_types.
mod api;
mod model;
mod stable_codec;
mod store;

use candid::Principal;
use dmsg_runtime::admin::Validation;
use dmsg_types::{cose::*, *};

ic_cdk::export_candid!();
