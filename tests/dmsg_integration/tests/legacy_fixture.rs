//! Upgrade real released legacy modules, verify freeze/reads, and serve a local
//! gateway for independently verifying ingress snapshot certificates in Chrome.
use candid::{utils::ArgumentEncoder, CandidType, Principal};
use ed25519_dalek::SigningKey;
use ic_message_types::{channel::*, migration::*, profile::*};
use pocket_ic::{PocketIc, PocketIcBuilder};
use serde::{de::DeserializeOwned, Deserialize};
use serde_bytes::{ByteArray, ByteBuf};
use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(CandidType, Deserialize)]
enum Args {
    Init {
        name: String,
        managers: BTreeSet<Principal>,
        schnorr_key_name: String,
        session_expires_in_ms: u64,
    },
}
#[derive(CandidType)]
enum Kind {
    Cose,
    Profile,
    OssCluster,
    OssBucket,
}

#[derive(CandidType, Deserialize)]
struct Delegator {
    owner: Principal,
    sign_in_at: u64,
    role: i8,
}

fn update<R: DeserializeOwned + CandidType>(
    ic: &PocketIc,
    canister: Principal,
    caller: Principal,
    method: &str,
    args: impl ArgumentEncoder,
) -> R {
    let bytes = ic
        .update_call(canister, caller, method, candid::encode_args(args).unwrap())
        .unwrap();
    candid::decode_one(&bytes).unwrap()
}

fn query<R: DeserializeOwned + CandidType>(
    ic: &PocketIc,
    canister: Principal,
    caller: Principal,
    method: &str,
    args: impl ArgumentEncoder,
) -> R {
    let bytes = ic
        .query_call(canister, caller, method, candid::encode_args(args).unwrap())
        .unwrap();
    candid::decode_one(&bytes).unwrap()
}

