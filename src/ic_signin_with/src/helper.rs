use base64::{Engine, engine::general_purpose::URL_SAFE as BASE64_URL};
use ic_auth_types::SignInResponse;
use ic_canister_sig_creation::{CanisterSigPublicKey, delegation_signature_msg};
use sha3::{Digest, Sha3_256};
use std::fmt;

use crate::store;

pub const NANOSECONDS_PER_MILLISECOND: u64 = 1_000_000;
pub const CLOCK_SKEW_MS: u64 = 2 * 60 * 1_000;
pub const MAX_DOMAIN_BYTES: usize = 255;
pub const MAX_URI_BYTES: usize = 2_048;
pub const MAX_STATEMENT_BYTES: usize = 1_024;
pub const MAX_PUBLIC_KEY_DER_BYTES: usize = 128;

#[derive(Clone, Copy)]
pub enum SignInNetwork {
    Ethereum(u32),
    Solana,
}

impl SignInNetwork {
    fn account_name(self) -> &'static str {
        match self {
            Self::Ethereum(_) => "Ethereum",
            Self::Solana => "Solana",
        }
    }
}

impl fmt::Display for SignInNetwork {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ethereum(chain_id) => write!(f, "{chain_id}"),
            Self::Solana => f.write_str("mainnet"),
        }
    }
}

pub struct PreparedSignInMessage {
    pub message: String,
    /// The Expiration Time the wallet signs, in nanoseconds.
    pub expiration: u64,
}

/// Render the message a wallet signs to bind its account to `session_pubkey`.
/// `local_now_ms` is the canister time; `now_ms` is the client's.
pub fn prepare_sign_in_message(
    domain: &str,
    address: &str,
    network: SignInNetwork,
    now_ms: u64,
    session_pubkey: &[u8],
    local_now_ms: u64,
) -> Result<PreparedSignInMessage, String> {
    if session_pubkey.is_empty() || session_pubkey.len() > MAX_PUBLIC_KEY_DER_BYTES {
        return Err("invalid public key length".to_string());
    }
    if now_ms.abs_diff(local_now_ms) > CLOCK_SKEW_MS {
        return Err("timestamp is not within acceptable range".to_string());
    }

    store::state::with(|state| {
        let uri = state
            .domains
            .get(domain)
            .ok_or_else(|| format!("unsupported domain: {domain}"))?;
        // Nanoseconds in a u64 end in 2554, so the timestamps keep four-digit years.
        let expiration_ms = now_ms.saturating_add(state.session_expires_in_ms);
        let expiration = expiration_ms
            .checked_mul(NANOSECONDS_PER_MILLISECOND)
            .ok_or_else(|| "session expiration is out of range".to_string())?;

        let message = SignInMessage {
            domain,
            address,
            statement: &state.statement,
            uri,
            network,
            nonce: sign_in_nonce(now_ms, session_pubkey),
            issued_at: now_ms,
            expiration_time: expiration_ms,
        }
        .to_string();
        Ok(PreparedSignInMessage {
            message,
            expiration,
        })
    })
}

/// Certify a delegation from the wallet's canister key to the session key.
pub fn issue_delegation(seed: Vec<u8>, session_pubkey: &[u8], expiration: u64) -> SignInResponse {
    let user_key = CanisterSigPublicKey::new(ic_cdk::api::canister_self(), seed);
    let delegation_hash = delegation_signature_msg(session_pubkey, expiration, None);
    store::state::add_signature(&user_key.seed, &delegation_hash);

    SignInResponse {
        expiration,
        user_key: user_key.to_der().into(),
        seed: user_key.seed.into(),
    }
}

/// The nonce commits to the session key, so a wallet signature authorizes only
/// that key: replayed with any other key, it signs a different message.
fn sign_in_nonce(now_ms: u64, session_pubkey: &[u8]) -> [u8; 12] {
    let digest = Sha3_256::new()
        .chain_update(now_ms.to_be_bytes())
        .chain_update(session_pubkey)
        .finalize();
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&digest[..12]);
    nonce
}

