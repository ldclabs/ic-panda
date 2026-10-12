#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

pub mod codec;
pub mod crypto;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod vectors;
