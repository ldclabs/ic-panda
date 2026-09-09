use crate::state::*;
use candid::Principal;
use dmsg_protocol::*;
use dmsg_types::{user::*, *};

use crate::account::changed;

pub(crate) fn begin_recovery(
    s: &mut AccountState,
    request: &RecoveryRequest,
    signature: &[u8],
    device_proof: &[u8],
    now: u64,
) -> Result<()> {
    let policy = s.recovery.as_ref().ok_or(Error::RecoveryIncomplete)?;
    request.device.validate()?;
    authenticated(request.new_auth)?;
    nonzero(request.op_id.as_slice())?;
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
            && request.expires_at > now + policy.delay_ms
            && request.expires_at - now <= 14 * DAY,
        Error::Expired,
    )?;
    verify(
        &policy.signing_pub,
        digest(
            "dmsg/recovery-request/v1",
            &(s.home_user, s.account_id, s.recovery_nonce, request),
        )
        .as_slice(),
        signature,
    )?;
    verify(
        &request.device.signing_pub,
        digest(
            "dmsg/recovery-device/v1",
            &(s.home_user, s.account_id, request),
        )
        .as_slice(),
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
        execute_after: now + policy.delay_ms,
        dispute: None,
        reconfirmed: false,
        confirmation: None,
    });
    Ok(())
}

pub(crate) fn reconfirm_recovery(
    s: &mut AccountState,
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
        recovery_confirmation_message(
            s.home_user,
            s.account_id,
            s.recovery_nonce,
            &r.request,
            confirmation,
        )
        .as_slice(),
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
        confirmation.expires_at > now.checked_add(policy.delay_ms).ok_or(Error::Expired)?,
        Error::Expired,
    )?;
    let r = s.pending_recovery.as_mut().unwrap();
    r.reconfirmed = true;
    r.execute_after = now + policy.delay_ms;
    r.confirmation = Some(confirmation.clone());
    Ok(())
}

pub(crate) fn complete_recovery(s: &mut AccountState, caller: Principal, now: u64) -> Result<()> {
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
