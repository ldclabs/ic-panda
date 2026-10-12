//! Primitives of the encryption design (§4): `D(K, C)`, COSE_Encrypt0 with
//! A256GCM, HPKE base mode (X25519, HKDF-SHA-256, AES-256-GCM) and Ed25519
//! device keys. Contexts and AADs are any `Serialize` value, encoded canonically.

use crate::codec::{b64, base64url, canonical, decode_value, unb64};
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm,
};
use cbor2::Value;
use dmsg_types::{ensure, Error, Result};
use ed25519_dalek::{Signer, SigningKey};
use hkdf::Hkdf;
use hpke::{
    aead::AesGcm256,
    kdf::HkdfSha256,
    kem::X25519HkdfSha256,
    rand_core::{CryptoRng, UnwrapErr},
    Deserializable, Kem, OpModeR, OpModeS, Serializable,
};
use serde::{Deserialize, Serialize};
use serde_bytes::Bytes;
use sha2::Sha256;
use std::collections::BTreeMap;
use zeroize::Zeroizing;

/// A 32-byte symmetric key or seed.
pub type Key = [u8; 32];

/// Largest base64url-decoded COSE_Encrypt0 that [`open`] accepts.
pub const MAX_SEALED: usize = 2 * 1024 * 1024;

/// Canonical protected header `{1: 3}`: alg A256GCM.
const PROTECTED: [u8; 3] = [0xa1, 0x01, 0x03];

/// 32 random bytes, for keys and seeds.
///
/// # Panics
/// Panics if the operating system has no random source.
pub fn random_key() -> Key {
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).expect("operating system random source");
    key
}

/// A random 32-byte identifier; never all zero.
pub fn random_id() -> [u8; 32] {
    loop {
        let id = random_key();
        if id.iter().any(|b| *b != 0) {
            return id;
        }
    }
}

/// `D(K, C)`: HKDF-SHA-256 with a 32-byte zero salt and `info = CBOR(C)`.
pub fn derive<C: Serialize>(key: &[u8], context: &C) -> Zeroizing<Key> {
    let mut out = Zeroizing::new([0u8; 32]);
    Hkdf::<Sha256>::new(Some(&[0u8; 32]), key)
        .expand(&canonical(context), out.as_mut())
        .expect("32 bytes is a valid HKDF-SHA-256 output length");
    out
}

/// The local unlock key (LUK) of a workspace from the user home's unlock secret.
pub fn unlock_key(secret: &Key, workspace: &str) -> Zeroizing<Key> {
    derive(secret, &("dmsg/local-unlock/2", workspace))
}

/// Untagged COSE_Encrypt0 under a fresh random nonce, as base64url.
///
/// # Panics
/// Panics if the operating system has no random source.
pub fn seal<A: Serialize>(key: &Key, plaintext: &[u8], aad: &A) -> String {
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut nonce).expect("operating system random source");
    seal_with_nonce(key, plaintext, aad, &nonce)
}

/// [`seal`] under a caller-chosen nonce. Only formats that fix the nonce,
/// such as file chunks, may use it; never reuse a nonce under one key.
pub fn seal_with_nonce<A: Serialize>(
    key: &Key,
    plaintext: &[u8],
    aad: &A,
    nonce: &[u8; 12],
) -> String {
    let ciphertext = Aes256Gcm::new(key.into())
        .encrypt(
            nonce.into(),
            Payload {
                msg: plaintext,
                aad: &enc_structure(aad),
            },
        )
        .expect("AES-GCM encrypts any in-memory plaintext");
    b64(&canonical(&(
        Bytes::new(&PROTECTED),
        BTreeMap::from([(5u8, Bytes::new(nonce))]),
        Bytes::new(&ciphertext),
    )))
}

