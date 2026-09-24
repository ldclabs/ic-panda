use super::*;

fn seal_snapshot(f: &Fixture, entries: &[LegacyReservation]) -> LegacySnapshot {
    let snapshot = LegacySnapshot {
        source_canister: person(80),
        snapshot_id: Hash::new([5; 32]),
        freeze_version: 1,
        event_tip: Hash::new([7; 32]),
        count: entries.len() as u64,
        entries_digest: entries.iter().fold(Hash::new([0; 32]), |tip, entry| {
            digest("dmsg/legacy-entry/v1", &(tip, entry))
        }),
    };
    let started: Result<()> = update(
        &f.ic,
        f.handle,
        Principal::anonymous(),
        "begin_legacy_snapshot",
        (snapshot.clone(),),
    );
    started.unwrap();
    for chunk in entries.chunks(256) {
        let imported: Result<SnapshotProgress> = update(
            &f.ic,
            f.handle,
            Principal::anonymous(),
            "import_legacy_handles",
            (snapshot.snapshot_id, chunk),
        );
        assert!(imported.unwrap().imported <= snapshot.count);
    }
    let sealed: Result<SnapshotProgress> = update(
        &f.ic,
        f.handle,
        Principal::anonymous(),
        "seal_legacy_snapshot",
        (),
    );
    assert!(sealed.unwrap().sealed);
    snapshot
}

fn registration(f: &Fixture, owner: &AccountId, name: &str, nonce: u8) -> Registration {
    let payer = account(person(1));
    Registration {
        intent: HandleIntent {
            handle_canister: f.handle,
            action: HandleAction::Register,
            account_id: *owner,
            target_account: None,
            handle: name.into(),
            expected_version: 0,
            op_id: Hash::new([nonce; 32]),
            terms_digest: charge_terms_digest(f.ledger, &payer, price(name) - 10, 10),
        },
        payer,
        fee: 10,
    }
}

fn authorize(f: &Fixture, n: u8, intent: &HandleIntent) {
    f.mutate(
        n,
        &intent.account_id,
        AccountCommand::AuthorizeHandle {
            intent: intent.clone(),
        },
    )
    .unwrap();
}

fn register(f: &Fixture, input: &Registration) -> Result<HandleOperation> {
    update(&f.ic, f.handle, person(1), "register_handle", (input,))
}

// Retries an unknown charge with the original ledger arguments.
fn commit(f: &Fixture, input: &Registration) -> Result<HandleOperation> {
    update(
        &f.ic,
        f.handle,
        person(1),
        "commit_handle",
        (&input.intent.account_id, input.intent.op_id),
    )
}

fn measure<T>(f: &Fixture, history: usize, method: &str, run: impl FnOnce() -> T) -> T {
    let before = f.ic.cycle_balance(f.handle);
    let result = run();
    println!(
        "handle_cycles history={history} method={method} cycles={}",
        before - f.ic.cycle_balance(f.handle)
    );
    result
}

fn operation(f: &Fixture, input: &Registration) -> Result<HandleOperation> {
    query(
        &f.ic,
        f.handle,
        person(1),
        "get_handle_operation",
        (&input.intent.account_id, input.intent.op_id),
    )
}

fn legacy(f: &Fixture, name: &str) -> Result<Option<LegacyReservation>> {
    query(
        &f.ic,
        f.handle,
        person(1),
        "get_legacy_reservation",
        (name,),
    )
}

fn progress(f: &Fixture) -> SnapshotProgress {
    query(&f.ic, f.handle, person(1), "snapshot_progress", ())
}

fn with_max_pending(f: &Fixture, max_pending: u32) {
    f.ic.reinstall_canister(
        f.handle,
        wasm("dmsg_handle"),
        candid::encode_args((HandleInit {
            home_user: f.user,
            ledger: f.ledger,
            ledger_fee: 10,
            max_pending,
        },))
        .unwrap(),
        None,
    )
    .unwrap();
}

