use super::*;

fn text_statement(f: &Fixture, id: &AccountId, text: &str) -> Statement {
    Statement {
        issuer: f.account_id(1, id).issuer,
        subject: Some("release".into()),
        issued_at: None,
        content: StatementContent::Text(text.into()),
    }
}

// Run explicitly with --ignored --nocapture against each build's DMSG_WASM_DIR.
// Only the user canister's balance delta is measured. PocketIC's deterministic
// clock does not model production traffic.
#[test]
#[ignore = "cycles comparison for dmsg_user builds"]
fn user_cycles_profile() {
    let f = Fixture::new();
    let id = f.create(1);
    let policy = SensitivePolicy {
        daily_executions: 100,
        ..SensitivePolicy::default()
    };
    f.mutate(
        1,
        &id,
        AccountCommand::SetPolicy {
            policy: policy.clone(),
        },
    )
    .unwrap();
    for history in 0..9 {
        let measured = [0, 4, 8].contains(&history);
        if measured {
            let offer = f.order(&id, 1, 1).offer;
            let before = f.ic.cycle_balance(f.user);
            let checked: Result<u64> =
                update(&f.ic, f.user, f.payment, "verify_payment_offer", (offer,));
            checked.unwrap();
            println!(
                "user_cycles history={history} method=verify_payment_offer cycles={}",
                before - f.ic.cycle_balance(f.user)
            );
            let before = f.ic.cycle_balance(f.user);
            f.mutate(
                1,
                &id,
                AccountCommand::SetPolicy {
                    policy: policy.clone(),
                },
            )
            .unwrap();
            println!(
                "user_cycles history={history} method=mutate_account cycles={}",
                before - f.ic.cycle_balance(f.user)
            );
        }
        let request = f.attest_request(1, &id, text_statement(&f, &id, &"x".repeat(4096)));
        let before = f.ic.cycle_balance(f.user);
        let result: Result<SignedArtifact> =
            update(&f.ic, f.user, person(1), "attest", (request.clone(),));
        let result = result.unwrap();
        if measured {
            println!(
                "user_cycles history={history} method=attest cycles={}",
                before - f.ic.cycle_balance(f.user)
            );
            let before = f.ic.cycle_balance(f.user);
            let retried: Result<SignedArtifact> =
                update(&f.ic, f.user, person(1), "attest", (request,));
            assert_eq!(retried, Ok(result));
            println!(
                "user_cycles history={history} method=attest_retry cycles={}",
                before - f.ic.cycle_balance(f.user)
            );
        }
    }
}

#[test]
fn file_statement_attestation_uses_statement_policy_and_binds_the_opinion_and_file() {
    let f = Fixture::new();
    let id = f.create(1);
    f.mutate(
        1,
        &id,
        AccountCommand::SetPolicy {
            policy: SensitivePolicy {
                allowed_purposes: vec![KeyPurpose::Statement],
                ..SensitivePolicy::default()
            },
        },
    )
    .unwrap();
    let mut statement = text_statement(&f, &id, "x");
    statement.content = StatementContent::FileStatement {
        text: "  第三章需要补充实验数据。\n".into(),
        sha256: sha256(b"document"),
        content_type: Some("application/pdf".into()),
        location: Some("urn:example:report".into()),
    };
    let request = f.attest_request(1, &id, statement.clone());
    // A website cannot swap either the opinion or the file after approval.
    for change_text in [true, false] {
        let mut changed = request.clone();
        if let StatementContent::FileStatement {
            text,
            sha256: digest,
            ..
        } = &mut changed.statement.content
        {
            if change_text {
                *text = "Approved.".into();
            } else {
                *digest = sha256(b"different");
            }
        }
        let before = f.account_id(1, &id);
        let rejected: Result<SignedArtifact> =
            update(&f.ic, f.user, person(1), "attest", (changed,));
        assert!(rejected.is_err());
        assert_eq!(f.account_id(1, &id), before);
    }
    let result: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest", (request.clone(),));
    let artifact = result.unwrap();
    assert_eq!(verify_artifact(&artifact).unwrap(), statement);
    let retried: Result<SignedArtifact> = update(&f.ic, f.user, person(1), "attest", (request,));
    assert_eq!(retried.unwrap(), artifact);
    // The digest profile is outside this account's policy.
    let mut digest_statement = text_statement(&f, &id, "x");
    digest_statement.content = StatementContent::Digest {
        sha256: sha256(b"document"),
        content_type: None,
        location: None,
    };
    assert_eq!(
        f.attest(1, &id, digest_statement),
        Err(Error::Forbidden)
    );
}

