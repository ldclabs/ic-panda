use crate::error::MobileError;
use dmsg_client::{
    crypto::Key,
    secure::{ApprovalKeys, Authorized, HardwareAnchor, Presence, PresenceKey, SecureStorage},
};
use dmsg_types::{Error, Hash, Result, ensure};
use std::sync::Arc;
use zeroize::Zeroizing;

#[derive(uniffi::Record)]
pub struct HardwareSecret {
    pub handle: Vec<u8>,
    pub context: Vec<u8>,
    pub secret: Vec<u8>,
}

#[derive(uniffi::Record)]
pub struct SigningKey {
    pub public_key: Vec<u8>,
    pub handle: Vec<u8>,
}

/// Private platform adapter. User-facing APIs never accept a confirmation flag.
#[uniffi::export(callback_interface)]
pub trait Platform: Send + Sync {
    fn get(&self, name: String) -> std::result::Result<Option<Vec<u8>>, MobileError>;
    fn set(&self, name: String, secret: Vec<u8>) -> std::result::Result<(), MobileError>;
    fn delete(&self, name: String) -> std::result::Result<(), MobileError>;
    /// 0 unavailable, 1 device passcode, 2 biometrics with passcode fallback.
    fn presence(&self) -> u8;
    fn biometry_state(&self) -> Option<Vec<u8>>;
    fn supports_biometry_only(&self) -> bool;
    fn create_secret(
        &self,
        biometry_only: bool,
    ) -> std::result::Result<HardwareSecret, MobileError>;
    fn open_secret(
        &self,
        handle: Vec<u8>,
        context: Vec<u8>,
        biometry_only: bool,
    ) -> std::result::Result<Vec<u8>, MobileError>;
    fn create_signing(&self, biometry_only: bool) -> std::result::Result<SigningKey, MobileError>;
    /// Delete a named hardware key whose handle is no longer referenced. Wrapped
    /// key providers have no separate persistent object and may return success.
    fn delete_signing(&self, handle: Vec<u8>) -> std::result::Result<(), MobileError>;
    /// Register final bytes and an operation identity before user interaction.
    fn prepare_signature(
        &self,
        reason: String,
        biometry_only: bool,
        handle: Vec<u8>,
        message: Vec<u8>,
    ) -> std::result::Result<u64, MobileError>;
    fn sign_prepared(&self, preparation: u64) -> std::result::Result<Vec<u8>, MobileError>;
    fn release_prepared(&self, preparation: u64);
    /// A private, cancellable authorization, owned by one Rust decision.
    fn authorize(
        &self,
        reason: String,
        biometry_only: bool,
    ) -> std::result::Result<u64, MobileError>;
    fn authorize_keys(
        &self,
        reason: String,
        biometry_only: bool,
        handles: Vec<Vec<u8>>,
    ) -> std::result::Result<u64, MobileError>;
    fn sign_authorized(
        &self,
        authorization: u64,
        handle: Vec<u8>,
        message: Vec<u8>,
    ) -> std::result::Result<Vec<u8>, MobileError>;
    fn release_authorization(&self, authorization: u64);
    /// A Secure Enclave signing key that never asks for the person and
    /// works after the first unlock since the device started; its public
    /// key in SEC1 form.
    fn create_anchor(&self) -> std::result::Result<SigningKey, MobileError>;
    /// Sign a bounded message with an anchor or independent login key, asking nothing:
    /// `INTEGRITY_FAILED` when this device cannot use `handle`.
    fn sign_anchor(
        &self,
        handle: Vec<u8>,
        message: Vec<u8>,
    ) -> std::result::Result<Vec<u8>, MobileError>;
    fn confirm(
        &self,
        request_id: String,
        digest: String,
        reason: String,
    ) -> std::result::Result<(), MobileError>;
    fn cancel_presence(&self);
}

pub struct Adapter(pub Arc<dyn Platform>);