fn upgrade(f: &Fixture) {
    f.ic.upgrade_canister(
        f.handle,
        wasm("dmsg_handle"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
}

fn verify_names(f: &Fixture, expected: &[HandleRecord]) {
    let mut handles: Vec<_> = expected.iter().map(|r| r.handle.to_uppercase()).collect();
    handles.push("missing".into());
    let batch: Result<CertifiedBatch> = query(
        &f.ic,
        f.handle,
        person(99),
        "resolve_handle_certified",
        (handles,),
    );
    let batch = batch.unwrap();
    assert_eq!(batch.canister, f.handle);
    assert_eq!(batch.entries.len(), expected.len() + 1);
    let cert: ic_certification::Certificate = cbor2::from_slice(&batch.certificate).unwrap();
    for (index, entry) in batch.entries.iter().enumerate() {
        let witness: ic_certification::HashTree = cbor2::from_slice(&entry.witness).unwrap();
        assert_eq!(
            cert.tree.lookup_path([
                b"canister".as_slice(),
                f.handle.as_slice(),
                b"certified_data".as_slice(),
            ]),
            ic_certification::LookupResult::Found(&witness.digest())
        );
        if let Some(record) = expected.get(index) {
            assert_eq!(entry.key.as_ref(), record.handle.as_bytes());
            assert_eq!(entry.value.as_ref().unwrap().as_ref(), canonical(record));
            assert_eq!(
                witness.lookup_path([entry.key.as_ref()]),
                ic_certification::LookupResult::Found(entry.value.as_ref().unwrap())
            );
        } else {
            assert!(entry.value.is_none());
            assert_eq!(
                witness.lookup_path([b"missing".as_slice()]),
                ic_certification::LookupResult::Absent
            );
        }
    }
}

#[test]
fn handle_snapshot_import_validates_the_whole_batch_before_writing() {
    let f = Fixture::new();
    let entries: Vec<_> = ["alpha", "beta"]
        .into_iter()
        .map(|name| LegacyReservation {
            handle: name.into(),
            legacy_owner: person(1),
            legacy_name_principal: None,
            frozen_admins: vec![],
            quarantined: false,
        })
        .collect();
    let snapshot = LegacySnapshot {
        source_canister: person(80),
        snapshot_id: Hash::new([5; 32]),
        freeze_version: 1,
        event_tip: Hash::new([7; 32]),
        count: 2,
        entries_digest: entries.iter().fold(Hash::new([0; 32]), |tip, entry| {
            digest("dmsg/legacy-entry/v1", &(tip, entry))
        }),
    };
    let started: Result<()> = update(
        &f.ic,
        f.handle,
        Principal::anonymous(),
        "begin_legacy_snapshot",
        (snapshot.clone(),),
    );
    started.unwrap();
    let import = |entries: Vec<LegacyReservation>| -> Result<SnapshotProgress> {
        update(
            &f.ic,
            f.handle,
            Principal::anonymous(),
            "import_legacy_handles",
            (snapshot.snapshot_id, entries),
        )
    };
    assert!(import(vec![entries[1].clone(), entries[0].clone()]).is_err());
    // A frozen administrator claims as a caller, so it must be authenticated.
    let mut anonymous_admin = entries[0].clone();
    anonymous_admin.frozen_admins = vec![Principal::anonymous()];
    assert_eq!(import(vec![anonymous_admin]), Err(Error::AuthRequired));
    assert_eq!(legacy(&f, "alpha"), Ok(None));
    assert_eq!(progress(&f).imported, 0);
    assert_eq!(import(vec![entries[0].clone()]).unwrap().imported, 1);
    let replay = import(vec![entries[0].clone()]).unwrap();
    assert_eq!(replay.imported, 1);
    let mut conflict = entries[0].clone();
    conflict.legacy_owner = person(2);
    assert!(matches!(
        import(vec![conflict]),
        Err(Error::IdempotencyConflict)
    ));
    // Retry an overlap and append the suffix exactly once.
    let complete = import(entries.clone()).unwrap();
    assert_eq!(complete.imported, 2);
    assert_eq!(complete.rolling_digest, snapshot.entries_digest);
    upgrade(&f);
    assert_eq!(import(entries.clone()).unwrap().imported, 2);
    let sealed: Result<SnapshotProgress> = update(
        &f.ic,
        f.handle,
        Principal::anonymous(),
        "seal_legacy_snapshot",
        (),
    );
    assert!(sealed.unwrap().sealed);
    assert!(matches!(import(entries), Err(Error::VersionConflict)));
}

#[test]
fn handle_unknown_charges_keep_locks_across_time_and_upgrade() {
    let f = Fixture::new();
    // Exercise the global counter as well as the per-account set at capacity.
    with_max_pending(&f, 1);
    let owner = f.create(1);
    let other = f.create(2);
    seal_snapshot(&f, &[]);
    f.mint(person(1), price("first") + price("rejected"));
    f.approve_handle(person(1), price("first"));
    let unknown = registration(&f, &owner, "first", 1);
    authorize(&f, 1, &unknown.intent);
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "lose_next_response",
        (),
    );
    assert_eq!(register(&f, &unknown), Err(Error::ExecutionUnknown));
    let pending = operation(&f, &unknown).unwrap();
    assert_eq!(pending.phase, HandlePhase::ChargeUnknown);
    let same_owner = registration(&f, &owner, "second", 2);
    let other_owner = registration(&f, &other, "other", 3);
    // No authorization exists for these requests: the quota must reject them
    // locally, before user calls that would instead return NotFound.
    for input in [&same_owner, &other_owner] {
        assert_eq!(register(&f, input), Err(Error::QuotaExceeded));
        assert_eq!(operation(&f, input), Err(Error::NotFound));
    }
    assert_eq!(register(&f, &unknown).unwrap(), pending);
    f.ic.advance_time(Duration::from_secs(16 * 60));
    upgrade(&f);
    assert_eq!(register(&f, &other_owner), Err(Error::QuotaExceeded));
    let paid = commit(&f, &unknown).unwrap(); // ledger Duplicate, original timestamp
    assert_eq!(paid.phase, HandlePhase::Committed);
    assert_eq!(paid.ledger_block, Some(0));
    assert_eq!(commit(&f, &unknown).unwrap(), paid);
    assert_eq!(register(&f, &unknown).unwrap(), paid);
    let balance: Nat = query(
        &f.ic,
        f.ledger,
        person(1),
        "icrc1_balance_of",
        (unknown.payer,),
    );
    assert_eq!(balance, Nat::from(price("rejected")));

    // A definitive failure releases both locks exactly once and appends no event.
    let rejected = registration(&f, &owner, "rejected", 4);
    authorize(&f, 1, &rejected.intent);
    assert!(
        matches!(register(&f, &rejected), Err(Error::Unavailable(ref reason)) if reason.contains("allowance"))
    );
    let failed = operation(&f, &rejected).unwrap();
    assert!(matches!(failed.phase, HandlePhase::Rejected { .. }));
    assert_eq!(register(&f, &rejected).unwrap(), failed);
    assert_eq!(commit(&f, &rejected), Err(Error::VersionConflict));
    let absent: Option<HandleEvent> =
        query(&f.ic, f.handle, person(1), "get_handle_event", (1u64,));
    assert_eq!(absent, None);
    f.approve_handle(person(1), price("rejected"));
    let next = registration(&f, &other, "rejected", 5);
    authorize(&f, 2, &next.intent);
    assert_eq!(register(&f, &next).unwrap().phase, HandlePhase::Committed);
}

#[test]
fn handle_concurrent_registrations_charge_once_and_rebuild_certificates() {
    let f = Fixture::new();
    let owner = f.create(1);
    let snapshot = seal_snapshot(&f, &[]);
    let total = price("alpha") + price("beta") + price("gamma");
    f.mint(person(1), total);
    f.approve_handle(person(1), total);
    let inputs = [
        registration(&f, &owner, "alpha", 1),
        registration(&f, &owner, "beta", 2),
    ];
    for input in &inputs {
        authorize(&f, 1, &input.intent);
    }
    let submit = |input: &Registration| {
        f.ic.submit_call(
            f.handle,
            person(1),
            "register_handle",
            candid::encode_args((input,)).unwrap(),
        )
        .unwrap()
    };
    let calls: Vec<_> = inputs.iter().map(submit).collect();
    let results: Vec<Result<HandleOperation>> = calls
        .into_iter()
        .map(|call| candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap())
        .collect();
    // An account holds at most one unresolved charge.
    assert_eq!(
        results
            .iter()
            .filter(|r| r.as_ref().is_ok_and(|o| o.phase == HandlePhase::Committed))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(Error::QuotaExceeded))
            .count(),
        1
    );
    let winner = results.iter().position(|r| r.is_ok()).unwrap();
    assert_eq!(
        register(&f, &inputs[winner]).unwrap().phase,
        HandlePhase::Committed
    );
    assert_eq!(
        register(&f, &inputs[1 - winner]).unwrap().phase,
        HandlePhase::Committed
    );
    let mut records = vec![];
    let mut previous = Hash::new([0; 32]);
    for sequence in 0..2u64 {
        let event: Option<HandleEvent> =
            query(&f.ic, f.handle, person(1), "get_handle_event", (sequence,));
        let event = event.unwrap();
        assert_eq!(event.sequence, sequence);
        assert_eq!(event.previous, previous);
        assert_eq!(event.legacy_snapshot, snapshot.snapshot_id);
        assert_eq!(event.to, owner);
        assert_eq!(event.version, 1);
        previous = digest("dmsg/handle-event/v1", &event);
        records.push(HandleRecord {
            handle: event.handle,
            owner_account: owner,
            version: 1,
            event_tip: previous,
        });
    }
    verify_names(&f, &records);
    upgrade(&f);
    verify_names(&f, &records);
    let event: Option<HandleEvent> = query(&f.ic, f.handle, person(1), "get_handle_event", (1u64,));
    assert_eq!(digest("dmsg/handle-event/v1", &event.unwrap()), previous);
    let absent: Option<HandleEvent> =
        query(&f.ic, f.handle, person(1), "get_handle_event", (2u64,));
    assert_eq!(absent, None);
    // The same request submitted twice charges once, and the log's own
    // persisted length supplies the next sequence after upgrade.
    let next = registration(&f, &owner, "gamma", 3);
    authorize(&f, 1, &next.intent);
    let calls = [submit(&next), submit(&next)];
    for call in calls {
        let result: Result<HandleOperation> =
            candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap();
        assert!(matches!(
            result.unwrap().phase,
            HandlePhase::Committed | HandlePhase::Charging
        ));
    }
    assert_eq!(register(&f, &next).unwrap().phase, HandlePhase::Committed);
    let event: Option<HandleEvent> = query(&f.ic, f.handle, person(1), "get_handle_event", (2u64,));
    let event = event.unwrap();
    assert_eq!(event.sequence, 2);
    assert_eq!(event.previous, previous);
    assert_eq!(event.handle, "gamma");
    let absent: Option<HandleEvent> =
        query(&f.ic, f.handle, person(1), "get_handle_event", (3u64,));
    assert_eq!(absent, None);
    let balance: Nat = query(
        &f.ic,
        f.ledger,
        person(1),
        "icrc1_balance_of",
        (next.payer,),
    );
    assert_eq!(balance, Nat::from(0u8));
    let oversized: Result<CertifiedBatch> = query(
        &f.ic,
        f.handle,
        person(1),
        "resolve_handle_certified",
        (vec!["alpha"; MAX_BATCH + 1],),
    );
    assert!(matches!(oversized, Err(Error::QuotaExceeded)));
}

