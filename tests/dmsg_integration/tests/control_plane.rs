use candid::{utils::ArgumentEncoder, CandidType, Nat, Principal};
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
        let ic = PocketIcBuilder::new()
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
            environment: Environment::Local,
            executing_canister: cose,
            initial_home_user: user,
            derivation_version: 1,
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
                home_cose: cose,
                handle_canister: handle,
                payment_canister: payment,
                max_subjects: 1000,
                daily_new_subjects: 100,
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
                home_user: user,
                ledger,
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
                home_user: user,
                ledger,
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
            ic,
            user,
            cose,
            handle,
            payment,
            ledger,
            cose_config,
        }
    }
    fn create(&self, n: u8) -> Hash {
        let expires = time(&self.ic) + MINUTE;
        let dev = device(n);
        let op = Hash::new([n; 32]);
        let proof = key(n)
            .sign(
                digest(
                    "dmsg/create-subject/v1",
                    &(self.user, person(n), &dev, op, expires),
                )
                .as_slice(),
            )
            .to_bytes()
            .to_vec()
            .into();
        let r: Result<Hash> = update(
            &self.ic,
            self.user,
            person(n),
            "create_subject",
            (CreateSubject {
                device: dev,
                op_id: op,
                expires_at: expires,
                proof,
            },),
        );
        r.unwrap()
    }
    fn subject(&self, n: u8, id: Hash) -> Subject {
        let r: Result<Subject> = query(&self.ic, self.user, person(n), "get_subject", (id,));
        r.unwrap()
    }
    fn mutate(&self, n: u8, id: Hash, command: AccountCommand) -> Result<OperationReceipt> {
        let s = self.subject(n, id);
        let mut m = AccountMutation {
            subject: id,
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
                    "dmsg/account/v1",
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
    fn recoverable(&self, n: u8, id: Hash) {
        let s = self.subject(n, id);
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
        let s = self.subject(n, id);
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
    fn fund(&self, e: &Escrow, amount: u128) -> u64 {
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
    fn order(&self, recipient: Hash, n: u8, nonce: u8) -> OpenEscrow {
        let s = self.subject(n, recipient);
        let now = time(&self.ic);
        let offer = PaymentOffer {
            subject: recipient,
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
    fn receipt(&self, e: &Escrow) -> SignedReceipt {
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
    fn derive(&self, n: u8, subject: Hash, transport_key: Vec<u8>) -> ExecutionResult {
        let s = self.subject(n, subject);
        let kind = ExecutionKind::Derive {
            generation: 1,
            root_op_id: None,
            transport_key: transport_key.into(),
        };
        let cost = 100_000_000_000u128;
        let request_id = execution_request_id(
            subject,
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
                    subject,
                    "dmsg/execute/v1",
                    &(&kind, cost),
                    &approval,
                )
                .as_slice(),
            )
            .to_bytes()
            .to_vec()
            .into();
        let r: Result<ExecutionResult> = update(
            &self.ic,
            self.user,
            person(n),
            "authorize_and_execute",
            (ExecuteRequest {
                subject,
                kind,
                max_cycles: cost,
                approval,
            },),
        );
        r.unwrap()
    }
}

#[test]
fn identity_roots_certification_formal_signing_and_upgrade() {
    let f = Fixture::new();
    let subject = f.create(1);
    assert_eq!(f.create(1), subject);
    f.recoverable(1, subject);
    f.mutate(
        1,
        subject,
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
        derivation_version: 1,
        key_generation: 1,
        bundle_digest: Hash::new([12; 32]),
        recovery_generation: 1,
    };
    f.mutate(
        1,
        subject,
        AccountCommand::CommitRoot {
            expected_generation: 0,
            op_id: Hash::new([8; 32]),
            root: root.clone(),
        },
    )
    .unwrap();
    let s = f.subject(1, subject);
    assert_eq!(s.current_root, Some(root));
    let batch: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "security_snapshot_batch",
        (vec![subject],),
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
        witness.lookup_path([subject.as_slice()]),
        ic_certification::LookupResult::Found(batch.entries[0].value.as_ref().unwrap())
    );
    let reply: Result<()> = update(&f.ic, f.cose, person(1), "register_subject", (subject,));
    assert_eq!(reply, Err(Error::Forbidden));
    #[derive(CandidType, Deserialize)]
    struct KeyState {
        initialization: Initialization,
    }
    let init: Result<KeyState> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    assert_eq!(init.unwrap().initialization, Initialization::Ready);
    let s = f.subject(1, subject);
    let expires = time(&f.ic) + MINUTE;
    let request_id = execution_request_id(
        subject,
        s.security_epoch,
        Hash::new([1; 32]),
        s.devices[&Hash::new([1; 32])].next_sequence,
    );
    let payload = FormalPayload {
        schema: 1,
        subject,
        request_id,
        origin: "https://example.com".into(),
        audience: "release".into(),
        expires_at: expires,
        body: FormalBody::FileAttestation {
            sha256: Hash::new([31; 32]),
            size: 123,
            version: Hash::new([4; 32]),
            project: "example".into(),
        },
    };
    let bytes = canonical(&payload);
    let kind = ExecutionKind::Sign {
        key: KeyRequest {
            purpose: KeyPurpose::FileAttestation,
            algorithm: Algorithm::Ed25519,
            generation: 1,
            provider: None,
        },
        canonical_payload: bytes.clone().into(),
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
                subject,
                "dmsg/execute/v1",
                &(&kind, cost),
                &approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .to_vec()
        .into();
    let request = ExecuteRequest {
        subject,
        kind,
        max_cycles: cost,
        approval,
    };
    let result: Result<ExecutionResult> = update(
        &f.ic,
        f.user,
        person(1),
        "authorize_and_execute",
        (request.clone(),),
    );
    let result = result.unwrap();
    assert_eq!(result.status, ExecutionStatus::Completed);
    verify(
        &result
            .key
            .as_ref()
            .unwrap()
            .public_key
            .to_vec()
            .try_into()
            .map(Hash::new)
            .unwrap(),
        bytes.as_slice(),
        result.result.as_ref().unwrap(),
    )
    .unwrap();
    let replay: Result<ExecutionResult> = update(
        &f.ic,
        f.user,
        person(1),
        "authorize_and_execute",
        (request,),
    );
    assert_eq!(replay.unwrap(), result);
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32]).unwrap();
    let derived = f.derive(1, subject, transport.public_key());
    assert_eq!(derived.status, ExecutionStatus::Completed);
    let encrypted =
        ic_vetkeys::EncryptedVetKey::deserialize(derived.result.as_ref().unwrap()).unwrap();
    let public =
        ic_vetkeys::DerivedPublicKey::deserialize(&derived.key.as_ref().unwrap().public_key)
            .unwrap();
    let root = encrypted
        .decrypt_and_verify(&transport, &public, &canonical(&(subject, 1u64)))
        .unwrap();
    assert!(encrypted
        .decrypt_and_verify(&transport, &public, &canonical(&(subject, 2u64)))
        .is_err());
    let other = ic_vetkeys::TransportSecretKey::from_seed(vec![92; 32]).unwrap();
    assert!(encrypted
        .decrypt_and_verify(&other, &public, &canonical(&(subject, 1u64)))
        .is_err());
    let derived = f.derive(1, subject, other.public_key());
    let same = ic_vetkeys::EncryptedVetKey::deserialize(derived.result.as_ref().unwrap())
        .unwrap()
        .decrypt_and_verify(&other, &public, &canonical(&(subject, 1u64)))
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
    assert_eq!(f.subject(1, subject).current_root, s.current_root);
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
        subject: owner,
        target_subject: None,
        handle: "alice".into(),
        expected_version: 0,
        op_id: Hash::new([6; 32]),
        terms_digest: digest(
            "dmsg/legacy-claim/v1",
            &(snapshot.snapshot_id, &legacy, owner),
        ),
    };
    f.mutate(
        1,
        owner,
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
    assert_eq!(claimed.unwrap().owner_subject, owner);
    let op_id = Hash::new([10; 32]);
    let terms = digest(
        "dmsg/handle-transfer/v1",
        &(f.handle, "alice", owner, target, 1u64, op_id),
    );
    let from = HandleIntent {
        handle_canister: f.handle,
        action: HandleAction::Transfer,
        subject: owner,
        target_subject: Some(target),
        handle: "alice".into(),
        expected_version: 1,
        op_id,
        terms_digest: terms,
    };
    let accept = HandleIntent {
        action: HandleAction::AcceptTransfer,
        subject: target,
        target_subject: Some(owner),
        ..from.clone()
    };
    f.mutate(
        1,
        owner,
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
        target,
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
    assert_eq!(transferred.owner_subject, target);
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
        subject: owner,
        target_subject: None,
        handle: "newname".into(),
        expected_version: 0,
        op_id: Hash::new([11; 32]),
        terms_digest: charge_terms_digest(f.ledger, &payer, amount, 10u128),
    };
    f.mutate(
        1,
        owner,
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
        (owner, Hash::new([11u8; 32])),
    );
    assert_eq!(committed.unwrap().phase, HandlePhase::Committed);
    let replay: Result<HandleOperation> = update(
        &f.ic,
        f.handle,
        person(1),
        "commit_handle",
        (owner, Hash::new([11u8; 32])),
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
    let input = f.order(recipient, 2, 1);
    let opened: Result<Escrow> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (input.clone(),),
    );
    let e = opened.unwrap();
    let replay: Result<Escrow> = update(&f.ic, f.payment, person(40), "open_escrow", (input,));
    assert_eq!(replay.unwrap(), e);
    let block = f.fund(&e, e.quote.amount + 17);
    let funded: Result<Escrow> = update(
        &f.ic,
        f.payment,
        person(40),
        "check_funding",
        (e.escrow_id, block),
    );
    let funded = funded.unwrap();
    assert_eq!(funded.funding_ref, Some(block));
    assert!(funded.conserved());
    let signed = f.receipt(&funded);
    let decided: Result<Escrow> = update(
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
    let duplicate: Result<Escrow> = update(
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
    let done: Result<Escrow> = query(&f.ic, f.payment, person(99), "get_escrow", (e.escrow_id,));
    let done = done.unwrap();
    assert!(done.conserved());
    assert_eq!(done.liabilities, 0);
    let input = f.order(recipient, 2, 2);
    let second: Result<Escrow> = update(&f.ic, f.payment, person(40), "open_escrow", (input,));
    let second = second.unwrap();
    let late_block = f.fund(&second, 1130);
    f.ic.advance_time(Duration::from_secs(46 * 60));
    let refund: Result<Escrow> = update(
        &f.ic,
        f.payment,
        person(40),
        "expiry_refund",
        (second.escrow_id,),
    );
    assert_eq!(refund.unwrap().decision, FundsDecision::RefundCommitted);
    let late: Result<Escrow> = update(
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
    let restored: Result<Escrow> = query(
        &f.ic,
        f.payment,
        person(40),
        "get_escrow",
        (second.escrow_id,),
    );
    let restored = restored.unwrap();
    assert_eq!(restored.liabilities, 0);
    assert!(restored.conserved());
    let revoked: Result<()> = update(
        &f.ic,
        f.payment,
        Principal::anonymous(),
        "revoke_receipt_signer",
        (1u64,),
    );
    revoked.unwrap();
    let original: Result<Escrow> =
        update(&f.ic, f.payment, person(99), "finalize_receipt", (signed,));
    assert_eq!(
        original.unwrap().decision,
        FundsDecision::SettlementCommitted
    );
}
