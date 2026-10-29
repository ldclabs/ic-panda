//! Cosigner keys on this device. A platform provider, such as the Secure
//! Enclave, holds a non-exportable P-256 key and asks for the person's
//! presence on every signature; without one, a software P-256 key is
//! sealed under this device's local key and declared weak, so a wallet that
//! requires hardware cosigners refuses it.
use crate::client::Client;
use crate::hold::Hold;
use crate::now_ms;
use dmsg_types::{Error, Hash, Result, ensure, wallet::*};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use p256::ecdsa::{VerifyingKey, signature::Verifier};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::{future::Future, pin::Pin, sync::Arc};
use zeroize::Zeroizing;

const COSIGNERS: &str = "wallet-cosigners";

/// Test only a fresh, domain-separated local challenge. No signature is
/// exported and no wallet or chain authorization is produced.
pub async fn test_key<H: Hold>(h: &H, label: &str) -> Result<Json> {
    let bound = crate::hold::Unlocked::new(h).await?;
    let h = &bound;
    let (public, signer) = h
        .with(|c| {
            let key = c
                .wallet_cosigner_keys()?
                .into_iter()
                .find(|k| k.label == label)
                .ok_or(Error::NotFound)?;
            Ok((key.public_key.clone(), c.prepare_cosign(&key.public_key)?))
        })
        .await?;
    let challenge = dmsg_protocol::digest(
        "dmsg-wallet/local-key-test/v1",
        &(public.clone(), crate::crypto::random_id()),
    );
    let signatures = signer.sign(vec![challenge]).await?;
    let signature = signatures.first().ok_or(Error::IntegrityFailed)?;
    VerifyingKey::from_sec1_bytes(&public)
        .map_err(|_| Error::IntegrityFailed)?
        .verify(
            &challenge[..],
            &Signature::from_slice(signature).map_err(|_| Error::IntegrityFailed)?,
        )
        .map_err(|_| Error::IntegrityFailed)?;
    h.with(|_| Ok(())).await?;
    Ok(json!({"label":label,"public_key":hex::encode(&public),"verified":true,"read_at":now_ms()}))
}

/// A batch signature operation that may wait for user presence.
pub type CosignFuture = Pin<Box<dyn Future<Output = Result<Vec<Vec<u8>>>> + Send>>;

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

    /// Sign the exact batch of messages (P-256 ECDSA over each SHA-256).
    /// Ask for presence once for this batch; never retain that authorization.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the key is gone or the person declines.
    fn sign(&self, handle: String, messages: Vec<Hash>) -> CosignFuture;

    /// The most messages one batch signs, as the macOS Secure Enclave does;
    /// more take several batches, each asking for presence.
    fn batch_limit(&self) -> usize {
        32
    }
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

    /// Create a cosigner key in the platform keystore when `hardware`, else
    /// in software. A hardware key that cannot be made is an error, never a
    /// software key in its place: only the caller chooses software.
    ///
    /// # Errors
    /// Returns `Error::IdempotencyConflict` for a label in use and
    /// `Error::Unavailable` when this device has no hardware keystore, or the
    /// keystore's error when it cannot make the key.
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
        let (public_key, backend) = if hardware {
            let platform = self.cosign.as_ref().ok_or_else(|| {
                Error::Unavailable("no hardware cosigner key on this device".into())
            })?;
            let (public_key, handle) = platform.create(label)?;
            (
                public_key,
                Backend::Platform {
                    provider: platform.name().into(),
                    handle,
                },
            )
        } else {
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

    /// Take the signer out of the client before waiting for user presence.
    pub(crate) fn prepare_cosign(&self, public_key: &[u8]) -> Result<PreparedSigner> {
        let key = self
            .wallet_cosigner_keys()?
            .into_iter()
            .find(|k| k.public_key.as_slice() == public_key)
            .ok_or(Error::NotFound)?;
        match key.backend {
            Backend::Software { secret } => {
                Ok(PreparedSigner::Software(Zeroizing::new(secret.into_vec())))
            }
            Backend::Platform { handle, provider } => {
                let available = self
                    .cosign
                    .clone()
                    .filter(|p| p.name() == provider)
                    .ok_or_else(|| {
                        Error::Unavailable("the cosigner's keystore is not available".into())
                    })?;
                Ok(PreparedSigner::Platform {
                    provider: available,
                    handle,
                })
            }
        }
    }
}

