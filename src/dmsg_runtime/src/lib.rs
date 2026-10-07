#![doc = include_str!("../README.md")]
//! Implementation support for the reference ICP canisters. Not a wire protocol.
pub mod admin;
mod budget;
pub mod cert_map;
mod certified;
pub mod ledger;
pub mod name_tree;
pub mod stable_types;
pub mod storage;
pub use budget::Budget;
pub use certified::{certified_batch, query_certificate, MAX_CERTIFIED_RESPONSE_BYTES};
mod calls;
pub use calls::{call, call_classified, CallFailure};

/// Retained executions (attestations and recovery derivations) per account.
pub const WINDOW: usize = 64;
/// Daily attestation ceiling a user home may record for one account; an
/// account policy may choose less.
pub const FORMAL_DAILY_EXECUTIONS: u32 = 100;
