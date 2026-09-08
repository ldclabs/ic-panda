use super::*;

fn open_order(f: &Fixture) -> Escrow {
    let subject = f.create(2);
    let input = f.order(subject, 2, 1);
    let opened: Result<Escrow> = update(&f.ic, f.payment, person(40), "open_escrow", (input,));
    opened.unwrap()
}
fn settle_order(f: &Fixture, e: &Escrow) -> Escrow {
    let block = f.fund(e, e.quote.amount);
    let funded: Result<Escrow> = update(
        &f.ic,
        f.payment,
        person(40),
        "check_funding",
        (e.escrow_id, block),
    );
    let decided: Result<Escrow> = update(
        &f.ic,
        f.payment,
        person(40),
        "finalize_receipt",
        (f.receipt(&funded.unwrap()),),
    );
    decided.unwrap()
}
fn derivation_request(f: &Fixture, subject: Hash, transport: Vec<u8>) -> ExecuteRequest {
    let s = f.subject(1, subject);
    let kind = ExecutionKind::Derive {
        generation: 1,
        root_op_id: None,
        transport_key: transport.into(),
    };
    let max_cycles = 100_000_000_000u128;
    let sequence = s.devices[&[1; 32]].next_sequence;
    let mut approval = Approval {
        device_id: [1; 32],
        security_epoch: s.security_epoch,
        sequence,
        request_id: execution_request_id(subject, s.security_epoch, [1; 32], sequence),
        expires_at: time(&f.ic) + MINUTE,
        signature: ByteBuf::new(),
    };
    approval.signature = key(1)
        .sign(&approval_message(
            f.user,
            subject,
            "dmsg/execute/v1",
            &(&kind, max_cycles),
            &approval,
        ))
        .to_bytes()
        .to_vec()
        .into();
    ExecuteRequest {
        subject,
        kind,
        max_cycles,
        approval,
    }
}
fn root_user(f: &Fixture) -> Hash {
    let subject = f.create(1);
    f.recoverable(1, subject);
    f.mutate(
        1,
        subject,
        AccountCommand::ReserveRoot {
            expected_generation: 0,
            op_id: [8; 32],
        },
    )
    .unwrap();
    f.mutate(
        1,
        subject,
        AccountCommand::CommitRoot {
            expected_generation: 0,
            op_id: [8; 32],
            root: ContentRootRef {
                generation: 1,
                suite: "dmsg-root-v1".into(),
                home_cose: f.cose,
                derivation_version: 1,
                key_generation: 1,
                bundle_digest: [12; 32],
                recovery_generation: 1,
            },
        },
    )
    .unwrap();
    let initialized: Result<candid::Reserved> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    initialized.unwrap();
    subject
}

#[test]
fn transfer_from_loss_is_reconciled_using_a_standard_2xfer_block() {
    let f = Fixture::new();
    let owner = f.create(1);
    let snapshot = LegacySnapshot {
        source_canister: person(80),
        snapshot_id: [5; 32],
        freeze_version: 1,
        event_tip: [7; 32],
        count: 0,
        entries_digest: [0; 32],
    };
    let started: Result<()> = update(
        &f.ic,
        f.handle,
        Principal::anonymous(),
        "begin_legacy_snapshot",
        (snapshot,),
    );
    started.unwrap();
    let sealed: Result<candid::Reserved> = update(
        &f.ic,
        f.handle,
        Principal::anonymous(),
        "seal_legacy_snapshot",
        (),
    );
    sealed.unwrap();
    let amount = price("newname") - 10;
    let payer = account(person(1));
    let op_id = [11u8; 32];
    let intent = HandleIntent {
        handle_canister: f.handle,
        action: HandleAction::Register,
        subject: owner,
        target_subject: None,
        handle: "newname".into(),
        expected_version: 0,
        op_id,
        terms_digest: digest("dmsg/handle-charge/v1", &(f.ledger, payer, amount, 10u128)),
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
    reserved.unwrap();
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "lose_next_response",
        (),
    );
    let lost: Result<HandleOperation> =
        update(&f.ic, f.handle, person(1), "commit_handle", (owner, op_id));
    assert_eq!(lost, Err(Error::ExecutionUnknown));
    let block: icrc_ledger_types::icrc3::blocks::GetBlocksResult = query(
        &f.ic,
        f.ledger,
        person(1),
        "icrc3_get_blocks",
        (vec![icrc_ledger_types::icrc3::blocks::GetBlocksRequest {
            start: 0u64.into(),
            length: 1u64.into(),
        }],),
    );
    let icrc_ledger_types::icrc::generic_value::ICRC3Value::Map(fields) = &block.blocks[0].block
    else {
        panic!("map")
    };
    assert_eq!(
        fields["btype"],
        icrc_ledger_types::icrc::generic_value::ICRC3Value::Text("2xfer".into())
    );
    let reconciled: Result<HandleOperation> = update(
        &f.ic,
        f.handle,
        person(99),
        "reconcile_handle_charge",
        (owner, op_id, 0u64),
    );
    let reconciled = reconciled.unwrap();
    assert_eq!(reconciled.phase, HandlePhase::Committed);
    let duplicate: Result<HandleOperation> =
        update(&f.ic, f.handle, person(1), "commit_handle", (owner, op_id));
    assert_eq!(duplicate.unwrap(), reconciled);
    let balance: Nat = query(&f.ic, f.ledger, person(1), "icrc1_balance_of", (payer,));
    assert_eq!(balance, 0u64);
}

