//! Formal attestations recorded in one message, and the single vetKD root
//! derivation a recovered device may request.
use crate::state::*;
use candid::Principal;
use dmsg_protocol::*;
use dmsg_runtime::*;
use dmsg_types::{cose::*, user::*, *};

use crate::account::{check_device, finish};

/// Shared read-only checks: binding, replay, request identity, lock state and
/// the retained-execution window. Nothing is consumed.
fn precheck(
    s: &AccountState,
    caller: Principal,
    account_id: &AccountId,
    approval: &Approval,
    fingerprint: Hash,
    now: u64,
) -> Result<()> {
    ensure(
        *account_id == s.account_id && s.auth_bindings.contains(&caller),
        Error::AuthRequired,
    )?;
    if let Some(r) = s.operations.iter().find(|r| r.id == approval.request_id) {
        // A cleaned result keeps its receipt: the same request has expired,
        // while different parameters under that ID conflict.
        return Err(if r.digest == fingerprint {
            Error::ResultExpired
        } else {
            Error::IdempotencyConflict
        });
    }
    ensure(
        approval.request_id
            == execution_request_id(
                &s.account_id,
                approval.security_epoch,
                approval.device_id,
                approval.sequence,
            ),
        Error::IdempotencyConflict,
    )?;
    ensure(!s.sensitive_policy.frozen, Error::Locked)?;
    // Count a small index, without loading or cloning historical payloads.
    let retained = s
        .execution_expirations
        .values()
        .filter(|expires_at| expires_at.is_none_or(|at| at > now))
        .count();
    ensure(
        retained < WINDOW && s.next_execution_sequence < u64::MAX,
        Error::QuotaExceeded,
    )
}

/// Validated attestation input and the budget it would consume.
pub(crate) struct CheckedAttestation {
    pub(crate) prepared: PreparedAttestation,
    pub(crate) budget: Budget,
}

/// Read-only authorization of a device-signed statement. The device must hold
/// `FormalApprove`, the statement purpose must be allowed, and the signature
/// must verify over the exact Sig_structure under the device's key.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_attestation(
    s: &AccountState,
    caller: Principal,
    account_id: &AccountId,
    statement: &Statement,
    origin: &str,
    signature: &Ed25519Signature,
    approval: &Approval,
    fingerprint: Hash,
    now: u64,
    init: &UserInit,
) -> Result<CheckedAttestation> {
    precheck(s, caller, account_id, approval, fingerprint, now)?;
    validate_origin(origin, &init.environment)?;
    ensure(
        statement.issuer == account_issuer(&init.issuer_namespace, &s.account_id),
        Error::IntegrityFailed,
    )?;
    if let StatementContent::AppAction(action) = &statement.content {
        ensure(
            action.origin == origin
                && action.issued_at_ms <= now
                && now < action.expires_at_ms
                && approval.expires_at <= action.expires_at_ms,
            Error::Expired,
        )?;
    }
    let device = check_device(
        s,
        caller,
        approval,
        ATTEST_APPROVAL_DOMAIN,
        &attest_approval_command(statement, origin, signature),
        (Some(Capability::FormalApprove), false),
        now,
    )?;
    let prepared = prepare_attestation(statement, &device.input.signing_pub)?;
    ensure(
        s.sensitive_policy
            .allowed_purposes
            .contains(&prepared.purpose),
        Error::Forbidden,
    )?;
    verify(
        &device.input.signing_pub,
        &prepared.to_be_signed,
        signature.as_slice(),
    )?;
    let mut budget = s.budget.clone();
    budget.count(now, s.sensitive_policy.daily_executions)?;
    Ok(CheckedAttestation { prepared, budget })
}

/// Commit a checked attestation in the same synchronous message as its final
/// authorization. The caller persists it after charging the month.
pub(crate) fn commit_attestation(
    s: &mut AccountState,
    approval: &Approval,
    origin: String,
    signature: &Ed25519Signature,
    checked: CheckedAttestation,
    fingerprint: Hash,
    now: u64,
) -> Result<AuthorizedExecution> {
    let artifact = parse_signing_input(&checked.prepared.to_be_signed)?
        .into_signature(
            &s.devices[&approval.device_id].input.signing_pub[..],
        )?
        .finish(signature.to_vec())?;
    s.budget = checked.budget;
    let e = AuthorizedExecution {
        account_id: s.account_id,
        request_id: approval.request_id,
        command_digest: fingerprint,
        record: ExecutionRecord::Attestation(Attestation {
            device_id: approval.device_id,
            security_epoch: s.security_epoch,
            approved_at: now,
            expires_at: approval.expires_at,
            origin,
            to_be_signed_digest: sha256(&checked.prepared.to_be_signed),
            public_key_fingerprint: checked.prepared.thumbprint,
            signature_digest: sha256(signature.as_slice()),
            artifact,
        }),
    };
    s.next_execution_sequence += 1;
    s.execution_expirations
        .insert(approval.request_id, Some(e.retention()));
    finish(s, approval, fingerprint);
    Ok(e)
}

