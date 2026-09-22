//! Live, local-only public user service for the extension/cloud interoperability probe.
use candid::Principal;
use dmsg_protocol::digest;
use dmsg_types::{user::*, *};
use ed25519_dalek::{Signer, SigningKey};
use pocket_ic::PocketIcBuilder;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[test]
#[ignore = "runs a live test gateway until the probe creates the stop file"]
fn cloud_extension_gateway() {
    let output =
        PathBuf::from(std::env::var_os("DMSG_CLOUD_FIXTURE_DIR").expect("fixture directory"));
    std::fs::create_dir_all(&output).unwrap();
    let mut ic = PocketIcBuilder::new()
        .with_nns_subnet()
        .with_application_subnet()
        .build();
    let user = ic.create_canister();
    let peer = ic.create_canister();
    ic.add_cycles(user, 10_000_000_000_000);
    let wasm_dir = std::env::var_os("DMSG_WASM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/wasm32-unknown-unknown/release")
        });
    ic.install_canister(
        user,
        std::fs::read(wasm_dir.join("dmsg_user.wasm")).unwrap(),
        candid::encode_args((UserInit {
            issuer_namespace: "https://dmsg.test/u/".into(),
            environment: Environment::Local,
            home_cose: peer,
            handle_canister: peer,
            payment_canister: peer,
            commerce_canister: peer,
            membership_canister: peer,
            max_accounts: 10,
            daily_new_accounts: 10,
        },))
        .unwrap(),
        None,
    );
    let signing = SigningKey::from_bytes(&[7; 32]);
    let caller = Principal::self_authenticating([7; 32]);
    let expires_at = nanos_to_millis(ic.get_time().as_nanos_since_unix_epoch()) + 60_000;
    let device = DeviceInput {
        device_id: Hash::new([7; 32]),
        signing_pub: Hash::new(signing.verifying_key().to_bytes()),
        hpke_pub: Hash::new([8; 32]),
        role: ControllerRole::Administrator,
        capabilities: vec![
            Capability::ContentSign,
            Capability::RootManage,
            Capability::VaultUnlock,
        ],
    };
    let op_id = Hash::new([9; 32]);
    let proof = signing
        .sign(
            digest(
                "dmsg/create-account/v1",
                &(user, caller, &device, op_id, expires_at),
            )
            .as_slice(),
        )
        .to_bytes()
        .to_vec()
        .into();
    let response = ic
        .update_call(
            user,
            caller,
            "create_account",
            candid::encode_args((CreateAccount {
                device,
                op_id,
                expires_at,
                proof,
            },))
            .unwrap(),
        )
        .unwrap();
    let account: Result<AccountId> = candid::decode_one(&response).unwrap();
    let account = account.unwrap();
    let gateway = ic.make_live(None);
    let root: String = ic
        .root_key()
        .unwrap()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let config = format!(
        r#"{{"test_only":true,"user":"{user}","account":"{account}","namespace":"https://dmsg.test/u/","gateway":"{gateway}","root_key_hex":"{root}","device_seed":7}}"#
    );
    std::fs::write(output.join("fixture.json.tmp"), config).unwrap();
    std::fs::rename(output.join("fixture.json.tmp"), output.join("fixture.json")).unwrap();
    let started = Instant::now();
    while !output.join("stop").exists() && started.elapsed() < Duration::from_secs(300) {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        output.join("stop").exists(),
        "cloud probe did not finish within five minutes"
    );
    ic.stop_live();
}