#[test]
fn user_execution_retention_survives_a_full_window_and_upgrade() {
    let f = Fixture::new();
    let id = f.create(1);
    f.mutate(
        1,
        &id,
        AccountCommand::SetPolicy {
            policy: SensitivePolicy {
                daily_executions: 100,
                ..SensitivePolicy::default()
            },
        },
    )
    .unwrap();
    let request = |n: usize| f.attest_request(1, &id, text_statement(&f, &id, &format!("s{n}")));
    let first = request(0);
    for n in 0..dmsg_runtime::WINDOW {
        let r = if n == 0 { first.clone() } else { request(n) };
        let result: Result<SignedArtifact> = update(&f.ic, f.user, person(1), "attest", (r,));
        result.unwrap();
    }
    let before = f.account_id(1, &id);
    let refused: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest", (request(64),));
    assert_eq!(refused, Err(Error::QuotaExceeded));
    assert_eq!(f.account_id(1, &id), before);

    f.ic.advance_time(Duration::from_secs(2 * 24 * 60 * 60));
    let next = request(65);
    let result: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest", (next.clone(),));
    let artifact = result.unwrap();
    let expired: Result<SignedArtifact> = query(
        &f.ic,
        f.user,
        person(1),
        "get_attestation",
        (&id, first.approval.request_id),
    );
    assert_eq!(expired, Err(Error::ResultExpired));
    let receipt: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_receipt",
        (&id, first.approval.request_id),
    );
    let receipt = receipt.unwrap();
    assert!(receipt.entries[0].value.is_none());
    let witness: ic_certification::HashTree =
        cbor2::from_slice(&receipt.entries[0].witness).unwrap();
    assert_eq!(
        witness.lookup_path([execution_receipt_key(&id, first.approval.request_id)]),
        ic_certification::LookupResult::Absent
    );

    let account_before = f.account_id(1, &id);
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(f.account_id(1, &id), account_before);
    let restored: Result<SignedArtifact> = query(
        &f.ic,
        f.user,
        person(1),
        "get_attestation",
        (&id, next.approval.request_id),
    );
    assert_eq!(restored, Ok(artifact.clone()));
    let receipt: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_receipt",
        (&id, next.approval.request_id),
    );
    let leaf: ExecutionReceipt = cbor2::from_slice(&certified_value(
        &f,
        receipt.unwrap(),
        &execution_receipt_key(&id, next.approval.request_id),
    ))
    .unwrap();
    match_execution_receipt(&artifact, &leaf).unwrap();
    let replayed: Result<SignedArtifact> = update(&f.ic, f.user, person(1), "attest", (first,));
    assert_eq!(replayed, Err(Error::ResultExpired));
    assert_eq!(f.account_id(1, &id), account_before);
}

