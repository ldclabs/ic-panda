use candid::Principal;
use ic_auth_types::{ByteArrayB64, ByteBufB64, SignInResponse, SignedDelegation};
use serde_bytes::ByteBuf;

mod api;
mod api_admin;
mod api_eth;
mod api_init;
mod api_sol;
mod helper;
mod store;

use crate::api_init::CanisterArgs;

ic_cdk::export_candid!();
