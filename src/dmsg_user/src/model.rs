use candid::Principal;
use dmsg_types::{cose::*, handle::*, user::*, *};
use std::collections::BTreeMap;

pub fn create(
    id: Principal,
    cose: Principal,
    subject_id: Hash,
    caller: Principal,
    input: &CreateSubject,
    now: u64,
) -> Result<Subject> {
    authenticated(caller)?;
    input.device.validate()?;
    nonzero(&input.op_id)?;
    expiry(now, input.expires_at, 5 * MINUTE)?;
    ensure(
        input.device.role == ControllerRole::Administrator
            && input.device.capabilities.contains(&Capability::RootManage),
        invalid("initial root administrator"),
    )?;
    verify(
        &input.device.signing_pub,
        &digest(
            "dmsg/create-subject/v1",
            &(id, caller, &input.device, input.op_id, input.expires_at),
        ),
        &input.proof,
    )?;
    Ok(Subject {
        subject_id,
        home_user: id,
        home_cose: cose,
        auth_bindings: vec![caller],
        account_version: 0,
        security_epoch: 0,
        status: AccountStatus::Active,
        devices: BTreeMap::from([(
            input.device.device_id,
            Device {
                input: input.device.clone(),
                added_at: now,
                added_by: None,
                revoked_at: None,
                next_sequence: 0,
            },
        )]),
        recovery: None,
        recovery_checked: false,
        recovery_nonce: 0,
        pending_recovery: None,
        current_root: None,
        root_slot: None,
        next_root_generation: 1,
        vault_write_state: VaultWriteState::Uninitialized,
        sensitive_policy: SensitivePolicy::default(),
        budget: Budget::default(),
        next_execution_sequence: 1,
        operations: vec![],
        handle_authorizations: BTreeMap::new(),
        executions: BTreeMap::new(),
    })
}

