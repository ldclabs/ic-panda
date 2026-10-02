use crate::state::*;
use candid::Principal;
use dmsg_protocol::*;
use dmsg_runtime::*;
use dmsg_types::{cose::*, user::*, *};

use crate::account::{check_device, finish};

/// Immutable parsing is reused across awaits; account authorization is not.
pub(crate) struct PreparedRequest {
    statement: Option<PreparedStatement>,
    pub(crate) event: Option<dmsg_protocol::agent::DelegationEvent>,
}

impl PreparedRequest {
    pub(crate) fn new(input: &ExecuteRequest) -> Result<Self> {
        Ok(Self {
            statement: match &input.kind {
                ExecutionKind::Sign { to_be_signed, .. } => {
                    Some(parse_signing_input(to_be_signed)?)
                }
                _ => None,
            },
            event: match &input.kind {
                ExecutionKind::AgentEvent { event, .. } => {
                    Some(dmsg_protocol::agent::parse_delegation_event(event)?)
                }
                _ => None,
            },
        })
    }

    pub(crate) fn action(&self) -> Option<&dmsg_types::app_action::AppAction> {
        match &self.statement.as_ref()?.statement().content {
            StatementContent::AppAction(action) => Some(action),
            _ => None,
        }
    }
}

/// Read-only authorization, including a candidate budget. No sequence or nonce
/// is consumed until all remote checks have returned and this check is rerun.
pub(crate) fn check(
    s: &AccountState,
    caller: Principal,
    input: &ExecuteRequest,
    now: u64,
    init: &UserInit,
    prepared: &PreparedRequest,
) -> Result<Budget> {
    ensure(s.auth_bindings.contains(&caller), Error::AuthRequired)?;
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
            public_key_fingerprint,
            origin,
            ..
        } => {
            ensure(s.recovery_checked, Error::RecoveryIncomplete)?;
            ensure(
                s.sensitive_policy.allowed_purposes.contains(&key.purpose),
                Error::Forbidden,
            )?;
            key.validate()?;
            nonzero(public_key_fingerprint.as_slice())?;
            validate_origin(origin, &init.environment)?;
            let prepared = prepared.statement.as_ref().expect("parsed signing request");
            ensure(
                *prepared.algorithm() == key.algorithm
                    && statement_purpose(prepared.statement()) == key.purpose,
                Error::UnsupportedProtocol,
            )?;
            ensure(
                prepared.statement().issuer
                    == account_issuer(&init.issuer_namespace, &s.account_id),
                Error::IntegrityFailed,
            )?;
            if let StatementContent::AppAction(action) = &prepared.statement().content {
                ensure(
                    action.origin == *origin
                        && action.issued_at_ms <= now
                        && now < action.expires_at_ms
                        && input.approval.expires_at <= action.expires_at_ms,
                    Error::Expired,
                )?;
            }
            (Capability::FormalApprove, false)
        }
        ExecutionKind::AgentEvent {
            key,
            principal_id,
            origin,
            ..
        } => {
            // Controller, event, policy and nonce checks need the principal
            // record; see principal::authorize_event in the same message.
            ensure(s.recovery_checked, Error::RecoveryIncomplete)?;
            ensure(
                key.purpose == KeyPurpose::AgentController
                    && s.sensitive_policy.allowed_purposes.contains(&key.purpose),
                Error::Forbidden,
            )?;
            key.validate()?;
            validate_origin(origin, &init.environment)?;
            ensure(
                *principal_id
                    == dmsg_protocol::agent::principal_id(&init.principal_origin, &s.account_id),
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
    let window = if input.kind.is_formal() {
        FORMAL_EXECUTION_WINDOW
    } else {
        WINDOW
    };
    ensure(
        retained < window && s.next_execution_sequence < u64::MAX,
        Error::QuotaExceeded,
    )?;
    let mut budget = if matches!(input.kind, ExecutionKind::Derive { .. }) {
        s.safety_budget.clone()
    } else {
        s.budget.clone()
    };
    if matches!(input.kind, ExecutionKind::Derive { .. }) {
        // Root derivations keep a separate hard cap from formal signatures.
        budget.reserve(
            now,
            input.max_cycles,
            ROOT_DAILY_EXECUTIONS,
            ROOT_DAILY_CYCLES,
        )?;
    } else {
        budget.reserve(
            now,
            input.max_cycles,
            s.sensitive_policy.daily_executions,
            s.sensitive_policy.daily_cycles,
        )?;
    }
    Ok(budget)
}

/// Commit the checked request in the same synchronous message as its final
/// authorization. The caller persists it only after commercial reservation.
pub(crate) fn commit(
    s: &mut AccountState,
    input: ExecuteRequest,
    fingerprint: Hash,
    budget: Budget,
    now: u64,
) -> AuthorizedExecution {
    if matches!(input.kind, ExecutionKind::Derive { .. }) {
        s.safety_budget = budget;
    } else {
        s.budget = budget;
    }
    let grant = ExecutionGrant {
        commerce: None,
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
        kind: input.kind,
        max_cycles: input.max_cycles,
    };
    s.next_execution_sequence += 1;
    let e = AuthorizedExecution {
        grant,
        command_digest: fingerprint,
        result: ExecutionResult {
            request_id: input.approval.request_id,
            outcome: ExecutionOutcome::Authorized,
            cycles_cost_upper_bound: 0,
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
                    ExecutionKind::AgentEvent {
                        key: requested,
                        event,
                        ..
                    },
                    ExecutionOutput::AgentSignature {
                        event_hash,
                        signature,
                        key,
                    },
                ) => {
                    ensure(
                        key.account_id == execution.grant.account_id
                            && key.home_cose == execution.grant.home_cose
                            && key.algorithm == Algorithm::Ed25519
                            && key.purpose == KeyPurpose::AgentController
                            && key.key_generation == requested.generation
                            && *event_hash == dmsg_protocol::agent::event_hash(event),
                        Error::IntegrityFailed,
                    )?;
                    let public_key: Hash = key
                        .public_key
                        .as_slice()
                        .try_into()
                        .map(Hash::new)
                        .map_err(|_| Error::IntegrityFailed)?;
                    verify(&public_key, event_hash.as_slice(), signature.as_slice())?;
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
        account_id: grant.account_id,
        issuer: account_issuer(namespace, &grant.account_id),
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