// Use the same PocketIC, dependencies and build settings for both Wasm sets.
// Balance deltas cover dmsg_handle only, excluding user/ledger execution fees.
#[test]
#[ignore = "cycles comparison for dmsg_handle builds"]
fn handle_cycles_profile() {
    let f = Fixture::new();
    let owner = f.create(1);
    let entries: Vec<_> = (0..256)
        .map(|n| LegacyReservation {
            handle: format!("legacy{n:04}"),
            legacy_owner: person(1),
            legacy_name_principal: None,
            frozen_admins: vec![],
            quarantined: false,
        })
        .collect();
    let snapshot = seal_snapshot(&f, &entries);
    // A sealed snapshot deliberately rejects further import calls. Profile the
    // idempotent seal separately from name writes and registration retries.
    measure(&f, 0, "seal_retry", || {
        let r: Result<SnapshotProgress> = update(
            &f.ic,
            f.handle,
            Principal::anonymous(),
            "seal_legacy_snapshot",
            (),
        );
        assert_eq!(r.unwrap().rolling_digest, snapshot.entries_digest);
    });
    f.mint(person(1), 40 * price("newname"));
    f.approve_handle(person(1), 40 * price("newname"));
    for history in 0..17 {
        let input = registration(&f, &owner, &format!("name{history:04}"), history as u8 + 1);
        authorize(&f, 1, &input.intent);
        if [0, 8, 16].contains(&history) {
            measure(&f, history, "register", || register(&f, &input)).unwrap();
            measure(&f, history, "register_retry", || register(&f, &input)).unwrap();
            let taken = registration(&f, &owner, &input.intent.handle, history as u8 + 80);
            assert_eq!(
                measure(&f, history, "register_taken", || register(&f, &taken)),
                Err(Error::VersionConflict)
            );
        } else {
            register(&f, &input).unwrap();
        }
    }
    println!(
        "handle_storage names=17 legacy=256 stable_bytes={} wasm_bytes={}",
        f.ic.get_stable_memory(f.handle).len(),
        wasm("dmsg_handle").len()
    );
    measure(&f, 17, "upgrade", || upgrade(&f));
}

