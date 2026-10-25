//! Secrets the operating system keeps for the device, outside the workspace
//! file: the desktop daemon uses the system credential store, a mobile app
//! the Keychain or Keystore. The workspace never holds what opens it.
//!
//! A [`PresenceKey`] is hardware that releases a secret only with the
//! person present, such as the Secure Enclave behind Touch ID; fast unlock
//! and the device-bound local key use it when the device has one.
//! [`ApprovalKeys`] are hardware signing keys whose signature the user home
//! requires for the approvals an account protects.

use crate::crypto::Key;
use dmsg_types::{Hash, Result};
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
    /// handle, the peer public key and the secret. Asks nothing. A
    /// `biometry_only` key takes only the fingerprints enrolled now, never
    /// the device password.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the hardware refuses.
    fn create(&self, biometry_only: bool) -> Result<(Vec<u8>, Vec<u8>, Zeroizing<Key>)>;

    /// Recover the secret of [`Self::create`]; the person must be present.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` when the person declines,
    /// `Error::IntegrityFailed` for a key this hardware no longer opens and
    /// `Error::Unavailable` when it cannot ask now, such as without its
    /// Touch ID keyboard or while locked out.
    fn open(&self, handle: &[u8], peer: &[u8], biometry_only: bool) -> Result<Zeroizing<Key>>;

    /// A digest of the enrolled fingerprints, which changes when they do;
    /// `None` without biometry. A `biometry_only` key that fails after it
    /// changed is lost, not just unavailable.
    fn biometry_state(&self) -> Option<Hash>;

    /// Ask the person to confirm approving a high-risk request of type
    /// `what` (such as `checkout` or `device_revoke`), which names the
    /// system's question.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` when the person declines and
    /// `Error::Unavailable` when the hardware cannot ask.
    fn confirm(&self, what: &str) -> Result<()>;
}

/// Hardware that keeps approval keys: P-256 signing keys that sign only
/// with the person present. The user home requires their signature for the
/// approvals an account protects.
pub trait ApprovalKeys: Send + Sync {
    /// A new key: its SEC1 compressed public key (33 bytes) and its opaque
    /// handle. Asks nothing. A `biometry_only` key takes only the
    /// fingerprints enrolled now.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the hardware refuses.
    fn create(&self, biometry_only: bool) -> Result<(Vec<u8>, Vec<u8>)>;

    /// Ask the person once, about a request of type `what`; the answer
    /// signs with this device's approval keys until it is dropped.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` when the person declines and
    /// `Error::Unavailable` when the hardware cannot ask now.
    fn authorize(&self, what: &str, biometry_only: bool) -> Result<Box<dyn Authorized>>;
}

/// The person's answer to one question, which signs with approval keys
/// until dropped; it is never kept beyond the decision it answers.
pub trait Authorized: Send + Sync {
    /// Sign a 32-byte message (P-256 ECDSA over its SHA-256) with the key
    /// `handle`, as `r || s`.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for a key this hardware no longer
    /// opens and `Error::Unavailable` otherwise.
    fn sign(&self, handle: &[u8], message: &Hash) -> Result<[u8; 64]>;
}
