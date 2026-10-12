//! Disposable user/COSE gateway for real extension account initialization tests.
mod support;

use candid::Principal;
use dmsg_types::{integration::*, integration_billing::*, user::*, *};
use pocket_ic::PocketIcBuilder;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[test]
#[ignore = "live local gateway; stopped by the extension test harness"]
fn account_extension_gateway() {
    let output =
        PathBuf::from(std::env::var_os("DMSG_CLOUD_FIXTURE_DIR").expect("fixture directory"));
    let mut deployment = support::Deployment::start();
    let (user, commerce, ledger, ledger_usdt, peer) = (
        deployment.user,
        deployment.commerce,
        deployment.ledger,
        deployment.ledger_usdt,
        deployment.peer,
    );
    std::fs::write(output.join("fixture.json.tmp"), deployment.fixture_json()).unwrap();
    std::fs::rename(output.join("fixture.json.tmp"), output.join("fixture.json")).unwrap();
    let ic = &mut deployment.ic;
    let started = Instant::now();
    let mut manual = false;
    let mut commerce_clock = false;
    let mut application_registered = false;
    while !output.join("stop").exists() && started.elapsed() < Duration::from_secs(300) {
        if !application_registered && output.join("cloud-launch.json").exists() {
            // dMsg's own operations use https://dmsg.net (plan D-19); an
            // extension origin is no longer a valid application origin.
            let origin = "https://dmsg.net".to_string();
            let app = AppRegistration {
                version: 1,
                environment: Environment::Local,
                app_id: "dmsg".into(),
                config_version: 1,
                origins: vec![origin],
                product_ids: vec!["dmsg".into()],
                capabilities: vec![AppCapability::Checkout],
                profiles: vec![],
                authentication_receiver: user,
                action_authority: user,
                action_schema: None,
                paused: false,
            };
            let reply = ic
                .update_call(
                    commerce,
                    peer,
                    "register_integration_app",
                    candid::encode_args((app,)).unwrap(),
                )
                .unwrap();
            candid::decode_one::<Result<()>>(&reply).unwrap().unwrap();
            for ledger in [ledger, ledger_usdt] {
                let reply = ic
                    .update_call(
                        commerce,
                        peer,
                        "publish_settlement_price",
                        candid::encode_args((ledger, 1_000_000u128, 30 * MINUTE)).unwrap(),
                    )
                    .unwrap();
                candid::decode_one::<Result<SettlementAsset>>(&reply)
                    .unwrap()
                    .unwrap();
            }
            application_registered = true;
            std::fs::write(output.join("commerce-ready.json"), "{}").unwrap();
        }
        if !commerce_clock && output.join("advance-commerce-clock").exists() {
            ic.stop_progress();
            ic.advance_time(Duration::from_millis(
                dmsg_types::integration::PANDA_COOLING_MS + 10_000,
            ));
            ic.tick();
            let at = nanos_to_millis(ic.get_time().as_nanos_since_unix_epoch());
            std::fs::write(
                output.join("commerce-clock.json"),
                format!("{{\"at\":{at}}}"),
            )
            .unwrap();
            manual = true;
            commerce_clock = true;
        }
        if !manual && output.join("advance-recovery-clock").exists() {
            ic.stop_progress();
            ic.advance_time(Duration::from_secs(3 * 86_400 + 1));
            ic.tick();
            let at = nanos_to_millis(ic.get_time().as_nanos_since_unix_epoch());
            std::fs::write(output.join("clock.json.tmp"), format!("{{\"at\":{at}}}")).unwrap();
            std::fs::rename(output.join("clock.json.tmp"), output.join("clock.json")).unwrap();
            manual = true;
        }
        if manual {
            ic.advance_time(Duration::from_millis(100));
            ic.tick();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(output.join("stop").exists(), "account probe timeout");
    ic.stop_live();
}

#[test]
#[ignore = "requires candidate user Wasm"]
fn certificate_queries_advance_without_account_writes() {
    let ic = PocketIcBuilder::new().with_application_subnet().build();
    let user = ic.create_canister();
    ic.add_cycles(user, 10_000_000_000_000);
    let dir = std::env::var_os("DMSG_WASM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/wasm32-unknown-unknown/release")
        });
    ic.install_canister(
        user,
        std::fs::read(dir.join("dmsg_user.wasm")).unwrap(),
        candid::encode_one(UserInit {
            issuer_namespace: "https://dmsg.test/u/".into(),
            environment: Environment::Local,
            home_cose: user,
            handle_canister: user,
            payment_canister: user,
            commerce_canister: user,
            membership_canister: user,
            max_accounts: 2,
            daily_new_accounts: 2,
            principal_origin: "https://id.dmsg.test".into(),
            directory_canister: user,
            governance: user,
            admission_key: None,
        })
        .unwrap(),
        None,
    );
    let query = || {
        let reply = ic
            .query_call(
                user,
                Principal::anonymous(),
                "security_snapshot_batch",
                candid::encode_one(vec![AccountId([8; 12])]).unwrap(),
            )
            .unwrap();
        candid::decode_one::<Result<CertifiedBatch>>(&reply)
            .unwrap()
            .unwrap()
    };
    let first = query();
    ic.advance_time(Duration::from_secs(2));
    ic.tick();
    let second = query();
    assert_eq!(
        first.entries, second.entries,
        "account state/witness must remain unchanged"
    );
    assert_ne!(
        first.certificate, second.certificate,
        "certificate queries must depend on current batch time, not cached caller/args alone"
    );
}