fn claim_intent(
    f: &Fixture,
    owner: &AccountId,
    snapshot: &LegacySnapshot,
    legacy: &LegacyReservation,
    nonce: u8,
) -> HandleIntent {
    HandleIntent {
        handle_canister: f.handle,
        action: HandleAction::ClaimLegacy,
        account_id: *owner,
        target_account: None,
        handle: legacy.handle.clone(),
        expected_version: 0,
        op_id: Hash::new([nonce; 32]),
        terms_digest: digest(
            "dmsg/legacy-claim/v1",
            &(snapshot.snapshot_id, legacy, owner),
        ),
    }
}

fn transfer_intents(
    f: &Fixture,
    owner: &AccountId,
    target: &AccountId,
    name: &str,
    version: u64,
    nonce: u8,
) -> (HandleIntent, HandleIntent) {
    let op_id = Hash::new([nonce; 32]);
    let from = HandleIntent {
        handle_canister: f.handle,
        action: HandleAction::Transfer,
        account_id: *owner,
        target_account: Some(*target),
        handle: name.into(),
        expected_version: version,
        op_id,
        terms_digest: digest(
            "dmsg/handle-transfer/v1",
            &(f.handle, name, owner, target, version, op_id),
        ),
    };
    let accept = HandleIntent {
        action: HandleAction::AcceptTransfer,
        account_id: *target,
        target_account: Some(*owner),
        ..from.clone()
    };
    (from, accept)
}

