use crate::error::MobileError;
use dmsg_client::{
    crypto::Key,
    secure::{ApprovalKeys, Authorized, Presence, PresenceKey, SecureStorage},
};
use dmsg_types::{Error, Hash, Result, ensure};
use std::sync::Arc;
use zeroize::Zeroizing;

#[derive(uniffi::Record)]
pub struct Agreement {
    pub handle: Vec<u8>,
    pub peer: Vec<u8>,
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
    fn create_agreement(&self, biometry_only: bool) -> std::result::Result<Agreement, MobileError>;
    fn open_agreement(
        &self,
        handle: Vec<u8>,
        peer: Vec<u8>,
        biometry_only: bool,
    ) -> std::result::Result<Vec<u8>, MobileError>;
    fn create_signing(&self, biometry_only: bool) -> std::result::Result<SigningKey, MobileError>;
    /// A private, cancellable authorization, owned by one Rust decision.
    fn authorize(
        &self,
        reason: String,
        biometry_only: bool,
    ) -> std::result::Result<u64, MobileError>;
    fn sign_authorized(
        &self,
        authorization: u64,
        handle: Vec<u8>,
        message: Vec<u8>,
    ) -> std::result::Result<Vec<u8>, MobileError>;
    fn release_authorization(&self, authorization: u64);
    fn confirm(
        &self,
        request_id: String,
        digest: String,
        reason: String,
    ) -> std::result::Result<(), MobileError>;
    fn cancel_presence(&self);
}

pub struct Adapter(pub Arc<dyn Platform>);

pub fn platform_error(error: MobileError) -> Error {
    let MobileError::Failure { code } = error;
    match code.as_str() {
        "CANCELLED" | "FORBIDDEN" => Error::Forbidden,
        "INTEGRITY_FAILED" => Error::IntegrityFailed,
        "HARDWARE_UNAVAILABLE" => Error::Unavailable("hardware: unavailable".into()),
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
            .create_agreement(biometry_only)
            .map_err(platform_error)?;
        let secret = Zeroizing::new(created.secret);
        let key: Key = secret
            .as_slice()
            .try_into()
            .map_err(|_| Error::IntegrityFailed)?;
        Ok((created.handle, created.peer, Zeroizing::new(key)))
    }

    fn open(&self, handle: &[u8], peer: &[u8], biometry_only: bool) -> Result<Zeroizing<Key>> {
        let bytes = Zeroizing::new(
            self.0
                .open_agreement(handle.into(), peer.into(), biometry_only)
                .map_err(platform_error)?,
        );
        Ok(Zeroizing::new(
            bytes
                .as_slice()
                .try_into()
                .map_err(|_| Error::IntegrityFailed)?,
        ))
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
    fn available(&self) -> bool {
        self.0.presence() != 0
    }

    fn create(&self, biometry_only: bool) -> Result<(Vec<u8>, Vec<u8>)> {
        let key = self
            .0
            .create_signing(biometry_only)
            .map_err(platform_error)?;
        dmsg_protocol::approval_key_id(&key.public_key)?;
        ensure(!key.handle.is_empty(), Error::IntegrityFailed)?;
        Ok((key.public_key, key.handle))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct PasscodeFallback {
        questions: std::sync::atomic::AtomicU64,
        released: std::sync::atomic::AtomicU64,
        cancelled: std::sync::atomic::AtomicBool,
    }

    impl Platform for PasscodeFallback {
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

        fn create_agreement(
            &self,
            biometry_only: bool,
        ) -> std::result::Result<Agreement, MobileError> {
            if biometry_only {
                return Err(MobileError::Failure {
                    code: "UNAVAILABLE".into(),
                });
            }
            Ok(Agreement {
                handle: vec![1],
                peer: vec![2],
                secret: vec![3; 32],
            })
        }

        fn open_agreement(
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
        }

        fn confirm(&self, _: String, _: String, _: String) -> std::result::Result<(), MobileError> {
            unreachable!()
        }

        fn cancel_presence(&self) {
            self.cancelled
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
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
}
