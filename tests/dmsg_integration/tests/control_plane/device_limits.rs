use super::*;

fn enrollment(f: &Fixture, id: &AccountId, n: u8) -> AccountMutation {
    let s = f.account_id(1, id);
    let input = device(n);
    let request_id = digest("device-limit-test", &(id, s.account_version, n));
    let proof = key(n)
        .sign(
            digest(
                "dmsg/add-device/v1",
                &(f.user, id, &input, s.account_version, request_id),
            )
            .as_slice(),
        )
        .to_bytes()
        .into();
    let mut m = AccountMutation {
        account_id: *id,
        expected_version: s.account_version,
        command: AccountCommand::AddDevice {
            device: input,
            proof,
        },
        approval: Approval {
            device_id: Hash::new([1; 32]),
            security_epoch: s.security_epoch,
            sequence: s.devices[&Hash::new([1; 32])].next_sequence,
            request_id,
            expires_at: time(&f.ic) + MINUTE,
            signature: Default::default(),
            hardware: None,
        },
    };
    m.approval.signature = key(1)
        .sign(
            approval_message(
                f.user,
                id,
                "dmsg/account/v2",
                &(&m.expected_version, &m.command),
                &m.approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .into();
    m
}

fn enroll(f: &Fixture, m: &AccountMutation) -> Result<OperationReceipt> {
    update(&f.ic, f.user, person(1), "mutate_account", (m,))
}

#[test]
fn device_limits_follow_membership_upgrade_expiry_and_revocation() {
    let f = Fixture::commercial();
    let id = f.create(1);
    enroll(&f, &enrollment(&f, &id, 2)).unwrap();
    let denied = enrollment(&f, &id, 3);
    let before = f.account_id(1, &id);
    assert_eq!(enroll(&f, &denied), Err(Error::QuotaExceeded));
    assert_eq!(f.account_id(1, &id), before);

    let mut last = None;
    let mut previous = 2;
    for (sku, limit, op) in [
        ("plus", 3, 40),
        ("upgrade-pro", 5, 50),
        ("upgrade-max", 10, 60),
    ] {
        // Each upgrade starts a new interval after the preceding purchase.
        f.ic.advance_time(Duration::from_secs(1));
        pay(&f, &id, sku, op);
        for n in previous + 1..=limit {
            let m = enrollment(&f, &id, n);
            let receipt = enroll(&f, &m).unwrap();
            last = Some((m, receipt));
        }
        let before = f.account_id(1, &id);
        assert_eq!(
            before
                .devices
                .values()
                .filter(|d| d.revoked_at.is_none())
                .count(),
            usize::from(limit)
        );
        assert_eq!(
            enroll(&f, &enrollment(&f, &id, 20)),
            Err(Error::QuotaExceeded)
        );
        assert_eq!(f.account_id(1, &id), before);
        previous = limit;
    }

    let before = f.account_id(1, &id);
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(f.account_id(1, &id), before);
    f.ic.advance_time(Duration::from_millis(367 * DAY));
    assert_eq!(
        enroll(&f, &enrollment(&f, &id, 20)),
        Err(Error::QuotaExceeded)
    );
    assert_eq!(f.account_id(1, &id), before);
    let (m, receipt) = last.unwrap();
    assert_eq!(enroll(&f, &m), Ok(receipt.clone()));
    // A committed retry and security maintenance do not depend on commerce.
    f.ic.stop_canister(f.commerce, None).unwrap();
    assert_eq!(enroll(&f, &m), Ok(receipt));
    for n in 2..=10 {
        let secret: Result<Hash> = query(
            &f.ic,
            f.user,
            person(1),
            "unlock_secret",
            (id, Hash::new([n; 32])),
        );
        secret.unwrap();
        f.mutate(
            1,
            &id,
            AccountCommand::RevokeDevice {
                device_id: Hash::new([n; 32]),
            },
        )
        .unwrap();
    }
    // The second active device is within Free and needs no commerce round trip.
    enroll(&f, &enrollment(&f, &id, 21)).unwrap();
    let before = f.account_id(1, &id);
    assert!(enroll(&f, &enrollment(&f, &id, 22)).is_err());
    assert_eq!(f.account_id(1, &id), before);
}

#[test]
fn device_entitlement_callback_rechecks_concurrent_account_changes() {
    let f = Fixture::commercial();
    let id = f.create(1);
    pay(&f, &id, "plus", 40);
    enroll(&f, &enrollment(&f, &id, 2)).unwrap();
    let m = enrollment(&f, &id, 3);
    let call =
        f.ic.submit_call(
            f.user,
            person(1),
            "mutate_account",
            candid::encode_one(&m).unwrap(),
        )
        .unwrap();
    f.ic.advance_time(Duration::from_millis(1));
    f.mutate(
        1,
        &id,
        AccountCommand::RevokeDevice {
            device_id: Hash::new([2; 32]),
        },
    )
    .unwrap();
    let result: Result<OperationReceipt> =
        candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap();
    assert!(matches!(
        result,
        Err(Error::PolicyStale | Error::VersionConflict)
    ));
    let state = f.account_id(1, &id);
    assert!(state.devices[&Hash::new([2; 32])].revoked_at.is_some());
    assert!(!state.devices.contains_key(&Hash::new([3; 32])));
    enroll(&f, &enrollment(&f, &id, 3)).unwrap();
}

#[test]
#[ignore = "requires DMSG_PRE_DEVICE_LIMIT_USER_WASM built from the unchanged pre-limit source"]
fn device_limit_upgrade_preserves_legacy_over_limit_accounts() {
    let old =
        std::fs::read(std::env::var_os("DMSG_PRE_DEVICE_LIMIT_USER_WASM").expect("old user Wasm"))
            .unwrap();
    let f = Fixture::with_identity_modules(
        false,
        100,
        MAX_PAYMENT_ESCROWS,
        |_| {},
        [old, wasm("dmsg_directory"), wasm("dmsg_handle")],
    );
    let id = f.create(1);
    let mut last = None;
    for n in 2..=5 {
        let m = enrollment(&f, &id, n);
        let receipt = enroll(&f, &m).unwrap();
        last = Some((m, receipt));
    }
    let before = f.account_id(1, &id);
    let snapshot: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(1),
        "security_snapshot_batch",
        (vec![id],),
    );
    let snapshot = snapshot.unwrap();
    let secret: Result<Hash> = query(
        &f.ic,
        f.user,
        person(1),
        "unlock_secret",
        (id, Hash::new([1; 32])),
    );
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(f.account_id(1, &id), before);
    let after_secret: Result<Hash> = query(
        &f.ic,
        f.user,
        person(1),
        "unlock_secret",
        (id, Hash::new([1; 32])),
    );
    assert_eq!(after_secret, secret);
    let after_snapshot: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(1),
        "security_snapshot_batch",
        (vec![id],),
    );
    assert_eq!(after_snapshot.unwrap().entries, snapshot.entries);
    let (m, receipt) = last.unwrap();
    assert_eq!(enroll(&f, &m), Ok(receipt));
    assert_eq!(
        enroll(&f, &enrollment(&f, &id, 6)),
        Err(Error::QuotaExceeded)
    );
    assert_eq!(f.account_id(1, &id), before);
    for n in 2..=5 {
        f.mutate(
            1,
            &id,
            AccountCommand::RevokeDevice {
                device_id: Hash::new([n; 32]),
            },
        )
        .unwrap();
    }
    enroll(&f, &enrollment(&f, &id, 6)).unwrap();
}
