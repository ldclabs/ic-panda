//! `dmsg-android/1`: domain-separated Play Integrity request commitments.
//! Integrity is an installation admission signal, never account authorization.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

/// Exact action covered by a standard Play Integrity request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AndroidAssertion {
    /// Original dMsg account.
    pub account: String,
    /// Approved device ID, lowercase hex.
    pub device: String,
    /// SHA-256 of the independent notification installation public key.
    pub installation: String,
    /// Fixed action: `push`, `purchase-prepare` or `purchase`.
    pub purpose: String,
    /// `digest("dmsg/android/action/v1", details)`, lowercase hex.
    pub digest: String,
    /// Server-generated 32-byte random nonce, lowercase hex.
    pub nonce: String,
    /// Business environment, independent of Play test purchases.
    pub environment: String,
    /// Exact Android application package.
    pub application: String,
}

/// Pass only this base64url digest to Google, never account IDs or provider tokens.
pub fn request_hash(value: &AndroidAssertion) -> String {
    URL_SAFE_NO_PAD.encode(crate::digest("dmsg/android/assert/v1", value).as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_vector_binds_every_authority_and_action_field() {
        let vector: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/vectors/client/android-v1.json"
        ))
        .unwrap();
        let assertion: AndroidAssertion =
            serde_json::from_value(vector["assertion"].clone()).unwrap();
        assert_eq!(request_hash(&assertion), vector["request_hash"]);
        for name in [
            "account",
            "device",
            "installation",
            "purpose",
            "digest",
            "nonce",
            "environment",
            "application",
        ] {
            let mut changed = vector["assertion"].clone();
            changed[name] = format!("{}x", changed[name].as_str().unwrap()).into();
            let changed: AndroidAssertion = serde_json::from_value(changed).unwrap();
            assert_ne!(request_hash(&assertion), request_hash(&changed), "{name}");
        }
    }
}
