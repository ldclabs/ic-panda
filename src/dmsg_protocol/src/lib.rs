#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
mod codec;
mod handle;
mod requests;
pub use codec::*;
pub use handle::*;
pub use requests::*;
/// Closed application-action profiles and admission checks.
pub mod app_action;
mod signing;
pub use signing::*;

mod identity;
pub use identity::*;
mod receipt;
pub use receipt::*;

pub mod billing;
pub mod membership;

/// Third-party authentication, signing and subscription contracts.
pub mod integration;

/// Independent verification of dedicated third-party IC authentication proofs.
pub mod authentication;

#[cfg(test)]
extern crate self as dmsg_protocol;

/// Pure operational v2 money and product-contract rules.
pub mod commerce_v2;

/// Shared product-side interval reservation and contract CAS.
pub mod product_book;
