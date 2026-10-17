//! Team wallets end to end: paid activation through the hub, members
//! joining with device approvals, ICP transfers, threshold signing for
//! Bitcoin, EVM and Solana, cosigners, freezes, vetoes, upgrades, locks,
//! cycles top-ups and refunds.
use super::*;
use dmsg_protocol::wallet::*;
use dmsg_types::{integration::*, wallet::*};
use std::io::Write as _;

const A: u8 = 41;
const B: u8 = 42;
const P: u8 = 43;
const RECOVERY: u8 = 44;
const PRICE: u64 = 1_000_000_000;
const SHARE: u64 = 300_000_000;

fn gz(bytes: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    e.write_all(bytes).unwrap();
    e.finish().unwrap()
}

/// The wallet module with an extra custom section, so it hashes differently.
fn variant(bytes: &[u8], tag: &[u8]) -> Vec<u8> {
    let name = b"dmsg-test";
    let body = [&[name.len() as u8][..], name, tag].concat();
    [bytes, &[0x00, body.len() as u8], &body].concat()
}

struct Team {
    f: Fixture,
    hub: Principal,
    wallet: Principal,
    a: AccountId,
    b: AccountId,
    quote: Hash,
}

fn member_device(n: u8) -> DeviceInput {
    let mut d = device(n);
    d.capabilities.push(Capability::WalletApprove);
    d
}

fn create_member(f: &Fixture, n: u8) -> AccountId {
    let mut input = f.create_input(n);
    input.device = member_device(n);
    input.proof = key(n)
        .sign(
            digest(
                "dmsg/create-account/v1",
                &(
                    f.user,
                    person(n),
                    &input.device,
                    input.op_id,
                    input.expires_at,
                ),
            )
            .as_slice(),
        )
        .to_bytes()
        .into();
    let r: Result<AccountId> = update(&f.ic, f.user, person(n), "create_account", (input,));
    r.unwrap()
}

fn register_module(f: &Fixture, hub: Principal, module: &[u8]) -> WalletVersion {
    let _: Result<()> = update(&f.ic, hub, f.sns, "clear_wasm_staging", ());
    for chunk in module.chunks(1024 * 1024) {
        let staged: Result<StagedModule> = update(
            &f.ic,
            hub,
            f.sns,
            "upload_wasm_chunk",
            (ByteBuf::from(chunk.to_vec()),),
        );
        staged.unwrap();
    }
    let hash = sha256(module);
    let rendered: std::result::Result<String, String> = query(
        &f.ic,
        hub,
        f.sns,
        "validate_admin_register_version",
        (hash, "test".to_string()),
    );
    assert!(rendered.unwrap().contains("Register wallet module"));
    let v: Result<WalletVersion> = update(
        &f.ic,
        hub,
        f.sns,
        "admin_register_version",
        (hash, "test".to_string()),
    );
    v.unwrap()
}

fn setup(f: &Fixture) -> Principal {
    let cmc = f.ic.create_canister();
    f.ic.add_cycles(cmc, 10_000_000_000_000_000);
    f.ic.install_canister(
        cmc,
        wasm("dmsg_test_cmc"),
        candid::encode_one(f.ledger).unwrap(),
        None,
    );
    let hub = f.ic.create_canister();
    f.ic.add_cycles(hub, 100_000_000_000_000);
    f.ic.install_canister(
        hub,
        wasm("dmsg_wallet_hub"),
        candid::encode_one(HubInit {
            environment: Environment::Local,
            governance: f.sns,
            icp_ledger: f.ledger,
            cmc,
            commerce: f.commerce,
            issuer_namespace: NAMESPACE.into(),
            user_homes: vec![f.user],
            key_name: "key_1".into(),
            btc_network: BtcNetwork::Regtest,
            pricing: HubPricing {
                price_e8s: PRICE,
                cycles_to_wallet: 3_000_000_000_000,
                cycles_share_e8s: SHARE,
                quotas: vec![PlanQuota {
                    plan: dmsg_types::billing::PlanId::Free,
                    wallets: 2,
                }],
                paused: false,
            },
            revenue: account(person(70)),
        })
        .unwrap(),
        None,
    );
    let product = ProductRegistration {
        version: 2,
        environment: Environment::Local,
        product_id: WALLET_PRODUCT_ID.into(),
        config_version: 1,
        quote_authority: hub,
        beneficiary_authorities: vec![hub],
        adapter: hub,
        subject_schema: WALLET_SUBJECT_SCHEMA.into(),
        subject_size: 10,
        merchant: account(person(70)),
        ledgers: vec![f.ledger],
        terms_hash: Hash::new([77; 32]),
        paused: false,
    };
    let r: Result<()> = update(
        &f.ic,
        f.commerce,
        f.sns,
        "register_integration_product",
        (product,),
    );
    r.unwrap();
    let app = AppRegistration {
        version: 1,
        environment: Environment::Local,
        app_id: WALLET_APP_ID.into(),
        config_version: 1,
        origins: vec!["https://dmsg.net".into()],
        product_ids: vec![WALLET_PRODUCT_ID.into()],
        capabilities: vec![AppCapability::Checkout],
        profiles: vec![],
        authentication_receiver: hub,
        action_authority: hub,
        action_schema: None,
        paused: false,
    };
    let r: Result<()> = update(&f.ic, f.commerce, f.sns, "register_integration_app", (app,));
    r.unwrap();
    register_module(f, hub, &gz(&wasm("dmsg_wallet")));
    hub
}