#[test]
fn unbound_requester_can_verify_reconfirm_and_finish_after_original_expiry() {
    let f = Fixture::new();
    let subject = f.create(1);
    f.recoverable(1, subject);
    let public: Result<(SecuritySnapshot, std::collections::BTreeMap<Hash, Device>)> =
        query(&f.ic, f.user, person(9), "get_device_bundle", (subject,));
    let snapshot = public.unwrap().0;
    assert_eq!(
        snapshot.recovery_signing_pub,
        Some(key(70).verifying_key().to_bytes())
    );
    let request = RecoveryRequest {
        op_id: [41; 32],
        new_auth: person(9),
        device: device(9),
        generation: snapshot.recovery_root_version,
        expires_at: time(&f.ic) + DAY + MINUTE,
    };
    let signature = key(70)
        .sign(&digest(
            "dmsg/recovery-request/v1",
            &(f.user, subject, snapshot.recovery_nonce, &request),
        ))
        .to_bytes()
        .to_vec();
    let pop = key(9)
        .sign(&digest(
            "dmsg/recovery-device/v1",
            &(f.user, subject, &request),
        ))
        .to_bytes()
        .to_vec();
    let submitted: Result<()> = update(
        &f.ic,
        f.user,
        person(9),
        "request_recovery",
        (
            subject,
            request.clone(),
            ByteBuf::from(signature),
            ByteBuf::from(pop),
        ),
    );
    submitted.unwrap();
    f.mutate(
        1,
        subject,
        AccountCommand::DisputeRecovery {
            op_id: request.op_id,
            dispute: [42; 32],
        },
    )
    .unwrap();
    let full: Result<Subject> = query(&f.ic, f.user, person(9), "get_subject", (subject,));
    assert_eq!(full, Err(Error::AuthRequired));
    let unauthorized: Result<Option<PendingRecovery>> = query(
        &f.ic,
        f.user,
        person(99),
        "get_recovery_request",
        (subject,),
    );
    assert_eq!(unauthorized, Err(Error::AuthRequired));
    let pending: Result<Option<PendingRecovery>> =
        query(&f.ic, f.user, person(9), "get_recovery_request", (subject,));
    let pending = pending.unwrap().unwrap();
    let certified: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(9),
        "security_snapshot_batch",
        (vec![subject],),
    );
    let leaf: SecuritySnapshot =
        decode_canonical(certified.unwrap().entries[0].value.as_ref().unwrap()).unwrap();
    assert_eq!(
        leaf.pending_recovery_digest,
        Some(digest("dmsg/pending-recovery/v1", &pending))
    );
    f.ic.advance_time(Duration::from_secs(120));
    let confirmation = RecoveryConfirmation {
        request_id: request.op_id,
        dispute: pending.dispute.unwrap(),
        expires_at: time(&f.ic) + 2 * DAY,
    };
    let sig = ByteBuf::from(
        key(70)
            .sign(&recovery_confirmation_message(
                f.user,
                subject,
                leaf.recovery_nonce,
                &request,
                &confirmation,
            ))
            .to_bytes()
            .to_vec(),
    );
    let mut changed = confirmation.clone();
    changed.expires_at += 1;
    let tampered: Result<()> = update(
        &f.ic,
        f.user,
        person(9),
        "reconfirm_recovery",
        (subject, changed, sig.clone()),
    );
    assert_eq!(tampered, Err(Error::IntegrityFailed));
    let accepted: Result<()> = update(
        &f.ic,
        f.user,
        person(9),
        "reconfirm_recovery",
        (subject, confirmation.clone(), sig.clone()),
    );
    accepted.unwrap();
    f.mutate(
        1,
        subject,
        AccountCommand::DisputeRecovery {
            op_id: request.op_id,
            dispute: [43; 32],
        },
    )
    .unwrap();
    let confirmed: Result<Option<PendingRecovery>> =
        query(&f.ic, f.user, person(9), "get_recovery_request", (subject,));
    let confirmed = confirmed.unwrap().unwrap();
    assert!(confirmed.execute_after > request.expires_at);
    let retry: Result<()> = update(
        &f.ic,
        f.user,
        person(9),
        "reconfirm_recovery",
        (subject, confirmation, sig),
    );
    retry.unwrap();
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let restored: Result<Option<PendingRecovery>> =
        query(&f.ic, f.user, person(9), "get_recovery_request", (subject,));
    assert_eq!(restored.unwrap(), Some(confirmed.clone()));
    f.ic.advance_time(Duration::from_nanos(confirmed.execute_after - time(&f.ic)));
    let completed: Result<()> = update(&f.ic, f.user, person(9), "complete_recovery", (subject,));
    completed.unwrap();
    assert_eq!(f.subject(9, subject).auth_bindings, vec![person(9)]);
}

