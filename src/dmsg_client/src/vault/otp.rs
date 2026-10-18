//! One-time passwords of `otp` fields: time-based (RFC 6238), from an
//! `otpauth://totp/…` URI or a bare base32 secret, and the QR codes that
//! carry such URIs.

use dmsg_types::{Error, Result, ensure, ensure_valid};
use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use zeroize::Zeroizing;

/// Largest image a QR code is read from, in pixels.
const MAX_PIXELS: u64 = 40_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Algorithm {
    Sha1,
    Sha256,
    Sha512,
}

/// A time-based one-time password generator.
pub struct Totp {
    secret: Zeroizing<Vec<u8>>,
    algorithm: Algorithm,
    digits: u32,
    period: u64,
}

/// The code of the current period.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Code {
    /// The digits.
    pub code: String,
    /// The period, in milliseconds.
    pub period_ms: u64,
    /// How long this code stays current, in milliseconds.
    pub remaining_ms: u64,
}

/// RFC 4648 base32, ignoring case, spaces, dashes and padding.
fn base32(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut buffer, mut bits) = (0u32, 0u32);
    for c in text.chars().filter(|c| !matches!(c, ' ' | '-' | '=')) {
        let value = match c.to_ascii_uppercase() {
            c @ 'A'..='Z' => c as u32 - 'A' as u32,
            c @ '2'..='7' => c as u32 - '2' as u32 + 26,
            _ => return None,
        };
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Read an `otp` field's value.
///
/// # Errors
/// Returns `Error::InvalidInput("otp")` for anything but a TOTP URI or a
/// base32 secret of at least 10 bytes, 6 to 8 digits and a period of 1 to
/// 300 seconds.
pub fn parse(value: &str) -> Result<Totp> {
    let invalid = || Error::InvalidInput("otp".into());
    let value = value.trim();
    let (mut secret, mut algorithm, mut digits, mut period) = (None, Algorithm::Sha1, 6, 30);
    if value
        .get(..10)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("otpauth://"))
    {
        let url = url::Url::parse(value).map_err(|_| invalid())?;
        ensure(
            url.host_str()
                .is_some_and(|h| h.eq_ignore_ascii_case("totp")),
            invalid(),
        )?;
        for (key, value) in url.query_pairs() {
            match key.to_ascii_lowercase().as_str() {
                "secret" => secret = base32(&value),
                "algorithm" => {
                    algorithm = match value.to_ascii_uppercase().as_str() {
                        "SHA1" => Algorithm::Sha1,
                        "SHA256" => Algorithm::Sha256,
                        "SHA512" => Algorithm::Sha512,
                        _ => return Err(invalid()),
                    }
                }
                "digits" => digits = value.parse().map_err(|_| invalid())?,
                "period" => period = value.parse().map_err(|_| invalid())?,
                _ => {}
            }
        }
    } else {
        secret = base32(value);
    }
    let secret = Zeroizing::new(secret.ok_or_else(invalid)?);
    ensure_valid(
        secret.len() >= 10 && (6..=8).contains(&digits) && (1..=300).contains(&period),
        "otp",
    )?;
    Ok(Totp {
        secret,
        algorithm,
        digits,
        period,
    })
}

fn hmac<D: Mac + KeyInit>(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = <D as KeyInit>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

impl Totp {
    /// The code at `now_ms`.
    pub fn code(&self, now_ms: u64) -> Code {
        let period_ms = self.period * 1000;
        let counter = (now_ms / period_ms).to_be_bytes();
        let digest = match self.algorithm {
            Algorithm::Sha1 => hmac::<Hmac<Sha1>>(&self.secret, &counter),
            Algorithm::Sha256 => hmac::<Hmac<Sha256>>(&self.secret, &counter),
            Algorithm::Sha512 => hmac::<Hmac<Sha512>>(&self.secret, &counter),
        };
        let offset = (digest[digest.len() - 1] & 0x0f) as usize;
        let number = u32::from_be_bytes(digest[offset..offset + 4].try_into().expect("4 bytes"))
            & 0x7fff_ffff;
        Code {
            code: format!(
                "{:0width$}",
                number % 10u32.pow(self.digits),
                width = self.digits as usize
            ),
            period_ms,
            remaining_ms: period_ms - now_ms % period_ms,
        }
    }
}

/// The text of the first readable QR code in an image (PNG, JPEG, GIF or
/// WebP), such as an `otpauth://` URI.
///
/// # Errors
/// Returns `Error::InvalidInput("image")` for an unreadable or oversized
/// image and `Error::NotFound` when no QR code reads.
pub fn scan(image: &[u8]) -> Result<String> {
    let invalid = || Error::InvalidInput("image".into());
    let reader = image::ImageReader::new(std::io::Cursor::new(image))
        .with_guessed_format()
        .map_err(|_| invalid())?;
    let (width, height) = reader.into_dimensions().map_err(|_| invalid())?;
    ensure(
        u64::from(width) * u64::from(height) <= MAX_PIXELS,
        invalid(),
    )?;
    let luma = image::load_from_memory(image)
        .map_err(|_| invalid())?
        .to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(
        luma.width() as usize,
        luma.height() as usize,
        |x, y| luma.get_pixel(x as u32, y as u32)[0],
    );
    prepared
        .detect_grids()
        .into_iter()
        .find_map(|grid| grid.decode().ok().map(|(_, text)| text))
        .ok_or(Error::NotFound)
}

#[cfg(test)]
#[path = "otp_tests.rs"]
mod tests;
