#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
pub mod account;
pub mod agent;
pub mod billing;
/// dmsg-connect/1 messages between the bridge page, the relay and devices.
pub mod connect;
pub mod cose;
pub mod handle;
/// dmsg-login/1 messages between local clients and the login page.
pub mod login;
pub mod membership;
pub mod payment;
pub mod protocol;
pub mod user;
/// `dmsg-wallet/1` team wallets and their hub.
pub mod wallet;
pub use protocol::*;
/// Application-action signing contract and registered command schemas.
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
