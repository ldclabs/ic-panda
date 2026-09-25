//! Disposable user/COSE gateway for real extension account initialization tests.
use candid::Principal;
use dmsg_types::{cose::*, user::*, *};
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
    let mut ic = PocketIcBuilder::new()
        .with_nns_subnet()
        .with_application_subnet()
        .with_fiduciary_subnet()
        .build();
    let subnet = ic.topology().get_app_subnets()[0];
    let user = ic.create_canister_on_subnet(None, None, subnet);
    let cose = ic.create_canister_on_subnet(None, None, subnet);
    let peer = ic.create_canister_on_subnet(None, None, subnet);
    let commerce = ic.create_canister_on_subnet(None, None, subnet);
    let handle = ic.create_canister_on_subnet(None, None, subnet);
    let ledger = ic.create_canister_on_subnet(None, None, subnet);
    let ledger_usdt = ic.create_canister_on_subnet(None, None, subnet);
    let membership = ic.create_canister_on_subnet(None, None, subnet);
    let sns = ic.create_canister_on_subnet(None, None, subnet);
    let payment = ic.create_canister_on_subnet(None, None, subnet);
    for id in [
        user,
        cose,
        commerce,
        handle,
        ledger,
        ledger_usdt,
        membership,
        sns,
        payment,
    ] {
        ic.add_cycles(id, 10_000_000_000_000_000);
    }
    let dir = std::env::var_os("DMSG_WASM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/wasm32-unknown-unknown/release")
        });
    ic.install_canister(
        user,
        std::fs::read(dir.join("dmsg_user.wasm")).unwrap(),
        candid::encode_args((UserInit {
            issuer_namespace: "https://dmsg.test/u/".into(),
            environment: Environment::Local,
            home_cose: cose,
            handle_canister: handle,
            payment_canister: payment,
            commerce_canister: commerce,
            membership_canister: membership,
            max_accounts: 20,
            daily_new_accounts: 20,
        },))
        .unwrap(),
        None,
    );
    ic.install_canister(
        cose,
        std::fs::read(dir.join("dmsg_cose.wasm")).unwrap(),
        candid::encode_args((CoseInit {
            issuer_namespace: "https://dmsg.test/u/".into(),
            environment: Environment::Local,
            executing_canister: cose,
            initial_home_user: user,
            derivation_version: 2,
            masters: vec![
                MasterKey {
                    algorithm: Algorithm::Ed25519,
                    key_name: "key_1".into(),
                    expected_fingerprint: Hash::new([0; 32]),
                },
                MasterKey {
                    algorithm: Algorithm::VetKdBls12381,
                    key_name: "key_1".into(),
                    expected_fingerprint: Hash::new([0; 32]),
                },
            ],
            daily_executions: 1000,
            daily_cycles: 10_000_000_000_000,
        },))
        .unwrap(),
        None,
    );
    ic.install_canister(
        ledger,
        std::fs::read(dir.join("dmsg_test_ledger.wasm")).unwrap(),
        candid::encode_args(()).unwrap(),
        None,
    );
    ic.install_canister(
        ledger_usdt,
        std::fs::read(dir.join("dmsg_test_ledger.wasm")).unwrap(),
        candid::encode_args(()).unwrap(),
        None,
    );
    ic.install_canister(
        sns,
        std::fs::read(dir.join("dmsg_test_sns.wasm")).unwrap(),
        candid::encode_args(()).unwrap(),
        None,
    );
    ic.install_canister(
        payment,
        std::fs::read(dir.join("dmsg_payment.wasm")).unwrap(),
        candid::encode_one(dmsg_types::payment::PaymentInit {
            ledger,
            home_user: user,
            platform: icrc_ledger_types::icrc1::account::Account {
                owner: peer,
                subaccount: None,
            },
            governance: peer,
            fee_policy: dmsg_types::payment::DeliveryFeePolicy {
                version: 1,
                effective_at_ms: 0,
                rate_bps: 500,
                minimum_atomic: 100,
            },
            ledger_fee: 10,
            max_fee: 20,
            signer: dmsg_types::payment::ReceiptSigner {
                epoch: 1,
                public_key: Hash::new(
                    ed25519_dalek::SigningKey::from_bytes(&[100; 32])
                        .verifying_key()
                        .to_bytes(),
                ),
                valid_from: 0,
                valid_until: u64::MAX / 2,
                revoked: false,
            },
            max_open_per_payer: 4,
            daily_orders: 100,
            enabled: true,
        })
        .unwrap(),
        None,
    );
    let mut plans = dmsg_protocol::billing::default_plans(1);
    // Synthetic local catalog: a just-created account must have a nonzero
    // prorated allowance at any point in the calendar month.
    plans[0].limits.monthly_execution_units = 1000;
    ic.install_canister(
        commerce,
        std::fs::read(dir.join("dmsg_commerce.wasm")).unwrap(),
        candid::encode_args((dmsg_types::billing::CommerceInit {
            environment: Environment::Local,
            governance: peer,
            membership_canister: membership,
            user_homes: vec![user],
            catalog: dmsg_types::billing::Catalog {
                schema: 1,
                version: 1,
                effective_at_ms: 0,
                plans: plans.clone(),
                storage_products: vec![],
                terms_digest: Hash::new([99; 32]),
            },
            max_subjects: 20,
            daily_orders: 20,
        },))
        .unwrap(),
        None,
    );
    ic.install_canister(
        membership,
        std::fs::read(dir.join("membership.wasm")).unwrap(),
        candid::encode_one(dmsg_types::membership::MembershipInit {
            environment: Environment::Local,
            governance: sns,
            sns_root: sns,
            panda_ledger: sns,
            expected_governance_module_hash: None,
        })
        .unwrap(),
        None,
    );
    let configured = ic
        .update_call(
            membership,
            sns,
            "verify_sns_configuration",
            candid::encode_args(()).unwrap(),
        )
        .unwrap();
    candid::decode_one::<Result<()>>(&configured)
        .unwrap()
        .unwrap();
    use dmsg_types::{integration::*, integration_billing::*, integration_membership::*};
    let call = |canister, caller, method: &str, args: Vec<u8>| {
        let reply = ic.update_call(canister, caller, method, args).unwrap();
        candid::decode_one::<Result<()>>(&reply).unwrap().unwrap();
    };
    call(
        membership,
        sns,
        "configure_panda_service",
        candid::encode_args((PandaServiceConfig {
            commerce_canister: commerce,
            max_claims: 100,
            hourly_applications: 100,
            cooling_ms: PANDA_COOLING_MS,
        },))
        .unwrap(),
    );
    let reply = ic
        .update_call(
            membership,
            sns,
            "schedule_panda_rate",
            candid::encode_args((PandaRatePolicy {
                version: 2,
                policy_version: 1,
                environment: Environment::Local,
                product_ids: vec!["dmsg".into()],
                r_num: 5000,
                r_den: 1,
                published_at_ms: 0,
                effective_at_ms: POLICY_NOTICE_MS,
            },))
            .unwrap(),
        )
        .unwrap();
    candid::decode_one::<Result<PandaRatePolicy>>(&reply)
        .unwrap()
        .unwrap();
    call(
        commerce,
        peer,
        "register_integration_product",
        candid::encode_args((ProductRegistration {
            version: 2,
            environment: Environment::Local,
            product_id: "dmsg".into(),
            config_version: 1,
            quote_authority: commerce,
            beneficiary_authority: user,
            adapter: commerce,
            subject_schema: "dmsg-account-v1".into(),
            subject_size: 12,
            merchant: peer.into(),
            ledgers: vec![ledger, ledger_usdt],
            terms_hash: Hash::new([99; 32]),
            paused: false,
        },))
        .unwrap(),
    );
    let at = nanos_to_millis(ic.get_time().as_nanos_since_unix_epoch());
    for (ledger, asset) in [
        (ledger, SettlementAssetKind::CkUsdc),
        (ledger_usdt, SettlementAssetKind::CkUsdt),
    ] {
        call(
            commerce,
            peer,
            "register_settlement_asset",
            candid::encode_args((SettlementAsset {
                version: 2,
                policy_version: 1,
                environment: Environment::Local,
                ledger,
                asset,
                decimals: 6,
                price_usd_micros: 1_000_000,
                price_observed_at_ms: at,
                price_valid_until_ms: at + 30 * MINUTE,
                network_fee_atomic: 10,
                max_network_fee_atomic: 20,
                enabled: true,
            },))
            .unwrap(),
        );
        call(
            commerce,
            peer,
            "verify_settlement_asset",
            candid::encode_args((ledger, None::<u128>)).unwrap(),
        );
    }
    ic.install_canister(
        handle,
        std::fs::read(dir.join("dmsg_handle.wasm")).unwrap(),
        candid::encode_one(dmsg_types::handle::HandleInit {
            home_user: user,
            ledger: peer,
            ledger_fee: 10,
            max_pending: 100,
        })
        .unwrap(),
        None,
    );
    // Synthetic old owner uses the same disposable II stand-in as the browser.
    let mut owner_der = vec![
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    owner_der.extend(
        ed25519_dalek::SigningKey::from_bytes(&[42; 32])
            .verifying_key()
            .to_bytes(),
    );
    let owner = Principal::self_authenticating(&owner_der);
    for ledger in [ledger, ledger_usdt] {
        ic.update_call(
            ledger,
            peer,
            "mint_test",
            candid::encode_args((
                icrc_ledger_types::icrc1::account::Account {
                    owner,
                    subaccount: None,
                },
                10_000_000_000u128,
            ))
            .unwrap(),
        )
        .unwrap();
    }
    #[derive(candid::CandidType, serde::Serialize)]
    struct NeuronId {
        id: Vec<u8>,
    }
    #[derive(candid::CandidType, serde::Serialize)]
    struct Permission {
        principal: Option<Principal>,
        permission_type: Vec<i32>,
    }
    #[derive(candid::CandidType, serde::Serialize)]
    enum DissolveState {
        DissolveDelaySeconds(u64),
    }
    #[derive(candid::CandidType, serde::Serialize)]
    struct Neuron {
        id: Option<NeuronId>,
        permissions: Vec<Permission>,
        cached_neuron_stake_e8s: u64,
        neuron_fees_e8s: u64,
        dissolve_state: Option<DissolveState>,
    }
    ic.update_call(
        sns,
        peer,
        "set_neuron",
        candid::encode_one(Some(Neuron {
            id: Some(NeuronId { id: vec![77; 32] }),
            permissions: vec![Permission {
                principal: Some(owner),
                permission_type: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
            }],
            cached_neuron_stake_e8s: 1_000_000_000_000_000,
            neuron_fees_e8s: 0,
            dissolve_state: Some(DissolveState::DissolveDelaySeconds(2 * 365 * 86400)),
        }))
        .unwrap(),
    )
    .unwrap();
    let entries = vec![dmsg_types::handle::LegacyReservation {
        handle: "legacy_probe".into(),
        legacy_owner: owner,
        legacy_name_principal: None,
        frozen_admins: vec![],
        quarantined: false,
    }];
    let entries_digest =
        dmsg_protocol::digest("dmsg/legacy-entry/v1", &(Hash::new([0; 32]), &entries[0]));
    let snapshot = dmsg_types::handle::LegacySnapshot {
        source_canister: peer,
        snapshot_id: Hash::new([53; 32]),
        freeze_version: 1,
        event_tip: Hash::new([54; 32]),
        count: 1,
        entries_digest,
    };
    for (method, args) in [
        (
            "begin_legacy_snapshot",
            candid::encode_args((snapshot.clone(),)).unwrap(),
        ),
        (
            "import_legacy_handles",
            candid::encode_args((snapshot.snapshot_id, entries)).unwrap(),
        ),
        ("seal_legacy_snapshot", candid::encode_args(()).unwrap()),
    ] {
        let reply = ic
            .update_call(handle, Principal::anonymous(), method, args)
            .unwrap();
        if method == "begin_legacy_snapshot" {
            candid::decode_one::<Result<()>>(&reply).unwrap().unwrap();
        } else {
            candid::decode_one::<Result<dmsg_types::handle::SnapshotProgress>>(&reply)
                .unwrap()
                .unwrap();
        }
    }
    let response = ic
        .update_call(
            cose,
            Principal::anonymous(),
            "initialize_keys",
            candid::encode_args(()).unwrap(),
        )
        .unwrap();
    let result: Result<KeyState> = candid::decode_one(&response).unwrap();
    result.unwrap();
    let gateway = ic.make_live(None);
    let root: String = ic
        .root_key()
        .unwrap()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let config = format!(
        r#"{{"test_only":true,"user":"{user}","cose":"{cose}","commerce":"{commerce}","handle":"{handle}","ledger":"{ledger}","ledger_usdt":"{ledger_usdt}","membership":"{membership}","sns":"{sns}","payment":"{payment}","platform":"{peer}","namespace":"https://dmsg.test/u/","gateway":"{gateway}","root_key_hex":"{root}"}}"#
    );
    std::fs::write(output.join("fixture.json.tmp"), config).unwrap();
    std::fs::rename(output.join("fixture.json.tmp"), output.join("fixture.json")).unwrap();
    let started = Instant::now();
    let mut manual = false;
    let mut commerce_clock = false;
    let mut application_registered = false;
    while !output.join("stop").exists() && started.elapsed() < Duration::from_secs(300) {
        if !application_registered && output.join("cloud-launch.json").exists() {
            let launch: serde_json::Value =
                serde_json::from_slice(&std::fs::read(output.join("cloud-launch.json")).unwrap())
                    .unwrap();
            let origin = launch["extensionOrigin"].as_str().unwrap().to_string();
            let app = AppRegistration {
                version: 1,
                environment: Environment::Local,
                app_id: "dmsg".into(),
                config_version: 1,
                origins: vec![origin],
                user_homes: vec![user],
                cose_homes: vec![cose],
                product_ids: vec!["dmsg".into()],
                capabilities: vec![AppCapability::Checkout],
                profiles: vec![],
                authentication_receiver: user,
                action_authority: user,
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
            ic.advance_time(Duration::from_secs(86_401));
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