/// Device `n` of `account` approves `application`; returns the approval ID.
fn approve_app(f: &Fixture, n: u8, account: &AccountId, application: &ApplicationApproval) -> Hash {
    let s = f.account_id(n, account);
    let device = Hash::new([n; 32]);
    let mut approval = Approval {
        device_id: device,
        security_epoch: s.security_epoch,
        sequence: s.devices[&device].next_sequence,
        request_id: digest(
            "wallet-test-approval",
            &(application, s.devices[&device].next_sequence),
        ),
        expires_at: time(&f.ic) + 4 * MINUTE,
        signature: Default::default(),
    };
    approval.signature = key(n)
        .sign(
            approval_message(
                f.user,
                account,
                "dmsg/application/approve/v1",
                application,
                &approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .into();
    let id: Result<Hash> = update(
        &f.ic,
        f.user,
        person(n),
        "approve_application",
        (application.clone(), approval),
    );
    id.unwrap()
}

fn application(
    f: &Fixture,
    hub: Principal,
    service: Principal,
    account: AccountId,
    actor: Principal,
    purpose: ApprovalPurpose,
    action_digest: Hash,
) -> ApplicationApproval {
    ApplicationApproval {
        version: 1,
        environment: Environment::Local,
        app_id: WALLET_APP_ID.into(),
        app_config_version: 1,
        origin: "https://dmsg.net".into(),
        approving_account: account,
        service,
        beneficiary: wallet_beneficiary(hub, service),
        actor,
        purpose,
        action_digest,
        operation_id: digest("wallet-test-op", &action_digest),
        nonce: digest("wallet-test-nonce", &action_digest),
        expires_at_ms: time(&f.ic) + 4 * MINUTE,
    }
}

fn policy(approvers: u8) -> Policy {
    Policy {
        version: 1,
        approver_threshold: approvers,
        admin_threshold: 1,
        tiers: vec![Tier {
            asset: AssetKey::Icp,
            up_to: 1_000_000_000,
            approvals: approvers,
            delay_ms: 0,
            book_only: false,
        }],
        address_book_cooldown_ms: 0,
        daily_limits: vec![],
        spendable_assets: vec![
            AssetKey::Icp,
            AssetKey::Btc,
            AssetKey::Evm {
                chain_id: 1,
                token: None,
            },
            AssetKey::Sol { mint: None },
        ],
        call_targets: vec![],
        evm_chains: vec![1],
        proposal_ttl_ms: 7 * DAY,
        unfreeze_delay_ms: DAY,
        daily_signatures: 50,
        refuel: None,
        hardware_cosigners: false,
        public: false,
    }
}

fn spec(approvers: u8) -> WalletSpec {
    let member = |id, role, label: &str| MemberInput {
        id,
        role,
        label: label.into(),
        agent_ids: vec![],
    };
    WalletSpec {
        name: "ICPanda DAO".into(),
        members: vec![
            member(MemberId::Account(AccountId([0; 12])), Role::Approver, "b"),
            member(MemberId::Principal(person(P)), Role::Admin, "p"),
            member(MemberId::Principal(person(RECOVERY)), Role::Recovery, "sns"),
        ],
        policy: policy(approvers),
        channel_hint: None,
    }
}

impl Team {
    fn new(approvers: u8) -> Self {
        let f = Fixture::new();
        let hub = setup(&f);
        let a = create_member(&f, A);
        let b = create_member(&f, B);
        let quote: Result<ActivationQuote> =
            update(&f.ic, hub, person(A), "quote_activation", (a,));
        let quote = quote.unwrap();
        assert_eq!(quote.remaining, 2);
        assert_eq!(quote.plan, dmsg_types::billing::PlanId::Free);
        void(
            &f.ic,
            f.ledger,
            person(1),
            "mint_test",
            (quote.deposit, u128::from(PRICE)),
        );
        let mut spec = spec(approvers);
        spec.members[0].id = MemberId::Account(b);
        let app = application(
            &f,
            hub,
            hub,
            a,
            person(A),
            ApprovalPurpose::WalletCreate,
            activation_digest(&quote, &spec),
        );
        let approval_id = approve_app(&f, A, &a, &app);
        let order: Result<ActivationOrder> = update(
            &f.ic,
            hub,
            person(A),
            "create_wallet",
            (CreateWallet {
                quote_id: quote.quote_id,
                spec: spec.clone(),
                approval_id,
                approval: app,
            },),
        );
        let order = order.unwrap();
        assert_eq!(order.status, ActivationStatus::Ready);
        let wallet = order.wallet.unwrap();
        for _ in 0..10 {
            f.ic.tick();
        }
        let t = Self {
            f,
            hub,
            wallet,
            a,
            b,
            quote: quote.quote_id,
        };
        t.join_b();
        let joined: Result<Member> = update(
            &t.f.ic,
            wallet,
            person(P),
            "join",
            (MemberApproval::Principal,),
        );
        assert_eq!(joined.unwrap().status, MemberStatus::Active);
        t
    }

    fn join_b(&self) {
        let app = application(
            &self.f,
            self.hub,
            self.wallet,
            self.b,
            person(B),
            ApprovalPurpose::WalletJoin,
            join_digest(self.wallet, &self.b, person(B)),
        );
        let approval_id = approve_app(&self.f, B, &self.b, &app);
        let joined: Result<Member> = update(
            &self.f.ic,
            self.wallet,
            person(B),
            "join",
            (MemberApproval::Account {
                approval_id,
                approval: app,
            },),
        );
        let joined = joined.unwrap();
        assert_eq!(joined.status, MemberStatus::Active);
        assert_eq!(joined.principals, vec![person(B)]);
    }

    fn view(&self) -> WalletView {
        let v: Result<WalletView> = query(&self.f.ic, self.wallet, person(A), "get_wallet", ());
        v.unwrap()
    }

    fn propose(&self, by: u8, intent: Intent) -> Proposal {
        let p: Result<Proposal> = update(
            &self.f.ic,
            self.wallet,
            person(by),
            "propose",
            (ProposeArgs {
                op_id: digest("wallet-test-proposal", &(&intent, time(&self.f.ic))),
                intent,
                via_agent: None,
            },),
        );
        p.unwrap()
    }

    /// Account member `n` decides with a device approval.
    fn decide_account(
        &self,
        n: u8,
        account: AccountId,
        p: &Proposal,
        decision: Decision,
    ) -> Result<Proposal> {
        let app = application(
            &self.f,
            self.hub,
            self.wallet,
            account,
            person(n),
            ApprovalPurpose::WalletProposal,
            decision_digest(&p.digest, decision),
        );
        let approval_id = approve_app(&self.f, n, &account, &app);
        let method = match decision {
            Decision::Approve => "approve_proposal",
            Decision::Reject => "reject_proposal",
            Decision::Veto => "veto_proposal",
        };
        update(
            &self.f.ic,
            self.wallet,
            person(n),
            method,
            (
                p.id,
                MemberApproval::Account {
                    approval_id,
                    approval: app,
                },
            ),
        )
    }

    fn approve_p(&self, id: u64) -> Proposal {
        let p: Result<Proposal> = update(
            &self.f.ic,
            self.wallet,
            person(P),
            "approve_proposal",
            (id, MemberApproval::Principal),
        );
        p.unwrap()
    }

    fn execute(
        &self,
        id: u64,
        plan: Option<u32>,
        cosigs: Vec<CosignerSig>,
    ) -> Result<ExecuteResult> {
        update(
            &self.f.ic,
            self.wallet,
            person(A),
            "execute",
            (ExecuteArgs {
                proposal: id,
                plan,
                cosigs,
            },),
        )
    }

    /// Plan an execution and check it with the client's own decoder.
    fn prepare(&self, id: u64, params: ExecutionParams) -> Result<ExecutionPlan> {
        let plan: Result<ExecutionPlan> = update(
            &self.f.ic,
            self.wallet,
            person(A),
            "prepare_execution",
            (id, params),
        );
        if let Ok(plan) = &plan {
            let p: Result<Proposal> =
                query(&self.f.ic, self.wallet, person(A), "get_proposal", (id,));
            let summary =
                dmsg_client::wallet::decode::verify_plan(&self.view(), &p.unwrap().intent, plan)
                    .unwrap();
            assert_eq!(summary, plan.summary);
        }
        plan
    }

    fn balance(&self, owner: Account) -> u128 {
        let n: Nat = query(
            &self.f.ic,
            self.f.ledger,
            person(1),
            "icrc1_balance_of",
            (owner,),
        );
        u128::try_from(n.0).unwrap()
    }
}

#[test]
fn wallet_activation_membership_and_icp_transfer() {
    let t = Team::new(2);
    let v = t.view();
    assert_eq!(v.name, "ICPanda DAO");
    assert_eq!(
        v.addresses.len(),
        4,
        "ICP and the three default chain addresses"
    );
    // Every chain address derives offline from the certified key roots.
    for a in &v.addresses[1..] {
        let (root, ecdsa) = match a.chain {
            Chain::Evm => (v.key_roots.ecdsa.clone().unwrap(), true),
            Chain::Bitcoin => (v.key_roots.bip340.clone().unwrap(), true),
            _ => (v.key_roots.ed25519.clone().unwrap(), false),
        };
        let path: Vec<Vec<u8>> = derivation_path(a.chain, &a.kind, a.id).unwrap();
        if ecdsa {
            let master = ic_secp256k1::PublicKey::deserialize_sec1(&root.public_key).unwrap();
            let derivation = ic_secp256k1::DerivationPath::new(
                path.into_iter()
                    .map(ic_secp256k1::DerivationIndex)
                    .collect(),
            );
            let (child, _) =
                master.derive_subkey_with_chain_code(&derivation, &root.chain_code.into_array());
            assert_eq!(child.serialize_sec1(true), a.public_key.to_vec());
        } else {
            let master = ic_ed25519::PublicKey::deserialize_raw(&root.public_key).unwrap();
            let derivation = ic_ed25519::DerivationPath::new(
                path.into_iter().map(ic_ed25519::DerivationIndex).collect(),
            );
            let (child, _) =
                master.derive_subkey_with_chain_code(&derivation, &root.chain_code.into_array());
            assert_eq!(child.serialize_raw().to_vec(), a.public_key.to_vec());
        }
        assert_eq!(
            a.text,
            address_text(
                t.wallet,
                BtcNetwork::Regtest,
                a.id,
                a.chain,
                &a.kind,
                &a.public_key,
                None
            )
            .unwrap()
        );
    }
    let listed: Vec<WalletRecord> = query(
        &t.f.ic,
        t.hub,
        person(1),
        "list_wallets_of",
        (MemberId::Account(t.a),),
    );
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].wallet, t.wallet);
    // Outsiders read nothing.
    let denied: Result<WalletView> = query(&t.f.ic, t.wallet, person(99), "get_wallet", ());
    assert_eq!(denied, Err(Error::Forbidden));
    assert_refused(&t.f.ic, t.wallet, person(99), "freeze", ());

    void(
        &t.f.ic,
        t.f.ledger,
        person(1),
        "mint_test",
        (account(t.wallet), 5_000_000_000u128),
    );
    let p = t.propose(
        A,
        Intent::Transfer {
            from: 0,
            asset: AssetKey::Icp,
            to: Destination::Icrc(account(person(80))),
            amount: 100_000_000,
            max_fee: 10_000,
            memo: None,
        },
    );
    assert_eq!(p.requirement.approvals, 2);
    assert_eq!(p.status, ProposalStatus::Open);
    // A digest approved for another decision is refused.
    let app = application(
        &t.f,
        t.hub,
        t.wallet,
        t.a,
        person(A),
        ApprovalPurpose::WalletProposal,
        decision_digest(&p.digest, Decision::Reject),
    );
    let approval_id = approve_app(&t.f, A, &t.a, &app);
    let wrong: Result<Proposal> = update(
        &t.f.ic,
        t.wallet,
        person(A),
        "approve_proposal",
        (
            p.id,
            MemberApproval::Account {
                approval_id,
                approval: app,
            },
        ),
    );
    assert_eq!(wrong, Err(Error::Forbidden));
    let p = t.decide_account(A, t.a, &p, Decision::Approve).unwrap();
    assert_eq!(p.status, ProposalStatus::Open);
    assert_eq!(
        t.decide_account(A, t.a, &p, Decision::Approve),
        Err(Error::IdempotencyConflict)
    );
    let p = t.approve_p(p.id);
    assert_eq!(p.status, ProposalStatus::Ready);
    let done = t.execute(p.id, None, vec![]).unwrap();
    assert_eq!(done.proposal.status, ProposalStatus::Settled);
    assert!(
        matches!(done.proposal.outcome, Some(Outcome::Ledger { ref blocks }) if blocks.len() == 1)
    );
    assert_eq!(t.balance(account(person(80))), 100_000_000);
    // Executing again reports the settled proposal and moves nothing.
    assert_eq!(
        t.execute(p.id, None, vec![]).unwrap().proposal.status,
        ProposalStatus::Settled
    );
    assert_eq!(t.balance(account(person(80))), 100_000_000);

    // A rejection by enough approvers closes a proposal.
    let p = t.propose(
        A,
        Intent::Transfer {
            from: 0,
            asset: AssetKey::Icp,
            to: Destination::Icrc(account(person(81))),
            amount: 1,
            max_fee: 10_000,
            memo: None,
        },
    );
    let p = t.decide_account(B, t.b, &p, Decision::Reject).unwrap();
    assert_eq!(p.status, ProposalStatus::Open);
    let p: Result<Proposal> = update(
        &t.f.ic,
        t.wallet,
        person(P),
        "reject_proposal",
        (p.id, MemberApproval::Principal),
    );
    assert_eq!(p.unwrap().status, ProposalStatus::Rejected);

    // The audit log is one hash chain ending at the certified head.
    let events: Result<Vec<AuditEvent>> =
        query(&t.f.ic, t.wallet, person(A), "audit", (0u64, 200u16));
    let events = events.unwrap();
    let head = verify_audit(Hash::new([0; 32]), 0, &events).unwrap();
    let v = t.view();
    assert_eq!(head, v.audit_head);
    assert_eq!(events.len() as u64, v.audit_len);
    let certified: Result<CertifiedBatch> =
        query(&t.f.ic, t.wallet, person(A), "get_wallet_certified", ());
    let certified = certified.unwrap();
    assert_eq!(
        certified.entries[0].value.as_ref().map(|b| b.to_vec()),
        Some(canonical(&v))
    );
}