struct SignInMessage<'a> {
    domain: &'a str,
    address: &'a str,
    statement: &'a str,
    uri: &'a str,
    network: SignInNetwork,
    nonce: [u8; 12],
    issued_at: u64,
    expiration_time: u64,
}

// Wallets sign this rendering verbatim, so every byte of it is wire format. The
// nonce is encoded here rather than through a helper type's `Display`: that of
// `ic_auth_types::BytesB64` grew a `b64:` prefix in 0.10, which would have
// invalidated every sign-in message without changing a line of this crate.
impl fmt::Display for SignInMessage<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{domain} wants you to sign in with your {account_name} account:\n\
             {address}\n\n\
             {statement}\n\n\
             URI: {uri}\n\
             Version: 1\n\
             Chain ID: {network}\n\
             Nonce: {nonce}\n\
             Issued At: {issued_at}\n\
             Expiration Time: {expiration_time}",
            domain = self.domain,
            account_name = self.network.account_name(),
            address = self.address,
            statement = self.statement,
            uri = self.uri,
            network = self.network,
            nonce = BASE64_URL.encode(self.nonce),
            issued_at = Rfc3339Seconds(self.issued_at / 1_000),
            expiration_time = Rfc3339Seconds(self.expiration_time / 1_000),
        )
    }
}

struct Rfc3339Seconds(u64);

impl fmt::Display for Rfc3339Seconds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let days = self.0 / 86_400;
        let seconds_of_day = self.0 % 86_400;
        let (year, month, day) = civil_date_from_unix_days(days);
        let hour = seconds_of_day / 3_600;
        let minute = (seconds_of_day % 3_600) / 60;
        let second = seconds_of_day % 60;
        write!(
            f,
            "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
        )
    }
}

// Howard Hinnant's civil-from-days algorithm, specialized for dates after 1970.
fn civil_date_from_unix_days(days: u64) -> (u64, u64, u64) {
    let shifted_days = days + 719_468;
    let era = shifted_days / 146_097;
    let day_of_era = shifted_days % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_rfc3339_without_allocating_intermediate_dates() {
        assert_eq!(Rfc3339Seconds(0).to_string(), "1970-01-01T00:00:00Z");
        assert_eq!(
            Rfc3339Seconds(951_782_400).to_string(),
            "2000-02-29T00:00:00Z"
        );
        assert_eq!(
            Rfc3339Seconds(1_735_689_599).to_string(),
            "2024-12-31T23:59:59Z"
        );
        assert_eq!(
            Rfc3339Seconds(u64::MAX / NANOSECONDS_PER_MILLISECOND / 1_000).to_string(),
            "2554-07-21T23:34:33Z"
        );
    }

    #[test]
    fn formats_the_existing_sign_in_message_shape() {
        let message = SignInMessage {
            domain: "example.com",
            address: "0x1234",
            statement: "Sign in",
            uri: "https://example.com",
            network: SignInNetwork::Ethereum(1),
            nonce: [0; 12],
            issued_at: 1_735_689_600_000,
            expiration_time: 1_735_693_200_000,
        }
        .to_string();

        assert_eq!(
            message,
            "example.com wants you to sign in with your Ethereum account:\n\
             0x1234\n\n\
             Sign in\n\n\
             URI: https://example.com\n\
             Version: 1\n\
             Chain ID: 1\n\
             Nonce: AAAAAAAAAAAAAAAA\n\
             Issued At: 2025-01-01T00:00:00Z\n\
             Expiration Time: 2025-01-01T01:00:00Z"
        );
    }

    #[test]
    fn the_nonce_commits_to_the_time_and_the_session_key() {
        let nonce = sign_in_nonce(1_735_689_600_000, &[1; 44]);
        // SHA3-256 of the big-endian time and the key, as clients may compute it.
        assert_eq!(BASE64_URL.encode(nonce), "6zpoWUtESbClg_hq");
        assert_ne!(nonce, sign_in_nonce(1_735_689_600_000, &[2; 44]));
        assert_ne!(nonce, sign_in_nonce(1_735_689_600_001, &[1; 44]));
    }
}
