//! Public name reads through the real daemon and canisters, without a relay.
use super::*;
use dmsg_types::{AccountId, agent::*, user::UserInit};

#[test]
#[ignore = "needs the dmsg binary, dMsg Wasm and PocketIC"]
fn names_survive_partial_home_and_directory_failures() {
    let deployment = support::Deployment::start();
    let ic = &deployment.ic;
    let query = |canister, method: &str| {
        ic.query_call(
            canister,
            Principal::anonymous(),
            method,
            candid::encode_args(()).unwrap(),
        )
        .unwrap()
    };
    let mut user: UserInit = candid::decode_one(&query(deployment.user, "user_config")).unwrap();
    let mut directory: DirectoryInit =
        candid::decode_one(&query(deployment.directory, "directory_config")).unwrap();
    let second_home = ic.create_canister();
    let second_directory = ic.create_canister();
    user.directory_canister = second_directory;
    directory.user_homes = vec![second_home];
    let wasm_dir = std::env::var_os("DMSG_WASM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/wasm32-unknown-unknown/release")
        });
    for (canister, wasm, args) in [
        (second_home, "dmsg_user", candid::encode_one(&user).unwrap()),
        (
            second_directory,
            "dmsg_directory",
            candid::encode_one(&directory).unwrap(),
        ),
    ] {
        ic.add_cycles(canister, 10_000_000_000_000_000);
        ic.install_canister(
            canister,
            std::fs::read(wasm_dir.join(format!("{wasm}.wasm"))).unwrap(),
            args,
            None,
        );
    }
    let added: dmsg_types::Result<()> = candid::decode_one(
        &ic.update_call(
            deployment.handle,
            deployment.peer,
            "admin_add_user_home",
            candid::encode_one(second_home).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    added.unwrap();

    let account = |home, index| {
        let mut bytes = [0; 12];
        bytes[4..9].copy_from_slice(
            &dmsg_protocol::agent::account_allocator_digest(
                &user.environment,
                &user.issuer_namespace,
                home,
            )[..5],
        );
        bytes[11] = index;
        AccountId(bytes)
    };
    let publish = |home, directory, account: AccountId, name: &str| {
        // PocketIC impersonates the account's home; the directory still
        // validates its routing, metadata and authenticated document bytes.
        let state = PrincipalState {
            principal_type: PrincipalType::Person,
            version: 1,
            updated_at: now_ms(),
            controllers: vec![],
            metadata: serde_json::to_vec(&json!({"name": name})).unwrap().into(),
        };
        let result: dmsg_types::Result<Publication> = candid::decode_one(
            &ic.update_call(
                directory,
                home,
                "publish",
                candid::encode_args((account, state)).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        result.unwrap();
    };
    let cached = account(second_home, 0);
    publish(second_home, second_directory, cached, "Cached name");

    let dir = std::env::temp_dir().join(format!("dmsg-partial-names-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("config.toml"),
        format!(
            "environment = \"local\"\nic_host = \"{}\"\nrelay = \"http://127.0.0.1:9\"\nhandle = \"{}\"\nuser_homes = [\"{}\"]\nroot_key = \"{}\"\nsecure_storage = \"file\"\n",
            deployment.gateway, deployment.handle, deployment.user, deployment.root_key_hex,
        ),
    )
    .unwrap();
    let desktop = Desktop {
        bin: std::env::var_os("DMSG_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/dmsg")
            }),
        home: dir.clone(),
    };
    let mut children = Children(vec![]);
    desktop.spawn(&mut children);
    let http = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let names = |accounts: &[AccountId]| {
        desktop
            .rpc(
                &http,
                "channel/names",
                json!({"accounts": accounts.iter().map(ToString::to_string).collect::<Vec<_>>()}),
            )
            .unwrap()
    };
    assert_eq!(names(&[cached]), json!({cached.to_string(): "Cached name"}));

    for (index, stopped) in [(1, second_home), (2, second_directory)] {
        let healthy = account(deployment.user, index);
        let failed = account(second_home, index);
        let absent = account(deployment.user, index + 10);
        publish(
            deployment.user,
            deployment.directory,
            healthy,
            "Healthy name",
        );
        publish(second_home, second_directory, failed, "Recovered name");
        ic.stop_canister(stopped, None).unwrap();
        // A failed config read must not discard either fresh names from
        // another home or previously cached names from the affected home.
        assert_eq!(
            names(&[failed, healthy, cached, absent, healthy]),
            json!({
                healthy.to_string(): "Healthy name",
                cached.to_string(): "Cached name",
                absent.to_string(): null,
            }),
        );
        ic.start_canister(stopped, None).unwrap();
        // Transient failures are omitted, not cached as absent: recovery
        // is visible on the next call without waiting for the missing TTL.
        assert_eq!(
            names(&[failed, healthy, cached]),
            json!({
                failed.to_string(): "Recovered name",
                healthy.to_string(): "Healthy name",
                cached.to_string(): "Cached name",
            }),
        );
    }
    drop(children);
    std::fs::remove_dir_all(dir).unwrap();
}
