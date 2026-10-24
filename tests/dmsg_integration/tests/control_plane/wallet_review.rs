//! Security and accounting regressions from the 2026-10-24 wallet review.
use super::*;

fn advance(t: &Team, millis: u64) {
    t.f.ic.advance_time(Duration::from_millis(millis));
    t.f.ic.tick();
}

fn transfer(asset: AssetKey, from: u32, amount: u128) -> Intent {
    let to = match asset {
        AssetKey::Icp => Destination::Icrc(account(person(80))),
        AssetKey::Evm { .. } => Destination::Evm(Hash20::new([0x33; 20])),
        AssetKey::Sol { .. } => Destination::Solana(Hash::new([0x55; 32])),
        _ => unreachable!(),
    };
    Intent::Transfer {
        from,
        asset,
        to,
        amount,
        max_fee: 10u128.pow(16),
        memo: None,
    }
}

fn evm(nonce: u64) -> ExecutionParams {
    ExecutionParams::Evm {
        nonce,
        max_fee_per_gas: 30,
        max_priority_fee_per_gas: 1,
        gas_limit: 21_000,
    }
}

fn sol(hash: u8) -> ExecutionParams {
    ExecutionParams::Solana {
        blockhash: Hash::new([hash; 32]),
        compute_unit_price: 0,
        compute_unit_limit: 100_000,
        decimals: None,
        create_ata: false,
    }
}

fn report(t: &Team, p: u64, plan: u32, outcome: ChainOutcome) -> Proposal {
    let r: Result<Proposal> = update(
        &t.f.ic,
        t.wallet,
        person(P),
        "report",
        (ChainReport {
            proposal: p,
            plan,
            outcome,
            note: String::new(),
        },),
    );
    r.unwrap()
}

#[test]
fn signed_solana_payment_cannot_change_blockhash_or_nonce_mode() {
    let t = Team::new(1);
    let p = t.approve_p(
        t.propose(P, transfer(AssetKey::Sol { mint: None }, 3, 1_000))
            .id,
    );
    let plan = t.prepare(p.id, sol(7)).unwrap();
    let signed = t
        .execute(p.id, Some(plan.seq), vec![])
        .unwrap()
        .signed
        .unwrap();
    advance(&t, 151 * SECOND);
    assert_eq!(t.prepare(p.id, sol(8)), Err(Error::Forbidden));
    assert_eq!(
        t.execute(p.id, Some(plan.seq), vec![]).unwrap().signed,
        Some(signed)
    );
    let init = t.approve_p(
        t.propose(
            P,
            Intent::InitSolNonce {
                address: 3,
                max_fee: 100_000,
            },
        )
        .id,
    );
    let init_plan = t.prepare(init.id, sol(9)).unwrap();
    t.execute(init.id, Some(init_plan.seq), vec![]).unwrap();
    report(&t, init.id, init_plan.seq, ChainOutcome::Settled);
    assert_eq!(t.prepare(p.id, sol(7)), Err(Error::Forbidden));
    // A new payment with a durable nonce may replace only that same nonce.
    let next = t.approve_p(
        t.propose(P, transfer(AssetKey::Sol { mint: None }, 3, 2_000))
            .id,
    );
    let first = t.prepare(next.id, sol(10)).unwrap();
    t.execute(next.id, Some(first.seq), vec![]).unwrap();
    assert_eq!(t.prepare(next.id, sol(11)), Err(Error::Forbidden));
    assert!(t.prepare(next.id, sol(10)).is_ok());
}

#[test]
fn membership_changes_invalidate_votes_and_recovery_authority() {
    let t = Team::new(2);
    let p = t.propose(P, transfer(AssetKey::Icp, 0, 1_000));
    t.approve_p(p.id);
    t.decide_account(B, t.b, &p, Decision::Approve).unwrap();
    let changed = t.approve_p(
        t.propose(
            P,
            Intent::UpdateMember(MemberInput {
                id: MemberId::Account(t.b),
                role: Role::Viewer,
                label: "b".into(),
                agent_ids: vec![],
            }),
        )
        .id,
    );
    assert_eq!(changed.status, ProposalStatus::Settled);
    assert_eq!(t.execute(p.id, None, vec![]), Err(Error::Expired));
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
            id: MemberId::Principal(person(91)),
            role: Role::Admin,
            label: "replacement".into(),
            agent_ids: vec![],
        }),
    );
    t.approve_p(
        t.propose(
            P,
            Intent::UpdateMember(MemberInput {
                id: MemberId::Principal(person(RECOVERY)),
                role: Role::Proposer,
                label: "former recovery".into(),
                agent_ids: vec![],
            }),
        )
        .id,
    );
    advance(&t, RECOVERY_DELAY_MS + MINUTE);
    let executed: Result<ExecuteResult> = update(
        &t.f.ic,
        t.wallet,
        person(RECOVERY),
        "execute",
        (ExecuteArgs {
            proposal: recovery.id,
            plan: None,
            cosigs: vec![],
        },),
    );
    assert_eq!(executed, Err(Error::Expired));
    assert!(
        t.view()
            .members
            .iter()
            .all(|m| m.id != MemberId::Principal(person(91)))
    );
}

