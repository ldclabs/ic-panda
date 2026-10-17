//! Primitives of the encryption design (§4): `D(K, C)`, COSE_Encrypt0 with
//! A256GCM, HPKE base mode (X25519, HKDF-SHA-256, AES-256-GCM) and Ed25519
//! device keys. Contexts and AADs are any `Serialize` value, encoded canonically.

use crate::codec::{b64, base64url, canonical, unb64};
use aes_gcm::{
    aead::{Aead, AeadInOut, KeyInit, Payload},
    Aes256Gcm,
};
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
    b64(&seal_bytes(key, plaintext, aad))
}

/// [`seal`] as bytes, for formats that carry bytes, such as upload
/// manifests and thumbnails.
///
/// # Panics
/// Panics if the operating system has no random source.
pub fn seal_bytes<A: Serialize>(key: &Key, plaintext: &[u8], aad: &A) -> Vec<u8> {
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut nonce).expect("operating system random source");
    seal_with_nonce(key, plaintext, aad, &nonce)
}

/// [`seal_bytes`] under a caller-chosen nonce. Only formats that fix the
/// nonce, such as file chunks, may use it; never reuse a nonce under one key.
pub fn seal_with_nonce<A: Serialize>(
    key: &Key,
    plaintext: &[u8],
    aad: &A,
    nonce: &[u8; 12],
) -> Vec<u8> {
    // The canonical encoding is fixed but for the ciphertext's length, so
    // it is written directly and the plaintext encrypted in place.
    let length = bstr_header(plaintext.len() + 16);
    let start = HEAD.len() + nonce.len() + length.len();
    let mut out = Vec::with_capacity(start + plaintext.len() + 16);
    out.extend_from_slice(&HEAD);
    out.extend_from_slice(nonce);
    out.extend_from_slice(&length);
    out.extend_from_slice(plaintext);
    let tag = Aes256Gcm::new(key.into())
        .encrypt_inout_detached(
            nonce.into(),
            &enc_structure(aad),
            (&mut out[start..]).into(),
        )
        .expect("AES-GCM encrypts any in-memory plaintext");
    out.extend_from_slice(&tag);
    out
}

/// Open a [`seal`] result under the same key and AAD.
///
/// # Errors
/// As [`open_bytes`], and `InvalidInput` for text that is not base64url.
pub fn open<A: Serialize>(key: &Key, sealed: &str, aad: &A) -> Result<Zeroizing<Vec<u8>>> {
    open_bytes(key, &unb64(sealed)?, aad)
}

/// Open a [`seal_bytes`] result under the same key and AAD.
///
/// # Errors
/// Another algorithm, nonce size or header set returns
/// `UnsupportedProtocol`; anything malformed, a wrong key or AAD or a
/// modified ciphertext returns `IntegrityFailed`.
pub fn open_bytes<A: Serialize>(key: &Key, sealed: &[u8], aad: &A) -> Result<Zeroizing<Vec<u8>>> {
    ensure(
        sealed.len() <= MAX_SEALED && sealed.first() == Some(&HEAD[0]),
        Error::IntegrityFailed,
    )?;
    let rest = sealed
        .strip_prefix(&HEAD)
        .ok_or(Error::UnsupportedProtocol)?;
    let (nonce, rest) = rest
        .split_first_chunk::<12>()
        .ok_or(Error::IntegrityFailed)?;
    let ciphertext = whole_bstr(rest).ok_or(Error::IntegrityFailed)?;
    ensure(ciphertext.len() >= 16, Error::IntegrityFailed)?;
    let (body, tag) = ciphertext.split_at(ciphertext.len() - 16);
    let tag = aes_gcm::Tag::try_from(tag).map_err(|_| Error::IntegrityFailed)?;
    let mut plain = Zeroizing::new(body.to_vec());
    Aes256Gcm::new(key.into())
        .decrypt_inout_detached(
            nonce.into(),
            &enc_structure(aad),
            plain.as_mut_slice().into(),
            &tag,
        )
        .map_err(|_| Error::IntegrityFailed)?;
    Ok(plain)
}

/// The canonical start of every COSE_Encrypt0 this module writes: an array
/// of three, the protected header `{1: 3}` and `{5: <12-byte nonce>}`.
const HEAD: [u8; 8] = [0x83, 0x43, 0xa1, 0x01, 0x03, 0xa1, 0x05, 0x4c];