pub fn check_device<'a, T: serde::Serialize>(
    s: &'a Subject,
    caller: Principal,
    approval: &Approval,
    domain: &str,
    payload: &T,
    required: (Option<Capability>, bool),
    now: u64,
) -> Result<&'a Device> {
    let (capability, admin) = required;
    authenticated(caller)?;
    ensure(s.auth_bindings.contains(&caller), Error::AuthRequired)?;
    let device = s
        .devices
        .get(&approval.device_id)
        .ok_or(Error::DeviceNotApproved)?;
    ensure(device.revoked_at.is_none(), Error::DeviceNotApproved)?;
    ensure(
        capability
            .as_ref()
            .is_none_or(|c| device.input.capabilities.contains(c))
            && (!admin || device.input.role == ControllerRole::Administrator),
        Error::Forbidden,
    )?;
    ensure(
        approval.security_epoch == s.security_epoch,
        Error::PolicyStale,
    )?;
    check_sequence(device.next_sequence, approval.sequence)?;
    expiry(now, approval.expires_at, 5 * MINUTE)?;
    nonzero(&approval.request_id)?;
    verify(
        &device.input.signing_pub,
        &approval_message(s.home_user, s.subject_id, domain, payload, approval),
        &approval.signature,
    )?;
    Ok(device)
}
fn changed(s: &mut Subject) {
    s.security_epoch = s
        .security_epoch
        .checked_add(1)
        .expect("security epoch exhausted");
    s.root_slot = None;
    if s.current_root.is_some() {
        s.vault_write_state = VaultWriteState::RekeyRequired;
    }
}
fn finish(s: &mut Subject, a: &Approval, fingerprint: Hash) -> OperationReceipt {
    s.devices
        .get_mut(&a.device_id)
        .expect("authorized device")
        .next_sequence += 1;
    s.account_version = s.account_version.checked_add(1).expect("version exhausted");
    let receipt = OperationReceipt {
        id: a.request_id,
        digest: fingerprint,
        account_version: s.account_version,
    };
    s.operations.push(receipt.clone());
    if s.operations.len() > WINDOW {
        s.operations.remove(0);
    }
    receipt
}
pub fn apply(
    s: &mut Subject,
    caller: Principal,
    m: &AccountMutation,
    now: u64,
    handle_canister: Principal,
) -> Result<OperationReceipt> {
    ensure(
        s.subject_id == m.subject && s.auth_bindings.contains(&caller),
        Error::AuthRequired,
    )?;
    let fp = digest("dmsg/account-operation/v1", m);
    if let Some(r) = s.operations.iter().find(|r| r.id == m.approval.request_id) {
        ensure(r.digest == fp, Error::IdempotencyConflict)?;
        return Ok(r.clone());
    }
    // Work on a candidate: errors cannot persist a partial device/root change.
    let mut next = s.clone();
    let payload = (&m.expected_version, &m.command);
    check_device(
        s,
        caller,
        &m.approval,
        "dmsg/account/v1",
        &payload,
        if matches!(m.command, AccountCommand::DisputeRecovery { .. }) {
            (None, false)
        } else {
            (Some(Capability::RootManage), true)
        },
        now,
    )?;
    ensure(
        m.expected_version == s.account_version,
        Error::VersionConflict,
    )?;
    if s.status != AccountStatus::Active {
        ensure(
            matches!(m.command, AccountCommand::DisputeRecovery { .. }),
            Error::Locked,
        )?;
    }
    match &m.command {
        AccountCommand::AddDevice { device, proof } => {
            device.validate()?;
            // Bound the current device set without making sixteen lifetime
            // enrollments a permanent account limit. Old approvals are still
            // rejected by the monotonically increasing security epoch.
            if next.devices.len() >= MAX_DEVICES {
                let retired = next
                    .devices
                    .iter()
                    .filter_map(|(id, d)| d.revoked_at.map(|at| (*id, at)))
                    .min_by_key(|(_, at)| *at);
                if let Some((id, _)) = retired {
                    next.devices.remove(&id);
                }
            }
            ensure(
                next.devices.len() < MAX_DEVICES && !next.devices.contains_key(&device.device_id),
                Error::QuotaExceeded,
            )?;
            verify(
                &device.signing_pub,
                &digest(
                    "dmsg/add-device/v1",
                    &(
                        s.home_user,
                        s.subject_id,
                        device,
                        m.expected_version,
                        m.approval.request_id,
                    ),
                ),
                proof,
            )?;
            next.devices.insert(
                device.device_id,
                Device {
                    input: device.clone(),
                    added_at: now,
                    added_by: Some(m.approval.device_id),
                    revoked_at: None,
                    next_sequence: 0,
                },
            );
            changed(&mut next);
        }
        AccountCommand::RevokeDevice { device_id } => {
            let d = next.devices.get_mut(device_id).ok_or(Error::NotFound)?;
            ensure(d.revoked_at.is_none(), Error::DeviceNotApproved)?;
            d.revoked_at = Some(now);
            ensure(
                next.devices.values().any(|d| {
                    d.revoked_at.is_none()
                        && d.input.role == ControllerRole::Administrator
                        && d.input.capabilities.contains(&Capability::RootManage)
                }),
                invalid("last administrator"),
            )?;
            changed(&mut next);
        }
        AccountCommand::BindAuth { principal, .. } => {
            authenticated(*principal)?;
            ensure(
                !next.auth_bindings.contains(principal)
                    && next.auth_bindings.len() < MAX_AUTH_BINDINGS,
                Error::QuotaExceeded,
            )?;
            next.auth_bindings.push(*principal);
            changed(&mut next);
        }
        AccountCommand::RemoveAuth { principal } => {
            ensure(
                next.auth_bindings.contains(principal) && next.auth_bindings.len() > 1,
                invalid("last/missing authentication binding"),
            )?;
            next.auth_bindings.retain(|p| p != principal);
            changed(&mut next);
        }
        AccountCommand::SetRecovery { policy, proof } => {
            // An old device cannot silently veto a recovery by replacing its
            // pre-registered recovery authority while its request is pending.
            ensure(
                s.pending_recovery
                    .as_ref()
                    .is_none_or(|r| now >= r.expires_at()),
                Error::Pending,
            )?;
            let generation = s.recovery.as_ref().map_or(0, |r| r.generation);
            ensure(
                policy.generation == generation.checked_add(1).ok_or(Error::QuotaExceeded)?
                    && (DAY..=7 * DAY).contains(&policy.delay_ns),
                invalid("recovery generation/delay"),
            )?;
            nonzero(&policy.hpke_pub)?;
            verify(
                &policy.signing_pub,
                &digest(
                    "dmsg/recovery-enroll/v1",
                    &(s.home_user, s.subject_id, policy, m.approval.request_id),
                ),
                proof,
            )?;
            next.recovery = Some(policy.clone());
            next.recovery_checked = false;
            next.pending_recovery = None;
            changed(&mut next);
        }
        AccountCommand::ConfirmRecovery { proof } => {
            let r = s.recovery.as_ref().ok_or(Error::RecoveryIncomplete)?;
            verify(
                &r.signing_pub,
                &digest(
                    "dmsg/recovery-check/v1",
                    &(
                        s.home_user,
                        s.subject_id,
                        r.generation,
                        m.expected_version,
                        m.approval.request_id,
                    ),
                ),
                proof,
            )?;
            next.recovery_checked = true;
        }
        AccountCommand::SetPolicy { policy } => {
            ensure(
                policy.daily_executions <= 100
                    && policy.daily_cycles <= 1_000_000_000_000
                    && policy.allowed_purposes.len() <= 2
                    && policy
                        .allowed_purposes
                        .iter()
                        .all(|p| matches!(p, KeyPurpose::FileAttestation | KeyPurpose::Statement)),
                Error::QuotaExceeded,
            )?;
            next.sensitive_policy = policy.clone();
            changed(&mut next);
        }
        AccountCommand::ReserveRoot {
            expected_generation,
            op_id,
        } => {
            ensure(next.recovery_checked, Error::RecoveryIncomplete)?;
            nonzero(op_id)?;
            ensure(
                *expected_generation == s.current_root.as_ref().map_or(0, |r| r.generation),
                Error::VersionConflict,
            )?;
            if let Some(slot) = &s.root_slot {
                ensure(now >= slot.expires_at, Error::VersionConflict)?;
            }
            let generation = next.next_root_generation;
            next.next_root_generation = generation.checked_add(1).ok_or(Error::QuotaExceeded)?;
            next.root_slot = Some(RootReservation {
                op_id: *op_id,
                expected_generation: *expected_generation,
                generation,
                security_epoch: s.security_epoch,
                expires_at: now + 15 * MINUTE,
            });
        }
        AccountCommand::CommitRoot {
            expected_generation,
            op_id,
            root,
        } => {
            let slot = s.root_slot.as_ref().ok_or(Error::VersionConflict)?;
            ensure(
                slot.op_id == *op_id
                    && slot.expected_generation == *expected_generation
                    && slot.generation == root.generation
                    && slot.security_epoch == s.security_epoch,
                Error::VersionConflict,
            )?;
            ensure(now < slot.expires_at, Error::Expired)?;
            ensure(
                root.home_cose == s.home_cose
                    && root.derivation_version == 1
                    && root.key_generation == root.generation
                    && root.suite == "dmsg-root-v1"
                    && root.recovery_generation
                        == s.recovery
                            .as_ref()
                            .ok_or(Error::RecoveryIncomplete)?
                            .generation,
                Error::IntegrityFailed,
            )?;
            nonzero(&root.bundle_digest)?;
            next.current_root = Some(root.clone());
            next.root_slot = None;
            next.vault_write_state = VaultWriteState::Ready;
        }
        AccountCommand::AuthorizeHandle { intent } => {
            ensure(
                intent.subject == s.subject_id
                    && intent.handle_canister == handle_canister
                    && normalize_handle(&intent.handle)? == intent.handle,
                Error::IntegrityFailed,
            )?;
            expiry(now, m.approval.expires_at, MINUTE)?;
            nonzero(&intent.op_id)?;
            next.handle_authorizations.retain(|_, a| a.expires_at > now);
            ensure(next.handle_authorizations.len() < 32, Error::QuotaExceeded)?;
            if let Some(a) = next.handle_authorizations.get(&intent.op_id) {
                ensure(a.intent == *intent, Error::IdempotencyConflict)?;
            } else {
                next.handle_authorizations.insert(
                    intent.op_id,
                    HandleAuthorization {
                        intent: intent.clone(),
                        expires_at: m.approval.expires_at,
                        consumed: false,
                    },
                );
            }
        }
        AccountCommand::DisputeRecovery { op_id, dispute } => {
            nonzero(dispute)?;
            let r = next.pending_recovery.as_mut().ok_or(Error::NotFound)?;
            ensure(
                r.request.op_id == *op_id && now < r.expires_at(),
                Error::Expired,
            )?;
            if r.dispute.is_none() {
                r.dispute = Some(*dispute);
                next.status = AccountStatus::RecoveryDisputed;
            }
            // Once reconfirmed, repeated disputes cannot restart the delay.
        }
    }
    let receipt = finish(&mut next, &m.approval, fp);
    *s = next;
    Ok(receipt)
}

