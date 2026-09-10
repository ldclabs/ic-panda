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
// Exercise the public typed interfaces while retaining independently assembled
// approval bytes in the existing interoperability/regression fixtures.
fn submit_execution(
    ic: &PocketIc,
    user: Principal,
    caller: Principal,
    request: ExecuteRequest,
) -> Result<ExecutionResult> {
    match request.kind {
        ExecutionKind::Sign {
            key,
            to_be_signed,
            public_key_fingerprint,
            origin,
        } => {
            let prepared = parse_signing_input(&to_be_signed).unwrap();
            let algorithm = match key.algorithm {
                Algorithm::Ed25519 => SigningAlgorithm::Ed25519,
                Algorithm::EcdsaSecp256k1 => SigningAlgorithm::EcdsaSecp256k1,
                _ => panic!("signing algorithm"),
            };
            update(
                ic,
                user,
                caller,
                "sign",
                (SignRequest {
                    account_id: request.account_id,
                    key: SigningKeyRef {
                        algorithm,
                        kid: prepared.kid.into(),
                        public_key_fingerprint,
                    },
                    statement: prepared.statement,
                    origin,
                    max_cycles: request.max_cycles,
                    approval: request.approval,
                },),
            )
        }
        ExecutionKind::Derive {
            generation,
            root_op_id,
            transport_key,
        } => update(
            ic,
            user,
            caller,
            "derive_root",
            (DeriveRootRequest {
                account_id: request.account_id,
                target: match root_op_id {
                    Some(op_id) => RootTarget::Candidate { generation, op_id },
                    None => RootTarget::Current { generation },
                },
                transport_public_key: transport_key
                    .as_slice()
                    .try_into()
                    .map(serde_bytes::ByteArray::new)
                    .unwrap(),
                max_cycles: request.max_cycles,
                approval: request.approval,
            },),
        ),
    }
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
struct Fixture {
    ic: PocketIc,
    user: Principal,
    cose: Principal,
    handle: Principal,
    payment: Principal,
    ledger: Principal,
    cose_config: CoseInit,
}
impl Fixture {
    fn new() -> Self {
        Self::with_algorithms(vec![Algorithm::Ed25519, Algorithm::VetKdBls12381])
    }
    fn with_algorithms(algorithms: Vec<Algorithm>) -> Self {
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
        for id in [user, cose, handle, payment, ledger] {
            ic.add_cycles(id, 10_000_000_000_000_000);
        }
        let cose_config = CoseInit {
            issuer_namespace: NAMESPACE.into(),
            environment: Environment::Local,
            executing_canister: cose,
            initial_home_user: user,
            derivation_version: 2,
            masters: algorithms
                .into_iter()
                .map(|algorithm| MasterKey {
                    algorithm,
                    key_name: "key_1".into(),
                    expected_fingerprint: Hash::new([0; 32]),
                })
                .collect(),
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
                issuer_namespace: NAMESPACE.into(),
                environment: Environment::Local,
                home_cose: cose,
                handle_canister: handle,
                payment_canister: payment,
                max_accounts: 1000,
                daily_new_accounts: 100,
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
                home_user: user,
                ledger_fee: 10,
                max_pending: 100,
            },))
            .unwrap(),
            None,
        );
        ic.install_canister(
            payment,
            wasm("dmsg_payment"),
            candid::encode_args((PaymentInit {
                ledger,
                home_user: user,
                platform: account(person(60)),
                service_fee: 100,
                ledger_fee: 10,
                max_fee: 20,
                signer: signer.clone(),
                max_open_per_payer: 4,
                daily_orders: 100,
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
        Self {
            ledger,
            ic,
            user,
            cose,
            handle,
            payment,
            cose_config,
        }
    }
    fn key_ref(
        &self,
        id: &AccountId,
        purpose: SigningPurpose,
        algorithm: SigningAlgorithm,
    ) -> SigningKeyRef {
        let result: Result<KeyDescriptor> = query(
            &self.ic,
            self.cose,
            Principal::anonymous(),
            "public_key",
            (
                id,
                KeySelector::Signing(dmsg_types::cose::SigningKey {
                    purpose,
                    algorithm: algorithm.clone(),
                }),
            ),
        );
        let key = result.unwrap();
        SigningKeyRef {
            algorithm,
            kid: key.key_id,
            public_key_fingerprint: key.public_key_fingerprint,
        }
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
            .to_vec()
            .into();
        CreateAccount {
            device: dev,
            op_id: op,
            expires_at: expires,
            proof,
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
        let s = self.account_id(n, id);
        let mut m = AccountMutation {
            account_id: id.clone(),
            expected_version: s.account_version,
            command,
            approval: Approval {
                device_id: Hash::new([n; 32]),
                security_epoch: s.security_epoch,
                sequence: s.devices[&Hash::new([n; 32])].next_sequence,
                request_id: digest("test-operation", &(id, s.account_version)),
                expires_at: time(&self.ic) + MINUTE,
                signature: ByteBuf::new(),
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
            .to_vec()
            .into();
        update(&self.ic, self.user, person(n), "mutate_account", (m,))
    }
    fn recoverable(&self, n: u8, id: &AccountId) {
        let s = self.account_id(n, id);
        let op = digest("test-operation", &(id, s.account_version));
        let policy = RecoveryPolicy {
            generation: 1,
            signing_pub: key(70).verifying_key().to_bytes().into(),
            hpke_pub: Hash::new([70; 32]),
            delay_ms: DAY,
        };
        let proof = key(70)
            .sign(digest("dmsg/recovery-enroll/v1", &(self.user, id, &policy, op)).as_slice())
            .to_bytes()
            .to_vec()
            .into();
        self.mutate(n, id, AccountCommand::SetRecovery { policy, proof })
            .unwrap();
        let s = self.account_id(n, id);
        let op = digest("test-operation", &(id, s.account_version));
        let proof = key(70)
            .sign(
                digest(
                    "dmsg/recovery-check/v1",
                    &(self.user, id, 1u64, s.account_version, op),
                )
                .as_slice(),
            )
            .to_bytes()
            .to_vec()
            .into();
        self.mutate(n, id, AccountCommand::ConfirmRecovery { proof })
            .unwrap();
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
        let s = self.account_id(n, recipient);
        let now = time(&self.ic);
        let offer = PaymentOffer {
            account_id: recipient.clone(),
            device_id: Hash::new([n; 32]),
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
        let signature = key(n)
            .sign(digest("dmsg/payment-offer/v1", &offer).as_slice())
            .to_bytes()
            .to_vec()
            .into();
        let quote = Quote {
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
            fee_reserve: 30,
            amount: 1130,
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
            .sign(digest("dmsg/quote/v1", &quote).as_slice())
            .to_bytes()
            .to_vec()
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
            protocol: 1,
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
            .sign(digest("dmsg/admission-receipt/v1", &receipt).as_slice())
            .to_bytes()
            .to_vec()
            .into();
        SignedReceipt { receipt, signature }
    }
    fn derive(&self, n: u8, account_id: &AccountId, transport_key: Vec<u8>) -> ExecutionResult {
        let s = self.account_id(n, account_id);
        let kind = ExecutionKind::Derive {
            generation: 1,
            root_op_id: None,
            transport_key: transport_key.into(),
        };
        let cost = 100_000_000_000u128;
        let request_id = execution_request_id(
            account_id,
            s.security_epoch,
            Hash::new([n; 32]),
            s.devices[&Hash::new([n; 32])].next_sequence,
        );
        let mut approval = Approval {
            device_id: Hash::new([n; 32]),
            security_epoch: s.security_epoch,
            sequence: s.devices[&Hash::new([n; 32])].next_sequence,
            request_id,
            expires_at: time(&self.ic) + MINUTE,
            signature: ByteBuf::new(),
        };
        approval.signature = key(n)
            .sign(
                approval_message(
                    self.user,
                    account_id,
                    "dmsg/execute/v3",
                    &(&kind, cost),
                    &approval,
                )
                .as_slice(),
            )
            .to_bytes()
            .to_vec()
            .into();
        let r = submit_execution(
            &self.ic,
            self.user,
            person(n),
            ExecuteRequest {
                account_id: account_id.clone(),
                kind,
                max_cycles: cost,
                approval,
            },
        );
        r.unwrap()
    }
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
    bad.proof = vec![0; 64].into();
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
    assert_eq!(account.issuer, account_issuer(NAMESPACE, &c).unwrap());
}

#[test]
fn identity_roots_certification_formal_signing_and_upgrade() {
    let f = Fixture::new();
    let account_id = f.create(1);
    assert_eq!(f.create(1), account_id);
    f.recoverable(1, &account_id);
    f.mutate(
        1,
        &account_id,
        AccountCommand::ReserveRoot {
            expected_generation: 0,
            op_id: Hash::new([8; 32]),
        },
    )
    .unwrap();
    let root = ContentRootRef {
        generation: 1,
        suite: "dmsg-root-v1".into(),
        home_cose: f.cose,
        derivation_version: 2,
        key_generation: 1,
        bundle_digest: Hash::new([12; 32]),
        recovery_generation: 1,
    };
    f.mutate(
        1,
        &account_id,
        AccountCommand::CommitRoot {
            expected_generation: 0,
            op_id: Hash::new([8; 32]),
            root: root.clone(),
        },
    )
    .unwrap();
    let s = f.account_id(1, &account_id);
    assert_eq!(s.current_root, Some(root));
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
    #[derive(CandidType, Deserialize)]
    struct KeyState {
        initialization: Initialization,
    }
    let init: Result<KeyState> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    assert_eq!(init.unwrap().initialization, Initialization::Ready);
    let s = f.account_id(1, &account_id);
    let expires = time(&f.ic) + MINUTE;
    let request_id = execution_request_id(
        &account_id,
        s.security_epoch,
        Hash::new([1; 32]),
        s.devices[&Hash::new([1; 32])].next_sequence,
    );
    let payload = Statement {
        issuer: account_issuer(NAMESPACE, &account_id).unwrap(),
        subject: Some("release/spec".into()),
        issued_at: None,
        content: StatementContent::Digest {
            sha256: Hash::new([31; 32]),
            content_type: Some("application/pdf".into()),
            location: None,
        },
    };
    let selected = f.key_ref(
        &account_id,
        SigningPurpose::FileAttestation,
        SigningAlgorithm::Ed25519,
    );
    let (_, to_be_signed) = prepare_cose(&payload, &Algorithm::Ed25519, &selected.kid).unwrap();
    let kind = ExecutionKind::Sign {
        key: KeyRequest {
            purpose: KeyPurpose::FileAttestation,
            algorithm: Algorithm::Ed25519,
            generation: 1,
        },
        to_be_signed: to_be_signed.into(),
        public_key_fingerprint: selected.public_key_fingerprint,
        origin: "https://example.com".into(),
    };
    let mut approval = Approval {
        device_id: Hash::new([1; 32]),
        security_epoch: s.security_epoch,
        sequence: s.devices[&Hash::new([1; 32])].next_sequence,
        request_id,
        expires_at: expires,
        signature: ByteBuf::new(),
    };
    let cost = 100_000_000_000u128;
    approval.signature = key(1)
        .sign(
            approval_message(
                f.user,
                &account_id,
                "dmsg/execute/v3",
                &(&kind, cost),
                &approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .to_vec()
        .into();
    let request = ExecuteRequest {
        account_id: account_id.clone(),
        kind,
        max_cycles: cost,
        approval,
    };
    let result = submit_execution(&f.ic, f.user, person(1), request.clone());
    let result = result.unwrap();
    assert_eq!(result.status(), ExecutionStatus::Completed);
    let ExecutionOutput::Signature { artifact, .. } = result.output().unwrap() else {
        panic!("signature output")
    };
    assert_eq!(verify_artifact(artifact).unwrap(), payload);
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
                artifact,
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
    match_execution_receipt(artifact, &receipt).unwrap();
    assert_eq!(receipt.request_id, request_id);
    assert_eq!(receipt.account_id, account_id);
    assert_eq!(receipt.origin, "https://example.com");
    let mut altered = receipt.clone();
    altered.to_be_signed_digest = Hash::new([99; 32]);
    assert_eq!(
        match_execution_receipt(artifact, &altered),
        Err(Error::IntegrityFailed)
    );
    altered = receipt.clone();
    altered.public_key_fingerprint = Hash::new([99; 32]);
    assert_eq!(
        match_execution_receipt(artifact, &altered),
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
    let replay = submit_execution(&f.ic, f.user, person(1), request);
    assert_eq!(replay.unwrap(), result);
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32]).unwrap();
    let derived = f.derive(1, &account_id, transport.public_key());
    assert_eq!(derived.status(), ExecutionStatus::Completed);
    let encrypted =
        ic_vetkeys::EncryptedVetKey::deserialize(derived.output().unwrap().bytes()).unwrap();
    let public =
        ic_vetkeys::DerivedPublicKey::deserialize(&derived.output().unwrap().key().public_key)
            .unwrap();
    let root = encrypted
        .decrypt_and_verify(&transport, &public, &canonical(&(&account_id, 1u64)))
        .unwrap();
    assert!(encrypted
        .decrypt_and_verify(&transport, &public, &canonical(&(&account_id, 2u64)))
        .is_err());
    let other = ic_vetkeys::TransportSecretKey::from_seed(vec![92; 32]).unwrap();
    assert!(encrypted
        .decrypt_and_verify(&other, &public, &canonical(&(&account_id, 1u64)))
        .is_err());
    let derived = f.derive(1, &account_id, other.public_key());
    let same = ic_vetkeys::EncryptedVetKey::deserialize(derived.output().unwrap().bytes())
        .unwrap()
        .decrypt_and_verify(&other, &public, &canonical(&(&account_id, 1u64)))
        .unwrap();
    assert_eq!(root.serialize(), same.serialize());
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
        candid::encode_args((None::<CoseInit>,)).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(f.account_id(1, &account_id).current_root, s.current_root);
    let restored: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_receipt",
        (&account_id, request_id),
    );
    let restored: ExecutionReceipt = decode_canonical(&certified_value(
        &f,
        restored.unwrap(),
        &execution_receipt_key(&account_id, request_id),
    ))
    .unwrap();
    assert_eq!(restored, receipt);
    match_execution_receipt(artifact, &restored).unwrap();
    // Both stores must rehydrate independently persisted executions after upgrade.
    for expected in [result, derived] {
        let local: Result<ExecutionResult> = query(
            &f.ic,
            f.user,
            person(1),
            "get_execution",
            (&account_id, expected.request_id),
        );
        let remote: Result<ExecutionResult> = query(
            &f.ic,
            f.cose,
            f.user,
            "get_execution",
            (&account_id, expected.request_id),
        );
        assert_eq!(local.unwrap(), expected);
        assert_eq!(remote.unwrap(), expected);
        let replay: Result<ExecutionResult> = update(
            &f.ic,
            f.user,
            person(1),
            "reconcile_execution",
            (&account_id, expected.request_id),
        );
        assert_eq!(replay.unwrap(), expected);
    }

    let mut changed = f.cose_config.clone();
    changed.masters[0].key_name = "test_key_1".into();
    assert!(f
        .ic
        .upgrade_canister(
            f.cose,
            wasm("dmsg_cose"),
            candid::encode_args((Some(changed),)).unwrap(),
            None
        )
        .is_err());
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
        (snapshot.snapshot_id, vec![legacy.clone()]),
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
        account_id: owner.clone(),
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
        account_id: owner.clone(),
        target_account: Some(target.clone()),
        handle: "alice".into(),
        expected_version: 1,
        op_id,
        terms_digest: terms,
    };
    let accept = HandleIntent {
        action: HandleAction::AcceptTransfer,
        account_id: target.clone(),
        target_account: Some(owner.clone()),
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
        account_id: owner.clone(),
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
    let reserved: Result<HandleOperation> = update(
        &f.ic,
        f.handle,
        person(1),
        "reserve_handle",
        (Registration {
            intent,
            payer,
            fee: 10,
        },),
    );
    assert_eq!(reserved.unwrap().phase, HandlePhase::Reserved);
    let committed: Result<HandleOperation> = update(
        &f.ic,
        f.handle,
        person(1),
        "commit_handle",
        (&owner, Hash::new([11u8; 32])),
    );
    assert_eq!(committed.unwrap().phase, HandlePhase::Committed);
    let replay: Result<HandleOperation> = update(
        &f.ic,
        f.handle,
        person(1),
        "commit_handle",
        (&owner, Hash::new([11u8; 32])),
    );
    assert_eq!(replay.unwrap().phase, HandlePhase::Committed);
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
    let excess: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(99),
        "claim_deposit_refund",
        (e.escrow_id, block),
    );
    let excess = excess.unwrap();
    assert_eq!(excess.amount, 7);
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
    let late_block = f.fund(&second, 1130);
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
        "claim_deposit_refund",
        (second.escrow_id, late_block),
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
