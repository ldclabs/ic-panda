//! Reference implementation; public records are defined by dmsg_types.
//!
//! Publishes Agent Delegation principal documents for every user home at one
//! custom domain, as ICP-certified HTTP responses. User homes stay the
//! authority for controllers; this canister only serves what they publish.
mod api;
mod http;
mod stable_codec;
mod store;

use candid::Principal;
use dmsg_runtime::admin::Validation;
use dmsg_types::{agent::*, *};

ic_cdk::export_candid!();
