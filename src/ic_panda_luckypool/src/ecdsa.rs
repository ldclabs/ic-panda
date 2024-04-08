use base64::{engine::general_purpose, Engine};
use ciborium::into_writer;
use ic_cdk::api::management_canister::ecdsa;

use crate::utils;

pub async fn sign_token(message: Vec<u8>) -> Result<String, String> {
    let args = ecdsa::SignWithEcdsaArgument {
        message_hash: utils::sha3_256(&message).to_vec(),
        derivation_path: vec![b"sign_token".to_vec()],
        key_id: ecdsa::EcdsaKeyId {
            curve: ecdsa::EcdsaCurve::Secp256k1,
            name: "test_key_1".to_string(),
        },
    };

    let (response,): (ecdsa::SignWithEcdsaResponse,) = ecdsa::sign_with_ecdsa(args)
        .await
        .map_err(|err| format!("sign_with_ecdsa failed {:?}", err))?;

    let mut buf = Vec::new();
    into_writer(&vec![message, response.signature], &mut buf)
        .map_err(|err| format!("into_writer failed {:?}", err))?;

    Ok(general_purpose::URL_SAFE_NO_PAD.encode(&buf))
}

pub async fn sign_token_key() -> Result<String, String> {
    let args = ecdsa::EcdsaPublicKeyArgument {
        canister_id: Some(ic_cdk::api::id()),
        derivation_path: vec![b"sign_token".to_vec()],
        key_id: ecdsa::EcdsaKeyId {
            curve: ecdsa::EcdsaCurve::Secp256k1,
            name: "test_key_1".to_string(),
        },
    };

    let (response,): (ecdsa::EcdsaPublicKeyResponse,) = ecdsa::ecdsa_public_key(args)
        .await
        .map_err(|err| format!("ecdsa_public_key failed {:?}", err))?;

    Ok(general_purpose::URL_SAFE_NO_PAD.encode(&response.public_key))
}
