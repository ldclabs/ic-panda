use super::*;

struct CoseFixture {
    ic: PocketIc,
    cose: Principal,
    home: Principal,
}

impl CoseFixture {
    fn new(daily_executions: u32) -> Self {
        let ic = PocketIcBuilder::new()
            .with_application_subnet()
            .with_fiduciary_subnet()
            .build();
        let cose = ic.create_canister();
        let home = person(70);
        ic.add_cycles(cose, 10_000_000_000_000_000);
        ic.install_canister(
            cose,
            wasm("dmsg_cose"),
            candid::encode_one(CoseInit {
                issuer_namespace: NAMESPACE.into(),
                environment: Environment::Local,
                executing_canister: cose,
                initial_home_user: home,
                derivation_version: 2,
                masters: vec![MasterKey {
                    algorithm: Algorithm::Ed25519,
                    key_name: "key_1".into(),
                    expected_fingerprint: Hash::new([0; 32]),
                }],
                daily_executions,
                daily_cycles: 10_000_000_000_000,
            })
            .unwrap(),
            None,
        );
        let initialized: Result<candid::Reserved> =
            update(&ic, cose, Principal::anonymous(), "initialize_keys", ());
        initialized.unwrap();
        Self { ic, cose, home }
    }

    fn grant(&self, account: u8, sequence: u64, body_bytes: usize) -> ExecutionGrant {
        let account_id = AccountId([account; 12]);
        let key = KeyRequest {
            purpose: KeyPurpose::Statement,
            algorithm: Algorithm::Ed25519,
            generation: 1,
        };
        let descriptor: Result<KeyDescriptor> = query(
            &self.ic,
            self.cose,
            Principal::anonymous(),
            "public_key",
            (
                &account_id,
                KeySelector::Signing(dmsg_types::cose::SigningKey {
                    purpose: SigningPurpose::Statement,
                    algorithm: SigningAlgorithm::Ed25519,
                }),
            ),
        );
        let descriptor = descriptor.unwrap();
        let statement = Statement {
            issuer: account_issuer(NAMESPACE, &account_id).unwrap(),
            subject: None,
            issued_at: None,
            content: StatementContent::Text("x".repeat(body_bytes)),
        };
        let (_, to_be_signed) =
            prepare_cose(&statement, &key.algorithm, &descriptor.key_id).unwrap();
        ExecutionGrant {
            request_id: execution_request_id(&account_id, 0, Hash::new([1; 32]), sequence),
            account_id,
            home_user: self.home,
            home_cose: self.cose,
            execution_sequence: sequence,
            security_epoch: 0,
            device_id: Hash::new([1; 32]),
            device_sequence: sequence,
            approved_at: time(&self.ic),
            expires_at: time(&self.ic) + MINUTE,
            kind: ExecutionKind::Sign {
                key,
                to_be_signed: to_be_signed.into(),
                public_key_fingerprint: descriptor.public_key_fingerprint,
                origin: "https://example.com".into(),
            },
            max_cycles: 100_000_000_000,
        }
    }

    fn execute(&self, grant: &ExecutionGrant) -> Result<ExecutionResult> {
        update(&self.ic, self.cose, self.home, "execute", (grant,))
    }

    fn result(&self, grant: &ExecutionGrant) -> Result<ExecutionResult> {
        query(
            &self.ic,
            self.cose,
            self.home,
            "get_execution",
            (&grant.account_id, grant.request_id),
        )
    }
}

// Compare identical release profiles using DMSG_WASM_DIR. This measures only
// COSE's balance delta. The quoted charged_cycles value includes a response
// reservation, so it must not be subtracted as if it were the actual bill.
#[test]
#[ignore = "cycles comparison for dmsg_cose builds"]
fn cose_cycles_profile() {
    let f = CoseFixture::new(1000);
    for history in 0..9 {
        let grant = f.grant(1, history + 1, 4096);
        let before = f.ic.cycle_balance(f.cose);
        let result = f.execute(&grant).unwrap();
        assert_eq!(result.status(), ExecutionStatus::Completed);
        if [0, 4, 8].contains(&history) {
            let total = before - f.ic.cycle_balance(f.cose);
            println!(
                "cose_cycles history={history} method=execute total={total} charged={}",
                result.charged_cycles
            );
            let before = f.ic.cycle_balance(f.cose);
            assert_eq!(f.execute(&grant), Ok(result.clone()));
            println!(
                "cose_cycles history={history} method=retry cycles={}",
                before - f.ic.cycle_balance(f.cose)
            );
            assert_eq!(f.result(&grant), Ok(result));
        }
    }
    for history in 0..9 {
        let mut grant = f.grant(2, history + 1, 4096);
        grant.approved_at -= MINUTE;
        grant.expires_at = time(&f.ic);
        let before = f.ic.cycle_balance(f.cose);
        let result = f.execute(&grant).unwrap();
        assert_eq!(result.outcome, ExecutionOutcome::ResultExpired);
        assert_eq!(result.charged_cycles, 0);
        if [0, 4, 8].contains(&history) {
            println!(
                "cose_cycles history={history} method=expired cycles={}",
                before - f.ic.cycle_balance(f.cose)
            );
        }
    }
    println!(
        "cose_storage stable_bytes={}",
        f.ic.get_stable_memory(f.cose).len()
    );
}

