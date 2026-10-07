use super::*;

struct CoseFixture {
    ic: PocketIc,
    cose: Principal,
    home: Principal,
    config: CoseInit,
}

impl CoseFixture {
    fn new(daily_executions: u32) -> Self {
        Self::configured(daily_executions, false)
    }

    fn configured(daily_executions: u32, root: bool) -> Self {
        let mut algorithms = vec![Algorithm::Ed25519];
        if root {
            algorithms.push(Algorithm::VetKdBls12381);
        }
        Self::with_algorithms(daily_executions, algorithms)
    }

    fn with_algorithms(daily_executions: u32, algorithms: Vec<Algorithm>) -> Self {
        let ic = PocketIcBuilder::new()
            .with_application_subnet()
            .with_fiduciary_subnet()
            .build();
        let cose = ic.create_canister();
        let home = person(70);
        ic.add_cycles(cose, 10_000_000_000_000_000);
        let masters = algorithms
            .into_iter()
            .map(|algorithm| MasterKey {
                algorithm,
                key_name: "key_1".into(),
                expected_fingerprint: Hash::new([0; 32]),
            })
            .collect();
        let config = CoseInit {
            issuer_namespace: NAMESPACE.into(),
            environment: Environment::Local,
            executing_canister: cose,
            user_homes: vec![home],
            governance: person(71),
            derivation_version: 2,
            masters,
            daily_executions,
            daily_cycles: 10_000_000_000_000,
        };
        ic.install_canister(
            cose,
            wasm("dmsg_cose"),
            candid::encode_one(&config).unwrap(),
            None,
        );
        let initialized: Result<candid::Reserved> =
            update(&ic, cose, Principal::anonymous(), "initialize_keys", ());
        initialized.unwrap();
        Self {
            ic,
            cose,
            home,
            config,
        }
    }