#[test]
fn root_public_key_is_queryable_after_initialization_and_opens_ibe_envelopes() {
    let f = Fixture::new();
    let account_id = f.root_account(1);
    let not_ready: Result<KeyDescriptor> = query(
        &f.ic,
        f.cose,
        Principal::anonymous(),
        "root_public_key",
        (&account_id, 1u64),
    );
    assert!(matches!(not_ready, Err(Error::Unavailable(_))));
    let initialized: Result<candid::Reserved> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    initialized.unwrap();
    let described: Result<KeyDescriptor> = query(
        &f.ic,
        f.cose,
        Principal::anonymous(),
        "root_public_key",
        (&account_id, 1u64),
    );
    let described = described.unwrap();
    assert_eq!(described.public_key.len(), 96);
    assert_eq!(described.public_key_fingerprint, sha256(&described.public_key));
    // The same executor key serves every account and generation.
    let other: Result<KeyDescriptor> = query(
        &f.ic,
        f.cose,
        Principal::anonymous(),
        "root_public_key",
        (&AccountId([2; 12]), 5u64),
    );
    let other = other.unwrap();
    assert_eq!(other.public_key, described.public_key);
    assert_eq!(other.key_generation, 5);
    let public = ic_vetkeys::DerivedPublicKey::deserialize(&described.public_key).unwrap();
    let identity = ic_vetkeys::IbeIdentity::from_bytes(&canonical(&(&account_id, 1u64)));
    let envelope = ic_vetkeys::IbeCiphertext::encrypt(
        &public,
        &identity,
        b"root",
        &ic_vetkeys::IbeSeed::from_bytes(&[3; 32]).unwrap(),
    );
    f.recover(1, 9, &account_id);
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32]).unwrap();
    let request = f.derive_request(1, 9, &account_id, transport.public_key());
    let before: Result<ExecutionResult> = query(
        &f.ic,
        f.cose,
        f.user,
        "get_execution",
        (&account_id, request.approval.request_id),
    );
    assert_eq!(before, Err(Error::NotFound));
    let mut altered = request.clone();
    altered.max_cycles -= 1;
    let rejected: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "derive_root", (altered,));
    assert_eq!(rejected, Err(Error::IntegrityFailed));
    let derived: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "derive_root", (request.clone(),));
    let derived = derived.unwrap();
    let output = completed(&derived);
    assert_eq!(output.key, described);
    assert!(derived.cycles_cost_upper_bound > 0 && derived.cycles_cost_upper_bound <= request.max_cycles);
    let vetkey = ic_vetkeys::EncryptedVetKey::deserialize(&output.encrypted_key)
        .unwrap()
        .decrypt_and_verify(&transport, &public, identity.value())
        .unwrap();
    assert_eq!(envelope.decrypt(&vetkey).unwrap(), b"root");
    let replay: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "derive_root", (request.clone(),));
    assert_eq!(replay.unwrap(), derived);
    let status: Result<ExecutionResult> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution",
        (&account_id, request.approval.request_id),
    );
    assert_eq!(status.unwrap(), derived);
    let grant = ExecutionGrant {
        account_id,
        home_user: f.user,
        home_cose: f.cose,
        request_id: request.approval.request_id,
        execution_sequence: 1,
        security_epoch: request.approval.security_epoch,
        device_id: request.approval.device_id,
        device_sequence: request.approval.sequence,
        approved_at: time(&f.ic),
        expires_at: request.approval.expires_at,
        generation: 1,
        transport_key: request.transport_public_key,
        max_cycles: request.max_cycles,
    };
    // A client cannot bypass its user home by calling COSE directly.
    assert_denied(&f.ic, f.cose, person(1), "execute", (grant,));
    f.ic.upgrade_canister(
        f.cose,
        wasm("dmsg_cose"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let restored: Result<KeyDescriptor> = query(
        &f.ic,
        f.cose,
        Principal::anonymous(),
        "root_public_key",
        (&account_id, 1u64),
    );
    assert_eq!(restored, Ok(described));
}

fn open_order(f: &Fixture) -> EscrowInfo {
    let account_id = f.create(2);
    let input = f.order(&account_id, 2, 1);
    let opened: Result<EscrowInfo> = update(&f.ic, f.payment, person(40), "open_escrow", (input,));
    opened.unwrap()
}
fn settle_order(f: &Fixture, e: &EscrowInfo) -> EscrowInfo {
    let block = f.fund(e, e.quote.amount);
    let funded: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "check_funding",
        (e.escrow_id, block),
    );
    let decided: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "finalize_receipt",
        (f.receipt(&funded.unwrap()),),
    );
    decided.unwrap()
}
/// A root account recovered onto device 9 with the COSE keys ready.
fn recovered_root_user(f: &Fixture) -> AccountId {
    let account_id = f.root_account(1);
    let initialized: Result<candid::Reserved> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    initialized.unwrap();
    f.recover(1, 9, &account_id);
    account_id
}

