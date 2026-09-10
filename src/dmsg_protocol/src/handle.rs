use crate::*;
use candid::Principal;
use dmsg_types::*;
use icrc_ledger_types::icrc1::account::Account;

/// Commit to name-registration ledger charge terms under `dmsg/handle-charge/v1`.
///
/// Binds the ledger Principal, explicit ICRC account representation, amount and
/// network fee. Both amounts are integer ledger base units. This does not check
/// allowances, balances, handle validity, or whether the charge has occurred.
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

/// Validate a handle and convert ASCII letters to lowercase.
///
/// Accepts 1..20 ASCII letters, digits or underscores, with no leading underscore.
/// Does not trim whitespace or perform Unicode normalization.
///
/// # Errors
/// Invalid length, prefix or characters return `Error::InvalidInput`.
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

/// Return the fixed PANDA registration price in base units (8 decimal places).
///
/// Call [`normalize_handle`] first: this function only examines byte length and
/// will also return a value for invalid handles. Token prices are 1,000,000 for
/// 1 byte, 200,000 for 2, 50,000 for 3..4, 20,000 for 5..6, and 5,000 otherwise.
/// It does not query a ledger, convert currencies, or quote a network fee.
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
