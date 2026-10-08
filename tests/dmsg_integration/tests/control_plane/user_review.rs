use super::*;

#[test]
fn untrusted_service_is_rejected_before_commerce() {
    use dmsg_types::{integration::*, membership::Beneficiary};
    let f = Fixture::new();
    let mut expected = ApplicationApproval {
        version: 1,
        environment: Environment::Local,
        app_id: "sample".into(),
        app_config_version: 1,
        origin: "https://sample.test".into(),
        approving_account: AccountId([1; 12]),
        service: f.commerce,
        beneficiary: Beneficiary {
            authority_canister: f.user,
            product_id: "sample".into(),
            subject_schema: "dmsg-account-v1".into(),
            subject_bytes: vec![1; 12].into(),
        },
        actor: person(1),
        purpose: ApprovalPurpose::CashCheckout,
        action_digest: Hash::new([1; 32]),
        operation_id: Hash::new([2; 32]),
        nonce: Hash::new([3; 32]),
        expires_at_ms: time(&f.ic) + MINUTE,
    };
    f.ic.stop_canister(f.commerce, None).unwrap();
    // Ingress from outside the configured services never runs.
    assert_refused(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "verify_application_authorization",
        (Hash::new([4; 32]), expected.clone()),
    );
    // A configured service other than the purpose's is refused before commerce.
    let denied: Result<ApplicationAuthorization> = update(
        &f.ic,
        f.user,
        f.membership,
        "verify_application_authorization",
        (Hash::new([4; 32]), expected.clone()),
    );
    assert_eq!(denied, Err(Error::Forbidden));
    expected.service = Principal::anonymous();
    let reached: Result<ApplicationAuthorization> = update(
        &f.ic,
        f.user,
        f.commerce,
        "verify_application_authorization",
        (Hash::new([4; 32]), expected),
    );
    assert_eq!(reached, Err(Error::Forbidden));
}

#[test]
fn inspect_refuses_product_ingress_and_anonymous_accounts_but_not_maintenance() {
    let f = Fixture::new();
    let beneficiary = dmsg_types::membership::Beneficiary {
        authority_canister: f.user,
        product_id: "sample".into(),
        subject_schema: "dmsg-account-v1".into(),
        subject_bytes: vec![1; 12].into(),
    };
    for caller in [person(1), f.commerce] {
        assert_refused(
            &f.ic,
            f.user,
            caller,
            "verify_product_account",
            ("sample".to_string(), beneficiary.clone()),
        );
    }
    assert_refused(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "create_account",
        (f.create_input(1),),
    );
    f.create(1);
    let pruned: Option<ByteBuf> = update(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "prune_executions",
        (ByteBuf::new(),),
    );
    assert_eq!(pruned, None);
}

#[test]
fn recovery_retries_preserve_pending_and_completed_results() {
    let f = Fixture::new();
    let id = f.create(1);
    let s = f.account_id(1, &id);
    let at = time(&f.ic);
    let request = RecoveryRequest {
        op_id: Hash::new([77; 32]),
        new_auth: person(1),
        device: device(4),
        expires_at: at + s.recovery_delay_ms + 2 * DAY,
    };
    let proof: ByteBuf = key(4)
        .sign(recovery_device_message(f.user, &id, &request).as_slice())
        .to_bytes()
        .to_vec()
        .into();
    let args = (id, request.clone(), proof);
    let first: Result<()> = update(&f.ic, f.user, person(1), "request_recovery", args.clone());
    first.unwrap();
    f.ic.advance_time(Duration::from_millis(s.recovery_delay_ms + MINUTE));
    let retry: Result<()> = update(&f.ic, f.user, person(1), "request_recovery", args);
    assert_eq!(retry, Ok(()));
    let complete: Result<()> = update(
        &f.ic,
        f.user,
        person(1),
        "complete_recovery",
        (id, request.op_id),
    );
    complete.unwrap();
    let before = f.account_id(1, &id);
    // No root was committed, so the recovered device has nothing to derive.
    assert_eq!(before.recovered_device, None);
    assert_eq!(before.devices.keys().copied().collect::<Vec<_>>(), vec![Hash::new([4; 32])]);
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let retry: Result<()> = update(
        &f.ic,
        f.user,
        person(1),
        "complete_recovery",
        (id, request.op_id),
    );
    assert_eq!(retry, Ok(()));
    assert_eq!(f.account_id(1, &id), before);
    let wrong: Result<()> = update(
        &f.ic,
        f.user,
        person(5),
        "complete_recovery",
        (id, request.op_id),
    );
    assert_eq!(wrong, Err(Error::AuthRequired));
    assert_eq!(f.account_id(1, &id).auth_bindings, vec![person(1)]);
}