/// Open a [`seal`] result under the same key and AAD.
///
/// # Errors
/// Malformed encodings return `InvalidInput` or `IntegrityFailed`; another
/// algorithm, nonce size or header set returns `UnsupportedProtocol`; a wrong
/// key, AAD or modified ciphertext returns `IntegrityFailed`.
pub fn open<A: Serialize>(key: &Key, sealed: &str, aad: &A) -> Result<Zeroizing<Vec<u8>>> {
    let Value::Array(items) = decode_value(&unb64(sealed)?, MAX_SEALED)? else {
        return Err(Error::IntegrityFailed);
    };
    let [Value::Bytes(protected), Value::Map(unprotected), Value::Bytes(ciphertext)] =
        items.as_slice()
    else {
        return Err(Error::IntegrityFailed);
    };
    let nonce = match unprotected.as_slice() {
        [(Value::Integer(label), Value::Bytes(nonce))] if i128::from(*label) == 5 => {
            <&[u8; 12]>::try_from(nonce.as_slice()).ok()
        }
        _ => None,
    };
    let nonce = nonce.ok_or(Error::UnsupportedProtocol)?;
    ensure(
        protected.as_slice() == PROTECTED && ciphertext.len() >= 16,
        Error::UnsupportedProtocol,
    )?;
    Aes256Gcm::new(key.into())
        .decrypt(
            nonce.into(),
            Payload {
                msg: ciphertext,
                aad: &enc_structure(aad),
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| Error::IntegrityFailed)
}

fn enc_structure<A: Serialize>(aad: &A) -> Vec<u8> {
    canonical(&(
        "Encrypt0",
        Bytes::new(&PROTECTED),
        Bytes::new(&canonical(aad)),
    ))
}

/// An HPKE base-mode envelope as client records carry it: a map of base64url text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HpkeEnvelope {
    /// The encapsulated ephemeral X25519 public key.
    #[serde(with = "base64url")]
    pub enc: Vec<u8>,
    /// AES-256-GCM ciphertext and tag.
    #[serde(with = "base64url")]
    pub ciphertext: Vec<u8>,
}

/// The X25519 public key of an HPKE seed, by RFC 9180 `DeriveKeyPair(seed)`.
pub fn hpke_public(seed: &Key) -> Key {
    X25519HkdfSha256::derive_keypair(seed).1.to_bytes().into()
}

/// Seal `plaintext` to a recipient X25519 public key; `context` is both the
/// HPKE info and the AAD.
///
/// # Errors
/// An invalid public key returns `IntegrityFailed`.
pub fn hpke_seal<C: Serialize>(
    recipient: &Key,
    plaintext: &[u8],
    context: &C,
) -> Result<HpkeEnvelope> {
    hpke_seal_with(
        recipient,
        plaintext,
        context,
        &mut UnwrapErr(getrandom::SysRng),
    )
}

fn hpke_seal_with<C: Serialize>(
    recipient: &Key,
    plaintext: &[u8],
    context: &C,
    rng: &mut impl CryptoRng,
) -> Result<HpkeEnvelope> {
    let recipient = <X25519HkdfSha256 as Kem>::PublicKey::from_bytes(recipient)
        .map_err(|_| Error::IntegrityFailed)?;
    let info = canonical(context);
    let (enc, ciphertext) = hpke::single_shot_seal_with_rng::<
        AesGcm256,
        HkdfSha256,
        X25519HkdfSha256,
    >(&OpModeS::Base, &recipient, &info, plaintext, &info, rng)
    .map_err(|_| Error::IntegrityFailed)?;
    Ok(HpkeEnvelope {
        enc: enc.to_bytes().to_vec(),
        ciphertext,
    })
}

/// Open an [`hpke_seal`] envelope with the recipient's HPKE seed.
///
/// # Errors
/// A malformed envelope, wrong seed or context returns `IntegrityFailed`.
pub fn hpke_open<C: Serialize>(
    seed: &Key,
    envelope: &HpkeEnvelope,
    context: &C,
) -> Result<Zeroizing<Vec<u8>>> {
    let (secret, _) = X25519HkdfSha256::derive_keypair(seed);
    let enc = <X25519HkdfSha256 as Kem>::EncappedKey::from_bytes(&envelope.enc)
        .map_err(|_| Error::IntegrityFailed)?;
    let info = canonical(context);
    hpke::single_shot_open::<AesGcm256, HkdfSha256, X25519HkdfSha256>(
        &OpModeR::Base,
        &secret,
        &enc,
        &info,
        &envelope.ciphertext,
        &info,
    )
    .map(Zeroizing::new)
    .map_err(|_| Error::IntegrityFailed)
}

/// The Ed25519 public key of a device signing seed.
pub fn signing_public(seed: &Key) -> Key {
    SigningKey::from_bytes(seed).verifying_key().to_bytes()
}

/// Ed25519 signature by a device signing seed; verify with
/// [`dmsg_protocol::verify`], which is strict.
pub fn sign(seed: &Key, message: &[u8]) -> [u8; 64] {
    SigningKey::from_bytes(seed).sign(message).to_bytes()
}

#[cfg(test)]
mod tests;
