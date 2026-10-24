#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

pub mod mail_events;
pub mod resources;
pub mod runtime;
pub mod tools;

pub use runtime::{Called, DurableMail, Host, Options, Runtime, Scope, ToolError, Turn};

#[cfg(test)]
mod runtime_tests;