// A blocking hardware call cannot be interrupted by dropping its JoinHandle.
// Keep the batch in the async owner so cancellation cannot start another prompt.
async fn sign_wallet_messages(
    platform: Arc<dyn Platform>,
    handle: Vec<u8>,
    messages: Vec<Hash>,
    per_use: bool,
) -> Result<Vec<Vec<u8>>> {
    if per_use {
        let mut signatures = Vec::with_capacity(messages.len());
        for message in messages {
            let token = platform
                .prepare_signature(
                    "wallet".into(),
                    false,
                    handle.clone(),
                    message.as_slice().into(),
                )
                .map_err(platform_error)?;
            let answer: Arc<dyn dmsg_client::secure::PreparedSignature> =
                Arc::new(PreparedAnswer {
                    platform: platform.clone(),
                    token,
                });
            let _signature = dmsg_client::secure::SigningCancellation::new(answer.clone());
            let signature = crate::runtime()
                .spawn_blocking(move || answer.sign().map(Vec::from))
                .await
                .map_err(|_| Error::Unavailable("hardware: wallet signer".into()))??;
            signatures.push(signature);
        }
        return Ok(signatures);
    }
    crate::runtime()
        .spawn_blocking(move || {
            let token = platform
                .authorize("wallet".into(), false)
                .map_err(platform_error)?;
            let answer = Answer { platform, token };
            messages
                .into_iter()
                .map(|message| answer.sign(&handle, &message).map(Vec::from))
                .collect()
        })
        .await
        .map_err(|_| Error::Unavailable("hardware: wallet signer".into()))?
}

pub fn platform_error(error: MobileError) -> Error {
    let MobileError::Failure { code } = error;
    match code.as_str() {
        "CANCELLED" | "FORBIDDEN" => Error::Forbidden,
        "INTEGRITY_FAILED" => Error::IntegrityFailed,
        "HARDWARE_UNAVAILABLE" => Error::Unavailable("hardware: unavailable".into()),
        "BIOMETRIC_LOCKOUT"
        | "BIOMETRIC_LOCKOUT_PERMANENT"
        | "BIOMETRIC_NOT_ENROLLED"
        | "DEVICE_CREDENTIAL_NOT_SET"
        | "HARDWARE_SECURITY_UPDATE_REQUIRED"
        | "AUTHENTICATION_TIMEOUT" => Error::Unavailable(format!("hardware: {code}")),
        _ => Error::Unavailable("platform".into()),
    }
}

impl SecureStorage for Adapter {
    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
        self.0
            .get(name.into())
            .map(|v| v.map(Zeroizing::new))
            .map_err(platform_error)
    }

    fn set(&self, name: &str, secret: &[u8]) -> Result<()> {
        self.0
            .set(name.into(), secret.into())
            .map_err(platform_error)
    }

    fn delete(&self, name: &str) -> Result<()> {
        self.0.delete(name.into()).map_err(platform_error)
    }
}

impl PresenceKey for Adapter {
    fn discard_local_key(&self, handle: &[u8]) -> Result<()> {
        self.0
            .delete_signing(handle.to_vec())
            .map_err(platform_error)
    }

    fn presence(&self) -> Option<Presence> {
        match self.0.presence() {
            1 => Some(Presence::Password),
            2 => Some(Presence::Biometrics),
            _ => None,
        }
    }

    fn create(&self, biometry_only: bool) -> Result<(Vec<u8>, Vec<u8>, Zeroizing<Key>)> {
        let created = self
            .0
            .create_secret(biometry_only)
            .map_err(platform_error)?;
        let secret = Zeroizing::new(created.secret);
        let key: Key = secret
            .as_slice()
            .try_into()
            .map_err(|_| Error::IntegrityFailed)?;
        Ok((created.handle, created.context, Zeroizing::new(key)))
    }

    fn open(&self, handle: &[u8], peer: &[u8], biometry_only: bool) -> Result<Zeroizing<Key>> {
        let bytes = Zeroizing::new(
            self.0
                .open_secret(handle.into(), peer.into(), biometry_only)
                .map_err(platform_error)?,
        );
        Ok(Zeroizing::new(
            bytes
                .as_slice()
                .try_into()
                .map_err(|_| Error::IntegrityFailed)?,
        ))
    }

