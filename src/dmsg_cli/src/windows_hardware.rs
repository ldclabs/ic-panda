//! Keys of the TPM and Windows Hello (Windows). The workspaces' anchors and
//! the Passkey login keys are P-256 keys of the Microsoft Platform Crypto
//! Provider: the TPM keeps them, never exports them and signs without
//! asking. The fast-unlock and device-bound keys share one Windows Hello key
//! of the Windows user; making it and every use of it ask for the face,
//! fingerprint or PIN. Each slot's secret comes from Windows Hello's
//! signature over the slot's random challenge, which RSASSA-PKCS1-v1_5 makes
//! the same every time; a new binding checks that before anything depends on
//! it. Windows Hello always accepts the PIN, so it makes no biometry-only
//! keys, and approval keys and wallet cosigners do not come from here.

use dmsg_client::crypto::{Key, derive, random_id};
use dmsg_client::secure::{HardwareAnchor, Presence, PresenceKey};
use dmsg_types::{Error, Hash, Result};
use sha2::{Digest, Sha256};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use windows::Security::Credentials::{
    KeyCredential, KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus,
};
use windows::Security::Cryptography::CryptographicBuffer;
use windows::Win32::Foundation::{NTE_BAD_DATA, NTE_BAD_KEYSET, NTE_NOT_FOUND, TPM_20_E_INTEGRITY};
use windows::Win32::Security::Cryptography::{
    BCRYPT_ECCPUBLIC_BLOB, BCRYPT_ECDSA_P256_ALGORITHM, BCRYPT_ECDSA_PUBLIC_P256_MAGIC,
    BCRYPT_OPAQUE_KEY_BLOB, CERT_KEY_SPEC, MS_PLATFORM_CRYPTO_PROVIDER, NCRYPT_FLAGS,
    NCRYPT_HANDLE, NCRYPT_KEY_HANDLE, NCRYPT_PROV_HANDLE, NCRYPT_SILENT_FLAG,
    NCryptCreatePersistedKey, NCryptDeleteKey, NCryptExportKey, NCryptFinalizeKey,
    NCryptFreeObject, NCryptImportKey, NCryptIsAlgSupported, NCryptOpenKey,
    NCryptOpenStorageProvider, NCryptSignHash,
};
use windows::Win32::System::Com::CoIncrementMTAUsage;
use windows::Win32::System::WinRT::IBufferByteAccess;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow};
use windows::core::{Array, HSTRING, Interface, PCWSTR, RuntimeType, w};
use windows_future::IAsyncOperation;
use zeroize::{Zeroize, Zeroizing};

/// The prefix of an anchor's key name, which is its handle.
const ANCHOR: &str = "dmsg-anchor-";
/// The Windows Hello key every slot signs with, its handle.
const HELLO: &str = "dmsg-presence-v1";
/// Identifies a login key wrapped by the local TPM.
const LOGIN: &[u8] = b"dmsg-tpm-login-v1\0";

/// The TPM and Windows Hello of this PC.
pub struct WindowsHardware {
    can_enroll: bool,
}

impl WindowsHardware {
    /// Both, when this PC has a TPM 2.0 that makes P-256 keys and the
    /// Windows user has set up Windows Hello; checked once, when the daemon
    /// starts, so that an anchor never stands without a presence key.
    pub fn detect(system_storage: bool) -> Self {
        // WinRT runs in the process's multithreaded apartment, kept for good.
        let tpm = system_storage
            && provider().is_ok_and(|p| unsafe {
                NCryptIsAlgSupported(p.provider(), BCRYPT_ECDSA_P256_ALGORITHM, 0).is_ok()
            });
        let can_enroll = tpm
            && unsafe { CoIncrementMTAUsage() }.is_ok()
            && KeyCredentialManager::IsSupportedAsync()
                .and_then(|supported| supported.join())
                .unwrap_or(false);
        Self { can_enroll }
    }

    pub fn can_enroll(&self) -> bool {
        self.can_enroll
    }
}

/// An NCrypt provider or key, freed when dropped.
struct Object(usize);

impl Object {
    fn provider(&self) -> NCRYPT_PROV_HANDLE {
        NCRYPT_PROV_HANDLE(self.0)
    }

    fn key(&self) -> NCRYPT_KEY_HANDLE {
        NCRYPT_KEY_HANDLE(self.0)
    }
}

