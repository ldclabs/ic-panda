//! Keys in the Secure Enclave (macOS), through CryptoKit
//! (`swift/Enclave.swift`): the fast-unlock key and team-wallet cosigner
//! keys. A key leaves the enclave only as an opaque handle that just this
//! Mac's enclave opens, so it needs no keychain item and no entitlement, and
//! every use of it asks for Touch ID or the login password.

use crate::locale::tr;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use dmsg_client::crypto::Key;
use dmsg_client::secure::{Presence, PresenceKey};
use dmsg_client::wallet::cosign::{CosignFuture, CosignProvider};
use dmsg_types::{Error, Hash, Result};
use std::ffi::{CString, c_char};
use zeroize::Zeroizing;

unsafe extern "C" {
    fn dmsg_se_presence() -> i32;
    fn dmsg_se_confirm(reason: *const c_char) -> i32;
    fn dmsg_se_agreement_create(
        handle: *mut u8,
        capacity: isize,
        length: *mut isize,
        peer: *mut u8,
        secret: *mut u8,
    ) -> i32;
    fn dmsg_se_agreement_open(
        handle: *const u8,
        length: isize,
        peer: *const u8,
        reason: *const c_char,
        secret: *mut u8,
    ) -> i32;
    fn dmsg_se_signing_create(
        handle: *mut u8,
        capacity: isize,
        length: *mut isize,
        public_key: *mut u8,
    ) -> i32;
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

    fn create(&self) -> Result<(Vec<u8>, Vec<u8>, Zeroizing<Key>)> {
        let mut peer = vec![0u8; POINT];
        let mut secret = Zeroizing::new([0u8; 32]);
        let handle = handle(|handle, capacity, length| unsafe {
            dmsg_se_agreement_create(
                handle,
                capacity,
                length,
                peer.as_mut_ptr(),
                secret.as_mut_ptr(),
            )
        })?;
        Ok((handle, peer, secret))
    }

    fn open(&self, handle: &[u8], peer: &[u8]) -> Result<Zeroizing<Key>> {
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
            )
        }))?;
        Ok(secret)
    }

    fn confirm(&self, what: &str) -> Result<()> {
        let reason = reason(match what {
            "checkout" | "letter" | "handle" | "connect" | "wallet" => {
                tr!("approve a payment in dMsg", "在 dMsg 中批准付款")
            }
            "device_revoke" => tr!("revoke a device in dMsg", "在 dMsg 中撤销设备"),
            "device_pair" => tr!("add a device to your dMsg account", "向 dMsg 账户添加设备"),
            "device_capabilities" => tr!(
                "change what a device may do in dMsg",
                "更改 dMsg 设备的权限"
            ),
            "account" => tr!(
                "change who controls your dMsg account",
                "更改 dMsg 账户的控制设置"
            ),
            "principal" => tr!(
                "change your dMsg Agent principal",
                "更改 dMsg 的 Agent 主体"
            ),
            _ => tr!("approve a dMsg request", "批准 dMsg 请求"),
        });
        checked(asking(|| unsafe { dmsg_se_confirm(reason.as_ptr()) }))
    }
}

impl CosignProvider for SecureEnclave {
    fn name(&self) -> &'static str {
        "secure-enclave"
    }

    fn create(&self, _label: &str) -> Result<(Vec<u8>, String)> {
        let mut point = [0u8; POINT];
        let handle = handle(|handle, capacity, length| unsafe {
            dmsg_se_signing_create(handle, capacity, length, point.as_mut_ptr())
        })?;
        if point[0] != 0x04 {
            return Err(Error::IntegrityFailed);
        }
        let compressed = [&[0x02 | (point[64] & 1)][..], &point[1..33]].concat();
        Ok((compressed, URL_SAFE_NO_PAD.encode(handle)))
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
    }

    /// Creating keys asks nothing; using them asks for the person, which a
    /// test cannot answer.
    #[test]
    fn creates_keys_without_asking() {
        let enclave = SecureEnclave;
        if enclave.presence().is_none() {
            return;
        }
        let (handle, peer, secret) = PresenceKey::create(&enclave).unwrap();
        assert!(!handle.is_empty() && handle.len() < HANDLE);
        assert_eq!((peer.len(), peer[0]), (POINT, 0x04));
        assert_ne!(*secret, [0; 32]);
        let (public, handle) = CosignProvider::create(&enclave, "test").unwrap();
        assert_eq!(public.len(), 33);
        assert!(URL_SAFE_NO_PAD.decode(handle).is_ok());
        assert_eq!(
            enclave.open(&[1, 2, 3], &[0; 64]).err(),
            Some(Error::IntegrityFailed)
        );
    }
}
