//! The system credential store: the macOS Keychain, the Windows Credential
//! Manager or the Secret Service on Linux, through `keyring`.

use dmsg_client::secure::SecureStorage;
use dmsg_types::{Error, Result};
use zeroize::Zeroizing;

const SERVICE: &str = "dmsg";

/// The system credential store under the service `dmsg`.
pub struct SystemStorage;

fn unavailable(error: keyring::Error) -> Error {
    Error::Unavailable(format!("credential store: {error}"))
}

impl SecureStorage for SystemStorage {
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