impl Drop for Object {
    fn drop(&mut self) {
        let _ = unsafe { NCryptFreeObject(NCRYPT_HANDLE(self.0)) };
    }
}

fn tpm(error: windows::core::Error) -> Error {
    Error::Unavailable(format!("tpm: {:#010x}", error.code().0))
}

fn key_error(error: windows::core::Error) -> Error {
    match error.code() {
        NTE_BAD_KEYSET | NTE_NOT_FOUND | NTE_BAD_DATA | TPM_20_E_INTEGRITY => {
            Error::IntegrityFailed
        }
        _ => tpm(error),
    }
}

fn provider() -> Result<Object> {
    let mut provider = NCRYPT_PROV_HANDLE::default();
    unsafe { NCryptOpenStorageProvider(&mut provider, MS_PLATFORM_CRYPTO_PROVIDER, 0) }
        .map_err(tpm)?;
    Ok(Object(provider.0))
}

/// The anchor a handle names; another platform's handle is unusable here.
fn anchor_name(handle: &[u8]) -> Result<HSTRING> {
    std::str::from_utf8(handle)
        .ok()
        .and_then(|name| name.strip_prefix(ANCHOR))
        .filter(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(|id| HSTRING::from(format!("{ANCHOR}{id}")))
        .ok_or(Error::IntegrityFailed)
}

/// A key and the provider that opened it, freed in that order.
struct Opened {
    key: Object,
    _provider: Object,
}

/// Open an anchor; a key this TPM does not have, or no longer opens, is
/// `Error::IntegrityFailed`.
fn open_anchor(handle: &[u8]) -> Result<Opened> {
    let name = anchor_name(handle)?;
    let provider = provider()?;
    let mut key = NCRYPT_KEY_HANDLE::default();
    unsafe {
        NCryptOpenKey(
            provider.provider(),
            &mut key,
            &name,
            CERT_KEY_SPEC(0),
            NCRYPT_SILENT_FLAG,
        )
    }
    .map_err(key_error)?;
    Ok(Opened {
        key: Object(key.0),
        _provider: provider,
    })
}

/// A P-256 signature over a SHA-256 digest, as `r || s`.
fn sign_digest(anchor: &Opened, digest: &[u8]) -> Result<[u8; 64]> {
    let mut signature = [0u8; 64];
    let mut length = 0u32;
    unsafe {
        NCryptSignHash(
            anchor.key.key(),
            None,
            digest,
            Some(&mut signature),
            &mut length,
            NCRYPT_SILENT_FLAG,
        )
    }
    .map_err(key_error)?;
    if length != 64 {
        return Err(Error::Unavailable("tpm: signature".into()));
    }
    Ok(signature)
}

/// The SEC1 compressed public key of a P-256 key.
fn public_key(key: &Object) -> Result<Vec<u8>> {
    // BCRYPT_ECCKEY_BLOB: magic and key size, then X and Y.
    let mut blob = [0u8; 8 + 64];
    let mut length = 0u32;
    unsafe {
        NCryptExportKey(
            key.key(),
            None,
            BCRYPT_ECCPUBLIC_BLOB,
            None,
            Some(&mut blob),
            &mut length,
            NCRYPT_FLAGS(0),
        )
    }
    .map_err(tpm)?;
    let magic = u32::from_le_bytes(blob[..4].try_into().expect("4 bytes"));
    let size = u32::from_le_bytes(blob[4..8].try_into().expect("4 bytes"));
    if length != 72 || magic != BCRYPT_ECDSA_PUBLIC_P256_MAGIC || size != 32 {
        return Err(Error::Unavailable("tpm: public key".into()));
    }
    Ok(compressed(&blob[8..40], &blob[40..72]))
}

/// The SEC1 compressed form of a P-256 point.
fn compressed(x: &[u8], y: &[u8]) -> Vec<u8> {
    [&[0x02 | (y[31] & 1)][..], x].concat()
}

impl HardwareAnchor for WindowsHardware {
    fn discard_local_key(&self, handle: &[u8]) -> Result<()> {
        let Opened { key, _provider } = match open_anchor(handle) {
            Ok(anchor) => anchor,
            Err(Error::IntegrityFailed) => return Ok(()),
            Err(error) => return Err(error),
        };
        unsafe { NCryptDeleteKey(key.key(), 0) }.map_err(tpm)?;
        // A deleted key's handle is freed with it.
        std::mem::forget(key);
        Ok(())
    }

    fn sign_message(&self, handle: &[u8], message: &[u8]) -> Result<[u8; 64]> {
        if message.is_empty() || message.len() > 512 {
            return Err(Error::IntegrityFailed);
        }
        let key = if let Some(blob) = handle.strip_prefix(LOGIN) {
            let provider = provider()?;
            let mut key = NCRYPT_KEY_HANDLE::default();
            unsafe {
                NCryptImportKey(
                    provider.provider(),
                    None,
                    BCRYPT_OPAQUE_KEY_BLOB,
                    None,
                    &mut key,
                    blob,
                    NCRYPT_SILENT_FLAG,
                )
            }
            .map_err(key_error)?;
            Opened {
                key: Object(key.0),
                _provider: provider,
            }
        } else {
            // Logins created by the original Windows adapter have a named key.
            open_anchor(handle)?
        };
        sign_digest(&key, &Sha256::digest(message))
    }

    fn create(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        let name = format!("{ANCHOR}{}", hex::encode(&random_id()[..16]));
        let wide = HSTRING::from(&name);
        let opened = self.create_key(PCWSTR(wide.as_ptr()))?;
        let public = public_key(&opened.key);
        if public.is_err() && unsafe { NCryptDeleteKey(opened.key.key(), 0) }.is_ok() {
            std::mem::forget(opened.key);
        }
        Ok((name.into_bytes(), public?))
    }

    fn sign(&self, handle: &[u8], message: &Hash) -> Result<[u8; 64]> {
        sign_digest(&open_anchor(handle)?, &Sha256::digest(message.as_slice()))
    }

    fn create_login_key(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        // Login attempts can be cancelled before they reach a workspace. An
        // unnamed key leaves no independently usable key in the user's KSP.
        let opened = self.create_key(PCWSTR::null())?;
        let public = public_key(&opened.key)?;
        let mut len = 0;
        unsafe {
            NCryptExportKey(
                opened.key.key(),
                None,
                BCRYPT_OPAQUE_KEY_BLOB,
                None,
                None,
                &mut len,
                NCRYPT_FLAGS(0),
            )
        }
        .map_err(tpm)?;
        let mut handle = LOGIN.to_vec();
        handle.resize(LOGIN.len() + len as usize, 0);
        unsafe {
            NCryptExportKey(
                opened.key.key(),
                None,
                BCRYPT_OPAQUE_KEY_BLOB,
                None,
                Some(&mut handle[LOGIN.len()..]),
                &mut len,
                NCRYPT_FLAGS(0),
            )
        }
        .map_err(tpm)?;
        handle.truncate(LOGIN.len() + len as usize);
        Ok((handle, public))
    }
}

impl WindowsHardware {
    fn create_key(&self, name: PCWSTR) -> Result<Opened> {
        if !self.can_enroll {
            return Err(Error::Unavailable(
                "hardware: TPM and Windows Hello required".into(),
            ));
        }
        let provider = provider()?;
        let mut key = NCRYPT_KEY_HANDLE::default();
        unsafe {
            NCryptCreatePersistedKey(
                provider.provider(),
                &mut key,
                BCRYPT_ECDSA_P256_ALGORITHM,
                name,
                CERT_KEY_SPEC(0),
                NCRYPT_FLAGS(0),
            )
        }
        .map_err(tpm)?;
        let key = Object(key.0);
        unsafe { NCryptFinalizeKey(key.key(), NCRYPT_SILENT_FLAG) }.map_err(tpm)?;
        Ok(Opened {
            key,
            _provider: provider,
        })
    }
}

fn hello(error: windows::core::Error) -> Error {
    Error::Unavailable(format!("windows hello: {:#010x}", error.code().0))
}

/// What a Windows Hello status other than success means here.
fn declined(status: KeyCredentialStatus) -> Error {
    match status {
        KeyCredentialStatus::UserCanceled | KeyCredentialStatus::UserPrefersPassword => {
            Error::Forbidden
        }
        // The Windows user reset Windows Hello, which deletes its keys.
        KeyCredentialStatus::NotFound => Error::IntegrityFailed,
        KeyCredentialStatus::SecurityDeviceLocked => {
            Error::Unavailable("windows hello: locked out for now".into())
        }
        status => Error::Unavailable(format!("windows hello: status {}", status.0)),
    }
}

/// The daemon has no window of its own. Look for the prompt only while the
/// operation is pending, and cancel an unanswered prompt after one minute.
fn asking<T: RuntimeType + Send + 'static>(
    operation: windows::core::Result<IAsyncOperation<T>>,
) -> Result<T> {
    let operation = operation.map_err(hello)?;
    let (send, receive) = mpsc::channel();
    operation
        .when(move |result| {
            let _ = send.send(result);
        })
        .map_err(hello)?;
    tokio::task::block_in_place(|| {
        let deadline = Instant::now() + Duration::from_secs(60);
        for _ in 0..20 {
            match receive.recv_timeout(Duration::from_millis(250)) {
                Ok(result) => return result.map_err(hello),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if let Ok(window) =
                unsafe { FindWindowW(w!("Credential Dialog Xaml Host"), PCWSTR::null()) }
            {
                let _ = unsafe { SetForegroundWindow(window) };
                break;
            }
        }
        match receive.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(result) => result.map_err(hello),
            Err(_) => {
                let _ = operation.Cancel();
                Err(Error::Unavailable(
                    "windows hello: request timed out".into(),
                ))
            }
        }
    })
}