    fn root(&self, account: u8, sequence: u64) -> ExecutionGrant {
        let mut grant = self.grant(account, sequence, 1);
        grant.commerce = None;
        grant.kind = ExecutionKind::Derive {
            generation: 1,
            root_op_id: None,
            transport_key: serde_bytes::ByteArray::new(
                ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32])
                    .unwrap()
                    .public_key()
                    .try_into()
                    .unwrap(),
            ),
        };
        grant
    }

    fn upgrade(&self) {
        self.ic
            .upgrade_canister(self.cose, wasm("dmsg_cose"), vec![], None)
            .unwrap();
    }

    /// Upgrade within one round without stopping the canister, as a careless
    /// controller could, so a pending management reply reaches the new module.
    fn upgrade_without_stopping(&self) {
        #[derive(CandidType, Deserialize)]
        enum Mode {
            #[serde(rename = "upgrade")]
            Upgrade(Option<()>),
        }
        #[derive(CandidType, Deserialize)]
        struct InstallCode {
            mode: Mode,
            canister_id: Principal,
            wasm_module: ByteBuf,
            arg: ByteBuf,
        }
        use std::io::Write;
        // The raw module exceeds one ingress message; gzip it to install at once.
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gzip.write_all(&wasm("dmsg_cose")).unwrap();
        let install = self
            .ic
            .submit_call_with_effective_principal(
                Principal::management_canister(),
                pocket_ic::common::rest::RawEffectivePrincipal::CanisterId(
                    self.cose.as_slice().to_vec(),
                ),
                Principal::anonymous(),
                "install_code",
                candid::encode_one(InstallCode {
                    mode: Mode::Upgrade(None),
                    canister_id: self.cose,
                    wasm_module: gzip.finish().unwrap().into(),
                    arg: candid::encode_args(()).unwrap().into(),
                })
                .unwrap(),
            )
            .unwrap();
        self.ic.await_call(install).unwrap();
    }

    fn stats(&self) -> CoseStats {
        query(
            &self.ic,
            self.cose,
            Principal::anonymous(),
            "cose_stats",
            (),
        )
    }

    fn account(&self, n: u8) -> AccountId {
        home_account(self.home, n)
    }

    fn grant(&self, account: u8, sequence: u64, body_bytes: usize) -> ExecutionGrant {
        let account_id = self.account(account);
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
            issuer: account_issuer(NAMESPACE, &account_id),
            subject: None,
            issued_at: None,
            content: StatementContent::Text("x".repeat(body_bytes)),
        };
        let (_, to_be_signed) =
            prepare_cose(&statement, &key.algorithm, &descriptor.key_id).unwrap();
        ExecutionGrant {
            commerce: Some(dmsg_types::billing::CommercialReservation {
                reservation_id: execution_request_id(&account_id, 0, Hash::new([1; 32]), sequence),
                month_utc: dmsg_protocol::billing::month_utc(time(&self.ic)).unwrap(),
                units: 1,
                weight_policy_version: 1,
                business_revision: 0,
                lease_revision: 1,
                valid_until_ms: time(&self.ic) + MINUTE,
            }),
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
// COSE's balance delta. The quoted cycles_cost_upper_bound value includes a response
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
                "cose_cycles history={history} method=execute total={total} upper_bound={} charged={}",
                result.cycles_cost_upper_bound, result.cycles_charged
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
        assert_eq!(result.outcome, ExecutionOutcome::Failed(Error::Expired));
        assert_eq!(result.cycles_cost_upper_bound, 0);
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
    for body_bytes in [1, 1024, 4096] {
        let f = CoseFixture::new(1000);
        let grant = f.grant(1, 1, body_bytes);
        let before = f.ic.cycle_balance(f.cose);
        assert_eq!(
            f.execute(&grant).unwrap().status(),
            ExecutionStatus::Completed
        );
        let cycles = before - f.ic.cycle_balance(f.cose);
        let status = f.ic.canister_status(f.cose, None).unwrap();
        println!(
            "cose_payload body_bytes={body_bytes} cycles={cycles} wasm_memory_bytes={}",
            status.memory_metrics.wasm_memory_size
        );
    }
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
#[ignore = "public-key and cleanup cycles comparison for dmsg_cose builds"]
fn cose_query_and_cleanup_cycles_profile() {
    let f = CoseFixture::with_algorithms(
        1000,
        vec![
            Algorithm::Ed25519,
            Algorithm::EcdsaSecp256k1,
            Algorithm::VetKdBls12381,
        ],
    );
    for selector in [
        KeySelector::Signing(dmsg_types::cose::SigningKey {
            purpose: SigningPurpose::Statement,
            algorithm: SigningAlgorithm::Ed25519,
        }),
        KeySelector::Signing(dmsg_types::cose::SigningKey {
            purpose: SigningPurpose::Statement,
            algorithm: SigningAlgorithm::EcdsaSecp256k1,
        }),
        KeySelector::AgentController { generation: 7 },
        KeySelector::ContentRoot { generation: 7 },
    ] {
        let args = (AccountId([1; 12]), selector.clone());
        let expected: Result<KeyDescriptor> =
            query(&f.ic, f.cose, f.home, "public_key", args.clone());
        // Replicated queries follow the same path used by the user canister;
        // measure their cycle balance delta rather than host/PocketIC latency.
        let before = f.ic.cycle_balance(f.cose);
        let actual: Result<KeyDescriptor> = update(&f.ic, f.cose, f.home, "public_key", args);
        let cycles = before - f.ic.cycle_balance(f.cose);
        assert_eq!(actual, expected);
        println!(
            "cose_public_key selector={selector:?} cycles={cycles} key={:?}",
            actual.unwrap().public_key
        );
    }
    for count in [0, 4, 8] {
        let f = CoseFixture::new(1000);
        for sequence in 1..=count {
            assert_eq!(
                f.execute(&f.grant(1, sequence, 4096)).unwrap().status(),
                ExecutionStatus::Completed
            );
        }
        let _: ExecutionCleanup = update(
            &f.ic,
            f.cose,
            Principal::anonymous(),
            "prune_executions",
            (None::<AccountId>,),
        );
        f.ic.advance_time(Duration::from_millis(2 * DAY));
        let before = f.ic.cycle_balance(f.cose);
        let cleaned: ExecutionCleanup = update(
            &f.ic,
            f.cose,
            Principal::anonymous(),
            "prune_executions",
            (None::<AccountId>,),
        );
        assert_eq!(u64::from(cleaned.results_removed), count);
        println!(
            "cose_cleanup results={count} cycles={}",
            before - f.ic.cycle_balance(f.cose)
        );
    }
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
        let mut expired = if sequence <= dmsg_runtime::FORMAL_EXECUTION_WINDOW as u64 {
            f.grant(1, sequence, 1)
        } else {
            f.root(1, sequence)
        };
        expired.approved_at -= MINUTE;
        expired.expires_at = time(&f.ic);
        let result = f.execute(&expired).unwrap();
        assert_eq!(result.outcome, ExecutionOutcome::Failed(Error::Expired));
        assert_eq!(result.cycles_cost_upper_bound, 0);
    }
    let refused = f.grant(1, 65, 1);
    assert_eq!(f.execute(&refused), Err(Error::QuotaExceeded));
    f.upgrade();
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
    assert_eq!(result.cycles_cost_upper_bound, 0);
    assert_eq!(f.result(&first), Err(Error::NotFound));
    assert_eq!(f.execute(&first), Err(Error::ResultExpired));
    assert_eq!(f.result(&neighbor), neighbor_result);
    f.upgrade();
    assert_eq!(f.result(&next), Ok(result));
    assert_eq!(f.execute(&first), Err(Error::ResultExpired));
}