pub fn authorize(
    s: &mut Subject,
    caller: Principal,
    input: &ExecuteRequest,
    now: u64,
) -> Result<AuthorizedExecution> {
    let fp = digest("dmsg/execute-request/v1", input);
    ensure(s.auth_bindings.contains(&caller), Error::AuthRequired)?;
    if let Some(e) = s.executions.get(&input.approval.request_id) {
        ensure(e.command_digest == fp, Error::IdempotencyConflict)?;
        return Ok(e.clone());
    }
    ensure(
        !s.operations
            .iter()
            .any(|r| r.id == input.approval.request_id),
        Error::IdempotencyConflict,
    )?;
    ensure(
        input.approval.request_id
            == execution_request_id(
                s.subject_id,
                input.approval.security_epoch,
                input.approval.device_id,
                input.approval.sequence,
            ),
        Error::IdempotencyConflict,
    )?;
    ensure(
        s.status == AccountStatus::Active && !s.sensitive_policy.frozen,
        Error::Locked,
    )?;
    let (cap, admin) = match &input.kind {
        ExecutionKind::Sign {
            key,
            canonical_payload,
        } => {
            ensure(s.recovery_checked, Error::RecoveryIncomplete)?;
            ensure(
                s.sensitive_policy.allowed_purposes.contains(&key.purpose),
                Error::Forbidden,
            )?;
            validate_payload(
                canonical_payload,
                s.subject_id,
                input.approval.request_id,
                key,
                input.approval.expires_at,
            )?;
            (Capability::FormalApprove, false)
        }
        ExecutionKind::Derive {
            generation,
            root_op_id,
            transport_key,
        } => {
            validate_transport_key(transport_key)?;
            match root_op_id {
                Some(op) => {
                    let slot = s.root_slot.as_ref().ok_or(Error::VersionConflict)?;
                    ensure(
                        slot.op_id == *op
                            && slot.generation == *generation
                            && slot.security_epoch == s.security_epoch
                            && now < slot.expires_at,
                        Error::VersionConflict,
                    )?;
                    (Capability::RootManage, true)
                }
                None => {
                    ensure(
                        s.current_root
                            .as_ref()
                            .is_some_and(|r| r.generation == *generation),
                        Error::VersionConflict,
                    )?;
                    (Capability::VaultUnlock, false)
                }
            }
        }
    };
    check_device(
        s,
        caller,
        &input.approval,
        "dmsg/execute/v1",
        &(&input.kind, input.max_cycles),
        (Some(cap), admin),
        now,
    )?;
    ensure(
        input.subject == s.subject_id
            && input.max_cycles > 0
            && input.max_cycles <= 100_000_000_000,
        Error::QuotaExceeded,
    )?;
    let mut next = s.clone();
    // Remove only terminal results; the per-device sequence and operation
    // receipt remain authoritative after result eviction.
    next.executions.retain(|_, e| {
        !matches!(
            e.result.status,
            ExecutionStatus::Completed | ExecutionStatus::Failed | ExecutionStatus::ResultExpired
        ) || e.grant.expires_at.saturating_add(DAY) > now
    });
    ensure(
        next.executions.len() < WINDOW && next.next_execution_sequence < u64::MAX,
        Error::QuotaExceeded,
    )?;
    next.budget.reserve(
        now,
        input.max_cycles,
        next.sensitive_policy.daily_executions,
        next.sensitive_policy.daily_cycles,
    )?;
    let grant = ExecutionGrant {
        subject: s.subject_id,
        home_user: s.home_user,
        home_cose: s.home_cose,
        request_id: input.approval.request_id,
        execution_sequence: next.next_execution_sequence,
        security_epoch: s.security_epoch,
        device_id: input.approval.device_id,
        device_sequence: input.approval.sequence,
        approved_at: now,
        expires_at: input.approval.expires_at,
        kind: input.kind.clone(),
        max_cycles: input.max_cycles,
    };
    next.next_execution_sequence += 1;
    let e = AuthorizedExecution {
        grant,
        command_digest: fp,
        result: ExecutionResult {
            status: ExecutionStatus::Authorized,
            result: None,
            key: None,
            charged_cycles: 0,
        },
    };
    next.executions.insert(input.approval.request_id, e.clone());
    finish(&mut next, &input.approval, fp);
    *s = next;
    Ok(e)
}

