//! Reference implementation; public records are defined by dmsg_types.
mod api;
mod calls;
mod stable_codec;
mod store;

use candid::Principal;
use dmsg_runtime::admin::Validation;
use dmsg_types::{handle::*, *};
use icrc_ledger_types::icrc1::account::Account;

ic_cdk::export_candid!();