#[test]
fn cose_global_budget_survives_upgrade_without_consuming_rejected_sequences() {
    let f = CoseFixture::new(2);
    let first = f.grant(1, 1, 1);
    let result = f.execute(&first).unwrap();
    assert_eq!(result.status(), ExecutionStatus::Completed);
    let second = f.grant(2, 1, 1);
    assert_eq!(f.execute(&second), Err(Error::QuotaExceeded));
    assert_eq!(f.result(&second), Err(Error::NotFound));
    // A request that sends no management call closes without the exhausted budget.
    let mut rejected = f.grant(3, 1, 1);
    rejected.max_cycles = 1;
    assert_eq!(
        f.execute(&rejected).unwrap().outcome,
        ExecutionOutcome::Failed(Error::QuotaExceeded)
    );
    f.upgrade();
    assert_eq!(f.execute(&first), Ok(result));
    assert_eq!(f.execute(&second), Err(Error::QuotaExceeded));
    f.ic.advance_time(Duration::from_secs(24 * 60 * 60));
    let accepted = f.grant(2, 1, 1);
    assert_eq!(
        f.execute(&accepted).unwrap().status(),
        ExecutionStatus::Completed
    );
}

#[test]
fn cose_unsent_calls_fail_without_trapping_or_consuming_budgets() {
    for formal in [false, true] {
        let f = CoseFixture::configured(if formal { 2 } else { 1 }, true);
        // Allocate the result/home tables and exercise key derivation before
        // reducing liquid cycles, so this probes dispatch rather than memory growth.
        let mut warmup = f.grant(1, 1, 1);
        warmup.max_cycles = 1;
        assert_eq!(
            f.execute(&warmup).unwrap().outcome,
            ExecutionOutcome::Failed(Error::QuotaExceeded)
        );
        let status = f.ic.canister_status(f.cose, None).unwrap();
        let idle: u128 = status.idle_cycles_burned_per_day.0.try_into().unwrap();
        let threshold = (f.ic.cycle_balance(f.cose) - 50_000_000_000) * 86_400 / idle;
        f.ic.update_canister_settings(
            f.cose,
            None,
            pocket_ic::CanisterSettings {
                freezing_threshold: Some(Nat::from(threshold)),
                ..Default::default()
            },
        )
        .unwrap();
        let grant = if formal {
            f.grant(1, 2, 1)
        } else {
            f.root(1, 2)
        };
        let failed = f.execute(&grant).unwrap();
        assert!(matches!(
            &failed.outcome,
            ExecutionOutcome::Failed(Error::Unavailable(detail))
                if detail.contains("InsufficientLiquidCycleBalance")
        ));
        assert_eq!(
            (failed.cycles_cost_upper_bound, failed.cycles_charged),
            (0, 0)
        );
        assert_eq!(f.result(&grant), Ok(failed.clone()));
        f.ic.add_cycles(f.cose, 1_000_000_000_000_000);
        f.upgrade();
        // A known failure is retained, not dispatched again after the top-up.
        assert_eq!(f.execute(&grant), Ok(failed));
        let next = if formal {
            f.grant(1, 3, 1)
        } else {
            f.root(1, 3)
        };
        assert_eq!(
            f.execute(&next).unwrap().status(),
            ExecutionStatus::Completed
        );
    }
}

