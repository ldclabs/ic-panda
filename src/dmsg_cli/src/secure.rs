//! The system credential store: the macOS Keychain, the Windows Credential
//! Manager or the Secret Service on Linux, through `keyring`. The Windows
//! Credential Manager keeps at most 2,560 bytes in one credential, less
//! than a Passkey login's canister-signed delegation, so a longer secret is
//! kept in parts behind a short index under its name.

use dmsg_client::secure::SecureStorage;
use dmsg_types::{Error, Result, ensure};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const SERVICE: &str = "dmsg";
/// The most one credential keeps (`CRED_MAX_CREDENTIAL_BLOB_SIZE` on Windows).
const PART: usize = if cfg!(windows) { 2560 } else { usize::MAX };
/// What starts an index: then the number of parts (u32, little-endian) and
/// the SHA-256 of the whole secret, whose first bytes name the parts.
const INDEX: &[u8; 16] = b"dmsg/parts/v1\0\0\0";

/// The system credential store under the service `dmsg`.
pub struct SystemStorage;

/// The credential store itself, one entry per name.
struct Keyring;

fn unavailable(error: keyring::Error) -> Error {
    Error::Unavailable(format!("credential store: {error}"))
}

impl SecureStorage for Keyring {
    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
        let entry = keyring::Entry::new(SERVICE, name).map_err(unavailable)?;
        match entry.get_secret() {
            Ok(secret) => Ok(Some(Zeroizing::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(unavailable(error)),
        }
    }

    fn set(&self, name: &str, secret: &[u8]) -> Result<()> {
        keyring::Entry::new(SERVICE, name)
            .and_then(|entry| entry.set_secret(secret))
            .map_err(unavailable)
    }

    fn delete(&self, name: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE, name).map_err(unavailable)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(unavailable(error)),
        }
    }
}

impl SecureStorage for SystemStorage {
    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
        Parted(&Keyring, PART).get(name)
    }

    fn set(&self, name: &str, secret: &[u8]) -> Result<()> {
        Parted(&Keyring, PART).set(name, secret)
    }

    fn delete(&self, name: &str) -> Result<()> {
        Parted(&Keyring, PART).delete(name)
    }
}

/// A store whose entries keep at most so many bytes. A longer secret's
/// parts are named after its digest, so a new secret never overwrites the
/// parts the old index still names; the index switches over last.
struct Parted<'a>(&'a dyn SecureStorage, usize);

/// The number of parts and the digest an index names.
fn index(entry: &[u8]) -> Option<(u32, [u8; 32])> {
    let rest = entry.strip_prefix(INDEX)?;
    let (count, digest) = rest.split_first_chunk::<4>()?;
    Some((u32::from_le_bytes(*count), digest.try_into().ok()?))
}

fn part_name(name: &str, digest: &[u8; 32], part: u32) -> String {
    format!("{name}#{}.{part}", hex::encode(&digest[..4]))
}

impl Parted<'_> {
    /// The parts an existing index names.
    fn parts(&self, name: &str) -> Result<Vec<String>> {
        let head = self.0.get(name)?;
        Ok(match head.as_deref().and_then(|head| index(head)) {
            Some((count, digest)) => (1..=count)
                .map(|part| part_name(name, &digest, part))
                .collect(),
            None => Vec::new(),
        })
    }

    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
        let Some(head) = self.0.get(name)? else {
            return Ok(None);
        };
        let Some((count, digest)) = index(&head) else {
            return Ok(Some(head));
        };
        let mut whole = Zeroizing::new(Vec::new());
        for part in 1..=count {
            let part = self
                .0
                .get(&part_name(name, &digest, part))?
                .ok_or(Error::IntegrityFailed)?;
            whole.extend_from_slice(&part);
        }
        ensure(
            Sha256::digest(whole.as_slice())[..] == digest,
            Error::IntegrityFailed,
        )?;
        Ok(Some(whole))
    }

    fn set(&self, name: &str, secret: &[u8]) -> Result<()> {
        let old = self.parts(name)?;
        let mut new = Vec::new();
        if secret.len() <= self.1 && !secret.starts_with(INDEX) {
            self.0.set(name, secret)?;
        } else {
            let digest: [u8; 32] = Sha256::digest(secret).into();
            for (part, chunk) in (1..).zip(secret.chunks(self.1)) {
                let part_name = part_name(name, &digest, part);
                self.0.set(&part_name, chunk)?;
                new.push(part_name);
            }
            let count =
                u32::try_from(new.len()).map_err(|_| Error::InvalidInput("secret".into()))?;
            let head = [&INDEX[..], &count.to_le_bytes(), &digest].concat();
            self.0.set(name, &head)?;
        }
        for part in old.iter().filter(|part| !new.contains(part)) {
            self.0.delete(part)?;
        }
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<()> {
        let old = self.parts(name)?;
        self.0.delete(name)?;
        for part in &old {
            self.0.delete(part)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dmsg_client::testing::MemoryStorage;

    #[test]
    fn long_secrets_keep_in_parts_behind_an_index() {
        let store = MemoryStorage::default();
        let parted = Parted(&store, 8);
        // Short secrets stay as they were, readable without the index.
        parted.set("a", b"short").unwrap();
        assert_eq!(store.get("a").unwrap().unwrap().as_slice(), b"short");
        assert_eq!(parted.get("a").unwrap().unwrap().as_slice(), b"short");

        let long = [7u8; 48];
        parted.set("a", &long).unwrap();
        assert_eq!(parted.get("a").unwrap().unwrap().as_slice(), long);
        let first = parted.parts("a").unwrap();
        assert_eq!(first.len(), 6);

        // A new long secret writes its own parts, then drops the old ones.
        let other = [9u8; 50];
        parted.set("a", &other).unwrap();
        assert_eq!(parted.get("a").unwrap().unwrap().as_slice(), other);
        assert!(first.iter().all(|part| store.get(part).unwrap().is_none()));

        // Back to a short one, and gone without a trace.
        let second = parted.parts("a").unwrap();
        parted.set("a", b"tiny").unwrap();
        assert!(second.iter().all(|part| store.get(part).unwrap().is_none()));
        parted.set("a", &long).unwrap();
        let parts = parted.parts("a").unwrap();
        parted.delete("a").unwrap();
        assert!(parted.get("a").unwrap().is_none());
        assert!(parts.iter().all(|part| store.get(part).unwrap().is_none()));
    }

    #[test]
    fn a_short_secret_that_looks_like_an_index_is_kept_in_parts() {
        let store = MemoryStorage::default();
        let parted = Parted(&store, 64);
        let tricky = [&INDEX[..], &[1, 0, 0, 0], &[0; 32]].concat();
        parted.set("a", &tricky).unwrap();
        assert_eq!(parted.get("a").unwrap().unwrap().as_slice(), tricky);
    }

    #[test]
    fn a_missing_or_changed_part_is_an_integrity_failure() {
        let store = MemoryStorage::default();
        let parted = Parted(&store, 4);
        parted.set("a", b"0123456789").unwrap();
        let parts = parted.parts("a").unwrap();
        store.set(&parts[1], b"XXXX").unwrap();
        assert_eq!(parted.get("a").err(), Some(Error::IntegrityFailed));
        store.delete(&parts[1]).unwrap();
        assert_eq!(parted.get("a").err(), Some(Error::IntegrityFailed));
    }
}