fn verify_bitcoin(raw: &[u8], plan: &ExecutionPlan, output_key: &[u8; 32]) {
    use bitcoin::secp256k1::{schnorr, Message, Secp256k1, XOnlyPublicKey};
    let tx: bitcoin::Transaction = bitcoin::consensus::deserialize(raw).unwrap();
    let secp = Secp256k1::verification_only();
    let key = XOnlyPublicKey::from_slice(output_key).unwrap();
    for (i, input) in tx.input.iter().enumerate() {
        let sig = schnorr::Signature::from_slice(&input.witness[0]).unwrap();
        secp.verify_schnorr(
            &sig,
            &Message::from_digest(plan.digests[i].into_array()),
            &key,
        )
        .unwrap();
    }
}

#[test]
fn wallet_signs_bitcoin_evm_and_solana() {
    let t = Team::new(1);
    let v = t.view();
    let btc = v
        .addresses
        .iter()
        .find(|a| a.chain == Chain::Bitcoin)
        .unwrap()
        .clone();
    let evm = v
        .addresses
        .iter()
        .find(|a| a.chain == Chain::Evm)
        .unwrap()
        .clone();
    let sol = v
        .addresses
        .iter()
        .find(|a| a.chain == Chain::Solana)
        .unwrap()
        .clone();

    // Bitcoin: plan, sign, replace with a higher fee, report.
    let p = t.propose(
        P,
        Intent::Transfer {
            from: btc.id,
            asset: AssetKey::Btc,
            to: Destination::Bitcoin(btc.text.clone()),
            amount: 50_000,
            max_fee: 20_000,
            memo: None,
        },
    );
    let p = t.approve_p(p.id);
    assert_eq!(p.status, ProposalStatus::Ready);
    let utxos = vec![Utxo {
        txid: Hash::new([5; 32]),
        vout: 1,
        value: 100_000,
        address: btc.id,
    }];
    let params = |fee_rate| ExecutionParams::Bitcoin {
        utxos: utxos.clone(),
        fee_rate,
        locktime: 0,
        timelock: false,
    };
    // Executing an unknown plan fails before anything is signed.
    assert_eq!(
        t.execute(p.id, Some(1), vec![]).err(),
        Some(Error::NotFound)
    );
    let plan = t.prepare(p.id, params(2)).unwrap();
    assert_eq!(plan.summary.outputs[0].amount, 50_000);
    let signed = t.execute(p.id, Some(plan.seq), vec![]).unwrap();
    assert_eq!(signed.proposal.status, ProposalStatus::Signed);
    let tx = signed.signed.unwrap();
    let (output_key, _) = taproot_output_key(&x_only(&btc.public_key).unwrap(), None).unwrap();
    verify_bitcoin(&tx.raw, &plan, &output_key);
    // A replacement must spend the same inputs.
    let other = ExecutionParams::Bitcoin {
        utxos: vec![Utxo {
            vout: 2,
            ..utxos[0].clone()
        }],
        fee_rate: 3,
        locktime: 0,
        timelock: false,
    };
    assert_eq!(t.prepare(p.id, other).err(), Some(Error::Forbidden));
    let bumped = t.prepare(p.id, params(5)).unwrap();
    let replaced = t.execute(p.id, Some(bumped.seq), vec![]).unwrap();
    verify_bitcoin(&replaced.signed.unwrap().raw, &bumped, &output_key);
    let reported: Result<Proposal> = update(
        &t.f.ic,
        t.wallet,
        person(A),
        "report",
        (ChainReport {
            proposal: p.id,
            plan: bumped.seq,
            outcome: ChainOutcome::Settled,
            note: "confirmed".into(),
        },),
    );
    assert_eq!(reported.unwrap().status, ProposalStatus::Settled);

    // EVM: one unsettled nonce per address; the signer recovers to the address.
    let transfer = |amount| Intent::Transfer {
        from: evm.id,
        asset: AssetKey::Evm {
            chain_id: 1,
            token: None,
        },
        to: Destination::Evm(Hash20::new([0x11; 20])),
        amount,
        max_fee: 10u128.pow(16),
        memo: None,
    };
    let evm_params = |nonce| ExecutionParams::Evm {
        nonce,
        max_fee_per_gas: 30_000_000_000,
        max_priority_fee_per_gas: 1_000_000_000,
        gas_limit: 21_000,
    };
    let p1 = t.approve_p(t.propose(P, transfer(1_000)).id);
    let plan = t.prepare(p1.id, evm_params(0)).unwrap();
    let tx = t
        .execute(p1.id, Some(plan.seq), vec![])
        .unwrap()
        .signed
        .unwrap();
    {
        use alloy_consensus::TxEnvelope;
        use alloy_eips::eip2718::Decodable2718;
        let TxEnvelope::Eip1559(signed) = TxEnvelope::decode_2718(&mut tx.raw.as_slice()).unwrap()
        else {
            panic!("EIP-1559")
        };
        assert_eq!(
            signed.recover_signer().unwrap().0 .0,
            evm_address(&evm.public_key).unwrap()
        );
        assert_eq!(format!("{:#x}", signed.hash()), tx.txid);
    }
    let p2 = t.approve_p(t.propose(P, transfer(2_000)).id);
    assert_eq!(
        t.prepare(p2.id, evm_params(1)).err(),
        Some(Error::Forbidden)
    );
    // Cancel the pending nonce with a zero self-transfer, then the next nonce signs.
    let cancel = t.approve_p(
        t.propose(
            P,
            Intent::CancelEvmNonce {
                address: evm.id,
                chain_id: 1,
                max_fee: 10u128.pow(16),
            },
        )
        .id,
    );
    let plan = t.prepare(cancel.id, evm_params(0)).unwrap();
    assert_eq!(plan.summary.outputs[0].to, evm.text);
    t.execute(cancel.id, Some(plan.seq), vec![]).unwrap();
    let settled: Result<Proposal> = update(
        &t.f.ic,
        t.wallet,
        person(A),
        "report",
        (ChainReport {
            proposal: cancel.id,
            plan: plan.seq,
            outcome: ChainOutcome::Settled,
            note: String::new(),
        },),
    );
    settled.unwrap();
    let replaced: Result<Proposal> = query(&t.f.ic, t.wallet, person(A), "get_proposal", (p1.id,));
    assert_eq!(replaced.unwrap().status, ProposalStatus::Failed);
    let plan = t.prepare(p2.id, evm_params(1)).unwrap();
    t.execute(p2.id, Some(plan.seq), vec![]).unwrap();

    // Solana: the Ed25519 signature covers the exact message.
    let p = t.approve_p(
        t.propose(
            P,
            Intent::Transfer {
                from: sol.id,
                asset: AssetKey::Sol { mint: None },
                to: Destination::Solana(Hash::new([0x55; 32])),
                amount: 1_000_000,
                max_fee: 100_000,
                memo: None,
            },
        )
        .id,
    );
    let sol_params = |blockhash| ExecutionParams::Solana {
        blockhash: Hash::new([blockhash; 32]),
        compute_unit_price: 0,
        compute_unit_limit: 10_000,
        decimals: None,
        create_ata: false,
    };
    let plan = t.prepare(p.id, sol_params(7)).unwrap();
    // A version built before the first one is signed is checked again when
    // signed: its blockhash cannot land beside the first's.
    let early = t.prepare(p.id, sol_params(8)).unwrap();
    let tx = t
        .execute(p.id, Some(plan.seq), vec![])
        .unwrap()
        .signed
        .unwrap();
    assert_eq!(
        t.execute(p.id, Some(early.seq), vec![]).err(),
        Some(Error::Forbidden)
    );
    assert_eq!(&tx.raw[65..], &plan.unsigned[..]);
    let key = ed25519_dalek::VerifyingKey::from_bytes(&sol.public_key.to_vec().try_into().unwrap())
        .unwrap();
    key.verify_strict(
        &tx.raw[65..],
        &ed25519_dalek::Signature::from_slice(&tx.raw[1..65]).unwrap(),
    )
    .unwrap();
    assert_eq!(t.view().addresses.len(), 4);
    let stats: WalletStats = query(&t.f.ic, t.wallet, person(A), "wallet_stats", ());
    assert_eq!(stats.signatures_today, 6);
}