    fn supports_biometry_only(&self) -> bool {
        self.0.supports_biometry_only()
    }

    fn biometry_state(&self) -> Option<Hash> {
        self.0.biometry_state()?.try_into().ok().map(Hash::new)
    }

    fn confirm(&self, _what: &str) -> Result<()> {
        // Approval confirmation must include its ID and displayed digest.
        Err(Error::Forbidden)
    }
}

struct Answer {
    platform: Arc<dyn Platform>,
    token: u64,
}

impl Drop for Answer {
    fn drop(&mut self) {
        self.platform.release_authorization(self.token);
    }
}

impl Authorized for Answer {
    fn sign(&self, handle: &[u8], message: &Hash) -> Result<[u8; 64]> {
        self.platform
            .sign_authorized(self.token, handle.into(), message.as_slice().into())
            .map_err(platform_error)?
            .try_into()
            .map_err(|_| Error::IntegrityFailed)
    }
}

impl ApprovalKeys for Adapter {
    fn prepare_signature(
        &self,
        what: &str,
        biometry_only: bool,
        handle: &[u8],
        message: &Hash,
    ) -> Result<Option<Arc<dyn dmsg_client::secure::PreparedSignature>>> {
        let token = self
            .0
            .prepare_signature(
                what.into(),
                biometry_only,
                handle.into(),
                message.as_slice().into(),
            )
            .map_err(platform_error)?;
        Ok(Some(Arc::new(PreparedAnswer {
            platform: self.0.clone(),
            token,
        })))
    }

    fn authorize_keys(
        &self,
        what: &str,
        biometry_only: bool,
        handles: &[Vec<u8>],
    ) -> Result<Box<dyn Authorized>> {
        ensure(
            !handles.is_empty()
                && handles.len() <= 2
                && handles.iter().all(|handle| !handle.is_empty()),
            Error::IntegrityFailed,
        )?;
        let token = self
            .0
            .authorize_keys(what.into(), biometry_only, handles.to_vec())
            .map_err(platform_error)?;
        Ok(Box::new(Answer {
            platform: self.0.clone(),
            token,
        }))
    }

    fn available(&self) -> bool {
        self.0.presence() != 0
    }

    fn create(&self, biometry_only: bool) -> Result<(Vec<u8>, Vec<u8>)> {
        let key = self
            .0
            .create_signing(biometry_only)
            .map_err(platform_error)?;
        if let Err(error) = dmsg_protocol::approval_key_id(&key.public_key) {
            let _ = self.0.delete_signing(key.handle);
            return Err(error);
        }
        ensure(!key.handle.is_empty(), Error::IntegrityFailed)?;
        Ok((key.public_key, key.handle))
    }

    fn delete_key(&self, handle: &[u8]) -> Result<()> {
        self.0
            .delete_signing(handle.to_vec())
            .map_err(platform_error)
    }

    fn authorize(&self, what: &str, biometry_only: bool) -> Result<Box<dyn Authorized>> {
        let token = self
            .0
            .authorize(what.into(), biometry_only)
            .map_err(platform_error)?;
        Ok(Box::new(Answer {
            platform: self.0.clone(),
            token,
        }))
    }
}

struct PreparedAnswer {
    platform: Arc<dyn Platform>,
    token: u64,
}

impl dmsg_client::secure::PreparedSignature for PreparedAnswer {
    fn sign(&self) -> Result<[u8; 64]> {
        self.platform
            .sign_prepared(self.token)
            .map_err(platform_error)?
            .try_into()
            .map_err(|_| Error::IntegrityFailed)
    }

    fn cancel(&self) {
        self.platform.release_prepared(self.token);
    }
}

impl Drop for PreparedAnswer {
    fn drop(&mut self) {
        self.platform.release_prepared(self.token);
    }
}

impl HardwareAnchor for Adapter {
    fn discard_local_key(&self, handle: &[u8]) -> Result<()> {
        self.0
            .delete_signing(handle.to_vec())
            .map_err(platform_error)
    }

