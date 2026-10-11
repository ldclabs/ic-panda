use candid::{utils::ArgumentEncoder, CandidType, Nat, Principal};
use dmsg_protocol::*;
use dmsg_types::profiles::delivery::*;
use dmsg_types::{cose::*, handle::*, payment::*, user::*, *};
use ed25519_dalek::{Signer, SigningKey};
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{TransferArg, TransferError},
};
use pocket_ic::{PocketIc, PocketIcBuilder};
use serde::{de::DeserializeOwned, Deserialize};
use serde_bytes::ByteBuf;
use std::{path::PathBuf, time::Duration};

#[path = "control_plane/review_regressions.rs"]
mod review_regressions;

#[path = "control_plane/cose_optimization.rs"]
mod cose_optimization;

#[path = "control_plane/handle.rs"]
mod handle_tests;

#[path = "control_plane/payment_review.rs"]
mod payment_review;

#[path = "control_plane/payment_regressions.rs"]
mod payment_regressions;

#[path = "control_plane/payment_optimization.rs"]
mod payment_optimization;

#[path = "control_plane/payment_operations.rs"]
mod payment_operations;

#[path = "control_plane/user.rs"]
mod user_tests;

#[path = "control_plane/user_review.rs"]
mod user_review;

#[path = "control_plane/commerce.rs"]
mod commerce;

#[path = "control_plane/agent.rs"]
mod agent;

#[path = "control_plane/user_homes.rs"]
mod user_homes;

#[path = "control_plane/governance.rs"]
mod governance;

fn wasm(name: &str) -> Vec<u8> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = std::env::var_os("DMSG_WASM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/wasm32-unknown-unknown/release"));
    std::fs::read(target.join(format!("{name}.wasm")))
        .expect("build dMsg Wasm before running PocketIC tests")
}
fn update<A: ArgumentEncoder, R: CandidType + DeserializeOwned>(
    ic: &PocketIc,
    id: Principal,
    caller: Principal,
    method: &str,
    args: A,
) -> R {
    let bytes = ic
        .update_call(id, caller, method, candid::encode_args(args).unwrap())
        .unwrap_or_else(|e| panic!("{method}: {e:?}"));
    candid::decode_one(&bytes).unwrap_or_else(|e| panic!("decode {method}: {e:?}"))
}

/// A caller without access is refused: the canister's inspect_message rejects
/// the ingress before execution, or the method replies Forbidden.
fn assert_denied<A: ArgumentEncoder>(
    ic: &PocketIc,
    id: Principal,
    caller: Principal,
    method: &str,
    args: A,
) {
    match ic.update_call(id, caller, method, candid::encode_args(args).unwrap()) {
        Err(reject) => assert_eq!(
            reject.error_code,
            pocket_ic::ErrorCode::CanisterRejectedMessage,
            "{method}: {reject:?}"
        ),
        Ok(bytes) => {
            let reply: Result<candid::Reserved> = candid::decode_one(&bytes).unwrap();
            assert_eq!(reply.map(|_| ()), Err(Error::Forbidden), "{method}");
        }
    }
}

/// The canister's inspect_message refuses the ingress before the method runs.
fn assert_refused<A: ArgumentEncoder>(
    ic: &PocketIc,
    id: Principal,
    caller: Principal,
    method: &str,
    args: A,
) {
    let reject = ic
        .update_call(id, caller, method, candid::encode_args(args).unwrap())
        .expect_err(method);
    assert_eq!(
        reject.error_code,
        pocket_ic::ErrorCode::CanisterRejectedMessage,
        "{method}: {reject:?}"
    );
}

fn attest_approval(home: Principal, request: &AttestRequest) -> Hash {
    approval_message(
        home,
        &request.account_id,
        ATTEST_APPROVAL_DOMAIN,
        &attest_approval_command(&request.statement, &request.origin, &request.signature),
        &request.approval,
    )
}

fn derive_approval(home: Principal, request: &DeriveRootRequest) -> Hash {
    approval_message(
        home,
        &request.account_id,
        DERIVE_APPROVAL_DOMAIN,
        &derive_approval_command(request),
        &request.approval,
    )
}

fn completed(result: &ExecutionResult) -> &EncryptedRootKey {
    let ExecutionOutcome::Completed(output) = &result.outcome else {
        panic!("execution is not completed: {:?}", result.outcome)
    };
    output
}

fn query<A: ArgumentEncoder, R: CandidType + DeserializeOwned>(
    ic: &PocketIc,
    id: Principal,
    caller: Principal,
    method: &str,
    args: A,
) -> R {
    let bytes = ic
        .query_call(id, caller, method, candid::encode_args(args).unwrap())
        .unwrap_or_else(|e| panic!("{method}: {e:?}"));
    candid::decode_one(&bytes).unwrap()
}
fn void<A: ArgumentEncoder>(
    ic: &PocketIc,
    id: Principal,
    caller: Principal,
    method: &str,
    args: A,
) {
    ic.update_call(id, caller, method, candid::encode_args(args).unwrap())
        .unwrap();
}
fn time(ic: &PocketIc) -> u64 {
    nanos_to_millis(ic.get_time().as_nanos_since_unix_epoch())
}
fn account(owner: Principal) -> Account {
    Account {
        owner,
        subaccount: None,
    }
}
fn person(n: u8) -> Principal {
    Principal::self_authenticating([n; 32])
}
fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
fn device(n: u8) -> DeviceInput {
    DeviceInput {
        device_id: Hash::new([n; 32]),
        signing_pub: key(n).verifying_key().to_bytes().into(),
        hpke_pub: Hash::new([n; 32]),
        role: ControllerRole::Administrator,
        capabilities: vec![
            Capability::RootManage,
            Capability::FormalApprove,
            Capability::VaultUnlock,
            Capability::PaymentOffer,
        ],
    }
}
const NAMESPACE: &str = "https://dmsg.test/u/";
const PRINCIPAL_ORIGIN: &str = "https://id.dmsg.test";
const DELEGATION_SERVICE: &str = "https://agents.dmsg.test";

/// An account ID carrying `home`'s allocator fingerprint in the test namespace.
fn home_account(home: Principal, n: u8) -> AccountId {
    let digest =
        dmsg_protocol::agent::account_allocator_digest(&Environment::Local, NAMESPACE, home);
    let mut id = [n; 12];
    id[4..9].copy_from_slice(&digest[..5]);
    AccountId(id)
}
struct Fixture {
    ic: PocketIc,
    user: Principal,
    cose: Principal,
    handle: Principal,
    payment: Principal,
    ledger: Principal,
    ledger2: Principal,
    commerce: Principal,
    membership: Principal,
    sns: Principal,
    directory: Principal,
    cose_config: CoseInit,
}
impl Fixture {
    fn new() -> Self {
        Self::with_policy(true)
    }
    fn commercial() -> Self {
        Self::with_policy(false)
    }
    fn with_policy(generous: bool) -> Self {
        Self::with_order_limit(generous, 100)
    }

