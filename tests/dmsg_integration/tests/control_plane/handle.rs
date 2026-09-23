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
            account_id: owner.clone(),
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

fn reserve(f: &Fixture, input: &Registration) -> Result<HandleOperation> {
    update(&f.ic, f.handle, person(1), "reserve_handle", (input,))
}

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

fn expire(f: &Fixture, input: &Registration) -> Result<()> {
    update(
        &f.ic,
        f.handle,
        person(99),
        "expire_handle_reservation",
        (&input.intent.account_id, input.intent.op_id),
    )
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
    let empty: Result<Vec<LegacyReservation>> = query(
        &f.ic,
        f.handle,
        person(1),
        "list_legacy_reservations",
        (None::<String>,),
    );
    assert!(empty.unwrap().is_empty());
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
fn handle_locks_survive_upgrade_and_unknown_charges_never_expire() {
    let f = Fixture::new();
    // Exercise the global counter as well as the per-account set at capacity.
    f.ic.reinstall_canister(
        f.handle,
        wasm("dmsg_handle"),
        candid::encode_args((HandleInit {
            home_user: f.user,
            ledger: f.ledger,
            ledger_fee: 10,
            max_pending: 1,
        },))
        .unwrap(),
        None,
    )
    .unwrap();
    let owner = f.create(1);
    let other = f.create(2);
    seal_snapshot(&f, &[]);
    let first = registration(&f, &owner, "first", 1);
    authorize(&f, 1, &first.intent);
    let reserved = reserve(&f, &first).unwrap();
    let same_owner = registration(&f, &owner, "second", 2);
    let other_owner = registration(&f, &other, "other", 3);
    // No authorization exists for these requests: the quota must reject them
    // locally, before user calls that would instead return NotFound.
    for input in [&same_owner, &other_owner] {
        assert_eq!(reserve(&f, input), Err(Error::QuotaExceeded));
        assert_eq!(operation(&f, input), Err(Error::NotFound));
    }
    assert_eq!(reserve(&f, &first).unwrap(), reserved);
    assert_eq!(expire(&f, &first), Err(Error::VersionConflict));
    upgrade(&f);
    assert_eq!(reserve(&f, &other_owner), Err(Error::QuotaExceeded));
    assert_eq!(operation(&f, &first).unwrap(), reserved);
    f.ic.advance_time(Duration::from_secs(16 * 60));
    expire(&f, &first).unwrap();
    assert_eq!(expire(&f, &first), Err(Error::VersionConflict));

    let unknown = registration(&f, &owner, "first", 4);
    authorize(&f, 1, &unknown.intent);
    reserve(&f, &unknown).unwrap();
    f.mint(person(1), price("first"));
    f.approve_handle(person(1), price("first"));
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "lose_next_response",
        (),
    );
    assert_eq!(commit(&f, &unknown), Err(Error::ExecutionUnknown));
    assert_eq!(
        operation(&f, &unknown).unwrap().phase,
        HandlePhase::ChargeUnknown
    );
    f.ic.advance_time(Duration::from_secs(16 * 60));
    upgrade(&f);
    assert_eq!(expire(&f, &unknown), Err(Error::VersionConflict));
    assert_eq!(reserve(&f, &other_owner), Err(Error::QuotaExceeded));
    let paid = commit(&f, &unknown).unwrap(); // ledger Duplicate, original timestamp
    assert_eq!(paid.phase, HandlePhase::Committed);
    assert_eq!(paid.ledger_block, Some(0));
    assert_eq!(commit(&f, &unknown).unwrap(), paid);
    let balance: Nat = query(
        &f.ic,
        f.ledger,
        person(1),
        "icrc1_balance_of",
        (unknown.payer,),
    );
    assert_eq!(balance, Nat::from(0u8));

    // A definitive failure releases both locks exactly once and appends no event.
    let rejected = registration(&f, &owner, "rejected", 5);
    authorize(&f, 1, &rejected.intent);
    reserve(&f, &rejected).unwrap();
    assert!(matches!(commit(&f, &rejected), Err(Error::Unavailable(_))));
    assert!(matches!(
        operation(&f, &rejected).unwrap().phase,
        HandlePhase::Rejected { .. }
    ));
    assert_eq!(commit(&f, &rejected), Err(Error::VersionConflict));
    let next = registration(&f, &other, "rejected", 6);
    authorize(&f, 2, &next.intent);
    assert_eq!(reserve(&f, &next).unwrap().phase, HandlePhase::Reserved);
    let absent: Option<HandleEvent> =
        query(&f.ic, f.handle, person(1), "get_handle_event", (1u64,));
    assert_eq!(absent, None);
}

