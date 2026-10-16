//! Team-wallet cosigner keys in the Secure Enclave (macOS). The private key
//! never leaves the enclave and each signature asks for Touch ID or the
//! login password. Creating one needs a code-signed build with a keychain
//! entitlement; a plain `cargo build` falls back to a software key, which
//! the wallet sees as weak.

use dmsg_client::wallet::cosign::CosignProvider;
use dmsg_types::{Error, Result};
use security_framework::access_control::{ProtectionMode, SecAccessControl};
use security_framework::item::{
    ItemClass, ItemSearchOptions, KeyClass, Location, Reference, SearchResult,
};
use security_framework::key::{Algorithm, GenerateKeyOptions, KeyType, SecKey, Token};
use security_framework_sys::access_control::{
    kSecAccessControlPrivateKeyUsage, kSecAccessControlUserPresence,
};

/// The Secure Enclave of this Mac.
pub struct SecureEnclave;

fn unavailable(e: impl std::fmt::Display) -> Error {
    Error::Unavailable(format!("secure enclave: {e}"))
}

fn find(handle: &str) -> Result<SecKey> {
    let found = ItemSearchOptions::new()
        .class(ItemClass::key())
        .key_class(KeyClass::private())
        .label(handle)
        .ignore_legacy_keychains()
        .load_refs(true)
        .search()
        .map_err(unavailable)?;
    found
        .into_iter()
        .find_map(|r| match r {
            SearchResult::Ref(Reference::Key(key)) => Some(key),
            _ => None,
        })
        .ok_or(Error::NotFound)
}

/// `r || s` of a DER ECDSA signature.
fn raw_signature(der: &[u8]) -> Result<Vec<u8>> {
    let bad = || Error::IntegrityFailed;
    let mut at = 2;
    if der.len() < 8 || der[0] != 0x30 || usize::from(der[1]) != der.len() - 2 {
        return Err(bad());
    }
    let mut out = vec![];
    for _ in 0..2 {
        if der.get(at) != Some(&0x02) {
            return Err(bad());
        }
        let len = usize::from(*der.get(at + 1).ok_or_else(bad)?);
        let int = der.get(at + 2..at + 2 + len).ok_or_else(bad)?;
        let int: Vec<u8> = int.iter().copied().skip_while(|b| *b == 0).collect();
        if int.len() > 32 {
            return Err(bad());
        }
        out.extend(std::iter::repeat_n(0, 32 - int.len()));
        out.extend(int);
        at += 2 + len;
    }
    Ok(out)
}

impl CosignProvider for SecureEnclave {
    fn name(&self) -> &'static str {
        "secure-enclave"
    }

    fn create(&self, label: &str) -> Result<(Vec<u8>, String)> {
        let handle = format!(
            "dmsg-wallet-cosigner:{label}:{}",
            hex::encode(&dmsg_client::crypto::random_id()[..8])
        );
        let access = SecAccessControl::create_with_protection(
            Some(ProtectionMode::AccessibleWhenUnlockedThisDeviceOnly),
            kSecAccessControlPrivateKeyUsage | kSecAccessControlUserPresence,
        )
        .map_err(unavailable)?;
        let mut options = GenerateKeyOptions::default();
        options
            .set_key_type(KeyType::ec_sec_prime_random())
            .set_size_in_bits(256)
            .set_token(Token::SecureEnclave)
            .set_location(Location::DataProtectionKeychain)
            .set_access_control(access)
            .set_label(handle.clone());
        let key = SecKey::new(&options).map_err(unavailable)?;
        let public = key
            .public_key()
            .and_then(|k| k.external_representation())
            .ok_or_else(|| unavailable("no public key"))?;
        let point = public.bytes();
        if point.len() != 65 || point[0] != 0x04 {
            return Err(Error::IntegrityFailed);
        }
        let compressed = [&[0x02 | (point[64] & 1)][..], &point[1..33]].concat();
        Ok((compressed, handle))
    }

    fn sign(&self, handle: &str, message: &[u8]) -> Result<Vec<u8>> {
        let key = find(handle)?;
        let der = key
            .create_signature(Algorithm::ECDSASignatureMessageX962SHA256, message)
            .map_err(unavailable)?;
        raw_signature(&der)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn der_signatures_become_fixed_width() {
        let der = [0x30, 0x08, 0x02, 0x02, 0x00, 0x81, 0x02, 0x02, 0x01, 0x02];
        let raw = super::raw_signature(&der).unwrap();
        assert_eq!(raw.len(), 64);
        assert_eq!(raw[31], 0x81);
        assert_eq!(&raw[62..], &[0x01, 0x02]);
        assert!(super::raw_signature(&der[..9]).is_err());
    }
}
