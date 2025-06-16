use candid::{CandidType, Principal};
use ic_cdk::management_canister as mgt;
use ic_cose_types::{
    format_error,
    types::{PublicKeyOutput, SchnorrAlgorithm},
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

pub fn derive_25519_public_key(
    public_key: &PublicKeyOutput,
    derivation_path: Vec<Vec<u8>>,
) -> Result<PublicKeyOutput, String> {
    let path = ic_ed25519::DerivationPath::new(
        derivation_path
            .into_iter()
            .map(ic_ed25519::DerivationIndex)
            .collect(),
    );

    let chain_code: [u8; 32] = public_key
        .chain_code
        .to_vec()
        .try_into()
        .map_err(format_error)?;
    let pk =
        ic_ed25519::PublicKey::deserialize_raw(&public_key.public_key).map_err(format_error)?;
    let (derived_public_key, derived_chain_code) =
        pk.derive_subkey_with_chain_code(&path, &chain_code);

    Ok(PublicKeyOutput {
        public_key: ByteBuf::from(derived_public_key.serialize_raw()),
        chain_code: ByteBuf::from(derived_chain_code),
    })
}

#[derive(CandidType, Deserialize, Serialize, Debug)]
pub struct SignWithSchnorrArgs {
    pub message: Vec<u8>,
    pub derivation_path: Vec<Vec<u8>>,
    pub key_id: SchnorrKeyId,
}

#[derive(CandidType, Deserialize, Serialize, Debug)]
pub struct SignWithSchnorrResult {
    pub signature: Vec<u8>,
}

#[derive(CandidType, Deserialize, Serialize, Debug)]
pub struct SchnorrPublicKeyArgs {
    pub canister_id: Option<Principal>,
    pub derivation_path: Vec<Vec<u8>>,
    pub key_id: SchnorrKeyId,
}

#[derive(CandidType, Deserialize, Serialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SchnorrKeyId {
    algorithm: SchnorrAlgorithm,
    name: String,
}

pub async fn schnorr_public_key(
    key_name: String,
    alg: mgt::SchnorrAlgorithm,
    derivation_path: Vec<Vec<u8>>,
) -> Result<PublicKeyOutput, String> {
    let args = mgt::SchnorrPublicKeyArgs {
        canister_id: None,
        derivation_path,
        key_id: mgt::SchnorrKeyId {
            algorithm: alg,
            name: key_name,
        },
    };

    let rt = mgt::schnorr_public_key(&args)
        .await
        .map_err(|err| format!("schnorr_public_key failed {:?}", err))?;
    Ok(PublicKeyOutput {
        public_key: ByteBuf::from(rt.public_key),
        chain_code: ByteBuf::from(rt.chain_code),
    })
}
