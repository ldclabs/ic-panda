//! Keys in the Secure Enclave (macOS), through CryptoKit
//! (`swift/Enclave.swift`): the fast-unlock and device-bound keys,
//! team-wallet cosigner keys and approval keys. A key leaves the enclave
//! only as an opaque handle that just this Mac's enclave opens, so it needs
//! no keychain item and no entitlement, and every use of it asks for Touch
//! ID or the login password; a biometry-only key takes only the
//! fingerprints enrolled when it was made.

use crate::locale::tr;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use dmsg_client::crypto::Key;
use dmsg_client::secure::{ApprovalKeys, Authorized, Presence, PresenceKey};
use dmsg_client::wallet::cosign::{CosignFuture, CosignProvider};
use dmsg_types::{Error, Hash, Result};
use std::ffi::{CString, c_char, c_void};
use zeroize::Zeroizing;

unsafe extern "C" {
    fn dmsg_se_presence() -> i32;
    fn dmsg_se_confirm(reason: *const c_char) -> i32;
    fn dmsg_se_biometry_state(state: *mut u8) -> i32;
    fn dmsg_se_agreement_create(
        handle: *mut u8,
        capacity: isize,
        length: *mut isize,
        peer: *mut u8,
        secret: *mut u8,
        biometry_only: i32,
    ) -> i32;
    fn dmsg_se_agreement_open(
        handle: *const u8,
        length: isize,
        peer: *const u8,
        reason: *const c_char,
        secret: *mut u8,
        biometry_only: i32,
    ) -> i32;
    fn dmsg_se_signing_create(
        handle: *mut u8,
        capacity: isize,
        length: *mut isize,
        public_key: *mut u8,
        biometry_only: i32,
    ) -> i32;
    fn dmsg_se_authorize(
        reason: *const c_char,
        biometry_only: i32,
        answer: *mut *mut c_void,
    ) -> i32;
    fn dmsg_se_sign_with(
        answer: *mut c_void,
        handle: *const u8,
        length: isize,
        message: *const u8,
        signature: *mut u8,
    ) -> i32;
    fn dmsg_se_release(answer: *mut c_void);
    fn dmsg_se_sign_batch(
        handle: *const u8,
        length: isize,
        message: *const u8,
        message_count: isize,
        reason: *const c_char,
        signature: *mut u8,
    ) -> i32;
}

/// Room for a key's handle, a few hundred bytes in practice.
const HANDLE: usize = 4096;
/// An X9.63 uncompressed P-256 public key.
const POINT: usize = 65;

/// The Secure Enclave of this Mac.
pub struct SecureEnclave;

/// A result code of `swift/Enclave.swift`.
fn checked(code: i32) -> Result<()> {
    match code {
        0 => Ok(()),
        2 => Err(Error::Forbidden),
        3 => Err(Error::IntegrityFailed),
        4 => Err(Error::Unavailable(
            "secure enclave: Touch ID cannot be used now".into(),
        )),
        status if status < 0 => Err(Error::Unavailable(format!(
            "secure enclave: OSStatus {status}"
        ))),
        _ => Err(Error::Unavailable("secure enclave".into())),
    }
}

/// The text the system shows when it asks for the person.
fn reason(text: String) -> CString {
    CString::new(text).unwrap_or_default()
}

/// The system's question about approving a request of type `what`, or
/// about a purpose an approval key signs for.
fn question(what: &str) -> CString {
    reason(match what {
        "checkout" | "letter" | "handle" | "connect" | "wallet" | "payment" => {
            tr!("approve a payment in dMsg", "在 dMsg 中批准付款")
        }
        "device_revoke" => tr!("revoke a device in dMsg", "在 dMsg 中撤销设备"),
        "device_pair" => tr!("add a device to your dMsg account", "向 dMsg 账户添加设备"),
        "device_capabilities" | "device_control" => tr!(
            "change what a device may do in dMsg",
            "更改 dMsg 设备的权限"
        ),
        "account" | "account_control" => tr!(
            "change who controls your dMsg account",
            "更改 dMsg 账户的控制设置"
        ),
        "principal" | "principal_control" => tr!(
            "change your dMsg Agent principal",
            "更改 dMsg 的 Agent 主体"
        ),
        "approval_key" | "approval_key_retire" => tr!(
            "change the approval keys of your dMsg account",
            "更改 dMsg 账户的批准钥匙"
        ),
        "hardware_policy" => tr!(
            "change which dMsg approvals need an approval key",
            "更改哪些 dMsg 批准需要批准钥匙"
        ),
        "attest" | "attestation" => tr!("sign a statement with dMsg", "用 dMsg 签署声明"),
        _ => tr!("approve a dMsg request", "批准 dMsg 请求"),
    })
}

