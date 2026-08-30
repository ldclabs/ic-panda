use candid::Principal;
use ic_auth_types::BytesB64;
use ic_auth_verifier::{sha3_256, user_public_key_from_der, verify_basic_sig, Algorithm};
use std::fmt::{self, Write as _};

use crate::store;

pub const NANOSECONDS_PER_MILLISECOND: u64 = 1_000_000;
pub const CLOCK_SKEW_MS: u64 = 2 * 60 * 1_000;
pub const MAX_DOMAIN_BYTES: usize = 255;
pub const MAX_URI_BYTES: usize = 2_048;
pub const MAX_STATEMENT_BYTES: usize = 1_024;
pub const MAX_SIGN_IN_MESSAGE_BYTES: usize = 4_096;
pub const MAX_PUBLIC_KEY_DER_BYTES: usize = 128;
pub const MAX_SIGNED_ENVELOPE_BYTES: usize = 256 * 1_024;

const BASIC_SIGNATURE_BYTES: usize = 64;
const MAX_RFC3339_TIMESTAMP_MS: u64 = 253_402_300_799_999;
const ED25519_DER_PREFIX: &[u8] = &[
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

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
    pub local_now_ms: u64,
    pub session_expires_in_ms: u64,
}

pub fn authenticated_caller() -> Result<Principal, String> {
    let caller = ic_cdk::api::msg_caller();
    if caller == Principal::anonymous() {
        Err("anonymous user is not allowed".to_string())
    } else {
        Ok(caller)
    }
}

pub fn validate_sign_in_request(
    domain: &str,
    message: &str,
    session_pubkey: &[u8],
    session_sig: &[u8],
) -> Result<(), String> {
    if domain.is_empty() || domain.len() > MAX_DOMAIN_BYTES {
        return Err("invalid domain length".to_string());
    }
    if message.is_empty() || message.len() > MAX_SIGN_IN_MESSAGE_BYTES {
        return Err("invalid signed message length".to_string());
    }
    validate_session_key(session_pubkey, session_sig)
}

pub fn verify_session_signature(
    session_pubkey: &[u8],
    challenge: &[u8],
    session_sig: &[u8],
) -> Result<(), String> {
    validate_session_key(session_pubkey, session_sig)?;

    if let Some(raw_key) = session_pubkey.strip_prefix(ED25519_DER_PREFIX) {
        if raw_key.len() == 32 {
            return verify_basic_sig(Algorithm::Ed25519, raw_key, challenge, session_sig)
                .map_err(|err| format!("challenge verification failed: {err}"));
        }
    }

    let (algorithm, raw_key) = user_public_key_from_der(session_pubkey)
        .map_err(|err| format!("invalid public key: {err}"))?;
    verify_basic_sig(algorithm, &raw_key, challenge, session_sig)
        .map_err(|err| format!("challenge verification failed: {err}"))
}

fn validate_session_key(session_pubkey: &[u8], session_sig: &[u8]) -> Result<(), String> {
    if session_pubkey.is_empty() || session_pubkey.len() > MAX_PUBLIC_KEY_DER_BYTES {
        return Err("invalid public key length".to_string());
    }
    if session_sig.len() != BASIC_SIGNATURE_BYTES {
        return Err("invalid session signature length".to_string());
    }
    Ok(())
}

pub fn prepare_sign_in_message(
    domain: &str,
    address: &str,
    network: SignInNetwork,
    now_ms: u64,
) -> Result<PreparedSignInMessage, String> {
    if domain.is_empty() || domain.len() > MAX_DOMAIN_BYTES {
        return Err("invalid domain length".to_string());
    }

    let local_now_ms = ic_cdk::api::time() / NANOSECONDS_PER_MILLISECOND;
    if now_ms.abs_diff(local_now_ms) > CLOCK_SKEW_MS {
        return Err("timestamp is not within acceptable range".to_string());
    }

    store::state::with(|state| {
        if state.nonce_iv.as_slice().iter().all(|byte| *byte == 0) {
            return Err("canister is still initializing".to_string());
        }
        if state.statement.len() > MAX_STATEMENT_BYTES || state.statement.contains(['\r', '\n']) {
            return Err("configured statement is invalid".to_string());
        }

        let uri = state
            .domains
            .get(domain)
            .ok_or_else(|| format!("unsupported domain: {domain}"))?;
        if uri.len() > MAX_URI_BYTES {
            return Err("configured uri is too long".to_string());
        }

        let expiration_time = now_ms
            .checked_add(state.session_expires_in_ms)
            .filter(|timestamp| *timestamp <= MAX_RFC3339_TIMESTAMP_MS)
            .ok_or_else(|| "session expiration is out of range".to_string())?;

        let mut nonce_seed = [0u8; 40];
        nonce_seed[..8].copy_from_slice(&now_ms.to_be_bytes());
        nonce_seed[8..].copy_from_slice(state.nonce_iv.as_slice());
        let nonce_hash = sha3_256(&nonce_seed);
        let nonce: [u8; 12] = nonce_hash[..12]
            .try_into()
            .expect("nonce hash prefix has a fixed length");

        let sign_in_message = SignInMessage {
            domain,
            address,
            statement: &state.statement,
            uri,
            network,
            nonce,
            issued_at: now_ms,
            expiration_time,
        };
        let mut message = String::with_capacity(
            domain.len() + address.len() + state.statement.len() + uri.len() + 192,
        );
        write!(&mut message, "{sign_in_message}")
            .map_err(|_| "failed to format sign-in message".to_string())?;

        if message.len() > MAX_SIGN_IN_MESSAGE_BYTES {
            return Err("generated sign-in message is too long".to_string());
        }

        Ok(PreparedSignInMessage {
            message,
            local_now_ms,
            session_expires_in_ms: state.session_expires_in_ms,
        })
    })
}

pub fn delegation_expiration_ns(
    local_now_ms: u64,
    session_expires_in_ms: u64,
) -> Result<u64, String> {
    local_now_ms
        .checked_add(session_expires_in_ms)
        .and_then(|timestamp| timestamp.checked_mul(NANOSECONDS_PER_MILLISECOND))
        .ok_or_else(|| "session expiration overflow".to_string())
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
            nonce = BytesB64::from(&self.nonce[..]),
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
            Rfc3339Seconds(253_402_300_799).to_string(),
            "9999-12-31T23:59:59Z"
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
    fn rejects_unbounded_inputs_before_crypto() {
        assert!(validate_session_key(&[], &[0; 64]).is_err());
        assert!(validate_session_key(&[0; MAX_PUBLIC_KEY_DER_BYTES + 1], &[0; 64]).is_err());
        assert!(validate_session_key(&[0; 44], &[0; 63]).is_err());
        assert!(validate_sign_in_request("", "message", &[0; 44], &[0; 64]).is_err());
        assert!(validate_sign_in_request(
            "example.com",
            &"x".repeat(MAX_SIGN_IN_MESSAGE_BYTES + 1),
            &[0; 44],
            &[0; 64]
        )
        .is_err());
    }
}
