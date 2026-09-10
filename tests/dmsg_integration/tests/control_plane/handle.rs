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
    if !entries.is_empty() {
        let imported: Result<SnapshotProgress> = update(
            &f.ic,
            f.handle,
            Principal::anonymous(),
            "import_legacy_handles",
            (snapshot.snapshot_id, entries),
        );
        assert_eq!(imported.unwrap().imported, snapshot.count);
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
    assert_eq!(
        operation(&f, &rejected).unwrap().phase,
        HandlePhase::Expired
    );
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
