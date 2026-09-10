//! Reference implementation; public records are defined by dmsg_types.
mod api;
mod stable_codec;
mod store;

use dmsg_types::{handle::*, *};

ic_cdk::export_candid!();