#[test]
fn cose_authenticates_the_configured_home_before_new_requests_and_replays() {
    let f = CoseFixture::new(1000);
    let grant = f.grant(1, 1, 1);
    let check = || {
        // A Principal that is no configured home is refused before execution.
        assert_refused(&f.ic, f.cose, person(99), "execute", (&grant,));
        for wrong_cose in [false, true] {
            let mut wrong = grant.clone();
            if wrong_cose {
                wrong.home_cose = person(99);
            } else {
                wrong.home_user = person(99);
            }
            assert_eq!(f.execute(&wrong), Err(Error::Forbidden));
        }
    };
    check();
    assert_eq!(f.result(&grant), Err(Error::NotFound));
    assert_eq!(
        f.execute(&grant).unwrap().status(),
        ExecutionStatus::Completed
    );
    check();
    let denied: Result<ExecutionResult> = query(
        &f.ic,
        f.cose,
        person(99),
        "get_execution",
        (&grant.account_id, grant.request_id),
    );
    assert_eq!(denied, Err(Error::Forbidden));
    // Proposals to add a home show how much shared capacity is in use.
    let rendered: std::result::Result<String, String> = query(
        &f.ic,
        f.cose,
        person(99),
        "validate_admin_add_user_home",
        (person(80),),
    );
    assert!(rendered
        .unwrap()
        .ends_with("All homes share this executor's 1 of 1000000 accounts."));
    let rendered: std::result::Result<String, String> = query(
        &f.ic,
        f.cose,
        person(99),
        "validate_admin_add_user_home",
        (f.home,),
    );
    assert!(rendered
        .unwrap()
        .ends_with("1 of 1000000 accounts. Already listed; no change."));
}

