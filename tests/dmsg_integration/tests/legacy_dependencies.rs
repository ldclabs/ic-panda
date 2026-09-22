//! Scope tests use matched public releases and local candidate worktrees only.
use candid::{utils::ArgumentEncoder, CandidType, Principal};
use ed25519_dalek::{Signer, SigningKey};
use ic_cose_types::{
    cose::{
        ecdh::{try_ecdh_x25519, PublicKey, StaticSecret},
        encrypt0::{cose_decrypt0, cose_encrypt0},
    },
    types::{
        namespace::CreateNamespaceInput,
        setting::{CreateSettingInput, CreateSettingOutput, SettingPath},
        ECDHInput, ECDHOutput,
    },
};
use ic_oss_types::{
    bucket::UpdateBucketInput,
    cose::{cose_sign1, cose_sign1_to_vec, EdDSA, Token, BUCKET_TOKEN_AAD},
    file::*,
    folder::*,
};
use ic_vetkeys::{DerivedPublicKey, EncryptedVetKey, TransportSecretKey};
use pocket_ic::{PocketIc, PocketIcBuilder};
use serde::{de::DeserializeOwned, Deserialize};
use serde_bytes::{ByteArray, ByteBuf};
use std::{collections::BTreeSet, path::PathBuf};

#[derive(CandidType, Deserialize)]
enum CoseArgs {
    Init {
        name: String,
        ecdsa_key_name: String,
        schnorr_key_name: String,
        vetkd_key_name: String,
        allowed_apis: BTreeSet<String>,
        subnet_size: u64,
        freezing_threshold: u64,
        governance_canister: Option<Principal>,
    },
}
#[derive(CandidType, Deserialize)]
struct Namespace {
    name: String,
    status: i8,
}
#[derive(CandidType, Deserialize, PartialEq, Debug)]
struct Setting {
    version: u32,
    dek: Option<ByteBuf>,
    payload: Option<ByteBuf>,
}
#[derive(CandidType, Deserialize)]
enum Mode {
    Draining,
    ReadOnly,
}
#[derive(CandidType, Deserialize)]
struct ScopeStatus {
    mode: Mode,
}

#[derive(CandidType, Deserialize)]
struct FolderManifest {
    status: ScopeStatus,
    entries: Vec<FrozenFile>,
    complete: bool,
}

#[derive(CandidType, Deserialize)]
struct FrozenFile {
    id: u32,
    chunks: Vec<(u32, u32, ByteArray<32>)>,
    missing: Vec<u32>,
    complete: bool,
}

fn update<R: DeserializeOwned + CandidType>(
    ic: &PocketIc,
    id: Principal,
    caller: Principal,
    method: &str,
    args: impl ArgumentEncoder,
) -> R {
    let bytes = ic
        .update_call(id, caller, method, candid::encode_args(args).unwrap())
        .unwrap();
    candid::decode_one(&bytes).unwrap()
}
fn query<R: DeserializeOwned + CandidType>(
    ic: &PocketIc,
    id: Principal,
    caller: Principal,
    method: &str,
    args: impl ArgumentEncoder,
) -> R {
    let bytes = ic
        .query_call(id, caller, method, candid::encode_args(args).unwrap())
        .unwrap();
    candid::decode_one(&bytes).unwrap()
}
fn variable(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} is required")))
}

fn vetkey(ic: &PocketIc, id: Principal, user: Principal, path: &SettingPath) -> Vec<u8> {
    let transport = TransportSecretKey::from_seed(vec![41; 32]).unwrap();
    let pk: Result<ByteBuf, String> = update(ic, id, user, "vetkd_public_key", (path,));
    let encrypted: Result<ByteBuf, String> = update(
        ic,
        id,
        user,
        "vetkd_encrypted_key",
        (
            path,
            ByteArray::<48>::new(transport.public_key().try_into().unwrap()),
        ),
    );
    EncryptedVetKey::deserialize(&encrypted.unwrap())
        .unwrap()
        .decrypt_and_verify(
            &transport,
            &DerivedPublicKey::deserialize(&pk.unwrap()).unwrap(),
            &path.key,
        )
        .unwrap()
        .derive_symmetric_key("", 32)
}
fn ecdh(ic: &PocketIc, id: Principal, user: Principal, path: &SettingPath) -> Vec<u8> {
    let secret = [42; 32];
    let public = PublicKey::from(&StaticSecret::from(secret));
    let result: Result<ECDHOutput<ByteBuf>, String> = update(
        ic,
        id,
        user,
        "ecdh_cose_encrypted_key",
        (
            path,
            ECDHInput {
                public_key: ByteArray::new(public.to_bytes()),
                nonce: ByteArray::new([3; 12]),
            },
        ),
    );
    let result = result.unwrap();
    let (shared, _) = try_ecdh_x25519(secret, *result.public_key).unwrap();
    cose_decrypt0(&result.payload, shared.as_bytes(), user.as_slice()).unwrap()
}

