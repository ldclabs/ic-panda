//! One memory-hard derivation at a time in this host process.
use crate::crypto::Key;
use dmsg_types::{Error, Result};
use std::sync::Mutex;
use zeroize::Zeroizing;

static TURN: Mutex<()> = Mutex::new(());

pub(crate) fn derive(
    password: &[u8],
    salt: &[u8],
    memory: u32,
    time: u32,
    lanes: u32,
) -> Result<Zeroizing<Key>> {
    let _turn = TURN
        .try_lock()
        .map_err(|_| dmsg_types::invalid("KDF_BUSY"))?;
    let params =
        argon2::Params::new(memory, time, lanes, Some(32)).map_err(|_| Error::IntegrityFailed)?;
    let mut key = Zeroizing::new([0u8; 32]);
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
        .hash_password_into(password, salt, key.as_mut_slice())
        .map_err(|_| Error::IntegrityFailed)?;
    Ok(key)
}
