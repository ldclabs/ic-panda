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
    assert_eq!(env.b.run(&["unlock"])["state"], "root_required");
    // A real lock loses the login, whereas waiting for the root does not.
    env.b.run(&["lock"]);
    assert_eq!(env.b.fail(&["unlock"]), "AUTH_REQUIRED");
    assert_eq!(
        env.b.login(&env.http, 41).unwrap()["state"],
        "root_required"
    );

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
    env.finish();
}
