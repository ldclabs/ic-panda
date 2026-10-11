//! Offline COSE master-key pin, enabled by the `cose-pins` feature.
//!
//! ICP derives every vetKD key from the calling canister's ID, so a COSE pin
//! is known only after its canister is created. This helper derives the same
//! content-root public key `initialize_keys` fetches, from the master public
//! keys hardcoded in the DFINITY vetKeys crate. Clients encrypt root-bundle
//! recovery envelopes to this key.
use crate::{content_root_context, sha256};
use candid::Principal;
use dmsg_types::{cose::*, *};

/// Where the master public keys come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySource {
    /// ICP mainnet: `key_1` and `test_key_1`.
    Mainnet,
    /// PocketIC, including local dfx replicas: `key_1`, `test_key_1` and
    /// `dfx_test_key`.
    PocketIc,
}

/// The derived content-root public key of COSE canister `canister` for
/// [`content_root_context`] with derivation version 2.
///
/// # Errors
/// Returns `Error::InvalidInput` for a key name the source does not hold.
pub fn content_root_public_key(
    source: KeySource,
    canister: Principal,
    environment: &Environment,
    key_name: &str,
) -> Result<Vec<u8>> {
    let id = vetkd_key_id::VetKDKeyId {
        curve: vetkd_key_id::VetKDCurve::Bls12_381_G2,
        name: key_name.into(),
    };
    let master = match source {
        KeySource::Mainnet => ic_vetkeys::MasterPublicKey::for_mainnet_key(&id),
        KeySource::PocketIc => ic_vetkeys::MasterPublicKey::for_pocketic_key(&id),
    }
    .ok_or_else(|| Error::InvalidInput(format!("unknown {source:?} key {key_name}")))?;
    Ok(master
        .derive_canister_key(canister.as_slice())
        .derive_sub_key(&content_root_context(environment, 2))
        .serialize())
}

/// `MasterKey` record for COSE canister `canister` whose `expected_fingerprint`
/// is the SHA-256 that `initialize_keys` checks.
///
/// # Errors
/// Returns `Error::InvalidInput` for a key name the source does not hold.
pub fn master_key_pin(
    source: KeySource,
    canister: Principal,
    environment: &Environment,
    key_name: &str,
) -> Result<MasterKey> {
    Ok(MasterKey {
        key_name: key_name.into(),
        expected_fingerprint: sha256(&content_root_public_key(
            source,
            canister,
            environment,
            key_name,
        )?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_bind_canister_source_and_environment() {
        let canister = Principal::from_slice(&[0, 0, 0, 0, 1, 2, 3, 4, 1, 1]);
        let pin = |source, canister, environment| {
            master_key_pin(source, canister, &environment, "key_1").unwrap()
        };
        let production = pin(KeySource::Mainnet, canister, Environment::Production);
        assert_eq!(production.key_name, "key_1");
        assert_ne!(production.expected_fingerprint, Hash::new([0; 32]));
        let other = pin(
            KeySource::Mainnet,
            Principal::from_slice(&[0, 0, 0, 0, 1, 2, 3, 5, 1, 1]),
            Environment::Production,
        );
        let pocketic = pin(KeySource::PocketIc, canister, Environment::Production);
        let staging = pin(KeySource::Mainnet, canister, Environment::Staging);
        for differing in [other, pocketic, staging] {
            assert_ne!(differing, production);
        }
        assert_eq!(
            content_root_public_key(
                KeySource::Mainnet,
                canister,
                &Environment::Production,
                "key_1"
            )
            .unwrap()
            .len(),
            96
        );
        assert_eq!(
            master_key_pin(
                KeySource::Mainnet,
                canister,
                &Environment::Local,
                "dfx_test_key",
            ),
            Err(Error::InvalidInput(
                "unknown Mainnet key dfx_test_key".into()
            ))
        );
    }
}
