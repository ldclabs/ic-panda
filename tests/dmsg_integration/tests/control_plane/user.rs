use super::*;

fn text_statement(f: &Fixture, id: &AccountId, text: &str) -> Statement {
    Statement {
        issuer: f.account_id(1, id).issuer,
        subject: None,
        issued_at: None,
        content: StatementContent::Text(text.into()),
    }
}

fn usage(f: &Fixture, id: &AccountId) -> dmsg_types::billing::ExecutionUsage {
    let month = dmsg_protocol::billing::month_utc(time(&f.ic)).unwrap();
    let usage: Result<dmsg_types::billing::ExecutionUsage> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_usage",
        (id, month),
    );
    usage.unwrap()
}

#[test]
fn attestations_charge_the_month_once_and_replay_the_stored_artifact() {
    let f = Fixture::new();
    let id = f.create(1);
    f.mutate(
        1,
        &id,
        AccountCommand::SetPolicy {
            policy: SensitivePolicy {
                daily_executions: 2,
                ..SensitivePolicy::default()
            },
        },
    )
    .unwrap();
    let first = f.attest_request(1, &id, text_statement(&f, &id, "approved statement"));
    let artifact: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest", (first.clone(),));
    let artifact = artifact.unwrap();
    assert_eq!(verify_artifact(&artifact).unwrap(), first.statement);
    let charged = usage(&f, &id);
    assert_eq!((charged.held_units, charged.charged_units), (0, 1));
    let account = f.account_id(1, &id);
    // A replay returns the artifact without charging or consuming a sequence.
    let replay: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest", (first.clone(),));
    assert_eq!(replay.unwrap(), artifact);
    assert_eq!(f.account_id(1, &id), account);
    assert_eq!(usage(&f, &id), charged);
    let stored: Result<SignedArtifact> = query(
        &f.ic,
        f.user,
        person(1),
        "get_attestation",
        (&id, first.approval.request_id),
    );
    assert_eq!(stored.unwrap(), artifact);
    let not_a_derivation: Result<ExecutionResult> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution",
        (&id, first.approval.request_id),
    );
    assert_eq!(not_a_derivation, Err(Error::UnsupportedProtocol));
    // A tampered replay under the same request ID is a conflict.
    let mut altered = first.clone();
    altered.origin = "https://other.test".into();
    let conflict: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest", (altered,));
    assert_eq!(conflict, Err(Error::IdempotencyConflict));
    // The account policy counts attestations per day.
    f.attest(1, &id, text_statement(&f, &id, "second")).unwrap();
    let before = f.account_id(1, &id);
    assert_eq!(
        f.attest(1, &id, text_statement(&f, &id, "third")),
        Err(Error::QuotaExceeded)
    );
    assert_eq!(f.account_id(1, &id), before);
    assert_eq!(usage(&f, &id).charged_units, 2);
    f.ic.advance_time(Duration::from_millis(DAY));
    f.attest(1, &id, text_statement(&f, &id, "tomorrow")).unwrap();
}

#[test]
fn fresh_handle_approval_renews_the_deadline() {
    let f = Fixture::new();
    let id = f.create(1);
    let intent = HandleIntent {
        handle_canister: f.handle,
        action: HandleAction::Register,
        account_id: id,
        target_account: None,
        handle: "probehandle".into(),
        expected_version: 0,
        op_id: Hash::new([80; 32]),
        terms_digest: Hash::new([81; 32]),
    };
    f.mutate(
        1,
        &id,
        AccountCommand::AuthorizeHandle {
            intent: intent.clone(),
        },
    )
    .unwrap();
    f.ic.advance_time(Duration::from_secs(30));
    f.mutate(
        1,
        &id,
        AccountCommand::AuthorizeHandle {
            intent: intent.clone(),
        },
    )
    .unwrap();
    f.ic.advance_time(Duration::from_secs(31));
    let result: Result<()> = update(
        &f.ic,
        f.user,
        f.handle,
        "consume_handle_authorization",
        (intent,),
    );
    assert_eq!(result, Ok(()));
}

