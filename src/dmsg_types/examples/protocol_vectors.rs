//! Deterministic public interoperability fixtures; every private key is a
//! fixed test seed. Run through scripts/verify-dmsg-vectors.mjs independently.
use candid::Principal;
use dmsg_protocol::*;
use dmsg_types::{agent::DelegationAuthority, cose::*, *};
use ed25519_dalek::{Signer, SigningKey};

mod support;
use support::vector;

fn main() {
    let account = AccountId([1; 12]);
    let namespace = "https://dmsg.test/u/";
    let signer = SigningKey::from_bytes(&[7; 32]);
    let public = signer.verifying_key().to_bytes();
    let temp_key = public_cose_key(&[], &public).unwrap();
    let fingerprint = key_thumbprint(&temp_key).unwrap();
    let kid = fingerprint.to_vec();
    let text = Statement {
        issuer: account_issuer(namespace, &account),
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
    let (_, text_tbs) = prepare_cose(&text, &kid).unwrap();
    let file_statement = Statement {
        content: StatementContent::FileStatement {
            text: "  第三章需要补充实验数据。\n".into(),
            sha256: sha256(b"document"),
            content_type: Some("application/pdf".into()),
            location: Some("urn:example:report".into()),
        },
        ..text.clone()
    };
    let (_, file_statement_tbs) = prepare_cose(&file_statement, &kid).unwrap();
    let (_, digest_tbs) = prepare_cose(&digest_statement, &kid).unwrap();
    let artifact = |tbs: &[u8]| {
        parse_signing_input(tbs)
            .unwrap()
            .into_signature(&public)
            .unwrap()
            .finish(signer.sign(tbs).to_bytes().to_vec())
            .unwrap()
    };
    let signed_text = artifact(&text_tbs);
    let signed_digest = artifact(&digest_tbs);
    let signed_file_statement = artifact(&file_statement_tbs);
    // Opaque CTT material at unprotected header 270; the signed bytes are unchanged.
    let mut timestamped = cose2::Sign1Message::from_slice(&signed_digest.cose_sign1).unwrap();
    timestamped.unprotected.insert(CTT_HEADER, vec![0x30, 0]);
    let mut values = vec![
        vector("cose_text_v3", signed_text.cose_sign1.to_vec()),
        vector("cose_digest_v3", signed_digest.cose_sign1.to_vec()),
        vector(
            "cose_file_statement_v1",
            signed_file_statement.cose_sign1.to_vec(),
        ),
        vector("file_statement_tbs_v1", file_statement_tbs.clone()),
        vector("timestamped_digest_v3", timestamped.to_vec().unwrap()),
        vector("text_tbs_v3", text_tbs),
        vector("digest_tbs_v3", digest_tbs.clone()),
        vector("cose_key_v3", signed_text.cose_key.to_vec()),
        vector("account_id", canonical(&account)),
        vector("account_xid_text", canonical(&account.to_string())),
        vector("account_issuer", canonical(&text.issuer)),
        vector(
            "principal_issuer",
            canonical(&format!(
                "https://id.test/ic/mainnet/principals/{}",
                Principal::from_slice(&[0, 1, 1])
            )),
        ),
        vector(
            "management_principal_issuer",
            canonical(&format!(
                "https://id.test/ic/mainnet/principals/{}",
                Principal::management_canister()
            )),
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
        signature: Default::default(),
    };
    let origin = "https://example.com";
    let attest = |statement: &Statement, tbs: &[u8]| AttestRequest {
        account_id: account,
        statement: statement.clone(),
        origin: origin.into(),
        signature: signer.sign(tbs).to_bytes().into(),
        approval: approval.clone(),
    };
    for (name, input) in [
        ("attest_approval_v1", attest(&digest_statement, &digest_tbs)),
        (
            "file_statement_approval_v1",
            attest(&file_statement, &file_statement_tbs),
        ),
    ] {
        let a = &input.approval;
        let preimage = canonical(&(
            1u8,
            "dmsg/device-approval/v2",
            (
                home_user,
                &input.account_id,
                ATTEST_APPROVAL_DOMAIN,
                a.device_id,
                a.security_epoch,
                a.sequence,
                a.request_id,
                a.expires_at,
                digest(
                    ATTEST_APPROVAL_DOMAIN,
                    &(&input.statement, &input.origin, &input.signature),
                ),
            ),
        ));
        assert_eq!(
            sha256(&preimage),
            approval_message(
                home_user,
                &input.account_id,
                ATTEST_APPROVAL_DOMAIN,
                &attest_approval_command(&input.statement, &input.origin, &input.signature),
                a,
            )
        );
        values.push(vector(name, preimage));
    }
    let derive = DeriveRootRequest {
        account_id: account,
        generation: 2,
        transport_public_key: ic_bls12_381::G1Affine::generator().to_compressed().into(),
        max_cycles: 70_000_000_000,
        approval: approval.clone(),
    };
    let preimage = canonical(&(
        1u8,
        "dmsg/device-approval/v2",
        (
            home_user,
            &derive.account_id,
            DERIVE_APPROVAL_DOMAIN,
            approval.device_id,
            approval.security_epoch,
            approval.sequence,
            approval.request_id,
            approval.expires_at,
            digest(
                DERIVE_APPROVAL_DOMAIN,
                &(
                    derive.generation,
                    &derive.transport_public_key,
                    derive.max_cycles,
                ),
            ),
        ),
    ));
    assert_eq!(
        sha256(&preimage),
        approval_message(
            home_user,
            &derive.account_id,
            DERIVE_APPROVAL_DOMAIN,
            &derive_approval_command(&derive),
            &approval,
        )
    );
    values.push(vector("derive_root_approval_v1", preimage));
    let devices = [Hash::new([3; 32]), Hash::new([2; 32])];
    let recipients = canonical(&(
        1u8,
        "dmsg/root-recipients/1",
        (vec![Hash::new([2; 32]), Hash::new([3; 32])], 2u64),
    ));
    assert_eq!(sha256(&recipients), root_recipients_digest(&devices, 2));
    values.push(vector("root_recipients_v1", recipients));
    let body = sha256(b"root bundle body");
    let bundle = canonical(&(
        1u8,
        "dmsg/root-bundle-digest/2",
        (root_recipients_digest(&devices, 2), body),
    ));
    assert_eq!(
        sha256(&bundle),
        root_bundle_digest(root_recipients_digest(&devices, 2), body)
    );
    values.push(vector("root_bundle_digest_v2", bundle));
    let delegation = DelegationAuthority::Restricted {
        scopes: vec!["message.draft".into()],
        audiences: vec!["https://dmsg.net".into()],
    };
    let pop = canonical(&(
        1u8,
        "dmsg/controller-pop/v1",
        (home_user, &account, 2u32, &delegation, vec![1u32], request_id),
    ));
    assert_eq!(
        sha256(&pop),
        controller_pop_message(home_user, &account, 2, &delegation, &[1], request_id)
    );
    values.push(vector("controller_pop_v1", pop));
    println!("{}", serde_json::to_string_pretty(&values).unwrap());
}