#[test]
fn reservations_survive_a_day_and_execution_starts_the_spend_window() {
    let t = Team::new(1);
    void(
        &t.f.ic,
        t.f.ledger,
        person(1),
        "mint_test",
        (account(t.wallet), 2_000_000_000u128),
    );
    let first = t.approve_p(t.propose(P, transfer(AssetKey::Icp, 0, 600_000_000)).id);
    let read_usage = || {
        let rows: Result<Vec<SpendUsage>> = query(&t.f.ic, t.wallet, person(P), "policy_usage", ());
        rows.unwrap()
    };
    assert_eq!(
        read_usage(),
        vec![SpendUsage {
            asset: AssetKey::Icp,
            reserved: 600_000_000,
            executed: 0
        }]
    );
    let denied: Result<Vec<SpendUsage>> = query(&t.f.ic, t.wallet, person(98), "policy_usage", ());
    assert_eq!(denied, Err(Error::Forbidden));
    advance(&t, DAY + MINUTE);
    assert_eq!(read_usage()[0].reserved, 600_000_000);
    let second = t.propose(P, transfer(AssetKey::Icp, 0, 600_000_001));
    assert_eq!(second.requirement.approvals, 2);
    assert_eq!(t.approve_p(second.id).status, ProposalStatus::Open);
    t.execute(first.id, None, vec![]).unwrap();
    assert_eq!(
        read_usage(),
        vec![SpendUsage {
            asset: AssetKey::Icp,
            reserved: 0,
            executed: 600_000_000
        }]
    );
    let third = t.propose(P, transfer(AssetKey::Icp, 0, 600_000_002));
    assert_eq!(third.requirement.approvals, 2);
    advance(&t, DAY + MINUTE);
    assert!(read_usage().is_empty());
    let fourth = t.propose(P, transfer(AssetKey::Icp, 0, 600_000_003));
    assert_eq!(fourth.requirement.approvals, 1);
}

#[test]
fn cancellation_reports_never_release_spend_or_affect_a_new_nonce() {
    let t = Team::new(1);
    let asset = AssetKey::Evm {
        chain_id: 1,
        token: None,
    };
    let mut policy = t.view().policy;
    policy.version += 1;
    policy.tiers.push(Tier {
        asset: asset.clone(),
        up_to: 1_000,
        approvals: 1,
        delay_ms: 0,
        book_only: false,
    });
    let policy = t.approve_p(t.propose(P, Intent::SetPolicy(policy)).id);
    advance(&t, SENSITIVE_DELAY_MS + MINUTE);
    t.execute(policy.id, None, vec![]).unwrap();
    let p = t.approve_p(t.propose(P, transfer(asset.clone(), 2, 600)).id);
    let plan = t.prepare(p.id, evm(0)).unwrap();
    t.execute(p.id, Some(plan.seq), vec![]).unwrap();
    let cancel = t.approve_p(
        t.propose(
            P,
            Intent::CancelEvmNonce {
                address: 2,
                chain_id: 1,
                proposal: p.id,
                nonce: 0,
                max_fee: 1_000_000,
            },
        )
        .id,
    );
    let cancelled = t.prepare(cancel.id, evm(0)).unwrap();
    t.execute(cancel.id, Some(cancelled.seq), vec![]).unwrap();
    let late = t.approve_p(
        t.propose(
            P,
            Intent::CancelEvmNonce {
                address: 2,
                chain_id: 1,
                proposal: p.id,
                nonce: 0,
                max_fee: 1_000_001,
            },
        )
        .id,
    );
    let late_plan = t.prepare(late.id, evm(0)).unwrap();
    t.execute(late.id, Some(late_plan.seq), vec![]).unwrap();
    report(&t, cancel.id, cancelled.seq, ChainOutcome::Settled);
    let next = t.propose(P, transfer(asset.clone(), 2, 601));
    assert_eq!(
        next.requirement.approvals, 2,
        "a fabricated cancellation cannot release 600"
    );
    t.approve_p(next.id);
    t.decide_account(B, t.b, &next, Decision::Approve).unwrap();
    let next_plan = t.prepare(next.id, evm(1)).unwrap();
    t.execute(next.id, Some(next_plan.seq), vec![]).unwrap();
    report(&t, late.id, late_plan.seq, ChainOutcome::Settled);
    let probe = t.propose(P, transfer(asset.clone(), 2, 1));
    t.approve_p(probe.id);
    t.decide_account(B, t.b, &probe, Decision::Approve).unwrap();
    assert_eq!(
        t.prepare(probe.id, evm(2)),
        Err(Error::Forbidden),
        "late cancellation must leave the new pending nonce locked"
    );
    // A cancellation proposal names the original payment, not any future pending one.
    let wrong: Result<Proposal> = update(
        &t.f.ic,
        t.wallet,
        person(P),
        "propose",
        (ProposeArgs {
            intent: Intent::CancelEvmNonce {
                address: 2,
                chain_id: 1,
                proposal: p.id,
                nonce: 0,
                max_fee: 1_000_000,
            },
            op_id: Hash::new([92; 32]),
            via_agent: None,
        },),
    );
    assert_eq!(wrong, Err(Error::NotFound));
}