#[test]
fn policy_changes_invalidate_approvals_without_rotating_content_roots() {
    let f = Fixture::new();
    let id = f.create(1);
    f.rekey(1, &id);
    let before = f.account_id(1, &id);
    assert_eq!(before.vault_write_state, VaultWriteState::Ready);
    let mut policy = before.sensitive_policy.clone();
    policy.daily_executions = 10;
    f.mutate(1, &id, AccountCommand::SetPolicy { policy })
        .unwrap();
    let after = f.account_id(1, &id);
    assert_eq!(after.vault_write_state, VaultWriteState::Ready);
    assert_eq!(after.current_root, before.current_root);
    assert_eq!(after.security_epoch, before.security_epoch + 1);
}

#[test]
fn pruned_attestations_keep_their_charge_and_leave_an_absence_proof() {
    let f = Fixture::new();
    let id = f.create(1);
    let request = f.attest_request(1, &id, text_statement(&f, &id, "approved statement"));
    let artifact: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest", (request.clone(),));
    artifact.unwrap();
    let before = usage(&f, &id);
    assert_eq!(before.charged_units, 1);
    let early: Result<u32> = update(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "prune_executions",
        (id,),
    );
    assert_eq!(early, Ok(0));
    f.ic.advance_time(Duration::from_millis(2 * DAY));
    let removed: Result<u32> = update(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "prune_executions",
        (id,),
    );
    assert_eq!(removed, Ok(1));
    assert_eq!(usage(&f, &id), before);
    let gone: Result<SignedArtifact> = query(
        &f.ic,
        f.user,
        person(1),
        "get_attestation",
        (&id, request.approval.request_id),
    );
    assert_eq!(gone, Err(Error::ResultExpired));
    let replay: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest", (request.clone(),));
    assert_eq!(replay, Err(Error::ResultExpired));
    for _ in 0..2 {
        let receipt: Result<CertifiedBatch> = query(
            &f.ic,
            f.user,
            person(1),
            "get_execution_receipt",
            (&id, request.approval.request_id),
        );
        let batch = receipt.unwrap();
        assert!(batch.entries[0].value.is_none());
        let witness: ic_certification::HashTree =
            cbor2::from_slice(&batch.entries[0].witness).unwrap();
        assert_eq!(
            witness.lookup_path([execution_receipt_key(&id, request.approval.request_id)]),
            ic_certification::LookupResult::Absent
        );
        f.ic.upgrade_canister(
            f.user,
            wasm("dmsg_user"),
            candid::encode_args(()).unwrap(),
            None,
        )
        .unwrap();
    }
    let month = dmsg_protocol::billing::month_utc(time(&f.ic)).unwrap();
    let certified: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_usage_certified",
        (&id, month),
    );
    let leaf: dmsg_types::billing::ExecutionUsage = cbor2::from_slice(&certified_value(
        &f,
        certified.unwrap(),
        dmsg_protocol::billing::usage_key(&id, month).as_slice(),
    ))
    .unwrap();
    assert_eq!(leaf, before);
}

#[test]
fn entitlement_callback_rechecks_a_concurrent_policy_change() {
    let f = Fixture::new();
    let id = f.create(1);
    let request = f.attest_request(1, &id, text_statement(&f, &id, "approved statement"));
    let call =
        f.ic.submit_call(
            f.user,
            person(1),
            "attest",
            candid::encode_one(request.clone()).unwrap(),
        )
        .unwrap();
    // Ingress with equal expiry is inducted in message-hash order; a later
    // expiry keeps the policy change behind the pending attestation.
    f.ic.advance_time(Duration::from_millis(1));
    // Queue a policy change before the first commercial lease callback can authorize the request.
    f.mutate(
        1,
        &id,
        AccountCommand::SetPolicy {
            policy: SensitivePolicy {
                frozen: true,
                ..SensitivePolicy::default()
            },
        },
    )
    .unwrap();
    let result: Result<SignedArtifact> =
        candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap();
    assert_eq!(result, Err(Error::Locked));
    let missing: Result<SignedArtifact> = query(
        &f.ic,
        f.user,
        person(1),
        "get_attestation",
        (&id, request.approval.request_id),
    );
    assert_eq!(missing, Err(Error::ResultExpired));
    let usage = usage(&f, &id);
    assert_eq!((usage.held_units, usage.charged_units), (0, 0));
}

