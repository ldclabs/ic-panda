use super::*;
use crate::vectors::{bytes, key, load, text, value};
use hpke::rand_core::{Infallible, TryCryptoRng, TryRng};

/// Replays fixed bytes, so HPKE derives the vector's ephemeral key from them.
pub(crate) struct Replay(pub(crate) Vec<u8>);

impl TryRng for Replay {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> std::result::Result<u32, Infallible> {
        let mut b = [0; 4];
        self.try_fill_bytes(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }

    fn try_next_u64(&mut self) -> std::result::Result<u64, Infallible> {
        let mut b = [0; 8];
        self.try_fill_bytes(&mut b)?;
        Ok(u64::from_le_bytes(b))
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> std::result::Result<(), Infallible> {
        dst.copy_from_slice(&self.0[..dst.len()]);
        self.0.drain(..dst.len());
        Ok(())
    }
}

impl TryCryptoRng for Replay {}

#[test]
fn derives_keys() {
    let vectors = load("primitives-v1.json");
    for case in vectors["derive"].as_array().unwrap() {
        let context = value(&case["context"]["value"]);
        assert_eq!(canonical(&context), bytes(&case["context"]["hex"]));
        assert_eq!(
            *derive(&bytes(&case["key_hex"]), &context),
            key(&case["output_hex"]),
            "{}",
            case["name"]
        );
    }
    let local = &vectors["local_unlock"];
    assert_eq!(
        *unlock_key(&key(&local["secret_hex"]), text(&local["workspace"])),
        key(&local["unlock_hex"])
    );
}

#[test]
fn seals_cose_encrypt0_exactly() {
    let vectors = load("primitives-v1.json");
    for case in vectors["seal"].as_array().unwrap() {
        let (k, aad) = (key(&case["key_hex"]), value(&case["aad"]["value"]));
        let nonce = bytes(&case["iv_hex"]).try_into().unwrap();
        let plaintext = bytes(&case["plaintext_hex"]);
        assert_eq!(enc_structure(&aad), bytes(&case["enc_structure_hex"]));
        let sealed = seal_with_nonce(&k, &plaintext, &aad, &nonce);
        assert_eq!(b64(&sealed), text(&case["sealed"]), "{}", case["name"]);
        assert_eq!(sealed, bytes(&case["cose_hex"]));
        assert_eq!(*open(&k, &b64(&sealed), &aad).unwrap(), plaintext);
        assert_eq!(*open_bytes(&k, &sealed, &aad).unwrap(), plaintext);
        assert_ne!(seal_bytes(&k, &plaintext, &aad), sealed);
        // The canonical encoder writes the same bytes.
        assert_eq!(
            canonical(&(
                serde_bytes::Bytes::new(&PROTECTED),
                std::collections::BTreeMap::from([(5u8, serde_bytes::Bytes::new(&nonce))]),
                serde_bytes::Bytes::new(&sealed[sealed.len() - plaintext.len() - 16..]),
            )),
            sealed
        );
    }
}

#[test]
fn open_rejects_altered_inputs() {
    let vectors = load("primitives-v1.json");
    for case in vectors["open_reject"].as_array().unwrap() {
        let result = open(
            &key(&case["key_hex"]),
            text(&case["sealed"]),
            &value(&case["aad"]["value"]),
        );
        assert!(result.is_err(), "{}", case["name"]);
    }
}

#[test]
fn seals_hpke_exactly_with_fixed_ephemeral_keys() {
    let vectors = load("primitives-v1.json");
    for case in vectors["hpke"].as_array().unwrap() {
        let seed = key(&case["seed_hex"]);
        let public = hpke_public(&seed);
        assert_eq!(public, key(&case["public_hex"]));
        let context = value(&case["context"]["value"]);
        assert_eq!(canonical(&context), bytes(&case["context"]["hex"]));
        let plaintext = bytes(&case["plaintext_hex"]);
        let envelope = hpke_seal_with(
            &OpModeS::Base,
            &public,
            &plaintext,
            &canonical(&context),
            &mut Replay(bytes(&case["ekm_hex"])),
        )
        .unwrap();
        assert_eq!(envelope.enc, bytes(&case["enc_hex"]), "{}", case["name"]);
        assert_eq!(envelope.ciphertext, bytes(&case["ciphertext_hex"]));
        assert_eq!(canonical(&envelope), bytes(&case["envelope_hex"]));
        assert_eq!(*hpke_open(&seed, &envelope, &context).unwrap(), plaintext);
        assert!(hpke_open(&[0x99; 32], &envelope, &context).is_err());
        assert!(hpke_open(&seed, &envelope, &("other",)).is_err());
        let fresh = hpke_seal(&public, &plaintext, &context).unwrap();
        assert_ne!(fresh.enc, envelope.enc);
        assert_eq!(*hpke_open(&seed, &fresh, &context).unwrap(), plaintext);
    }
}

#[test]
fn signs_ed25519_deterministically() {
    let vectors = load("primitives-v1.json");
    for case in vectors["ed25519"].as_array().unwrap() {
        let seed = key(&case["seed_hex"]);
        let public = signing_public(&seed);
        assert_eq!(public, key(&case["public_hex"]));
        let message = bytes(&case["message_hex"]);
        let signature = sign(&seed, &message);
        assert_eq!(signature.to_vec(), bytes(&case["signature_hex"]));
        dmsg_protocol::verify(&dmsg_types::Hash::new(public), &message, &signature).unwrap();
    }
}
