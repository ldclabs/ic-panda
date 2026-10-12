#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

pub mod account;
pub mod agent;
pub mod agents;
pub mod approval;
pub mod attest;
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
pub mod evidence;
pub mod files;
pub mod handle;
pub mod ic;
pub mod inbox;
pub mod login;
pub mod payment;
pub mod profile;
pub mod root;
pub mod secure;
pub mod store;
pub mod sync;
pub mod upload;
pub mod vault;
pub mod wallet;
pub mod workspace;

pub use client::{Client, Status};
use dmsg_types::Error;

/// The current time in Unix milliseconds.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// An error's variant name in `SCREAMING_SNAKE_CASE`, as the daemon and the
/// status report it.
pub fn error_code(error: &Error) -> String {
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
#[cfg(test)]
mod testing;
#[cfg(test)]
mod vectors;