    fn with_order_limit(generous: bool, daily_orders: u32) -> Self {
        Self::configured(generous, daily_orders, |_| {})
    }

    fn configured(
        generous: bool,
        daily_orders: u32,
        configure_membership: impl FnOnce(&mut dmsg_types::membership::MembershipInit),
    ) -> Self {
        Self::with_payment_capacity(generous, daily_orders, MAX_PAYMENT_ESCROWS, configure_membership)
    }

    fn with_payment_capacity(
        generous: bool,
        daily_orders: u32,
        max_escrows: u64,
        configure_membership: impl FnOnce(&mut dmsg_types::membership::MembershipInit),
    ) -> Self {
        let ic = PocketIcBuilder::new()
            .with_nns_subnet()
            .with_application_subnet()
            .with_fiduciary_subnet()
            .build();
        let user = ic.create_canister();
        let cose = ic.create_canister();
        let handle = ic.create_canister();
        let payment = ic.create_canister();
        let ledger = ic.create_canister();
        let ledger2 = ic.create_canister();
        let commerce = ic.create_canister();
        let membership = ic.create_canister();
        let sns = ic.create_canister();
        let directory = ic.create_canister();
        for id in [
            user, cose, handle, payment, ledger, ledger2, commerce, membership, sns, directory,
        ] {
            ic.add_cycles(id, 10_000_000_000_000_000);
        }
        let cose_config = CoseInit {
            issuer_namespace: NAMESPACE.into(),
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
        };
        let signer = ReceiptSigner {
            epoch: 1,
            public_key: key(50).verifying_key().to_bytes().into(),
            valid_from: time(&ic),
            valid_until: time(&ic) + 30 * DAY,
            revoked: false,
        };
        ic.install_canister(
            user,
            wasm("dmsg_user"),
            candid::encode_args((UserInit {
                commerce_canister: commerce,
                membership_canister: membership,
                issuer_namespace: NAMESPACE.into(),
                environment: Environment::Local,
                home_cose: cose,
                handle_canister: handle,
                payment_canister: payment,
                max_accounts: 1000,
                daily_new_accounts: 100,
                principal_origin: PRINCIPAL_ORIGIN.into(),
                directory_canister: directory,
                governance: sns,
                admission_key: None,
            },))
            .unwrap(),
            None,
        );
        ic.install_canister(
            directory,
            wasm("dmsg_directory"),
            candid::encode_args((dmsg_types::agent::DirectoryInit {
                environment: Environment::Local,
                issuer_namespace: NAMESPACE.into(),
                user_homes: vec![user],
                principal_origin: PRINCIPAL_ORIGIN.into(),
                controller_source: "https://dmsg.net".into(),
                delegation_service: DELEGATION_SERVICE.into(),
                profile_url_prefix: "https://dmsg.test/u/".into(),
                custom_domains: vec!["id.dmsg.test".into()],
                governance: sns,
            },))
            .unwrap(),
            None,
        );
        ic.install_canister(
            cose,
            wasm("dmsg_cose"),
            candid::encode_args((cose_config.clone(),)).unwrap(),
            None,
        );
        ic.install_canister(
            handle,
            wasm("dmsg_handle"),
            candid::encode_args((HandleInit {
                ledger,
                environment: Environment::Local,
                issuer_namespace: NAMESPACE.into(),
                user_homes: vec![user],
                registration_homes: vec![user],
                ledger_fee: 10,
                max_pending: 100,
                governance: sns,
            },))
            .unwrap(),
            None,
        );
        ic.install_canister(
            payment,
            wasm("dmsg_payment"),
            candid::encode_args((PaymentInit {
                ledger,
                environment: Environment::Local,
                issuer_namespace: NAMESPACE.into(),
                user_homes: vec![user],
                platform: account(person(60)),
                governance: candid::Principal::from_slice(&[90]),
                fee_policy: dmsg_types::payment::DeliveryFeePolicy {
                    version: 1,
                    effective_at_ms: 0,
                    rate_bps: 500,
                    minimum_atomic: 100,
                },
                ledger_fee: 10,
                max_fee: 20,
                signer: signer.clone(),
                limits: PaymentLimits {
                    max_escrows,
                    daily_orders,
                    max_open_per_payer: 4,
                    authorizations_per_minute: 200,
                    ledger_reads_per_minute: 200,
                    ledger_writes_per_minute: 200,
                    ledger_calls_per_caller: 40,
                },
                enabled: true,
            },))
            .unwrap(),
            None,
        );
        ic.install_canister(
            ledger,
            wasm("dmsg_test_ledger"),
            candid::encode_args(()).unwrap(),
            None,
        );
        ic.install_canister(
            sns,
            wasm("dmsg_test_sns"),
            candid::encode_args(()).unwrap(),
            None,
        );
        let mut plans = dmsg_protocol::billing::default_plans(1);
        // Existing execution regressions deliberately use a generous fixture policy.
        // Commerce-specific tests install and assert the production default separately.
        if generous {
            plans[0].limits.monthly_execution_units = 1000;
        }
        let catalog = dmsg_types::billing::Catalog {
            schema: 1,
            version: 1,
            effective_at_ms: 0,
            plans: plans.clone(),
            storage_products: vec![dmsg_types::billing::StorageProduct {
                product_id: Hash::new([120; 32]),
                storage_bytes: 1_073_741_824,
                price_cents: 100,
            }],
            terms_digest: Hash::new([99; 32]),
        };
        ic.install_canister(
            commerce,
            wasm("dmsg_commerce"),
            candid::encode_args((dmsg_types::billing::CommerceInit {
                environment: Environment::Local,
                governance: sns,
                membership_canister: membership,
                user_homes: vec![user],
                catalog,
                limits: dmsg_types::billing::CommerceLimits {
                    max_subjects: 1000,
                    max_hot_orders: 100_000,
                    max_orders: 1_000_000,
                    daily_orders,
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
            ledger2,
            wasm("dmsg_test_ledger"),
            candid::encode_args(()).unwrap(),
            None,
        );
        let mut membership_init = dmsg_types::membership::MembershipInit {
            environment: Environment::Local,
            governance: sns,
            sns_root: sns,
            panda_ledger: sns,
        };
        configure_membership(&mut membership_init);
        ic.install_canister(
            membership,
            wasm("membership"),
            candid::encode_args((membership_init,)).unwrap(),
            None,
        );
        let verified: Result<()> = update(&ic, membership, sns, "verify_sns_configuration", ());
        verified.unwrap();
        use dmsg_types::{integration::*, integration_billing::*, integration_membership::*};
        let configured: Result<()> = update(
            &ic,
            membership,
            sns,
            "configure_panda_service",
            (PandaServiceConfig {
                commerce_homes: vec![CommerceHome {
                    user_home: user,
                    commerce_canister: commerce,
                }],
                max_claims: 1000,
                hourly_applications: 100,
                cooling_ms: PANDA_COOLING_MS,
                qualifications_per_minute: 200,
                authorizations_per_minute: 200,
                product_calls_per_minute: 200,
            },),
        );
        configured.unwrap();
        let policy: Result<PandaRatePolicy> = update(
            &ic,
            membership,
            sns,
            "schedule_panda_rate",
            (PandaRatePolicy {
                version: 2,
                policy_version: 1,
                environment: Environment::Local,
                product_ids: vec!["dmsg".into(), "sample".into()],
                r_num: 5000,
                r_den: 1,
                published_at_ms: 0,
                effective_at_ms: POLICY_NOTICE_MS,
            },),
        );
        policy.unwrap();
        let product = ProductRegistration {
            version: 2,
            environment: Environment::Local,
            product_id: "dmsg".into(),
            config_version: 1,
            quote_authority: commerce,
            beneficiary_authorities: vec![user],
            adapter: commerce,
            subject_schema: "dmsg-account-v1".into(),
            subject_size: 12,
            merchant: account(person(60)),
            ledgers: vec![ledger, ledger2],
            terms_hash: Hash::new([99; 32]),
            paused: false,
        };
        let result: Result<()> = update(
            &ic,
            commerce,
            sns,
            "register_integration_product",
            (product,),
        );
        result.unwrap();
        let app = AppRegistration {
            version: 1,
            environment: Environment::Local,
            app_id: "dmsg".into(),
            config_version: 1,
            origins: vec![
                "https://dmsg.test".into(),
                format!("chrome-extension://{}", "a".repeat(32)),
            ],
            product_ids: vec!["dmsg".into()],
            capabilities: vec![AppCapability::Checkout],
            profiles: vec![],
            authentication_receiver: user,
            action_authority: user,
            action_schema: None,
            paused: false,
        };
        let result: Result<()> = update(&ic, commerce, sns, "register_integration_app", (app,));
        result.unwrap();
        for (ledger, kind) in [
            (ledger, SettlementAssetKind::CkUsdc),
            (ledger2, SettlementAssetKind::CkUsdt),
        ] {
            let result: Result<()> = update(
                &ic,
                commerce,
                sns,
                "register_settlement_asset",
                (SettlementAsset {
                    version: 2,
                    policy_version: 1,
                    environment: Environment::Local,
                    ledger,
                    asset: kind,
                    decimals: 6,
                    price_usd_micros: 1_000_000,
                    price_observed_at_ms: time(&ic),
                    price_valid_until_ms: time(&ic) + 30 * MINUTE,
                    network_fee_atomic: 10,
                    max_network_fee_atomic: 20,
                    enabled: true,
                },),
            );
            result.unwrap();
            let verified: Result<()> = update(
                &ic,
                commerce,
                sns,
                "verify_settlement_asset",
                (ledger, None::<u128>),
            );
            verified.unwrap();
        }
        Self {
            commerce,
            membership,
            sns,
            directory,
            ledger,
            ledger2,
            ic,
            user,
            cose,
            handle,
            payment,
            cose_config,
        }
    }

    /// A statement signed by device `n` and approved for attestation.
    fn attest_request(&self, n: u8, id: &AccountId, statement: Statement) -> AttestRequest {
        self.attest_request_by(n, n, id, statement)
    }
    /// An attestation signed by device `n` and submitted by login `login`.
    fn attest_request_by(&self, login: u8, n: u8, id: &AccountId, statement: Statement) -> AttestRequest {
        let s = self.account_id(login, id);
        let sequence = s.devices[&Hash::new([n; 32])].next_sequence;
        let prepared =
            prepare_attestation(&statement, &key(n).verifying_key().to_bytes().into()).unwrap();
        let mut request = AttestRequest {
            account_id: *id,
            statement,
            origin: "https://example.com".into(),
            signature: key(n).sign(&prepared.to_be_signed).to_bytes().into(),
            approval: Approval {
                device_id: Hash::new([n; 32]),
                security_epoch: s.security_epoch,
                sequence,
                request_id: execution_request_id(id, s.security_epoch, Hash::new([n; 32]), sequence),
                expires_at: time(&self.ic) + MINUTE,
                signature: Default::default(),
            },
        };
        request.approval.signature = key(n)
            .sign(attest_approval(self.user, &request).as_slice())
            .to_bytes()
            .into();
        request
    }
    fn attest(&self, n: u8, id: &AccountId, statement: Statement) -> Result<SignedArtifact> {
        self.attest_by(n, n, id, statement)
    }
    fn attest_by(&self, login: u8, n: u8, id: &AccountId, statement: Statement) -> Result<SignedArtifact> {
        let request = self.attest_request_by(login, n, id, statement);
        update(&self.ic, self.user, person(login), "attest", (request,))
    }
    fn create_input(&self, n: u8) -> CreateAccount {
        let expires = time(&self.ic) + MINUTE;
        let dev = device(n);
        let op = Hash::new([n; 32]);
        let proof = key(n)
            .sign(
                digest(
                    "dmsg/create-account/v1",
                    &(self.user, person(n), &dev, op, expires),
                )
                .as_slice(),
            )
            .to_bytes()
            .into();
        CreateAccount {
            device: dev,
            op_id: op,
            expires_at: expires,
            proof,
            admission: None,
        }
    }
    fn create(&self, n: u8) -> AccountId {
        let r: Result<AccountId> = update(
            &self.ic,
            self.user,
            person(n),
            "create_account",
            (self.create_input(n),),
        );
        r.unwrap()
    }
    fn account_id(&self, n: u8, id: &AccountId) -> AccountInfo {
        let r: Result<AccountInfo> = query(&self.ic, self.user, person(n), "get_account", (id,));
        r.unwrap()
    }
    fn mutate(&self, n: u8, id: &AccountId, command: AccountCommand) -> Result<OperationReceipt> {
        self.mutate_by(n, n, id, command)
    }
    /// A mutation approved by device `n` and submitted by login `login`.
    fn mutate_by(
        &self,
        login: u8,
        n: u8,
        id: &AccountId,
        command: AccountCommand,
    ) -> Result<OperationReceipt> {
        let s = self.account_id(login, id);
        let mut m = AccountMutation {
            account_id: *id,
            expected_version: s.account_version,
            command,
            approval: Approval {
                device_id: Hash::new([n; 32]),
                security_epoch: s.security_epoch,
                sequence: s.devices[&Hash::new([n; 32])].next_sequence,
                request_id: digest("test-operation", &(id, s.account_version)),
                expires_at: time(&self.ic) + MINUTE,
                signature: Default::default(),
            },
        };
        m.approval.signature = key(n)
            .sign(
                approval_message(
                    self.user,
                    id,
                    "dmsg/account/v2",
                    &(&m.expected_version, &m.command),
                    &m.approval,
                )
                .as_slice(),
            )
            .to_bytes()
            .into();
        update(&self.ic, self.user, person(login), "mutate_account", (m,))
    }
    /// Administrator device `admin` approves login `login`, which accepts it.
    fn bind(&self, admin: u8, login: u8, id: &AccountId, nonce: Hash) {
        self.mutate(
            admin,
            id,
            AccountCommand::BindAuth {
                principal: person(login),
                nonce,
            },
        )
        .unwrap();
        let accepted: Result<()> = update(
            &self.ic,
            self.user,
            person(login),
            "accept_auth_binding",
            (id, nonce),
        );
        accepted.unwrap();
    }
    /// Run the public execution cleanup from an empty cursor to the end;
    /// returns the number of pages.
    fn prune_executions(&self) -> usize {
        let mut cursor = ByteBuf::new();
        for page in 1.. {
            let next: Option<ByteBuf> = update(
                &self.ic,
                self.user,
                Principal::anonymous(),
                "prune_executions",
                (cursor,),
            );
            match next {
                Some(next) => cursor = next,
                None => return page,
            }
        }
        unreachable!()
    }
    /// The next root reference, wrapped to the account's active devices and
    /// the vetKD identity of `generation`, with an arbitrary body digest.
    fn root_ref(&self, n: u8, id: &AccountId, generation: u64) -> ContentRootRef {
        let s = self.account_id(n, id);
        let devices: Vec<Hash> = s
            .devices
            .iter()
            .filter(|(_, d)| d.revoked_at.is_none())
            .map(|(id, _)| *id)
            .collect();
        let recipients_digest = root_recipients_digest(&devices, generation);
        let body_digest = Hash::new([12 + generation as u8; 32]);
        ContentRootRef {
            generation,
            suite: "dmsg-root-v2".into(),
            bundle_digest: root_bundle_digest(recipients_digest, body_digest),
            recipients_digest,
            body_digest,
        }
    }

    /// Reserve and commit the next root generation with device `n`.
    fn rekey(&self, n: u8, id: &AccountId) -> ContentRootRef {
        self.rekey_by(n, n, id)
    }
    fn rekey_by(&self, login: u8, n: u8, id: &AccountId) -> ContentRootRef {
        let s = self.account_id(login, id);
        let expected = s.current_root.as_ref().map_or(0, |r| r.generation);
        let op_id = digest("test-root", &(id, expected, s.account_version));
        self.mutate_by(
            login,
            n,
            id,
            AccountCommand::ReserveRoot {
                expected_generation: expected,
                op_id,
            },
        )
        .unwrap();
        let generation = self.account_id(login, id).root_slot.unwrap().generation;
        let root = self.root_ref(login, id, generation);
        self.mutate_by(
            login,
            n,
            id,
            AccountCommand::CommitRoot {
                expected_generation: expected,
                op_id,
                root: root.clone(),
            },
        )
        .unwrap();
        root
    }

    /// An account with a committed generation-1 content root.
    fn root_account(&self, n: u8) -> AccountId {
        let account_id = self.create(n);
        self.rekey(n, &account_id);
        account_id
    }

    /// Recover the account of login `n` onto new device `m` after the delay.
    fn recover(&self, n: u8, m: u8, id: &AccountId) -> OpId {
        let s = self.account_id(n, id);
        let request = RecoveryRequest {
            op_id: digest("test-recovery", &(id, m, s.account_version)),
            new_auth: person(n),
            device: device(m),
            expires_at: time(&self.ic) + s.recovery_delay_ms + DAY,
        };
        let proof = ByteBuf::from(
            key(m)
                .sign(recovery_device_message(self.user, id, &request).as_slice())
                .to_bytes()
                .to_vec(),
        );
        let begun: Result<()> = update(
            &self.ic,
            self.user,
            person(n),
            "request_recovery",
            (id, request.clone(), proof),
        );
        begun.unwrap();
        self.ic
            .advance_time(Duration::from_millis(s.recovery_delay_ms + 1));
        let done: Result<()> = update(
            &self.ic,
            self.user,
            person(n),
            "complete_recovery",
            (id, request.op_id),
        );
        done.unwrap();
        request.op_id
    }

    fn mint(&self, who: Principal, n: u128) {
        void(
            &self.ic,
            self.ledger,
            Principal::anonymous(),
            "mint_test",
            (account(who), n),
        );
    }
    fn approve_handle(&self, who: Principal, amount: u128) {
        void(
            &self.ic,
            self.ledger,
            Principal::anonymous(),
            "approve_test",
            (account(who), account(self.handle), amount),
        );
    }
    fn fund(&self, e: &EscrowInfo, amount: u128) -> u64 {
        self.mint(e.payer_principal, amount + 10);
        let r: std::result::Result<Nat, TransferError> = update(
            &self.ic,
            self.ledger,
            e.payer_principal,
            "icrc1_transfer",
            (TransferArg {
                from_subaccount: None,
                to: Account {
                    owner: self.payment,
                    subaccount: Some(e.subaccount.into_array()),
                },
                amount: amount.into(),
                fee: Some(10u64.into()),
                memo: None,
                created_at_time: Some(millis_to_nanos(time(&self.ic)).unwrap()),
            },),
        );
        r.unwrap().0.to_string().parse().unwrap()
    }
    fn order(&self, recipient: &AccountId, n: u8, nonce: u8) -> OpenEscrow {
        self.order_by(recipient, n, n, nonce)
    }
    /// An offer of login `n`'s account signed by its device `device`.
    fn order_by(&self, recipient: &AccountId, n: u8, device: u8, nonce: u8) -> OpenEscrow {
        let s = self.account_id(n, recipient);
        let now = time(&self.ic);
        let offer = PaymentOffer {
            account_id: *recipient,
            device_id: Hash::new([device; 32]),
            security_epoch: s.security_epoch,
            home_payment: self.payment,
            offer_id: Hash::new([nonce; 32]),
            ledger: self.ledger,
            recipient: account(person(n)),
            recipient_net: 1000,
            quote_scope: Hash::new([nonce; 32]),
            version: 1,
            issued_at: now,
            expires_at: now + 15 * MINUTE,
        };
        let signature = key(device)
            .sign(digest("dmsg/payment-offer/v1", &offer).as_slice())
            .to_bytes()
            .into();
        let quote = Quote {
            fee_policy_version: 1,
            quote_id: Hash::new([nonce; 32]),
            home_payment: self.payment,
            payer: account(person(40)),
            offer_digest: digest("dmsg/payment-offer/v1", &offer),
            quote_scope: offer.quote_scope,
            ledger: self.ledger,
            recipient: offer.recipient,
            recipient_net: 1000,
            platform: account(person(60)),
            service_fee: 100,
            fee_reserve: 40,
            amount: 1140,
            max_network_fee: 20,
            max_bytes: 8192,
            retain_ms: DAY,
            envelope_digest: Hash::new([3; 32]),
            signer_epoch: 1,
            created_at: now,
            fund_by: now + 15 * MINUTE,
            accept_by: now + 45 * MINUTE,
        };
        let quote_signature = key(50)
            .sign(digest("dmsg/quote/v2", &quote).as_slice())
            .to_bytes()
            .into();
        OpenEscrow {
            op_id: Hash::new([nonce; 32]),
            quote,
            quote_signature,
            offer: SignedOffer { offer, signature },
        }
    }
    fn receipt(&self, e: &EscrowInfo) -> SignedReceipt {
        let now = time(&self.ic);
        let receipt = AdmissionReceipt {
            protocol: 2,
            relay_id: Hash::new([9; 32]),
            signer_epoch: 1,
            home_payment: self.payment,
            escrow_id: e.escrow_id,
            quote_digest: e.quote_digest,
            envelope_digest: e.quote.envelope_digest,
            size: 128,
            policy_version: 1,
            inbox_key_version: 1,
            admission_seq: 1,
            stored_at: now,
            retain_until: now + DAY,
            fund_by: e.quote.fund_by,
            accept_by: e.quote.accept_by,
        };
        let signature = key(50)
            .sign(digest("dmsg/admission-receipt/v2", &receipt).as_slice())
            .to_bytes()
            .into();
        SignedReceipt { receipt, signature }
    }
    fn derive_request(
        &self,
        n: u8,
        m: u8,
        account_id: &AccountId,
        transport_key: Vec<u8>,
    ) -> DeriveRootRequest {
        let s = self.account_id(n, account_id);
        let sequence = s.devices[&Hash::new([m; 32])].next_sequence;
        let mut request = DeriveRootRequest {
            account_id: *account_id,
            generation: s.current_root.as_ref().map_or(0, |r| r.generation),
            transport_public_key: serde_bytes::ByteArray::new(transport_key.try_into().unwrap()),
            max_cycles: 100_000_000_000,
            approval: Approval {
                device_id: Hash::new([m; 32]),
                security_epoch: s.security_epoch,
                sequence,
                request_id: execution_request_id(
                    account_id,
                    s.security_epoch,
                    Hash::new([m; 32]),
                    sequence,
                ),
                expires_at: time(&self.ic) + MINUTE,
                signature: Default::default(),
            },
        };
        request.approval.signature = key(m)
            .sign(derive_approval(self.user, &request).as_slice())
            .to_bytes()
            .into();
        request
    }

    /// Login `n`'s recovered device `m` derives the committed root.
    fn derive(&self, n: u8, m: u8, account_id: &AccountId, transport_key: Vec<u8>) -> ExecutionResult {
        let request = self.derive_request(n, m, account_id, transport_key);
        let result: Result<ExecutionResult> =
            update(&self.ic, self.user, person(n), "derive_root", (request,));
        result.unwrap()
    }
}

/// Install another `dmsg_user` sharing this deployment's services and buying
/// through `commerce`. The services route its accounts only after governance
/// lists it.
fn install_user_home(f: &Fixture, commerce: Principal) -> Principal {
    let home = f.ic.create_canister();
    f.ic.add_cycles(home, 10_000_000_000_000_000);
    f.ic.install_canister(
        home,
        wasm("dmsg_user"),
        candid::encode_args((UserInit {
            commerce_canister: commerce,
            membership_canister: f.membership,
            issuer_namespace: NAMESPACE.into(),
            environment: Environment::Local,
            home_cose: f.cose,
            handle_canister: f.handle,
            payment_canister: f.payment,
            max_accounts: 1000,
            daily_new_accounts: 100,
            principal_origin: PRINCIPAL_ORIGIN.into(),
            directory_canister: f.directory,
            governance: f.sns,
            admission_key: None,
        },))
        .unwrap(),
        None,
    );
    home
}

// PocketIC is the trusted certificate source here. Match the witness against
// its certified_data and the exact path/value; the SDK also checks the BLS root.
fn certified_value(f: &Fixture, batch: CertifiedBatch, path: &[u8]) -> Vec<u8> {
    assert_eq!(batch.canister, f.user);
    assert_eq!(batch.entries.len(), 1);
    let entry = &batch.entries[0];
    assert_eq!(entry.key.as_ref(), path);
    let witness: ic_certification::HashTree = cbor2::from_slice(&entry.witness).unwrap();
    let cert: ic_certification::Certificate = cbor2::from_slice(&batch.certificate).unwrap();
    assert_eq!(
        cert.tree.lookup_path([
            b"canister".as_slice(),
            f.user.as_slice(),
            b"certified_data".as_slice()
        ]),
        ic_certification::LookupResult::Found(&witness.digest())
    );
    assert_eq!(
        witness.lookup_path([path]),
        ic_certification::LookupResult::Found(entry.value.as_ref().unwrap())
    );
    entry.value.as_ref().unwrap().to_vec()
}

#[test]
fn xid_allocation_is_atomic_idempotent_and_persistent() {
    let f = Fixture::new();
    let a = f.create(1);
    assert_eq!(a.as_slice().len(), 12);
    assert_eq!(a.to_string().len(), 20);
    let mut bad = f.create_input(2);
    bad.proof = [0; 64].into();
    let failed: Result<AccountId> = update(&f.ic, f.user, person(2), "create_account", (bad,));
    assert!(failed.is_err());
    let unbound: Option<AccountId> = query(&f.ic, f.user, person(2), "my_account", ());
    assert_eq!(unbound, None);
    assert_eq!(f.create(1), a);
    let b = f.create(2);
    assert!(a < b);
    let count = |id: &AccountId| u32::from_be_bytes([0, id[9], id[10], id[11]]);
    assert_eq!(count(&b), if a[..4] == b[..4] { count(&a) + 1 } else { 0 });
    let fingerprint = digest(
        "dmsg/account-id-generator/v1",
        &("dmsg", Environment::Local, NAMESPACE, f.user),
    );
    assert_eq!(&b[4..9], &fingerprint[..5]);
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(f.create(2), b);
    let c = f.create(3);
    assert!(b < c);
    assert_eq!(count(&c), if b[..4] == c[..4] { count(&b) + 1 } else { 0 });
    let account = f.account_id(3, &c);
    assert_eq!(account.issuer, account_issuer(NAMESPACE, &c));
}

#[test]
fn identity_roots_certification_attestation_recovery_and_upgrade() {
    let f = Fixture::new();
    let account_id = f.create(1);
    assert_eq!(f.create(1), account_id);
    let root = f.rekey(1, &account_id);
    let s = f.account_id(1, &account_id);
    assert_eq!(s.current_root, Some(root.clone()));
    assert_eq!(s.recovery_delay_ms, DEFAULT_RECOVERY_DELAY_MS);
    let batch: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "security_snapshot_batch",
        (vec![&account_id],),
    );
    let batch = batch.unwrap();
    let witness: ic_certification::HashTree = cbor2::from_slice(&batch.entries[0].witness).unwrap();
    let cert: ic_certification::Certificate = cbor2::from_slice(&batch.certificate).unwrap();
    assert_eq!(
        cert.tree.lookup_path([
            b"canister".as_slice(),
            f.user.as_slice(),
            b"certified_data".as_slice()
        ]),
        ic_certification::LookupResult::Found(&witness.digest())
    );
    assert_eq!(
        witness.lookup_path([account_id.as_slice()]),
        ic_certification::LookupResult::Found(batch.entries[0].value.as_ref().unwrap())
    );
    let snapshot: SecuritySnapshot =
        decode_canonical(batch.entries[0].value.as_ref().unwrap()).unwrap();
    assert_eq!(snapshot.schema, 4);
    assert_eq!(snapshot.content_root_digest, Some(root.bundle_digest));

    // The unlock secret is served only to a bound login for an active device,
    // once the home has drawn its master secret.
    let stats: UserStats = query(&f.ic, f.user, Principal::anonymous(), "user_stats", ());
    assert!(stats.unlock_ready);
    assert_eq!((stats.accounts, stats.created_today), (1, 1));
    let secret: Result<Hash> = query(
        &f.ic,
        f.user,
        person(1),
        "unlock_secret",
        (&account_id, Hash::new([1; 32])),
    );
    let secret = secret.unwrap();
    let again: Result<Hash> = query(
        &f.ic,
        f.user,
        person(1),
        "unlock_secret",
        (&account_id, Hash::new([1; 32])),
    );
    assert_eq!(again, Ok(secret));
    let denied: Result<Hash> = query(
        &f.ic,
        f.user,
        person(2),
        "unlock_secret",
        (&account_id, Hash::new([1; 32])),
    );
    assert_eq!(denied, Err(Error::AuthRequired));
    let unknown: Result<Hash> = query(
        &f.ic,
        f.user,
        person(1),
        "unlock_secret",
        (&account_id, Hash::new([2; 32])),
    );
    assert_eq!(unknown, Err(Error::DeviceNotApproved));

    // A device-signed statement is certified in the same message.
    let payload = Statement {
        issuer: account_issuer(NAMESPACE, &account_id),
        subject: Some("release/spec".into()),
        issued_at: None,
        content: StatementContent::Digest {
            sha256: Hash::new([31; 32]),
            content_type: Some("application/pdf".into()),
            location: None,
        },
    };
    let request = f.attest_request(1, &account_id, payload.clone());
    let request_id = request.approval.request_id;
    let artifact: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest", (request.clone(),));
    let artifact = artifact.unwrap();
    assert_eq!(verify_artifact(&artifact).unwrap(), payload);
    assert_eq!(
        key_thumbprint(&artifact.cose_key).unwrap(),
        key_thumbprint(&public_cose_key(&[], &key(1).verifying_key().to_bytes()).unwrap())
            .unwrap()
    );
    let batch: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_receipt",
        (&account_id, request_id),
    );
    let batch = batch.unwrap();
    // Optional reproducible fixture for SDK certificate/receipt verification.
    // The root and account keys belong to this local PocketIC instance only.
    if let Some(path) = std::env::var_os("DMSG_EXPORT_RECEIPT") {
        std::fs::write(
            path,
            canonical(&(
                ByteBuf::from(f.ic.root_key().unwrap()),
                time(&f.ic),
                &batch,
                &artifact,
                &account_id,
                request_id,
                &payload.issuer,
            )),
        )
        .unwrap();
    }
    let receipt: ExecutionReceipt = decode_canonical(&certified_value(
        &f,
        batch,
        &execution_receipt_key(&account_id, request_id),
    ))
    .unwrap();
    match_execution_receipt(&artifact, &receipt).unwrap();
    assert_eq!(receipt.request_id, request_id);
    assert_eq!(receipt.account_id, account_id);
    assert_eq!(receipt.device_id, Hash::new([1; 32]));
    assert_eq!(receipt.origin, "https://example.com");
    let mut altered = receipt.clone();
    altered.to_be_signed_digest = Hash::new([99; 32]);
    assert_eq!(
        match_execution_receipt(&artifact, &altered),
        Err(Error::IntegrityFailed)
    );
    altered = receipt.clone();
    altered.public_key_fingerprint = Hash::new([99; 32]);
    assert_eq!(
        match_execution_receipt(&artifact, &altered),
        Err(Error::IntegrityFailed)
    );
    let denied: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(2),
        "get_execution_receipt",
        (&account_id, request_id),
    );
    assert!(denied.is_err());
    let replay: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest", (request,));
    assert_eq!(replay.unwrap(), artifact);
    let stored: Result<SignedArtifact> = query(
        &f.ic,
        f.user,
        person(1),
        "get_attestation",
        (&account_id, request_id),
    );
    assert_eq!(stored.unwrap(), artifact);

    // The root bundle's recovery envelope is encrypted offline to the
    // account's vetKD identity; only a recovered device may derive it.
    #[derive(CandidType, Deserialize)]
    struct KeyState {
        initialization: Initialization,
    }
    let init: Result<KeyState> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    assert_eq!(init.unwrap().initialization, Initialization::Ready);
    let described: Result<KeyDescriptor> = query(
        &f.ic,
        f.cose,
        Principal::anonymous(),
        "root_public_key",
        (&account_id, 1u64),
    );
    let described = described.unwrap();
    assert_eq!(described.home_cose, f.cose);
    assert_eq!(described.key_generation, 1);
    let public = ic_vetkeys::DerivedPublicKey::deserialize(&described.public_key).unwrap();
    let envelope = ic_vetkeys::IbeCiphertext::encrypt(
        &public,
        &ic_vetkeys::IbeIdentity::from_bytes(&canonical(&(&account_id, 1u64))),
        b"content root of generation 1",
        &ic_vetkeys::IbeSeed::from_bytes(&[7; 32]).unwrap(),
    );
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32]).unwrap();
    let unrecovered = f.derive_request(1, 1, &account_id, transport.public_key());
    let refused: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "derive_root", (unrecovered,));
    assert_eq!(refused, Err(Error::Forbidden));
    f.recover(1, 9, &account_id);
    let s = f.account_id(1, &account_id);
    assert_eq!(s.auth_bindings, vec![person(1)]);
    assert_eq!(s.devices.len(), 1);
    assert_eq!(s.recovered_device, Some((Hash::new([9; 32]), 1)));
    assert_eq!(s.vault_write_state, VaultWriteState::RekeyRequired);
    let derived = f.derive(1, 9, &account_id, transport.public_key());
    assert_eq!(derived.status(), ExecutionStatus::Completed);
    let output = completed(&derived);
    assert_eq!(output.key, described);
    let encrypted = ic_vetkeys::EncryptedVetKey::deserialize(&output.encrypted_key).unwrap();
    let vetkey = encrypted
        .decrypt_and_verify(&transport, &public, &canonical(&(&account_id, 1u64)))
        .unwrap();
    assert_eq!(
        envelope.decrypt(&vetkey).unwrap(),
        b"content root of generation 1"
    );
    assert!(encrypted
        .decrypt_and_verify(&transport, &public, &canonical(&(&account_id, 2u64)))
        .is_err());
    // The recovered device rekeys and loses the derivation right.
    let rotated = f.rekey_by(1, 9, &account_id);
    assert_eq!(rotated.generation, 2);
    assert_eq!(f.account_id(1, &account_id).recovered_device, None);
    let again = f.derive_request(1, 9, &account_id, transport.public_key());
    let refused: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "derive_root", (again,));
    assert_eq!(refused, Err(Error::Forbidden));
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    f.ic.upgrade_canister(
        f.cose,
        wasm("dmsg_cose"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(f.account_id(1, &account_id).current_root, Some(rotated));
    // The recovery delay outlived the attestation's retention: the receipt is
    // now a certified absence, and the artifact is no longer served.
    let pruned: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_receipt",
        (&account_id, request_id),
    );
    assert!(pruned.unwrap().entries[0].value.is_none());
    let expired: Result<SignedArtifact> = query(
        &f.ic,
        f.user,
        person(1),
        "get_attestation",
        (&account_id, request_id),
    );
    assert_eq!(expired, Err(Error::ResultExpired));
    // Both stores must rehydrate the independently persisted derivation after upgrade.
    let local: Result<ExecutionResult> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution",
        (&account_id, derived.request_id),
    );
    let remote: Result<ExecutionResult> = query(
        &f.ic,
        f.cose,
        f.user,
        "get_execution",
        (&account_id, derived.request_id),
    );
    assert_eq!(local.unwrap(), derived);
    assert_eq!(remote.unwrap(), derived);
    let replay: Result<ExecutionResult> = update(
        &f.ic,
        f.user,
        person(1),
        "reconcile_execution",
        (&account_id, derived.request_id),
    );
    assert_eq!(replay.unwrap(), derived);
    let secret_after: Result<Hash> = query(
        &f.ic,
        f.user,
        person(1),
        "unlock_secret",
        (&account_id, Hash::new([9; 32])),
    );
    assert!(secret_after.is_ok());
    let revoked_secret: Result<Hash> = query(
        &f.ic,
        f.user,
        person(1),
        "unlock_secret",
        (&account_id, Hash::new([1; 32])),
    );
    assert_eq!(revoked_secret, Err(Error::DeviceNotApproved));

    // Upgrades take no configuration; key descriptions stay as installed.
    let state: dmsg_types::cose::KeyState = query(&f.ic, f.cose, person(1), "key_state", ());
    assert_eq!(state.config, f.cose_config);
}