/// The Windows Hello key; made, asking for the person, only when `make`.
fn credential(make: bool) -> Result<KeyCredential> {
    let name = HSTRING::from(HELLO);
    let opened = KeyCredentialManager::OpenAsync(&name)
        .and_then(|opening| opening.join())
        .map_err(hello)?;
    match opened.Status().map_err(hello)? {
        KeyCredentialStatus::Success => return opened.Credential().map_err(hello),
        KeyCredentialStatus::NotFound if make => {}
        status => return Err(declined(status)),
    }
    let made = asking(KeyCredentialManager::RequestCreateAsync(
        &name,
        KeyCredentialCreationOption::FailIfExists,
    ))?;
    match made.Status().map_err(hello)? {
        KeyCredentialStatus::Success => made.Credential().map_err(hello),
        // Another process made it first.
        KeyCredentialStatus::CredentialAlreadyExists => credential(false),
        status => Err(declined(status)),
    }
}

/// The secret of `challenge`, from Windows Hello's signature over it.
fn secret(credential: &KeyCredential, challenge: &[u8]) -> Result<Zeroizing<Key>> {
    let data = CryptographicBuffer::CreateFromByteArray(challenge).map_err(hello)?;
    let signed = asking(credential.RequestSignAsync(&data))?;
    match signed.Status().map_err(hello)? {
        KeyCredentialStatus::Success => {}
        status => return Err(declined(status)),
    }
    let buffer = signed.Result().map_err(hello)?;
    let len = buffer.Length().map_err(hello)? as usize;
    let access: IBufferByteAccess = buffer.cast().map_err(hello)?;
    let data = unsafe { access.Buffer() }.map_err(hello)?;
    if data.is_null() || len == 0 {
        return Err(Error::Unavailable("windows hello: empty signature".into()));
    }
    // The operation has completed and its result stays on this thread. The
    // buffer owns these initialized bytes until after we derive and erase them.
    let signature = unsafe { std::slice::from_raw_parts_mut(data, len) };
    let result = if len >= 256 {
        Ok(derive(signature, &"dmsg/windows-hello/v1"))
    } else {
        Err(Error::Unavailable("windows hello: signature".into()))
    };
    signature.zeroize();
    result
}

