//! Stable account binding shared by StoreKit clients and the verification service.
use candid::Principal;
use dmsg_types::AccountId;
use sha2::{Digest, Sha256};

/// A UUID-shaped token binding purchases to both the user home and account.
/// It carries no authentication or read authority.
pub fn account_token(home: Principal, account: &AccountId) -> String {
    let mut bytes = Sha256::digest(format!(
        "dmsg/apple/account/1\n{}\n{account}",
        home.to_text()
    ));
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let h: String = bytes[..16].iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}