#[test]
fn a_login_binds_only_by_accepting_an_administrator_approval() {
    let f = Fixture::new();
    let id = f.create(1);
    let nonce = Hash::new([5; 32]);
    let accept = |n: u8, nonce: Hash| -> Result<()> {
        update(&f.ic, f.user, person(n), "accept_auth_binding", (id, nonce))
    };
    // Nobody but an administrator device can stage a binding.
    assert_eq!(accept(5, nonce), Err(Error::NotFound));
    let version = f.account_id(1, &id).account_version;
    f.mutate(
        1,
        &id,
        AccountCommand::BindAuth {
            principal: person(5),
            nonce,
        },
    )
    .unwrap();
    // The approval alone binds nothing.
    let unbound: Option<AccountId> = query(&f.ic, f.user, person(5), "my_account", ());
    assert_eq!(unbound, None);
    let epoch = f.account_id(1, &id).security_epoch;
    assert_eq!(accept(6, nonce), Err(Error::NotFound));
    assert_eq!(accept(5, Hash::new([6; 32])), Err(Error::NotFound));
    assert_eq!(accept(5, nonce), Ok(()));
    assert_eq!(accept(5, nonce), Ok(()));
    let routed: Option<AccountId> = query(&f.ic, f.user, person(5), "my_account", ());
    assert_eq!(routed, Some(id));
    let info = f.account_id(5, &id);
    assert!(info.auth_bindings.contains(&person(5)));
    assert_eq!(
        (info.account_version, info.security_epoch),
        (version + 2, epoch + 1)
    );

    // A login that already routes to another account cannot accept.
    let other = f.create(7);
    f.mutate(
        1,
        &id,
        AccountCommand::BindAuth {
            principal: person(7),
            nonce,
        },
    )
    .unwrap();
    assert_eq!(accept(7, nonce), Err(Error::IdempotencyConflict));
    let kept: Option<AccountId> = query(&f.ic, f.user, person(7), "my_account", ());
    assert_eq!(kept, Some(other));

    // An approval lapses after ten minutes.
    f.mutate(
        1,
        &id,
        AccountCommand::BindAuth {
            principal: person(8),
            nonce,
        },
    )
    .unwrap();
    f.ic.advance_time(Duration::from_millis(BINDING_ACCEPT_MS));
    assert_eq!(accept(8, nonce), Err(Error::NotFound));
}

#[test]
fn admission_tickets_gate_new_accounts_when_governance_sets_a_key() {
    let f = Fixture::new();
    let issuer = key(60);
    let issuer_key: Hash = issuer.verifying_key().to_bytes().into();
    let validate = |key: Option<Hash>| -> std::result::Result<String, String> {
        query(
            &f.ic,
            f.user,
            Principal::anonymous(),
            "validate_admin_set_admission_key",
            (key,),
        )
    };
    assert!(validate(Some(issuer_key))
        .unwrap()
        .contains("none (open admission)"));
    assert!(validate(Some(Hash::new([0; 32]))).is_err());
    assert_refused(
        &f.ic,
        f.user,
        person(1),
        "admin_set_admission_key",
        (Some(issuer_key),),
    );
    let set: Result<()> = update(
        &f.ic,
        f.user,
        f.sns,
        "admin_set_admission_key",
        (Some(issuer_key),),
    );
    set.unwrap();
    let config: UserInit = query(&f.ic, f.user, Principal::anonymous(), "user_config", ());
    assert_eq!(config.admission_key, Some(issuer_key));
    let ticket = |home: Principal, caller: Principal, expires_at: u64| AdmissionTicket {
        expires_at,
        signature: issuer
            .sign(account_admission_message(home, caller, expires_at).as_slice())
            .to_bytes()
            .into(),
    };
    let create = |n: u8, admission: Option<AdmissionTicket>| -> Result<AccountId> {
        let input = CreateAccount {
            admission,
            ..f.create_input(n)
        };
        update(&f.ic, f.user, person(n), "create_account", (input,))
    };
    let deadline = time(&f.ic) + 5 * MINUTE;
    assert_eq!(create(1, None), Err(Error::Forbidden));
    assert_eq!(
        create(1, Some(ticket(f.user, person(2), deadline))),
        Err(Error::IntegrityFailed)
    );
    assert_eq!(
        create(
            1,
            Some(ticket(f.user, person(1), time(&f.ic) + 11 * MINUTE))
        ),
        Err(Error::Expired)
    );
    let stats: UserStats = query(&f.ic, f.user, Principal::anonymous(), "user_stats", ());
    assert_eq!((stats.accounts, stats.created_today), (0, 0));
    let id = create(1, Some(ticket(f.user, person(1), deadline))).unwrap();
    // The route makes a retry return the account without another ticket.
    assert_eq!(create(1, None), Ok(id));
    // Clearing the key reopens creation to any authenticated caller.
    let cleared: Result<()> = update(
        &f.ic,
        f.user,
        f.sns,
        "admin_set_admission_key",
        (None::<Hash>,),
    );
    cleared.unwrap();
    assert!(create(2, None).is_ok());
}

