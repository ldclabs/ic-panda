use ic_cdk::management_canister as mgt;
use ic_cose_types::{format_error, types::PublicKeyOutput};
use serde_bytes::ByteBuf;

/// Returns a valid extended BIP-32 derivation path from an Account (Principal + subaccount)
pub fn derive_public_key(
    ecdsa_public_key: &PublicKeyOutput,
    derivation_path: Vec<Vec<u8>>,
) -> Result<PublicKeyOutput, String> {
    let path = ic_secp256k1::DerivationPath::new(
        derivation_path
            .into_iter()
            .map(ic_secp256k1::DerivationIndex)
            .collect(),
    );

    let chain_code: [u8; 32] = ecdsa_public_key
        .chain_code
        .to_vec()
        .try_into()
        .map_err(format_error)?;
    let pk = ic_secp256k1::PublicKey::deserialize_sec1(&ecdsa_public_key.public_key)
        .map_err(format_error)?;
    let (derived_public_key, derived_chain_code) =
        pk.derive_subkey_with_chain_code(&path, &chain_code);

    Ok(PublicKeyOutput {
        public_key: ByteBuf::from(derived_public_key.serialize_sec1(true)),
        chain_code: ByteBuf::from(derived_chain_code),
    })
}

pub async fn sign_with_ecdsa(
    key_name: String,
    derivation_path: Vec<Vec<u8>>,
    message_hash: Vec<u8>,
) -> Result<Vec<u8>, String> {
    if message_hash.len() != 32 {
        return Err("message must be 32 bytes".to_string());
    }

    let args = mgt::SignWithEcdsaArgs {
        message_hash,
        derivation_path,
        key_id: mgt::EcdsaKeyId {
            curve: mgt::EcdsaCurve::Secp256k1,
            name: key_name,
        },
    };

    let rt = mgt::sign_with_ecdsa(&args)
        .await
        .map_err(|err| format!("sign_with_ecdsa failed {:?}", err))?;

    Ok(rt.signature)
}

pub async fn ecdsa_public_key(
    key_name: String,
    derivation_path: Vec<Vec<u8>>,
) -> Result<PublicKeyOutput, String> {
    let args = mgt::EcdsaPublicKeyArgs {
        canister_id: None,
        derivation_path,
        key_id: mgt::EcdsaKeyId {
            curve: mgt::EcdsaCurve::Secp256k1,
            name: key_name,
        },
    };

    let rt = mgt::ecdsa_public_key(&args)
        .await
        .map_err(|err| format!("ecdsa_public_key failed {:?}", err))?;

    Ok(PublicKeyOutput {
        public_key: ByteBuf::from(rt.public_key),
        chain_code: ByteBuf::from(rt.chain_code),
    })
}
