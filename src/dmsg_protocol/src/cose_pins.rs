//! Offline COSE master-key pins, enabled by the `cose-pins` feature.
//!
//! ICP derives every threshold key from the calling canister's ID, so a COSE
//! pin is known only after its canister is created. These helpers derive the
//! same public keys `initialize_keys` fetches, from the master public keys
//! hardcoded in the DFINITY key crates.
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

/// `MasterKey` records for COSE canister `canister`, one per algorithm, whose
/// `expected_fingerprint` is the SHA-256 that `initialize_keys` checks: of the
/// raw Ed25519 key or compressed SEC1 secp256k1 key at the empty derivation
/// path, or of the vetKD public key for [`content_root_context`] with
/// derivation version 2.
///
/// # Errors
/// Returns `Error::InvalidInput` for a key name the source does not hold.
pub fn master_key_pins(
    source: KeySource,
    canister: Principal,
    environment: &Environment,
    key_name: &str,
    algorithms: &[Algorithm],
) -> Result<Vec<MasterKey>> {
    let unknown = || Error::InvalidInput(format!("unknown {source:?} key {key_name}"));
    algorithms
        .iter()
        .map(|algorithm| {
            let public_key = match algorithm {
                Algorithm::Ed25519 => {
                    use ic_ed25519::{MasterPublicKeyId as M, PocketIcMasterPublicKeyId as P};
                    let (key, _) = match (source, key_name) {
                        (KeySource::Mainnet, "key_1") => {
                            ic_ed25519::PublicKey::derive_mainnet_key(M::Key1, &canister, &[])
                        }
                        (KeySource::Mainnet, "test_key_1") => {
                            ic_ed25519::PublicKey::derive_mainnet_key(M::TestKey1, &canister, &[])
                        }
                        (KeySource::PocketIc, name) => {
                            let id = match name {
                                "key_1" => P::Key1,
                                "test_key_1" => P::TestKey1,
                                "dfx_test_key" => P::DfxTestKey,
                                _ => return Err(unknown()),
                            };
                            ic_ed25519::PublicKey::derive_pocketic_key(id, &canister, &[])
                        }
                        _ => return Err(unknown()),
                    };
                    key.serialize_raw().to_vec()
                }
                Algorithm::EcdsaSecp256k1 => {
                    use ic_secp256k1::{MasterPublicKeyId as M, PocketIcMasterPublicKeyId as P};
                    let (key, _) = match (source, key_name) {
                        (KeySource::Mainnet, "key_1") => {
                            ic_secp256k1::PublicKey::derive_mainnet_key(
                                M::EcdsaKey1,
                                &canister,
                                &[],
                            )
                        }
                        (KeySource::Mainnet, "test_key_1") => {
                            ic_secp256k1::PublicKey::derive_mainnet_key(
                                M::EcdsaTestKey1,
                                &canister,
                                &[],
                            )
                        }
                        (KeySource::PocketIc, name) => {
                            let id = match name {
                                "key_1" => P::EcdsaKey1,
                                "test_key_1" => P::EcdsaTestKey1,
                                "dfx_test_key" => P::EcdsaDfxTestKey,
                                _ => return Err(unknown()),
                            };
                            ic_secp256k1::PublicKey::derive_pocketic_key(id, &canister, &[])
                        }
                        _ => return Err(unknown()),
                    };
                    key.serialize_sec1(true)
                }
                Algorithm::VetKdBls12381 => {
                    let id = vetkd_key_id::VetKDKeyId {
                        curve: vetkd_key_id::VetKDCurve::Bls12_381_G2,
                        name: key_name.into(),
                    };
                    let master = match source {
                        KeySource::Mainnet => ic_vetkeys::MasterPublicKey::for_mainnet_key(&id),
                        KeySource::PocketIc => ic_vetkeys::MasterPublicKey::for_pocketic_key(&id),
                    }
                    .ok_or_else(unknown)?;
                    master
                        .derive_canister_key(canister.as_slice())
                        .derive_sub_key(&content_root_context(environment, 2))
                        .serialize()
                }
            };
            Ok(MasterKey {
                algorithm: algorithm.clone(),
                key_name: key_name.into(),
                expected_fingerprint: sha256(&public_key),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_bind_canister_source_and_content_root_environment() {
        let all = [
            Algorithm::Ed25519,
            Algorithm::EcdsaSecp256k1,
            Algorithm::VetKdBls12381,
        ];
        let canister = Principal::from_slice(&[0, 0, 0, 0, 1, 2, 3, 4, 1, 1]);
        let pins = |source, canister, environment| {
            master_key_pins(source, canister, &environment, "key_1", &all).unwrap()
        };
        let production = pins(KeySource::Mainnet, canister, Environment::Production);
        assert_eq!(production.len(), 3);
        assert!(production
            .iter()
            .all(|m| m.key_name == "key_1" && m.expected_fingerprint != Hash::new([0; 32])));
        let other = pins(
            KeySource::Mainnet,
            Principal::from_slice(&[0, 0, 0, 0, 1, 2, 3, 5, 1, 1]),
            Environment::Production,
        );
        let pocketic = pins(KeySource::PocketIc, canister, Environment::Production);
        for i in 0..3 {
            assert_ne!(production[i], other[i]);
            assert_ne!(production[i], pocketic[i]);
        }
        // Only the vetKD context depends on the environment.
        let staging = pins(KeySource::Mainnet, canister, Environment::Staging);
        assert_eq!(staging[..2], production[..2]);
        assert_ne!(staging[2], production[2]);
        assert_eq!(
            master_key_pins(
                KeySource::Mainnet,
                canister,
                &Environment::Local,
                "dfx_test_key",
                &all[..1],
            ),
            Err(Error::InvalidInput(
                "unknown Mainnet key dfx_test_key".into()
            ))
        );
    }
}