#[test]
fn wallet_cosigners_freeze_veto_and_recovery() {
    use p256::ecdsa::signature::Signer as _;
    let t = Team::new(1);
    let evm = t
        .view()
        .addresses
        .iter()
        .find(|a| a.chain == Chain::Evm)
        .unwrap()
        .clone();
    let cosigner = p256::ecdsa::SigningKey::from_slice(&[21; 32]).unwrap();
    let added = t.approve_p(
        t.propose(
            P,
            Intent::AddCosigner(CosignerInput {
                member: MemberId::Principal(person(P)),
                kind: CosignerKind::P256,
                public_key: cosigner
                    .verifying_key()
                    .to_sec1_point(true)
                    .as_bytes()
                    .to_vec()
                    .into(),
                envelope: CosignerEnvelope::Raw,
                hardware: true,
                label: "enclave".into(),
            }),
        )
        .id,
    );
    assert_eq!(
        added.status,
        ProposalStatus::Settled,
        "internal changes apply when ready"
    );
    assert_eq!(added.outcome, Some(Outcome::CosignerAdded(1)));
    let set = t.approve_p(
        t.propose(
            P,
            Intent::SetCosign {
                address: evm.id,
                cosign: Some(CosignPolicy {
                    cosigners: vec![1],
                    threshold: 1,
                }),
            },
        )
        .id,
    );
    assert_eq!(set.status, ProposalStatus::Delayed);
    t.f.ic
        .advance_time(Duration::from_millis(SENSITIVE_DELAY_MS + MINUTE));
    t.f.ic.tick();
    assert_eq!(
        t.execute(set.id, None, vec![]).unwrap().proposal.status,
        ProposalStatus::Settled
    );

    let p = t.approve_p(
        t.propose(
            P,
            Intent::Transfer {
                from: evm.id,
                asset: AssetKey::Evm {
                    chain_id: 1,
                    token: None,
                },
                to: Destination::Evm(Hash20::new([0x22; 20])),
                amount: 5,
                max_fee: 10u128.pow(16),
                memo: None,
            },
        )
        .id,
    );
    let plan = t
        .prepare(
            p.id,
            ExecutionParams::Evm {
                nonce: 0,
                max_fee_per_gas: 30_000_000_000,
                max_priority_fee_per_gas: 1_000_000_000,
                gas_limit: 21_000,
            },
        )
        .unwrap();
    assert_eq!(
        t.execute(p.id, Some(plan.seq), vec![]).err(),
        Some(Error::Forbidden)
    );
    let message = cosign_message(t.wallet, p.id, plan.seq, 0, &plan.digests[0]);
    let sig: p256::ecdsa::Signature = cosigner.sign(&message[..]);
    let wrong: p256::ecdsa::Signature =
        cosigner.sign(&cosign_message(t.wallet, p.id, plan.seq + 1, 0, &plan.digests[0])[..]);
    let cosig = |s: p256::ecdsa::Signature| CosignerSig {
        cosigner: 1,
        digest_index: 0,
        signature: s.to_bytes().to_vec().into(),
        authenticator_data: None,
        client_data_json: None,
    };
    assert_eq!(
        t.execute(p.id, Some(plan.seq), vec![cosig(wrong)]).err(),
        Some(Error::IntegrityFailed)
    );
    let signed = t.execute(p.id, Some(plan.seq), vec![cosig(sig)]).unwrap();
    assert_eq!(signed.proposal.status, ProposalStatus::Signed);

    // Any approver freezes at once; unfreezing waits for its delay.
    let frozen: Result<()> = update(&t.f.ic, t.wallet, person(B), "freeze", ());
    frozen.unwrap();
    let blocked: Result<Proposal> = update(
        &t.f.ic,
        t.wallet,
        person(P),
        "propose",
        (ProposeArgs {
            intent: Intent::Transfer {
                from: 0,
                asset: AssetKey::Icp,
                to: Destination::Icrc(account(person(80))),
                amount: 1,
                max_fee: 10_000,
                memo: None,
            },
            op_id: Hash::new([91; 32]),
            via_agent: None,
        },),
    );
    assert_eq!(blocked, Err(Error::Locked));
    let unfreeze = t.approve_p(t.propose(P, Intent::Unfreeze).id);
    assert_eq!(unfreeze.status, ProposalStatus::Delayed);
    t.f.ic.advance_time(Duration::from_millis(DAY + MINUTE));
    t.f.ic.tick();
    t.execute(unfreeze.id, None, vec![]).unwrap();
    assert!(!t.view().frozen);

    // Recovery proposes membership changes without approvals, after seven
    // days; an approver vetoes it during the delay.
    let joined: Result<Member> = update(
        &t.f.ic,
        t.wallet,
        person(RECOVERY),
        "join",
        (MemberApproval::Principal,),
    );
    joined.unwrap();
    let recovery = t.propose(
        RECOVERY,
        Intent::AddMember(MemberInput {
            id: MemberId::Principal(person(45)),
            role: Role::Admin,
            label: "new admin".into(),
            agent_ids: vec![],
        }),
    );
    assert_eq!(recovery.status, ProposalStatus::Delayed);
    assert_eq!(recovery.requirement.delay_ms, RECOVERY_DELAY_MS);
    let vetoed = t.decide_account(B, t.b, &recovery, Decision::Veto).unwrap();
    assert_eq!(vetoed.status, ProposalStatus::Vetoed);
    let unvetoed = t.propose(
        RECOVERY,
        Intent::AddMember(MemberInput {
            id: MemberId::Principal(person(45)),
            role: Role::Admin,
            label: "replacement admin".into(),
            agent_ids: vec![],
        }),
    );
    t.f.ic
        .advance_time(Duration::from_millis(RECOVERY_DELAY_MS + MINUTE));
    t.f.ic.tick();
    let done: Result<ExecuteResult> = update(
        &t.f.ic,
        t.wallet,
        person(RECOVERY),
        "execute",
        (ExecuteArgs {
            proposal: unvetoed.id,
            plan: None,
            cosigs: vec![],
        },),
    );
    assert_eq!(done.unwrap().proposal.status, ProposalStatus::Settled);
    assert!(t
        .view()
        .members
        .iter()
        .any(|m| m.id == MemberId::Principal(person(45))));
    let synced: Result<WalletRecord> =
        update(&t.f.ic, t.hub, person(1), "sync_wallet", (t.wallet,));
    assert!(synced
        .unwrap()
        .members
        .contains(&MemberId::Principal(person(45))));
    let listed: Vec<WalletRecord> = query(
        &t.f.ic,
        t.hub,
        person(1),
        "list_wallets_of",
        (MemberId::Principal(person(45)),),
    );
    assert_eq!(listed.len(), 1);

    // A native Taproot address commits to its cosigner's key, so removing
    // the cosigner's member keeps that cosigner and the address spends.
    let btc = t
        .view()
        .addresses
        .iter()
        .find(|a| a.chain == Chain::Bitcoin)
        .unwrap()
        .clone();
    let added = t.approve_p(
        t.propose(
            P,
            Intent::AddCosigner(CosignerInput {
                member: MemberId::Account(t.b),
                kind: CosignerKind::Bip340,
                public_key: x_only(&btc.public_key).unwrap().to_vec().into(),
                envelope: CosignerEnvelope::Raw,
                hardware: true,
                label: "hardware wallet".into(),
            }),
        )
        .id,
    );
    let Some(Outcome::CosignerAdded(cosigner)) = added.outcome else {
        panic!("cosigner {added:?}")
    };
    let native = t.approve_p(
        t.propose(
            P,
            Intent::AddAddress {
                chain: Chain::Bitcoin,
                kind: AddressKind::Taproot(TaprootControl {
                    cosigner,
                    chain_key_after: 144,
                    recovery: None,
                }),
                label: "cold".into(),
                cosign: None,
            },
        )
        .id,
    );
    let created = t.execute(native.id, None, vec![]).unwrap().proposal;
    let Some(Outcome::AddressCreated(cold)) = created.outcome else {
        panic!("address {created:?}")
    };
    let removed = t.approve_p(
        t.propose(P, Intent::RemoveMember(MemberId::Account(t.b)))
            .id,
    );
    t.f.ic
        .advance_time(Duration::from_millis(SENSITIVE_DELAY_MS + MINUTE));
    t.f.ic.tick();
    assert_eq!(
        t.execute(removed.id, None, vec![]).unwrap().proposal.status,
        ProposalStatus::Settled
    );
    let v = t.view();
    assert!(v.members.iter().all(|m| m.id != MemberId::Account(t.b)));
    assert!(v.cosigners.iter().any(|c| c.id == cosigner));
    let spend = t.approve_p(
        t.propose(
            P,
            Intent::Transfer {
                from: cold,
                asset: AssetKey::Btc,
                to: Destination::Bitcoin(btc.text.clone()),
                amount: 50_000,
                max_fee: 20_000,
                memo: None,
            },
        )
        .id,
    );
    // The chain key spends alone through the timelocked leaf.
    let plan = t
        .prepare(
            spend.id,
            ExecutionParams::Bitcoin {
                utxos: vec![Utxo {
                    txid: Hash::new([6; 32]),
                    vout: 0,
                    value: 100_000,
                    address: cold,
                }],
                fee_rate: 2,
                locktime: 0,
                timelock: true,
            },
        )
        .unwrap();
    assert_eq!(plan.summary.outputs[0].amount, 50_000);
}