#[test]
fn transfer_from_loss_is_reconciled_using_a_standard_2xfer_block() {
    let f = Fixture::new();
    let owner = f.create(1);
    let snapshot = LegacySnapshot {
        source_canister: person(80),
        snapshot_id: Hash::new([5; 32]),
        freeze_version: 1,
        event_tip: Hash::new([7; 32]),
        count: 0,
        entries_digest: Hash::new([0; 32]),
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
    let op_id = Hash::new([11u8; 32]);
    let intent = HandleIntent {
        handle_canister: f.handle,
        action: HandleAction::Register,
        account_id: owner,
        target_account: None,
        handle: "newname".into(),
        expected_version: 0,
        op_id,
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
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "lose_next_response",
        (),
    );
    let lost: Result<HandleOperation> = update(
        &f.ic,
        f.handle,
        person(1),
        "register_handle",
        (Registration {
            intent,
            payer,
            fee: 10,
        },),
    );
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
        (&owner, op_id, 0u64),
    );
    let reconciled = reconciled.unwrap();
    assert_eq!(reconciled.phase, HandlePhase::Committed);
    let duplicate: Result<HandleOperation> =
        update(&f.ic, f.handle, person(1), "commit_handle", (&owner, op_id));
    assert_eq!(duplicate.unwrap(), reconciled);
    let balance: Nat = query(&f.ic, f.ledger, person(1), "icrc1_balance_of", (payer,));
    assert_eq!(balance, 0u64);
}

#[test]
fn login_recovery_is_visible_disputable_and_completes_after_the_delay() {
    let f = Fixture::new();
    let account_id = f.root_account(1);
    let s = f.account_id(1, &account_id);
    let request = RecoveryRequest {
        op_id: Hash::new([41; 32]),
        new_auth: person(1),
        device: device(9),
        expires_at: time(&f.ic) + s.recovery_delay_ms + DAY,
    };
    let pop = ByteBuf::from(
        key(9)
            .sign(recovery_device_message(f.user, &account_id, &request).as_slice())
            .to_bytes()
            .to_vec(),
    );
    let submitted: Result<()> = update(
        &f.ic,
        f.user,
        person(1),
        "request_recovery",
        (&account_id, request.clone(), pop.clone()),
    );
    submitted.unwrap();
    let unauthorized: Result<Option<PendingRecovery>> = query(
        &f.ic,
        f.user,
        person(99),
        "get_recovery_request",
        (&account_id,),
    );
    assert_eq!(unauthorized, Err(Error::AuthRequired));
    let pending: Result<Option<PendingRecovery>> = query(
        &f.ic,
        f.user,
        person(1),
        "get_recovery_request",
        (&account_id,),
    );
    let pending = pending.unwrap().unwrap();
    assert_eq!(pending.execute_after, time(&f.ic) + s.recovery_delay_ms);
    let certified: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(9),
        "security_snapshot_batch",
        (vec![&account_id],),
    );
    let leaf: SecuritySnapshot =
        decode_canonical(certified.unwrap().entries[0].value.as_ref().unwrap()).unwrap();
    assert_eq!(
        leaf.pending_recovery_digest,
        Some(digest("dmsg/pending-recovery/v1", &pending))
    );
    // A device in hand cancels the takeover; completing it is then impossible.
    f.mutate(
        1,
        &account_id,
        AccountCommand::DisputeRecovery {
            op_id: request.op_id,
        },
    )
    .unwrap();
    let cancelled: Result<Option<PendingRecovery>> = query(
        &f.ic,
        f.user,
        person(1),
        "get_recovery_request",
        (&account_id,),
    );
    assert_eq!(cancelled, Ok(None));
    f.ic.advance_time(Duration::from_millis(s.recovery_delay_ms + 1));
    let missing: Result<()> = update(
        &f.ic,
        f.user,
        person(1),
        "complete_recovery",
        (&account_id, request.op_id),
    );
    assert_eq!(missing, Err(Error::NotFound));
    assert_eq!(f.account_id(1, &account_id).devices.len(), 1);
    // Without a dispute the takeover survives an upgrade and completes.
    let request = RecoveryRequest {
        op_id: Hash::new([42; 32]),
        expires_at: time(&f.ic) + s.recovery_delay_ms + DAY,
        ..request
    };
    let pop = ByteBuf::from(
        key(9)
            .sign(recovery_device_message(f.user, &account_id, &request).as_slice())
            .to_bytes()
            .to_vec(),
    );
    let submitted: Result<()> = update(
        &f.ic,
        f.user,
        person(1),
        "request_recovery",
        (&account_id, request.clone(), pop),
    );
    submitted.unwrap();
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let restored: Result<Option<PendingRecovery>> = query(
        &f.ic,
        f.user,
        person(1),
        "get_recovery_request",
        (&account_id,),
    );
    let restored = restored.unwrap().unwrap();
    assert_eq!(restored.request, request);
    let early: Result<()> = update(
        &f.ic,
        f.user,
        person(1),
        "complete_recovery",
        (&account_id, request.op_id),
    );
    assert_eq!(early, Err(Error::Expired));
    f.ic.advance_time(Duration::from_millis(restored.execute_after - time(&f.ic)));
    let completed: Result<()> = update(
        &f.ic,
        f.user,
        person(1),
        "complete_recovery",
        (&account_id, request.op_id),
    );
    completed.unwrap();
    let after = f.account_id(1, &account_id);
    assert_eq!(after.auth_bindings, vec![person(1)]);
    assert_eq!(after.devices.keys().copied().collect::<Vec<_>>(), vec![Hash::new([9; 32])]);
    assert_eq!(after.recovered_device, Some((Hash::new([9; 32]), 1)));
}

