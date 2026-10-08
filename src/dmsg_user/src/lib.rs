//! Reference implementation; public records are defined by dmsg_types.
mod account;
mod api;
#[cfg(test)]
mod capacity;
mod commerce;
mod execution;
mod external;
mod principal;
mod recovery;
mod stable_codec;
mod state;
mod store;
mod xid;

use dmsg_runtime::admin::Validation;
use dmsg_types::{
    agent::*, billing::*, cose::*, handle::*, integration::*, payment::SignedOffer, user::*, *,
};
use serde_bytes::ByteBuf;

ic_cdk::export_candid!();
