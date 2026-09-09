//! Reference implementation; public records are defined by dmsg_types.
mod api;
mod store;

use dmsg_types::{handle::*, *};

ic_cdk::export_candid!();
