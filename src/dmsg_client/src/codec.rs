//! Canonical CBOR and base64url for client records.
//!
//! Encoding is [`canonical`] (RFC 8949 core deterministic). Every incoming
//! record is decoded with [`decode`] or [`decode_value`]: the bytes are scanned
//! before any allocation, so hostile input cannot exhaust memory or recursion,
//! and must re-encode to themselves.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use candid::Principal;
use cbor2::Value;
use dmsg_types::{ensure, ensure_valid, Error, Result};
use serde::{de::DeserializeOwned, Serialize};

pub use dmsg_protocol::{canonical, digest, sha256};

/// Default size bound of one decoded record.
pub const MAX_RECORD: usize = 262_144;

const MAX_DEPTH: usize = 24;
const MAX_NODES: usize = 50_000;
const MAX_ITEMS: u64 = 25_000;

/// SHA-256 of `bytes` as lowercase hex.
pub fn hex_sha256(bytes: &[u8]) -> String {
    hex::encode(*sha256(bytes))
}

/// A Principal in its canonical text, as records carry it.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for any other text.
pub fn principal(text: &str) -> Result<Principal> {
    let value = Principal::from_text(text).map_err(|_| Error::IntegrityFailed)?;
    ensure(value.to_text() == text, Error::IntegrityFailed)?;
    Ok(value)
}

/// A Principal a person or a client typed, in any text `from_text` takes.
///
/// # Errors
/// Returns `Error::InvalidInput` for anything else.
pub fn principal_input(text: &str) -> Result<Principal> {
    Principal::from_text(text).map_err(|_| dmsg_types::invalid("principal"))
}

/// Base64url without padding.
pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decode canonical base64url: no padding, no whitespace, zero trailing bits.
///
/// # Errors
/// Any other spelling returns `Error::InvalidInput`.
pub fn unb64(text: &str) -> Result<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(text)
        .map_err(|_| Error::InvalidInput("base64url".into()))
}

/// Serde adapter for byte fields carried as base64url text.
pub mod base64url {
    use serde::{de::Error as _, Deserialize, Deserializer, Serializer};

    /// Serialize bytes as base64url text.
    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&super::b64(bytes))
    }

    /// Deserialize canonical base64url text into bytes.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        super::unb64(&String::deserialize(deserializer)?)
            .map_err(|_| D::Error::custom("invalid base64url"))
    }
}

/// Decode one canonical CBOR item of at most `max` bytes into a dynamic value.
///
/// Besides canonical form this enforces the client data model: at most 24
/// levels, 50,000 items and 25,000 entries per container; strictly ascending
/// map keys; integers in `[-2^63, 2^64)` or a tag 2 bignum of 9 to 16 bytes;
/// no floats, undefined, other simple values or other tags.
///
/// # Errors
/// Empty or oversized input returns `Error::QuotaExceeded`; anything outside
/// the model returns `InvalidInput`, `UnsupportedProtocol` or `IntegrityFailed`.
pub fn decode_value(bytes: &[u8], max: usize) -> Result<Value> {
    decode(bytes, max)
}

/// Decode one canonical CBOR item of at most `max` bytes into `T`.
///
/// The bounds are those of [`decode_value`]. `T` must preserve the wire
/// representation: unknown, missing or reordered fields fail the re-encoding.
///
/// # Errors
/// As [`decode_value`], and `IntegrityFailed` when `T` cannot hold the item.
pub fn decode<T: DeserializeOwned + Serialize>(bytes: &[u8], max: usize) -> Result<T> {
    ensure(
        !bytes.is_empty() && bytes.len() <= max,
        Error::QuotaExceeded,
    )?;
    let mut scanner = Scanner {
        data: bytes,
        offset: 0,
        nodes: 0,
    };
    scanner.item(0)?;
    ensure(scanner.offset == bytes.len(), Error::IntegrityFailed)?;
    let value: T = cbor2::from_slice(bytes).map_err(|_| Error::IntegrityFailed)?;
    let encoded = cbor2::to_canonical_vec(&value).map_err(|_| Error::IntegrityFailed)?;
    ensure(encoded == bytes, Error::IntegrityFailed)?;
    Ok(value)
}

struct Scanner<'a> {
    data: &'a [u8],
    offset: usize,
    nodes: usize,
}

impl Scanner<'_> {
    /// Read one header: major type, additional information and argument.
    fn header(&mut self) -> Result<(u8, u8, u64)> {
        let first = *self.data.get(self.offset).ok_or(Error::IntegrityFailed)?;
        self.offset += 1;
        let (major, info) = (first >> 5, first & 31);
        ensure(info < 28, Error::UnsupportedProtocol)?;
        if info < 24 {
            return Ok((major, info, info.into()));
        }
        let bytes = self.take(1 << (info - 24))?;
        Ok((
            major,
            info,
            bytes.iter().fold(0, |n, b| (n << 8) | u64::from(*b)),
        ))
    }

    fn take(&mut self, size: u64) -> Result<&[u8]> {
        let remaining = self.data.len() - self.offset;
        ensure(size <= remaining as u64, Error::IntegrityFailed)?;
        let start = self.offset;
        self.offset += size as usize;
        Ok(&self.data[start..self.offset])
    }

    fn item(&mut self, depth: usize) -> Result<()> {
        self.nodes += 1;
        ensure_valid(
            depth <= MAX_DEPTH && self.nodes <= MAX_NODES,
            "cbor nesting",
        )?;
        let (major, info, argument) = self.header()?;
        match major {
            0 => Ok(()),
            1 => ensure_valid(argument < 1 << 63, "cbor integer range"),
            2 | 3 => self.take(argument).map(|_| ()),
            4 => {
                ensure(argument <= MAX_ITEMS, Error::QuotaExceeded)?;
                (0..argument).try_for_each(|_| self.item(depth + 1))
            }
            5 => {
                ensure(argument <= MAX_ITEMS, Error::QuotaExceeded)?;
                let mut previous: Option<(usize, usize)> = None;
                for _ in 0..argument {
                    let start = self.offset;
                    self.item(depth + 1)?;
                    if let Some((a, b)) = previous {
                        ensure(
                            self.data[a..b] < self.data[start..self.offset],
                            Error::IntegrityFailed,
                        )?;
                    }
                    previous = Some((start, self.offset));
                    self.item(depth + 1)?;
                }
                Ok(())
            }
            6 => {
                // Only a positive bignum that does not fit in a u64.
                ensure(info == 2, Error::UnsupportedProtocol)?;
                let (head, lead) = self
                    .data
                    .get(self.offset..self.offset + 2)
                    .map(|h| (h[0], h[1]))
                    .ok_or(Error::IntegrityFailed)?;
                // A byte string of 9 to 16 bytes without a leading zero.
                ensure_valid((0x49..=0x50).contains(&head) && lead != 0, "cbor bignum")?;
                self.item(depth + 1)
            }
            _ => ensure(matches!(info, 20..=22), Error::UnsupportedProtocol),
        }
    }
}

#[cfg(test)]
mod tests;