pub fn begin_recovery(
    s: &mut Subject,
    request: &RecoveryRequest,
    signature: &[u8],
    device_proof: &[u8],
    now: u64,
) -> Result<()> {
    let policy = s.recovery.as_ref().ok_or(Error::RecoveryIncomplete)?;
    request.device.validate()?;
    authenticated(request.new_auth)?;
    nonzero(&request.op_id)?;
    ensure(
        request.device.role == ControllerRole::Administrator
            && request
                .device
                .capabilities
                .contains(&Capability::RootManage),
        invalid("recovery administrator"),
    )?;
    ensure(
        request.generation == policy.generation
            && request.expires_at > now + policy.delay_ns
            && request.expires_at - now <= 14 * DAY,
        Error::Expired,
    )?;
    verify(
        &policy.signing_pub,
        &digest(
            "dmsg/recovery-request/v1",
            &(s.home_user, s.subject_id, s.recovery_nonce, request),
        ),
        signature,
    )?;
    verify(
        &request.device.signing_pub,
        &digest(
            "dmsg/recovery-device/v1",
            &(s.home_user, s.subject_id, request),
        ),
        device_proof,
    )?;
    if let Some(r) = &s.pending_recovery {
        if r.request == *request {
            return Ok(());
        }
        ensure(now >= r.expires_at(), Error::Pending)?;
    }
    s.pending_recovery = Some(PendingRecovery {
        request: request.clone(),
        execute_after: now + policy.delay_ns,
        dispute: None,
        reconfirmed: false,
        confirmation: None,
    });
    Ok(())
}
pub fn reconfirm_recovery(
    s: &mut Subject,
    confirmation: &RecoveryConfirmation,
    signature: &[u8],
    now: u64,
) -> Result<()> {
    let r = s.pending_recovery.as_ref().ok_or(Error::NotFound)?;
    let policy = s.recovery.as_ref().ok_or(Error::RecoveryIncomplete)?;
    let dispute = r.dispute.ok_or(Error::VersionConflict)?;
    ensure(
        confirmation.request_id == r.request.op_id && confirmation.dispute == dispute,
        Error::IntegrityFailed,
    )?;
    verify(
        &policy.signing_pub,
        &recovery_confirmation_message(
            s.home_user,
            s.subject_id,
            s.recovery_nonce,
            &r.request,
            confirmation,
        ),
        signature,
    )?;
    if r.reconfirmed {
        ensure(
            r.confirmation.as_ref() == Some(confirmation),
            Error::IdempotencyConflict,
        )?;
        return Ok(());
    }
    expiry(now, confirmation.expires_at, 14 * DAY)?;
    ensure(
        confirmation.expires_at > now.checked_add(policy.delay_ns).ok_or(Error::Expired)?,
        Error::Expired,
    )?;
    let r = s.pending_recovery.as_mut().unwrap();
    r.reconfirmed = true;
    r.execute_after = now + policy.delay_ns;
    r.confirmation = Some(confirmation.clone());
    Ok(())
}
pub fn complete_recovery(s: &mut Subject, caller: Principal, now: u64) -> Result<()> {
    let r = s.pending_recovery.clone().ok_or(Error::NotFound)?;
    ensure(caller == r.request.new_auth, Error::AuthRequired)?;
    ensure(
        now >= r.execute_after && now < r.expires_at(),
        Error::Expired,
    )?;
    ensure(r.dispute.is_none() || r.reconfirmed, Error::Locked)?;
    s.devices.clear();
    s.devices.insert(
        r.request.device.device_id,
        Device {
            input: r.request.device,
            added_at: now,
            added_by: None,
            revoked_at: None,
            next_sequence: 0,
        },
    );
    s.auth_bindings = vec![r.request.new_auth];
    s.pending_recovery = None;
    s.status = AccountStatus::Active;
    s.recovery_nonce += 1;
    s.account_version += 1;
    s.recovery_checked = true;
    s.sensitive_policy.frozen = false;
    changed(s);
    Ok(())
}

