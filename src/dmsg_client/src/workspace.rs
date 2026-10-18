//! One device's workspace: its keys, its local data key (LDK) and the unlock
//! chain of the account root contract (`docs/protocol/account_root_zh.md`,
//! "本机解锁").
//!
//! A new workspace generates the device's signing and HPKE seeds and a random
//! LDK, all sealed under the LDK, and wraps the LDK under a provisional key
//! kept in plain text: the device can sign its own account or pairing request,
//! nothing more. Once the device belongs to an account the LDK is rewrapped
//! under `LUK = HKDF(unlock_secret, ["dmsg/local-unlock/2", name])` with
//! `name = "dmsg:" + environment + ":" + account + ":" + device`, and the
//! provisional key is deleted. Every later unlock needs the user home's
//! login-gated `unlock_secret`.
//!
//! Fast unlock seals a second copy of the LDK under a key the operating
//! system keeps, or that hardware recovers only with the person present
//! (encryption design §5.2; plan D-6); it is refused thirty days after the
//! last unlock through a login.

use crate::codec::{MAX_RECORD, canonical, decode};
use crate::config::environment_name;
use crate::crypto::{Key, hpke_public, open, random_key, seal, signing_public};
use crate::store::Store;
use candid::Principal;
use dmsg_types::{
    AccountId, Environment, Error, Hash, Result, ensure,
    user::{Capability, ControllerRole, DeviceInput},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_bytes::ByteArray;
use std::path::Path;
use zeroize::Zeroizing;

pub(crate) const META: &str = "workspace";
const PROVISIONAL: &str = "provisional";
const LDK: &str = "ldk";
const FAST_LDK: &str = "ldk-fast";
const DEVICE: &str = "device";
/// Largest value [`Workspace::private`] decodes.
const MAX_PRIVATE: usize = 2 * 1024 * 1024;

/// Plaintext facts about the workspace; nothing here is secret.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    /// Lowercase environment name.
    pub environment: String,
    /// This device's ID.
    pub device_id: Hash,
    /// This device's Ed25519 public key.
    pub signing_pub: Hash,
    /// This device's X25519 public key.
    pub hpke_pub: Hash,
    /// Creation time in Unix milliseconds.
    pub created_at: u64,
    /// The account once the device belongs to it.
    pub account: Option<AccountRef>,
    /// The latest unlock through a login, in Unix milliseconds.
    pub login_unlocked_at: Option<u64>,
    /// When fast unlock was enabled, if it is.
    pub fast_unlock: Option<u64>,
    /// Fast unlock asks for the person: hardware keeps its key.
    #[serde(default)]
    pub fast_presence: bool,
    /// The person turned fast unlock off; a login unlock leaves it off.
    #[serde(default)]
    pub fast_declined: bool,
}

/// The account a workspace belongs to.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AccountRef {
    /// Account ID.
    pub id: AccountId,
    /// The account's user home.
    pub home: Principal,
    /// The account's issuer URI.
    pub issuer: String,
}

/// Unlocked key material; dropping it zeroes every key.
pub struct Keys {
    ldk: Zeroizing<Key>,
    /// Device Ed25519 seed.
    pub signing: Zeroizing<Key>,
    /// Device HPKE seed.
    pub hpke: Zeroizing<Key>,
}

#[derive(Serialize, Deserialize)]
struct DeviceSecrets {
    signing: ByteArray<32>,
    hpke: ByteArray<32>,
}

/// A workspace database and its metadata.
pub struct Workspace {
    /// The database.
    pub store: Store,
    /// Plaintext metadata.
    pub meta: Meta,
}

impl Workspace {
    /// Open the workspace at `path`, creating a new device if it is empty.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` on storage failures and
    /// `Error::IntegrityFailed` when the file belongs to another environment.
    pub fn open(path: &Path, environment: &Environment, now: u64) -> Result<Self> {
        Self::load(Store::open(path)?, environment, now)
    }

    /// [`Self::open`] on an existing store.
    ///
    /// # Errors
    /// As [`Self::open`].
    pub fn load(store: Store, environment: &Environment, now: u64) -> Result<Self> {
        let name = environment_name(environment);
        if let Some(meta) = store.meta::<Meta>(META)? {
            ensure(meta.environment == name, Error::IntegrityFailed)?;
            return Ok(Self { store, meta });
        }
        let ldk = Zeroizing::new(random_key());
        let signing = Zeroizing::new(random_key());
        let hpke = Zeroizing::new(random_key());
        let provisional = Zeroizing::new(random_key());
        let meta = Meta {
            environment: name.into(),
            device_id: Hash::new(crate::crypto::random_id()),
            signing_pub: Hash::new(signing_public(&signing)),
            hpke_pub: Hash::new(hpke_public(&hpke)),
            created_at: now,
            account: None,
            login_unlocked_at: None,
            fast_unlock: None,
            fast_presence: false,
            fast_declined: false,
        };
        let workspace = Self { store, meta };
        let keys = Keys { ldk, signing, hpke };
        workspace.store.atomic(|store| {
            store.set_meta(PROVISIONAL, &ByteArray::new(*provisional))?;
            store.set_sealed(
                LDK,
                &seal(
                    &provisional,
                    keys.ldk.as_slice(),
                    &workspace.provisional_context(),
                ),
            )?;
            workspace.seal_device(&keys)?;
            store.set_meta(META, &workspace.meta)
        })?;
        Ok(workspace)
    }

