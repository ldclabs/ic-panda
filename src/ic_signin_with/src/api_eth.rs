use ic_auth_types::{ByteArrayB64, ByteBufB64, SignInResponse};
use ic_auth_verifier::keccak256;
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use sha3::{Digest, Keccak256};

use crate::helper::{
    NANOSECONDS_PER_MILLISECOND, SignInNetwork, issue_delegation, prepare_sign_in_message,
};

const ETH_ADDRESS_BYTES: usize = 20;
const ETH_SIGNATURE_BYTES: usize = 65;

#[ic_cdk::query]
fn get_sign_in_with_ethereum_message(
    domain: String,
    address: String,
    chain_id: u32,
    now_ms: u64,
    session_pubkey: ByteBufB64,
) -> Result<String, String> {
    let local_now_ms = ic_cdk::api::time() / NANOSECONDS_PER_MILLISECOND;
    parse_eth_address(&address)?;
    let prepared = prepare_sign_in_message(
        &domain,
        &address,
        SignInNetwork::Ethereum(chain_id),
        now_ms,
        session_pubkey.as_slice(),
        local_now_ms,
    )?;
    Ok(prepared.message)
}

#[ic_cdk::update]
fn sign_in_with_ethereum(
    domain: String,
    address: String,
    chain_id: u32,
    now_ms: u64,
    session_pubkey: ByteBufB64,
    message_sig: ByteArrayB64<65>,
) -> Result<SignInResponse, String> {
    let local_now_ms = ic_cdk::api::time() / NANOSECONDS_PER_MILLISECOND;
    let expected_address = parse_eth_address(&address)?;
    let prepared = prepare_sign_in_message(
        &domain,
        &address,
        SignInNetwork::Ethereum(chain_id),
        now_ms,
        session_pubkey.as_slice(),
        local_now_ms,
    )?;
    if recover_eth_address(prepared.message.as_bytes(), &message_sig)? != expected_address {
        return Err("signature does not match expected address".to_string());
    }

    Ok(issue_delegation(
        expected_address.to_vec(),
        session_pubkey.as_slice(),
        prepared.expiration,
    ))
}

fn parse_eth_address(address: &str) -> Result<[u8; ETH_ADDRESS_BYTES], String> {
    let encoded = address.strip_prefix("0x").unwrap_or(address);
    if encoded.len() != ETH_ADDRESS_BYTES * 2 {
        return Err("invalid ethereum address".to_string());
    }

    let mut decoded = [0u8; ETH_ADDRESS_BYTES];
    hex::decode_to_slice(encoded, &mut decoded)
        .map_err(|_| "invalid ethereum address".to_string())?;
    Ok(decoded)
}

fn recover_eth_address(
    message: &[u8],
    signature: &[u8; ETH_SIGNATURE_BYTES],
) -> Result<[u8; ETH_ADDRESS_BYTES], String> {
    let recovery_id = match signature[64] {
        value @ 0..=1 => value,
        value @ 27..=28 => value - 27,
        _ => return Err("invalid recovery id".to_string()),
    };
    let recovery_id =
        RecoveryId::try_from(recovery_id).map_err(|_| "invalid recovery id".to_string())?;
    let signature = Signature::from_slice(&signature[..64])
        .map_err(|_| "invalid ethereum signature".to_string())?;

    let verifying_key =
        VerifyingKey::recover_from_prehash(&eip191_hash(message), &signature, recovery_id)
            .map_err(|_| "public key recovery failed".to_string())?;
    let public_key = verifying_key.to_sec1_point(false);
    let digest = keccak256(&public_key.as_bytes()[1..]);
    let mut address = [0u8; ETH_ADDRESS_BYTES];
    address.copy_from_slice(&digest[12..]);
    Ok(address)
}

fn eip191_hash(message: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(b"\x19Ethereum Signed Message:\n");

    let mut length_digits = [0u8; 20];
    let mut cursor = length_digits.len();
    let mut length = message.len();
    loop {
        cursor -= 1;
        length_digits[cursor] = b'0' + (length % 10) as u8;
        length /= 10;
        if length == 0 {
            break;
        }
    }
    hasher.update(&length_digits[cursor..]);
    hasher.update(message);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_canonical_length_eth_addresses() {
        let expected = [0x11; ETH_ADDRESS_BYTES];
        assert_eq!(
            parse_eth_address("0x1111111111111111111111111111111111111111").unwrap(),
            expected
        );
        assert_eq!(
            parse_eth_address("1111111111111111111111111111111111111111").unwrap(),
            expected
        );
        assert!(parse_eth_address("0x1234").is_err());
        assert!(parse_eth_address("0xzz11111111111111111111111111111111111111").is_err());
    }

    #[test]
    fn hashes_eip191_without_copying_the_message() {
        for message in [b"".as_slice(), b"hello".as_slice(), &[42; 1_000]] {
            let encoded = format!(
                "\x19Ethereum Signed Message:\n{}{}",
                message.len(),
                String::from_utf8_lossy(message)
            );
            assert_eq!(eip191_hash(message), keccak256(encoded.as_bytes()));
        }
    }

    #[test]
    fn invalid_recovery_ids_return_an_error_instead_of_trapping() {
        let mut signature = [0; ETH_SIGNATURE_BYTES];
        signature[64] = 2;
        assert!(recover_eth_address(b"message", &signature).is_err());
    }
}
