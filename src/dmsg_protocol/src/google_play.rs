//! Independent Google Play account binding and purchase identities.
use candid::Principal;
use dmsg_types::{AccountId, Hash, google_play::*};
use sha2::{Digest, Sha256};

/// Google accepts at most 64 characters. The digest reveals no account ID and
/// grants no authority; the verifier must match Google's returned binding.
pub fn account_token(home: Principal, account: &AccountId) -> String {
    Sha256::digest(format!(
        "dmsg/google-play/account/1\n{}\n{account}",
        home.to_text()
    ))
    .iter()
    .map(|b| format!("{b:02x}"))
    .collect()
}

/// Subscription periods use the actual successful order; consumables use their
/// immutable token hash, including purchases for which Google has no order ID.
pub fn transaction_id(transaction: &PlayTransaction, kind: &PlayProductKind) -> Hash {
    let order = match kind {
        PlayProductKind::Plan { .. } => transaction.order_id.as_ref(),
        PlayProductKind::Storage { .. } => None,
    };
    crate::digest(
        "dmsg/google-play/transaction/v1",
        &(
            &transaction.environment,
            &transaction.package_name,
            &transaction.token_hash,
            order,
        ),
    )
}

/// A token's ownership cannot change across orders or restored installations.
pub fn token_id(environment: &PlayEnvironment, package: &str, token_hash: &Hash) -> Hash {
    crate::digest(
        "dmsg/google-play/token/v1",
        &(environment, package, token_hash),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_separates_home_and_account_without_exposing_either() {
        let first = Principal::from_slice(&[1, 1]);
        let second = Principal::from_slice(&[2, 1]);
        let account = AccountId([3; 12]);
        let token = account_token(first, &account);
        assert_eq!(token.len(), 64);
        assert!(
            token
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
        assert_ne!(token, account_token(second, &account));
        assert_ne!(token, account_token(first, &AccountId([4; 12])));
    }

    #[test]
    fn public_play_vectors_bind_token_environment_package_and_subscription_order() {
        let vectors: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/vectors/client/google-play-v1.json"
        ))
        .unwrap();
        for case in vectors["cases"].as_array().unwrap() {
            let t: PlayTransaction = serde_json::from_value(case["transaction"].clone()).unwrap();
            let kind: PlayProductKind = serde_json::from_value(case["kind"].clone()).unwrap();
            assert_eq!(
                hex::encode(transaction_id(&t, &kind).as_slice()),
                case["transaction_id"]
            );
            assert_eq!(
                hex::encode(token_id(&t.environment, &t.package_name, &t.token_hash).as_slice()),
                case["token_id"]
            );
            let mut changed = t.clone();
            changed.environment = PlayEnvironment::Test;
            assert_ne!(transaction_id(&t, &kind), transaction_id(&changed, &kind));
            changed = t.clone();
            changed.token_hash = Hash::new([9; 32]);
            assert_ne!(transaction_id(&t, &kind), transaction_id(&changed, &kind));
            changed = t.clone();
            changed.package_name.push_str(".staging");
            assert_ne!(transaction_id(&t, &kind), transaction_id(&changed, &kind));
            changed = t.clone();
            changed.order_id = Some("GPA.other-order".into());
            if matches!(kind, PlayProductKind::Storage { .. }) {
                assert_eq!(transaction_id(&t, &kind), transaction_id(&changed, &kind));
            } else {
                assert_ne!(transaction_id(&t, &kind), transaction_id(&changed, &kind));
            }
        }
    }
}