pub fn recovery_request(s: &Subject, caller: Principal) -> Result<Option<PendingRecovery>> {
    authenticated(caller)?;
    ensure(
        s.auth_bindings.contains(&caller)
            || s.pending_recovery
                .as_ref()
                .is_some_and(|r| r.request.new_auth == caller),
        Error::AuthRequired,
    )?;
    Ok(s.pending_recovery.clone())
}

/// Persist a terminal expiry instead of leaking a permanent Unknown slot when
/// cose's short-lived result has already been replaced by its replay watermark.
pub fn record_execution_response(
    s: &mut Subject,
    request_id: Hash,
    response: Result<ExecutionResult>,
) -> Result<ExecutionResult> {
    let execution = s.executions.get_mut(&request_id).ok_or(Error::NotFound)?;
    if matches!(
        execution.result.status,
        ExecutionStatus::Completed | ExecutionStatus::Failed | ExecutionStatus::ResultExpired
    ) {
        return Ok(execution.result.clone());
    }
    match response {
        Ok(result) => {
            execution.result = result.clone();
            Ok(result)
        }
        Err(Error::ResultExpired) => {
            execution.result.status = ExecutionStatus::ResultExpired;
            execution.result.result = None;
            Ok(execution.result.clone())
        }
        Err(error) => {
            execution.result.status = ExecutionStatus::Unknown;
            Err(error)
        }
    }
}
