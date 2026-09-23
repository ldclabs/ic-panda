#![doc = include_str!("../README.md")]
//! Implementation support for the reference ICP canisters. Not a wire protocol.
mod budget;
mod certified;
pub mod ledger;
pub mod stable_types;
pub mod storage;
pub use budget::Budget;
pub use certified::{Certification, MAX_CERTIFIED_RESPONSE_BYTES};
mod calls;
pub use calls::{call, call_classified, CallFailure};

pub const WINDOW: usize = 64;
/// Leave eight retained-execution slots available for content-root operations.
pub const FORMAL_EXECUTION_WINDOW: usize = WINDOW - 8;