/// The content of a canonical CBOR byte string that is all of `bytes`.
fn whole_bstr(bytes: &[u8]) -> Option<&[u8]> {
    let (&first, rest) = bytes.split_first()?;
    let (len, rest) = match first {
        0x40..=0x57 => (usize::from(first & 0x1f), rest),
        0x58 => {
            let (&n, rest) = rest.split_first()?;
            (usize::from(n), rest)
        }
        0x59 => {
            let (n, rest) = rest.split_first_chunk::<2>()?;
            (usize::from(u16::from_be_bytes(*n)), rest)
        }
        0x5a => {
            let (n, rest) = rest.split_first_chunk::<4>()?;
            (u32::from_be_bytes(*n) as usize, rest)
        }
        _ => return None,
    };
    // Only the shortest head is canonical, and nothing may follow.
    let head = &bytes[..bytes.len() - rest.len()];
    (bstr_header(len) == head && rest.len() == len).then_some(rest)
}

/// The canonical CBOR head of a byte string of `len` bytes.
fn bstr_header(len: usize) -> Vec<u8> {
    match len {
        0..=23 => vec![0x40 | len as u8],
        24..=0xff => vec![0x58, len as u8],
        0x100..=0xffff => [0x59]
            .into_iter()
            .chain((len as u16).to_be_bytes())
            .collect(),
        _ => [0x5a]
            .into_iter()
            .chain((len as u32).to_be_bytes())
            .collect(),
    }
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
    hpke_seal_info(recipient, plaintext, &canonical(context))
}

/// [`hpke_seal`] with the encoded info, which is also the AAD.
///
/// # Errors
/// An invalid public key returns `IntegrityFailed`.
pub fn hpke_seal_info(recipient: &Key, plaintext: &[u8], info: &[u8]) -> Result<HpkeEnvelope> {
    hpke_seal_with(
        &OpModeS::Base,
        recipient,
        plaintext,
        info,
        &mut UnwrapErr(getrandom::SysRng),
    )
}

/// HPKE auth mode: seal to `recipient` and authenticate with the sender's
/// HPKE seed; `info` is also the AAD.
///
/// # Errors
/// An invalid public key returns `IntegrityFailed`.
pub fn hpke_seal_auth(
    sender: &Key,
    recipient: &Key,
    plaintext: &[u8],
    info: &[u8],
) -> Result<HpkeEnvelope> {
    hpke_seal_auth_with(
        sender,
        recipient,
        plaintext,
        info,
        &mut UnwrapErr(getrandom::SysRng),
    )
}

pub(crate) fn hpke_seal_auth_with(
    sender: &Key,
    recipient: &Key,
    plaintext: &[u8],
    info: &[u8],
    rng: &mut impl CryptoRng,
) -> Result<HpkeEnvelope> {
    let (secret, public) = X25519HkdfSha256::derive_keypair(sender);
    hpke_seal_with(
        &OpModeS::Auth((secret, public)),
        recipient,
        plaintext,
        info,
        rng,
    )
}

pub(crate) fn hpke_seal_with(
    mode: &OpModeS<X25519HkdfSha256>,
    recipient: &Key,
    plaintext: &[u8],
    info: &[u8],
    rng: &mut impl CryptoRng,
) -> Result<HpkeEnvelope> {
    let recipient = <X25519HkdfSha256 as Kem>::PublicKey::from_bytes(recipient)
        .map_err(|_| Error::IntegrityFailed)?;
    let (enc, ciphertext) =
        hpke::single_shot_seal_with_rng::<AesGcm256, HkdfSha256, X25519HkdfSha256>(
            mode, &recipient, info, plaintext, info, rng,
        )
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
    hpke_open_info(seed, envelope, &canonical(context))
}

/// [`hpke_open`] with the encoded info.
///
/// # Errors
/// A malformed envelope, wrong seed or info returns `IntegrityFailed`.
pub fn hpke_open_info(
    seed: &Key,
    envelope: &HpkeEnvelope,
    info: &[u8],
) -> Result<Zeroizing<Vec<u8>>> {
    hpke_open_with(&OpModeR::Base, seed, envelope, info)
}

