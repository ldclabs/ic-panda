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

#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn one_invitation_binds_a_new_login_then_enrolls_its_device() {
    let mut env = Env::start("invited-login", "");
    let (same, _) = env.desktop("same-login");
    let (other, _) = env.desktop("other-account");
    let (a, phone, http) = (&env.a, &env.b, &env.http);
    a.login(http, 41).unwrap();
    let account = a.run(&["account", "create"])["account"].clone();
    a.run(&["vault", "add", "--title", "desktop vault"]);
    a.run(&["sync"]);
    let invitation = a
        .rpc(http, "device/pair_invite", json!({"with_login":true}))
        .unwrap();
    let token = &invitation["token"];
    assert!(token.as_str().unwrap().starts_with("dmsg-device-invite/3:"));
    let read = |who: &Desktop| {
        who.rpc(http, "device/join_invitation", json!({"invitation":token}))
            .unwrap()
    };
    let principal = login_principal(42);
    let send = |bind| {
        phone
            .rpc(
                http,
                "device/join_invitation",
                json!({"invitation":token,"principal":principal,"bind_login":bind}),
            )
            .unwrap()
    };
    let candidate = || {
        a.rpc(
            http,
            "device/pair_invite_status",
            json!({"id":invitation["id"]}),
        )
        .unwrap()["candidate"]
            .clone()
    };

    assert_eq!(read(phone)["stage"], "sign_in");
    let existing = phone.login(http, 41).unwrap();
    assert_eq!(existing["state"], "pairing_required");
    assert_eq!(phone.login(http, 42).unwrap()["state"], "no_account");
    let preview = read(phone);
    assert_eq!(preview["account"], account);
    assert_eq!(preview["stage"], "bind_login");
    assert_eq!(preview["device_id"], existing["device_id"]);
    assert_eq!(send(false)["stage"], "bind_login");
    assert!(candidate().is_null());
    other.login(http, 43).unwrap();
    let separate = other.run(&["account", "create"])["account"].clone();
    assert_eq!(read(&other)["stage"], "other_account");
    assert_eq!(read(&other)["current_account"], separate);
    same.login(http, 41).unwrap();
    assert_eq!(read(&same)["stage"], "device");

    let first = send(true);
    assert_eq!(first["stage"], "waiting_login");
    assert_eq!(send(false), first);
    assert_eq!(
        &phone
            .rpc(http, "device/saved_invitation", Value::Null)
            .unwrap(),
        token
    );
    let login = candidate();
    assert_eq!(login["code"], first["code"]);
    assert_eq!(login["principal"], principal);
    let request = a
        .rpc(
            http,
            "account/bind_approve",
            json!({"token":login["token"],"principal":principal}),
        )
        .unwrap();
    assert_eq!(a.decide(http, &request, true).unwrap()["state"], "done");
    let until = Instant::now() + Duration::from_secs(35);
    let second = loop {
        let view = send(false);
        if view["stage"] == "waiting_device" {
            break view;
        }
        assert!(Instant::now() < until, "{view}");
        std::thread::sleep(Duration::from_millis(250));
    };
    assert_eq!(second["code"], first["code"]);
    let device = candidate();
    assert_eq!(device["code"], first["code"]);
    assert_eq!(device["device_id"], login["device_id"]);
    assert_eq!(a.run(&["device", "list"]).as_array().unwrap().len(), 1);
    assert_eq!(a.approve_pairing(http, &device)["state"], "done");
    assert_eq!(
        phone
            .rpc(http, "session/unlock", json!({"pairing_poll":true}))
            .unwrap()["account"],
        account
    );
    phone.run(&["sync"]);
    assert!(titles(phone).contains(&"desktop vault".to_owned()));
    assert_eq!(other.run(&["status"])["account"], separate);
    // Use the other account for a separate two-device pair within the Free quota.
    same.login(http, 43).unwrap();
    let legacy = other.rpc(http, "device/pair_invite", Value::Null).unwrap();
    let legacy_preview = same
        .rpc(
            http,
            "device/join_invitation",
            json!({"invitation":legacy["token"]}),
        )
        .unwrap();
    assert_eq!(legacy_preview["stage"], "device");
    assert!(legacy_preview["device_id"].is_string());
    assert!(legacy_preview.get("code").is_none());
    let direct = other
        .rpc(http, "device/pair_invite", json!({"with_login":true}))
        .unwrap();
    let submitted = same
        .rpc(
            http,
            "device/join_invitation",
            json!({"invitation":direct["token"],"principal":login_principal(43)}),
        )
        .unwrap();
    assert_eq!(submitted["stage"], "waiting_device");
    let device = other
        .rpc(
            http,
            "device/pair_invite_status",
            json!({"id":direct["id"]}),
        )
        .unwrap()["candidate"]
        .clone();
    assert_eq!(device["code"], submitted["code"]);
    assert_eq!(other.approve_pairing(http, &device)["state"], "done");
    assert_eq!(same.run(&["unlock"])["account"], separate);
    env.finish();
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn mobile_approves_a_desktop_with_a_new_login() {
    phone_to_desktop(false);
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn phone_camera_scans_both_requests_on_a_new_desktop() {
    phone_to_desktop(true);
}

fn scan_view_qr(view: &Value) -> String {
    let png = base64::engine::general_purpose::STANDARD
        .decode(
            view["qr"]
                .as_str()
                .unwrap()
                .strip_prefix("data:image/png;base64,")
                .unwrap(),
        )
        .unwrap();
    let token = dmsg_client::qr::scan(&png).unwrap();
    assert_eq!(token, view["token"]);
    token
}

fn phone_to_desktop(phone_scans: bool) {
    use dmsg_client::{Client, testing, vault::Item};
    use dmsg_mobile::{MobileEngine, OperationToken};
    let env = Env::start("phone-invites-desktop", "");
    let desktop = &env.b;
    let mut config = testing::config(false);
    config.ic_host = env.deployment.gateway.trim_end_matches('/').into();
    config.handle = env.deployment.handle;
    config.user_homes = vec![env.deployment.user];
    config.root_key = Some(hex::decode(&env.deployment.root_key_hex).unwrap());
    config.relay = env.relay.clone();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (phone, account) = rt.block_on(async {
        let mut client = Client::open(config, &env.dir.join("phone"), None).unwrap();
        client.require_prepared_approval();
        let login = testing::login(&mut client, 51).await;
        client.login(login).await.unwrap();
        let (created, _) = client.initialize_account().await.unwrap();
        client
            .vault_add_all(vec![Item::new("note", "from the phone", now_ms())])
            .unwrap();
        client.sync().await.unwrap();
        (
            MobileEngine::integration_fixture(client, env.dir.join("phone-staging")),
            created.id.to_string(),
        )
    });
    let scoped = || phone.scoped(phone.control_generation()).unwrap();
    let value = |snapshot: dmsg_mobile::ObjectSnapshot| {
        serde_json::from_str::<Value>(&snapshot.json).unwrap()
    };
    let decide = |review: Value| {
        let done = rt
            .block_on(scoped().decide(
                review["id"].as_str().unwrap().into(),
                review["digest"].as_str().unwrap().into(),
                true,
                OperationToken::new(),
            ))
            .unwrap();
        assert_eq!(value(done)["state"], "done");
    };
    desktop.login(&env.http, 52).unwrap();
    let mut invite_id = None;
    let mut shown_code = None;
    let (raw, invitation) = if phone_scans {
        let binding = desktop
            .rpc(
                &env.http,
                "account/bind_request",
                json!({"account":account}),
            )
            .unwrap();
        (scan_view_qr(&binding), None)
    } else {
        let invite = value(rt.block_on(scoped().start_account_invitation()).unwrap());
        let id = invite["id"].as_str().unwrap().to_string();
        let token = scan_view_qr(&invite);
        let args = json!({"invitation":token,"principal":login_principal(52),"bind_login":true});
        let first = desktop
            .rpc(&env.http, "device/join_invitation", args.clone())
            .unwrap();
        assert_eq!(first["stage"], "waiting_login");
        let status = value(
            rt.block_on(scoped().account_invitation_status(id.clone()))
                .unwrap(),
        );
        let candidate = &status["candidate"];
        assert_eq!(candidate["code"], first["code"]);
        invite_id = Some(id);
        shown_code = Some(first["code"].clone());
        (candidate["token"].as_str().unwrap().to_string(), Some(args))
    };
    let scanned = value(
        rt.block_on(scoped().inspect_pairing_request(raw.clone()))
            .unwrap(),
    );
    assert_eq!(scanned["kind"], "login_binding");
    assert_eq!(scanned["principal"], login_principal(52));
    decide(value(rt.block_on(scoped().review_binding(raw)).unwrap()));
    let device = if let Some(args) = invitation {
        eventually(30, || {
            (desktop
                .rpc(&env.http, "device/join_invitation", args.clone())
                .unwrap()["stage"]
                == "waiting_device")
                .then_some(())
        });
        let status = value(
            rt.block_on(scoped().account_invitation_status(invite_id.clone().unwrap()))
                .unwrap(),
        );
        let device = status["candidate"].clone();
        assert_eq!(device["code"], shown_code.unwrap());
        device
    } else {
        assert_eq!(
            desktop
                .rpc(&env.http, "account/bind_accept", json!({"account":account}))
                .unwrap()["state"],
            "pairing_required"
        );
        let device = desktop.run(&["device", "pair-request"]);
        let raw = scan_view_qr(&device);
        let preview = value(rt.block_on(scoped().inspect_pairing_request(raw)).unwrap());
        assert_eq!(preview["device_id"], device["device_id"]);
        device
    };
    decide(value(
        rt.block_on(scoped().approve_pairing_request(
            device["token"].as_str().unwrap().into(),
            device["device_id"].as_str().unwrap().into(),
        ))
        .unwrap(),
    ));
    assert_eq!(desktop.run(&["unlock"])["account"], account);
    desktop.run(&["sync"]);
    assert!(titles(desktop).contains(&"from the phone".to_string()));
    if let Some(id) = invite_id {
        rt.block_on(scoped().cancel_account_invitation(id)).unwrap();
    }
    env.finish();
}
