//! Deterministic public interoperability fixtures; every private key is a
//! fixed test seed. Run through scripts/verify-dmsg-vectors.mjs independently.
use candid::Principal;
use dmsg_protocol::*;
use dmsg_types::{cose::*, *};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value as Json};

fn tree(v: cbor2::Value) -> Json {
    use cbor2::Value::*;
    match v {
        Integer(n) => match u128::try_from(n) {
            Ok(n) => json!({"uint":n.to_string()}),
            Err(_) => json!({"int":i128::from(n).to_string()}),
        },
        Text(s) => json!({"text":s}),
        Bytes(b) => json!({"bytes":hex::encode(b)}),
        Array(a) => json!({"array":a.into_iter().map(tree).collect::<Vec<_>>()}),
        Map(m) => {
            json!({"map":m.into_iter().map(|(k,v)|vec![tree(k),tree(v)]).collect::<Vec<_>>()})
        }
        Null => Json::Null,
        Bool(b) => json!(b),
        Tag(tag, v) => json!({"tag":tag,"value":tree(*v)}),
        _ => panic!("protocol forbids floats, negative and simple values"),
    }
}
fn vector(name: &str, bytes: Vec<u8>) -> Json {
    let hash = sha256(&bytes);
    let key = SigningKey::from_bytes(&[7; 32]);
    json!({"name":name,"value":tree(cbor2::from_slice(&bytes).unwrap()),"cbor_hex":hex::encode(bytes),"sha256_hex":hex::encode(hash.as_slice()),"ed25519_public_hex":hex::encode(key.verifying_key().to_bytes()),"signature_over_sha256_hex":hex::encode(key.sign(hash.as_slice()).to_bytes())})
}
fn main() {
    let account = AccountId([1; 12]);
    let namespace = "https://dmsg.test/u/";
    let signer = SigningKey::from_bytes(&[7; 32]);
    let public = signer.verifying_key().to_bytes();
    let temp_key = public_cose_key(&Algorithm::Ed25519, &[], &public).unwrap();
    let fingerprint = key_thumbprint(&temp_key).unwrap();
    let kid = fingerprint.to_vec();
    let text = Statement {
        issuer: account_issuer(namespace, &account).unwrap(),
        subject: Some("release/spec".into()),
        issued_at: Some(1_800_000_000),
        content: StatementContent::Text("Approved release v1".into()),
    };
    let digest_statement = Statement {
        content: StatementContent::Digest {
            sha256: sha256(b"document"),
            content_type: Some("application/pdf".into()),
            location: None,
        },
        ..text.clone()
    };
    let (_, text_tbs) = prepare_cose(&text, &Algorithm::Ed25519, &kid).unwrap();
    let (_, digest_tbs) = prepare_cose(&digest_statement, &Algorithm::Ed25519, &kid).unwrap();
    let artifact =
        |tbs: &[u8]| finish_cose(tbs, &public, signer.sign(tbs).to_bytes().to_vec()).unwrap();
    let signed_text = artifact(&text_tbs);
    let signed_digest = artifact(&digest_tbs);
    let timestamped = attach_unverified_timestamp_token(&signed_digest, &[0x30, 0]).unwrap();
    let mut values = vec![
        vector("cose_text_v3", signed_text.cose_sign1.to_vec()),
        vector("cose_digest_v3", signed_digest.cose_sign1.to_vec()),
        vector("timestamped_digest_v3", timestamped.cose_sign1.to_vec()),
        vector("text_tbs_v3", text_tbs),
        vector("digest_tbs_v3", digest_tbs.clone()),
        vector("cose_key_v3", signed_text.cose_key.to_vec()),
        vector("account_id", canonical(&account)),
        vector("account_xid_text", canonical(&account.to_string())),
        vector("account_issuer", canonical(&text.issuer)),
        vector(
            "principal_issuer",
            canonical(
                &principal_issuer(
                    "https://id.test/ic/mainnet/principals/",
                    Principal::from_slice(&[0, 1, 1]),
                )
                .unwrap(),
            ),
        ),
        vector(
            "management_principal_issuer",
            canonical(
                &principal_issuer(
                    "https://id.test/ic/mainnet/principals/",
                    Principal::management_canister(),
                )
                .unwrap(),
            ),
        ),
        vector(
            "ctt_imprint_input",
            canonical(&serde_bytes::Bytes::new(
                &signer.sign(&digest_tbs).to_bytes(),
            )),
        ),
        vector("amount_over_u64", canonical(&("dmsg/amount/v1", u128::MAX))),
        vector(
            "fixed_bytes_containers",
            canonical(&(
                Hash::new([6; 32]),
                Some(Hash::new([7; 32])),
                std::collections::BTreeMap::from([(Hash::new([8; 32]), Hash::new([9; 32]))]),
            )),
        ),
        vector(
            "account_with_subaccount",
            canonical(&account::account_cbor::value(
                &icrc_ledger_types::icrc1::account::Account {
                    owner: Principal::from_slice(&[0, 1, 1]),
                    subaccount: Some([10; 32]),
                },
            )),
        ),
        vector(
            "map_key_order",
            canonical(&std::collections::BTreeMap::from([
                ("aa", 1u64),
                ("b", 2u64),
            ])),
        ),
        vector("root_input_v2", canonical(&(&account, 2u64))),
        vector(
            "root_context_v2",
            canonical(&("dmsg/content-root/v2", Environment::Local, 2u16)),
        ),
    ];
    let key_value: cbor2::Value = cbor2::from_slice(&temp_key).unwrap();
    let cbor2::Value::Map(fields) = key_value else {
        panic!()
    };
    let required = cbor2::Value::Map(fields.into_iter().filter(|(label,_)| matches!(label,cbor2::Value::Integer(n) if [1i64,-1,-2].contains(&i64::try_from(*n).unwrap()))).collect());
    assert_eq!(sha256(&canonical(&required)), fingerprint);
    values.push(vector("key_thumbprint_input", canonical(&required)));
    let home_user = Principal::from_slice(&[0, 1, 1]);
    let allocator_namespace = canonical(&(
        1u8,
        "dmsg/account-id-generator/v1",
        ("dmsg", Environment::Local, namespace, home_user),
    ));
    let hash = sha256(&allocator_namespace);
    let (allocated, _) = ic_auth_types::XidGenerator::new(hash[..5].try_into().unwrap())
        .allocate(100)
        .unwrap();
    values.push(vector("allocator_namespace_v1", allocator_namespace));
    values.push(vector("allocated_account_v1", canonical(&allocated)));
    let request_id = execution_request_id(&account, 0, Hash::new([2; 32]), 0);
    values.push(vector(
        "execution_request_id_v2",
        canonical(&(
            1u8,
            "dmsg/execution-request/v2",
            (&account, 0u64, Hash::new([2; 32]), 0u64),
        )),
    ));
    let approval = Approval {
        device_id: Hash::new([2; 32]),
        security_epoch: 0,
        sequence: 0,
        request_id,
        expires_at: 1_800_000_000_000,
        signature: vec![].into(),
    };
    let sign = SignRequest {
        account_id: account.clone(),
        key: SigningKeyRef {
            algorithm: SigningAlgorithm::Ed25519,
            kid: kid.into(),
            public_key_fingerprint: fingerprint,
        },
        statement: digest_statement,
        origin: "https://example.com".into(),
        max_cycles: 100_000_000_000,
        approval: approval.clone(),
    }
    .into_execution()
    .unwrap();
    let derive = DeriveRootRequest {
        account_id: account,
        target: RootTarget::Candidate {
            generation: 2,
            op_id: Hash::new([9; 32]),
        },
        transport_public_key: ic_bls12_381::G1Affine::generator().to_compressed().into(),
        max_cycles: 100_000_000_000,
        approval,
    }
    .into_execution();
    for (name, input) in [
        ("sign_approval_v3", sign),
        ("derive_root_approval_v3", derive),
    ] {
        let a = &input.approval;
        let preimage = canonical(&(
            1u8,
            "dmsg/device-approval/v2",
            (
                home_user,
                &input.account_id,
                "dmsg/execute/v3",
                a.device_id,
                a.security_epoch,
                a.sequence,
                a.request_id,
                a.expires_at,
                digest("dmsg/execute/v3", &(&input.kind, input.max_cycles)),
            ),
        ));
        assert_eq!(sha256(&preimage), input.approval_message(home_user));
        values.push(vector(name, preimage));
    }
    println!("{}", serde_json::to_string_pretty(&values).unwrap());
}
