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
/// Daily formal-signature ceilings a user home may authorize for one account.
/// COSE sizes its per-account hard budget to cover these and the root ceilings.
pub const FORMAL_DAILY_EXECUTIONS: u32 = 100;
pub const FORMAL_DAILY_CYCLES: u128 = 800_000_000_000;
/// Separate daily content-root ceilings for bootstrap, approved-device unlock
/// and revocation rekeys, at the measured ~68.3B vetKD cost.
pub const ROOT_DAILY_EXECUTIONS: u32 = 20;
pub const ROOT_DAILY_CYCLES: u128 = 300_000_000_000;
