//! Windows Credential Manager limits each blob to 2,560 bytes. Publish the
//! head after all chunks, keeping both generations discoverable until cleanup
//! finishes. A retry can settle an interrupted write, replacement or deletion.

use dmsg_client::secure::SecureStorage;
use dmsg_types::{Error, Result, ensure};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const V1: &[u8; 16] = b"dmsg/parts/v1\0\0\0";
const V2: &[u8; 16] = b"dmsg/parts/v2\0\0\0";
const INDEX_LEN: usize = 16 + 4 + 32;
// At 2,560 bytes per part this permits 10 MiB, well above the client's records.
// Bound iteration even when a stored part count is corrupt.
const MAX_PARTS: u32 = 4096;

#[cfg(windows)]
pub struct SystemStorage;

#[cfg(windows)]
impl SystemStorage {
    fn access<T>(&self, f: impl FnOnce(Parted<'_>) -> Result<T>) -> Result<T> {
        static ACCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ACCESS.lock().unwrap_or_else(|e| e.into_inner());
        f(Parted(&super::Keyring, 2560))
    }
}

#[cfg(windows)]
impl SecureStorage for SystemStorage {
    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
        self.access(|store| store.get(name))
    }

    fn set(&self, name: &str, secret: &[u8]) -> Result<()> {
        self.access(|store| store.set(name, secret))
    }

    fn delete(&self, name: &str) -> Result<()> {
        self.access(|store| store.delete(name))
    }
}

struct Parted<'a>(&'a dyn SecureStorage, usize);

#[derive(PartialEq, Eq)]
struct Index {
    legacy: bool,
    count: u32,
    digest: [u8; 32],
}

impl Index {
    fn parse(entry: &[u8]) -> Result<Option<Self>> {
        let legacy = entry.starts_with(V1);
        if !legacy && !entry.starts_with(V2) {
            return Ok(None);
        }
        ensure(entry.len() == INDEX_LEN, Error::IntegrityFailed)?;
        let count = u32::from_le_bytes(entry[16..20].try_into().unwrap());
        ensure((1..=MAX_PARTS).contains(&count), Error::IntegrityFailed)?;
        Ok(Some(Self {
            legacy,
            count,
            digest: entry[20..].try_into().unwrap(),
        }))
    }

    fn bytes(&self) -> Vec<u8> {
        let version = if self.legacy { V1 } else { V2 };
        [version.as_slice(), &self.count.to_le_bytes(), &self.digest].concat()
    }

    fn names(&self, name: &str) -> impl Iterator<Item = String> {
        let digest = if self.legacy {
            &self.digest[..4]
        } else {
            &self.digest[..]
        };
        let prefix = format!("{name}#{}", hex::encode(digest));
        (1..=self.count).map(move |i| format!("{prefix}.{i}"))
    }
}