#[test]
fn handle_failed_charges_are_diagnostic_and_release_names_accounts_and_capacity() {
    let f = Fixture::new();
    with_max_pending(&f, 1);
    let owner = f.create(1);
    let other = f.create(2);
    seal_snapshot(&f, &[]);
    f.mint(person(1), 3 * price("allowance"));
    for (nonce, approved) in [(1, 0), (2, price("allowance") - 1)] {
        f.approve_handle(person(1), approved);
        let input = registration(&f, &owner, "allowance", nonce);
        authorize(&f, 1, &input.intent);
        let result = register(&f, &input);
        assert!(
            matches!(result, Err(Error::Unavailable(ref reason)) if reason.contains("allowance"))
        );
        let HandlePhase::Rejected { reason } = operation(&f, &input).unwrap().phase else {
            panic!("rejected")
        };
        assert!(reason.contains("allowance"));
    }
    f.approve_handle(person(1), price("allowance"));
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "reject_next_transfers",
        (1u32,),
    );
    let input = registration(&f, &owner, "allowance", 3);
    authorize(&f, 1, &input.intent);
    assert!(
        matches!(register(&f, &input), Err(Error::Unavailable(ref reason)) if reason.contains("temporarily"))
    );
    let rejected = operation(&f, &input).unwrap();
    assert!(matches!(rejected.phase, HandlePhase::Rejected { .. }));
    // The spent operation ID never charges again, even after upgrade.
    upgrade(&f);
    assert_eq!(register(&f, &input).unwrap(), rejected);
    assert_eq!(commit(&f, &input), Err(Error::VersionConflict));
    // Another account can buy the name at once: no unpaid lock or capacity remains.
    let next = registration(&f, &other, "allowance", 4);
    authorize(&f, 2, &next.intent);
    let paid = register(&f, &next).unwrap();
    assert_eq!(paid.phase, HandlePhase::Committed);
    assert_eq!(register(&f, &next).unwrap(), paid);
    let allowance: icrc_ledger_types::icrc2::allowance::Allowance = query(
        &f.ic,
        f.ledger,
        person(1),
        "icrc2_allowance",
        (icrc_ledger_types::icrc2::allowance::AllowanceArgs {
            account: next.payer,
            spender: account(f.handle),
        },),
    );
    assert_eq!(allowance.allowance, 0u64);
    let balance: Nat = query(
        &f.ic,
        f.ledger,
        person(1),
        "icrc1_balance_of",
        (next.payer,),
    );
    assert_eq!(balance, Nat::from(2 * price("allowance")));
}

