//! Secrets the operating system keeps for the device, outside the workspace
//! file: the desktop daemon uses the system credential store, a mobile app
//! the Keychain or Keystore. The workspace never holds what opens it.
//!
//! A [`PresenceKey`] is hardware that releases a secret only with the
//! person present, such as the Secure Enclave behind Touch ID; fast unlock
//! uses it when the device has one.

use crate::crypto::Key;
use dmsg_types::Result;
use serde::Serialize;
use zeroize::Zeroizing;

/// A credential store.
pub trait SecureStorage: Send + Sync {
    /// The secret stored under `name`, if any.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the store cannot be reached.
    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>>;

    /// Store `secret` under `name`, replacing any earlier one.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the store cannot be reached.
    fn set(&self, name: &str, secret: &[u8]) -> Result<()>;

    /// Remove the secret under `name`; an absent one is not an error.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the store cannot be reached.
    fn delete(&self, name: &str) -> Result<()>;
}

/// How a [`PresenceKey`] checks the person.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    /// A fingerprint or face, falling back to the device password.
    Biometrics,
    /// The device password.
    Password,
}

/// Hardware that keeps a key and uses it only with the person present.
pub trait PresenceKey: Send + Sync {
    /// How this device checks the person; `None` without usable hardware.
    fn presence(&self) -> Option<Presence>;

    /// A new hardware key and a secret only it recovers: the key's opaque
    /// handle, the peer public key and the secret. Asks nothing.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the hardware refuses.
    fn create(&self) -> Result<(Vec<u8>, Vec<u8>, Zeroizing<Key>)>;

    /// Recover the secret of [`Self::create`]; the person must be present.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` when the person declines,
    /// `Error::IntegrityFailed` for a key this hardware no longer opens and
    /// `Error::Unavailable` otherwise.
    fn open(&self, handle: &[u8], peer: &[u8]) -> Result<Zeroizing<Key>>;
}
