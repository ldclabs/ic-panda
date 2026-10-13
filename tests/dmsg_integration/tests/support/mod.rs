//! The live local deployment of the desktop tests: the dMsg canisters, three
//! test ledgers and an SNS stand-in behind PocketIC's HTTP gateway. Only
//! generated test identities and seeds are used.
use candid::Principal;
use dmsg_types::{cose::*, user::*, *};
use pocket_ic::{PocketIc, PocketIcBuilder};
use std::path::PathBuf;

/// A running local deployment.
pub struct Deployment {
    pub ic: PocketIc,
    pub user: Principal,
    pub cose: Principal,
    pub commerce: Principal,
    pub handle: Principal,
    pub ledger: Principal,
    pub ledger_usdt: Principal,
    /// The ledger names are paid on, in place of PANDA.
    pub name_ledger: Principal,
    pub membership: Principal,
    pub sns: Principal,
    pub payment: Principal,
    /// The principal directory, serving `https://id.dmsg.test/<account>`.
    pub directory: Principal,
    pub peer: Principal,
    pub gateway: String,
    pub root_key_hex: String,
    /// Declared after `ic`, so the instance is deleted before the server stops.
    _server: Server,
}

/// The PocketIC server process, stopped when dropped.
struct Server(std::process::Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A PocketIC server of its own. Requests through the HTTP gateway do not
/// count as server activity, so the default 60-second idle TTL would stop
/// the server while daemons and the relay still use the deployment.
fn server() -> (Server, reqwest::Url) {
    let bin = std::env::var_os("POCKET_IC_BIN").expect("POCKET_IC_BIN");
    let port_file = std::env::temp_dir().join(format!(
        "dmsg-pocket-ic-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let server = Server(
        std::process::Command::new(bin)
            .arg("--port-file")
            .arg(&port_file)
            .args(["--ttl", "600"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let started = std::time::Instant::now();
    loop {
        let port = std::fs::read_to_string(&port_file).unwrap_or_default();
        if let Ok(port) = port.trim().parse::<u16>() {
            let _ = std::fs::remove_file(&port_file);
            return (server, format!("http://127.0.0.1:{port}/").parse().unwrap());
        }
        assert!(started.elapsed().as_secs() < 30, "PocketIC did not start");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

impl Deployment {
    /// Install and configure every canister, then open the HTTP gateway.
    pub fn start() -> Self {
        let (server, url) = server();
        let mut ic = PocketIcBuilder::new()
            .with_server_url(url)
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
        let name_ledger = ic.create_canister_on_subnet(None, None, subnet);
        let membership = ic.create_canister_on_subnet(None, None, subnet);
        let sns = ic.create_canister_on_subnet(None, None, subnet);
        let payment = ic.create_canister_on_subnet(None, None, subnet);
        let directory = ic.create_canister_on_subnet(None, None, subnet);
        for id in [
            user,
            cose,
            commerce,
            handle,
            ledger,
            ledger_usdt,
            name_ledger,
            membership,
            sns,
            payment,
            directory,
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
                principal_origin: "https://id.dmsg.test".into(),
                directory_canister: directory,
                governance: sns,
                admission_key: None,
            },))
            .unwrap(),
            None,
        );
        ic.install_canister(
            directory,
            std::fs::read(dir.join("dmsg_directory.wasm")).unwrap(),
            candid::encode_args((dmsg_types::agent::DirectoryInit {
                environment: Environment::Local,
                issuer_namespace: "https://dmsg.test/u/".into(),
                user_homes: vec![user],
                principal_origin: "https://id.dmsg.test".into(),
                controller_source: "https://dmsg.test".into(),
                delegation_service: "https://agents.dmsg.test".into(),
                profile_url_prefix: "https://dmsg.test/".into(),
                custom_domains: vec!["id.dmsg.test".into()],
                governance: sns,
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
                user_homes: vec![user],
                governance: sns,
                derivation_version: 2,
                master: MasterKey {
                    key_name: "key_1".into(),
                    expected_fingerprint: Hash::new([0; 32]),
                },
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
        for id in [ledger_usdt, name_ledger] {
            ic.install_canister(
                id,
                std::fs::read(dir.join("dmsg_test_ledger.wasm")).unwrap(),
                candid::encode_args(()).unwrap(),
                None,
            );
        }
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
                environment: Environment::Local,
                issuer_namespace: "https://dmsg.test/u/".into(),
                user_homes: vec![user],
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
                limits: dmsg_types::payment::PaymentLimits {
                    max_escrows: 10_000,
                    daily_orders: 100,
                    max_open_per_payer: 4,
                    authorizations_per_minute: 200,
                    ledger_reads_per_minute: 200,
                    ledger_writes_per_minute: 200,
                    ledger_calls_per_caller: 40,
                },
                enabled: true,
            })
            .unwrap(),
            None,
        );
        let mut plans = dmsg_protocol::billing::default_plans(1);
        // Synthetic local catalog: a just-created account must have a nonzero
        // prorated allowance at any point in the calendar month, and room for
        // the desktop acceptance's 100 MiB file with its encryption overhead.
        plans[0].limits.monthly_execution_units = 1000;
        plans[0].limits.storage_bytes = 1_073_741_824;
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
                limits: dmsg_types::billing::CommerceLimits {
                    max_subjects: 20,
                    max_hot_orders: 100_000,
                    max_orders: 1_000_000,
                    daily_orders: 20,
                    calls_per_minute: 400,
                    calls_per_caller: 40,
                    authorizations_per_minute: 200,
                    refreshes_per_minute: 200,
                },
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
                commerce_homes: vec![CommerceHome {
                    user_home: user,
                    commerce_canister: commerce,
                }],
                max_claims: 100,
                hourly_applications: 100,
                cooling_ms: PANDA_COOLING_MS,
                qualifications_per_minute: 200,
                authorizations_per_minute: 200,
                product_calls_per_minute: 200,
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
                beneficiary_authorities: vec![user],
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
                environment: Environment::Local,
                issuer_namespace: "https://dmsg.test/u/".into(),
                user_homes: vec![user],
                registration_homes: vec![user],
                ledger: name_ledger,
                ledger_fee: 10,
                max_pending: 100,
                governance: peer,
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
        // Names cost at least 100 PANDA (10^10 base units).
        for (ledger, amount) in [
            (ledger, 10_000_000_000u128),
            (ledger_usdt, 10_000_000_000),
            (name_ledger, 100_000_000_000),
        ] {
            ic.update_call(
                ledger,
                peer,
                "mint_test",
                candid::encode_args((
                    icrc_ledger_types::icrc1::account::Account {
                        owner,
                        subaccount: None,
                    },
                    amount,
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
                candid::encode_args((snapshot.snapshot_id, 0u64, entries)).unwrap(),
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
        let gateway = ic.make_live(None).to_string();
        let root_key_hex: String = ic
            .root_key()
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Self {
            ic,
            user,
            cose,
            commerce,
            handle,
            ledger,
            ledger_usdt,
            name_ledger,
            membership,
            sns,
            payment,
            directory,
            peer,
            gateway,
            root_key_hex,
            _server: server,
        }
    }

    /// Register an application with commerce, as its governance does.
    #[allow(dead_code)]
    pub fn register_app(&self, app: &dmsg_types::integration::AppRegistration) {
        let reply = self
            .ic
            .update_call(
                self.commerce,
                self.peer,
                "register_integration_app",
                candid::encode_args((app,)).unwrap(),
            )
            .unwrap();
        candid::decode_one::<Result<()>>(&reply).unwrap().unwrap();
    }

    /// `fixture.json` as the relay launchers read it.
    pub fn fixture_json(&self) -> String {
        let Self {
            user,
            cose,
            commerce,
            handle,
            ledger,
            ledger_usdt,
            membership,
            sns,
            payment,
            directory,
            peer,
            gateway,
            root_key_hex: root,
            ..
        } = self;
        format!(
            r#"{{"test_only":true,"user":"{user}","cose":"{cose}","commerce":"{commerce}","handle":"{handle}","ledger":"{ledger}","ledger_usdt":"{ledger_usdt}","membership":"{membership}","sns":"{sns}","payment":"{payment}","directory":"{directory}","platform":"{peer}","namespace":"https://dmsg.test/u/","gateway":"{gateway}","root_key_hex":"{root}"}}"#
        )
    }
}
