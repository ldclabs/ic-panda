use super::*;

fn typed_statement(f: &Fixture, account_id: AccountId, algorithm: SigningAlgorithm) -> SignRequest {
    let s = f.account_id(1, account_id);
    let sequence = s.devices[&Hash::new([1; 32])].next_sequence;
    let mut request = SignRequest {
        account_id,
        key: f.key_ref(account_id, SigningPurpose::Statement, algorithm),
        statement: Statement {
            issuer: s.issuer,
            subject: Some("release".into()),
            issued_at: None,
            content: StatementContent::Text("verify an offline-derived key".into()),
        },
        origin: "https://example.com".into(),
        max_cycles: 100_000_000_000,
        approval: Approval {
            device_id: Hash::new([1; 32]),
            security_epoch: s.security_epoch,
            sequence,
            request_id: execution_request_id(
                account_id,
                s.security_epoch,
                Hash::new([1; 32]),
                sequence,
            ),
            expires_at: time(&f.ic) + MINUTE,
            signature: vec![].into(),
        },
    };
    request.approval.signature = key(1)
        .sign(
            request
                .clone()
                .into_execution()
                .unwrap()
                .approval_message(f.user)
                .as_slice(),
        )
        .to_bytes()
        .to_vec()
        .into();
    request
}

#[test]
fn keys_are_queryable_before_execution_and_verify_all_signing_algorithms() {
    use k256::ecdsa::signature::hazmat::PrehashVerifier;
    let f = Fixture::with_algorithms(vec![
        Algorithm::Ed25519,
        Algorithm::EcdsaSecp256k1,
        Algorithm::VetKdBls12381,
    ]);
    let account_id = f.create(1);
    f.recoverable(1, account_id);
    let selector = |algorithm| {
        KeySelector::Signing(dmsg_types::cose::SigningKey {
            purpose: SigningPurpose::Statement,
            algorithm,
        })
    };
    let not_ready: Result<KeyDescriptor> = query(
        &f.ic,
        f.cose,
        Principal::anonymous(),
        "public_key",
        (account_id, selector(SigningAlgorithm::Ed25519)),
    );
    assert!(matches!(not_ready, Err(Error::Unavailable(_))));
    let initialized: Result<candid::Reserved> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    initialized.unwrap();
    for algorithm in [SigningAlgorithm::Ed25519, SigningAlgorithm::EcdsaSecp256k1] {
        let described: Result<KeyDescriptor> = query(
            &f.ic,
            f.cose,
            Principal::anonymous(),
            "public_key",
            (account_id, selector(algorithm.clone())),
        );
        let described = described.unwrap();
        let request = typed_statement(&f, account_id, algorithm.clone());
        let before: Result<ExecutionResult> = query(
            &f.ic,
            f.cose,
            f.user,
            "get_execution",
            (account_id, request.approval.request_id),
        );
        assert_eq!(before, Err(Error::NotFound));
        let mut altered = request.clone();
        altered.statement.subject = Some("not approved".into());
        let rejected: Result<ExecutionResult> =
            update(&f.ic, f.user, person(1), "sign", (altered,));
        assert_eq!(rejected, Err(Error::IntegrityFailed));
        let signed: Result<ExecutionResult> =
            update(&f.ic, f.user, person(1), "sign", (request.clone(),));
        let signed = signed.unwrap();
        let output = signed.output().unwrap();
        assert!(matches!(output, ExecutionOutput::Signature { .. }));
        assert_eq!(output.key(), &described);
        assert!(signed.charged_cycles > 0 && signed.charged_cycles <= request.max_cycles);
        let ExecutionOutput::Signature { artifact, .. } = output else {
            panic!("signature output")
        };
        assert_eq!(verify_artifact(artifact).unwrap(), request.statement);
        let wire: cbor2::Value = cbor2::from_slice(&artifact.cose_sign1).unwrap();
        let cbor2::Value::Tag(18, wire) = wire else {
            panic!("COSE tag")
        };
        let cbor2::Value::Array(fields) = *wire else {
            panic!("COSE array")
        };
        let cbor2::Value::Bytes(protected) = &fields[0] else {
            panic!("protected bstr")
        };
        let cbor2::Value::Bytes(signature) = &fields[3] else {
            panic!("signature bstr")
        };
        let cbor2::Value::Bytes(raw_payload) = &fields[2] else {
            panic!("payload bytes")
        };
        let payload = canonical(&(
            "Signature1",
            serde_bytes::Bytes::new(protected),
            serde_bytes::Bytes::new(&[]),
            serde_bytes::Bytes::new(raw_payload),
        ));
        match algorithm {
            SigningAlgorithm::Ed25519 => verify(
                &Hash::new(described.public_key.as_slice().try_into().unwrap()),
                &payload,
                signature,
            )
            .unwrap(),
            SigningAlgorithm::EcdsaSecp256k1 => {
                let key =
                    k256::ecdsa::VerifyingKey::from_sec1_bytes(&described.public_key).unwrap();
                let signature = k256::ecdsa::Signature::from_slice(signature).unwrap();
                let signature = signature.normalize_s();
                key.verify_prehash(sha256(&payload).as_slice(), &signature)
                    .unwrap();
            }
        }
        let replay: Result<ExecutionResult> =
            update(&f.ic, f.user, person(1), "sign", (request.clone(),));
        assert_eq!(replay.unwrap(), signed);
        let status: Result<ExecutionResult> = query(
            &f.ic,
            f.user,
            person(1),
            "get_execution",
            (account_id, request.approval.request_id),
        );
        assert_eq!(status.unwrap(), signed);
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
            kind: request.clone().into_execution().unwrap().kind,
            max_cycles: request.max_cycles,
        };
        let bypass: Result<ExecutionResult> = update(&f.ic, f.cose, person(1), "execute", (grant,));
        assert_eq!(bypass, Err(Error::Forbidden));
    }
    f.ic.upgrade_canister(
        f.cose,
        wasm("dmsg_cose"),
        candid::encode_args((None::<CoseInit>,)).unwrap(),
        None,
    )
    .unwrap();
    let restored: Result<KeyDescriptor> = query(
        &f.ic,
        f.cose,
        Principal::anonymous(),
        "public_key",
        (account_id, selector(SigningAlgorithm::Ed25519)),
    );
    assert!(restored.is_ok());
}

