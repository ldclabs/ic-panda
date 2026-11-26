//! Secrets the operating system keeps for the device, outside the workspace
//! file: the desktop daemon uses the system credential store, a mobile app
//! the Keychain or Keystore. The workspace never holds what opens it.
//!
//! A [`PresenceKey`] is hardware that releases a secret only with the
//! person present, such as the Secure Enclave behind Touch ID; fast unlock
//! and the device-bound local key use it when the device has one.
//! [`ApprovalKeys`] are hardware signing keys whose signature the user home
//! requires for the approvals an account protects. A [`HardwareAnchor`]
//! never asks for the person: it shows that a workspace still runs on the
//! hardware that made it.

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
    /// Release a platform-owned presence key once its standalone workspace no longer references it.
    fn discard_local_key(&self, _handle: &[u8]) -> Result<()> {
        Ok(())
    }

    /// How this device checks the person; `None` without usable hardware.
    fn presence(&self) -> Option<Presence>;

    /// A new hardware key and a secret only it recovers: the key's opaque
    /// handle, opaque platform context and the secret. The Secure Enclave
    /// and Keystore ask nothing; Windows Hello asks for the person, so a
    /// login's new binding makes its key outside the client. A
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

    /// A platform can enforce current-biometry keys without exposing an enrollment digest.
    fn supports_biometry_only(&self) -> bool {
        self.biometry_state().is_some()
    }

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
    /// Prepare one exact signature without prompting. Mobile providers return
    /// a cancellable operation; legacy desktop providers authorize at sign time.
    /// # Errors
    /// Refuses unavailable keys or an invalid message before any human prompt.
    fn prepare_signature(
        &self,
        _what: &str,
        _biometry_only: bool,
        _handle: &[u8],
        _message: &Hash,
    ) -> Result<Option<std::sync::Arc<dyn PreparedSignature>>> {
        Ok(None)
    }

    /// Whether this provider currently has hardware capable of user presence.
    /// This is capability discovery, not evidence that a stored key will open.
    fn available(&self) -> bool {
        true
    }

    /// A new key: its SEC1 compressed public key (33 bytes) and its opaque
    /// handle. Asks nothing. A `biometry_only` key takes only the
    /// fingerprints enrolled now.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the hardware refuses.
    fn create(&self, biometry_only: bool) -> Result<(Vec<u8>, Vec<u8>)>;

    /// Remove an unreferenced key from providers that retain named key objects.
    /// Wrapped-key providers need no separate deletion once their handle is gone.
    ///
    /// # Errors
    /// Returns the provider's storage failure; callers never delete a kept key
    /// merely because authentication was cancelled.
    fn delete_key(&self, _handle: &[u8]) -> Result<()> {
        Ok(())
    }

    /// Ask the person once, about a request of type `what`; the answer
    /// signs with this device's approval keys until it is dropped.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` when the person declines and
    /// `Error::Unavailable` when the hardware cannot ask now.
    fn authorize(&self, what: &str, biometry_only: bool) -> Result<Box<dyn Authorized>>;

    /// Prepare only the named signing-key operations outside the Client.
    /// Per-use platforms authenticate each operation now; sign must not prompt.
    fn authorize_keys(
        &self,
        what: &str,
        biometry_only: bool,
        _handles: &[Vec<u8>],
    ) -> Result<Box<dyn Authorized>> {
        self.authorize(what, biometry_only)
    }
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

/// One immutable, single-use signing operation owned by a host decision.
pub trait PreparedSignature: Send + Sync {
    /// Authenticate and sign the already fixed message, off the Client executor.
    /// # Errors
    /// Refuses cancelled, consumed or expired operations and invalidated keys.
    fn sign(&self) -> Result<[u8; 64]>;

    /// Revoke this operation, including a prompt queued but not yet displayed.
    fn cancel(&self);
}

/// A host keeps this guard across the blocking worker await. Cancelling that
/// future immediately revokes only its prepared signature, even while the
/// worker still owns another reference to the operation.
#[derive(Default)]
pub struct SigningCancellation(pub(crate) Option<std::sync::Arc<dyn PreparedSignature>>);

impl SigningCancellation {
    /// Bind the async owner's lifetime to this one prepared operation.
    pub fn new(signature: std::sync::Arc<dyn PreparedSignature>) -> Self {
        Self(Some(signature))
    }
}

impl Drop for SigningCancellation {
    fn drop(&mut self) {
        if let Some(operation) = &self.0 {
            operation.cancel();
        }
    }
}

/// Hardware that keeps signing keys usable without the person, only on the
/// hardware that made them: a workspace signs with its anchor to show it
/// has not been copied to other hardware, such as by a system migration or
/// a restored backup.
pub trait HardwareAnchor: Send + Sync {
    /// Release a platform-owned anchor key once its standalone workspace no longer references it.
    fn discard_local_key(&self, _handle: &[u8]) -> Result<()> {
        Ok(())
    }

    /// Sign a bounded IC request or delegation message with an independent
    /// login key, using P-256 ECDSA over SHA-256. Never asks for presence.
    /// # Errors
    /// Unsupported providers refuse rather than using a software key.
    fn sign_message(&self, _handle: &[u8], _message: &[u8]) -> Result<[u8; 64]> {
        Err(dmsg_types::Error::UnsupportedProtocol)
    }

    /// A platform identifier for hosts without an enclave (older Macs).
    /// This only detects ordinary system copies, not deliberate tampering.
    fn platform(&self) -> Option<Hash> {
        None
    }

    /// A new key: its opaque handle and its SEC1 public key. Asks nothing.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the hardware refuses.
    fn create(&self) -> Result<(Vec<u8>, Vec<u8>)>;

    /// Sign a 32-byte message (P-256 ECDSA over its SHA-256) with the key
    /// `handle`, as `r || s`, asking nothing.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` when this hardware cannot use
    /// `handle` and `Error::Unavailable` when it may later.
    fn sign(&self, handle: &[u8], message: &Hash) -> Result<[u8; 64]>;
}