#[test]
fn invalid_transport_is_rejected_without_consuming_authorization() {
    let f = Fixture::new();
    let subject = root_user(&f);
    let before = f.subject(1, subject);
    let bad = derivation_request(&f, subject, vec![1; 48]);
    let result: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "authorize_and_execute", (bad,));
    assert_eq!(result, Err(Error::IntegrityFailed));
    assert_eq!(f.subject(1, subject), before);
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32]).unwrap();
    assert_eq!(
        f.derive(1, subject, transport.public_key()).status,
        ExecutionStatus::Completed
    );
}

#[test]
fn cleaned_request_id_cannot_be_reapproved_for_another_operation() {
    let f = Fixture::new();
    let subject = root_user(&f);
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32]).unwrap();
    let first = derivation_request(&f, subject, transport.public_key());
    let result: Result<ExecutionResult> = update(
        &f.ic,
        f.user,
        person(1),
        "authorize_and_execute",
        (first.clone(),),
    );
    assert_eq!(result.unwrap().status, ExecutionStatus::Completed);
    for _ in 0..65 {
        f.mutate(
            1,
            subject,
            AccountCommand::SetPolicy {
                policy: SensitivePolicy::default(),
            },
        )
        .unwrap();
    }
    f.ic.advance_time(Duration::from_secs(2 * 24 * 60 * 60));
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![92; 32]).unwrap();
    f.derive(1, subject, transport.public_key());
    let expired: Result<ExecutionResult> = query(
        &f.ic,
        f.cose,
        f.user,
        "get_execution",
        (subject, first.approval.request_id),
    );
    assert_eq!(expired, Err(Error::NotFound));
    let mut reused = derivation_request(&f, subject, transport.public_key());
    reused.approval.request_id = first.approval.request_id;
    reused.approval.signature = key(1)
        .sign(&approval_message(
            f.user,
            subject,
            "dmsg/execute/v1",
            &(&reused.kind, reused.max_cycles),
            &reused.approval,
        ))
        .to_bytes()
        .to_vec()
        .into();
    let refused: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "authorize_and_execute", (reused,));
    assert_eq!(refused, Err(Error::IdempotencyConflict));
}