    fn sign_message(&self, handle: &[u8], message: &[u8]) -> Result<[u8; 64]> {
        ensure(
            !message.is_empty() && message.len() <= 512,
            Error::IntegrityFailed,
        )?;
        self.0
            .sign_anchor(handle.into(), message.into())
            .map_err(platform_error)?
            .try_into()
            .map_err(|_| Error::IntegrityFailed)
    }

    fn create(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        let key = self.0.create_anchor().map_err(platform_error)?;
        ensure(!key.handle.is_empty(), Error::IntegrityFailed)?;
        Ok((key.handle, key.public_key))
    }

    fn sign(&self, handle: &[u8], message: &Hash) -> Result<[u8; 64]> {
        self.0
            .sign_anchor(handle.into(), message.as_slice().into())
            .map_err(platform_error)?
            .try_into()
            .map_err(|_| Error::IntegrityFailed)
    }
}

impl dmsg_client::wallet::cosign::CosignProvider for Adapter {
    fn name(&self) -> &'static str {
        if cfg!(target_os = "android") {
            "android-keystore"
        } else {
            "ios-secure-enclave"
        }
    }

    fn create(&self, _label: &str) -> Result<(Vec<u8>, String)> {
        let key = self.0.create_signing(false).map_err(platform_error)?;
        dmsg_protocol::approval_key_id(&key.public_key)?;
        Ok((key.public_key, dmsg_client::codec::b64(&key.handle)))
    }

    fn sign(
        &self,
        handle: String,
        messages: Vec<Hash>,
    ) -> dmsg_client::wallet::cosign::CosignFuture {
        let platform = self.0.clone();
        Box::pin(async move {
            ensure(
                !messages.is_empty() && messages.len() <= 64,
                Error::QuotaExceeded,
            )?;
            let handle = dmsg_client::codec::unb64(&handle)?;
            sign_wallet_messages(platform, handle, messages, cfg!(target_os = "android")).await
        })
    }

    fn batch_limit(&self) -> usize {
        64
    }
}

#[cfg(any(
    test,
    all(
        feature = "ui-fixtures",
        not(any(target_os = "android", target_os = "ios"))
    )
))]
pub(crate) mod tests {
    use super::*;

    #[derive(Default)]
    pub(crate) struct PasscodeFallback {
        questions: std::sync::atomic::AtomicU64,
        released: std::sync::atomic::AtomicU64,
        cancelled: std::sync::atomic::AtomicBool,
        blocked: Option<SigningGate>,
        released_signal: tokio::sync::Notify,
        next_prepared: std::sync::atomic::AtomicU64,
        prepared: std::sync::Mutex<std::collections::BTreeMap<u64, (Vec<u8>, bool)>>,
    }

    #[derive(Default)]
    struct SigningGate {
        entered: tokio::sync::Notify,
        resume: std::sync::Mutex<bool>,
        wake: std::sync::Condvar,
    }

    impl Platform for PasscodeFallback {
        fn prepare_signature(
            &self,
            _: String,
            _: bool,
            _: Vec<u8>,
            message: Vec<u8>,
        ) -> std::result::Result<u64, MobileError> {
            assert_eq!(message.len(), 32);
            let token = self
                .next_prepared
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 100;
            self.prepared
                .lock()
                .unwrap()
                .insert(token, (message, false));
            Ok(token)
        }

        fn sign_prepared(&self, token: u64) -> std::result::Result<Vec<u8>, MobileError> {
            let cancelled = || MobileError::Failure {
                code: "CANCELLED".into(),
            };
            {
                let mut prepared = self.prepared.lock().unwrap();
                let item = prepared.get_mut(&token).ok_or_else(cancelled)?;
                if item.1 {
                    return Err(cancelled());
                }
                item.1 = true;
            }
            self.questions
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if let Some(gate) = &self.blocked {
                gate.entered.notify_one();
                let mut resume = gate.resume.lock().unwrap();
                while !*resume {
                    resume = gate.wake.wait(resume).unwrap();
                }
            }
            if !self.prepared.lock().unwrap().contains_key(&token) {
                return Err(cancelled());
            }
            Ok(vec![1; 64])
        }

