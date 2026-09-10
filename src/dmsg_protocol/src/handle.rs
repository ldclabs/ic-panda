use crate::*;
use candid::Principal;
use dmsg_types::*;
use icrc_ledger_types::icrc1::account::Account;

pub fn charge_terms_digest(ledger: Principal, payer: &Account, amount: u128, fee: u128) -> Hash {
    digest(
        "dmsg/handle-charge/v1",
        &(
            ledger,
            dmsg_types::account::account_cbor::value(payer),
            amount,
            fee,
        ),
    )
}

pub fn normalize_handle(handle: &str) -> Result<String> {
    ensure_valid(
        !handle.is_empty() && handle.len() <= 20 && !handle.starts_with('_'),
        "handle length/prefix",
    )?;
    ensure_valid(
        handle
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_'),
        "handle characters",
    )?;
    Ok(handle.to_ascii_lowercase())
}

/// Price in the token's smallest units; the caller must validate the handle first.
pub fn price(handle: &str) -> u128 {
    let tokens = match handle.len() {
        1 => 1_000_000,
        2 => 200_000,
        3 | 4 => 50_000,
        5 | 6 => 20_000,
        _ => 5_000,
    };
    tokens * 100_000_000
}

#[cfg(test)]
mod tests;