#[test]
fn handle_fee_updates_only_affect_new_operations() {
    let f = Fixture::new();
    let owner = f.create(1);
    seal_snapshot(&f, &[]);
    f.mint(person(1), 3 * price("feechange"));
    f.approve_handle(person(1), 3 * price("feechange"));
    let denied: Result<()> = update(&f.ic, f.handle, person(99), "update_ledger_fee", (11u128,));
    assert_eq!(denied, Err(Error::Forbidden));
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "set_fee",
        (11u128,),
    );
    let changed: Result<()> = update(
        &f.ic,
        f.handle,
        Principal::anonymous(),
        "update_ledger_fee",
        (11u128,),
    );
    changed.unwrap();
    let config: HandleInit = query(&f.ic, f.handle, person(1), "get_handle_config", ());
    assert_eq!(config.ledger_fee, 11);
    // An approval prepared under the old quote must be prepared again.
    let old = registration(&f, &owner, "feechange", 1);
    authorize(&f, 1, &old.intent);
    assert_eq!(register(&f, &old), Err(Error::FeeBlocked));
    assert_eq!(operation(&f, &old), Err(Error::NotFound));
    let mut new = registration(&f, &owner, "feechange", 2);
    new.fee = 11;
    new.intent.terms_digest =
        charge_terms_digest(f.ledger, &new.payer, price("feechange") - 11, 11);
    authorize(&f, 1, &new.intent);
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "lose_next_response",
        (),
    );
    assert_eq!(register(&f, &new), Err(Error::ExecutionUnknown));
    let unknown = operation(&f, &new).unwrap();
    let changed: Result<()> = update(
        &f.ic,
        f.handle,
        Principal::anonymous(),
        "update_ledger_fee",
        (12u128,),
    );
    changed.unwrap();
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "set_fee",
        (12u128,),
    );
    upgrade(&f);
    // Replays keep the original terms instead of requoting the current fee.
    assert_eq!(register(&f, &new).unwrap(), unknown);
    let result = commit(&f, &new).unwrap();
    assert_eq!(result.phase, HandlePhase::Committed);
    assert_eq!(result.registration.fee, 11);
    assert_eq!(result.memo, unknown.memo);
    assert_eq!(result.created_at, unknown.created_at);
}