        fn release_prepared(&self, token: u64) {
            if self.prepared.lock().unwrap().remove(&token).is_some() {
                self.released
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                self.released_signal.notify_one();
            }
        }

        fn get(&self, _: String) -> std::result::Result<Option<Vec<u8>>, MobileError> {
            unreachable!()
        }

        fn set(&self, _: String, _: Vec<u8>) -> std::result::Result<(), MobileError> {
            unreachable!()
        }

        fn delete(&self, _: String) -> std::result::Result<(), MobileError> {
            unreachable!()
        }

        fn presence(&self) -> u8 {
            2
        }

        fn biometry_state(&self) -> Option<Vec<u8>> {
            None
        }

        fn supports_biometry_only(&self) -> bool {
            false
        }

        fn authorize_keys(
            &self,
            reason: String,
            only: bool,
            _: Vec<Vec<u8>>,
        ) -> std::result::Result<u64, MobileError> {
            self.authorize(reason, only)
        }

        fn create_secret(
            &self,
            biometry_only: bool,
        ) -> std::result::Result<HardwareSecret, MobileError> {
            if biometry_only {
                return Err(MobileError::Failure {
                    code: "UNAVAILABLE".into(),
                });
            }
            Ok(HardwareSecret {
                handle: vec![1],
                context: vec![2],
                secret: vec![3; 32],
            })
        }

        fn open_secret(
            &self,
            _: Vec<u8>,
            _: Vec<u8>,
            biometry_only: bool,
        ) -> std::result::Result<Vec<u8>, MobileError> {
            if biometry_only {
                return Err(MobileError::Failure {
                    code: "UNAVAILABLE".into(),
                });
            }
            Ok(vec![3; 32])
        }

        fn create_signing(&self, _: bool) -> std::result::Result<SigningKey, MobileError> {
            unreachable!()
        }

        fn delete_signing(&self, _: Vec<u8>) -> std::result::Result<(), MobileError> {
            Ok(())
        }
        fn authorize(&self, _: String, _: bool) -> std::result::Result<u64, MobileError> {
            self.questions
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.cancelled
                .store(false, std::sync::atomic::Ordering::SeqCst);
            Ok(7)
        }
        fn sign_authorized(
            &self,
            token: u64,
            _: Vec<u8>,
            message: Vec<u8>,
        ) -> std::result::Result<Vec<u8>, MobileError> {
            assert_eq!(token, 7);
            assert_eq!(message.len(), 32);
            if let Some(gate) = &self.blocked {
                gate.entered.notify_one();
                let mut resume = gate.resume.lock().unwrap();
                while !*resume {
                    resume = gate.wake.wait(resume).unwrap();
                }
            }
            if self.cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(MobileError::Failure {
                    code: "CANCELLED".into(),
                });
            }
            Ok(vec![1; 64])
        }
        fn release_authorization(&self, token: u64) {
            assert_eq!(token, 7);
            self.released
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.released_signal.notify_one();
        }

        fn create_anchor(&self) -> std::result::Result<SigningKey, MobileError> {
            unreachable!()
        }

        fn sign_anchor(&self, _: Vec<u8>, _: Vec<u8>) -> std::result::Result<Vec<u8>, MobileError> {
            unreachable!()
        }

        fn confirm(&self, _: String, _: String, _: String) -> std::result::Result<(), MobileError> {
            unreachable!()
        }