/// A reset can recreate the shared name for a different key. Identify the
/// public key before accepting a signature for an older slot.
fn hello_handle(credential: &KeyCredential) -> Result<Vec<u8>> {
    let public = credential
        .RetrievePublicKeyWithDefaultBlobType()
        .map_err(hello)?;
    let mut bytes = Array::new();
    CryptographicBuffer::CopyToByteArray(&public, &mut bytes).map_err(hello)?;
    Ok([HELLO.as_bytes(), &Sha256::digest(bytes.as_slice())].concat())
}

impl PresenceKey for WindowsHardware {
    fn presence(&self) -> Option<Presence> {
        // Windows Hello: the face or a fingerprint, else the PIN.
        self.can_enroll.then_some(Presence::Biometrics)
    }

    fn create(&self, biometry_only: bool) -> Result<(Vec<u8>, Vec<u8>, Zeroizing<Key>)> {
        if biometry_only {
            return Err(Error::Unavailable("hardware: biometry unavailable".into()));
        }
        let challenge = random_id();
        let key = credential(true)?;
        let handle = hello_handle(&key)?;
        let secret = secret(&key, &challenge)?;
        Ok((handle, challenge.to_vec(), secret))
    }

    fn open(&self, handle: &[u8], peer: &[u8], biometry_only: bool) -> Result<Zeroizing<Key>> {
        // Another platform's slot, or a biometry-only one, was not made here.
        let expected = handle
            .strip_prefix(HELLO.as_bytes())
            .ok_or(Error::IntegrityFailed)?;
        if biometry_only || !matches!(expected.len(), 0 | 32) || peer.len() != 32 {
            return Err(Error::IntegrityFailed);
        }
        let key = credential(false)?;
        // Original slots stored only the name. Their workspace wrapping
        // continues to authenticate the resulting secret.
        if !expected.is_empty() && handle != hello_handle(&key)? {
            return Err(Error::IntegrityFailed);
        }
        secret(&key, peer)
    }