/// Open an [`hpke_seal_auth`] envelope, checking it came from `sender`, as
/// the bridge page opens replies; for tests and vectors.
///
/// # Errors
/// A malformed envelope, another sender, a wrong seed or info returns
/// `IntegrityFailed`.
#[cfg(test)]
pub fn hpke_open_auth(
    seed: &Key,
    sender: &Key,
    envelope: &HpkeEnvelope,
    info: &[u8],
) -> Result<Zeroizing<Vec<u8>>> {
    let sender = <X25519HkdfSha256 as Kem>::PublicKey::from_bytes(sender)
        .map_err(|_| Error::IntegrityFailed)?;
    hpke_open_with(&OpModeR::Auth(sender), seed, envelope, info)
}

fn hpke_open_with(
    mode: &OpModeR<X25519HkdfSha256>,
    seed: &Key,
    envelope: &HpkeEnvelope,
    info: &[u8],
) -> Result<Zeroizing<Vec<u8>>> {
    let (secret, _) = X25519HkdfSha256::derive_keypair(seed);
    let enc = <X25519HkdfSha256 as Kem>::EncappedKey::from_bytes(&envelope.enc)
        .map_err(|_| Error::IntegrityFailed)?;
    hpke::single_shot_open::<AesGcm256, HkdfSha256, X25519HkdfSha256>(
        mode,
        &secret,
        &enc,
        info,
        &envelope.ciphertext,
        info,
    )
    .map(Zeroizing::new)
    .map_err(|_| Error::IntegrityFailed)
}

/// AES-256-GCM of `plaintext` under a raw key and nonce, with `aad`, as the
/// bridge page seals requests; for tests and vectors.
#[cfg(test)]
pub fn aes_gcm_seal(key: &Key, nonce: &[u8; 12], plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
    Aes256Gcm::new(key.into())
        .encrypt(
            nonce.into(),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .expect("AES-GCM encrypts any in-memory plaintext")
}

/// Open AES-256-GCM `ciphertext` under a raw key and nonce, with `aad`.
///
/// # Errors
/// A wrong key, nonce or AAD, or modified ciphertext, returns `IntegrityFailed`.
pub fn aes_gcm_open(
    key: &Key,
    nonce: &[u8; 12],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Zeroizing<Vec<u8>>> {
    Aes256Gcm::new(key.into())
        .decrypt(
            nonce.into(),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| Error::IntegrityFailed)
}

/// Verify an ECDSA P-256 / SHA-256 signature in IEEE P1363 form (64 bytes)
/// with a P-256 SPKI DER key, as WebCrypto produces both.
///
/// # Errors
/// A malformed key or signature, or a wrong signature, returns `IntegrityFailed`.
pub fn p256_verify(spki: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
    use p256::ecdsa::signature::Verifier;
    dmsg_protocol::connect::validate_p256_spki(spki).map_err(|_| Error::IntegrityFailed)?;
    let key = p256::ecdsa::VerifyingKey::from_sec1_bytes(&spki[26..])
        .map_err(|_| Error::IntegrityFailed)?;
    let signature =
        p256::ecdsa::Signature::from_slice(signature).map_err(|_| Error::IntegrityFailed)?;
    key.verify(message, &signature)
        .map_err(|_| Error::IntegrityFailed)
}

/// The P-256 SPKI DER of a 32-byte private scalar; for tests and vectors.
///
/// # Errors
/// An invalid scalar returns `IntegrityFailed`.
#[cfg(test)]
pub fn p256_public(secret: &Key) -> Result<Vec<u8>> {
    let key = p256::ecdsa::SigningKey::from_slice(secret).map_err(|_| Error::IntegrityFailed)?;
    let point = key.verifying_key().to_sec1_point(false);
    let mut der = dmsg_protocol::connect::P256_SPKI_PREFIX.to_vec();
    der.extend_from_slice(point.as_bytes());
    Ok(der)
}

/// Deterministic (RFC 6979) ECDSA P-256 / SHA-256 signature in IEEE P1363
/// form; the page and bridge sides use it in tests and vectors.
///
/// # Errors
/// An invalid scalar returns `IntegrityFailed`.
#[cfg(test)]
pub fn p256_sign(secret: &Key, message: &[u8]) -> Result<[u8; 64]> {
    use p256::ecdsa::signature::Signer;
    let key = p256::ecdsa::SigningKey::from_slice(secret).map_err(|_| Error::IntegrityFailed)?;
    let signature: p256::ecdsa::Signature = key.sign(message);
    Ok(signature.to_bytes().into())
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
pub(crate) mod tests;