/// Wait for the person without holding up the daemon's other tasks.
fn asking<T>(ask: impl FnOnce() -> T) -> T {
    tokio::task::block_in_place(ask)
}

/// A new key's handle, written by `create` into a buffer of [`HANDLE`] bytes.
fn handle(create: impl FnOnce(*mut u8, isize, *mut isize) -> i32) -> Result<Vec<u8>> {
    let mut handle = vec![0u8; HANDLE];
    let mut length: isize = 0;
    checked(create(handle.as_mut_ptr(), HANDLE as isize, &mut length))?;
    handle.truncate(usize::try_from(length).map_err(|_| Error::IntegrityFailed)?);
    Ok(handle)
}

impl PresenceKey for SecureEnclave {
    fn presence(&self) -> Option<Presence> {
        match unsafe { dmsg_se_presence() } {
            2 => Some(Presence::Biometrics),
            1 => Some(Presence::Password),
            _ => None,
        }
    }

    fn create(&self, biometry_only: bool) -> Result<(Vec<u8>, Vec<u8>, Zeroizing<Key>)> {
        let mut peer = vec![0u8; POINT];
        let mut secret = Zeroizing::new([0u8; 32]);
        let handle = handle(|handle, capacity, length| unsafe {
            dmsg_se_agreement_create(
                handle,
                capacity,
                length,
                peer.as_mut_ptr(),
                secret.as_mut_ptr(),
                i32::from(biometry_only),
            )
        })?;
        Ok((handle, peer, secret))
    }

    fn open(&self, handle: &[u8], peer: &[u8], biometry_only: bool) -> Result<Zeroizing<Key>> {
        if peer.len() != POINT {
            return Err(Error::IntegrityFailed);
        }
        let reason = reason(tr!("unlock dMsg", "解锁 dMsg"));
        let mut secret = Zeroizing::new([0u8; 32]);
        checked(asking(|| unsafe {
            dmsg_se_agreement_open(
                handle.as_ptr(),
                handle.len() as isize,
                peer.as_ptr(),
                reason.as_ptr(),
                secret.as_mut_ptr(),
                i32::from(biometry_only),
            )
        }))?;
        Ok(secret)
    }

    fn biometry_state(&self) -> Option<Hash> {
        let mut state = [0u8; 32];
        (unsafe { dmsg_se_biometry_state(state.as_mut_ptr()) } == 0).then(|| Hash::new(state))
    }

    fn confirm(&self, what: &str) -> Result<()> {
        let reason = question(what);
        checked(asking(|| unsafe { dmsg_se_confirm(reason.as_ptr()) }))
    }
}

/// The person's answer to one question: an evaluated authentication
/// context the enclave signs with, withdrawn when dropped.
struct Answer(*mut c_void);

// The context is only handed back to CryptoKit, which may use it from any
// thread; nothing reads it here.
unsafe impl Send for Answer {}
unsafe impl Sync for Answer {}

impl Authorized for Answer {
    fn sign(&self, handle: &[u8], message: &Hash) -> Result<[u8; 64]> {
        let mut signature = [0u8; 64];
        checked(unsafe {
            dmsg_se_sign_with(
                self.0,
                handle.as_ptr(),
                handle.len() as isize,
                message.as_ptr(),
                signature.as_mut_ptr(),
            )
        })?;
        Ok(signature)
    }
}

impl Drop for Answer {
    fn drop(&mut self) {
        unsafe { dmsg_se_release(self.0) }
    }
}

impl ApprovalKeys for SecureEnclave {
    fn create(&self, biometry_only: bool) -> Result<(Vec<u8>, Vec<u8>)> {
        let mut point = [0u8; POINT];
        let handle = handle(|handle, capacity, length| unsafe {
            dmsg_se_signing_create(
                handle,
                capacity,
                length,
                point.as_mut_ptr(),
                i32::from(biometry_only),
            )
        })?;
        Ok((compressed(&point)?, handle))
    }