#[test]
fn valid_presigned_offer_survives_a_minute_but_current_revocation_still_applies() {
    let f = Fixture::new();
    let subject = f.create(2);
    let mut old = f.order(subject, 2, 1);
    old.offer.offer.expires_at = time(&f.ic) + DAY;
    old.offer.signature = key(2)
        .sign(&digest("dmsg/payment-offer/v1", &old.offer.offer))
        .to_bytes()
        .to_vec()
        .into();
    old.quote.offer_digest = digest("dmsg/payment-offer/v1", &old.offer.offer);
    old.quote_signature = key(50)
        .sign(&digest("dmsg/quote/v1", &old.quote))
        .to_bytes()
        .to_vec()
        .into();
    f.ic.advance_time(Duration::from_secs(61));
    let accepted: Result<Escrow> = update(&f.ic, f.payment, person(40), "open_escrow", (old,));
    accepted.unwrap();
    let stale = f.order(subject, 2, 2);
    f.mutate(
        2,
        subject,
        AccountCommand::SetPolicy {
            policy: SensitivePolicy::default(),
        },
    )
    .unwrap();
    let denied: Result<Escrow> = update(&f.ic, f.payment, person(40), "open_escrow", (stale,));
    assert_eq!(denied, Err(Error::PolicyStale));
}

#[test]
fn repricing_rejects_unrelated_callers_and_invented_fees() {
    let f = Fixture::new();
    let e = settle_order(&f, &open_order(&f));
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "set_fee",
        (20u128,),
    );
    let failed: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(99),
        "process_transfer",
        (e.escrow_id, 0u64),
    );
    assert_eq!(failed.unwrap().expected_fee, Some(20));
    for _ in 0..16 {
        let stranger: Result<TransferLeg> = update(
            &f.ic,
            f.payment,
            person(99),
            "revise_rejected_transfer",
            (e.escrow_id, 0u64, 0u128),
        );
        assert_eq!(stranger, Err(Error::Forbidden));
        let wrong_fee: Result<TransferLeg> = update(
            &f.ic,
            f.payment,
            person(40),
            "revise_rejected_transfer",
            (e.escrow_id, 0u64, 0u128),
        );
        assert_eq!(wrong_fee, Err(Error::FeeBlocked));
    }
    let after: Result<Escrow> = query(&f.ic, f.payment, person(99), "get_escrow", (e.escrow_id,));
    assert_eq!(after.unwrap().next_leg, 2);
    let revised: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "revise_rejected_transfer",
        (e.escrow_id, 0u64, 20u128),
    );
    let revised = revised.unwrap();
    let paid: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(99),
        "process_transfer",
        (e.escrow_id, revised.leg_id),
    );
    assert_eq!(paid.unwrap().status, LegStatus::Succeeded);
}

#[test]
fn bounded_failed_history_keeps_the_latest_transfer_recoverable() {
    let f = Fixture::new();
    let e = settle_order(&f, &open_order(&f));
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "reject_next_transfers",
        (16u32,),
    );
    let mut latest = 0u64;
    for _ in 0..16 {
        let rejected: Result<TransferLeg> = update(
            &f.ic,
            f.payment,
            person(99),
            "process_transfer",
            (e.escrow_id, latest),
        );
        assert_eq!(rejected.unwrap().status, LegStatus::Rejected);
        let revised: Result<TransferLeg> = update(
            &f.ic,
            f.payment,
            person(40),
            "revise_rejected_transfer",
            (e.escrow_id, latest, 10u128),
        );
        latest = revised.unwrap().leg_id;
    }
    let legs: Result<Vec<TransferLeg>> = query(
        &f.ic,
        f.payment,
        person(99),
        "list_transfers",
        (e.escrow_id, None::<u64>),
    );
    let legs = legs.unwrap();
    assert_eq!(legs.len(), TRANSFER_HISTORY_LIMIT + 1); // independent platform leg
    let current = legs.iter().find(|l| l.leg_id == latest).unwrap();
    assert_eq!(current.revision, 16);
    assert_ne!(current.history_digest, [0; 32]);
    let stale: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "revise_rejected_transfer",
        (e.escrow_id, 0u64, 10u128),
    );
    assert_eq!(stale, Err(Error::NotFound));
    f.ic.upgrade_canister(
        f.payment,
        wasm("dmsg_payment"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    for id in [latest, 1] {
        let sent: Result<TransferLeg> = update(
            &f.ic,
            f.payment,
            person(99),
            "process_transfer",
            (e.escrow_id, id),
        );
        assert_eq!(sent.unwrap().status, LegStatus::Succeeded);
    }
    let current: Result<Escrow> = query(&f.ic, f.payment, person(99), "get_escrow", (e.escrow_id,));
    let current = current.unwrap();
    assert!(current.conserved());
    assert_eq!(current.pending_payouts, 0);
    assert_eq!(current.transferred, 1100);
}