/// A single batch, used without holding Client. Software material is erased on drop.
pub(crate) enum PreparedSigner {
    Software(Zeroizing<Vec<u8>>),
    Platform {
        provider: Arc<dyn CosignProvider>,
        handle: String,
    },
}

impl PreparedSigner {
    pub(crate) async fn sign(self, messages: Vec<Hash>) -> Result<Vec<Vec<u8>>> {
        match self {
            Self::Software(secret) => {
                let key = SigningKey::from_slice(&secret).map_err(|_| Error::IntegrityFailed)?;
                Ok(messages
                    .iter()
                    .map(|m| {
                        let signature: Signature = key.sign(&m[..]);
                        signature.to_bytes().to_vec()
                    })
                    .collect())
            }
            Self::Platform { provider, handle } => {
                let mut signatures = Vec::with_capacity(messages.len());
                for batch in messages.chunks(provider.batch_limit().max(1)) {
                    signatures.extend(provider.sign(handle.clone(), batch.to_vec()).await?);
                }
                Ok(signatures)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        hold::Hold,
        testing::{TempDir, unlocked},
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::{Mutex, Notify};

    #[derive(Default)]
    struct WaitingProvider {
        started: Arc<Notify>,
        release: Arc<Notify>,
        calls: Arc<AtomicUsize>,
    }

    impl CosignProvider for WaitingProvider {
        fn name(&self) -> &'static str {
            "test-hardware"
        }

        fn create(&self, _: &str) -> Result<(Vec<u8>, String)> {
            let key = SigningKey::from_slice(&[7; 32]).unwrap();
            Ok((
                key.verifying_key().to_sec1_point(true).as_bytes().to_vec(),
                "key".into(),
            ))
        }

        fn sign(&self, _: String, messages: Vec<Hash>) -> CosignFuture {
            let started = self.started.clone();
            let release = self.release.clone();
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                started.notify_one();
                release.notified().await;
                let key = SigningKey::from_slice(&[7; 32]).unwrap();
                Ok(messages
                    .iter()
                    .map(|m| {
                        let s: Signature = key.sign(&m[..]);
                        s.to_bytes().to_vec()
                    })
                    .collect())
            })
        }
    }

    struct Shared(Mutex<Client>);

    impl Hold for Shared {
        async fn with<T: Send>(
            &self,
            step: impl FnOnce(&mut Client) -> Result<T> + Send,
        ) -> Result<T> {
            step(&mut *self.0.lock().await)
        }
    }

    /// Signs with one fixed key, two messages a batch.
    #[derive(Default)]
    struct SmallBatches(std::sync::Mutex<Vec<usize>>);

    impl CosignProvider for SmallBatches {
        fn name(&self) -> &'static str {
            "test-hardware"
        }

        fn create(&self, _: &str) -> Result<(Vec<u8>, String)> {
            let key = SigningKey::from_slice(&[7; 32]).unwrap();
            Ok((
                key.verifying_key().to_sec1_point(true).as_bytes().to_vec(),
                "key".into(),
            ))
        }

        fn sign(&self, _: String, messages: Vec<Hash>) -> CosignFuture {
            self.0.lock().unwrap().push(messages.len());
            Box::pin(async move {
                let key = SigningKey::from_slice(&[7; 32]).unwrap();
                Ok(messages
                    .iter()
                    .map(|m| {
                        let s: Signature = key.sign(&m[..]);
                        s.to_bytes().to_vec()
                    })
                    .collect())
            })
        }