    fn biometry_state(&self) -> Option<Hash> {
        None
    }

    fn creates_without_prompt(&self) -> bool {
        false
    }

    fn confirm(&self, _what: &str) -> Result<()> {
        // A Windows Hello signature over nothing in particular: the system's
        // question names no request, the approval window does.
        secret(&credential(true)?, &random_id()).map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_from_other_hardware_are_unusable() {
        let mac = [0x30, 0x81, 0x87, 0x02, 0x01];
        assert_eq!(anchor_name(&mac).err(), Some(Error::IntegrityFailed));
        assert_eq!(
            anchor_name(b"dmsg-anchor-zz").err(),
            Some(Error::IntegrityFailed)
        );
        let name = format!("{ANCHOR}{}", "0a".repeat(16));
        assert_eq!(anchor_name(name.as_bytes()).unwrap(), HSTRING::from(&name));
        let hardware = WindowsHardware { can_enroll: false };
        assert!(hardware.presence().is_none());
        assert!(HardwareAnchor::create(&hardware).is_err());
        assert!(hardware.create_login_key().is_err());
        assert!(!hardware.creates_without_prompt());
        assert_eq!(
            hardware.open(&mac, &[0; 32], false).err(),
            Some(Error::IntegrityFailed)
        );
        assert_eq!(
            hardware.open(HELLO.as_bytes(), &[0; 32], true).err(),
            Some(Error::IntegrityFailed)
        );
        assert!(matches!(
            PresenceKey::create(&hardware, true),
            Err(Error::Unavailable(_))
        ));
    }

    /// An anchor asks nothing to make or to use; this needs a TPM 2.0.
    #[test]
    fn anchors_sign_without_asking() {
        let hardware = WindowsHardware::detect(true);
        if !hardware.can_enroll() {
            return;
        }
        let (handle, public) = HardwareAnchor::create(&hardware).unwrap();
        assert_eq!(public.len(), 33);
        let message = Hash::new([7; 32]);
        use p256::ecdsa::signature::Verifier;
        let signature = HardwareAnchor::sign(&hardware, &handle, &message).unwrap();
        p256::ecdsa::VerifyingKey::from_sec1_bytes(&public)
            .unwrap()
            .verify(
                message.as_slice(),
                &p256::ecdsa::Signature::from_slice(&signature).unwrap(),
            )
            .unwrap();
        // Removing Hello does not disable verification of an existing TPM anchor.
        let disabled = WindowsHardware { can_enroll: false };
        assert!(HardwareAnchor::sign(&disabled, &handle, &message).is_ok());
        HardwareAnchor::discard_local_key(&hardware, &handle).unwrap();
        assert_eq!(
            HardwareAnchor::sign(&hardware, &handle, &message).err(),
            Some(Error::IntegrityFailed)
        );
        let (handle, public) = hardware.create_login_key().unwrap();
        assert!(handle.starts_with(LOGIN));
        let signed = hardware.sign_message(&handle, b"login delegation").unwrap();
        p256::ecdsa::VerifyingKey::from_sec1_bytes(&public)
            .unwrap()
            .verify(
                b"login delegation",
                &p256::ecdsa::Signature::from_slice(&signed).unwrap(),
            )
            .unwrap();
    }
}
