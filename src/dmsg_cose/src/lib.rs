//! Reference implementation; public records are defined by dmsg_types.
mod api;
mod model;
mod stable_codec;
mod store;

use dmsg_types::{cose::*, *};

ic_cdk::export_candid!();