// Small reproducible growth samples, not a production capacity certification.
#[test]
#[ignore = "storage and upgrade comparison for dmsg_user builds"]
fn user_upgrade_profile() {
    let f = Fixture::new();
    let mut accounts = vec![];
    for owner in 1..=64u8 {
        let id = f.create(owner);
        let usage: Result<dmsg_types::billing::ExecutionUsage> = update(
            &f.ic,
            f.user,
            person(owner),
            "refresh_execution_entitlement",
            (&id,),
        );
        usage.unwrap();
        accounts.push((owner, id));
        if [1, 16, 64].contains(&owner) {
            measure_user_upgrade(&f, &accounts, 1);
        }
    }
    for _ in 1..12 {
        f.ic.advance_time(Duration::from_millis(32 * DAY));
        for (owner, id) in &accounts {
            let usage: Result<dmsg_types::billing::ExecutionUsage> = update(
                &f.ic,
                f.user,
                person(*owner),
                "refresh_execution_entitlement",
                (id,),
            );
            usage.unwrap();
        }
    }
    measure_user_upgrade(&f, &accounts, 12);
}

pub(super) fn measure_user_upgrade(f: &Fixture, accounts: &[(u8, AccountId)], months: usize) {
    let before = f.ic.cycle_balance(f.user);
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let cycles = before - f.ic.cycle_balance(f.user);
    let memory = f.ic.canister_status(f.user, None).unwrap().memory_metrics;
    let stable_bytes = memory.stable_memory_size;
    let wasm_bytes = memory.wasm_memory_size;
    let month = dmsg_protocol::billing::month_utc(time(&f.ic)).unwrap();
    let (owner, id) = accounts.last().unwrap();
    let certified: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(*owner),
        "get_execution_usage_certified",
        (id, month),
    );
    let leaf: dmsg_types::billing::ExecutionUsage = cbor2::from_slice(&certified_value(
        f,
        certified.unwrap(),
        dmsg_protocol::billing::usage_key(id, month).as_slice(),
    ))
    .unwrap();
    assert_eq!(leaf.account_id, *id);
    assert_eq!(leaf.month_utc, month);
    println!(
        "user_upgrade accounts={} month_rows={} cycles={cycles} stable_bytes={stable_bytes} wasm_bytes={wasm_bytes}",
        accounts.len(),
        accounts.len() * months
    );
    if let Some(log) =
        f.ic.fetch_canister_logs(f.user, Principal::anonymous())
            .unwrap()
            .last()
    {
        println!("{}", String::from_utf8_lossy(&log.content));
    }
}

