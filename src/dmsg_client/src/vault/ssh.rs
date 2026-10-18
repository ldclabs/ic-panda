//! SSH keys of `ssh` items: new Ed25519 keys in the OpenSSH format, and the
//! public key and fingerprint of a private key people paste, read from its
//! unencrypted header (so they come even from a passphrase-protected key).

use super::Item;
use crate::crypto::random_key;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use ed25519_dalek::SigningKey;
use serde::Serialize;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const MAGIC: &[u8] = b"openssh-key-v1\0";
const BEGIN: &str = "-----BEGIN OPENSSH PRIVATE KEY-----";
const END: &str = "-----END OPENSSH PRIVATE KEY-----";

/// A new key, as an `ssh` item holds it.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct SshKey {
    /// The private key in the OpenSSH format, unencrypted.
    pub private_key: String,
    /// The `authorized_keys` line.
    pub public_key: String,
    /// `SHA256:…`, as `ssh-keygen -l` shows it.
    pub fingerprint: String,
}

fn put(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend((bytes.len() as u32).to_be_bytes());
    out.extend(bytes);
}

fn take<'a>(data: &mut &'a [u8]) -> Option<&'a [u8]> {
    let length = u32::from_be_bytes(data.get(..4)?.try_into().ok()?) as usize;
    let bytes = data.get(4..4usize.checked_add(length)?)?;
    *data = &data[4 + length..];
    Some(bytes)
}

fn public_line(blob: &[u8], comment: &str) -> Option<String> {
    let kind = std::str::from_utf8(take(&mut &blob[..])?).ok()?;
    let line = format!("{kind} {}", STANDARD.encode(blob));
    Some(match comment.trim() {
        "" => line,
        comment => format!("{line} {comment}"),
    })
}

fn fingerprint(blob: &[u8]) -> String {
    format!("SHA256:{}", STANDARD_NO_PAD.encode(Sha256::digest(blob)))
}

/// A new Ed25519 key.
pub fn generate(comment: &str) -> SshKey {
    let seed = Zeroizing::new(random_key());
    let public = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
    let mut blob = Vec::new();
    put(&mut blob, b"ssh-ed25519");
    put(&mut blob, &public);
    let check = &random_key()[..4];
    let mut private = Zeroizing::new(Vec::new());
    private.extend(check);
    private.extend(check);
    put(&mut private, b"ssh-ed25519");
    put(&mut private, &public);
    put(
        &mut private,
        &Zeroizing::new([&seed[..], &public[..]].concat()),
    );
    put(&mut private, comment.as_bytes());
    let mut pad = 1u8;
    while private.len() % 8 != 0 {
        private.push(pad);
        pad += 1;
    }
    let mut body = Zeroizing::new(MAGIC.to_vec());
    put(&mut body, b"none");
    put(&mut body, b"none");
    put(&mut body, b"");
    body.extend(1u32.to_be_bytes());
    put(&mut body, &blob);
    put(&mut body, &private);
    let encoded = Zeroizing::new(STANDARD.encode(&*body));
    let mut pem = format!("{BEGIN}\n");
    for line in encoded.as_bytes().chunks(70) {
        pem.push_str(std::str::from_utf8(line).expect("base64 is ASCII"));
        pem.push('\n');
    }
    pem.push_str(END);
    pem.push('\n');
    SshKey {
        private_key: pem,
        public_key: public_line(&blob, comment).expect("an Ed25519 key blob"),
        fingerprint: fingerprint(&blob),
    }
}

/// The public key line and fingerprint of an OpenSSH private key; `None`
/// for anything else.
pub fn inspect(private_key: &str) -> Option<(String, String)> {
    let text = private_key.trim();
    let inner = text.strip_prefix(BEGIN)?.strip_suffix(END)?;
    let encoded: String = inner.chars().filter(|c| !c.is_whitespace()).collect();
    let body = Zeroizing::new(STANDARD.decode(encoded).ok()?);
    let mut data = body.strip_prefix(MAGIC)?;
    let cipher = take(&mut data)?;
    take(&mut data)?;
    take(&mut data)?;
    let count = u32::from_be_bytes(data.get(..4)?.try_into().ok()?);
    data = &data[4..];
    if count != 1 {
        return None;
    }
    let blob = take(&mut data)?;
    // The comment sits in the private part, readable only unencrypted.
    let comment = (cipher == b"none")
        .then(|| {
            let mut private = take(&mut data)?.get(8..)?;
            let kind = take(&mut private)?;
            let parts = match kind {
                b"ssh-ed25519" => 2,
                b"ssh-rsa" => 6,
                kind if kind.starts_with(b"ecdsa-sha2-") => 3,
                _ => return None,
            };
            for _ in 0..parts {
                take(&mut private)?;
            }
            String::from_utf8(take(&mut private)?.to_vec()).ok()
        })
        .flatten()
        .unwrap_or_default();
    Some((public_line(blob, &comment)?, fingerprint(blob)))
}

/// Fill an `ssh` item's empty public key and fingerprint from its private
/// key.
pub(crate) fn complete(item: &mut Item) {
    let Some((public, print)) = inspect(item.value("private_key")) else {
        return;
    };
    for (id, value) in [("public_key", public), ("fingerprint", print)] {
        if let Some(field) = item.fields.iter_mut().find(|f| f.id == id)
            && field.value.is_empty()
        {
            field.value = value;
        }
    }
}

#[cfg(test)]
#[path = "ssh_tests.rs"]
mod tests;