#[test]
#[ignore = "local Chrome legacy snapshot gateway; requires verified release artifacts"]
fn legacy_extension_gateway() {
    let output =
        PathBuf::from(std::env::var_os("DMSG_CLOUD_FIXTURE_DIR").expect("fixture directory"));
    let releases =
        PathBuf::from(std::env::var_os("DMSG_LEGACY_RELEASES").expect("verified legacy releases"));
    let wasm = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-unknown-unknown/release");
    let mut ic = PocketIcBuilder::new()
        .with_nns_subnet()
        .with_application_subnet()
        .with_fiduciary_subnet()
        .build();
    let subnet = ic.topology().get_app_subnets()[0];
    let gov = Principal::from_slice(&[90]);
    let mut der = vec![
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    der.extend(SigningKey::from_bytes(&[52; 32]).verifying_key().to_bytes());
    let user = Principal::self_authenticating(der);
    let message = ic
        .create_canister_with_id(
            Some(gov),
            None,
            Principal::from_text("nscli-qiaaa-aaaaj-qa4pa-cai").unwrap(),
        )
        .unwrap();
    let identity = ic
        .create_canister_with_id(
            Some(gov),
            None,
            Principal::from_text("2rgax-kyaaa-aaaap-anvba-cai").unwrap(),
        )
        .unwrap();
    let ledger = ic
        .create_canister_with_id(
            Some(gov),
            None,
            Principal::from_text("druyg-tyaaa-aaaaq-aactq-cai").unwrap(),
        )
        .unwrap();
    let profile = ic.create_canister_on_subnet(Some(gov), None, subnet);
    let channel = ic.create_canister_on_subnet(Some(gov), None, subnet);
    let gate = ic.create_canister_on_subnet(Some(gov), None, subnet);
    for id in [message, identity, ledger, profile, channel, gate] {
        ic.add_cycles(id, 10_000_000_000_000_000);
    }
    let args = || {
        candid::encode_args((Some(Args::Init {
            name: "legacy fixture".into(),
            managers: BTreeSet::from([gov, message]),
            schnorr_key_name: "key_1".into(),
            session_expires_in_ms: 3600000,
        }),))
        .unwrap()
    };
    for (id, release) in [
        (message, "ic-panda-v2.14.0-ic_message.wasm.gz"),
        (identity, "ic-panda-v2.15.1-ic_name_identity.wasm.gz"),
        (profile, "ic-panda-v2.14.0-ic_message_profile.wasm.gz"),
        (channel, "ic-panda-v2.13.0-ic_message_channel.wasm.gz"),
    ] {
        ic.install_canister(
            id,
            std::fs::read(releases.join(release)).unwrap(),
            args(),
            Some(gov),
        );
    }
    for (id, name) in [(ledger, "dmsg_test_ledger"), (gate, "dmsg_test_legacy")] {
        ic.install_canister(
            id,
            std::fs::read(wasm.join(format!("{name}.wasm"))).unwrap(),
            candid::encode_args(()).unwrap(),
            Some(gov),
        );
    }
    let _: () = update(
        &ic,
        ledger,
        gov,
        "mint_test",
        (
            icrc_ledger_types::icrc1::account::Account {
                owner: user,
                subaccount: None,
            },
            1_000_000_000_000_000_000u128,
        ),
    );
    for (kind, id) in [(Kind::Cose, gate), (Kind::Profile, profile)] {
        update::<Result<(), String>>(&ic, message, gov, "admin_add_canister", (kind, id)).unwrap();
    }
    update::<Result<UserInfo, String>>(
        &ic,
        message,
        user,
        "register_username",
        ("snapshot_owner".to_string(), None::<String>),
    )
    .unwrap();
    update::<Result<Vec<Delegator>, String>>(
        &ic,
        identity,
        user,
        "activate_name",
        ("snapshot_owner".to_string(),),
    )
    .unwrap();
    // Public synthetic keys; exercise actual historical COSE bytes in clients.
    let kek = ic_cose_types::cose::cose_aes256_key([8; 32], vec![])
        .to_vec()
        .unwrap();
    let dek = ic_cose_types::cose::cose_aes256_key([9; 32], vec![])
        .to_vec()
        .unwrap();
    let encrypted =
        ic_cose_types::cose::encrypt0::cose_encrypt0(&dek, &[8; 32], &[], &[1; 12], None).unwrap();
    let mut create = CreateChannelInput {
        name: "Frozen channel".into(),
        image: String::new(),
        description: String::new(),
        managers: HashMap::from([(
            user,
            ChannelECDHInput {
                ecdh_pub: None,
                ecdh_remote: None,
            },
        )]),
        dek: encrypted.clone().into(),
        created_by: user,
        paid: 1000000000,
    };
    if std::env::var_os("DMSG_SHARED_FIXTURE").is_some() {
        let mut der = vec![
            0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
        ];
        der.extend(SigningKey::from_bytes(&[53; 32]).verifying_key().to_bytes());
        create.managers.insert(
            Principal::self_authenticating(der),
            ChannelECDHInput {
                ecdh_pub: None,
                ecdh_remote: None,
            },
        );
    }
    let created: Result<ChannelInfo, String> =
        update(&ic, channel, gov, "admin_create_channel", (create.clone(),));
    let id = created.unwrap().id;
    let clean: Result<ChannelInfo, String> =
        update(&ic, channel, gov, "admin_create_channel", (create.clone(),));
    let clean_id = clean.unwrap().id;
    for number in 0..65u32 {
        let mut nonce = [2; 12];
        nonce[8..].copy_from_slice(&number.to_be_bytes());
        let payload = ic_cose_types::cose::encrypt0::cose_encrypt0(
            &cbor2::to_vec(&format!("Legacy shared history {number}")).unwrap(),
            &[9; 32],
            &[],
            &nonce,
            None,
        )
        .unwrap();
        update::<Result<AddMessageOutput, String>>(
            &ic,
            channel,
            user,
            "add_message",
            (AddMessageInput {
                channel: id,
                payload: payload.into(),
                reply_to: None,
            },),
        )
        .unwrap();
    }
    let profile_before: Result<ProfileInfo, String> =
        query(&ic, profile, user, "get_profile", (None::<Principal>,));
    for _ in 0..30 {
        ic.tick();
    }
    let iv_before: Result<ByteBuf, String> = query(&ic, message, user, "my_iv", ());
    let iv_before = iv_before.unwrap();
    for (id, name) in [
        (message, "ic_message"),
        (identity, "ic_name_identity"),
        (profile, "ic_message_profile"),
        (channel, "ic_message_channel"),
    ] {
        ic.upgrade_canister(
            id,
            std::fs::read(wasm.join(format!("{name}.wasm"))).unwrap(),
            candid::encode_args((None::<Args>,)).unwrap(),
            Some(gov),
        )
        .unwrap();
        let status: FreezeStatus = query(&ic, id, user, "legacy_status", ());
        assert!(status.baseline_needed);
        update::<Result<(), String>>(
            &ic,
            id,
            gov,
            "admin_legacy_acknowledge_baseline",
            (ByteArray::new([9; 32]),),
        )
        .unwrap();
    }
    let iv_after: Result<ByteBuf, String> = query(&ic, message, user, "my_iv", ());
    assert_eq!(iv_before, iv_after.unwrap());
    let profile_after: Result<ProfileInfo, String> =
        query(&ic, profile, user, "get_profile", (None::<Principal>,));
    assert_eq!(
        candid::encode_one(profile_before.unwrap()).unwrap(),
        candid::encode_one(profile_after.unwrap()).unwrap()
    );
    for kind in [Kind::OssCluster, Kind::OssBucket] {
        update::<Result<(), String>>(&ic, channel, gov, "admin_add_canister", (kind, gate))
            .unwrap();
    }
    let _: () = update(&ic, gate, gov, "pause", ());
    let pending = ic
        .submit_call(
            channel,
            user,
            "update_storage",
            candid::encode_args((UpdateChannelStorageInput {
                id,
                file_max_size: 1024 * 1024,
            },))
            .unwrap(),
        )
        .unwrap();
    for _ in 0..10 {
        ic.tick();
    }
    let status: FreezeStatus = query(&ic, channel, user, "legacy_status", ());
    assert_eq!(status.pending, 1);
    let cutover = ByteArray::new([8; 32]);
    update::<Result<FreezeStatus, String>>(&ic, channel, gov, "admin_legacy_drain", (cutover,))
        .unwrap();
    assert!(update::<Result<FreezeStatus, String>>(
        &ic,
        channel,
        gov,
        "admin_legacy_seal",
        (cutover,)
    )
    .is_err());
    let _: () = update(&ic, gate, gov, "release", ());
    let response = ic.await_call(pending).unwrap();
    candid::decode_one::<Result<Message, String>>(&response)
        .unwrap()
        .unwrap();
    for canister in [message, identity, profile, channel] {
        update::<Result<FreezeStatus, String>>(
            &ic,
            canister,
            gov,
            "admin_legacy_drain",
            (cutover,),
        )
        .unwrap();
        update::<Result<FreezeStatus, String>>(&ic, canister, gov, "admin_legacy_seal", (cutover,))
            .unwrap();
    }
    assert_eq!(
        update::<Result<ChannelInfo, String>>(&ic, channel, gov, "admin_create_channel", (create,))
            .unwrap_err(),
        "LegacyWriteDisabled"
    );
    assert_eq!(
        update::<Result<AddMessageOutput, String>>(
            &ic,
            channel,
            user,
            "add_message",
            (AddMessageInput {
                channel: id,
                payload: encrypted.into(),
                reply_to: None
            },)
        )
        .unwrap_err(),
        "LegacyWriteDisabled"
    );
    assert_eq!(
        update::<Result<(), String>>(
            &ic,
            profile,
            gov,
            "admin_upsert_profile",
            (user, None::<(Principal, u64)>)
        )
        .unwrap_err(),
        "LegacyWriteDisabled"
    );
    update::<Result<DownloadFilesToken, String>>(&ic, channel, user, "download_files_token", (id,))
        .unwrap();
    assert_eq!(
        query::<Result<ByteBuf, String>>(&ic, message, user, "my_iv", ()).unwrap(),
        iv_before
    );
    let snapshot: Result<SnapshotPage, String> = update(
        &ic,
        channel,
        user,
        "legacy_snapshot",
        (SnapshotScope::Messages(id), None::<String>),
    );
    let snapshot = snapshot.unwrap();
    assert!(!snapshot.complete);
    let gateway = ic.make_live(None);
    let root: String = ic
        .root_key()
        .unwrap()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let kek_hex: String = kek.iter().map(|b| format!("{b:02x}")).collect();
    let dek_hex: String = dek.iter().map(|b| format!("{b:02x}")).collect();
    let config = format!(
        r#"{{"test_only":true,"gateway":"{gateway}","root_key_hex":"{root}","message":"{message}","identity":"{identity}","profile":"{profile}","channel":"{channel}","channel_id":{id},"clean_channel_id":{clean_id},"kek_hex":"{kek_hex}","dek_hex":"{dek_hex}","messages":{}}}"#,
        snapshot.count
    );
    std::fs::write(output.join("fixture.json"), config).unwrap();
    let started = Instant::now();
    while !output.join("stop").exists() && started.elapsed() < Duration::from_secs(300) {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(output.join("stop").exists());
    ic.stop_live();
}