#[test]
fn removed_and_recovered_logins_lose_their_routes() {
    let f = Fixture::new();
    let id = f.create(1);
    let bind = |n: u8, nonce: u8| {
        let nonce = Hash::new([nonce; 32]);
        let begun: Result<()> = update(
            &f.ic,
            f.user,
            person(n),
            "begin_auth_binding",
            (&id, nonce, time(&f.ic) + MINUTE),
        );
        begun.unwrap();
        f.mutate(
            1,
            &id,
            AccountCommand::BindAuth {
                principal: person(n),
                nonce,
            },
        )
        .unwrap();
    };
    bind(2, 21);
    let routed: Option<AccountId> = query(&f.ic, f.user, person(2), "my_account", ());
    assert_eq!(routed, Some(id));
    let same: Result<AccountId> = update(
        &f.ic,
        f.user,
        person(2),
        "create_account",
        (f.create_input(2),),
    );
    assert_eq!(same, Ok(id));
    f.mutate(
        1,
        &id,
        AccountCommand::RemoveAuth {
            principal: person(2),
        },
    )
    .unwrap();
    let unrouted: Option<AccountId> = query(&f.ic, f.user, person(2), "my_account", ());
    assert_eq!(unrouted, None);
    let denied: Result<AccountInfo> = query(&f.ic, f.user, person(2), "get_account", (&id,));
    assert_eq!(denied, Err(Error::AuthRequired));
    assert_ne!(f.create(2), id);

    // An unbound login cannot start a takeover; a bound one replaces every
    // other binding and drops their routes.
    bind(3, 31);
    let request = RecoveryRequest {
        op_id: Hash::new([41; 32]),
        new_auth: person(9),
        device: device(9),
        expires_at: time(&f.ic) + 4 * DAY,
    };
    let pop = ByteBuf::from(
        key(9)
            .sign(recovery_device_message(f.user, &id, &request).as_slice())
            .to_bytes()
            .to_vec(),
    );
    let refused: Result<()> = update(
        &f.ic,
        f.user,
        person(9),
        "request_recovery",
        (&id, request, pop),
    );
    assert_eq!(refused, Err(Error::AuthRequired));
    f.recover(1, 9, &id);
    assert_eq!(f.account_id(1, &id).auth_bindings, vec![person(1)]);
    let recovered: Option<AccountId> = query(&f.ic, f.user, person(1), "my_account", ());
    assert_eq!(recovered, Some(id));
    let cut: Option<AccountId> = query(&f.ic, f.user, person(3), "my_account", ());
    assert_eq!(cut, None);
    assert_ne!(f.create(3), id);
}

#[test]
fn reconcile_transport_failures_do_not_rewrite_the_execution() {
    let f = Fixture::new();
    let id = f.root_account(1);
    f.recover(1, 9, &id);
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32]).unwrap();
    let request = f.derive_request(1, 9, &id, transport.public_key());
    // Uninitialized COSE keys reject the grant before recording anything.
    let rejected: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "derive_root", (request.clone(),));
    assert!(
        matches!(rejected, Err(Error::Unavailable(_))),
        "{rejected:?}"
    );
    let authorized: Result<ExecutionResult> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution",
        (&id, request.approval.request_id),
    );
    assert_eq!(
        authorized.as_ref().unwrap().outcome,
        ExecutionOutcome::Authorized
    );
    f.ic.stop_canister(f.cose, None).unwrap();
    let failed: Result<ExecutionResult> = update(
        &f.ic,
        f.user,
        person(1),
        "reconcile_execution",
        (&id, request.approval.request_id),
    );
    assert!(failed.is_err(), "{failed:?}");
    let unchanged: Result<ExecutionResult> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution",
        (&id, request.approval.request_id),
    );
    assert_eq!(unchanged, authorized);
    f.ic.start_canister(f.cose, None).unwrap();
    let redispatched: Result<ExecutionResult> = update(
        &f.ic,
        f.user,
        person(1),
        "reconcile_execution",
        (&id, request.approval.request_id),
    );
    assert!(
        matches!(redispatched, Err(Error::Unavailable(_))),
        "{redispatched:?}"
    );
    // Once the keys are ready the same grant completes.
    let initialized: Result<KeyState> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    initialized.unwrap();
    let completed: Result<ExecutionResult> = update(
        &f.ic,
        f.user,
        person(1),
        "reconcile_execution",
        (&id, request.approval.request_id),
    );
    assert_eq!(completed.unwrap().status(), ExecutionStatus::Completed);
}
