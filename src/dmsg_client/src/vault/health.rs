//! A security check of the vault, run on this device: weak and reused
//! passwords, addresses without TLS, and expiry dates passed or close.
//! Findings name items and fields, never values.

use super::{Item, VaultEntry};
use crate::client::Client;
use dmsg_types::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// An expiry this close, in days, is reported as `expiring`.
const SOON_DAYS: i64 = 30;
/// Hidden fields that hold a password people chose.
const PASSWORDS: [&str; 1] = ["password"];
/// Hidden fields whose reuse is reported.
const REUSE: [&str; 2] = ["password", "credential"];
/// Dated fields that are expiries.
const EXPIRIES: [&str; 2] = ["expires", "expiry"];
/// Passwords too common to keep.
const COMMON: [&str; 16] = [
    "password",
    "passw0rd",
    "123456",
    "12345678",
    "123456789",
    "1234567890",
    "qwerty",
    "qwerty123",
    "111111",
    "abc123",
    "letmein",
    "iloveyou",
    "admin",
    "welcome",
    "000000",
    "monkey",
];

/// One finding of [`Client::vault_health`].
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// The item.
    pub id: String,
    /// The field.
    pub field: String,
    /// `weak`, `reused`, `insecure`, `expired` or `expiring`.
    pub issue: &'static str,
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn month_days(year: i64, month: u32) -> u32 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The numbers of `text` split at `-`, when it is digits and dashes in
/// exactly that shape, such as `[4, 2]` for `YYYY-MM`.
fn numbers(text: &str, widths: &[usize]) -> Option<Vec<u32>> {
    let parts: Vec<&str> = text.split('-').collect();
    (parts.len() == widths.len()
        && parts
            .iter()
            .zip(widths)
            .all(|(part, width)| part.len() == *width && part.bytes().all(|b| b.is_ascii_digit())))
    .then(|| parts.iter().map(|p| p.parse().expect("digits")).collect())
}

/// A `date` value, `YYYY-MM-DD`, as days since 1970-01-01.
pub fn days(text: &str) -> Option<i64> {
    let [year, month, day] = numbers(text, &[4, 2, 2])?[..] else {
        return None;
    };
    let year = i64::from(year);
    ((1..=12).contains(&month) && (1..=month_days(year, month)).contains(&day))
        .then(|| days_from_civil(year, month, day))
}

/// A `month` value, `YYYY-MM`, as the days since 1970-01-01 of its last day.
pub fn month_end(text: &str) -> Option<i64> {
    let [year, month] = numbers(text, &[4, 2])?[..] else {
        return None;
    };
    let year = i64::from(year);
    (1..=12)
        .contains(&month)
        .then(|| days_from_civil(year, month, month_days(year, month)))
}

/// Whether a password is easy to guess: short, common, one repeated
/// character, or under 50 bits by its length and kinds of characters.
pub fn weak(password: &str) -> bool {
    let chars: Vec<char> = password.chars().collect();
    if chars.len() < 10 || COMMON.contains(&password.to_lowercase().as_str()) {
        return true;
    }
    if chars.iter().all(|c| *c == chars[0]) {
        return true;
    }
    let pool: u32 = [
        (chars.iter().any(char::is_ascii_lowercase), 26),
        (chars.iter().any(char::is_ascii_uppercase), 26),
        (chars.iter().any(char::is_ascii_digit), 10),
        (
            chars.iter().any(|c| c.is_ascii_punctuation() || *c == ' '),
            33,
        ),
        (chars.iter().any(|c| !c.is_ascii()), 100),
    ]
    .into_iter()
    .filter_map(|(has, size)| has.then_some(size))
    .sum();
    (chars.len() as f64) * f64::from(pool.max(1)).log2() < 50.0
}

fn insecure(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    let local = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host.ends_with(".localhost")
            || host.ends_with(".local")
            || host.ends_with(".test")
            || host == "127.0.0.1"
            || host == "[::1]"
    });
    url.scheme() == "http" && !local
}

/// The findings over items, at `now_ms`.
pub fn findings(items: &[(String, Item)], now_ms: u64) -> Vec<Finding> {
    let today = (now_ms / 86_400_000) as i64;
    let mut found = Vec::new();
    let mut uses: BTreeMap<[u8; 32], Vec<(String, String)>> = BTreeMap::new();
    for (id, item) in items {
        for field in &item.fields {
            let finding = |issue| Finding {
                id: id.clone(),
                field: field.id.clone(),
                issue,
            };
            if field.value.is_empty() {
                continue;
            }
            if field.hidden() && PASSWORDS.contains(&field.id.as_str()) && weak(&field.value) {
                found.push(finding("weak"));
            }
            if field.hidden() && REUSE.contains(&field.id.as_str()) {
                uses.entry(Sha256::digest(field.value.as_bytes()).into())
                    .or_default()
                    .push((id.clone(), field.id.clone()));
            }
            if field.kind == "url" && insecure(&field.value) {
                found.push(finding("insecure"));
            }
            if EXPIRIES.contains(&field.id.as_str()) {
                let end = match field.kind.as_str() {
                    "date" => days(&field.value),
                    "month" => month_end(&field.value),
                    _ => None,
                };
                match end {
                    Some(end) if end < today => found.push(finding("expired")),
                    Some(end) if end - today <= SOON_DAYS => found.push(finding("expiring")),
                    _ => {}
                }
            }
        }
    }
    for places in uses.into_values() {
        let items: std::collections::BTreeSet<&String> = places.iter().map(|(id, _)| id).collect();
        if items.len() > 1 {
            found.extend(places.into_iter().map(|(id, field)| Finding {
                id,
                field,
                issue: "reused",
            }));
        }
    }
    found
}

impl Client {
    /// The security check of the vault's current items, dMsg's own keys
    /// left out.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn vault_health(&self) -> Result<Vec<Finding>> {
        let items: Vec<(String, Item)> = self
            .vault_list(&super::Filter::default())?
            .into_iter()
            .filter(|e| !e.item.reserved())
            .map(|VaultEntry { id, item, .. }| (id, item))
            .collect();
        Ok(findings(&items, crate::now_ms()))
    }
}

#[cfg(test)]
#[path = "health_tests.rs"]
mod tests;
