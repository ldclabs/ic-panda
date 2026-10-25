use crate::error::MobileError;
use dmsg_client::{
    crypto::Key,
    secure::{Presence, PresenceKey, SecureStorage},
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

/// Private platform adapter. User-facing APIs never accept a confirmation flag.
#[uniffi::export(callback_interface)]
pub trait Platform: Send + Sync {
    fn get(&self, name: String) -> std::result::Result<Option<Vec<u8>>, MobileError>;
    fn set(&self, name: String, secret: Vec<u8>) -> std::result::Result<(), MobileError>;
    fn delete(&self, name: String) -> std::result::Result<(), MobileError>;
    /// 0 unavailable, 1 device passcode, 2 biometrics with passcode fallback.
    fn presence(&self) -> u8;
    fn create_agreement(&self) -> std::result::Result<Agreement, MobileError>;
    fn open_agreement(
        &self,
        handle: Vec<u8>,
        peer: Vec<u8>,
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

pub fn platform_error(error: MobileError) -> Error {
    let MobileError::Failure { code } = error;
    match code.as_str() {
        "CANCELLED" | "FORBIDDEN" => Error::Forbidden,
        "INTEGRITY_FAILED" => Error::IntegrityFailed,
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
        // The current Swift adapter allows device-passcode fallback. Never
        // silently use that weaker policy for a biometry-only key.
        ensure(
            !biometry_only,
            Error::Unavailable("biometry-only keys are not supported on mobile yet".into()),
        )?;
        let created = self.0.create_agreement().map_err(platform_error)?;
        let secret = Zeroizing::new(created.secret);
        let key: Key = secret
            .as_slice()
            .try_into()
            .map_err(|_| Error::IntegrityFailed)?;
        Ok((created.handle, created.peer, Zeroizing::new(key)))
    }

    fn open(&self, handle: &[u8], peer: &[u8], biometry_only: bool) -> Result<Zeroizing<Key>> {
        ensure(
            !biometry_only,
            Error::Unavailable("biometry-only keys are not supported on mobile yet".into()),
        )?;
        let bytes = Zeroizing::new(
            self.0
                .open_agreement(handle.into(), peer.into())
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
        None
    }

    fn confirm(&self, _what: &str) -> Result<()> {
        // Approval confirmation must include its ID and displayed digest.
        Err(Error::Forbidden)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PasscodeFallback;

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

        fn create_agreement(&self) -> std::result::Result<Agreement, MobileError> {
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
        ) -> std::result::Result<Vec<u8>, MobileError> {
            Ok(vec![3; 32])
        }

        fn confirm(&self, _: String, _: String, _: String) -> std::result::Result<(), MobileError> {
            unreachable!()
        }

        fn cancel_presence(&self) {}
    }

    #[test]
    fn biometry_only_never_falls_back_to_device_passcode() {
        let adapter = Adapter(Arc::new(PasscodeFallback));
        assert!(adapter.create(false).is_ok());
        assert!(adapter.open(&[1], &[2], false).is_ok());
        assert!(matches!(adapter.create(true), Err(Error::Unavailable(_))));
        assert!(matches!(
            adapter.open(&[1], &[2], true),
            Err(Error::Unavailable(_))
        ));
        assert_eq!(adapter.biometry_state(), None);
    }
}
