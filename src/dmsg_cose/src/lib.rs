//! Reference implementation; public records are defined by dmsg_types.
mod api;
mod model;
mod store;

use dmsg_types::{cose::*, *};

ic_cdk::export_candid!();
