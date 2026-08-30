use ed25519_dalek::{Signature, VerifyingKey};
use ic_auth_types::{ByteArrayB64, ByteBufB64, SignInResponse};
use ic_canister_sig_creation::{delegation_signature_msg, CanisterSigPublicKey};

use crate::{
    helper::{
        delegation_expiration_ns, prepare_sign_in_message, validate_sign_in_request,
        verify_session_signature, SignInNetwork,
    },
    store,
};

const SOLANA_PUBLIC_KEY_BYTES: usize = 32;
const MAX_SOLANA_ADDRESS_BYTES: usize = 44;

#[ic_cdk::query]
fn get_sign_in_with_solana_message(
    domain: String,
    address: String,
    now_ms: u64,
) -> Result<String, String> {
    parse_solana_address(&address)?;
    Ok(prepare_sign_in_message(&domain, &address, SignInNetwork::Solana, now_ms)?.message)
}

#[ic_cdk::update]
fn sign_in_with_solana(
    domain: String,
    address: String,
    now_ms: u64,
    message: String,
    message_sig: ByteArrayB64<64>,
    session_pubkey: ByteBufB64,
    session_sig: ByteArrayB64<64>,
) -> Result<SignInResponse, String> {
    validate_sign_in_request(
        &domain,
        &message,
        session_pubkey.as_slice(),
        session_sig.as_slice(),
    )?;

    let public_key = parse_solana_address(&address)?;
    let prepared = prepare_sign_in_message(&domain, &address, SignInNetwork::Solana, now_ms)?;
    if !message.starts_with(&prepared.message) {
        return Err("signed message does not match expected message".to_string());
    }

    let verifying_key =
        VerifyingKey::from_bytes(&public_key).map_err(|_| "invalid public key".to_string())?;
    let signature = Signature::from_bytes(&message_sig);
    verifying_key
        .verify_strict(message.as_bytes(), &signature)
        .map_err(|_| "verification failed".to_string())?;
    verify_session_signature(
        session_pubkey.as_slice(),
        message_sig.as_slice(),
        session_sig.as_slice(),
    )?;

    let user_key = CanisterSigPublicKey::new(ic_cdk::api::canister_self(), public_key.to_vec());
    let expiration =
        delegation_expiration_ns(prepared.local_now_ms, prepared.session_expires_in_ms)?;
    let delegation_hash = delegation_signature_msg(session_pubkey.as_slice(), expiration, None);
    store::state::add_signature(user_key.seed.as_slice(), delegation_hash.as_slice());

    Ok(SignInResponse {
        expiration,
        user_key: user_key.to_der().into(),
        seed: user_key.seed.into(),
    })
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