#[test]
fn cose_returned_unknown_survives_upgrade_without_blocking_later_cleanup() {
    use dmsg_runtime::storage::compact_bytes;
    use ic_stable_structures::{
        memory_manager::{MemoryId, MemoryManager},
        StableBTreeMap,
    };
    use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

    let f = CoseFixture::configured(1000, true);
    let first = f.grant(1, 1, 1);
    let mut unknown = f.execute(&first).unwrap();
    assert_eq!(unknown.status(), ExecutionStatus::Completed);
    unknown.outcome = ExecutionOutcome::Unknown(Error::ExecutionUnknown);
    // Fault injection into schema 8: emulate a returned unknown management
    // outcome, without adding any test-only entry point to the production Wasm.
    let memory = Rc::new(RefCell::new(f.ic.get_stable_memory(f.cose)));
    let manager = MemoryManager::init(memory.clone());
    let mut homes = StableBTreeMap::<Vec<u8>, Vec<u8>, _>::init(manager.get(MemoryId::new(1)));
    let account_key = first.account_id.to_vec();
    let mut home: BTreeMap<u8, cbor2::Value> =
        cbor2::from_slice(&homes.get(&account_key).unwrap()).unwrap();
    let mut metadata: BTreeMap<u64, BTreeMap<u8, cbor2::Value>> =
        cbor2::from_slice(&cbor2::to_vec(&home[&4]).unwrap()).unwrap();
    let state = metadata.get_mut(&1).unwrap().get_mut(&4).unwrap();
    assert_eq!(*state, cbor2::Value::Text("Terminal".into()));
    *state = cbor2::Value::Text("Unknown".into());
    home.insert(
        4,
        cbor2::from_slice(&cbor2::to_vec(&metadata).unwrap()).unwrap(),
    );
    homes.insert(account_key.clone(), cbor2::to_vec(&home).unwrap());
    let mut results = StableBTreeMap::<Vec<u8>, Vec<u8>, _>::init(manager.get(MemoryId::new(2)));
    results.insert(
        [account_key, 1u64.to_be_bytes().to_vec()].concat(),
        compact_bytes(&unknown),
    );
    drop((homes, results, manager));
    f.ic.set_stable_memory(
        f.cose,
        memory.borrow().clone(),
        pocket_ic::common::rest::BlobCompression::NoCompression,
    );
    f.upgrade();
    assert_eq!(f.execute(&first), Ok(unknown.clone()));
    let mut conflict = first.clone();
    conflict.max_cycles -= 1;
    assert_eq!(f.execute(&conflict), Err(Error::IdempotencyConflict));
    for sequence in 2..=64 {
        let mut expired = if sequence <= dmsg_runtime::FORMAL_EXECUTION_WINDOW as u64 {
            f.grant(1, sequence, 1)
        } else {
            f.root(1, sequence)
        };
        expired.approved_at -= MINUTE;
        expired.expires_at = time(&f.ic);
        assert_eq!(
            f.execute(&expired).unwrap().outcome,
            ExecutionOutcome::Failed(Error::Expired)
        );
    }
    f.ic.advance_time(Duration::from_millis(30 * DAY));
    let cleaned: ExecutionCleanup = update(
        &f.ic,
        f.cose,
        Principal::anonymous(),
        "prune_executions",
        (None::<AccountId>,),
    );
    assert_eq!(cleaned.results_removed, 63);
    f.upgrade();
    assert_eq!(f.result(&first), Ok(unknown.clone()));
    assert_eq!(f.execute(&first), Ok(unknown));
    assert_eq!(
        f.execute(&f.root(1, 65)).unwrap().status(),
        ExecutionStatus::Completed
    );
}

#[test]
fn cose_small_deployment_budgets_leave_a_real_root_operation() {
    for limit in [1u32, 4, 5, 6] {
        let f = CoseFixture::configured(limit, true);
        let formal_limit = limit - limit.div_ceil(5);
        for n in 1..=formal_limit {
            assert_eq!(
                f.execute(&f.grant(n as u8, 1, 1)).unwrap().status(),
                ExecutionStatus::Completed
            );
        }
        let denied = f.grant(20, 1, 1);
        assert_eq!(f.execute(&denied), Err(Error::QuotaExceeded));
        assert_eq!(f.result(&denied), Err(Error::NotFound));
        let root = f.root(20, 1);
        assert_eq!(
            f.execute(&root).unwrap().status(),
            ExecutionStatus::Completed
        );
        if limit <= 5 {
            assert_eq!(f.execute(&f.root(21, 1)), Err(Error::QuotaExceeded));
        }
    }
}

