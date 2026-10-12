#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

pub mod account;
pub mod client;
pub mod cloud;
pub mod codec;
pub mod config;
pub mod crypto;
pub mod ic;
pub mod login;
pub mod root;
pub mod store;
pub mod upload;
pub mod vault;
pub mod workspace;

pub use client::{Client, Status};

/// The current time in Unix milliseconds.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod vectors;
