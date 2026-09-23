//! Reference implementation; public records are defined by dmsg_types.
mod account;
mod api;
mod commerce;
mod execution;
mod external;
mod recovery;
mod stable_codec;
mod state;
mod store;
mod xid;

use dmsg_types::{
    billing::*, cose::*, handle::*, integration::*, membership::*, payment::SignedOffer, user::*, *,
};
use serde_bytes::ByteBuf;

ic_cdk::export_candid!();
