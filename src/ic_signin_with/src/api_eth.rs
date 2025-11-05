use ic_auth_types::{ByteArrayB64, ByteBufB64, BytesB64, SignInResponse};
use ic_auth_verifier::{keccak256, sha3_256, user_public_key_from_der, verify_basic_sig};
use ic_canister_sig_creation::{delegation_signature_msg, CanisterSigPublicKey};
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use time::{macros::format_description, OffsetDateTime};

use crate::{
    helper::{CLOCK_SKEW_MS, MILLISECONDS},
    store,
};

#[ic_cdk::query]
fn get_sign_in_with_ethereum_message(
    domain: String,  // Domain requesting sign-in
    address: String, // Evm address in hex format
    chain_id: u32,   // Evm chain ID
    now_ms: u64,     // Request timestamp in milliseconds
) -> Result<String, String> {
    let (_, msg) = sign_in_with_ethereum_message(domain, address, chain_id, now_ms)?;
    Ok(msg)
}

#[allow(clippy::too_many_arguments)]
#[ic_cdk::update]
fn sign_in_with_ethereum(
    domain: String,                // Domain requesting sign-in
    address: String,               // Evm address in hex format
    chain_id: u32,                 // Evm chain ID
    now_ms: u64,                   // Request timestamp in milliseconds
    message: String,               // Original message that was signed
    message_sig: ByteBufB64,       // Signature over message
    session_pubkey: ByteBufB64,    // Session public key in DER format
    session_sig: ByteArrayB64<64>, // Signature over challenge
) -> Result<SignInResponse, String> {
    let local_now_ms = ic_cdk::api::time() / MILLISECONDS;

    let addr = hex::decode(address.trim_start_matches("0x"))
        .map_err(|_| "invalid ethereum address".to_string())?;
    let (pubkey, msg) = sign_in_with_ethereum_message(domain, address, chain_id, now_ms)?;
    if !message.starts_with(&msg) {
        return Err("signed message does not match expected message".to_string());
    }

    let _addr = recover_eth_address(&message, &message_sig)?;
    if addr != _addr {
        return Err("signature does not match expected address".to_string());
    }

    let (alg, pk) = user_public_key_from_der(session_pubkey.as_slice())
        .map_err(|err| format!("invalid public key: {:?}", err))?;
    verify_basic_sig(alg, &pk, message_sig.as_slice(), session_sig.as_slice())
        .map_err(|err| format!("challenge verification failed: {:?}", err))?;

    let user_key = CanisterSigPublicKey::new(ic_cdk::api::canister_self(), pubkey.to_vec());
    let session_expires_in_ms = store::state::with(|state| state.session_expires_in_ms);
    let expiration = (local_now_ms + session_expires_in_ms) * MILLISECONDS;
    let delegation_hash = delegation_signature_msg(session_pubkey.as_slice(), expiration, None);
    store::state::add_signature(user_key.seed.as_slice(), delegation_hash.as_slice());

    Ok(SignInResponse {
        expiration,
        user_key: user_key.to_der().into(),
        seed: user_key.seed.into(),
    })
}

fn sign_in_with_ethereum_message(
    domain: String,
    address: String,
    chain_id: u32,
    now_ms: u64,
) -> Result<([u8; 20], String), String> {
    let addr = hex::decode(address.trim_start_matches("0x"))
        .map_err(|_| "invalid ethereum address".to_string())?;
    let addr: [u8; 20] = addr
        .try_into()
        .map_err(|_| "invalid ethereum address".to_string())?;

    let local_now_ms = ic_cdk::api::time() / MILLISECONDS;
    if now_ms < local_now_ms - CLOCK_SKEW_MS || now_ms > local_now_ms + CLOCK_SKEW_MS {
        return Err("timestamp is not within acceptable range".to_string());
    }

    store::state::with(|state| {
        let uri = state
            .domains
            .get(&domain)
            .cloned()
            .ok_or_else(|| format!("unsupported domain: {}", domain))?;

        let mut nonce_seed = Vec::with_capacity(40);
        nonce_seed.extend_from_slice(&now_ms.to_be_bytes());
        nonce_seed.extend_from_slice(state.nonce_iv.as_slice());

        let nonce = sha3_256(nonce_seed.as_slice());
        let siws_message = SiweMessage {
            domain,
            address,
            statement: state.statement.clone(),
            uri,
            version: 1,
            chain_id,
            nonce: BytesB64::from(&nonce[..12]).to_string(),
            issued_at: now_ms,
            expiration_time: now_ms + state.session_expires_in_ms,
        };
        Ok((addr, siws_message.to_string()))
    })
}