#[test]
fn handle_concurrent_reservations_commit_once_and_rebuild_certificates() {
    let f = Fixture::new();
    let owner = f.create(1);
    let snapshot = seal_snapshot(&f, &[]);
    let inputs = [
        registration(&f, &owner, "alpha", 1),
        registration(&f, &owner, "beta", 2),
    ];
    for input in &inputs {
        authorize(&f, 1, &input.intent);
    }
    let calls: Vec<_> = inputs
        .iter()
        .map(|input| {
            f.ic.submit_call(
                f.handle,
                person(1),
                "reserve_handle",
                candid::encode_args((input,)).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let results: Vec<Result<HandleOperation>> = calls
        .into_iter()
        .map(|call| candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap())
        .collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(Error::QuotaExceeded))
            .count(),
        1
    );
    let winner = results.iter().position(|r| r.is_ok()).unwrap();
    f.mint(person(1), price("alpha") + price("beta"));
    f.approve_handle(person(1), price("alpha") + price("beta"));
    let calls: Vec<_> = (0..2)
        .map(|_| {
            f.ic.submit_call(
                f.handle,
                person(1),
                "commit_handle",
                candid::encode_args((&owner, inputs[winner].intent.op_id)).unwrap(),
            )
            .unwrap()
        })
        .collect();
    for call in calls {
        let result: Result<HandleOperation> =
            candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap();
        assert!(
            result
                .as_ref()
                .is_ok_and(|o| o.phase == HandlePhase::Committed)
                || result == Err(Error::Pending)
        );
    }
    assert_eq!(
        commit(&f, &inputs[winner]).unwrap().phase,
        HandlePhase::Committed
    );
    reserve(&f, &inputs[1 - winner]).unwrap();
    commit(&f, &inputs[1 - winner]).unwrap();
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
            owner_account: owner.clone(),
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
    // The log's own persisted length supplies the next sequence after upgrade.
    let next = registration(&f, &owner, "gamma", 3);
    authorize(&f, 1, &next.intent);
    f.mint(person(1), price("gamma"));
    f.approve_handle(person(1), price("gamma"));
    reserve(&f, &next).unwrap();
    commit(&f, &next).unwrap();
    let event: Option<HandleEvent> = query(&f.ic, f.handle, person(1), "get_handle_event", (2u64,));
    let event = event.unwrap();
    assert_eq!(event.sequence, 2);
    assert_eq!(event.previous, previous);
    assert_eq!(event.handle, "gamma");
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
            measure(&f, history, "reserve", || reserve(&f, &input)).unwrap();
            measure(&f, history, "reserve_retry", || reserve(&f, &input)).unwrap();
            let blocked = registration(&f, &owner, "blocked", history as u8 + 80);
            authorize(&f, 1, &blocked.intent);
            assert_eq!(
                measure(&f, history, "reserve_quota", || reserve(&f, &blocked)),
                Err(Error::QuotaExceeded)
            );
            measure(&f, history, "commit", || commit(&f, &input)).unwrap();
            measure(&f, history, "commit_retry", || commit(&f, &input)).unwrap();
        } else {
            reserve(&f, &input).unwrap();
            commit(&f, &input).unwrap();
        }
    }
    println!(
        "handle_storage names=17 legacy=256 stable_bytes={} wasm_bytes={}",
        f.ic.get_stable_memory(f.handle).len(),
        wasm("dmsg_handle").len()
    );
    measure(&f, 17, "upgrade", || {
        f.ic.upgrade_canister(
            f.handle,
            wasm("dmsg_handle"),
            candid::encode_args(()).unwrap(),
            None,
        )
        .unwrap();
    });
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
        account_id: owner.clone(),
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
        account_id: owner.clone(),
        target_account: Some(target.clone()),
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
        account_id: target.clone(),
        target_account: Some(owner.clone()),
        ..from.clone()
    };
    (from, accept)
}