#[test]
fn public_cleanup_pages_through_every_account() {
    let f = Fixture::new();
    let statement = |n: u8, id: &AccountId, text: &str| Statement {
        issuer: f.account_id(n, id).issuer,
        subject: None,
        issued_at: None,
        content: StatementContent::Text(text.into()),
    };
    let mut requests = vec![];
    for n in 1..=3u8 {
        let id = f.create(n);
        for i in 0..2 {
            let request = f.attest_request(n, &id, statement(n, &id, &format!("s{n}-{i}")));
            let done: Result<SignedArtifact> =
                update(&f.ic, f.user, person(n), "attest", (request.clone(),));
            done.unwrap();
            requests.push((n, id, request.approval.request_id));
        }
    }
    let present = |n: u8, id: &AccountId, request: Hash| {
        query::<_, Result<SignedArtifact>>(
            &f.ic,
            f.user,
            person(n),
            "get_attestation",
            (id, request),
        )
        .is_ok()
    };
    f.prune_executions();
    assert!(requests.iter().all(|(n, id, r)| present(*n, id, *r)));
    f.ic.advance_time(Duration::from_millis(2 * DAY));
    // Six records of three accounts fit in one page; the next returns None.
    assert_eq!(f.prune_executions(), 2);
    assert!(requests.iter().all(|(n, id, r)| !present(*n, id, *r)));
    assert_eq!(f.prune_executions(), 1);
}

#[test]
fn logins_start_at_most_the_hourly_remote_calls() {
    let f = Fixture::new();
    let id = f.create(1);
    let refresh = || -> Result<dmsg_types::billing::ExecutionUsage> {
        update(
            &f.ic,
            f.user,
            person(1),
            "refresh_execution_entitlement",
            (id,),
        )
    };
    // Start on an hour boundary so the whole budget falls in one hour.
    let hour = 60 * MINUTE;
    f.ic.advance_time(Duration::from_millis(hour - time(&f.ic) % hour));
    for _ in 0..MAX_HOURLY_ACCOUNT_CALLS {
        refresh().unwrap();
    }
    assert_eq!(refresh(), Err(Error::QuotaExceeded));
    // Another account's budget is separate.
    let other = f.create(2);
    let theirs: Result<dmsg_types::billing::ExecutionUsage> = update(
        &f.ic,
        f.user,
        person(2),
        "refresh_execution_entitlement",
        (other,),
    );
    assert!(theirs.is_ok());
    f.ic.advance_time(Duration::from_millis(hour));
    assert!(refresh().is_ok());
}

