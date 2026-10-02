//! Agent Delegation principal of an account: hosted controllers, publication
//! to the directory, and the hosted-signing authorization point.
//!
//! This home is the linearization point for controller changes and signing: a
//! retired key cannot sign once its retirement commits, however late the
//! directory publishes it. The directory only serves the published state.
use crate::{
    account::{self, Authorized},
    state::*,
    store,
};
use candid::Principal;
use dmsg_protocol::agent;
use dmsg_runtime::storage::{CompactStored, MapExt};
use dmsg_types::{agent::*, cose::*, user::*, *};
use ic_stable_structures::{memory_manager::VirtualMemory, DefaultMemoryImpl, StableBTreeMap};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentPrincipal {
    pub(crate) state: PrincipalState,
    pub(crate) published_version: u64,
    pub(crate) last_nonces: BTreeMap<u32, u64>,
}

type Memory = VirtualMemory<DefaultMemoryImpl>;
thread_local! {
    static PRINCIPALS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<AgentPrincipal>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(8)));
}

pub(crate) fn load(id: &AccountId) -> Option<AgentPrincipal> {
    PRINCIPALS.with_borrow(|t| t.load(id.as_slice()))
}

pub(crate) fn save(id: &AccountId, p: &AgentPrincipal) {
    PRINCIPALS.with_borrow_mut(|t| t.put(id.as_slice(), p));
}

pub(crate) fn is_command(command: &AccountCommand) -> bool {
    matches!(
        command,
        AccountCommand::EnablePrincipal { .. }
            | AccountCommand::RegisterController { .. }
            | AccountCommand::RetireController { .. }
            | AccountCommand::MarkControllerCompromised { .. }
            | AccountCommand::RenameController { .. }
    )
}

fn controller(p: &mut AgentPrincipal, generation: u32) -> Result<&mut HostedController> {
    p.state
        .controllers
        .iter_mut()
        .find(|c| c.generation == generation)
        .ok_or(Error::NotFound)
}

/// Apply one principal command after the shared account-mutation checks.
/// `RegisterController` must carry a key the caller already matched to the
/// COSE derivation. On error neither record changes.
pub(crate) fn apply(
    s: &mut AccountState,
    principal: &mut Option<AgentPrincipal>,
    caller: Principal,
    m: &AccountMutation,
    now: u64,
) -> Result<OperationReceipt> {
    let fp = match account::authorize_mutation(s, caller, m, now)? {
        Authorized::Replay(r) => return Ok(r),
        Authorized::Fresh(fp) => fp,
    };
    let next = prepare(s, principal.clone(), &m.command, now)?;
    // All fallible checks finished; no account clone is needed for atomicity.
    s.principal_updated_at = Some(next.state.updated_at);
    let receipt = account::finish(s, &m.approval, fp);
    *principal = Some(next);
    Ok(receipt)
}

