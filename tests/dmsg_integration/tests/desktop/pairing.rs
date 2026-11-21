use super::*;

#[test]
#[ignore = "needs DMSG_PRE_RECOVERY_USER_WASM, the daemon, PocketIC and the private relay"]
fn legacy_home_pairing_rotates_the_root_and_unlocks_the_new_device() {
    let env = Env::launch(
        "pairing-legacy-home",
        Options {
            pre_policy_user: true,
            ..Options::default()
        },
    );
    let (a, b, http) = (&env.a, &env.b, &env.http);
    a.login(http, 41).unwrap();
    assert_eq!(a.run(&["account", "create"])["root_generation"], 1);
    assert_eq!(a.run(&["account", "security"])["recovery_enabled"], true);
    a.run(&["vault", "add", "--title", "before-device-approval"]);
    a.run(&["sync"]);
    assert_eq!(b.login(http, 41).unwrap()["state"], "pairing_required");
    let pairing = b.run(&["device", "pair-request"]);
    assert_eq!(a.approve_pairing(http, &pairing)["state"], "done");
    let account = a.run(&["account", "show"]);
    assert_eq!(account["root_generation"], 2);
    assert_eq!(account["vault_write_state"], "Ready");
    assert_eq!(b.run(&["unlock"])["state"], "unlocked");
    b.run(&["sync"]);
    assert!(titles(b).contains(&"before-device-approval".into()));
    env.finish();
}

#[test]
#[ignore = "also needs DMSG_BROKEN_PAIRING_BIN from the affected desktop release"]
fn legacy_home_recovers_a_device_added_before_its_v3_rotation_failed() {
    let mut env = Env::launch(
        "pairing-interrupted-legacy-home",
        Options {
            pre_policy_user: true,
            ..Options::default()
        },
    );
    env.a.login(&env.http, 41).unwrap();
    env.a.run(&["account", "create"]);
    env.a
        .run(&["vault", "add", "--title", "before-interrupted-pairing"]);
    env.a.run(&["sync"]);
    assert_eq!(
        env.b.login(&env.http, 41).unwrap()["state"],
        "pairing_required"
    );
    let pairing = env.b.run(&["device", "pair-request"]);

    let fixed = env.a.bin.clone();
    env.a.bin =
        PathBuf::from(std::env::var_os("DMSG_BROKEN_PAIRING_BIN").expect("affected release"));
    env.restart(0);
    env.a.login(&env.http, 41).unwrap();
    let request = env.a.run(&[
        "device",
        "approve",
        pairing["token"].as_str().unwrap(),
        "--device",
        pairing["device_id"].as_str().unwrap(),
    ]);
    assert_eq!(
        env.a.decide(&env.http, &request, true).unwrap_err(),
        "INTEGRITY_FAILED"
    );
    let partial = env.a.run(&["account", "show"]);
    assert_eq!(partial["devices"].as_array().unwrap().len(), 2);
    assert_eq!(partial["root_generation"], 1);
    assert_eq!(partial["vault_write_state"], "RekeyRequired");
    // The old release no longer retries the failed approval, but its saved
    // root job blocks idle-only updates indefinitely, even while unlocked.
    for _ in 0..2 {
        let stopped = env
            .a
            .rpc(&env.http, "daemon/stop", json!({"if_idle": true}))
            .unwrap();
        assert_eq!(stopped["stopped"], false);
        assert_eq!(stopped["in_flight"]["root_rotation"], true);
        assert_eq!(stopped["in_flight"]["approvals"], 0);
    }
    assert_eq!(env.b.run(&["unlock"])["state"], "root_required");
    // A real lock loses the login, whereas waiting for the root does not.
    env.b.run(&["lock"]);
    assert_eq!(env.b.fail(&["unlock"]), "AUTH_REQUIRED");
    assert_eq!(
        env.b.login(&env.http, 41).unwrap()["state"],
        "root_required"
    );

    // The desktop's explicit restart choice uses this graceful stop. The
    // journal must survive so the new release finishes the original approval.
    env.a.run(&["stop"]);
    env.a.bin = fixed;
    env.restart(0);
    env.a.login(&env.http, 41).unwrap();
    env.a
        .rpc(&env.http, "approval/reconcile", Value::Null)
        .unwrap();
    eventually(30, || {
        (approval_state(&env.a, &request) == "done").then_some(())
    });
    let repaired = env.a.run(&["account", "show"]);
    assert_eq!(
        repaired["devices"].as_array().unwrap().len(),
        2,
        "do not enroll again"
    );
    assert_eq!(repaired["root_generation"], 2, "reuse the live reservation");
    assert_eq!(repaired["vault_write_state"], "Ready");
    assert_eq!(env.b.run(&["unlock"])["state"], "unlocked");
    env.b.run(&["sync"]);
    assert!(titles(&env.b).contains(&"before-interrupted-pairing".into()));
    assert_eq!(
        env.a
            .rpc(&env.http, "daemon/stop", json!({"if_idle": true}))
            .unwrap()["stopped"],
        true,
        "the recovered root must no longer block future updates"
    );
    env.finish();
}