#[test]
fn code_claim_serializes_acceptance_and_lock_keeps_canister_running() {
    let t = Team::new(1);
    let version = register_module(&t.f, t.hub, &gz(&variant(&wasm("dmsg_wallet"), b"claim")));
    let upgrade = t.approve_p(
        t.propose(
            P,
            Intent::AcceptUpgrade {
                version: version.version,
                module_hash: version.module_hash,
            },
        )
        .id,
    );
    let lock = t.approve_p(t.propose(P, Intent::LockCode).id);
    advance(&t, SENSITIVE_DELAY_MS + MINUTE);
    t.execute(upgrade.id, None, vec![]).unwrap();
    let claim: Result<()> = update(
        &t.f.ic,
        t.wallet,
        t.hub,
        "claim_code",
        (CodeAcceptance::Upgrade {
            version: version.version,
            module_hash: version.module_hash,
        },),
    );
    claim.unwrap();
    assert_eq!(
        t.execute(lock.id, None, vec![]).unwrap().proposal.status,
        ProposalStatus::Failed
    );
    let released: Result<()> = update(&t.f.ic, t.wallet, t.hub, "release_code", ());
    released.unwrap();
    let lock = t.approve_p(t.propose(P, Intent::LockCode).id);
    advance(&t, SENSITIVE_DELAY_MS + MINUTE);
    t.execute(lock.id, None, vec![]).unwrap();
    let locked: Result<WalletRecord> =
        update(&t.f.ic, t.hub, person(99), "lock_wallet", (t.wallet,));
    assert!(locked.unwrap().locked);
    assert!(t.f.ic.get_controllers(t.wallet).is_empty());
    assert_eq!(
        t.view().code,
        CodeAcceptance::Lock,
        "a controllerless wallet still answers"
    );
}

