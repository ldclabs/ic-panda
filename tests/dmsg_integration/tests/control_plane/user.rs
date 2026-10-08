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
    assert_eq!(charged.charged_units, 1);
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
    f.prune_executions();
    let kept: Result<SignedArtifact> = query(
        &f.ic,
        f.user,
        person(1),
        "get_attestation",
        (&id, request.approval.request_id),
    );
    assert!(kept.is_ok());
    f.ic.advance_time(Duration::from_millis(2 * DAY));
    f.prune_executions();
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
    assert_eq!(usage(&f, &id), before);
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
    assert_eq!(usage.charged_units, 0);
}

#[test]
fn month_ledgers_keep_the_current_and_previous_month() {
    let f = Fixture::new();
    let id = f.create(1);
    let mut months = vec![];
    for _ in 0..3 {
        let usage: Result<dmsg_types::billing::ExecutionUsage> = update(
            &f.ic,
            f.user,
            person(1),
            "refresh_execution_entitlement",
            (&id,),
        );
        months.push(usage.unwrap().month_utc);
        f.ic.advance_time(Duration::from_millis(32 * DAY));
    }
    let row = |month: u32| -> Result<dmsg_types::billing::ExecutionUsage> {
        query(
            &f.ic,
            f.user,
            person(1),
            "get_execution_usage",
            (&id, month),
        )
    };
    assert_eq!(row(months[0]).map(|u| u.month_utc), Err(Error::NotFound));
    assert_eq!(row(months[1]).map(|u| u.month_utc), Ok(months[1]));
    assert_eq!(row(months[2]).map(|u| u.month_utc), Ok(months[2]));
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
    let kept: Result<dmsg_types::billing::ExecutionUsage> = query(
        &f.ic,
        f.user,
        person(*owner),
        "get_execution_usage",
        (id, month),
    );
    let kept = kept.unwrap();
    assert_eq!(kept.account_id, *id);
    assert_eq!(kept.month_utc, month);
    println!(
        "user_upgrade accounts={} month_rows={} cycles={cycles} stable_bytes={stable_bytes} wasm_bytes={wasm_bytes}",
        accounts.len(),
        accounts.len() * months.min(2)
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
    let bind = |n: u8, nonce: u8| f.bind(1, n, &id, Hash::new([nonce; 32]));
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

// Loads the host images of dmsg_user::capacity (every fifth account active)
// and measures the user canister alone. Two steps:
//   1. run this test once; it prints the canister ID to build images for;
//   2. DMSG_USER_IMAGE_DIR=<dir> DMSG_USER_IMAGE_CANISTER=<id> cargo test --release
//      -p dmsg_user --lib capacity_image -- --ignored --nocapture
//   then run this test again with DMSG_USER_IMAGE_DIR.
#[test]
#[ignore = "capacity profile over host-built dmsg_user images"]
fn user_capacity_profile() {
    // Matches dmsg_user::capacity.
    let at = 1_800_000_000_000u64;
    let login = |i: u64, n: u64| {
        Principal::self_authenticating(digest("capacity login", &(i, n)).as_slice())
    };
    let device_key = |i: u64, n: u64| SigningKey::from_bytes(&digest("capacity device", &(i, n)));
    let device_id = |i: u64, n: u64| digest("capacity device id", &(i, n));
    let dir = std::env::var_os("DMSG_USER_IMAGE_DIR").map(PathBuf::from);
    for accounts in [10_000u64, 1_000_000] {
        // Compress before starting PocketIC: its server stops after a minute idle.
        let image = dir
            .as_ref()
            .and_then(|d| std::fs::read(d.join(format!("user-{accounts}.bin"))).ok())
            .map(|image| {
                let mut gzip =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
                std::io::Write::write_all(&mut gzip, &image).unwrap();
                (image.len(), gzip.finish().unwrap())
            });
        let ic = PocketIcBuilder::new().with_application_subnet().build();
        ic.set_time(pocket_ic::Time::from_nanos_since_unix_epoch(
            millis_to_nanos(at + MINUTE).unwrap(),
        ));
        let user = ic.create_canister();
        let Some((image_bytes, gzip)) = image else {
            println!("user_capacity build images with DMSG_USER_IMAGE_CANISTER={user}");
            continue;
        };
        ic.add_cycles(user, 100_000_000_000_000_000);
        // An empty module has no hooks that could write over the image.
        ic.install_canister(user, b"\0asm\x01\0\0\0".to_vec(), vec![], None);
        ic.set_stable_memory(user, gzip, pocket_ic::common::rest::BlobCompression::Gzip);
        let cycles = ic.cycle_balance(user);
        ic.upgrade_canister(
            user,
            wasm("dmsg_user"),
            candid::encode_args(()).unwrap(),
            None,
        )
        .unwrap();
        let upgrade_cycles = cycles - ic.cycle_balance(user);
        let upgrade = ic
            .fetch_canister_logs(user, Principal::anonymous())
            .unwrap()
            .into_iter()
            .map(|l| String::from_utf8_lossy(&l.content).into_owned())
            .find(|l| l.contains("dmsg_user upgrade"))
            .unwrap();
        let spent = |sender: Principal, method: &str, args: Vec<u8>| -> (u128, Vec<u8>) {
            let cycles = ic.cycle_balance(user);
            let reply = ic.update_call(user, sender, method, args).unwrap();
            (cycles - ic.cycle_balance(user), reply)
        };
        // Active accounts sampled across the whole ID range.
        let active: Vec<(u64, AccountId)> = (0..64)
            .map(|k| {
                let i = 5 * (k * (accounts / 5 / 64));
                let id: Option<AccountId> = query(&ic, user, login(i, 0), "my_account", ());
                (i, id.unwrap())
            })
            .collect();
        let batch: Vec<AccountId> = active.iter().map(|(_, id)| *id).collect();
        let began = std::time::Instant::now();
        let certified: Result<CertifiedBatch> = query(
            &ic,
            user,
            Principal::anonymous(),
            "security_snapshot_batch",
            (&batch,),
        );
        let query_ms = began.elapsed().as_millis();
        assert!(certified.unwrap().entries.iter().all(|e| e.value.is_some()));
        // Replicated, so its instructions are charged; inspect admits a login.
        let (batch_cycles, _) = spent(
            person(250),
            "security_snapshot_batch",
            candid::encode_args((&batch,)).unwrap(),
        );

        // A new account.
        let new_login = Principal::self_authenticating([201; 32]);
        let input = {
            let expires = at + 2 * MINUTE;
            let dev = DeviceInput {
                device_id: Hash::new([201; 32]),
                signing_pub: key(201).verifying_key().to_bytes().into(),
                ..device(201)
            };
            let op = Hash::new([201; 32]);
            let proof = key(201)
                .sign(
                    digest(
                        "dmsg/create-account/v1",
                        &(user, new_login, &dev, op, expires),
                    )
                    .as_slice(),
                )
                .to_bytes()
                .into();
            CreateAccount {
                device: dev,
                op_id: op,
                expires_at: expires,
                proof,
                admission: None,
            }
        };
        let (create_cycles, reply) = spent(
            new_login,
            "create_account",
            candid::encode_args((input,)).unwrap(),
        );
        let created: Result<AccountId> = candid::decode_one(&reply).unwrap();
        created.unwrap();

        // A mutation and an attestation of an active account; the image's
        // month lease is current, so neither leaves the canister.
        let (i, id) = active[17];
        let info = || -> AccountInfo {
            let r: Result<AccountInfo> = query(&ic, user, login(i, 0), "get_account", (&id,));
            r.unwrap()
        };
        let s = info();
        let admin = device_id(i, 0);
        let mut m = AccountMutation {
            account_id: id,
            expected_version: s.account_version,
            command: AccountCommand::SetRecoveryDelay { delay_ms: 2 * DAY },
            approval: Approval {
                device_id: admin,
                security_epoch: s.security_epoch,
                sequence: s.devices[&admin].next_sequence,
                request_id: digest("capacity mutation", &i),
                expires_at: at + 2 * MINUTE,
                signature: Default::default(),
            },
        };
        m.approval.signature = device_key(i, 0)
            .sign(
                approval_message(
                    user,
                    &id,
                    "dmsg/account/v2",
                    &(&m.expected_version, &m.command),
                    &m.approval,
                )
                .as_slice(),
            )
            .to_bytes()
            .into();
        let (mutate_cycles, reply) = spent(
            login(i, 0),
            "mutate_account",
            candid::encode_args((m,)).unwrap(),
        );
        let mutated: Result<OperationReceipt> = candid::decode_one(&reply).unwrap();
        mutated.unwrap();
        let s = info();
        let statement = Statement {
            issuer: s.issuer.clone(),
            subject: None,
            issued_at: None,
            content: StatementContent::Text("x".repeat(256)),
        };
        let key = device_key(i, 0);
        let prepared =
            prepare_attestation(&statement, &key.verifying_key().to_bytes().into()).unwrap();
        let sequence = s.devices[&admin].next_sequence;
        let mut request = AttestRequest {
            account_id: id,
            statement,
            origin: "https://example.com".into(),
            signature: key.sign(&prepared.to_be_signed).to_bytes().into(),
            approval: Approval {
                device_id: admin,
                security_epoch: s.security_epoch,
                sequence,
                request_id: execution_request_id(&id, s.security_epoch, admin, sequence),
                expires_at: at + 2 * MINUTE,
                signature: Default::default(),
            },
        };
        request.approval.signature = key
            .sign(attest_approval(user, &request).as_slice())
            .to_bytes()
            .into();
        let (attest_cycles, reply) = spent(
            login(i, 0),
            "attest",
            candid::encode_args((request,)).unwrap(),
        );
        let attested: Result<SignedArtifact> = candid::decode_one(&reply).unwrap();
        attested.unwrap();

        // Public cleanup: the first pages of expired attestations.
        let mut cursor = ByteBuf::new();
        let mut prune_cycles = 0;
        for _ in 0..10 {
            let (cycles, reply) = spent(
                Principal::anonymous(),
                "prune_executions",
                candid::encode_args((cursor,)).unwrap(),
            );
            prune_cycles += cycles;
            cursor = candid::decode_one::<Option<ByteBuf>>(&reply)
                .unwrap()
                .unwrap();
        }

        let memory = ic.canister_status(user, None).unwrap().memory_metrics;
        println!(
            "user_capacity accounts={accounts} image_bytes={image_bytes} stable_bytes={} \
             upgrade_cycles={upgrade_cycles} create_cycles={create_cycles} \
             mutate_cycles={mutate_cycles} attest_cycles={attest_cycles} \
             snapshot_batch64_cycles={batch_cycles} snapshot_batch64_query_ms={query_ms} \
             prune_page_cycles={}",
            memory.stable_memory_size,
            prune_cycles / 10,
        );
        println!("{upgrade}");
    }
}