#[test]
fn handle_legacy_lookups_and_frozen_admin_claims_survive_upgrade() {
    let f = Fixture::new();
    let owner = f.create(1);
    let entries = vec![
        LegacyReservation {
            handle: "namedowner".into(),
            legacy_owner: person(80),
            legacy_name_principal: Some(person(80)),
            frozen_admins: vec![person(1)],
            quarantined: false,
        },
        LegacyReservation {
            handle: "quarantined".into(),
            legacy_owner: person(1),
            legacy_name_principal: None,
            frozen_admins: vec![],
            quarantined: true,
        },
    ];
    let snapshot = seal_snapshot(&f, &entries);
    for upgraded in [false, true] {
        if upgraded {
            upgrade(&f);
        }
        assert_eq!(legacy(&f, "NAMEDOWNER"), Ok(Some(entries[0].clone())));
        assert_eq!(legacy(&f, "missing"), Ok(None));
        // Only the snapshot commitment is certified. Frozen records stay out of
        // the heap tree because every claim rechecks the exact record on chain.
        let progress = progress(&f);
        assert!(progress.sealed);
        let proof: Result<CertifiedBatch> =
            query(&f.ic, f.handle, person(1), "snapshot_certified", ());
        let proof = proof.unwrap();
        let cert: ic_certification::Certificate = cbor2::from_slice(&proof.certificate).unwrap();
        let entry = &proof.entries[0];
        let witness: ic_certification::HashTree = cbor2::from_slice(&entry.witness).unwrap();
        assert_eq!(
            cert.tree.lookup_path([
                b"canister".as_slice(),
                f.handle.as_slice(),
                b"certified_data".as_slice()
            ]),
            ic_certification::LookupResult::Found(&witness.digest())
        );
        assert_eq!(entry.key.as_ref(), b"_legacy_snapshot");
        assert_eq!(entry.value.as_ref().unwrap().as_ref(), canonical(&progress));
        assert_eq!(
            witness.lookup_path([entry.key.as_ref()]),
            ic_certification::LookupResult::Found(entry.value.as_ref().unwrap())
        );
        assert_eq!(
            witness.lookup_path([b"_legacy/namedowner".as_slice()]),
            ic_certification::LookupResult::Absent
        );
    }
    for (n, entry) in entries.iter().enumerate() {
        let intent = claim_intent(&f, &owner, &snapshot, entry, n as u8 + 1);
        authorize(&f, 1, &intent);
        for caller in [person(2), person(80)] {
            let result: Result<HandleRecord> = update(
                &f.ic,
                f.handle,
                caller,
                "claim_legacy_handle",
                (&intent, snapshot.snapshot_id),
            );
            assert_eq!(
                result,
                Err(if entry.quarantined {
                    Error::Locked
                } else {
                    Error::Forbidden
                })
            );
        }
        let result: Result<HandleRecord> = update(
            &f.ic,
            f.handle,
            person(1),
            "claim_legacy_handle",
            (&intent, snapshot.snapshot_id),
        );
        if entry.quarantined {
            assert_eq!(result, Err(Error::Locked));
        } else {
            assert_eq!(result.unwrap().owner_account, owner);
        }
    }
}

#[test]
fn handle_batched_transfer_checks_both_approvals_and_commits_once() {
    let f = Fixture::new();
    let owner = f.create(1);
    let target = f.create(2);
    let legacy = LegacyReservation {
        handle: "transfername".into(),
        legacy_owner: person(1),
        legacy_name_principal: None,
        frozen_admins: vec![],
        quarantined: false,
    };
    let snapshot = seal_snapshot(&f, std::slice::from_ref(&legacy));
    let claim = claim_intent(&f, &owner, &snapshot, &legacy, 1);
    authorize(&f, 1, &claim);
    let result: Result<HandleRecord> = update(
        &f.ic,
        f.handle,
        person(1),
        "claim_legacy_handle",
        (claim, snapshot.snapshot_id),
    );
    result.unwrap();
    let (from, accept) = transfer_intents(&f, &owner, &target, "transfername", 1, 2);
    authorize(&f, 1, &from);
    let denied: Result<HandleRecord> = update(
        &f.ic,
        f.handle,
        person(1),
        "transfer_handle",
        (&from, &accept),
    );
    assert_eq!(denied, Err(Error::NotFound));
    // Renew only the receiver. The expired sender still rejects the whole pair.
    f.ic.advance_time(Duration::from_secs(61));
    authorize(&f, 2, &accept);
    let expired: Result<HandleRecord> = update(
        &f.ic,
        f.handle,
        person(1),
        "transfer_handle",
        (&from, &accept),
    );
    assert_eq!(expired, Err(Error::Expired));
    authorize(&f, 1, &from);
    let forbidden: Result<()> = update(
        &f.ic,
        f.user,
        person(1),
        "consume_handle_transfer_authorizations",
        (&from, &accept),
    );
    assert_eq!(forbidden, Err(Error::Forbidden));
    let calls: Vec<_> = (0..2)
        .map(|_| {
            f.ic.submit_call(
                f.handle,
                person(1),
                "transfer_handle",
                candid::encode_args((&from, &accept)).unwrap(),
            )
            .unwrap()
        })
        .collect();
    for call in calls {
        let result: Result<HandleRecord> =
            candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap();
        assert_eq!(result.unwrap().version, 2);
    }
    let absent: Option<HandleEvent> =
        query(&f.ic, f.handle, person(1), "get_handle_event", (2u64,));
    assert!(absent.is_none());
    upgrade(&f);
    let replay: Result<HandleRecord> = update(
        &f.ic,
        f.handle,
        person(1),
        "transfer_handle",
        (&from, &accept),
    );
    assert_eq!(replay.unwrap().owner_account, target);
}