#[test]
#[ignore = "requires local candidate Wasm and verified I0 releases"]
fn freezes_one_cose_namespace_without_changing_existing_recovery_roots() {
    let ic = PocketIcBuilder::new()
        .with_nns_subnet()
        .with_application_subnet()
        .with_fiduciary_subnet()
        .build();
    let gov = Principal::from_slice(&[90]);
    let user = Principal::from_slice(&[91]);
    let id = ic.create_canister_with_settings(Some(gov), None);
    ic.add_cycles(id, 10_000_000_000_000_000);
    ic.install_canister(
        id,
        std::fs::read(
            variable("DMSG_LEGACY_RELEASES").join("ic-cose-v0.9.3-ic_cose_canister.wasm.gz"),
        )
        .unwrap(),
        candid::encode_args((Some(CoseArgs::Init {
            name: "legacy".into(),
            ecdsa_key_name: "key_1".into(),
            schnorr_key_name: "key_1".into(),
            vetkd_key_name: "key_1".into(),
            allowed_apis: BTreeSet::new(),
            subnet_size: 0,
            freezing_threshold: 1_000_000_000_000,
            governance_canister: Some(gov),
        }),))
        .unwrap(),
        Some(gov),
    );
    update::<Result<(), String>>(&ic, id, gov, "admin_add_managers", (BTreeSet::from([gov]),))
        .unwrap();
    for name in ["dmsg_legacy", "unrelated"] {
        update::<Result<Namespace, String>>(
            &ic,
            id,
            gov,
            "admin_create_namespace",
            (CreateNamespaceInput {
                name: name.into(),
                visibility: 0,
                desc: None,
                max_payload_size: Some(1024),
                managers: BTreeSet::from([gov]),
                auditors: BTreeSet::from([user]),
                users: BTreeSet::from([user]),
                session_expires_in_ms: None,
            },),
        )
        .unwrap();
    }
    for _ in 0..30 {
        ic.tick();
    }
    let path = SettingPath {
        ns: "dmsg_legacy".into(),
        user_owned: true,
        subject: Some(user),
        key: ByteBuf::from(cbor2::to_vec("PANDA").unwrap()),
        version: 0,
    };
    let root = vetkey(&ic, id, user, &path);
    let old_ecdh = ecdh(&ic, id, user, &path);
    let wrapped = cose_encrypt0(
        b"synthetic old master",
        &root.clone().try_into().unwrap(),
        user.as_slice(),
        &[4; 12],
        None,
    )
    .unwrap();
    update::<Result<CreateSettingOutput, String>>(
        &ic,
        id,
        user,
        "setting_create",
        (
            &path,
            CreateSettingInput {
                payload: None,
                desc: None,
                status: None,
                tags: None,
                dek: Some(wrapped.clone().into()),
            },
        ),
    )
    .unwrap();
    let before: Result<Setting, String> = query(&ic, id, user, "setting_get", (&path,));
    ic.upgrade_canister(
        id,
        std::fs::read(variable("DMSG_LEGACY_COSE_WASM")).unwrap(),
        candid::encode_args((None::<CoseArgs>,)).unwrap(),
        Some(gov),
    )
    .unwrap();
    assert_eq!(root, vetkey(&ic, id, user, &path));
    assert_eq!(old_ecdh, ecdh(&ic, id, user, &path));
    update::<Result<ScopeStatus, String>>(
        &ic,
        id,
        gov,
        "admin_legacy_drain_namespace",
        (
            "dmsg_legacy".to_string(),
            ByteArray::new([8; 32]),
            ByteArray::new([9; 32]),
        ),
    )
    .unwrap();
    let frozen: Result<ScopeStatus, String> = update(
        &ic,
        id,
        gov,
        "admin_legacy_seal_namespace",
        ("dmsg_legacy".to_string(), ByteArray::new([8; 32])),
    );
    assert!(matches!(frozen.unwrap().mode, Mode::ReadOnly));
    for (method, apis) in [
        (
            "admin_add_allowed_apis",
            BTreeSet::from(["setting_create".to_string()]),
        ),
        (
            "admin_remove_allowed_apis",
            BTreeSet::from(["vetkd_encrypted_key".to_string()]),
        ),
    ] {
        assert!(update::<Result<(), String>>(&ic, id, gov, method, (apis,)).is_err());
    }
    let after: Result<Setting, String> = query(&ic, id, user, "setting_get", (&path,));
    assert_eq!(before.unwrap(), after.unwrap());
    assert_eq!(root, vetkey(&ic, id, user, &path));
    assert_eq!(old_ecdh, ecdh(&ic, id, user, &path));
    assert_eq!(
        cose_decrypt0(&wrapped, &root.try_into().unwrap(), user.as_slice()).unwrap(),
        b"synthetic old master"
    );
    let input = CreateSettingInput {
        payload: Some(vec![1].into()),
        desc: None,
        status: None,
        tags: None,
        dek: None,
    };
    let mut new_path = path.clone();
    new_path.key = vec![1].into();
    assert!(update::<Result<CreateSettingOutput, String>>(
        &ic,
        id,
        user,
        "setting_create",
        (&new_path, &input)
    )
    .is_err());
    new_path.ns = "unrelated".into();
    update::<Result<CreateSettingOutput, String>>(
        &ic,
        id,
        user,
        "setting_create",
        (&new_path, &input),
    )
    .unwrap();
    assert!(update::<Result<(), String>>(
        &ic,
        id,
        gov,
        "namespace_add_users",
        (
            "dmsg_legacy".to_string(),
            BTreeSet::from([Principal::from_slice(&[92])])
        )
    )
    .is_err());
}