#[test]
fn cose_callbacks_preserve_other_executions_and_pending_replays() {
    let f = CoseFixture::new(1000);
    let first = f.grant(1, 1, 4096);
    let second = f.grant(1, 2, 4096);
    let submitted =
        f.ic.submit_call(
            f.cose,
            f.home,
            "execute",
            candid::encode_one(&second).unwrap(),
        )
        .unwrap();
    f.ic.tick();
    let pending = f.result(&second).unwrap();
    assert_eq!(pending.status(), ExecutionStatus::Executing);
    // Queue a replay while the original management call is in flight.
    let duplicate =
        f.ic.submit_call(
            f.cose,
            f.home,
            "execute",
            candid::encode_one(&second).unwrap(),
        )
        .unwrap();
    let other =
        f.ic.submit_call(
            f.cose,
            f.home,
            "execute",
            candid::encode_one(&first).unwrap(),
        )
        .unwrap();
    let second_result: Result<ExecutionResult> =
        candid::decode_one(&f.ic.await_call(submitted).unwrap()).unwrap();
    let first_result: Result<ExecutionResult> =
        candid::decode_one(&f.ic.await_call(other).unwrap()).unwrap();
    let replay: Result<ExecutionResult> =
        candid::decode_one(&f.ic.await_call(duplicate).unwrap()).unwrap();
    assert_eq!(
        first_result.as_ref().unwrap().status(),
        ExecutionStatus::Completed
    );
    assert_eq!(
        second_result.as_ref().unwrap().status(),
        ExecutionStatus::Completed
    );
    assert!(replay == Ok(pending) || replay == second_result);
    assert_eq!(f.result(&first), first_result);
    assert_eq!(f.execute(&second), second_result);
    let mut conflict = second;
    conflict.max_cycles -= 1;
    assert_eq!(f.execute(&conflict), Err(Error::IdempotencyConflict));
}

#[test]
fn cose_retention_cleanup_and_upgrade_preserve_replay_protection() {
    let f = CoseFixture::new(1000);
    let first = f.grant(1, 1, 4096);
    let result = f.execute(&first).unwrap();
    assert_eq!(result.status(), ExecutionStatus::Completed);
    let neighbor = f.grant(2, 1, 4096);
    let neighbor_result = f.execute(&neighbor);
    for sequence in 2..=64 {
        let mut expired = f.grant(1, sequence, 1);
        expired.approved_at -= MINUTE;
        expired.expires_at = time(&f.ic);
        let result = f.execute(&expired).unwrap();
        assert_eq!(result.outcome, ExecutionOutcome::ResultExpired);
        assert_eq!(result.charged_cycles, 0);
    }
    let refused = f.grant(1, 65, 1);
    assert_eq!(f.execute(&refused), Err(Error::QuotaExceeded));
    f.ic.upgrade_canister(
        f.cose,
        wasm("dmsg_cose"),
        candid::encode_one(None::<CoseInit>).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(f.execute(&first), Ok(result));
    assert_eq!(f.execute(&refused), Err(Error::QuotaExceeded));
    f.ic.advance_time(Duration::from_secs(2 * 24 * 60 * 60));
    let mut next = f.grant(1, 65, 1);
    next.max_cycles = 1;
    let result = f.execute(&next).unwrap();
    assert_eq!(
        result.outcome,
        ExecutionOutcome::Failed(Error::QuotaExceeded)
    );
    assert_eq!(result.charged_cycles, 0);
    assert_eq!(f.result(&first), Err(Error::NotFound));
    assert_eq!(f.execute(&first), Err(Error::ResultExpired));
    assert_eq!(f.result(&neighbor), neighbor_result);
    f.ic.upgrade_canister(
        f.cose,
        wasm("dmsg_cose"),
        candid::encode_one(None::<CoseInit>).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(f.result(&next), Ok(result));
    assert_eq!(f.execute(&first), Err(Error::ResultExpired));
}

#[test]
fn cose_global_budget_survives_upgrade_without_consuming_rejected_sequences() {
    let f = CoseFixture::new(1);
    let mut first = f.grant(1, 1, 1);
    first.max_cycles = 1;
    let result = f.execute(&first).unwrap();
    assert_eq!(
        result.outcome,
        ExecutionOutcome::Failed(Error::QuotaExceeded)
    );
    let second = f.grant(2, 1, 1);
    assert_eq!(f.execute(&second), Err(Error::QuotaExceeded));
    assert_eq!(f.result(&second), Err(Error::NotFound));
    f.ic.upgrade_canister(
        f.cose,
        wasm("dmsg_cose"),
        candid::encode_one(None::<CoseInit>).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(f.execute(&first), Ok(result));
    assert_eq!(f.execute(&second), Err(Error::QuotaExceeded));
    f.ic.advance_time(Duration::from_secs(24 * 60 * 60));
    let accepted = f.grant(2, 1, 1);
    assert_eq!(
        f.execute(&accepted).unwrap().status(),
        ExecutionStatus::Completed
    );
}
