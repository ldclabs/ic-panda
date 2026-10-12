//! Secrets the operating system keeps for the device, outside the workspace
//! file: the desktop daemon uses the system credential store, a mobile app
//! the Keychain or Keystore. The workspace never holds what opens it.

use dmsg_types::Result;
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