#[test]
fn cose_budget_change_keeps_key_identity_and_current_usage() {
    let f = CoseFixture::new(2);
    let first = f.grant(1, 1, 1);
    let completed = f.execute(&first).unwrap();
    // Three 128-page buckets plus the memory-manager header; no fourth budget bucket.
    assert!(f.ic.get_stable_memory(f.cose).len() <= (1 + 3 * 128) * 65_536);
    let second = f.grant(2, 1, 1);
    assert_eq!(f.execute(&second), Err(Error::QuotaExceeded));
    let governance = f.config.governance;
    let set_budget = |caller: Principal, executions: u32, cycles: u128| -> Result<()> {
        update(
            &f.ic,
            f.cose,
            caller,
            "admin_set_daily_budget",
            (executions, cycles),
        )
    };
    let cycles = f.config.daily_cycles;
    let rendered: std::result::Result<String, String> = query(
        &f.ic,
        f.cose,
        Principal::anonymous(),
        "validate_admin_set_daily_budget",
        (4u32, cycles * 2),
    );
    assert!(rendered.unwrap().contains("4 executions"));
    let rejected: std::result::Result<String, String> = query(
        &f.ic,
        f.cose,
        Principal::anonymous(),
        "validate_admin_set_daily_budget",
        (0u32, cycles),
    );
    assert!(rejected.is_err());
    assert_refused(
        &f.ic,
        f.cose,
        f.home,
        "admin_set_daily_budget",
        (4u32, cycles * 2),
    );
    assert!(matches!(
        set_budget(governance, 0, cycles),
        Err(Error::InvalidInput(_))
    ));
    set_budget(governance, 4, cycles * 2).unwrap();
    f.upgrade();
    assert_eq!(f.execute(&first), Ok(completed));
    assert_eq!(
        f.execute(&second).unwrap().status(),
        ExecutionStatus::Completed
    );
    assert_eq!(
        f.execute(&f.grant(3, 1, 1)).unwrap().status(),
        ExecutionStatus::Completed
    );
    assert_eq!(f.execute(&f.grant(4, 1, 1)), Err(Error::QuotaExceeded));
    // Lowering the limit must not clear already-used counts.
    set_budget(governance, 2, cycles).unwrap();
    assert_eq!(f.execute(&f.grant(4, 1, 1)), Err(Error::QuotaExceeded));
    let state: KeyState = query(&f.ic, f.cose, Principal::anonymous(), "key_state", ());
    assert_eq!(state.config, f.config);
    // The retained successful result keeps its conservative cost upper bound.
    assert!(f.result(&first).unwrap().cycles_cost_upper_bound > 0);
}

#[test]
fn cose_idle_cleanup_is_bounded_public_and_preserves_replay_guards() {
    let f = CoseFixture::new(1000);
    let mut grants = vec![];
    for account in 1..=70 {
        let mut grant = f.grant(account, 1, 1);
        grant.max_cycles = 1;
        f.execute(&grant).unwrap();
        grants.push(grant);
    }
    let prune = |after: Option<AccountId>| -> ExecutionCleanup {
        update(&f.ic, f.cose, person(72), "prune_executions", (after,))
    };
    let early = prune(None);
    assert_eq!((early.homes_scanned, early.results_removed), (64, 0));
    f.ic.advance_time(Duration::from_millis(2 * DAY));
    let first = prune(None);
    assert_eq!((first.homes_scanned, first.results_removed), (64, 64));
    assert_eq!(first.next_after, Some(f.account(64)));
    assert!(f.result(&grants[64]).is_ok());
    f.upgrade();
    let last = prune(first.next_after);
    assert_eq!((last.homes_scanned, last.results_removed), (6, 6));
    assert_eq!(last.next_after, None);
    for grant in grants {
        assert_eq!(f.result(&grant), Err(Error::NotFound));
        assert_eq!(f.execute(&grant), Err(Error::ResultExpired));
    }
    assert_eq!(
        f.execute(&f.grant(1, 2, 1)).unwrap().status(),
        ExecutionStatus::Completed
    );
}