/// Read-only authorization of the one derivation a recovered device may make:
/// the committed generation it was enrolled for, before it commits a new root.
pub(crate) fn check_derivation(
    s: &AccountState,
    caller: Principal,
    input: &DeriveRootRequest,
    fingerprint: Hash,
    now: u64,
) -> Result<Budget> {
    precheck(s, caller, &input.account_id, &input.approval, fingerprint, now)?;
    validate_transport_key(&input.transport_public_key)?;
    ensure(
        s.completed_recovery.as_ref().is_some_and(|r| {
            r.device_id == input.approval.device_id && r.root_generation == Some(input.generation)
        }) && s
            .current_root
            .as_ref()
            .is_some_and(|r| r.generation == input.generation),
        Error::Forbidden,
    )?;
    check_device(
        s,
        caller,
        &input.approval,
        DERIVE_APPROVAL_DOMAIN,
        &derive_approval_command(input),
        (Some(Capability::RootManage), true),
        now,
    )?;
    ensure(
        input.max_cycles > 0 && input.max_cycles <= 100_000_000_000,
        Error::QuotaExceeded,
    )?;
    let mut budget = s.budget.clone();
    budget.count(now, s.sensitive_policy.daily_executions)?;
    Ok(budget)
}

/// Commit the checked derivation and build the grant the COSE home executes.
pub(crate) fn commit_derivation(
    s: &mut AccountState,
    input: DeriveRootRequest,
    fingerprint: Hash,
    budget: Budget,
    now: u64,
) -> AuthorizedExecution {
    s.budget = budget;
    let grant = ExecutionGrant {
        account_id: s.account_id,
        home_user: s.home_user,
        home_cose: s.home_cose,
        request_id: input.approval.request_id,
        execution_sequence: s.next_execution_sequence,
        security_epoch: s.security_epoch,
        device_id: input.approval.device_id,
        device_sequence: input.approval.sequence,
        approved_at: now,
        expires_at: input.approval.expires_at,
        generation: input.generation,
        transport_key: input.transport_public_key,
        max_cycles: input.max_cycles,
    };
    s.next_execution_sequence += 1;
    let e = AuthorizedExecution {
        account_id: s.account_id,
        request_id: input.approval.request_id,
        command_digest: fingerprint,
        record: ExecutionRecord::Derivation {
            grant,
            result: ExecutionResult {
                request_id: input.approval.request_id,
                outcome: ExecutionOutcome::Authorized,
                cycles_cost_upper_bound: 0,
                cycles_charged: 0,
            },
        },
    };
    s.execution_expirations
        .insert(input.approval.request_id, None);
    finish(s, &input.approval, fingerprint);
    e
}

/// A clean failure describes only this attempt; never overwrite a concurrent
/// completion or turn an earlier unknown result into an unsent operation.
pub(crate) fn rejected_dispatch_result(
    current: ExecutionResult,
    error: Error,
) -> Result<ExecutionResult> {
    if current.status() == ExecutionStatus::Authorized {
        Err(error)
    } else {
        Ok(current)
    }
}

/// Return the counted derivation when COSE ran nothing; a completed or
/// unknown derivation keeps its count.
pub(crate) fn settle_budget(s: &mut AccountState, grant: &ExecutionGrant, result: &ExecutionResult) {
    let executed = match result.outcome {
        ExecutionOutcome::Completed(_) => true,
        ExecutionOutcome::Failed(_) => result.cycles_charged > 0,
        _ => return,
    };
    s.budget.settle(grant.approved_at, 0, 0, executed);
}

/// Record COSE's answer on a derivation. The answer must name this request
/// and, when completed, describe this account's key of the granted generation.
pub(crate) fn record_execution_response(
    execution: &mut AuthorizedExecution,
    response: Result<ExecutionResult>,
) -> ExecutionResult {
    let ExecutionRecord::Derivation { grant, result } = &mut execution.record else {
        unreachable!("attestations complete at commit");
    };
    if result.is_terminal() {
        return result.clone();
    }
    let response = response.and_then(|r| {
        ensure(r.request_id == grant.request_id, Error::IntegrityFailed)?;
        if let ExecutionOutcome::Completed(output) = &r.outcome {
            ensure(
                output.key.account_id == grant.account_id
                    && output.key.home_cose == grant.home_cose
                    && output.key.key_generation == grant.generation,
                Error::IntegrityFailed,
            )?;
        }
        Ok(r)
    });
    match response {
        Ok(r) => *result = r,
        Err(Error::ResultExpired) => result.outcome = ExecutionOutcome::ResultExpired,
        Err(error) => result.outcome = ExecutionOutcome::Unknown(error),
    }
    result.clone()
}

pub(crate) fn receipt(execution: &AuthorizedExecution, namespace: &str) -> Result<ExecutionReceipt> {
    let ExecutionRecord::Attestation(a) = &execution.record else {
        return Err(Error::UnsupportedProtocol);
    };
    Ok(ExecutionReceipt {
        schema: 2,
        account_id: execution.account_id,
        issuer: account_issuer(namespace, &execution.account_id),
        request_id: execution.request_id,
        device_id: a.device_id,
        security_epoch: a.security_epoch,
        approved_at: a.approved_at,
        expires_at: a.expires_at,
        origin: a.origin.clone(),
        to_be_signed_digest: a.to_be_signed_digest,
        public_key_fingerprint: a.public_key_fingerprint,
        signature_digest: a.signature_digest,
    })
}