#[test]
fn candidate_root_is_typed_and_failed_execution_keeps_its_reason() {
    let f = Fixture::new();
    let account_id = f.create(1);
    f.recoverable(1, account_id);
    let initialized: Result<candid::Reserved> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    initialized.unwrap();
    let mut disabled = typed_statement(&f, account_id, SigningAlgorithm::Ed25519);
    disabled.key.public_key_fingerprint = Hash::new([99; 32]);
    disabled.approval.signature = key(1)
        .sign(
            disabled
                .clone()
                .into_execution()
                .unwrap()
                .approval_message(f.user)
                .as_slice(),
        )
        .to_bytes()
        .to_vec()
        .into();
    let failure: Result<ExecutionResult> = update(&f.ic, f.user, person(1), "sign", (disabled,));
    assert!(matches!(
        failure.unwrap().outcome,
        ExecutionOutcome::Failed(Error::IntegrityFailed)
    ));
    let op_id = Hash::new([8; 32]);
    f.mutate(
        1,
        account_id,
        AccountCommand::ReserveRoot {
            expected_generation: 0,
            op_id,
        },
    )
    .unwrap();
    let s = f.account_id(1, account_id);
    let generation = s.root_slot.as_ref().unwrap().generation;
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32]).unwrap();
    let mut request = DeriveRootRequest {
        account_id,
        target: RootTarget::Candidate { generation, op_id },
        transport_public_key: transport
            .public_key()
            .as_slice()
            .try_into()
            .map(serde_bytes::ByteArray::new)
            .unwrap(),
        max_cycles: 100_000_000_000,
        approval: typed_statement(&f, account_id, SigningAlgorithm::Ed25519).approval,
    };
    request.approval.signature = key(1)
        .sign(
            request
                .clone()
                .into_execution()
                .approval_message(f.user)
                .as_slice(),
        )
        .to_bytes()
        .to_vec()
        .into();
    let result: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "derive_root", (request,));
    let result = result.unwrap();
    let output = result.output().unwrap();
    assert!(matches!(output, ExecutionOutput::EncryptedRootKey { .. }));
    let described: Result<KeyDescriptor> = query(
        &f.ic,
        f.cose,
        Principal::anonymous(),
        "public_key",
        (account_id, KeySelector::ContentRoot { generation }),
    );
    assert_eq!(described.unwrap(), *output.key());
    let encrypted = ic_vetkeys::EncryptedVetKey::deserialize(output.bytes()).unwrap();
    let public = ic_vetkeys::DerivedPublicKey::deserialize(&output.key().public_key).unwrap();
    encrypted
        .decrypt_and_verify(&transport, &public, &canonical(&(account_id, generation)))
        .unwrap();
}