    fn authorize(&self, what: &str, biometry_only: bool) -> Result<Box<dyn Authorized>> {
        let reason = question(what);
        let mut answer: *mut c_void = std::ptr::null_mut();
        checked(asking(|| unsafe {
            dmsg_se_authorize(reason.as_ptr(), i32::from(biometry_only), &mut answer)
        }))?;
        if answer.is_null() {
            return Err(Error::Unavailable("secure enclave".into()));
        }
        Ok(Box::new(Answer(answer)))
    }
}

/// The SEC1 compressed form of an X9.63 uncompressed P-256 point.
fn compressed(point: &[u8; POINT]) -> Result<Vec<u8>> {
    if point[0] != 0x04 {
        return Err(Error::IntegrityFailed);
    }
    Ok([&[0x02 | (point[64] & 1)][..], &point[1..33]].concat())
}

impl CosignProvider for SecureEnclave {
    fn name(&self) -> &'static str {
        "secure-enclave"
    }

    fn create(&self, _label: &str) -> Result<(Vec<u8>, String)> {
        let mut point = [0u8; POINT];
        let handle = handle(|handle, capacity, length| unsafe {
            dmsg_se_signing_create(handle, capacity, length, point.as_mut_ptr(), 0)
        })?;
        Ok((compressed(&point)?, URL_SAFE_NO_PAD.encode(handle)))
    }

    fn sign(&self, handle: String, messages: Vec<Hash>) -> CosignFuture {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let handle = URL_SAFE_NO_PAD
                    .decode(handle)
                    .map_err(|_| Error::IntegrityFailed)?;
                let reason = reason(tr!("sign for a team wallet", "为团队钱包签名"));
                let message: Vec<u8> = messages.iter().flat_map(|m| m.iter().copied()).collect();
                let mut signatures = vec![0u8; 64 * messages.len()];
                let signed = checked(unsafe {
                    dmsg_se_sign_batch(
                        handle.as_ptr(),
                        handle.len() as isize,
                        message.as_ptr(),
                        messages.len() as isize,
                        reason.as_ptr(),
                        signatures.as_mut_ptr(),
                    )
                });
                match signed {
                    Ok(()) => Ok(signatures
                        .as_chunks::<64>()
                        .0
                        .iter()
                        .map(|s| s.to_vec())
                        .collect()),
                    Err(Error::Forbidden) => {
                        Err(Error::Unavailable("secure enclave: declined".into()))
                    }
                    Err(error) => Err(error),
                }
            })
            .await
            .map_err(|e| Error::Unavailable(format!("cosigning: {e}")))?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_status_failures_remain_unavailable_with_their_diagnostic_code() {
        // Interaction disallowed, authentication failure and invalid parameters
        // are system errors, distinct from an explicit user cancellation (2).
        for status in [-25308, -25293, -50] {
            assert_eq!(
                checked(status),
                Err(Error::Unavailable(format!(
                    "secure enclave: OSStatus {status}"
                )))
            );
        }
        assert_eq!(checked(2), Err(Error::Forbidden));
        assert_eq!(checked(3), Err(Error::IntegrityFailed));
        // A person who cannot be asked now is no lost key.
        assert!(matches!(checked(4), Err(Error::Unavailable(_))));
    }

    /// Creating keys asks nothing; using them asks for the person, which a
    /// test cannot answer.
    #[test]
    fn creates_keys_without_asking() {
        let enclave = SecureEnclave;
        if enclave.presence().is_none() {
            return;
        }
        let (handle, peer, secret) = PresenceKey::create(&enclave, false).unwrap();
        assert!(!handle.is_empty() && handle.len() < HANDLE);
        assert_eq!((peer.len(), peer[0]), (POINT, 0x04));
        assert_ne!(*secret, [0; 32]);
        let (public, handle) = CosignProvider::create(&enclave, "test").unwrap();
        assert_eq!(public.len(), 33);
        assert!(URL_SAFE_NO_PAD.decode(handle).is_ok());
        assert_eq!(
            enclave.open(&[1, 2, 3], &[0; 64], false).err(),
            Some(Error::IntegrityFailed)
        );
        let (public, handle) = ApprovalKeys::create(&enclave, false).unwrap();
        assert_eq!((public.len(), handle.is_empty()), (33, false));
        assert!(matches!(public[0], 2 | 3));
    }
}