#[test]
fn unknown_icrc_transfer_survives_deduplication_expiry_and_verifies_a_block() {
    let t = Team::new(1);
    void(
        &t.f.ic,
        t.f.ledger,
        person(1),
        "mint_test",
        (account(t.wallet), 100_000u128),
    );
    let p = t.approve_p(t.propose(P, transfer(AssetKey::Icp, 0, 1_000)).id);
    void(&t.f.ic, t.f.ledger, person(1), "lose_next_response", ());
    assert_eq!(t.execute(p.id, None, vec![]), Err(Error::ExecutionUnknown));
    assert_eq!(t.balance(account(person(80))), 1_000);
    advance(&t, DAY + 2 * MINUTE);
    let pending: Result<ExecuteResult> = update(&t.f.ic, t.wallet, person(P), "reconcile", (p.id,));
    assert_eq!(pending, Err(Error::ExecutionUnknown));
    let state: Result<Proposal> = query(&t.f.ic, t.wallet, person(P), "get_proposal", (p.id,));
    assert_eq!(state.unwrap().status, ProposalStatus::Executing);
    // Locate the real block through the fixture's public ICRC-3 interface.
    let blocks: icrc_ledger_types::icrc3::blocks::GetBlocksResult = query(
        &t.f.ic,
        t.f.ledger,
        person(P),
        "icrc3_get_blocks",
        (vec![icrc_ledger_types::icrc3::blocks::GetBlocksRequest {
            start: 0u8.into(),
            length: 100u8.into(),
        }],),
    );
    let block =
        dmsg_runtime::ledger::block_index(blocks.blocks.last().unwrap().id.clone()).unwrap();
    void(
        &t.f.ic,
        t.f.ledger,
        person(1),
        "mint_test",
        (account(person(90)), 100u128),
    );
    let unrelated: std::result::Result<Nat, TransferError> = update(
        &t.f.ic,
        t.f.ledger,
        person(90),
        "icrc1_transfer",
        (TransferArg {
            from_subaccount: None,
            to: account(person(91)),
            amount: 1u128.into(),
            fee: None,
            memo: None,
            created_at_time: None,
        },),
    );
    let unrelated = dmsg_runtime::ledger::block_index(unrelated.unwrap()).unwrap();
    let wrong: Result<ExecuteResult> = update(
        &t.f.ic,
        t.wallet,
        person(P),
        "reconcile_transfer",
        (p.id, 0u16, unrelated),
    );
    assert_eq!(wrong, Err(Error::IntegrityFailed));
    let done: Result<ExecuteResult> = update(
        &t.f.ic,
        t.wallet,
        person(P),
        "reconcile_transfer",
        (p.id, 0u16, block),
    );
    assert_eq!(done.unwrap().proposal.status, ProposalStatus::Settled);
    assert_eq!(t.balance(account(person(80))), 1_000);
}

#[test]
fn expired_unsent_batch_items_fail_after_recovering_the_paid_item() {
    let t = Team::new(1);
    void(
        &t.f.ic,
        t.f.ledger,
        person(1),
        "mint_test",
        (account(t.wallet), 100_000u128),
    );
    let p = t.approve_p(
        t.propose(
            P,
            Intent::Batch {
                from: 0,
                asset: AssetKey::Icp,
                items: vec![
                    BatchItem {
                        to: Destination::Icrc(account(person(80))),
                        amount: 1_000,
                    },
                    BatchItem {
                        to: Destination::Icrc(account(person(81))),
                        amount: 2_000,
                    },
                ],
                max_fee: 100,
            },
        )
        .id,
    );
    void(&t.f.ic, t.f.ledger, person(1), "lose_next_response", ());
    assert_eq!(t.execute(p.id, None, vec![]), Err(Error::ExecutionUnknown));
    assert_eq!(t.balance(account(person(80))), 1_000);
    assert_eq!(t.balance(account(person(81))), 0);
    let blocks: icrc_ledger_types::icrc3::blocks::GetBlocksResult = query(
        &t.f.ic,
        t.f.ledger,
        person(P),
        "icrc3_get_blocks",
        (vec![icrc_ledger_types::icrc3::blocks::GetBlocksRequest {
            start: 0u8.into(),
            length: 100u8.into(),
        }],),
    );
    let block =
        dmsg_runtime::ledger::block_index(blocks.blocks.last().unwrap().id.clone()).unwrap();
    advance(&t, DAY + 2 * MINUTE);
    let recovered: Result<ExecuteResult> = update(
        &t.f.ic,
        t.wallet,
        person(P),
        "reconcile_transfer",
        (p.id, 0u16, block),
    );
    assert!(
        matches!(recovered, Err(Error::InvalidInput(_))),
        "{recovered:?}"
    );
    let state: Result<Proposal> = query(&t.f.ic, t.wallet, person(P), "get_proposal", (p.id,));
    let state = state.unwrap();
    assert_eq!(state.status, ProposalStatus::Failed);
    assert_eq!(
        state.outcome,
        Some(Outcome::Ledger {
            blocks: vec![block]
        })
    );
    assert_eq!(t.balance(account(person(80))), 1_000);
    assert_eq!(t.balance(account(person(81))), 0);
}