fn open_order(f: &Fixture) -> EscrowInfo {
    let account_id = f.create(2);
    let input = f.order(account_id, 2, 1);
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
fn derivation_request(f: &Fixture, account_id: AccountId, transport: Vec<u8>) -> ExecuteRequest {
    let s = f.account_id(1, account_id);
    let kind = ExecutionKind::Derive {
        generation: 1,
        root_op_id: None,
        transport_key: transport.into(),
    };
    let max_cycles = 100_000_000_000u128;
    let sequence = s.devices[&Hash::new([1; 32])].next_sequence;
    let mut approval = Approval {
        device_id: Hash::new([1; 32]),
        security_epoch: s.security_epoch,
        sequence,
        request_id: execution_request_id(
            account_id,
            s.security_epoch,
            Hash::new([1; 32]),
            sequence,
        ),
        expires_at: time(&f.ic) + MINUTE,
        signature: ByteBuf::new(),
    };
    approval.signature = key(1)
        .sign(
            approval_message(
                f.user,
                account_id,
                "dmsg/execute/v3",
                &(&kind, max_cycles),
                &approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .to_vec()
        .into();
    ExecuteRequest {
        account_id,
        kind,
        max_cycles,
        approval,
    }
}
fn root_user(f: &Fixture) -> AccountId {
    let account_id = f.create(1);
    f.recoverable(1, account_id);
    f.mutate(
        1,
        account_id,
        AccountCommand::ReserveRoot {
            expected_generation: 0,
            op_id: Hash::new([8; 32]),
        },
    )
    .unwrap();
    f.mutate(
        1,
        account_id,
        AccountCommand::CommitRoot {
            expected_generation: 0,
            op_id: Hash::new([8; 32]),
            root: ContentRootRef {
                generation: 1,
                suite: "dmsg-root-v1".into(),
                home_cose: f.cose,
                derivation_version: 2,
                key_generation: 1,
                bundle_digest: Hash::new([12; 32]),
                recovery_generation: 1,
            },
        },
    )
    .unwrap();
    let initialized: Result<candid::Reserved> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    initialized.unwrap();
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
    let account_id = f.create(1);
    f.recoverable(1, account_id);
    let public: Result<(SecuritySnapshot, std::collections::BTreeMap<Hash, Device>)> =
        query(&f.ic, f.user, person(9), "get_device_bundle", (account_id,));
    let snapshot = public.unwrap().0;
    assert_eq!(
        snapshot.recovery_signing_pub,
        Some(key(70).verifying_key().to_bytes().into())
    );
    let request = RecoveryRequest {
        op_id: Hash::new([41; 32]),
        new_auth: person(9),
        device: device(9),
        generation: snapshot.recovery_root_version,
        expires_at: time(&f.ic) + DAY + MINUTE,
    };
    let signature = key(70)
        .sign(
            digest(
                "dmsg/recovery-request/v1",
                &(f.user, account_id, snapshot.recovery_nonce, &request),
            )
            .as_slice(),
        )
        .to_bytes()
        .to_vec();
    let pop = key(9)
        .sign(digest("dmsg/recovery-device/v1", &(f.user, account_id, &request)).as_slice())
        .to_bytes()
        .to_vec();
    let submitted: Result<()> = update(
        &f.ic,
        f.user,
        person(9),
        "request_recovery",
        (
            account_id,
            request.clone(),
            ByteBuf::from(signature),
            ByteBuf::from(pop),
        ),
    );
    submitted.unwrap();
    f.mutate(
        1,
        account_id,
        AccountCommand::DisputeRecovery {
            op_id: request.op_id,
            dispute: Hash::new([42; 32]),
        },
    )
    .unwrap();
    let full: Result<AccountInfo> = query(&f.ic, f.user, person(9), "get_account", (account_id,));
    assert_eq!(full, Err(Error::AuthRequired));
    let unauthorized: Result<Option<PendingRecovery>> = query(
        &f.ic,
        f.user,
        person(99),
        "get_recovery_request",
        (account_id,),
    );
    assert_eq!(unauthorized, Err(Error::AuthRequired));
    let pending: Result<Option<PendingRecovery>> = query(
        &f.ic,
        f.user,
        person(9),
        "get_recovery_request",
        (account_id,),
    );
    let pending = pending.unwrap().unwrap();
    let certified: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(9),
        "security_snapshot_batch",
        (vec![account_id],),
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
            .sign(
                recovery_confirmation_message(
                    f.user,
                    account_id,
                    leaf.recovery_nonce,
                    &request,
                    &confirmation,
                )
                .as_slice(),
            )
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
        (account_id, changed, sig.clone()),
    );
    assert_eq!(tampered, Err(Error::IntegrityFailed));
    let accepted: Result<()> = update(
        &f.ic,
        f.user,
        person(9),
        "reconfirm_recovery",
        (account_id, confirmation.clone(), sig.clone()),
    );
    accepted.unwrap();
    f.mutate(
        1,
        account_id,
        AccountCommand::DisputeRecovery {
            op_id: request.op_id,
            dispute: Hash::new([43; 32]),
        },
    )
    .unwrap();
    let confirmed: Result<Option<PendingRecovery>> = query(
        &f.ic,
        f.user,
        person(9),
        "get_recovery_request",
        (account_id,),
    );
    let confirmed = confirmed.unwrap().unwrap();
    assert!(confirmed.execute_after > request.expires_at);
    let retry: Result<()> = update(
        &f.ic,
        f.user,
        person(9),
        "reconfirm_recovery",
        (account_id, confirmation, sig),
    );
    retry.unwrap();
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
        person(9),
        "get_recovery_request",
        (account_id,),
    );
    assert_eq!(restored.unwrap(), Some(confirmed.clone()));
    f.ic.advance_time(Duration::from_millis(confirmed.execute_after - time(&f.ic)));
    let completed: Result<()> =
        update(&f.ic, f.user, person(9), "complete_recovery", (account_id,));
    completed.unwrap();
    assert_eq!(f.account_id(9, account_id).auth_bindings, vec![person(9)]);
}

#[test]
fn invalid_transport_is_rejected_without_consuming_authorization() {
    let f = Fixture::new();
    let account_id = root_user(&f);
    let before = f.account_id(1, account_id);
    let bad = derivation_request(&f, account_id, vec![1; 48]);
    let result: Result<ExecutionResult> = submit_execution(&f.ic, f.user, person(1), bad);
    assert_eq!(result, Err(Error::IntegrityFailed));
    assert_eq!(f.account_id(1, account_id), before);
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32]).unwrap();
    assert_eq!(
        f.derive(1, account_id, transport.public_key()).status(),
        ExecutionStatus::Completed
    );
}

