//! The system credential store: the macOS Keychain, the Windows Credential
//! Manager or the Secret Service on Linux, through `keyring`. The Windows
//! Credential Manager keeps at most 2,560 bytes in one credential, less
//! than a Passkey login's canister-signed delegation, so a longer secret is
//! kept in parts behind a short index under its name.

use dmsg_client::secure::SecureStorage;
use dmsg_types::{Error, Result};
use zeroize::Zeroizing;

const SERVICE: &str = "dmsg";

#[cfg(any(windows, test))]
mod parts;
#[cfg(not(windows))]
pub use Keyring as SystemStorage;
#[cfg(windows)]
pub use parts::SystemStorage;

/// One native credential per name.
pub struct Keyring;

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