        fn batch_limit(&self) -> usize {
            2
        }
    }

    #[tokio::test]
    async fn hardware_signing_splits_what_one_batch_cannot_hold() {
        let provider = Arc::new(SmallBatches::default());
        let signer = PreparedSigner::Platform {
            provider: provider.clone(),
            handle: "key".into(),
        };
        let messages: Vec<Hash> = (0..5u8).map(|i| Hash::new([i; 32])).collect();
        let signatures = signer.sign(messages.clone()).await.unwrap();
        assert_eq!(*provider.0.lock().unwrap(), [2, 2, 1]);
        let key = SigningKey::from_slice(&[7; 32]).unwrap();
        for (message, signature) in messages.iter().zip(&signatures) {
            key.verifying_key()
                .verify(&message[..], &Signature::from_slice(signature).unwrap())
                .unwrap();
        }
    }

    #[test]
    fn only_the_caller_chooses_a_software_key() {
        let dir = TempDir::new();
        let mut client = unlocked(&dir.0, false, None);
        assert!(matches!(
            client.wallet_cosigner_create("missing", true),
            Err(Error::Unavailable(_))
        ));
        client.set_cosign_provider(Arc::new(WaitingProvider::default()));
        assert!(
            client
                .wallet_cosigner_create("hardware", true)
                .unwrap()
                .hardware()
        );
        assert!(
            !client
                .wallet_cosigner_create("software", false)
                .unwrap()
                .hardware()
        );
        let labels: Vec<_> = client
            .wallet_cosigner_keys()
            .unwrap()
            .into_iter()
            .map(|k| k.label)
            .collect();
        assert_eq!(labels, ["hardware", "software"]);
    }

    #[tokio::test]
    async fn local_key_test_verifies_a_challenge_without_exporting_a_signature() {
        let dir = TempDir::new();
        let mut client = unlocked(&dir.0, false, None);
        client.wallet_cosigner_create("local test", false).unwrap();
        let h = Shared(Mutex::new(client));
        let result = test_key(&h, "local test").await.unwrap();
        assert_eq!(result["verified"], true);
        assert!(result.get("signature").is_none());
        assert_eq!(
            h.with(|c| Ok(c.wallet_cosigner_keys()?.len()))
                .await
                .unwrap(),
            1
        );
        assert!(test_key(&h, "missing").await.is_err());
    }

    #[tokio::test]
    async fn hardware_batch_releases_client_and_discards_signatures_after_lock() {
        let dir = TempDir::new();
        let mut client = unlocked(&dir.0, false, None);
        let provider = Arc::new(WaitingProvider::default());
        client.set_cosign_provider(provider.clone());
        let key = client.wallet_cosigner_create("hardware", true).unwrap();
        let vectors: Json = serde_json::from_str(include_str!(
            "../../../../tests/vectors/wallet/plans-v1.json"
        ))
        .unwrap();
        let mut view: WalletView = dmsg_protocol::decode_canonical(
            &hex::decode(vectors["view_cbor_hex"].as_str().unwrap()).unwrap(),
        )
        .unwrap();
        view.cosigners = vec![Cosigner {
            id: 1,
            input: key.input(client.wallet_member().unwrap()),
            counter: 0,
            added_at_ms: 0,
        }];
        let shared = Arc::new(Shared(Mutex::new(client)));
        for lock in [false, true] {
            let h = shared.clone();
            let view = view.clone();
            let task = tokio::spawn(async move {
                crate::wallet::ops::local_cosigs(
                    h.as_ref(),
                    &view,
                    1,
                    1,
                    1,
                    &[Hash::new([1; 32]), Hash::new([2; 32])],
                )
                .await
            });
            provider.started.notified().await;
            tokio::time::timeout(
                std::time::Duration::from_secs(1),
                shared.with(|c| {
                    if lock {
                        c.lock();
                    } else {
                        c.keys()?;
                    }
                    Ok(())
                }),
            )
            .await
            .expect("user presence must not hold the client")
            .unwrap();
            provider.release.notify_one();
            let result = task.await.unwrap();
            if lock {
                assert_eq!(result.err(), Some(Error::Locked));
            } else {
                assert_eq!(result.unwrap().len(), 2);
            }
        }
        assert_eq!(
            provider.calls.load(Ordering::SeqCst),
            2,
            "one presence request per batch, not per digest"
        );
    }
}
