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
/// Closed application-action signing contract.
pub mod app_action;
pub mod signing;
pub use signing::*;
pub mod profiles;

pub mod account_id;
pub use account_id::AccountId;

/// Third-party authentication, signing and subscription contracts.
pub mod integration;

/// Operational v2 checkout and shared product subscription state.
pub mod integration_billing;

/// Operational full-waiver PANDA application and immutable commitment types.
pub mod integration_membership;
