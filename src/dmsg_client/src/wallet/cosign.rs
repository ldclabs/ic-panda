//! Cosigner keys on this device. A platform provider, such as the Secure
//! Enclave, holds a non-exportable P-256 key and asks for the person's
//! presence on every signature; without one, a software P-256 key is
//! sealed under this device's local key and declared weak, so a wallet that
//! requires hardware cosigners refuses it.
use crate::client::Client;
use crate::now_ms;
use dmsg_types::{Error, Result, ensure, wallet::*};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use zeroize::Zeroizing;

const COSIGNERS: &str = "wallet-cosigners";

/// A platform keystore holding non-exportable P-256 cosigner keys.
pub trait CosignProvider: Send + Sync {
    /// Short name, such as `secure-enclave`.
    fn name(&self) -> &'static str;

    /// Create a key labelled `label`; returns its SEC1 compressed public key
    /// and the handle to sign with.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the platform refuses.
    fn create(&self, label: &str) -> Result<(Vec<u8>, String)>;

    /// Sign `message` (P-256 ECDSA over its SHA-256); returns `r || s`.
    /// The platform asks for the person's presence.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the key is gone or the person declines.
    fn sign(&self, handle: &str, message: &[u8]) -> Result<Vec<u8>>;
}

/// Where a local cosigner key lives.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Backend {
    /// A software key, sealed with the rest of this record.
    Software {
        /// The P-256 scalar.
        secret: serde_bytes::ByteBuf,
    },
    /// A platform key named by its handle.
    Platform {
        /// Provider name.
        provider: String,
        /// Key handle.
        handle: String,
    },
}

/// A cosigner key of this device.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LocalCosigner {
    /// Local label, unique on this device.
    pub label: String,
    /// SEC1 compressed P-256 public key.
    pub public_key: serde_bytes::ByteBuf,
    /// Key storage.
    pub backend: Backend,
    /// Creation time.
    pub created_at: u64,
}

impl LocalCosigner {
    /// Whether the key cannot leave its hardware.
    pub fn hardware(&self) -> bool {
        matches!(self.backend, Backend::Platform { .. })
    }

    /// The record without its secret.
    pub fn view(&self) -> Json {
        json!({
            "label": self.label,
            "public_key": hex::encode(&self.public_key),
            "hardware": self.hardware(),
            "backend": match &self.backend {
                Backend::Software { .. } => "software",
                Backend::Platform { provider, .. } => provider.as_str(),
            },
            "created_at": self.created_at,
        })
    }

    /// The input of an `AddCosigner` proposal for `member`.
    pub fn input(&self, member: MemberId) -> CosignerInput {
        CosignerInput {
            member,
            kind: CosignerKind::P256,
            public_key: self.public_key.clone(),
            envelope: CosignerEnvelope::Raw,
            hardware: self.hardware(),
            label: self.label.clone(),
        }
    }
}

impl Client {
    /// Use a platform keystore for new cosigner keys.
    pub fn set_cosign_provider(&mut self, provider: std::sync::Arc<dyn CosignProvider>) {
        self.cosign = Some(provider);
    }

    /// Cosigner keys of this device.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn wallet_cosigner_keys(&self) -> Result<Vec<LocalCosigner>> {
        Ok(self
            .workspace
            .private(self.keys()?, COSIGNERS)?
            .unwrap_or_default())
    }

    /// Cosigner keys of this device, without secrets.
    ///
    /// # Errors
    /// As [`Self::wallet_cosigner_keys`].
    pub fn wallet_cosigners_local(&self) -> Result<Json> {
        Ok(Json::Array(
            self.wallet_cosigner_keys()?
                .iter()
                .map(LocalCosigner::view)
                .collect(),
        ))
    }

    /// Create a cosigner key: in the platform keystore when one is set and
    /// works, else in software unless `hardware` is required.
    ///
    /// # Errors
    /// Returns `Error::IdempotencyConflict` for a label in use and
    /// `Error::Unavailable` when hardware is required but missing.
    pub fn wallet_cosigner_create(&mut self, label: &str, hardware: bool) -> Result<LocalCosigner> {
        ensure(
            !label.is_empty() && label.len() <= 64,
            dmsg_types::invalid("label"),
        )?;
        let mut keys = self.wallet_cosigner_keys()?;
        ensure(
            keys.iter().all(|k| k.label != label),
            Error::IdempotencyConflict,
        )?;
        let platform = self.cosign.as_ref().map(|p| (p.name(), p.create(label)));
        let (public_key, backend) = match platform {
            Some((name, Ok((public_key, handle)))) => (
                public_key,
                Backend::Platform {
                    provider: name.into(),
                    handle,
                },
            ),
            _ if hardware => {
                return Err(Error::Unavailable(
                    "no hardware cosigner key on this device".into(),
                ));
            }
            _ => {
                let secret = loop {
                    let candidate = Zeroizing::new(crate::crypto::random_key());
                    if SigningKey::from_slice(candidate.as_slice()).is_ok() {
                        break candidate;
                    }
                };
                let key = SigningKey::from_slice(secret.as_slice()).expect("valid scalar");
                let public_key = key.verifying_key().to_sec1_point(true).as_bytes().to_vec();
                (
                    public_key,
                    Backend::Software {
                        secret: serde_bytes::ByteBuf::from(secret.to_vec()),
                    },
                )
            }
        };
        let cosigner = LocalCosigner {
            label: label.into(),
            public_key: public_key.into(),
            backend,
            created_at: now_ms(),
        };
        keys.push(cosigner.clone());
        self.workspace.put_private(self.keys()?, COSIGNERS, &keys)?;
        Ok(cosigner)
    }

    /// Sign a cosign message with the local key whose public key is `public_key`.
    pub(crate) fn wallet_cosign(&self, public_key: &[u8], message: &[u8]) -> Result<Vec<u8>> {
        let key = self
            .wallet_cosigner_keys()?
            .into_iter()
            .find(|k| k.public_key.as_slice() == public_key)
            .ok_or(Error::NotFound)?;
        match &key.backend {
            Backend::Software { secret } => {
                let signing = SigningKey::from_slice(secret).map_err(|_| Error::IntegrityFailed)?;
                let sig: Signature = signing.sign(message);
                Ok(sig.to_bytes().to_vec())
            }
            Backend::Platform { handle, .. } => {
                let provider = self.cosign.as_ref().ok_or_else(|| {
                    Error::Unavailable("the cosigner's keystore is not available".into())
                })?;
                provider.sign(handle, message)
            }
        }
    }
}