/// Build a validated principal candidate without changing the account.
pub(crate) fn prepare(
    s: &AccountState,
    principal: Option<AgentPrincipal>,
    command: &AccountCommand,
    now: u64,
) -> Result<AgentPrincipal> {
    // Every change advances updated_at, so each new valid_from/retired_at is
    // later than all earlier ones and the document never moves backwards.
    let at = principal
        .as_ref()
        .map_or(now, |p| now.max(p.state.updated_at.saturating_add(1)));
    let mut next = match (command, principal) {
        (AccountCommand::EnablePrincipal { principal_type }, None) => {
            ensure(s.recovery_checked, Error::RecoveryIncomplete)?;
            AgentPrincipal {
                state: PrincipalState {
                    principal_type: principal_type.clone(),
                    controllers: vec![],
                    version: 0,
                    updated_at: at,
                },
                published_version: 0,
                last_nonces: BTreeMap::new(),
            }
        }
        (AccountCommand::EnablePrincipal { .. }, Some(_)) => return Err(Error::VersionConflict),
        (_, None) => return Err(Error::NotFound),
        (_, Some(p)) => p,
    };
    match command {
        AccountCommand::EnablePrincipal { .. } => {}
        AccountCommand::RegisterController {
            generation,
            public_key,
            name,
            delegation,
            supersedes,
        } => {
            ensure(s.recovery_checked, Error::RecoveryIncomplete)?;
            let expected = next
                .state
                .controllers
                .last()
                .map_or(Some(1), |c| c.generation.checked_add(1))
                .ok_or(Error::QuotaExceeded)?;
            ensure(*generation == expected, Error::VersionConflict)?;
            next.state.controllers.push(HostedController {
                generation: *generation,
                public_key: *public_key,
                name: name.clone(),
                valid_from: at,
                delegation: delegation.clone(),
                supersedes: supersedes.clone(),
                retired_at: None,
                invalid_from: None,
            });
        }
        AccountCommand::RetireController { generation } => {
            let c = controller(&mut next, *generation)?;
            ensure(c.retired_at.is_none(), Error::VersionConflict)?;
            c.retired_at = Some(at);
        }
        AccountCommand::MarkControllerCompromised {
            generation,
            invalid_from,
        } => {
            let c = controller(&mut next, *generation)?;
            let retired_at = *c.retired_at.get_or_insert(at);
            ensure_valid(
                c.valid_from <= *invalid_from
                    && *invalid_from <= retired_at
                    && c.invalid_from.is_none_or(|old| *invalid_from <= old),
                "invalid_from may only be added or moved earlier within the binding",
            )?;
            c.invalid_from = Some(*invalid_from);
        }
        AccountCommand::RenameController { generation, name } => {
            controller(&mut next, *generation)?.name = name.clone();
        }
        _ => return Err(invalid("not a principal command")),
    }
    next.state.version = next
        .state
        .version
        .checked_add(1)
        .ok_or(Error::QuotaExceeded)?;
    next.state.updated_at = at;
    agent::validate_principal_state(&next.state)?;
    // A principal change does not bump the security epoch: signing rechecks the
    // controller, so unrelated device approvals stay valid.
    Ok(next)
}

/// Hosted-signing authorization for an `AgentEvent`, run in the same message
/// that commits the execution. Checks the controller, principal, delegation-ID
/// prefix, time, grant policy and nonce, then records the nonce. An execution
/// that later fails leaves a nonce gap, which Agent Identity allows.
pub(crate) fn authorize_event(
    p: &mut AgentPrincipal,
    s: &AccountState,
    init: &UserInit,
    kind: &ExecutionKind,
    parsed: &agent::DelegationEvent,
    now: u64,
) -> Result<()> {
    let ExecutionKind::AgentEvent {
        key, principal_id, ..
    } = kind
    else {
        return Ok(());
    };
    ensure(
        *principal_id == agent::principal_id(&init.principal_origin, &s.account_id),
        Error::IntegrityFailed,
    )?;
    let generation = u32::try_from(key.generation).map_err(|_| Error::NotFound)?;
    let c = p
        .state
        .controllers
        .iter()
        .find(|c| c.generation == generation)
        .ok_or(Error::NotFound)?;
    agent::check_hosted_event(parsed, &s.account_id, principal_id, c, now)?;
    let last = p.last_nonces.get(&generation).copied().unwrap_or(0);
    ensure(parsed.nonce > last, Error::VersionConflict)?;
    p.last_nonces.insert(generation, parsed.nonce);
    Ok(())
}

/// Push the current state to the directory. Idempotent and safe to call
/// concurrently or out of order: the directory accepts only increasing
/// versions, and `published_version` only moves forward.
pub(crate) async fn publish(account_id: AccountId) -> Result<u64> {
    let p = load(&account_id).ok_or(Error::NotFound)?;
    if p.published_version >= p.state.version {
        return Ok(p.published_version);
    }
    let directory = store::config().init.directory_canister;
    let result: Result<Publication> =
        dmsg_runtime::call(directory, "publish", (account_id, p.state)).await?;
    let published = result?.version;
    // Reread after the call: another publication or a newer change may have committed.
    let mut p = load(&account_id).ok_or(Error::NotFound)?;
    ensure(published <= p.state.version, Error::IntegrityFailed)?;
    if published > p.published_version {
        p.published_version = published;
        save(&account_id, &p);
    }
    Ok(p.published_version)
}

pub(crate) fn info(account_id: &AccountId, p: AgentPrincipal) -> PrincipalInfo {
    PrincipalInfo {
        principal_id: agent::principal_id(&store::config().init.principal_origin, account_id),
        state: p.state,
        published_version: p.published_version,
        last_nonces: p.last_nonces,
    }
}