#[test]
fn activation_surplus_and_late_deposits_are_refundable_without_touching_revenue() {
    let t = Team::new(1);
    let q: Option<ActivationQuote> = query(&t.f.ic, t.hub, person(A), "get_quote", (t.quote,));
    let q = q.unwrap();
    void(
        &t.f.ic,
        t.f.ledger,
        person(1),
        "mint_test",
        (q.deposit, 100_000u128),
    );
    let refunded = claim_refund(&t.f, t.hub, A, t.quote, account(person(PAYER))).unwrap();
    assert_eq!(t.balance(q.deposit), u128::from(PRICE));
    assert_eq!(t.balance(account(person(PAYER))), 99_990);
    assert_eq!(
        claim_refund(&t.f, t.hub, A, t.quote, account(person(PAYER))),
        Ok(refunded)
    );
    let collected: Result<ActivationOrder> =
        update(&t.f.ic, t.hub, person(99), "collect_revenue", (t.quote,));
    assert!(collected.unwrap().collected);
    assert_eq!(
        t.balance(account(person(70))),
        u128::from(PRICE - SHARE) - 20
    );
    void(
        &t.f.ic,
        t.f.ledger,
        person(1),
        "mint_test",
        (q.deposit, 100_000u128),
    );
    let later = claim_refund(&t.f, t.hub, A, t.quote, account(person(PAYER))).unwrap();
    assert_ne!(refunded, later);
    assert_eq!(t.balance(account(person(PAYER))), 199_980);
    assert_eq!(t.balance(q.deposit), 0);
    assert_eq!(
        t.balance(account(person(70))),
        u128::from(PRICE - SHARE) - 20
    );
}

#[test]
fn unexpired_quotes_pin_retired_modules() {
    let t = Team::new(1);
    let q: Result<ActivationQuote> = update(&t.f.ic, t.hub, person(A), "quote_activation", (t.a,));
    q.unwrap();
    register_module(
        &t.f,
        t.hub,
        &gz(&variant(&wasm("dmsg_wallet"), b"retention")),
    );
    let r: Result<()> = update(&t.f.ic, t.hub, t.f.sns, "admin_retire_version", (1u32,));
    r.unwrap();
    let r: Result<()> = update(&t.f.ic, t.hub, t.f.sns, "admin_remove_version", (1u32,));
    assert!(matches!(r, Err(Error::InvalidInput(_))));
    advance(&t, QUOTE_TTL_MS + MINUTE);
    let r: Result<()> = update(&t.f.ic, t.hub, t.f.sns, "admin_remove_version", (1u32,));
    r.unwrap();
}

#[test]
fn an_uninstalled_activation_can_abort_and_its_version_stays_pinned_until_then() {
    let t = Team::new(1);
    let version = register_module(&t.f, t.hub, b"deliberately invalid test-only wasm");
    let q: Result<ActivationQuote> = update(&t.f.ic, t.hub, person(A), "quote_activation", (t.a,));
    let q = q.unwrap();
    pay(&t.f, &q);
    let mut requested = spec(1);
    requested.members[0].id = MemberId::Account(t.b);
    let app = application(
        &t.f,
        t.hub,
        t.hub,
        t.a,
        person(A),
        ApprovalPurpose::WalletCreate,
        activation_digest(&q, &requested),
    );
    let approval_id = approve_app(&t.f, A, &t.a, &app);
    let created: Result<ActivationOrder> = update(
        &t.f.ic,
        t.hub,
        person(A),
        "create_wallet",
        (CreateWallet {
            quote_id: q.quote_id,
            spec: requested,
            approval_id,
            approval: app,
        },),
    );
    assert!(created.is_err());
    let order: Option<ActivationOrder> =
        query(&t.f.ic, t.hub, person(A), "get_order", (q.quote_id,));
    assert_eq!(order.unwrap().status, ActivationStatus::Created);
    let retired: Result<()> = update(
        &t.f.ic,
        t.hub,
        t.f.sns,
        "admin_retire_version",
        (version.version,),
    );
    retired.unwrap();
    advance(&t, QUOTE_TTL_MS + MINUTE);
    let removed: Result<()> = update(
        &t.f.ic,
        t.hub,
        t.f.sns,
        "admin_remove_version",
        (version.version,),
    );
    assert!(matches!(removed, Err(Error::InvalidInput(_))));
    let wrong: Result<ActivationOrder> =
        update(&t.f.ic, t.hub, person(B), "abort_activation", (q.quote_id,));
    assert_eq!(wrong, Err(Error::Forbidden));
    let aborted: Result<ActivationOrder> =
        update(&t.f.ic, t.hub, person(A), "abort_activation", (q.quote_id,));
    assert!(matches!(
        aborted.unwrap().status,
        ActivationStatus::Failed(_)
    ));
    claim_refund(&t.f, t.hub, A, q.quote_id, account(person(PAYER))).unwrap();
    assert_eq!(t.balance(account(person(PAYER))), u128::from(PRICE) - 10);
    let removed: Result<()> = update(
        &t.f.ic,
        t.hub,
        t.f.sns,
        "admin_remove_version",
        (version.version,),
    );
    removed.unwrap();
    let active: Result<ActivationOrder> =
        update(&t.f.ic, t.hub, person(A), "abort_activation", (t.quote,));
    assert_eq!(active, Err(Error::VersionConflict));
}