#[test]
fn handle_expired_reservations_reclaim_names_accounts_and_global_capacity() {
    let f = Fixture::new();
    f.ic.reinstall_canister(
        f.handle,
        wasm("dmsg_handle"),
        candid::encode_args((HandleInit {
            home_user: f.user,
            ledger: f.ledger,
            ledger_fee: 10,
            max_pending: 1,
        },))
        .unwrap(),
        None,
    )
    .unwrap();
    let owner = f.create(1);
    let other = f.create(2);
    seal_snapshot(&f, &[]);
    let first = registration(&f, &owner, "abandoned", 1);
    authorize(&f, 1, &first.intent);
    reserve(&f, &first).unwrap();
    upgrade(&f);
    f.ic.advance_time(Duration::from_secs(16 * 60));
    let next = registration(&f, &other, "abandoned", 2);
    authorize(&f, 2, &next.intent);
    reserve(&f, &next).unwrap();
    assert_eq!(operation(&f, &first).unwrap().phase, HandlePhase::Expired);
    assert_eq!(expire(&f, &first), Err(Error::VersionConflict));
    // An expired commit also releases its resources without a separate call.
    f.ic.advance_time(Duration::from_secs(16 * 60));
    assert_eq!(commit(&f, &next), Err(Error::Expired));
    let last = registration(&f, &other, "freshname", 3);
    authorize(&f, 2, &last.intent);
    reserve(&f, &last).unwrap();
    assert_eq!(operation(&f, &next).unwrap().phase, HandlePhase::Expired);
}

#[test]
fn handle_cleanup_is_bounded_and_always_checks_the_requesting_account() {
    let f = Fixture::new();
    seal_snapshot(&f, &[]);
    let mut inputs = vec![];
    for n in 1..=66u8 {
        let owner = f.create(n);
        let input = registration(&f, &owner, &format!("held{n:02}"), n);
        authorize(&f, n, &input.intent);
        reserve(&f, &input).unwrap();
        inputs.push(input);
        // Distinct deadlines make the expiry-index order deterministic.
        f.ic.advance_time(Duration::from_secs(1));
    }
    f.ic.advance_time(Duration::from_secs(16 * 60));
    upgrade(&f);
    let next = registration(&f, &inputs[65].intent.account_id, "anothername", 100);
    // Stop at authorization to inspect one bounded synchronous cleanup batch.
    assert_eq!(reserve(&f, &next), Err(Error::NotFound));
    assert_eq!(
        operation(&f, &inputs[0]).unwrap().phase,
        HandlePhase::Expired
    );
    assert_eq!(
        operation(&f, &inputs[64]).unwrap().phase,
        HandlePhase::Reserved
    );
    assert_eq!(
        operation(&f, &inputs[65]).unwrap().phase,
        HandlePhase::Expired
    );
    assert_eq!(expire(&f, &inputs[65]), Err(Error::VersionConflict));
    authorize(&f, 66, &next.intent);
    reserve(&f, &next).unwrap();
}

