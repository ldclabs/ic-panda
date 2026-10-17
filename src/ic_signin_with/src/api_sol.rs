use ic_auth_types::{ByteArrayB64, ByteBufB64, SignInResponse};
use ic_auth_verifier::{Algorithm, verify_basic_sig};

use crate::helper::{
    NANOSECONDS_PER_MILLISECOND, SignInNetwork, issue_delegation, prepare_sign_in_message,
};

const SOLANA_PUBLIC_KEY_BYTES: usize = 32;
const MAX_SOLANA_ADDRESS_BYTES: usize = 44;

#[ic_cdk::query]
fn get_sign_in_with_solana_message(
    domain: String,
    address: String,
    now_ms: u64,
    session_pubkey: ByteBufB64,
) -> Result<String, String> {
    let local_now_ms = ic_cdk::api::time() / NANOSECONDS_PER_MILLISECOND;
    parse_solana_address(&address)?;
    let prepared = prepare_sign_in_message(
        &domain,
        &address,
        SignInNetwork::Solana,
        now_ms,
        session_pubkey.as_slice(),
        local_now_ms,
    )?;
    Ok(prepared.message)
}

#[ic_cdk::update]
fn sign_in_with_solana(
    domain: String,
    address: String,
    now_ms: u64,
    session_pubkey: ByteBufB64,
    message_sig: ByteArrayB64<64>,
) -> Result<SignInResponse, String> {
    let local_now_ms = ic_cdk::api::time() / NANOSECONDS_PER_MILLISECOND;
    let public_key = parse_solana_address(&address)?;
    let prepared = prepare_sign_in_message(
        &domain,
        &address,
        SignInNetwork::Solana,
        now_ms,
        session_pubkey.as_slice(),
        local_now_ms,
    )?;
    verify_basic_sig(
        Algorithm::Ed25519,
        &public_key,
        prepared.message.as_bytes(),
        message_sig.as_slice(),
    )?;

    Ok(issue_delegation(
        public_key.to_vec(),
        session_pubkey.as_slice(),
        prepared.expiration,
    ))
}

fn parse_solana_address(address: &str) -> Result<[u8; SOLANA_PUBLIC_KEY_BYTES], String> {
    if address.is_empty() || address.len() > MAX_SOLANA_ADDRESS_BYTES {
        return Err("invalid solana address".to_string());
    }

    let mut public_key = [0u8; SOLANA_PUBLIC_KEY_BYTES];
    let decoded_len = bs58::decode(address)
        .onto(&mut public_key)
        .map_err(|_| "invalid solana address".to_string())?;
    if decoded_len != SOLANA_PUBLIC_KEY_BYTES {
        return Err("invalid solana address".to_string());
    }
    Ok(public_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_solana_keys_without_an_intermediate_vec() {
        assert_eq!(
            parse_solana_address("11111111111111111111111111111111").unwrap(),
            [0; SOLANA_PUBLIC_KEY_BYTES]
        );
        assert!(parse_solana_address("").is_err());
        assert!(parse_solana_address("1").is_err());
        assert!(parse_solana_address(&"1".repeat(MAX_SOLANA_ADDRESS_BYTES + 1)).is_err());
    }
}
