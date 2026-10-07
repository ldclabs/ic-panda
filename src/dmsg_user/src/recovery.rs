//! Delayed recovery authorized by a bound login Principal. An active device
//! cancels it by dispute; otherwise the replacement device takes over after
//! the account's delay and may derive the current root through vetKD until it
//! commits a new one.
use crate::state::*;
use candid::Principal;
use dmsg_protocol::*;
use dmsg_types::{user::*, *};

use crate::account::changed;

pub(crate) fn begin_recovery(
    s: &mut AccountState,
    caller: Principal,
    request: &RecoveryRequest,
    device_proof: &[u8],
    now: u64,
) -> Result<()> {
    authenticated(caller)?;
    ensure(
        caller == request.new_auth && s.auth_bindings.contains(&caller),
        Error::AuthRequired,
    )?;
    request.device.validate()?;
    nonzero(request.op_id.as_slice())?;
    ensure_valid(
        request.device.role == ControllerRole::Administrator
            && request
                .device
                .capabilities
                .contains(&Capability::RootManage),
        "recovery administrator",
    )?;
    verify(
        &request.device.signing_pub,
        recovery_device_message(s.home_user, &s.account_id, request).as_slice(),
        device_proof,
    )?;
    if let Some(r) = &s.pending_recovery {
        if r.request == *request {
            // A retry does not start a new delay.
            ensure(now < r.request.expires_at, Error::Expired)?;
            return Ok(());
        }
        ensure(now >= r.request.expires_at, Error::Pending)?;
    }
    ensure(
        s.completed_recovery
            .as_ref()
            .is_none_or(|r| r.request_id != request.op_id),
        Error::IdempotencyConflict,
    )?;
    ensure(
        request.expires_at > now + s.recovery_delay_ms && request.expires_at - now <= 14 * DAY,
        Error::Expired,
    )?;
    s.pending_recovery = Some(PendingRecovery {
        request: request.clone(),
        execute_after: now + s.recovery_delay_ms,
    });
    Ok(())
}

/// Returns the login bindings replaced by the recovered principal so the caller
/// can drop their authentication routes; None is an exact completion retry.
pub(crate) fn complete_recovery(
    s: &mut AccountState,
    caller: Principal,
    request_id: OpId,
    now: u64,
) -> Result<Option<Vec<Principal>>> {
    if let Some(r) = &s.completed_recovery {
        if r.request_id == request_id {
            ensure(caller == r.new_auth, Error::AuthRequired)?;
            return Ok(None);
        }
    }
    let r = s.pending_recovery.clone().ok_or(Error::NotFound)?;
    ensure(r.request.op_id == request_id, Error::IdempotencyConflict)?;
    ensure(caller == r.request.new_auth, Error::AuthRequired)?;
    ensure(
        now >= r.execute_after && now < r.request.expires_at,
        Error::Expired,
    )?;
    s.devices.clear();
    s.devices.insert(
        r.request.device.device_id,
        Device {
            input: r.request.device.clone(),
            added_at: now,
            added_by: None,
            revoked_at: None,
            next_sequence: 0,
        },
    );
    let removed = std::mem::replace(&mut s.auth_bindings, vec![r.request.new_auth]);
    s.pending_recovery = None;
    s.completed_recovery = Some(RecoveryReceipt {
        request_id,
        new_auth: caller,
        device_id: r.request.device.device_id,
        root_generation: s.current_root.as_ref().map(|root| root.generation),
    });
    s.account_version += 1;
    s.sensitive_policy.frozen = false;
    changed(s);
    Ok(Some(removed))
}

pub(crate) fn recovery_request(
    s: &AccountState,
    caller: Principal,
) -> Result<Option<PendingRecovery>> {
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
