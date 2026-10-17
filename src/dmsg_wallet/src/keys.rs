//! Chain-key public keys and threshold signatures.
use crate::store::SignSpec;
use dmsg_protocol::wallet::KeyAlgorithm;
use dmsg_types::*;
use ic_cdk_management_canister::{
    Bip341, EcdsaCurve, EcdsaKeyId, EcdsaPublicKeyArgs, SchnorrAlgorithm, SchnorrAux, SchnorrKeyId,
    SchnorrPublicKeyArgs, SignWithEcdsaArgs, SignWithSchnorrArgs, ecdsa_public_key,
    schnorr_public_key, sign_with_ecdsa, sign_with_schnorr,
};
use serde_bytes::ByteBuf;

fn ecdsa_key(name: &str) -> EcdsaKeyId {
    EcdsaKeyId {
        curve: EcdsaCurve::Secp256k1,
        name: name.into(),
    }
}

fn schnorr_key(name: &str, algorithm: SchnorrAlgorithm) -> SchnorrKeyId {
    SchnorrKeyId {
        algorithm,
        name: name.into(),
    }
}

fn unavailable(e: impl std::fmt::Debug) -> Error {
    Error::Unavailable(format!("chain key: {e:?}"))
}

/// Public key and chain code at `path`.
pub(crate) async fn public_key(
    key_name: &str,
    algorithm: KeyAlgorithm,
    path: Vec<Vec<u8>>,
) -> Result<(Vec<u8>, Vec<u8>)> {
    match algorithm {
        KeyAlgorithm::Ecdsa => ecdsa_public_key(&EcdsaPublicKeyArgs {
            canister_id: None,
            derivation_path: path,
            key_id: ecdsa_key(key_name),
        })
        .await
        .map(|r| (r.public_key, r.chain_code))
        .map_err(unavailable),
        KeyAlgorithm::Bip340 | KeyAlgorithm::Ed25519 => schnorr_public_key(&SchnorrPublicKeyArgs {
            canister_id: None,
            derivation_path: path,
            key_id: schnorr_key(
                key_name,
                if algorithm == KeyAlgorithm::Bip340 {
                    SchnorrAlgorithm::Bip340secp256k1
                } else {
                    SchnorrAlgorithm::Ed25519
                },
            ),
        })
        .await
        .map(|r| (r.public_key, r.chain_code))
        .map_err(unavailable),
    }
}

pub(crate) fn path_bytes(path: &[Vec<u8>]) -> Vec<ByteBuf> {
    path.iter().cloned().map(ByteBuf::from).collect()
}

fn path_vec(path: &[ByteBuf]) -> Vec<Vec<u8>> {
    path.iter().map(|p| p.to_vec()).collect()
}

/// One threshold signature of `message`: 64-byte `r || s` for ECDSA,
/// BIP-340 or Ed25519.
pub(crate) async fn sign(key_name: &str, spec: &SignSpec, message: &[u8]) -> Result<Vec<u8>> {
    match spec {
        SignSpec::Ecdsa { path } => sign_with_ecdsa(&SignWithEcdsaArgs {
            message_hash: message.to_vec(),
            derivation_path: path_vec(path),
            key_id: ecdsa_key(key_name),
        })
        .await
        .map(|r| r.signature)
        .map_err(unavailable),
        SignSpec::Bip340 { path, tweak } => sign_with_schnorr(&SignWithSchnorrArgs {
            message: message.to_vec(),
            derivation_path: path_vec(path),
            key_id: schnorr_key(key_name, SchnorrAlgorithm::Bip340secp256k1),
            aux: tweak.as_ref().map(|root| {
                SchnorrAux::Bip341(Bip341 {
                    merkle_root_hash: root.to_vec(),
                })
            }),
        })
        .await
        .map(|r| r.signature)
        .map_err(unavailable),
        SignSpec::Ed25519 { path } => sign_with_schnorr(&SignWithSchnorrArgs {
            message: message.to_vec(),
            derivation_path: path_vec(path),
            key_id: schnorr_key(key_name, SchnorrAlgorithm::Ed25519),
            aux: None,
        })
        .await
        .map(|r| r.signature)
        .map_err(unavailable),
    }
}
