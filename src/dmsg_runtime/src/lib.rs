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
pub use certified::{MAX_CERTIFIED_RESPONSE_BYTES, certified_batch, query_certificate};
mod calls;
pub use calls::{CallFailure, call, call_classified};

/// Retained executions (attestations and recovery derivations) per account.
pub const WINDOW: usize = 64;
/// Daily attestation ceiling a user home may record for one account; an
/// account policy may choose less. Results stay retained for a day, so the
/// window already bounds a day's executions.
pub const FORMAL_DAILY_EXECUTIONS: u32 = WINDOW as u32;
