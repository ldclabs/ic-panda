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
use std::time::Duration;
use windows::Security::Credentials::{
    KeyCredential, KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus,
};
use windows::Security::Cryptography::CryptographicBuffer;
use windows::Win32::Foundation::{NTE_BAD_KEYSET, NTE_NOT_FOUND};
use windows::Win32::Security::Cryptography::{
    BCRYPT_ECCPUBLIC_BLOB, BCRYPT_ECDSA_P256_ALGORITHM, BCRYPT_ECDSA_PUBLIC_P256_MAGIC,
    CERT_KEY_SPEC, MS_PLATFORM_CRYPTO_PROVIDER, NCRYPT_FLAGS, NCRYPT_HANDLE, NCRYPT_KEY_HANDLE,
    NCRYPT_PROV_HANDLE, NCRYPT_SILENT_FLAG, NCryptCreatePersistedKey, NCryptDeleteKey,
    NCryptExportKey, NCryptFinalizeKey, NCryptFreeObject, NCryptIsAlgSupported, NCryptOpenKey,
    NCryptOpenStorageProvider, NCryptSignHash,
};
use windows::Win32::System::Com::CoIncrementMTAUsage;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow};
use windows::core::{Array, HSTRING, PCWSTR, w};
use zeroize::Zeroizing;

/// The prefix of an anchor's key name, which is its handle.
const ANCHOR: &str = "dmsg-anchor-";
/// The Windows Hello key every slot signs with, its handle.
const HELLO: &str = "dmsg-presence-v1";
/// The TPM key failed its integrity check: it was made by another TPM, or
/// before this one was cleared.
const TPM_20_E_INTEGRITY: i32 = 0x8028_009F_u32 as i32;

/// The TPM and Windows Hello of this PC.
pub struct WindowsHardware;

impl WindowsHardware {
    /// Both, when this PC has a TPM 2.0 that makes P-256 keys and the
    /// Windows user has set up Windows Hello; checked once, when the daemon
    /// starts, so that an anchor never stands without a presence key.
    pub fn detect() -> Option<Self> {
        // WinRT runs in the process's multithreaded apartment, kept for good.
        unsafe { CoIncrementMTAUsage() }.ok()?;
        let tpm = provider().is_ok_and(|p| unsafe {
            NCryptIsAlgSupported(p.provider(), BCRYPT_ECDSA_P256_ALGORITHM, 0).is_ok()
        });
        let hello = KeyCredentialManager::IsSupportedAsync()
            .and_then(|supported| supported.join())
            .unwrap_or(false);
        (tpm && hello).then_some(Self)
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
    .map_err(|error| match error.code() {
        NTE_BAD_KEYSET | NTE_NOT_FOUND => Error::IntegrityFailed,
        code if code.0 == TPM_20_E_INTEGRITY => Error::IntegrityFailed,
        _ => tpm(error),
    })?;
    Ok(Opened {
        key: Object(key.0),
        _provider: provider,
    })
}

/// A P-256 signature over a SHA-256 digest, as `r || s`.
fn sign_digest(handle: &[u8], digest: &[u8]) -> Result<[u8; 64]> {
    let anchor = open_anchor(handle)?;
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
    .map_err(|error| match error.code().0 {
        TPM_20_E_INTEGRITY => Error::IntegrityFailed,
        _ => tpm(error),
    })?;
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
        sign_digest(handle, &Sha256::digest(message))
    }

    fn create(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        let name = format!("{ANCHOR}{}", hex::encode(&random_id()[..16]));
        let provider = provider()?;
        let mut key = NCRYPT_KEY_HANDLE::default();
        unsafe {
            NCryptCreatePersistedKey(
                provider.provider(),
                &mut key,
                BCRYPT_ECDSA_P256_ALGORITHM,
                &HSTRING::from(&name),
                CERT_KEY_SPEC(0),
                NCRYPT_FLAGS(0),
            )
        }
        .map_err(tpm)?;
        let key = Object(key.0);
        unsafe { NCryptFinalizeKey(key.key(), NCRYPT_SILENT_FLAG) }.map_err(tpm)?;
        let public = public_key(&key);
        if public.is_err() && unsafe { NCryptDeleteKey(key.key(), 0) }.is_ok() {
            std::mem::forget(key);
        }
        Ok((name.into_bytes(), public?))
    }

    fn sign(&self, handle: &[u8], message: &Hash) -> Result<[u8; 64]> {
        sign_digest(handle, &Sha256::digest(message.as_slice()))
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

/// Wait for the person without holding up the daemon's other tasks, with
/// the system's question in front: the daemon has no window of its own, so
/// Windows may otherwise open it behind the app that asked.
fn asking<T>(ask: impl FnOnce() -> T) -> T {
    std::thread::spawn(|| {
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(250));
            if let Ok(window) =
                unsafe { FindWindowW(w!("Credential Dialog Xaml Host"), PCWSTR::null()) }
            {
                let _ = unsafe { SetForegroundWindow(window) };
                return;
            }
        }
    });
    tokio::task::block_in_place(ask)
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
    let made = asking(|| {
        KeyCredentialManager::RequestCreateAsync(&name, KeyCredentialCreationOption::FailIfExists)
            .and_then(|making| making.join())
    })
    .map_err(hello)?;
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
    let signed = asking(|| {
        credential
            .RequestSignAsync(&data)
            .and_then(|signing| signing.join())
    })
    .map_err(hello)?;
    match signed.Status().map_err(hello)? {
        KeyCredentialStatus::Success => {}
        status => return Err(declined(status)),
    }
    let mut signature = Array::<u8>::new();
    CryptographicBuffer::CopyToByteArray(&signed.Result().map_err(hello)?, &mut signature)
        .map_err(hello)?;
    let signature = Zeroizing::new(signature.to_vec());
    if signature.len() < 256 {
        return Err(Error::Unavailable("windows hello: signature".into()));
    }
    Ok(derive(&signature, &"dmsg/windows-hello/v1"))
}

impl PresenceKey for WindowsHardware {
    fn presence(&self) -> Option<Presence> {
        // Windows Hello: the face or a fingerprint, else the PIN.
        Some(Presence::Biometrics)
    }

    fn create(&self, biometry_only: bool) -> Result<(Vec<u8>, Vec<u8>, Zeroizing<Key>)> {
        if biometry_only {
            return Err(Error::Unavailable("hardware: biometry unavailable".into()));
        }
        let challenge = random_id();
        let secret = secret(&credential(true)?, &challenge)?;
        Ok((HELLO.as_bytes().to_vec(), challenge.to_vec(), secret))
    }

    fn open(&self, handle: &[u8], peer: &[u8], biometry_only: bool) -> Result<Zeroizing<Key>> {
        // Another platform's slot, or a biometry-only one, was not made here.
        if biometry_only || handle != HELLO.as_bytes() || peer.len() != 32 {
            return Err(Error::IntegrityFailed);
        }
        secret(&credential(false)?, peer)
    }

    fn biometry_state(&self) -> Option<Hash> {
        None
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
        let hardware = WindowsHardware;
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
        if WindowsHardware::detect().is_none() {
            return;
        }
        let hardware = WindowsHardware;
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
        HardwareAnchor::discard_local_key(&hardware, &handle).unwrap();
        assert_eq!(
            HardwareAnchor::sign(&hardware, &handle, &message).err(),
            Some(Error::IntegrityFailed)
        );
    }
}