#[test]
fn invalid_transport_is_rejected_without_consuming_authorization() {
    let f = Fixture::new();
    let account_id = recovered_root_user(&f);
    let before = f.account_id(1, &account_id);
    let bad = f.derive_request(1, 9, &account_id, vec![1; 48]);
    let result: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "derive_root", (bad,));
    assert_eq!(result, Err(Error::IntegrityFailed));
    assert_eq!(f.account_id(1, &account_id), before);
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32]).unwrap();
    assert_eq!(
        f.derive(1, 9, &account_id, transport.public_key()).status(),
        ExecutionStatus::Completed
    );
}

#[test]
fn cleaned_request_id_cannot_be_reapproved_for_another_operation() {
    let f = Fixture::new();
    let account_id = recovered_root_user(&f);
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32]).unwrap();
    let first = f.derive_request(1, 9, &account_id, transport.public_key());
    let result: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "derive_root", (first.clone(),));
    assert_eq!(result.unwrap().status(), ExecutionStatus::Completed);
    for _ in 0..65 {
        f.mutate_by(
            1,
            9,
            &account_id,
            AccountCommand::SetPolicy {
                policy: SensitivePolicy::default(),
            },
        )
        .unwrap();
    }
    f.ic.advance_time(Duration::from_secs(2 * 24 * 60 * 60));
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![92; 32]).unwrap();
    f.derive(1, 9, &account_id, transport.public_key());
    let expired: Result<ExecutionResult> = query(
        &f.ic,
        f.cose,
        f.user,
        "get_execution",
        (&account_id, first.approval.request_id),
    );
    assert_eq!(expired, Err(Error::NotFound));
    let mut reused = f.derive_request(1, 9, &account_id, transport.public_key());
    reused.approval.request_id = first.approval.request_id;
    reused.approval.signature = key(9)
        .sign(derive_approval(f.user, &reused).as_slice())
        .to_bytes()
        .into();
    let refused: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "derive_root", (reused,));
    assert_eq!(refused, Err(Error::IdempotencyConflict));
}