#[test]
fn wallet_upgrade_lock_refuel_refund_and_revenue() {
    let t = Team::new(1);
    let bytes = wasm("dmsg_wallet");
    let v2 = register_module(&t.f, t.hub, &gz(&variant(&bytes, b"v2")));
    assert_eq!(v2.version, 2);
    let accept = t.approve_p(
        t.propose(
            P,
            Intent::AcceptUpgrade {
                version: 2,
                module_hash: v2.module_hash,
            },
        )
        .id,
    );
    assert_eq!(accept.status, ProposalStatus::Delayed);
    // Not accepted yet: the hub refuses.
    let early: Result<WalletRecord> =
        update(&t.f.ic, t.hub, person(99), "upgrade_wallet", (t.wallet,));
    assert_eq!(early, Err(Error::NotFound));
    t.f.ic
        .advance_time(Duration::from_millis(SENSITIVE_DELAY_MS + MINUTE));
    t.f.ic.tick();
    t.execute(accept.id, None, vec![]).unwrap();
    let upgraded: Result<WalletRecord> =
        update(&t.f.ic, t.hub, person(99), "upgrade_wallet", (t.wallet,));
    assert_eq!(upgraded.unwrap().version, 2);
    let v = t.view();
    assert_eq!(v.code, CodeAcceptance::None);
    assert_eq!(v.addresses.len(), 4, "state survives the upgrade");
    let again: Result<WalletRecord> =
        update(&t.f.ic, t.hub, person(99), "upgrade_wallet", (t.wallet,));
    assert!(again.is_err());

    // Cycles from the wallet's own ICP through the CMC.
    void(
        &t.f.ic,
        t.f.ledger,
        person(1),
        "mint_test",
        (account(t.wallet), 1_000_000_000u128),
    );
    let before = t.f.ic.cycle_balance(t.wallet);
    let refuel = t.approve_p(
        t.propose(
            P,
            Intent::Refuel {
                from: 0,
                icp_e8s: 100_000_000,
            },
        )
        .id,
    );
    let done = t.execute(refuel.id, None, vec![]).unwrap();
    assert!(
        matches!(done.proposal.outcome, Some(Outcome::Refueled { cycles, .. }) if cycles == 1_000_000_000_000)
    );
    assert!(t.f.ic.cycle_balance(t.wallet) as i128 - before as i128 > 900_000_000_000);

    // Locking removes every controller; the hub can no longer upgrade it.
    let lock = t.approve_p(t.propose(P, Intent::LockCode).id);
    t.f.ic
        .advance_time(Duration::from_millis(SENSITIVE_DELAY_MS + MINUTE));
    t.f.ic.tick();
    t.execute(lock.id, None, vec![]).unwrap();
    let locked: Result<WalletRecord> =
        update(&t.f.ic, t.hub, person(99), "lock_wallet", (t.wallet,));
    assert!(locked.unwrap().locked);
    assert!(t.f.ic.get_controllers(t.wallet).is_empty());
    let refused: Result<WalletRecord> =
        update(&t.f.ic, t.hub, person(99), "upgrade_wallet", (t.wallet,));
    assert_eq!(refused, Err(Error::Locked));

    // Revenue: the cycles share to the CMC for the hub, the rest to the DAO.
    let hub_cycles = t.f.ic.cycle_balance(t.hub);
    let collected: Result<ActivationOrder> =
        update(&t.f.ic, t.hub, person(99), "collect_revenue", (t.quote,));
    assert!(collected.unwrap().collected);
    assert_eq!(
        t.balance(account(person(70))),
        u128::from(PRICE - SHARE) - 20
    );
    assert!(t.f.ic.cycle_balance(t.hub) > hub_cycles);

    // A quote paid but never used is refunded after it expires.
    let quote: Result<ActivationQuote> =
        update(&t.f.ic, t.hub, person(B), "quote_activation", (t.b,));
    let quote = quote.unwrap();
    void(
        &t.f.ic,
        t.f.ledger,
        person(1),
        "mint_test",
        (quote.deposit, u128::from(PRICE)),
    );
    let early: Result<u64> = update(
        &t.f.ic,
        t.hub,
        person(B),
        "claim_activation_refund",
        (quote.quote_id,),
    );
    assert_eq!(early, Err(Error::Forbidden));
    t.f.ic
        .advance_time(Duration::from_millis(QUOTE_TTL_MS + MINUTE));
    let refunded: Result<u64> = update(
        &t.f.ic,
        t.hub,
        person(B),
        "claim_activation_refund",
        (quote.quote_id,),
    );
    refunded.unwrap();
    assert_eq!(t.balance(account(person(B))), u128::from(PRICE) - 10);
    assert_eq!(t.balance(quote.deposit), 0);
}

