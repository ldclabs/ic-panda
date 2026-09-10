use crate::state::*;
use candid::Principal;
use dmsg_protocol::*;
use dmsg_runtime::*;
use dmsg_types::{cose::*, user::*, *};
use std::collections::BTreeMap;

pub(crate) fn create(
    id: Principal,
    cose: Principal,
    account_id: AccountId,
    caller: Principal,
    input: &CreateAccount,
    now: u64,
) -> Result<AccountState> {
    authenticated(caller)?;
    input.device.validate()?;
    nonzero(input.op_id.as_slice())?;
    expiry(now, input.expires_at, 5 * MINUTE)?;
    ensure(
        input.device.role == ControllerRole::Administrator
            && input.device.capabilities.contains(&Capability::RootManage),
        invalid("initial root administrator"),
    )?;
    verify(
        &input.device.signing_pub,
        digest(
            "dmsg/create-account/v1",
            &(id, caller, &input.device, input.op_id, input.expires_at),
        )
        .as_slice(),
        &input.proof,
    )?;
    Ok(AccountState {
        account_id,
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

pub(crate) fn check_device<'a, T: serde::Serialize>(
    s: &'a AccountState,
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
    nonzero(approval.request_id.as_slice())?;
    verify(
        &device.input.signing_pub,
        approval_message(s.home_user, &s.account_id, domain, payload, approval).as_slice(),
        &approval.signature,
    )?;
    Ok(device)
}

pub(crate) fn changed(s: &mut AccountState) {
    s.security_epoch = s
        .security_epoch
        .checked_add(1)
        .expect("security epoch exhausted");
    s.root_slot = None;
    if s.current_root.is_some() {
        s.vault_write_state = VaultWriteState::RekeyRequired;
    }
}

pub(crate) fn finish(s: &mut AccountState, a: &Approval, fingerprint: Hash) -> OperationReceipt {
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

pub(crate) fn apply(
    s: &mut AccountState,
    caller: Principal,
    m: &AccountMutation,
    now: u64,
    handle_canister: Principal,
) -> Result<OperationReceipt> {
    ensure(
        s.account_id == m.account_id && s.auth_bindings.contains(&caller),
        Error::AuthRequired,
    )?;
    let fp = digest("dmsg/account-operation/v2", m);
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
        "dmsg/account/v2",
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
                digest(
                    "dmsg/add-device/v1",
                    &(
                        s.home_user,
                        &s.account_id,
                        device,
                        m.expected_version,
                        m.approval.request_id,
                    ),
                )
                .as_slice(),
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
                    && (DAY..=7 * DAY).contains(&policy.delay_ms),
                invalid("recovery generation/delay"),
            )?;
            nonzero(policy.hpke_pub.as_slice())?;
            verify(
                &policy.signing_pub,
                digest(
                    "dmsg/recovery-enroll/v1",
                    &(s.home_user, &s.account_id, policy, m.approval.request_id),
                )
                .as_slice(),
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
                digest(
                    "dmsg/recovery-check/v1",
                    &(
                        s.home_user,
                        &s.account_id,
                        r.generation,
                        m.expected_version,
                        m.approval.request_id,
                    ),
                )
                .as_slice(),
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
            nonzero(op_id.as_slice())?;
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
                    && root.derivation_version == 2
                    && root.key_generation == root.generation
                    && root.suite == "dmsg-root-v1"
                    && root.recovery_generation
                        == s.recovery
                            .as_ref()
                            .ok_or(Error::RecoveryIncomplete)?
                            .generation,
                Error::IntegrityFailed,
            )?;
            nonzero(root.bundle_digest.as_slice())?;
            next.current_root = Some(root.clone());
            next.root_slot = None;
            next.vault_write_state = VaultWriteState::Ready;
        }
        AccountCommand::AuthorizeHandle { intent } => {
            ensure(
                intent.account_id == s.account_id
                    && intent.handle_canister == handle_canister
                    && normalize_handle(&intent.handle)? == intent.handle,
                Error::IntegrityFailed,
            )?;
            expiry(now, m.approval.expires_at, MINUTE)?;
            nonzero(intent.op_id.as_slice())?;
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
            nonzero(dispute.as_slice())?;
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
