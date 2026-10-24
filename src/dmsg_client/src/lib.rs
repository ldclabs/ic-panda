#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

pub mod account;
pub mod agent;
pub mod agents;
pub mod approval;
pub mod attest;
pub mod audit;
pub mod certified;
pub mod channel;
pub mod channel_log;
pub mod client;
pub mod cloud;
pub mod codec;
pub mod commerce;
pub mod config;
pub mod connect;
pub mod content;
pub mod crypto;
pub mod diagnostic;
mod downloads;
pub mod evidence;
pub mod files;
pub mod handle;
pub mod hold;
pub mod ic;
pub mod inbox;
pub mod legacy;
pub mod legacy_proof;
pub mod library;
pub mod login;
pub mod mail_push;
pub mod mcp_events;
pub mod migration;
pub mod payer;
pub mod payment;
pub mod principal;
pub mod recovery;
pub mod root;
pub mod secrets;
pub mod secure;
pub mod security;
pub mod store;
pub mod sync;
mod transport;
pub mod upload;
pub mod vault;
pub mod wallet;
pub mod workspace;

pub use client::{Client, InFlight, SessionState, Status};
use dmsg_types::Error;

/// The current time in Unix milliseconds.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// A file system failure of the device's storage.
pub(crate) fn storage(error: std::io::Error) -> Error {
    diagnostic::Diagnostic::io("storage", &error).emit();
    Error::Unavailable(format!("storage: {error}"))
}

/// The code of a payer whose ledger balance cannot cover a transfer. The
/// ledger's typed refusal becomes `InvalidInput`, so the client tags this
/// one with a stable code in its diagnostic.
pub(crate) const INSUFFICIENT_FUNDS: &str = "INSUFFICIENT_FUNDS";

/// A payer balance that does not cover the amount and the network fee.
pub(crate) fn insufficient_funds() -> Error {
    Error::InvalidInput(format!(
        "{INSUFFICIENT_FUNDS}: the payer's balance does not cover the amount and the network fee"
    ))
}

/// An error's variant name in `SCREAMING_SNAKE_CASE`, as the daemon and the
/// status report it; a tagged client failure keeps its own code.
pub fn error_code(error: &Error) -> String {
    if let Error::Unavailable(message) = error
        && (message.starts_with("storage: ") || message == "corrupt record")
    {
        return "LOCAL_STORAGE_UNAVAILABLE".into();
    }
    if let Error::Unavailable(message) = error
        && message.starts_with("network: ")
    {
        return "NETWORK_UNAVAILABLE".into();
    }
    if let Error::InvalidInput(message) = error
        && message
            .strip_prefix(INSUFFICIENT_FUNDS)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(':'))
    {
        return INSUFFICIENT_FUNDS.into();
    }
    if let Error::InvalidInput(message) = error
        && crate::secrets::ERROR_CODES.contains(&message.as_str())
    {
        return message.clone();
    }
    let name = format!("{error:?}");
    let name = name.split('(').next().unwrap_or_default();
    let mut code = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            code.push('_');
        }
        code.push(c.to_ascii_uppercase());
    }
    code
}

#[cfg(test)]
mod agent_tests;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
#[cfg(test)]
mod vectors;

#[cfg(test)]
mod error_tests {
    use super::*;

    #[test]
    fn a_fundless_payer_keeps_its_code() {
        assert_eq!(
            insufficient_funds().to_string(),
            format!(
                "InvalidInput: {INSUFFICIENT_FUNDS}: the payer's balance does not cover the amount and the network fee"
            )
        );
        assert_eq!(error_code(&insufficient_funds()), INSUFFICIENT_FUNDS);
        assert_eq!(
            error_code(&Error::InvalidInput(format!(
                "{INSUFFICIENT_FUNDS}: balance 0"
            ))),
            INSUFFICIENT_FUNDS
        );
        assert_eq!(
            error_code(&Error::InvalidInput("other: detail".into())),
            "INVALID_INPUT"
        );
        assert_eq!(
            error_code(&Error::InvalidInput("INSUFFICIENT_FUNDS_EXTRA".into())),
            "INVALID_INPUT"
        );
    }
}