#[test]
fn a_snapshot_restores_accounts_and_their_certified_state() {
    let f = Fixture::new();
    let id = f.create(1);
    let controller = f.ic.get_controllers(f.user)[0];
    f.ic.stop_canister(f.user, Some(controller)).unwrap();
    let snapshot =
        f.ic.take_canister_snapshot(f.user, Some(controller), None)
            .unwrap();
    f.ic.start_canister(f.user, Some(controller)).unwrap();
    let before = f.account_id(1, &id);
    f.mutate(
        1,
        &id,
        AccountCommand::SetRecoveryDelay { delay_ms: 2 * DAY },
    )
    .unwrap();
    let later = f.create(2);
    // Roll back: stop, load the snapshot, start.
    f.ic.stop_canister(f.user, Some(controller)).unwrap();
    f.ic.load_canister_snapshot(f.user, Some(controller), snapshot.id)
        .unwrap();
    f.ic.start_canister(f.user, Some(controller)).unwrap();
    assert_eq!(f.account_id(1, &id), before);
    let gone: Option<AccountId> = query(&f.ic, f.user, person(2), "my_account", ());
    assert_eq!(gone, None);
    // The restored certification root matches the restored records.
    let batch: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "security_snapshot_batch",
        (vec![id],),
    );
    let leaf = certified_value(&f, batch.unwrap(), id.as_slice());
    let snapshot: SecuritySnapshot = cbor2::from_slice(&leaf).unwrap();
    assert_eq!(snapshot.account_version, before.account_version);
    assert_eq!(snapshot.recovery_delay_ms, before.recovery_delay_ms);
    // IDs start with the allocation second, so an account created after the
    // rollback never reuses an ID handed out before it.
    f.ic.advance_time(Duration::from_secs(1));
    assert_ne!(f.create(2), later);
}

#[test]
fn independent_cleanup_preserves_pending_executions_billing_and_replay_guards() {
    let f = Fixture::new();
    let id = f.root_account(1);
    let statement = |text: &str| Statement {
        issuer: f.account_id(1, &id).issuer,
        subject: None,
        issued_at: None,
        content: StatementContent::Text(text.into()),
    };
    // Recovery moves the clock past the delay; attest afterwards so the
    // retention deadlines below are the ones being tested.
    f.recover(1, 9, &id);
    let signed = f.attest_request_by(1, 9, &id, statement("first"));
    let done: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest", (signed.clone(),));
    done.unwrap();
    f.attest_by(1, 9, &id, statement("second")).unwrap();
    // A derivation stays pending while COSE is unreachable.
    f.ic.stop_canister(f.cose, None).unwrap();
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32]).unwrap();
    let pending = f.derive_request(1, 9, &id, transport.public_key());
    let _: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "derive_root", (pending.clone(),));
    let pending_result: Result<ExecutionResult> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution",
        (id, pending.approval.request_id),
    );
    assert!(!pending_result.as_ref().unwrap().is_terminal());
    let month = dmsg_protocol::billing::month_utc(time(&f.ic)).unwrap();
    let usage = || {
        query::<_, Result<dmsg_types::billing::ExecutionUsage>>(
            &f.ic,
            f.user,
            person(1),
            "get_execution_usage",
            (id, month),
        )
        .unwrap()
    };
    let before_usage = usage();
    assert_eq!(before_usage.charged_units, 2);
    let before_account = f.account_id(1, &id);
    let attested = |request: Hash| {
        query::<_, Result<SignedArtifact>>(
            &f.ic,
            f.user,
            person(1),
            "get_attestation",
            (id, request),
        )
        .is_ok()
    };
    f.prune_executions();
    assert!(attested(signed.approval.request_id));
    f.ic.advance_time(Duration::from_millis(2 * DAY));
    f.prune_executions();
    assert!(!attested(signed.approval.request_id));
    f.prune_executions();
    assert_eq!(f.account_id(1, &id), before_account);
    assert_eq!(usage(), before_usage);
    let current: Result<ExecutionResult> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution",
        (id, pending.approval.request_id),
    );
    assert_eq!(current, pending_result);
    let replay: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest", (signed.clone(),));
    assert_eq!(replay, Err(Error::ResultExpired));
    for _ in 0..2 {
        let receipt: Result<CertifiedBatch> = query(
            &f.ic,
            f.user,
            person(1),
            "get_execution_receipt",
            (id, signed.approval.request_id),
        );
        let batch = receipt.unwrap();
        assert!(batch.entries[0].value.is_none());
        let witness: ic_certification::HashTree =
            cbor2::from_slice(&batch.entries[0].witness).unwrap();
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
            witness.lookup_path([execution_receipt_key(&id, signed.approval.request_id)]),
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
}