#[test]
fn cose_settles_budgets_to_the_charged_fee() {
    let f = CoseFixture::configured(1000, true);
    let signed = f.execute(&f.grant(1, 1, 1)).unwrap();
    assert_eq!(signed.status(), ExecutionStatus::Completed);
    assert!(signed.cycles_charged > 0 && signed.cycles_charged < signed.cycles_cost_upper_bound);
    let root = f.execute(&f.root(2, 1)).unwrap();
    assert_eq!(root.status(), ExecutionStatus::Completed);
    assert!(root.cycles_charged > 0 && root.cycles_charged < root.cycles_cost_upper_bound);
    println!(
        "cose_charged sign={} sign_upper_bound={} root={} root_upper_bound={}",
        signed.cycles_charged,
        signed.cycles_cost_upper_bound,
        root.cycles_charged,
        root.cycles_cost_upper_bound
    );
    let stats = f.stats();
    assert_eq!(
        (
            stats.accounts,
            stats.max_accounts,
            stats.results,
            stats.in_flight,
            stats.unknown
        ),
        (2, 1_000_000, 2, 0, 0)
    );
    assert_eq!(stats.budget_day, time(&f.ic) / DAY);
    assert_eq!(
        (stats.executions_today, stats.cycles_today),
        (2, signed.cycles_charged + root.cycles_charged)
    );
    assert_eq!(
        (stats.formal_executions_today, stats.formal_cycles_today),
        (1, signed.cycles_charged)
    );
    assert!(stats.stable_pages > 0 && stats.cycles > 0);
    // At the upper bound one account fit 12 signatures a day (880B / 68.3B);
    // the settled fee leaves room for many more.
    for sequence in 2..=20 {
        assert_eq!(
            f.execute(&f.grant(1, sequence, 1)).unwrap().status(),
            ExecutionStatus::Completed
        );
    }
    assert_eq!(f.stats().formal_cycles_today, 20 * signed.cycles_charged);
}

#[test]
fn cose_upgrade_without_stopping_records_abandoned_calls_as_unknown() {
    let f = CoseFixture::new(1000);
    let first = f.grant(1, 1, 1);
    let call =
        f.ic.submit_call(
            f.cose,
            f.home,
            "execute",
            candid::encode_args((&first,)).unwrap(),
        )
        .unwrap();
    f.ic.tick();
    assert_eq!(
        f.result(&first).unwrap().status(),
        ExecutionStatus::Executing
    );
    assert_eq!(f.stats().in_flight, 1);
    f.upgrade_without_stopping();
    for _ in 0..40 {
        f.ic.tick();
    }
    // The reply reached the new module, which has no callback for it.
    assert!(f.ic.await_call(call).is_err());
    let unknown = ExecutionResult {
        outcome: ExecutionOutcome::Unknown(Error::ExecutionUnknown),
        ..f.result(&first).unwrap()
    };
    // The query reports what the next update records.
    assert_eq!(f.result(&first), Ok(unknown.clone()));
    assert_eq!(f.stats().unknown, 0);
    assert_eq!(
        f.execute(&f.grant(1, 2, 1)).unwrap().status(),
        ExecutionStatus::Completed
    );
    let stats = f.stats();
    assert_eq!((stats.in_flight, stats.unknown), (0, 1));
    // The original request is not signed again, and the window moves on:
    // sequence 65 would be refused while sequence 1 stayed in flight.
    assert_eq!(f.execute(&first), Ok(unknown));
    let mut expired = f.root(1, 65);
    expired.approved_at -= MINUTE;
    expired.expires_at = time(&f.ic);
    assert_eq!(
        f.execute(&expired).unwrap().outcome,
        ExecutionOutcome::Failed(Error::Expired)
    );
}