#[derive(Debug, Clone)]
pub struct SiweMessage {
    // RFC 4501 dns authority that is requesting the signing.
    pub domain: String,

    // Solana address performing the signing
    pub address: String,

    // Human-readable ASCII assertion for the user to sign; optional and must not contain newline characters.
    pub statement: String,

    // RFC 3986 URI referring to the resource that is the subject of the signing
    pub uri: String,

    // Current version of the message.
    pub version: u32,

    // Chain ID to which the session is bound, optional
    pub chain_id: u32,

    // Randomized token used to prevent replay attacks
    pub nonce: String,

    /// Timestamp in milliseconds
    pub issued_at: u64,

    /// Timestamp in milliseconds
    pub expiration_time: u64,
}

impl std::fmt::Display for SiweMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Custom date format to match the JS ISO 8601 format that has less precision than the default Rfc3339 format.
        let js_iso_format = format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z");

        let issued_at_iso_8601 =
            OffsetDateTime::from_unix_timestamp((self.issued_at / 1000) as i64)
                .unwrap()
                .format(&js_iso_format)
                .unwrap();
        let expiration_iso_8601 =
            OffsetDateTime::from_unix_timestamp((self.expiration_time / 1000) as i64)
                .unwrap()
                .format(&js_iso_format)
                .unwrap();

        write!(
            f,
            "{domain} wants you to sign in with your Ethereum account:\n\
            {address}\n\n\
            {statement}\n\n\
            URI: {uri}\n\
            Version: {version}\n\
            Chain ID: {chain_id}\n\
            Nonce: {nonce}\n\
            Issued At: {issued_at_iso_8601}\n\
            Expiration Time: {expiration_iso_8601}",
            domain = self.domain,
            address = self.address,
            statement = self.statement,
            uri = self.uri,
            version = self.version,
            chain_id = self.chain_id,
            nonce = self.nonce,
        )
    }
}

fn recover_eth_address(message: &str, signature: &[u8]) -> Result<[u8; 20], String> {
    let message_hash = eip191_hash(message);

    let recovery_id = RecoveryId::try_from(signature[64] % 27)
        .map_err(|_| String::from("Invalid recovery ID"))?;

    let signature =
        Signature::from_slice(&signature[..64]).map_err(|_| String::from("Invalid signature"))?;

    let verifying_key = VerifyingKey::recover_from_prehash(&message_hash, &signature, recovery_id)
        .map_err(|_| String::from("Public key recovery failure"))?;
    let mut addr: [u8; 20] = [0; 20];
    addr.copy_from_slice(&keccak256(&verifying_key.to_encoded_point(false).as_bytes()[1..])[12..]);
    Ok(addr)
}

/// Hashes a message using the EIP-191 standard. See [EIP-191 spec](https://eips.ethereum.org/EIPS/eip-191) for more information.
fn eip191_hash(message: &str) -> [u8; 32] {
    keccak256(&eip191_bytes(message))
}

/// Formats a message according to the EIP-191 standard. See [EIP-191 spec](https://eips.ethereum.org/EIPS/eip-191) for for more information.
fn eip191_bytes(message: &str) -> Vec<u8> {
    format!("\x19Ethereum Signed Message:\n{}{}", message.len(), message).into_bytes()
}