/// Exercise the two separate commits in a device join, using a counted
/// hardware provider in place of a system prompt.
#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn waiting_for_the_pairing_root_opens_the_device_hardware_once() {
    use dmsg_client::{
        Client, codec,
        crypto::Key,
        secure::{Presence, PresenceKey},
        testing,
    };
    use dmsg_types::{
        Error, Hash, Result,
        user::{AccountMutation, ControllerRole},
    };
    use k256::elliptic_curve::zeroize::Zeroizing;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Default)]
    struct Phone(AtomicUsize);

    impl PresenceKey for Phone {
        fn presence(&self) -> Option<Presence> {
            Some(Presence::Password)
        }

        fn create(&self, _: bool) -> Result<(Vec<u8>, Vec<u8>, Zeroizing<Key>)> {
            Ok((vec![11], vec![12], Zeroizing::new([13; 32])))
        }

        fn open(&self, handle: &[u8], context: &[u8], _: bool) -> Result<Zeroizing<Key>> {
            assert_eq!((handle, context), (&[11][..], &[12][..]));
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(Zeroizing::new([13; 32]))
        }

        fn biometry_state(&self) -> Option<Hash> {
            None
        }

        fn confirm(&self, _: &str) -> Result<()> {
            panic!("unexpected approval on the joining device")
        }
    }

    #[derive(serde::Deserialize, serde::Serialize)]
    struct Packet {
        format: String,
        home: Principal,
        request: AccountMutation,
    }

    let env = Env::start("pairing-root-single-unlock", "");
    let mut config = testing::config(false);
    config.ic_host = env.deployment.gateway.trim_end_matches('/').to_owned();
    config.handle = env.deployment.handle;
    config.user_homes = vec![env.deployment.user];
    config.root_key = Some(hex::decode(&env.deployment.root_key_hex).unwrap());
    config.relay = env.relay.clone();
    let mut admin = Client::open(config.clone(), &env.dir.join("administrator"), None).unwrap();
    let mut phone = Client::open(
        config,
        &env.dir.join("joining"),
        Some(Arc::new(testing::MemoryStorage::default())),
    )
    .unwrap();
    let hardware = Arc::new(Phone::default());
    phone.set_presence_key(hardware.clone());
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let login = testing::login(&mut admin, 41).await;
        admin.login(login).await.unwrap();
        admin.initialize_account().await.unwrap();
        let login = testing::login(&mut phone, 41).await;
        assert_eq!(phone.login(login).await.unwrap().state, "pairing_required");
        let token = phone
            .pair_request(ControllerRole::Administrator)
            .await
            .unwrap();
        assert!(phone.prepare_pairing_refresh().await.unwrap().is_none());
        assert_eq!(hardware.0.load(Ordering::SeqCst), 0);

        let preview =
            dmsg_client::approval_key::inspect_device_request(&admin.cooperative_hold(), &token)
                .await
                .unwrap();
        assert_eq!(preview["route"], "approve");
        admin
            .mutate(
                dmsg_types::user::AccountCommand::SetRecoveryDelay {
                    delay_ms: 4 * dmsg_types::DAY,
                },
                None,
            )
            .await
            .unwrap();
        let stale =
            dmsg_client::approval_key::inspect_device_request(&admin.cooperative_hold(), &token)
                .await
                .unwrap();
        assert_eq!(stale["route"], "refresh");
        let result = dmsg_client::approval::add_device_request(
            &admin.cooperative_hold(),
            "gui",
            &token,
            preview["device_id"].as_str().unwrap(),
        )
        .await;
        assert_eq!(result, Err(Error::VersionConflict));
        assert_eq!(admin.approvals_waiting().unwrap(), 0);
        let token = phone
            .pair_request(ControllerRole::Administrator)
            .await
            .unwrap();
        let packet: Packet = codec::decode(&codec::unb64(&token).unwrap(), 4096).unwrap();
        let request = packet.request;
        admin
            .mutate(
                request.command,
                Some((request.approval.request_id, request.expected_version)),
            )
            .await
            .unwrap();
        let done =
            dmsg_client::approval_key::inspect_device_request(&admin.cooperative_hold(), &token)
                .await
                .unwrap();
        assert_eq!(done["route"], "done");
        let prepared = phone.prepare_pairing_refresh().await.unwrap().unwrap();
        let answer = prepared.open();
        assert_eq!(
            phone
                .complete_login_unlock(prepared, answer)
                .await
                .unwrap()
                .state,
            "root_required"
        );
        for _ in 0..2 {
            assert!(phone.prepare_pairing_refresh().await.unwrap().is_none());
            assert_eq!(phone.status().unwrap().state, "root_required");
        }
        admin.rotate_root().await.unwrap();
        assert!(phone.prepare_pairing_refresh().await.unwrap().is_none());
        assert_eq!(phone.status().unwrap().state, "unlocked");
        assert_eq!(hardware.0.load(Ordering::SeqCst), 1);
        phone.lock();
        assert_eq!(phone.unlock_fast().await.unwrap().state, "unlocked");
        assert_eq!(hardware.0.load(Ordering::SeqCst), 2);
    });
    env.finish();
}