#[test]
fn cleaned_request_id_cannot_be_reapproved_for_another_operation() {
    let f = Fixture::new();
    let account_id = root_user(&f);
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32]).unwrap();
    let first = derivation_request(&f, account_id, transport.public_key());
    let result: Result<ExecutionResult> = submit_execution(&f.ic, f.user, person(1), first.clone());
    assert_eq!(result.unwrap().status(), ExecutionStatus::Completed);
    for _ in 0..65 {
        f.mutate(
            1,
            account_id,
            AccountCommand::SetPolicy {
                policy: SensitivePolicy::default(),
            },
        )
        .unwrap();
    }
    f.ic.advance_time(Duration::from_secs(2 * 24 * 60 * 60));
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![92; 32]).unwrap();
    f.derive(1, account_id, transport.public_key());
    let expired: Result<ExecutionResult> = query(
        &f.ic,
        f.cose,
        f.user,
        "get_execution",
        (account_id, first.approval.request_id),
    );
    assert_eq!(expired, Err(Error::NotFound));
    let mut reused = derivation_request(&f, account_id, transport.public_key());
    reused.approval.request_id = first.approval.request_id;
    reused.approval.signature = key(1)
        .sign(
            approval_message(
                f.user,
                account_id,
                "dmsg/execute/v3",
                &(&reused.kind, reused.max_cycles),
                &reused.approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .to_vec()
        .into();
    let refused: Result<ExecutionResult> = submit_execution(&f.ic, f.user, person(1), reused);
    assert_eq!(refused, Err(Error::IdempotencyConflict));
}

#[test]
fn valid_presigned_offer_survives_a_minute_but_current_revocation_still_applies() {
    let f = Fixture::new();
    let account_id = f.create(2);
    let mut old = f.order(account_id, 2, 1);
    old.offer.offer.expires_at = time(&f.ic) + DAY;
    old.offer.signature = key(2)
        .sign(digest("dmsg/payment-offer/v1", &old.offer.offer).as_slice())
        .to_bytes()
        .to_vec()
        .into();
    old.quote.offer_digest = digest("dmsg/payment-offer/v1", &old.offer.offer);
    old.quote_signature = key(50)
        .sign(digest("dmsg/quote/v1", &old.quote).as_slice())
        .to_bytes()
        .to_vec()
        .into();
    f.ic.advance_time(Duration::from_secs(61));
    let accepted: Result<EscrowInfo> = update(&f.ic, f.payment, person(40), "open_escrow", (old,));
    accepted.unwrap();
    let stale = f.order(account_id, 2, 2);
    f.mutate(
        2,
        account_id,
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
