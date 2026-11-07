use candid::{CandidType, Principal};
use dmsg_types::{Hash, app_store::*, billing::ContractSource};
use serde::Serialize;

#[derive(CandidType, Serialize)]
struct LegacyConfig {
    verifiers: Vec<Principal>,
    bundle_id: String,
    app_apple_id: u64,
    environment: StoreEnvironment,
    products: Vec<StoreProduct>,
}

#[derive(CandidType, Serialize)]
enum LegacySource {
    AppStore { transaction_id: Hash },
}

#[test]
fn old_candid_and_stable_records_decode_without_enabling_sandbox() {
    let old = LegacyConfig {
        verifiers: vec![Principal::from_slice(&[1])],
        bundle_id: "net.dmsg.ios".into(),
        app_apple_id: 6827050680,
        environment: StoreEnvironment::Production,
        products: vec![],
    };
    let candid: AppStoreConfig = candid::decode_one(&candid::encode_one(&old).unwrap()).unwrap();
    let stable: AppStoreConfig = cbor2::from_slice(&dmsg_protocol::canonical(&old)).unwrap();
    for mut config in [candid, stable] {
        assert_eq!(config.accept_sandbox, None);
        assert!(config.accepts(&StoreEnvironment::Production));
        assert!(!config.accepts(&StoreEnvironment::Sandbox));
        config.accept_sandbox = Some(true);
        assert!(config.accepts(&StoreEnvironment::Sandbox));
    }
    let old = LegacySource::AppStore {
        transaction_id: Hash::new([1; 32]),
    };
    let candid: ContractSource = candid::decode_one(&candid::encode_one(&old).unwrap()).unwrap();
    let stable: ContractSource = cbor2::from_slice(&dmsg_protocol::canonical(&old)).unwrap();
    for source in [candid, stable] {
        assert!(matches!(
            source,
            ContractSource::AppStore {
                environment: None,
                ..
            }
        ));
    }
}