#[test]
fn valid_presigned_offer_survives_a_minute_but_current_revocation_still_applies() {
    let f = Fixture::new();
    let account_id = f.create(2);
    let mut old = f.order(&account_id, 2, 1);
    old.offer.offer.expires_at = time(&f.ic) + DAY;
    old.offer.signature = key(2)
        .sign(digest("dmsg/payment-offer/v1", &old.offer.offer).as_slice())
        .to_bytes()
        .into();
    old.quote.offer_digest = digest("dmsg/payment-offer/v1", &old.offer.offer);
    old.quote_signature = key(50)
        .sign(digest("dmsg/quote/v2", &old.quote).as_slice())
        .to_bytes()
        .into();
    f.ic.advance_time(Duration::from_secs(61));
    let accepted: Result<EscrowInfo> = update(&f.ic, f.payment, person(40), "open_escrow", (old,));
    accepted.unwrap();
    let stale = f.order(&account_id, 2, 2);
    f.mutate(
        2,
        &account_id,
        AccountCommand::SetPolicy {
            policy: SensitivePolicy::default(),
        },
    )
    .unwrap();
    let denied: Result<EscrowInfo> = update(&f.ic, f.payment, person(40), "open_escrow", (stale,));
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
    let after: Result<EscrowInfo> =
        query(&f.ic, f.payment, person(99), "get_escrow", (e.escrow_id,));
    assert_eq!(after.unwrap().escrow_id, e.escrow_id);
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
    assert_ne!(current.history_digest, Hash::new([0; 32]));
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
    let current: Result<EscrowInfo> =
        query(&f.ic, f.payment, person(99), "get_escrow", (e.escrow_id,));
    let current = current.unwrap();
    assert!(funds_conserved(&current));
    assert_eq!(current.decision, FundsDecision::SettlementCommitted);
    assert_eq!(current.transferred, 1100);
}

#[test]
fn clean_transport_rejection_preserves_earlier_unknown_payouts() {
    let f = Fixture::new();
    let escrow = settle_order(&f, &open_order(&f));
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "lose_next_response",
        (),
    );
    let unknown: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(99),
        "process_transfer",
        (escrow.escrow_id, 0u64),
    );
    assert_eq!(unknown, Err(Error::ExecutionUnknown));
    let before: Result<EscrowInfo> = query(
        &f.ic,
        f.payment,
        person(99),
        "get_escrow",
        (escrow.escrow_id,),
    );

    // Removing this disposable test ledger causes a clean DestinationInvalid
    // rejection. It says nothing about the first transfer's lost reply.
    f.ic.stop_canister(f.ledger, None).unwrap();
    f.ic.delete_canister(f.ledger, None).unwrap();
    let unsent: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(99),
        "process_transfer",
        (escrow.escrow_id, 1u64),
    );
    assert!(matches!(unsent, Err(Error::Unavailable(_))), "{unsent:?}");
    let still_unknown: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(99),
        "process_transfer",
        (escrow.escrow_id, 0u64),
    );
    assert_eq!(still_unknown, Err(Error::ExecutionUnknown));
    let legs: Result<Vec<TransferLeg>> = query(
        &f.ic,
        f.payment,
        person(99),
        "list_transfers",
        (escrow.escrow_id, None::<u64>),
    );
    let legs = legs.unwrap();
    assert_eq!(
        legs.iter().find(|leg| leg.leg_id == 0).unwrap().status,
        LegStatus::Unknown
    );
    assert_eq!(
        legs.iter().find(|leg| leg.leg_id == 1).unwrap().status,
        LegStatus::Rejected
    );
    let after: Result<EscrowInfo> = query(
        &f.ic,
        f.payment,
        person(99),
        "get_escrow",
        (escrow.escrow_id,),
    );
    assert_eq!(before, after);
}
