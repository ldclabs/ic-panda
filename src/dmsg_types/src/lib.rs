#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
pub mod account;
pub mod billing;
pub mod cose;
pub mod handle;
pub mod membership;
pub mod payment;
pub mod protocol;
pub mod user;
pub use protocol::*;
pub mod signing;
pub use signing::*;
pub mod profiles;

pub mod account_id;
pub use account_id::AccountId;

/// Third-party authentication, signing and subscription contracts.
pub mod integration;