#[test]
fn concurrent_address_creation_and_key_removal_never_leave_a_dangling_cosigner() {
    let t = Team::new(1);
    let key = p256::ecdsa::SigningKey::from_slice(&[31; 32]).unwrap();
    t.approve_p(
        t.propose(
            P,
            Intent::AddCosigner(CosignerInput {
                member: MemberId::Principal(person(P)),
                kind: CosignerKind::P256,
                public_key: key
                    .verifying_key()
                    .to_sec1_point(true)
                    .as_bytes()
                    .to_vec()
                    .into(),
                envelope: CosignerEnvelope::Raw,
                hardware: false,
                label: "race key".into(),
            }),
        )
        .id,
    );
    let removal = t.approve_p(t.propose(P, Intent::RemoveCosigner(1)).id);
    advance(&t, SENSITIVE_DELAY_MS + MINUTE);
    let address = t.approve_p(
        t.propose(
            P,
            Intent::AddAddress {
                chain: Chain::Evm,
                kind: AddressKind::ChainKey,
                label: "guarded".into(),
                cosign: Some(CosignPolicy {
                    threshold: 1,
                    cosigners: vec![1],
                }),
            },
        )
        .id,
    );
    let calls: Vec<_> = [address.id, removal.id]
        .into_iter()
        .map(|proposal| {
            t.f.ic
                .submit_call(
                    t.wallet,
                    person(A),
                    "execute",
                    candid::encode_args((ExecuteArgs {
                        proposal,
                        plan: None,
                        cosigs: vec![],
                    },))
                    .unwrap(),
                )
                .unwrap()
        })
        .collect();
    for call in calls {
        let _: Result<ExecuteResult> =
            candid::decode_one(&t.f.ic.await_call(call).unwrap()).unwrap();
    }
    let view = t.view();
    for address in &view.addresses {
        if let Some(policy) = &address.cosign {
            assert!(
                policy
                    .cosigners
                    .iter()
                    .all(|id| view.cosigners.iter().any(|c| c.id == *id))
            );
        }
    }
}

#[test]
fn proposal_certificate_authenticates_votes_requirements_and_exact_content() {
    let t = Team::new(1);
    let p = t.propose(P, transfer(AssetKey::Icp, 0, 1_000));
    let response: Result<CertifiedBatch> = query(
        &t.f.ic,
        t.wallet,
        person(A),
        "get_proposal_certified",
        (p.id,),
    );
    let batch = response.unwrap();
    let key = [b"proposal/".as_slice(), &p.id.to_be_bytes()].concat();
    let root = t.f.ic.root_key().unwrap();
    let verify = |b: &CertifiedBatch| {
        dmsg_client::certified::certified_leaf(b, &key, t.wallet, &root, time(&t.f.ic))
    };
    let bytes = verify(&batch).unwrap().unwrap();
    assert_eq!(
        dmsg_protocol::decode_canonical::<Proposal>(&bytes).unwrap(),
        p
    );
    let mut altered = p.clone();
    altered.requirement.approvals = 1;
    altered.votes.push(Vote {
        member: MemberId::Account(t.b),
        decision: Decision::Approve,
        at_ms: time(&t.f.ic),
    });
    let mut forged = batch.clone();
    forged.entries[0].value = Some(dmsg_protocol::canonical(&altered).into());
    assert_eq!(verify(&forged), Err(Error::IntegrityFailed));
    forged = batch.clone();
    forged.certificate = vec![].into();
    assert_eq!(verify(&forged), Err(Error::IntegrityFailed));
}

fn chain_rpc(url: &str, method: &str, params: serde_json::Value) -> serde_json::Value {
    let r: serde_json::Value = reqwest::blocking::Client::new()
        .post(url)
        .json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert!(r.get("error").is_none(), "{method}: {r}");
    r["result"].clone()
}

