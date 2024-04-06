use base64::{engine::general_purpose, Engine};
use ciborium::{from_reader, into_writer};
use hmac::{Hmac, Mac};
use serde::Serialize;
use serde_bytes::ByteBuf;
use sha3::{Digest, Sha3_256};

pub fn sha3_256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    hasher.finalize().into()
}

pub fn mac_256(key: &[u8], add: &[u8]) -> [u8; 32] {
    let mut mac = Hmac::<Sha3_256>::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(add);
    mac.finalize().into_bytes().into()
}

pub fn mac_256_2(key: &[u8], add1: &[u8], add2: &[u8]) -> [u8; 32] {
    let mut mac = Hmac::<Sha3_256>::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(add1);
    mac.update(add2);
    mac.finalize().into_bytes().into()
}

// to_cbor_bytes returns the CBOR encoding of the given object that implements the Serialize trait.
pub fn to_cbor_bytes(obj: &impl Serialize) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    into_writer(obj, &mut buf).expect("Failed to encode in CBOR format");
    buf
}

// Challenge is a trait for generating and verifying challenges.
pub trait Challenge {
    fn challenge(&self, key: &[u8], timestamp: u64) -> Vec<u8>;
    fn verify(&self, key: &[u8], expire_at: u64, challenge: &[u8]) -> Result<(), String>;
}

// Implement the Challenge trait for any type that implements the Serialize trait.
impl<T> Challenge for T
where
    T: Serialize,
{
    fn challenge(&self, key: &[u8], timestamp: u64) -> Vec<u8> {
        let mac = &mac_256_2(key, &to_cbor_bytes(self), &to_cbor_bytes(&timestamp))[0..16];
        to_cbor_bytes(&(timestamp, ByteBuf::from(mac)))
    }

    fn verify(&self, key: &[u8], expire_at: u64, challenge: &[u8]) -> Result<(), String> {
        let arr: (u64, ByteBuf) =
            from_reader(challenge).map_err(|_err| "failed to decode the challenge")?;

        if arr.0 < expire_at {
            return Err("the challenge is expired".to_string());
        }

        let mac = &mac_256_2(key, &to_cbor_bytes(self), &to_cbor_bytes(&arr.0))[0..16];
        if mac != &arr.1[..] {
            return Err("failed to verify the challenge".to_string());
        }

        Ok(())
    }
}

pub fn luckycode_to_string(code: u32) -> String {
    general_purpose::URL_SAFE_NO_PAD.encode(code.to_be_bytes())
}

pub fn luckycode_from_string(code: &str) -> Result<u32, String> {
    let code = general_purpose::URL_SAFE_NO_PAD
        .decode(code.as_bytes())
        .map_err(|_err| "invalid lucky code".to_string())?;
    if code.len() != 4 {
        return Err("invalid lucky code".to_string());
    }
    Ok(u32::from_be_bytes([code[0], code[1], code[2], code[3]]))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_challenge() {
        let key = b"secret key";
        let challenge = "challenge";
        let expire_at = 1000;
        let c = challenge.challenge(key, expire_at);
        println!("challenge: {}, {:?}", c.len(), c);
        assert!(c.len() < 24);
        assert!(challenge.verify(key, expire_at, &c).is_ok());
        assert!(challenge.verify(key, expire_at, &c[1..]).is_err());
        assert!(challenge.verify(&key[1..], expire_at, &c).is_err());
        assert!(challenge.verify(key, expire_at + 1, &c).is_err());
    }

    #[test]
    fn test_luckycode() {
        for code in [u32::MIN, 9, 0x12345678, 0x87654321, u32::MAX].iter() {
            let s = luckycode_to_string(*code); // 0, "AAAAAA"
            let c = luckycode_from_string(&s).unwrap();
            assert_eq!(code, &c);
        }
    }
}