#[test]
fn cose_offline_pins_match_initialized_production_keys() {
    use dmsg_protocol::cose_pins::{master_key_pins, KeySource};

    let ic = PocketIcBuilder::new()
        .with_application_subnet()
        .with_fiduciary_subnet()
        .build();
    let algorithms = [
        Algorithm::Ed25519,
        Algorithm::EcdsaSecp256k1,
        Algorithm::VetKdBls12381,
    ];
    for tampered in [false, true] {
        let cose = ic.create_canister();
        ic.add_cycles(cose, 10_000_000_000_000_000);
        let mut masters = master_key_pins(
            KeySource::PocketIc,
            cose,
            &Environment::Production,
            "key_1",
            &algorithms,
        )
        .unwrap();
        let pins: Vec<Hash> = masters.iter().map(|m| m.expected_fingerprint).collect();
        if tampered {
            masters[2].expected_fingerprint = Hash::new([1; 32]);
        }
        let config = CoseInit {
            issuer_namespace: NAMESPACE.into(),
            environment: Environment::Production,
            executing_canister: cose,
            user_homes: vec![person(70)],
            governance: person(71),
            derivation_version: 2,
            masters,
            daily_executions: 10,
            daily_cycles: 10_000_000_000_000,
        };
        ic.install_canister(
            cose,
            wasm("dmsg_cose"),
            candid::encode_one(&config).unwrap(),
            None,
        );
        let initialized: Result<KeyState> =
            update(&ic, cose, Principal::anonymous(), "initialize_keys", ());
        if tampered {
            assert_eq!(initialized, Err(Error::IntegrityFailed));
        } else {
            let state = initialized.unwrap();
            assert_eq!(state.initialization, Initialization::Ready);
            assert_eq!(state.fingerprints, pins);
        }
    }
}

// A full public cleanup page: 64 idle accounts, each with 64 expired results.
// They carry no payload; a removal also reads the result and rewrites the rest
// of its leaf, so larger signature results cost more. Run with --ignored after
// building the release Wasm.
#[test]
#[ignore = "cleanup page profile for dmsg_cose builds"]
fn cose_cleanup_page_profile() {
    let f = CoseFixture::new(1000);
    for account in 1..=64u8 {
        for sequence in 1..=64 {
            let mut expired = f.root(account, sequence);
            expired.approved_at -= MINUTE;
            expired.expires_at = time(&f.ic);
            assert_eq!(
                f.execute(&expired).unwrap().outcome,
                ExecutionOutcome::Failed(Error::Expired)
            );
        }
    }
    f.ic.advance_time(Duration::from_millis(2 * DAY));
    let before = f.ic.cycle_balance(f.cose);
    let cleaned: ExecutionCleanup = update(
        &f.ic,
        f.cose,
        Principal::anonymous(),
        "prune_executions",
        (None::<AccountId>,),
    );
    assert_eq!((cleaned.homes_scanned, cleaned.results_removed), (64, 4096));
    println!(
        "cose_cleanup_page homes=64 results=4096 cycles={}",
        before - f.ic.cycle_balance(f.cose)
    );
}

// One cleanup page of signature results: the first 64 of 66 idle accounts,
// each with 32 expired 4 KiB statement signatures (a day's formal budget of
// one account). Run with --ignored after building the release Wasm.
#[test]
#[ignore = "signature cleanup page profile for dmsg_cose builds"]
fn cose_signature_cleanup_page_profile() {
    let f = CoseFixture::new(1000);
    let budget: Result<()> = update(
        &f.ic,
        f.cose,
        f.config.governance,
        "admin_set_daily_budget",
        (10_000u32, 100_000_000_000_000u128),
    );
    budget.unwrap();
    for account in 1..=66u8 {
        for sequence in 1..=32 {
            assert_eq!(
                f.execute(&f.grant(account, sequence, 4096))
                    .unwrap()
                    .status(),
                ExecutionStatus::Completed
            );
        }
    }
    f.ic.advance_time(Duration::from_millis(2 * DAY));
    let before = f.ic.cycle_balance(f.cose);
    let cleaned: ExecutionCleanup = update(
        &f.ic,
        f.cose,
        Principal::anonymous(),
        "prune_executions",
        (None::<AccountId>,),
    );
    assert_eq!((cleaned.homes_scanned, cleaned.results_removed), (64, 2048));
    println!(
        "cose_signature_cleanup_page homes=64 results=2048 cycles={}",
        before - f.ic.cycle_balance(f.cose)
    );
}