    fn provisional_context(&self) -> (&'static str, String) {
        (
            "dmsg/local-key/2",
            format!("provisional:{}", hex::encode(*self.meta.device_id)),
        )
    }

    fn seal_device(&self, keys: &Keys) -> Result<()> {
        let secrets = DeviceSecrets {
            signing: ByteArray::new(*keys.signing),
            hpke: ByteArray::new(*keys.hpke),
        };
        let bytes = Zeroizing::new(canonical(&secrets));
        self.put_sealed(keys, DEVICE, &bytes)
    }

    /// `dmsg:<environment>:<account>:<device>`, the name the LUK binds.
    ///
    /// # Errors
    /// Returns `Error::RecoveryIncomplete` before the device has an account.
    pub fn name(&self) -> Result<String> {
        name_of(&self.meta)
    }

    /// Whether the workspace still waits for its account.
    pub fn provisional(&self) -> bool {
        self.meta.account.is_none()
    }

    /// Unlock a workspace that has no account yet.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` once the workspace belongs to an account.
    pub fn unlock_provisional(&self) -> Result<Keys> {
        ensure(self.provisional(), Error::Forbidden)?;
        let key: ByteArray<32> = self
            .store
            .meta(PROVISIONAL)?
            .ok_or(Error::IntegrityFailed)?;
        self.open_keys(&Zeroizing::new(*key), LDK, &self.provisional_context())
    }

    /// Unlock with the user home's unlock secret for this device.
    ///
    /// # Errors
    /// A wrong secret returns `Error::IntegrityFailed`.
    pub fn unlock(&self, secret: &Key) -> Result<Keys> {
        let name = self.name()?;
        let luk = crate::crypto::unlock_key(secret, &name);
        self.open_keys(&luk, LDK, &("dmsg/local-key/2", name.as_str()))
    }

    fn fast_context(&self) -> Result<(&'static str, String)> {
        Ok(("dmsg/local-key/2", format!("fast:{}", self.name()?)))
    }

    /// Seal a second copy of the LDK under `key`, which the caller keeps in
    /// the operating system's credential store; `presence` when hardware
    /// recovers it only with the person present.
    ///
    /// # Errors
    /// Returns `Error::RecoveryIncomplete` before the device has an account.
    pub fn enable_fast(&mut self, keys: &Keys, key: &Key, now: u64, presence: bool) -> Result<()> {
        let wrapped = seal(key, keys.ldk.as_slice(), &self.fast_context()?);
        let mut meta = self.meta.clone();
        meta.fast_unlock = Some(now);
        meta.fast_presence = presence;
        meta.fast_declined = false;
        self.store.atomic(|store| {
            store.set_sealed(FAST_LDK, &wrapped)?;
            store.set_meta(META, &meta)
        })?;
        self.meta = meta;
        Ok(())
    }

    /// Unlock with the fast-unlock key.
    ///
    /// # Errors
    /// Returns `Error::NotFound` when fast unlock is off and
    /// `Error::IntegrityFailed` for another key.
    pub fn unlock_fast(&self, key: &Key) -> Result<Keys> {
        ensure(self.meta.fast_unlock.is_some(), Error::NotFound)?;
        self.open_keys(key, FAST_LDK, &self.fast_context()?)
    }

