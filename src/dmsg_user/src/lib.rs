//! Reference implementation; public records are defined by dmsg_types.
mod account;
mod api;
mod execution;
mod recovery;
mod stable_codec;
mod state;
mod store;
mod xid;

use dmsg_types::{cose::*, handle::*, payment::SignedOffer, user::*, *};
use serde_bytes::ByteBuf;

ic_cdk::export_candid!();