// Measures deployed public endpoints, without a privileged seeding API. Frozen
// legacy names stay out of the heap certification tree; active names rebuild it.
#[test]
#[ignore = "legacy and active-name capacity profile"]
fn handle_scale_profile() {
    for (legacy_names, active) in [(1_000usize, 17usize), (10_000, 17), (1_000, 1_017)] {
        let f = Fixture::new();
        let owner = f.create(1);
        let target = f.create(2);
        let entries: Vec<_> = (0..legacy_names)
            .map(|n| LegacyReservation {
                handle: format!("legacy{n:05}"),
                legacy_owner: person(1),
                legacy_name_principal: None,
                frozen_admins: vec![],
                quarantined: false,
            })
            .collect();
        seal_snapshot(&f, &entries);
        let total = active as u128 * price("scale0000");
        f.mint(person(1), total);
        f.approve_handle(person(1), total);
        let label = format!("legacy={legacy_names} active={active}");
        for n in 0..active {
            let mut input = registration(&f, &owner, &format!("scale{n:04}"), 0);
            let mut op_id = [1; 32];
            op_id[..8].copy_from_slice(&(n as u64).to_be_bytes());
            input.intent.op_id = Hash::new(op_id);
            // An account keeps at most 32 live one-minute handle authorizations.
            if n % 30 == 29 {
                f.ic.advance_time(Duration::from_secs(61));
            }
            authorize(&f, 1, &input.intent);
            if n + 1 == active {
                measure(&f, active, &format!("scale_register {label}"), || {
                    register(&f, &input)
                })
                .unwrap();
            } else {
                register(&f, &input).unwrap();
            }
        }
        let (from, accept) = transfer_intents(&f, &owner, &target, "scale0000", 1, 100);
        authorize(&f, 1, &from);
        authorize(&f, 2, &accept);
        let result: Result<HandleRecord> =
            measure(&f, active, &format!("scale_transfer {label}"), || {
                update(
                    &f.ic,
                    f.handle,
                    person(1),
                    "transfer_handle",
                    (&from, &accept),
                )
            });
        assert_eq!(result.unwrap().version, 2);
        let found = legacy(&f, "legacy00000");
        let bytes = candid::encode_one(&found).unwrap().len();
        assert!(found.unwrap().is_some());
        let names: Result<CertifiedBatch> = query(
            &f.ic,
            f.handle,
            person(1),
            "resolve_handle_certified",
            (vec!["scale0000", "missing"],),
        );
        assert_eq!(names.unwrap().entries.len(), 2);
        let status = f.ic.canister_status(f.handle, None).unwrap();
        println!(
            "handle_scale {label} heap_bytes={} stable_bytes={} legacy_response_bytes={bytes}",
            status.memory_metrics.wasm_memory_size, status.memory_metrics.stable_memory_size
        );
        measure(&f, active, &format!("scale_upgrade {label}"), || {
            upgrade(&f)
        });
        // The first upgrade after a long run can include a one-off charge that
        // does not recur; an install-code rate limit also applies right after it.
        f.ic.advance_time(Duration::from_secs(600));
        for _ in 0..20 {
            f.ic.tick();
        }
        measure(&f, active, &format!("scale_upgrade_repeat {label}"), || {
            upgrade(&f)
        });
        assert!(legacy(&f, "legacy00000").unwrap().is_some());
        verify_names(
            &f,
            &[record_of(&f, "scale0001"), record_of(&f, "scale0000")],
        );
    }
}

fn record_of(f: &Fixture, name: &str) -> HandleRecord {
    let batch: Result<CertifiedBatch> = query(
        &f.ic,
        f.handle,
        person(1),
        "resolve_handle_certified",
        (vec![name],),
    );
    decode_canonical(batch.unwrap().entries[0].value.as_ref().unwrap()).unwrap()
}