#[test]
fn frozen_names_cannot_be_sold_and_transfers_require_both_subjects() {
    let f = Fixture::new();
    let owner = f.create(1);
    let target = f.create(2);
    let legacy = LegacyReservation {
        handle: "alice".into(),
        legacy_owner: person(1),
        legacy_name_principal: None,
        frozen_admins: vec![],
        quarantined: false,
    };
    let snapshot = LegacySnapshot {
        source_canister: person(80),
        snapshot_id: Hash::new([5; 32]),
        freeze_version: 1,
        event_tip: Hash::new([7; 32]),
        count: 1,
        entries_digest: digest("dmsg/legacy-entry/v1", &(Hash::new([0u8; 32]), &legacy)),
    };
    let r: Result<()> = update(
        &f.ic,
        f.handle,
        Principal::anonymous(),
        "begin_legacy_snapshot",
        (snapshot.clone(),),
    );
    r.unwrap();
    let prematurely: Result<candid::Reserved> = update(
        &f.ic,
        f.handle,
        Principal::anonymous(),
        "seal_legacy_snapshot",
        (),
    );
    assert!(prematurely.is_err());
    let imported: Result<candid::Reserved> = update(
        &f.ic,
        f.handle,
        Principal::anonymous(),
        "import_legacy_handles",
        (snapshot.snapshot_id, 0u64, vec![legacy.clone()]),
    );
    imported.unwrap();
    let sealed: Result<candid::Reserved> = update(
        &f.ic,
        f.handle,
        Principal::anonymous(),
        "seal_legacy_snapshot",
        (),
    );
    sealed.unwrap();
    let intent = HandleIntent {
        handle_canister: f.handle,
        action: HandleAction::ClaimLegacy,
        account_id: owner,
        target_account: None,
        handle: "alice".into(),
        expected_version: 0,
        op_id: Hash::new([6; 32]),
        terms_digest: digest(
            "dmsg/legacy-claim/v1",
            &(snapshot.snapshot_id, &legacy, &owner),
        ),
    };
    f.mutate(
        1,
        &owner,
        AccountCommand::AuthorizeHandle {
            intent: intent.clone(),
        },
    )
    .unwrap();
    let forged: Result<HandleRecord> = update(
        &f.ic,
        f.handle,
        person(2),
        "claim_legacy_handle",
        (intent.clone(), snapshot.snapshot_id),
    );
    assert_eq!(forged, Err(Error::Forbidden));
    let claimed: Result<HandleRecord> = update(
        &f.ic,
        f.handle,
        person(1),
        "claim_legacy_handle",
        (intent, snapshot.snapshot_id),
    );
    assert_eq!(claimed.unwrap().owner_account, owner);
    let op_id = Hash::new([10; 32]);
    let terms = digest(
        "dmsg/handle-transfer/v1",
        &(f.handle, "alice", &owner, &target, 1u64, op_id),
    );
    let from = HandleIntent {
        handle_canister: f.handle,
        action: HandleAction::Transfer,
        account_id: owner,
        target_account: Some(target),
        handle: "alice".into(),
        expected_version: 1,
        op_id,
        terms_digest: terms,
    };
    let accept = HandleIntent {
        action: HandleAction::AcceptTransfer,
        account_id: target,
        target_account: Some(owner),
        ..from.clone()
    };
    f.mutate(
        1,
        &owner,
        AccountCommand::AuthorizeHandle {
            intent: from.clone(),
        },
    )
    .unwrap();
    let denied: Result<HandleRecord> = update(
        &f.ic,
        f.handle,
        person(1),
        "transfer_handle",
        (from.clone(), accept.clone()),
    );
    assert!(denied.is_err());
    f.mutate(
        2,
        &target,
        AccountCommand::AuthorizeHandle {
            intent: accept.clone(),
        },
    )
    .unwrap();
    let transferred: Result<HandleRecord> = update(
        &f.ic,
        f.handle,
        person(1),
        "transfer_handle",
        (from.clone(), accept.clone()),
    );
    let transferred = transferred.unwrap();
    assert_eq!(transferred.owner_account, target);
    let replay: Result<HandleRecord> = update(
        &f.ic,
        f.handle,
        person(1),
        "transfer_handle",
        (from, accept),
    );
    assert_eq!(replay.unwrap(), transferred);
    // A new name charges exactly once, without creating profile/namespace data.
    let amount = price("newname") - 10;
    let payer = account(person(1));
    let intent = HandleIntent {
        handle_canister: f.handle,
        action: HandleAction::Register,
        account_id: owner,
        target_account: None,
        handle: "newname".into(),
        expected_version: 0,
        op_id: Hash::new([11; 32]),
        terms_digest: charge_terms_digest(f.ledger, &payer, amount, 10u128),
    };
    f.mutate(
        1,
        &owner,
        AccountCommand::AuthorizeHandle {
            intent: intent.clone(),
        },
    )
    .unwrap();
    f.mint(person(1), amount + 10);
    f.approve_handle(person(1), amount + 10);
    let registration = Registration {
        intent,
        payer,
        fee: 10,
    };
    let committed: Result<HandleOperation> = update(
        &f.ic,
        f.handle,
        person(1),
        "register_handle",
        (&registration,),
    );
    let committed = committed.unwrap();
    assert_eq!(committed.phase, HandlePhase::Committed);
    let replay: Result<HandleOperation> = update(
        &f.ic,
        f.handle,
        person(1),
        "register_handle",
        (&registration,),
    );
    assert_eq!(replay.unwrap(), committed);
    let balance: Nat = query(&f.ic, f.ledger, person(1), "icrc1_balance_of", (payer,));
    assert_eq!(balance, Nat::from(0u8));
    f.ic.upgrade_canister(
        f.handle,
        wasm("dmsg_handle"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let proof: Result<CertifiedBatch> = query(
        &f.ic,
        f.handle,
        person(1),
        "resolve_handle_certified",
        (vec!["ALICE".to_string()],),
    );
    assert!(proof.unwrap().entries[0].value.is_some());
}

#[test]
fn escrow_settlement_duplicate_callbacks_fee_repair_and_direct_refunds() {
    let f = Fixture::new();
    let recipient = f.create(2);
    let input = f.order(&recipient, 2, 1);
    let opened: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (input.clone(),),
    );
    let e = opened.unwrap();
    let replay: Result<EscrowInfo> = update(&f.ic, f.payment, person(40), "open_escrow", (input,));
    assert_eq!(replay.unwrap(), e);
    let block = f.fund(&e, e.quote.amount + 17);
    let funded: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "check_funding",
        (e.escrow_id, block),
    );
    let funded = funded.unwrap();
    assert_eq!(funded.funding_ref, Some(block));
    assert!(funds_conserved(&funded));
    let signed = f.receipt(&funded);
    let decided: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(99),
        "finalize_receipt",
        (signed.clone(),),
    );
    assert_eq!(
        decided.unwrap().decision,
        FundsDecision::SettlementCommitted
    );
    let duplicate: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(99),
        "finalize_receipt",
        (signed.clone(),),
    );
    duplicate.unwrap();
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "lose_next_response",
        (),
    );
    let lost: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(99),
        "process_transfer",
        (e.escrow_id, 0u64),
    );
    assert_eq!(lost, Err(Error::ExecutionUnknown));
    let retry: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(99),
        "process_transfer",
        (e.escrow_id, 0u64),
    );
    assert_eq!(retry.unwrap().status, LegStatus::Succeeded);
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "set_fee",
        (20u128,),
    );
    let blocked: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(99),
        "process_transfer",
        (e.escrow_id, 1u64),
    );
    assert_eq!(blocked.unwrap().status, LegStatus::FeeBlocked);
    let revised: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "revise_rejected_transfer",
        (e.escrow_id, 1u64, 20u128),
    );
    let revised = revised.unwrap();
    assert_eq!(revised.amount, 100);
    let paid: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(99),
        "process_transfer",
        (e.escrow_id, revised.leg_id),
    );
    assert_eq!(paid.unwrap().status, LegStatus::Succeeded);
    let recipient_balance: Nat = query(
        &f.ic,
        f.ledger,
        person(99),
        "icrc1_balance_of",
        (account(person(2)),),
    );
    assert_eq!(recipient_balance, Nat::from(1000u64));
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "set_fee",
        (10u128,),
    );
    // Only the payer or the funds' owner chooses when to combine a refund.
    let stranger: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(99),
        "claim_refund",
        (e.escrow_id, vec![block], true),
    );
    assert_eq!(stranger, Err(Error::Forbidden));
    let excess: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "claim_refund",
        (e.escrow_id, vec![block], true),
    );
    let excess = excess.unwrap();
    assert_eq!(excess.amount, 17);
    let sent: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(99),
        "process_transfer",
        (e.escrow_id, excess.leg_id),
    );
    sent.unwrap();
    let done: Result<EscrowInfo> =
        query(&f.ic, f.payment, person(99), "get_escrow", (e.escrow_id,));
    let done = done.unwrap();
    assert!(funds_conserved(&done));
    assert_eq!(done.liabilities, 0);
    let input = f.order(&recipient, 2, 2);
    let second: Result<EscrowInfo> = update(&f.ic, f.payment, person(40), "open_escrow", (input,));
    let second = second.unwrap();
    let late_block = f.fund(&second, second.quote.amount);
    f.ic.advance_time(Duration::from_secs(46 * 60));
    let refund: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "expiry_refund",
        (second.escrow_id,),
    );
    assert_eq!(refund.unwrap().decision, FundsDecision::RefundCommitted);
    let late: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "check_funding",
        (second.escrow_id, late_block),
    );
    let late = late.unwrap();
    assert!(late.funding_ref.is_none());
    let leg: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "claim_refund",
        (second.escrow_id, vec![late_block], true),
    );
    let leg = leg.unwrap();
    assert_eq!(leg.to, account(person(40)));
    let result: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "process_transfer",
        (second.escrow_id, leg.leg_id),
    );
    result.unwrap();
    f.ic.upgrade_canister(
        f.payment,
        wasm("dmsg_payment"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let restored: Result<EscrowInfo> = query(
        &f.ic,
        f.payment,
        person(40),
        "get_escrow",
        (second.escrow_id,),
    );
    let restored = restored.unwrap();
    assert_eq!(restored.liabilities, 0);
    assert!(funds_conserved(&restored));
    let revoked: Result<()> = update(
        &f.ic,
        f.payment,
        Principal::anonymous(),
        "revoke_receipt_signer",
        (1u64,),
    );
    revoked.unwrap();
    let original: Result<EscrowInfo> =
        update(&f.ic, f.payment, person(99), "finalize_receipt", (signed,));
    assert_eq!(
        original.unwrap().decision,
        FundsDecision::SettlementCommitted
    );
}

fn funds_conserved(e: &EscrowInfo) -> bool {
    e.liabilities
        .checked_add(e.transferred)
        .and_then(|v| v.checked_add(e.network_fees))
        == Some(e.confirmed_in)
}
