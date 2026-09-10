use crate::state::*;
use candid::Principal;
use dmsg_protocol::*;
use dmsg_runtime::*;
use dmsg_types::{cose::*, user::*, *};

use crate::account::{check_device, finish};

pub(crate) fn authorize(
    s: &mut AccountState,
    caller: Principal,
    input: &ExecuteRequest,
    now: u64,
    namespace: &str,
    previous: Option<&AuthorizedExecution>,
) -> Result<AuthorizedExecution> {
    ensure(s.auth_bindings.contains(&caller), Error::AuthRequired)?;
    let fp = digest("dmsg/execute-request/v2", input);
    if let Some(e) = previous {
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
                &s.account_id,
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
            to_be_signed,
            public_key_fingerprint,
            origin,
        } => {
            ensure(s.recovery_checked, Error::RecoveryIncomplete)?;
            ensure(
                s.sensitive_policy.allowed_purposes.contains(&key.purpose),
                Error::Forbidden,
            )?;
            key.validate()?;
            validate_origin(origin)?;
            nonzero(public_key_fingerprint.as_slice())?;
            let prepared = parse_signing_input(to_be_signed)?;
            ensure(
                prepared.algorithm == key.algorithm
                    && statement_purpose(&prepared.statement) == key.purpose,
                Error::UnsupportedProtocol,
            )?;
            ensure(
                prepared.statement.issuer == account_issuer(namespace, &s.account_id)?,
                Error::IntegrityFailed,
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
        "dmsg/execute/v3",
        &(&input.kind, input.max_cycles),
        (Some(cap), admin),
        now,
    )?;
    ensure(
        input.account_id == s.account_id
            && input.max_cycles > 0
            && input.max_cycles <= 100_000_000_000,
        Error::QuotaExceeded,
    )?;
    // Count a small index, without loading or cloning historical payloads.
    let retained = s
        .execution_expirations
        .values()
        .filter(|expires_at| expires_at.is_none_or(|at| at > now))
        .count();
    ensure(
        retained < WINDOW && s.next_execution_sequence < u64::MAX,
        Error::QuotaExceeded,
    )?;
    s.budget.reserve(
        now,
        input.max_cycles,
        s.sensitive_policy.daily_executions,
        s.sensitive_policy.daily_cycles,
    )?;
    // All fallible checks have passed, including the atomic budget reservation.
    // Unknown executions stay pinned; device sequences still reject old requests
    // after terminal results and operation receipts have been evicted.
    s.execution_expirations
        .retain(|_, expires_at| expires_at.is_none_or(|at| at > now));
    let grant = ExecutionGrant {
        account_id: s.account_id.clone(),
        home_user: s.home_user,
        home_cose: s.home_cose,
        request_id: input.approval.request_id,
        execution_sequence: s.next_execution_sequence,
        security_epoch: s.security_epoch,
        device_id: input.approval.device_id,
        device_sequence: input.approval.sequence,
        approved_at: now,
        expires_at: input.approval.expires_at,
        kind: input.kind.clone(),
        max_cycles: input.max_cycles,
    };
    s.next_execution_sequence += 1;
    let e = AuthorizedExecution {
        grant,
        command_digest: fp,
        result: ExecutionResult {
            request_id: input.approval.request_id,
            outcome: ExecutionOutcome::Authorized,
            charged_cycles: 0,
        },
    };
    s.execution_expirations
        .insert(input.approval.request_id, None);
    finish(s, &input.approval, fp);
    Ok(e)
}

pub(crate) fn record_execution_response(
    execution: &mut AuthorizedExecution,
    response: Result<ExecutionResult>,
) -> ExecutionResult {
    if execution.result.is_terminal() {
        return execution.result.clone();
    }
    let request_id = execution.grant.request_id;
    let response = response.and_then(|result| {
        ensure(result.request_id == request_id, Error::IntegrityFailed)?;
        if let ExecutionOutcome::Completed(output) = &result.outcome {
            match (&execution.grant.kind, output.as_ref()) {
                (
                    ExecutionKind::Sign {
                        key: requested,
                        to_be_signed,
                        public_key_fingerprint,
                        ..
                    },
                    ExecutionOutput::Signature { artifact, key },
                ) => {
                    match_signing_result(artifact, to_be_signed, *public_key_fingerprint)?;
                    ensure(
                        key.account_id == execution.grant.account_id
                            && key.home_cose == execution.grant.home_cose
                            && key.algorithm == requested.algorithm
                            && key.purpose == requested.purpose
                            && key.public_key_fingerprint == *public_key_fingerprint,
                        Error::IntegrityFailed,
                    )?;
                }
                (
                    ExecutionKind::Derive { generation, .. },
                    ExecutionOutput::EncryptedRootKey { key, .. },
                ) => {
                    ensure(
                        key.account_id == execution.grant.account_id
                            && key.home_cose == execution.grant.home_cose
                            && key.algorithm == Algorithm::VetKdBls12381
                            && key.key_generation == *generation,
                        Error::IntegrityFailed,
                    )?;
                }
                _ => return Err(Error::IntegrityFailed),
            }
        }
        Ok(result)
    });
    match response {
        Ok(result) => {
            execution.result = result;
        }
        Err(Error::ResultExpired) => {
            execution.result.outcome = ExecutionOutcome::ResultExpired;
        }
        Err(error) => {
            execution.result.outcome = ExecutionOutcome::Unknown(error);
        }
    }
    execution.result.clone()
}

pub(crate) fn receipt(
    execution: &AuthorizedExecution,
    namespace: &str,
) -> Result<ExecutionReceipt> {
    let grant = &execution.grant;
    let ExecutionKind::Sign {
        to_be_signed,
        public_key_fingerprint,
        origin,
        ..
    } = &grant.kind
    else {
        return Err(Error::UnsupportedProtocol);
    };
    let signature_digest = match &execution.result.outcome {
        ExecutionOutcome::Completed(output) => match output.as_ref() {
            ExecutionOutput::Signature { artifact, .. } => {
                Some(dmsg_protocol::signature_digest(&artifact.cose_sign1)?)
            }
            _ => return Err(Error::IntegrityFailed),
        },
        _ => None,
    };
    Ok(ExecutionReceipt {
        schema: 1,
        account_id: grant.account_id.clone(),
        issuer: account_issuer(namespace, &grant.account_id)?,
        request_id: grant.request_id,
        device_id: grant.device_id,
        security_epoch: grant.security_epoch,
        approved_at: grant.approved_at,
        expires_at: grant.expires_at,
        origin: origin.clone(),
        max_cycles: grant.max_cycles,
        to_be_signed_digest: sha256(to_be_signed),
        public_key_fingerprint: *public_key_fingerprint,
        status: execution.result.status(),
        signature_digest,
    })
}