    /// Delete the fast-unlock copy of the LDK.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn disable_fast(&mut self) -> Result<()> {
        let mut meta = self.meta.clone();
        meta.fast_unlock = None;
        meta.fast_presence = false;
        self.store.atomic(|store| {
            store.delete_sealed(FAST_LDK)?;
            store.set_meta(META, &meta)
        })?;
        self.meta = meta;
        Ok(())
    }

    fn open_keys<C: Serialize>(&self, key: &Key, record: &str, context: &C) -> Result<Keys> {
        let sealed = self.store.sealed(record)?.ok_or(Error::IntegrityFailed)?;
        let ldk = open(key, &sealed, context)?;
        let ldk: Key = ldk
            .as_slice()
            .try_into()
            .map_err(|_| Error::IntegrityFailed)?;
        let mut keys = Keys {
            ldk: Zeroizing::new(ldk),
            signing: Zeroizing::new([0; 32]),
            hpke: Zeroizing::new([0; 32]),
        };
        let device = self
            .open_sealed(&keys, DEVICE)?
            .ok_or(Error::IntegrityFailed)?;
        let secrets: DeviceSecrets = decode(&device, MAX_RECORD)?;
        keys.signing = Zeroizing::new(*secrets.signing);
        keys.hpke = Zeroizing::new(*secrets.hpke);
        ensure(
            signing_public(&keys.signing) == *self.meta.signing_pub
                && hpke_public(&keys.hpke) == *self.meta.hpke_pub,
            Error::IntegrityFailed,
        )?;
        Ok(keys)
    }

    /// Move a provisional workspace into its account: rewrap the LDK under
    /// the LUK and delete the provisional key in one transaction.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` when the workspace already has an account.
    pub fn bind(&mut self, keys: &Keys, account: AccountRef, secret: &Key) -> Result<()> {
        ensure(self.provisional(), Error::Forbidden)?;
        let mut meta = self.meta.clone();
        meta.account = Some(account);
        let name = name_of(&meta)?;
        let luk = crate::crypto::unlock_key(secret, &name);
        let wrapped = seal(
            &luk,
            keys.ldk.as_slice(),
            &("dmsg/local-key/2", name.as_str()),
        );
        self.store.atomic(|store| {
            store.set_sealed(LDK, &wrapped)?;
            store.delete_meta(PROVISIONAL)?;
            store.set_meta(META, &meta)
        })?;
        self.meta = meta;
        Ok(())
    }

    /// Persist changed metadata.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn save_meta(&self) -> Result<()> {
        self.store.set_meta(META, &self.meta)
    }

    fn sealed_context<'a>(&self, key: &'a str) -> (&'static str, String, &'a str) {
        (
            "dmsg/local-private/1",
            hex::encode(*self.meta.device_id),
            key,
        )
    }

    /// Seal bytes under the LDK for a record named `key`, which only that
    /// name opens; for tables of their own, such as MCP events.
    pub fn seal_record(&self, keys: &Keys, key: &str, bytes: &[u8]) -> String {
        seal(&keys.ldk, bytes, &self.sealed_context(key))
    }

    /// Open a [`Self::seal_record`] result under the same name.
    ///
    /// # Errors
    /// A record sealed under another key or name returns `Error::IntegrityFailed`.
    pub fn open_record(&self, keys: &Keys, key: &str, sealed: &str) -> Result<Zeroizing<Vec<u8>>> {
        open(&keys.ldk, sealed, &self.sealed_context(key))
    }

    /// Seal bytes under the LDK.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn put_sealed(&self, keys: &Keys, key: &str, bytes: &[u8]) -> Result<()> {
        self.store
            .set_sealed(key, &self.seal_record(keys, key, bytes))
    }

    /// Open bytes sealed under the LDK.
    ///
    /// # Errors
    /// A record sealed under another key returns `Error::IntegrityFailed`.
    pub fn open_sealed(&self, keys: &Keys, key: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
        self.store
            .sealed(key)?
            .map(|sealed| self.open_record(keys, key, &sealed))
            .transpose()
    }

    /// Seal a value under the LDK as canonical CBOR.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn put_private<T: Serialize>(&self, keys: &Keys, key: &str, value: &T) -> Result<()> {
        self.put_sealed(keys, key, &Zeroizing::new(canonical(value)))
    }

    /// Open a value sealed by [`Self::put_private`].
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for a corrupt or foreign record.
    pub fn private<T: DeserializeOwned + Serialize>(
        &self,
        keys: &Keys,
        key: &str,
    ) -> Result<Option<T>> {
        self.open_sealed(keys, key)?
            .map(|bytes| decode(&bytes, MAX_PRIVATE))
            .transpose()
    }

    /// At most `limit` values sealed by [`Self::put_private`] under names
    /// starting with `prefix` that sort after `after`, in name order.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for a corrupt or foreign record.
    pub fn private_after<T: DeserializeOwned + Serialize>(
        &self,
        keys: &Keys,
        prefix: &str,
        after: &str,
        limit: usize,
    ) -> Result<Vec<T>> {
        self.store
            .sealed_after(prefix, after, limit)?
            .into_iter()
            .map(|(key, sealed)| decode(&self.open_record(keys, &key, &sealed)?, MAX_PRIVATE))
            .collect()
    }

    /// Delete a sealed record.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn delete_private(&self, key: &str) -> Result<()> {
        self.store.delete_sealed(key)
    }

    /// This device's public enrollment record. Administrators also manage the
    /// root; neither role gets `FormalApprove` or `PaymentOffer` by default.
    pub fn device_input(&self, role: ControllerRole) -> DeviceInput {
        let mut capabilities = vec![Capability::ContentSign, Capability::VaultUnlock];
        if role == ControllerRole::Administrator {
            capabilities.push(Capability::RootManage);
        }
        DeviceInput {
            device_id: self.meta.device_id,
            signing_pub: self.meta.signing_pub,
            hpke_pub: self.meta.hpke_pub,
            role,
            capabilities,
        }
    }
}

fn name_of(meta: &Meta) -> Result<String> {
    let account = meta.account.as_ref().ok_or(Error::RecoveryIncomplete)?;
    Ok(format!(
        "dmsg:{}:{}:{}",
        meta.environment,
        account.id,
        hex::encode(*meta.device_id)
    ))
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