#[test]
#[ignore = "requires local candidate Wasm and verified I0 releases"]
fn freezes_only_selected_oss_tree_and_rejects_previously_issued_write_tokens() {
    let ic = PocketIcBuilder::new().with_application_subnet().build();
    let gov = Principal::from_slice(&[90]);
    let user = Principal::from_slice(&[91]);
    let id = ic.create_canister_with_settings(Some(gov), None);
    ic.add_cycles(id, 10_000_000_000_000_000);
    ic.install_canister(
        id,
        std::fs::read(variable("DMSG_LEGACY_RELEASES").join("ic-oss-v1.2.3-ic_oss_bucket.wasm.gz"))
            .unwrap(),
        candid::encode_args((None::<String>,)).unwrap(),
        Some(gov),
    );
    update::<Result<(), String>>(&ic, id, gov, "admin_set_managers", (BTreeSet::from([gov]),))
        .unwrap();
    let signer = SigningKey::from_bytes(&[61; 32]);
    update::<Result<(), String>>(
        &ic,
        id,
        gov,
        "admin_update_bucket",
        (UpdateBucketInput {
            trusted_eddsa_pub_keys: Some(vec![signer.verifying_key().to_bytes().into()]),
            ..Default::default()
        },),
    )
    .unwrap();
    let root: Result<CreateFolderOutput, String> = update(
        &ic,
        id,
        gov,
        "create_folder",
        (
            CreateFolderInput {
                name: "dmsg".into(),
                parent: 0,
            },
            None::<ByteBuf>,
        ),
    );
    let root = root.unwrap().id;
    let other: Result<CreateFolderOutput, String> = update(
        &ic,
        id,
        gov,
        "create_folder",
        (
            CreateFolderInput {
                name: "other".into(),
                parent: 0,
            },
            None::<ByteBuf>,
        ),
    );
    let other = other.unwrap().id;
    let now = (ic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000) as i64;
    let mut token = cose_sign1(
        Token {
            subject: user,
            audience: id,
            policies: format!("Folder.*:{root}"),
        }
        .to_cwt(now, 86400),
        EdDSA,
        None,
    )
    .unwrap();
    let bytes = token
        .prepare_signature(None, None, Some(BUCKET_TOKEN_AAD))
        .unwrap();
    token
        .set_signature(signer.sign(&bytes).to_bytes().to_vec())
        .unwrap();
    let token = ByteBuf::from(cose_sign1_to_vec(&token).unwrap());
    let content = cose_encrypt0(&vec![7; 262151], &[8; 32], &[], &[9; 12], None).unwrap();
    let input = CreateFileInput {
        parent: root,
        name: "encrypted.cbor".into(),
        content_type: "application/cbor".into(),
        size: Some(content.len() as u64),
        content: Some(content.clone().into()),
        ..Default::default()
    };
    let file: Result<CreateFileOutput, String> =
        update(&ic, id, user, "create_file", (&input, Some(token.clone())));
    let file = file.unwrap().id;
    ic.upgrade_canister(
        id,
        std::fs::read(variable("DMSG_LEGACY_OSS_WASM")).unwrap(),
        candid::encode_args((None::<String>,)).unwrap(),
        Some(gov),
    )
    .unwrap();
    update::<Result<ScopeStatus, String>>(
        &ic,
        id,
        gov,
        "admin_legacy_drain_folder",
        (root, ByteArray::new([8; 32]), ByteArray::new([9; 32])),
    )
    .unwrap();
    update::<Result<ScopeStatus, String>>(
        &ic,
        id,
        gov,
        "admin_legacy_seal_folder",
        (root, ByteArray::new([8; 32])),
    )
    .unwrap();
    for config in [
        UpdateBucketInput {
            status: Some(-1),
            ..Default::default()
        },
        UpdateBucketInput {
            visibility: Some(1),
            ..Default::default()
        },
        UpdateBucketInput {
            trusted_eddsa_pub_keys: Some(vec![]),
            ..Default::default()
        },
    ] {
        assert!(
            update::<Result<(), String>>(&ic, id, gov, "admin_update_bucket", (config,)).is_err()
        );
    }
    let manifest = update::<Result<FolderManifest, String>>(
        &ic,
        id,
        user,
        "legacy_folder_manifest",
        (root, None::<u32>, Some(token.clone())),
    )
    .unwrap();
    assert!(manifest.complete && matches!(manifest.status.mode, Mode::ReadOnly));
    assert_eq!(manifest.entries.len(), 1);
    let frozen = &manifest.entries[0];
    assert_eq!(frozen.id, file);
    assert!(frozen.complete && frozen.missing.is_empty());
    assert_eq!(frozen.chunks.len(), 2);
    assert_eq!(
        frozen
            .chunks
            .iter()
            .map(|(_, size, _)| *size as usize)
            .sum::<usize>(),
        content.len()
    );
    let write = ic.update_call(
        id,
        user,
        "update_file_chunk",
        candid::encode_args((
            UpdateFileChunkInput {
                id: file,
                chunk_index: 0,
                content: vec![0].into(),
            },
            Some(token.clone()),
        ))
        .unwrap(),
    );
    assert!(
        write.is_err()
            || candid::decode_one::<Result<UpdateFileChunkOutput, String>>(&write.unwrap())
                .unwrap()
                .is_err()
    );
    let chunks: Result<Vec<(u32, ByteBuf)>, String> = query(
        &ic,
        id,
        user,
        "get_file_chunks",
        (file, 0u32, Some(8u32), Some(token.clone())),
    );
    let chunks = chunks.unwrap();
    assert_eq!(
        chunks
            .iter()
            .flat_map(|(_, b)| b.iter().copied())
            .collect::<Vec<_>>(),
        content
    );
    assert_eq!(
        update::<Result<bool, String>>(&ic, id, gov, "delete_folder", (root, None::<ByteBuf>))
            .unwrap_err(),
        "LegacyWriteDisabled"
    );
    let outside = CreateFileInput {
        parent: other,
        name: "other.bin".into(),
        content_type: "application/octet-stream".into(),
        content: Some(vec![1, 2, 3].into()),
        ..Default::default()
    };
    update::<Result<CreateFileOutput, String>>(
        &ic,
        id,
        gov,
        "create_file",
        (outside, None::<ByteBuf>),
    )
    .unwrap();
}