#[test]
#[ignore = "requires DMSG_SOLANA_RPC_URL pointing to an isolated local validator"]
fn solana_validator_accepts_only_one_payment_from_the_approved_proposal() {
    use base64::Engine;
    use serde_json::json;
    let url = std::env::var("DMSG_SOLANA_RPC_URL").unwrap();
    assert!(url.starts_with("http://127.0.0.1:"));
    let t = Team::new(1);
    let address = t
        .view()
        .addresses
        .into_iter()
        .find(|a| a.chain == Chain::Solana)
        .unwrap();
    chain_rpc(&url, "requestAirdrop", json!([address.text, 1_000_000_000]));
    for _ in 0..60 {
        if chain_rpc(
            &url,
            "getBalance",
            json!([address.text,{"commitment":"confirmed"}]),
        )["value"]
            .as_u64()
            .unwrap_or(0)
            > 0
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let to = sol_address(&[0x55; 32]);
    let before = chain_rpc(&url, "getBalance", json!([to,{"commitment":"confirmed"}]))["value"]
        .as_u64()
        .unwrap();
    let p = t.approve_p(
        t.propose(P, transfer(AssetKey::Sol { mint: None }, 3, 1_000_000))
            .id,
    );
    let blockhash = chain_rpc(
        &url,
        "getLatestBlockhash",
        json!([{"commitment":"confirmed"}]),
    )["value"]["blockhash"]
        .as_str()
        .unwrap()
        .to_owned();
    let params = ExecutionParams::Solana {
        blockhash: Hash::new(parse_sol_address(&blockhash).unwrap()),
        compute_unit_price: 0,
        compute_unit_limit: 100_000,
        decimals: None,
        create_ata: false,
    };
    let plan = t.prepare(p.id, params).unwrap();
    let tx = t
        .execute(p.id, Some(plan.seq), vec![])
        .unwrap()
        .signed
        .unwrap();
    let signature = chain_rpc(
        &url,
        "sendTransaction",
        json!([base64::engine::general_purpose::STANDARD.encode(&tx.raw),{"encoding":"base64","preflightCommitment":"confirmed"}]),
    );
    for _ in 0..60 {
        let status = chain_rpc(
            &url,
            "getSignatureStatuses",
            json!([[signature],{"searchTransactionHistory":true}]),
        );
        if status["value"][0]["confirmationStatus"] == "confirmed"
            || status["value"][0]["confirmationStatus"] == "finalized"
        {
            assert!(status["value"][0]["err"].is_null());
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    assert_eq!(
        chain_rpc(&url, "getBalance", json!([to,{"commitment":"confirmed"}]))["value"]
            .as_u64()
            .unwrap(),
        before + 1_000_000
    );
    advance(&t, 151 * SECOND);
    let hash = chain_rpc(
        &url,
        "getLatestBlockhash",
        json!([{"commitment":"confirmed"}]),
    )["value"]["blockhash"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut next = sol(9);
    if let ExecutionParams::Solana { blockhash, .. } = &mut next {
        *blockhash = Hash::new(parse_sol_address(&hash).unwrap());
    }
    assert_eq!(t.prepare(p.id, next), Err(Error::Forbidden));
    assert_eq!(
        chain_rpc(&url, "getBalance", json!([to,{"commitment":"confirmed"}]))["value"]
            .as_u64()
            .unwrap(),
        before + 1_000_000
    );
}

#[test]
#[ignore = "requires DMSG_EVM_RPC_URL pointing to an isolated local anvil"]
fn evm_estimated_gas_delivers_native_funds_to_a_payable_contract() {
    use serde_json::json;
    let url = std::env::var("DMSG_EVM_RPC_URL").unwrap();
    assert!(url.starts_with("http://127.0.0.1:"));
    let accounts = chain_rpc(&url, "eth_accounts", json!([]));
    // A tiny payable receiver that stores 42: its execution needs more than
    // the 21000 intrinsic gas of a plain account transfer.
    let deployed = chain_rpc(
        &url,
        "eth_sendTransaction",
        json!([{"from":accounts[0],"data":"0x6006600c60003960066000f3602a60005500","gas":"0x3d090"}]),
    );
    let receipt = evm_receipt(&url, deployed);
    let to = receipt["contractAddress"].as_str().unwrap();
    let t = Team::new(1);
    let address = t
        .view()
        .addresses
        .into_iter()
        .find(|a| a.chain == Chain::Evm)
        .unwrap();
    chain_rpc(
        &url,
        "anvil_setBalance",
        json!([address.text, "0x8ac7230489e80000"]),
    );
    let chain = chain_rpc(&url, "eth_chainId", json!([]));
    let chain_id =
        u64::from_str_radix(chain.as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
    let mut policy = t.view().policy;
    policy.version += 1;
    policy.evm_chains.push(chain_id);
    let asset = AssetKey::Evm {
        chain_id,
        token: None,
    };
    policy.spendable_assets.push(asset.clone());
    let change = t.approve_p(t.propose(P, Intent::SetPolicy(policy)).id);
    advance(&t, SENSITIVE_DELAY_MS + MINUTE);
    t.execute(change.id, None, vec![]).unwrap();
    let amount = 1_000_000_000_000_000u128;
    let p = t.approve_p(
        t.propose(
            P,
            Intent::Transfer {
                from: 2,
                asset,
                to: Destination::Evm(Hash20::new(parse_evm_address(to).unwrap())),
                amount,
                max_fee: 10u128.pow(17),
                memo: None,
            },
        )
        .id,
    );
    let gas = chain_rpc(
        &url,
        "eth_estimateGas",
        json!([{"from":address.text,"to":to,"value":format!("0x{amount:x}"),"data":"0x"}]),
    );
    let gas =
        u64::from_str_radix(gas.as_str().unwrap().trim_start_matches("0x"), 16).unwrap() * 6 / 5;
    assert!(gas > 21_000);
    let params = ExecutionParams::Evm {
        nonce: 0,
        gas_limit: gas,
        max_fee_per_gas: 10_000_000_000,
        max_priority_fee_per_gas: 1_000_000_000,
    };
    let plan = t.prepare(p.id, params).unwrap();
    let signed = t
        .execute(p.id, Some(plan.seq), vec![])
        .unwrap()
        .signed
        .unwrap();
    let tx = chain_rpc(
        &url,
        "eth_sendRawTransaction",
        json!([format!("0x{}", hex::encode(&signed.raw))]),
    );
    let receipt = evm_receipt(&url, tx);
    assert_eq!(receipt["status"], "0x1");
    let balance = chain_rpc(&url, "eth_getBalance", json!([to, "latest"]));
    assert_eq!(
        u128::from_str_radix(balance.as_str().unwrap().trim_start_matches("0x"), 16).unwrap(),
        amount
    );
    let value = chain_rpc(&url, "eth_getStorageAt", json!([to, "0x0", "latest"]));
    assert_eq!(
        u128::from_str_radix(value.as_str().unwrap().trim_start_matches("0x"), 16).unwrap(),
        42
    );
}

fn evm_receipt(url: &str, tx: serde_json::Value) -> serde_json::Value {
    for _ in 0..100 {
        let r = chain_rpc(url, "eth_getTransactionReceipt", serde_json::json!([tx]));
        if !r.is_null() {
            return r;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("local EVM transaction was not mined");
}

#[test]
fn interrupted_first_signing_restores_the_unexecuted_reservation() {
    let t = Team::new(1);
    let asset = AssetKey::Evm {
        chain_id: 1,
        token: None,
    };
    let mut policy = t.view().policy;
    policy.version += 1;
    policy.tiers.push(Tier {
        asset: asset.clone(),
        up_to: 1000,
        approvals: 1,
        delay_ms: 0,
        book_only: false,
    });
    let change = t.approve_p(t.propose(P, Intent::SetPolicy(policy)).id);
    advance(&t, SENSITIVE_DELAY_MS + MINUTE);
    t.execute(change.id, None, vec![]).unwrap();
    let p = t.approve_p(t.propose(P, transfer(asset.clone(), 2, 600)).id);
    let plan = t.prepare(p.id, evm(0)).unwrap();
    let signing =
        t.f.ic
            .submit_call(
                t.wallet,
                person(A),
                "execute",
                candid::encode_args((ExecuteArgs {
                    proposal: p.id,
                    plan: Some(plan.seq),
                    cosigs: vec![],
                },))
                .unwrap(),
            )
            .unwrap();
    let frozen: Result<()> = update(&t.f.ic, t.wallet, person(P), "freeze", ());
    frozen.unwrap();
    let result: Result<ExecuteResult> =
        candid::decode_one(&t.f.ic.await_call(signing).unwrap()).unwrap();
    assert_eq!(result, Err(Error::Locked));
    let unfreeze = t.approve_p(t.propose(P, Intent::Unfreeze).id);
    advance(&t, DAY + MINUTE);
    t.execute(unfreeze.id, None, vec![]).unwrap();
    let next = t.propose(P, transfer(asset, 2, 601));
    assert_eq!(next.requirement.approvals, 2);
}