        fn cancel_presence(&self) {
            self.cancelled
                .store(true, std::sync::atomic::Ordering::SeqCst);
            let tokens: Vec<_> = self.prepared.lock().unwrap().keys().copied().collect();
            for token in tokens {
                self.release_prepared(token);
            }
        }
    }

    #[test]
    fn cancelling_a_prepared_worker_before_it_starts_cannot_prompt_or_cancel_another_operation() {
        use std::sync::atomic::Ordering::SeqCst;
        let platform = Arc::new(PasscodeFallback::default());
        let adapter = Adapter(platform.clone());
        let first = adapter
            .prepare_signature("attest", false, &[1], &Hash::new([2; 32]))
            .unwrap()
            .unwrap();
        let second = adapter
            .prepare_signature("account", false, &[3], &Hash::new([4; 32]))
            .unwrap()
            .unwrap();
        assert_eq!(platform.questions.load(SeqCst), 0);
        let owner = dmsg_client::secure::SigningCancellation::new(first.clone());
        drop(owner);
        assert_eq!(first.sign(), Err(Error::Forbidden));
        assert_eq!(platform.questions.load(SeqCst), 0);
        assert!(second.sign().is_ok());
        assert_eq!(second.sign(), Err(Error::Forbidden));
        assert_eq!(platform.questions.load(SeqCst), 1);
    }

    #[test]
    fn biometry_only_never_falls_back_to_device_passcode() {
        let adapter = Adapter(Arc::new(PasscodeFallback::default()));
        assert!(PresenceKey::create(&adapter, false).is_ok());
        assert!(adapter.open(&[1], &[2], false).is_ok());
        assert!(matches!(
            PresenceKey::create(&adapter, true),
            Err(Error::Unavailable(_))
        ));
        assert!(matches!(
            adapter.open(&[1], &[2], true),
            Err(Error::Unavailable(_))
        ));
        assert_eq!(adapter.biometry_state(), None);
    }

    #[test]
    fn one_answer_signs_only_until_cancelled_and_releases_on_drop() {
        use std::sync::atomic::Ordering::SeqCst;
        let platform = Arc::new(PasscodeFallback::default());
        let adapter = Adapter(platform.clone());
        let answer = adapter.authorize("account", false).unwrap();
        assert_eq!(answer.sign(&[1], &Hash::new([2; 32])).unwrap(), [1; 64]);
        assert_eq!(answer.sign(&[3], &Hash::new([4; 32])).unwrap(), [1; 64]);
        assert_eq!(platform.questions.load(SeqCst), 1);
        platform.cancel_presence();
        assert_eq!(
            answer.sign(&[1], &Hash::new([2; 32])).err(),
            Some(Error::Forbidden)
        );
        drop(answer);
        assert_eq!(platform.released.load(SeqCst), 1);
        let answer = adapter.authorize("account", false).unwrap();
        assert_eq!(platform.questions.load(SeqCst), 2);
        drop(answer);
        assert_eq!(platform.released.load(SeqCst), 2);
    }

    #[test]
    fn temporary_hardware_failure_is_not_network_or_key_loss() {
        assert_eq!(
            platform_error(MobileError::Failure {
                code: "HARDWARE_UNAVAILABLE".into()
            }),
            Error::Unavailable("hardware: unavailable".into())
        );
    }

    #[test]
    fn local_authentication_preserves_system_prompt_errors() {
        for code in [
            "BIOMETRIC_LOCKOUT",
            "BIOMETRIC_LOCKOUT_PERMANENT",
            "BIOMETRIC_NOT_ENROLLED",
            "DEVICE_CREDENTIAL_NOT_SET",
            "HARDWARE_SECURITY_UPDATE_REQUIRED",
            "AUTHENTICATION_TIMEOUT",
        ] {
            let error = platform_error(MobileError::Failure { code: code.into() });
            let MobileError::Failure { code: returned } = error.into();
            assert_eq!(returned, code);
        }
    }

    #[tokio::test]
    async fn cancelling_an_android_batch_cannot_open_the_next_prompt() {
        use std::sync::atomic::Ordering::SeqCst;
        let platform = Arc::new(PasscodeFallback {
            blocked: Some(SigningGate::default()),
            ..Default::default()
        });
        let task = tokio::spawn(sign_wallet_messages(
            platform.clone(),
            vec![1],
            vec![Hash::new([2; 32]), Hash::new([3; 32])],
            true,
        ));
        let gate = platform.blocked.as_ref().unwrap();
        gate.entered.notified().await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        platform.cancel_presence();
        *gate.resume.lock().unwrap() = true;
        gate.wake.notify_one();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            platform.released_signal.notified(),
        )
        .await
        .unwrap();
        assert_eq!(platform.questions.load(SeqCst), 1);
        assert_eq!(platform.released.load(SeqCst), 1);
    }
}