impl Parted<'_> {
    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
        let Some(head) = self.0.get(name)? else {
            return Ok(None);
        };
        let Some(index) = Index::parse(&head)? else {
            return Ok(Some(head));
        };
        let mut whole = Zeroizing::new(Vec::with_capacity(index.count as usize * self.1));
        for part in index.names(name) {
            let bytes = self.0.get(&part)?.ok_or(Error::IntegrityFailed)?;
            whole.extend_from_slice(&bytes);
        }
        ensure(
            Sha256::digest(&*whole)[..] == index.digest,
            Error::IntegrityFailed,
        )?;
        Ok(Some(whole))
    }

    // The head is the commit point. Only discard parts no longer referenced
    // there, including a partially staged new value after a failed write.
    fn clean(&self, name: &str, pending: &str, active: Option<&Index>) -> Result<()> {
        if let Some(bytes) = self.0.get(pending)? {
            ensure(
                bytes.len() == INDEX_LEN || bytes.len() == 2 * INDEX_LEN,
                Error::IntegrityFailed,
            )?;
            for entry in bytes.chunks_exact(INDEX_LEN) {
                let index = Index::parse(entry)?.ok_or(Error::IntegrityFailed)?;
                if Some(&index) != active {
                    for part in index.names(name) {
                        self.0.delete(&part)?;
                    }
                }
            }
            self.0.delete(pending)?;
        }
        Ok(())
    }

    fn replace(&self, name: &str, value: Option<&[u8]>) -> Result<()> {
        let head = self.0.get(name)?;
        let old = Index::parse(head.as_deref().map(Vec::as_slice).unwrap_or_default())?;
        let pending = format!(
            "dmsg-parts:{}",
            hex::encode(Sha256::digest(name.as_bytes()))
        );
        self.clean(name, &pending, old.as_ref())?;
        let new = match value {
            Some(bytes)
                if bytes.len() > self.1 || bytes.starts_with(V1) || bytes.starts_with(V2) =>
            {
                let count = bytes.len().div_ceil(self.1);
                ensure(
                    count <= MAX_PARTS as usize,
                    Error::InvalidInput("credential size".into()),
                )?;
                Some(Index {
                    legacy: false,
                    count: count as u32,
                    digest: Sha256::digest(bytes).into(),
                })
            }
            _ => None,
        };
        let journal: Vec<u8> = old.iter().chain(&new).flat_map(Index::bytes).collect();
        if !journal.is_empty() {
            self.0.set(&pending, &journal)?;
        }
        let result = match (value, &new) {
            (Some(bytes), Some(index)) => index
                .names(name)
                .zip(bytes.chunks(self.1))
                .try_for_each(|(part, chunk)| self.0.set(&part, chunk))
                .and_then(|()| self.0.set(name, &index.bytes())),
            (Some(bytes), None) => self.0.set(name, bytes),
            (None, _) => self.0.delete(name),
        };
        if !journal.is_empty() {
            let active = if result.is_ok() {
                new.as_ref()
            } else {
                old.as_ref()
            };
            self.clean(name, &pending, active)?;
        }
        result
    }

    fn set(&self, name: &str, secret: &[u8]) -> Result<()> {
        self.replace(name, Some(secret))
    }

    fn delete(&self, name: &str) -> Result<()> {
        self.replace(name, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dmsg_client::testing::MemoryStorage;

    fn names(store: &Parted<'_>) -> Vec<String> {
        let head = store.0.get("a").unwrap().unwrap();
        Index::parse(&head).unwrap().unwrap().names("a").collect()
    }

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
        let first = names(&parted);
        assert_eq!(first.len(), 6);

        // A new long secret writes its own parts, then drops the old ones.
        let other = [9u8; 50];
        parted.set("a", &other).unwrap();
        assert_eq!(parted.get("a").unwrap().unwrap().as_slice(), other);
        assert!(first.iter().all(|part| store.get(part).unwrap().is_none()));

        // Back to a short one, and gone without a trace.
        let second = names(&parted);
        parted.set("a", b"tiny").unwrap();
        assert!(second.iter().all(|part| store.get(part).unwrap().is_none()));
        parted.set("a", &long).unwrap();
        let parts = names(&parted);
        parted.delete("a").unwrap();
        assert!(parted.get("a").unwrap().is_none());
        assert!(parts.iter().all(|part| store.get(part).unwrap().is_none()));
    }

    #[test]
    fn a_short_secret_that_looks_like_an_index_is_kept_in_parts() {
        let store = MemoryStorage::default();
        let parted = Parted(&store, 64);
        let tricky = [&V1[..], &[1, 0, 0, 0], &[0; 32]].concat();
        parted.set("a", &tricky).unwrap();
        assert_eq!(parted.get("a").unwrap().unwrap().as_slice(), tricky);
    }

    #[test]
    fn a_missing_or_changed_part_is_an_integrity_failure() {
        let store = MemoryStorage::default();
        let parted = Parted(&store, 4);
        parted.set("a", b"0123456789").unwrap();
        let parts = names(&parted);
        store.set(&parts[1], b"XXXX").unwrap();
        assert_eq!(parted.get("a").err(), Some(Error::IntegrityFailed));
        store.delete(&parts[1]).unwrap();
        assert_eq!(parted.get("a").err(), Some(Error::IntegrityFailed));
    }

    #[test]
    fn failed_replacement_and_interrupted_logout_keep_cleanup_retryable() {
        use std::sync::Mutex;
        #[derive(Default)]
        struct Failing {
            store: MemoryStorage,
            fail: Mutex<String>,
        }
        impl SecureStorage for Failing {
            fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
                self.store.get(name)
            }

            fn set(&self, name: &str, value: &[u8]) -> Result<()> {
                ensure(
                    *self.fail.lock().unwrap() != name,
                    Error::Unavailable("write".into()),
                )?;
                self.store.set(name, value)
            }

            fn delete(&self, name: &str) -> Result<()> {
                ensure(
                    *self.fail.lock().unwrap() != name,
                    Error::Unavailable("delete".into()),
                )?;
                self.store.delete(name)
            }
        }
        let store = Failing::default();
        let parted = Parted(&store, 8);
        // A v1 credential written by the original Windows release.
        let old = b"previous value";
        let index = Index {
            legacy: true,
            count: 2,
            digest: Sha256::digest(old).into(),
        };
        let old_parts: Vec<_> = index.names("a").collect();
        for (name, bytes) in old_parts.iter().zip(old.chunks(8)) {
            store.set(name, bytes).unwrap();
        }
        store.set("a", &index.bytes()).unwrap();
        let new = b"renewed credential";
        *store.fail.lock().unwrap() = "a".into();
        assert!(parted.set("a", new).is_err());
        assert_eq!(parted.get("a").unwrap().unwrap().as_slice(), old);
        let new_index = Index {
            legacy: false,
            count: 3,
            digest: Sha256::digest(new).into(),
        };
        assert!(
            new_index
                .names("a")
                .all(|p| store.get(&p).unwrap().is_none())
        );

        // The new head is committed, but deleting the old chunks fails.
        *store.fail.lock().unwrap() = old_parts[1].clone();
        assert!(parted.set("a", new).is_err());
        assert_eq!(parted.get("a").unwrap().unwrap().as_slice(), new);
        let new_parts = names(&parted);
        *store.fail.lock().unwrap() = new_parts[1].clone();
        assert!(parted.delete("a").is_err());
        assert!(old_parts.iter().all(|p| store.get(p).unwrap().is_none()));
        assert!(parted.get("a").unwrap().is_none());
        store.fail.lock().unwrap().clear();
        parted.delete("a").unwrap();
        assert!(new_parts.iter().all(|p| store.get(p).unwrap().is_none()));
    }

    #[test]
    fn rejects_invalid_counts_before_accessing_parts() {
        let store = MemoryStorage::default();
        let parted = Parted(&store, 2560);
        for count in [0u32, u32::MAX] {
            let head = [V1.as_slice(), &count.to_le_bytes(), &[0; 32]].concat();
            store.set("a", &head).unwrap();
            assert_eq!(parted.get("a").err(), Some(Error::IntegrityFailed));
            assert_eq!(parted.delete("a").err(), Some(Error::IntegrityFailed));
        }
    }
}