#[test]
fn hub_quota_and_creation_checks() {
    let f = Fixture::new();
    let hub = setup(&f);
    let a = create_member(&f, A);
    // Another account's quote cannot be used, and an unpaid quote creates nothing.
    let quote: Result<ActivationQuote> = update(&f.ic, hub, person(A), "quote_activation", (a,));
    let quote = quote.unwrap();
    let mut s = spec(1);
    s.members.remove(0);
    let app = application(
        &f,
        hub,
        hub,
        a,
        person(A),
        ApprovalPurpose::WalletCreate,
        activation_digest(&quote, &s),
    );
    let approval_id = approve_app(&f, A, &a, &app);
    let req = CreateWallet {
        quote_id: quote.quote_id,
        spec: s.clone(),
        approval_id,
        approval: app.clone(),
    };
    let unpaid: Result<ActivationOrder> =
        update(&f.ic, hub, person(A), "create_wallet", (req.clone(),));
    assert_eq!(unpaid, Err(Error::FeeBlocked));
    let stolen: Result<ActivationOrder> =
        update(&f.ic, hub, person(B), "create_wallet", (req.clone(),));
    assert_eq!(stolen, Err(Error::Forbidden));
    // A spec other than the approved one is refused.
    void(
        &f.ic,
        f.ledger,
        person(1),
        "mint_test",
        (quote.deposit, u128::from(PRICE)),
    );
    let mut changed = req.clone();
    changed.spec.name = "other".into();
    let refused: Result<ActivationOrder> =
        update(&f.ic, hub, person(A), "create_wallet", (changed,));
    assert_eq!(refused, Err(Error::Forbidden));
    let order: Result<ActivationOrder> = update(&f.ic, hub, person(A), "create_wallet", (req,));
    assert_eq!(order.unwrap().status, ActivationStatus::Ready);
    // The second wallet of the Free quota, then none.
    let second: Result<ActivationQuote> = update(&f.ic, hub, person(A), "quote_activation", (a,));
    assert_eq!(second.unwrap().remaining, 1);
    let stats: HubStats = query(&f.ic, hub, person(1), "hub_stats", ());
    assert_eq!((stats.wallets, stats.current_version), (1, Some(1)));
    assert_refused(
        &f.ic,
        hub,
        person(1),
        "admin_set_uploader",
        (Some(person(1)),),
    );
}