#[test]
fn handle_allowance_failures_are_diagnostic_and_temporary_rejections_retry() {
    let f = Fixture::new();
    let owner = f.create(1);
    seal_snapshot(&f, &[]);
    f.mint(person(1), 3 * price("allowance"));
    for (nonce, approved) in [(1, 0), (2, price("allowance") - 1)] {
        f.approve_handle(person(1), approved);
        let input = registration(&f, &owner, "allowance", nonce);
        authorize(&f, 1, &input.intent);
        reserve(&f, &input).unwrap();
        let result = commit(&f, &input);
        assert!(
            matches!(result, Err(Error::Unavailable(ref reason)) if reason.contains("allowance"))
        );
        let HandlePhase::Rejected { reason } = operation(&f, &input).unwrap().phase else {
            panic!("rejected")
        };
        assert!(reason.contains("allowance"));
    }
    let input = registration(&f, &owner, "allowance", 3);
    authorize(&f, 1, &input.intent);
    let reserved = reserve(&f, &input).unwrap();
    f.approve_handle(person(1), price("allowance"));
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "reject_next_transfers",
        (1u32,),
    );
    assert!(
        matches!(commit(&f, &input), Err(Error::Unavailable(ref reason)) if reason.contains("temporarily"))
    );
    assert_eq!(operation(&f, &input).unwrap(), reserved);
    upgrade(&f);
    let paid = commit(&f, &input).unwrap();
    assert_eq!(paid.phase, HandlePhase::Committed);
    assert_eq!(commit(&f, &input).unwrap(), paid);
    let allowance: icrc_ledger_types::icrc2::allowance::Allowance = query(
        &f.ic,
        f.ledger,
        person(1),
        "icrc2_allowance",
        (icrc_ledger_types::icrc2::allowance::AllowanceArgs {
            account: input.payer,
            spender: account(f.handle),
        },),
    );
    assert_eq!(allowance.allowance, 0u64);
    let balance: Nat = query(
        &f.ic,
        f.ledger,
        person(1),
        "icrc1_balance_of",
        (input.payer,),
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
    let old = registration(&f, &owner, "feechange", 1);
    authorize(&f, 1, &old.intent);
    let reserved = reserve(&f, &old).unwrap();
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
    assert_eq!(reserve(&f, &old).unwrap(), reserved);
    assert!(
        matches!(commit(&f, &old), Err(Error::Unavailable(ref reason)) if reason.contains("fee"))
    );
    let mut new = registration(&f, &owner, "feechange", 2);
    assert_eq!(reserve(&f, &new), Err(Error::FeeBlocked));
    new.fee = 11;
    new.intent.terms_digest =
        charge_terms_digest(f.ledger, &new.payer, price("feechange") - 11, 11);
    authorize(&f, 1, &new.intent);
    reserve(&f, &new).unwrap();
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "lose_next_response",
        (),
    );
    assert_eq!(commit(&f, &new), Err(Error::ExecutionUnknown));
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
    assert_eq!(reserve(&f, &new).unwrap(), unknown);
    let result = commit(&f, &new).unwrap();
    assert_eq!(result.phase, HandlePhase::Committed);
    assert_eq!(result.registration.fee, 11);
    assert_eq!(result.memo, unknown.memo);
    assert_eq!(result.created_at, unknown.created_at);
}

#[test]
fn handle_legacy_certificates_and_frozen_admin_claims_survive_upgrade() {
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
        let mut replies = vec![];
        for name in ["namedowner", "missing"] {
            let result: Result<CertifiedLegacyReservation> = query(
                &f.ic,
                f.handle,
                person(1),
                "get_legacy_reservation_certified",
                (name.to_uppercase(),),
            );
            let result = result.unwrap();
            replies.push(ByteBuf::from(
                candid::encode_one(Ok::<_, Error>(&result)).unwrap(),
            ));
            assert!(result.progress.sealed);
            assert_eq!(result.proof.entries.len(), 2);
            let cert: ic_certification::Certificate =
                cbor2::from_slice(&result.proof.certificate).unwrap();
            for entry in &result.proof.entries {
                let witness: ic_certification::HashTree =
                    cbor2::from_slice(&entry.witness).unwrap();
                assert_eq!(
                    cert.tree.lookup_path([
                        b"canister".as_slice(),
                        f.handle.as_slice(),
                        b"certified_data".as_slice()
                    ]),
                    ic_certification::LookupResult::Found(&witness.digest())
                );
                match &entry.value {
                    Some(value) => assert_eq!(
                        witness.lookup_path([entry.key.as_ref()]),
                        ic_certification::LookupResult::Found(value.as_ref())
                    ),
                    None => assert_eq!(
                        witness.lookup_path([entry.key.as_ref()]),
                        ic_certification::LookupResult::Absent
                    ),
                }
            }
            assert_eq!(
                result.proof.entries[0].value.as_ref().unwrap().as_ref(),
                canonical(&result.progress)
            );
            assert_eq!(
                result.proof.entries[1].key.as_ref(),
                format!("_legacy/{name}").as_bytes()
            );
            assert_eq!(
                result.proof.entries[1].value.as_ref().map(|v| v.to_vec()),
                result
                    .reservation
                    .as_ref()
                    .map(|r| canonical(&digest("dmsg/legacy-reservation/v1", r)))
            );
            assert_eq!(result.reservation.is_some(), name == "namedowner");
        }
        if !upgraded {
            if let Some(path) = std::env::var_os("DMSG_EXPORT_HANDLE_CERTIFICATE") {
                std::fs::write(
                    path,
                    canonical(&(
                        ByteBuf::from(f.ic.root_key().unwrap()),
                        time(&f.ic),
                        f.handle.to_text(),
                        person(1).to_text(),
                        &replies,
                    )),
                )
                .unwrap();
            }
        }
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

// Measures deployed public endpoints, without a privileged seeding API. Legacy
// leaves exercise the same heap certification tree as active ownership leaves.
#[test]
#[ignore = "1k/10k legacy certification and operation capacity profile"]
fn handle_scale_profile() {
    for size in [1_000usize, 10_000] {
        let f = Fixture::new();
        let owner = f.create(1);
        let target = f.create(2);
        let entries: Vec<_> = (0..size)
            .map(|n| LegacyReservation {
                handle: format!("legacy{n:05}"),
                legacy_owner: person(1),
                legacy_name_principal: None,
                frozen_admins: vec![],
                quarantined: false,
            })
            .collect();
        seal_snapshot(&f, &entries);
        f.mint(person(1), 20 * price("scale000"));
        f.approve_handle(person(1), 20 * price("scale000"));
        for n in 0..17u8 {
            let input = registration(&f, &owner, &format!("scale{n:03}"), n + 1);
            authorize(&f, 1, &input.intent);
            if n == 16 {
                measure(&f, size, "scale_reserve", || reserve(&f, &input)).unwrap();
                measure(&f, size, "scale_commit", || commit(&f, &input)).unwrap();
            } else {
                reserve(&f, &input).unwrap();
                commit(&f, &input).unwrap();
            }
        }
        let (from, accept) = transfer_intents(&f, &owner, &target, "scale000", 1, 100);
        authorize(&f, 1, &from);
        authorize(&f, 2, &accept);
        let result: Result<HandleRecord> = measure(&f, size, "scale_transfer", || {
            update(
                &f.ic,
                f.handle,
                person(1),
                "transfer_handle",
                (&from, &accept),
            )
        });
        assert_eq!(result.unwrap().version, 2);
        let start = std::time::Instant::now();
        let result: Result<CertifiedLegacyReservation> = query(
            &f.ic,
            f.handle,
            person(1),
            "get_legacy_reservation_certified",
            ("legacy00000",),
        );
        let elapsed = start.elapsed();
        let bytes = candid::encode_one(&result).unwrap().len();
        assert!(result.unwrap().reservation.is_some());
        let names: Result<CertifiedBatch> = query(
            &f.ic,
            f.handle,
            person(1),
            "resolve_handle_certified",
            (vec!["scale000", "missing"],),
        );
        assert_eq!(names.unwrap().entries.len(), 2);
        let status = f.ic.canister_status(f.handle, None).unwrap();
        println!("handle_scale legacy={size} active=17 heap_bytes={} stable_bytes={} legacy_response_bytes={bytes} local_query_us={}",
            status.memory_metrics.wasm_memory_size, status.memory_metrics.stable_memory_size, elapsed.as_micros());
        measure(&f, size, "scale_upgrade", || upgrade(&f));
        let restored: Result<CertifiedLegacyReservation> = query(
            &f.ic,
            f.handle,
            person(1),
            "get_legacy_reservation_certified",
            ("legacy00000",),
        );
        assert!(restored.unwrap().reservation.is_some());
    }
}
