//! Account security with the user home (`docs/protocol/account_root_zh.md`):
//! the login Principals bound to the account, the sensitive-execution
//! policy, the recovery delay and disputes of a pending recovery. Every
//! change is an account mutation an administrator device signs once a
//! person approves it in the approval center; a dispute any active device
//! may sign.
//!
//! Binding another login takes three steps: the new login asks with a
//! one-time nonce, an administrator device approves `BindAuth { principal,
//! nonce }`, and the new login accepts with the same nonce within ten
//! minutes (`accept_auth_binding`). On one device the daemon holds both
//! logins: a login started for binding hands its identity to the approval,
//! which accepts as soon as the administrator's change commits. Across
//! devices the request travels as a token.

use crate::approval::{Action, HIGH_RISK_TTL_MS};
use crate::client::{Client, Status};
use crate::codec::{b64, canonical, decode, unb64};
use crate::ic::Ic;
use crate::now_ms;
use candid::Principal;
use dmsg_types::{
    ensure, ensure_valid,
    user::{AccountCommand, PendingRecovery, SensitivePolicy},
    AccountId, Error, Hash, Result, DAY,
};
use ic_agent::Identity;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use std::sync::Arc;

/// Format of a binding request token.
pub const AUTH_REQUEST: &str = "dmsg-auth-request/1";
/// A binding request is valid this long.
const REQUEST_MS: u64 = 300_000;
/// An approved binding waits this long for its login.
const ACCEPT_MS: u64 = 600_000;
/// The binding request of a login, in its workspace's metadata.
const BINDING: &str = "binding:";

#[derive(Serialize, Deserialize)]
struct AuthPacket {
    format: String,
    home: Principal,
    account: AccountId,
    principal: Principal,
    nonce: Hash,
    expires_at: u64,
}

/// A binding request another login produced, after its checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingRequest {
    /// The login asking to be bound.
    pub principal: Principal,
    /// Its one-time nonce.
    pub nonce: Hash,
}

/// A login waiting on this device for its binding: the identity that
/// accepts once the administrator's approval commits.
pub(crate) struct PendingBinding {
    principal: Principal,
    nonce: Hash,
    ic: Ic,
    expires_at: u64,
}

/// A pending recovery for display.
pub(crate) fn recovery_view(pending: &PendingRecovery, device: &Hash) -> Json {
    json!({
        "op_id": hex::encode(*pending.request.op_id),
        "new_auth": pending.request.new_auth.to_text(),
        "device_id": hex::encode(*pending.request.device.device_id),
        "this_device": pending.request.device.device_id == *device,
        "execute_after": pending.execute_after,
        "expires_at": pending.request.expires_at,
    })
}

/// A change of account security, as an approval request carries it. Login
/// Principals are text: approval records are internally tagged, and their
/// encoding does not keep a Principal's bytes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum AccountChange {
    /// Bind another login; it accepts with `nonce`.
    BindLogin {
        /// The login Principal.
        principal: String,
        /// Its one-time nonce.
        nonce: Hash,
    },
    /// Remove a bound login.
    RemoveLogin {
        /// The login Principal.
        principal: String,
    },
    /// Replace the sensitive-execution policy.
    Policy {
        /// The new policy.
        policy: SensitivePolicy,
    },
    /// Set the recovery delay.
    RecoveryDelay {
        /// Milliseconds, one to seven days.
        delay_ms: u64,
    },
    /// Cancel a pending recovery.
    DisputeRecovery {
        /// Its operation ID.
        op_id: Hash,
    },
}

fn principal(text: &str) -> Result<Principal> {
    Principal::from_text(text).map_err(|_| dmsg_types::invalid("principal"))
}

impl AccountChange {
    /// The account mutation that makes the change.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for a malformed Principal.
    pub fn command(&self) -> Result<AccountCommand> {
        Ok(match self {
            AccountChange::BindLogin {
                principal: p,
                nonce,
            } => AccountCommand::BindAuth {
                principal: principal(p)?,
                nonce: *nonce,
            },
            AccountChange::RemoveLogin { principal: p } => AccountCommand::RemoveAuth {
                principal: principal(p)?,
            },
            AccountChange::Policy { policy } => AccountCommand::SetPolicy {
                policy: policy.clone(),
            },
            AccountChange::RecoveryDelay { delay_ms } => AccountCommand::SetRecoveryDelay {
                delay_ms: *delay_ms,
            },
            AccountChange::DisputeRecovery { op_id } => {
                AccountCommand::DisputeRecovery { op_id: *op_id }
            }
        })
    }

    /// The change for display.
    pub fn view(&self) -> Json {
        match self {
            AccountChange::BindLogin { principal, nonce } => json!({
                "change": "bind_login",
                "principal": principal,
                "nonce": hex::encode(**nonce),
            }),
            AccountChange::RemoveLogin { principal } => json!({
                "change": "remove_login",
                "principal": principal,
            }),
            AccountChange::Policy { policy } => json!({
                "change": "policy",
                "frozen": policy.frozen,
                "allowed_purposes": policy.allowed_purposes.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>(),
                "daily_executions": policy.daily_executions,
            }),
            AccountChange::RecoveryDelay { delay_ms } => json!({
                "change": "recovery_delay",
                "delay_ms": delay_ms,
            }),
            AccountChange::DisputeRecovery { op_id } => json!({
                "change": "dispute_recovery",
                "op_id": hex::encode(**op_id),
            }),
        }
    }
}

impl Client {
    /// The account's security as the login reads it: bound logins, the
    /// sensitive-execution policy, the recovery delay and any pending
    /// recovery.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` without a login.
    pub async fn account_security(&mut self) -> Result<Json> {
        self.session()?;
        let account = self.account()?.clone();
        let state = self.refresh(account.home, &account.id).await?;
        let info = &state.info;
        let principal = self.session()?.principal;
        Ok(json!({
            "logins": info.auth_bindings.iter().map(|p| json!({
                "principal": p.to_text(),
                "current": *p == principal,
            })).collect::<Vec<_>>(),
            "policy": {
                "frozen": info.sensitive_policy.frozen,
                "allowed_purposes": info.sensitive_policy.allowed_purposes.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>(),
                "daily_executions": info.sensitive_policy.daily_executions,
            },
            "recovery_delay_ms": info.recovery_delay_ms,
            "pending_recovery": info.pending_recovery.as_ref().map(|p| recovery_view(p, &self.workspace.meta.device_id)),
            "recovered_device": info.recovered_device.map(|(d, g)| json!({"device_id": hex::encode(*d), "generation": g})),
        }))
    }

    /// Ask an administrator device of `account` to bind this login, which
    /// has no account yet: a token to hand over. The nonce stays in this
    /// login's workspace until it accepts.
    ///
    /// # Errors
    /// Returns `Error::IdempotencyConflict` when the login already has an
    /// account and `Error::NotFound` when no user home knows `account`.
    pub async fn bind_request(&mut self, account: &AccountId) -> Result<String> {
        let principal = self.session()?.principal;
        ensure(self.locate().await?.is_none(), Error::IdempotencyConflict)?;
        let home = self.peer_security(account).await?.snapshot.home_user;
        let packet = AuthPacket {
            format: AUTH_REQUEST.into(),
            home,
            account: *account,
            principal,
            nonce: Hash::new(crate::crypto::random_id()),
            expires_at: now_ms() + REQUEST_MS,
        };
        self.workspace
            .store
            .set_meta(&format!("{BINDING}{account}"), &(home, packet.nonce))?;
        Ok(b64(&canonical(&packet)))
    }

    /// Check a binding token for this administrator's account.
    ///
    /// # Errors
    /// A token for another home or account, or an expired one, is refused.
    pub fn inspect_binding(&self, token: &str) -> Result<BindingRequest> {
        ensure(token.len() <= 1024, Error::QuotaExceeded)?;
        let packet: AuthPacket = decode(&unb64(token)?, 1024)?;
        let account = self.account()?;
        ensure(
            packet.format == AUTH_REQUEST
                && packet.home == account.home
                && packet.account == account.id,
            Error::IntegrityFailed,
        )?;
        let now = now_ms();
        ensure(
            packet.expires_at > now && packet.expires_at <= now + REQUEST_MS,
            Error::Expired,
        )?;
        dmsg_protocol::authenticated(packet.principal)?;
        Ok(BindingRequest {
            principal: packet.principal,
            nonce: packet.nonce,
        })
    }

    /// Accept the binding an administrator approved for this login's
    /// request, then continue in the account's workspace: unlocked when this
    /// device already belongs to it, otherwise waiting to pair or recover.
    ///
    /// # Errors
    /// Returns `Error::NotFound` without this login's request or until an
    /// administrator approved it.
    pub async fn bind_accept(&mut self, account: &AccountId) -> Result<Status> {
        let key = format!("{BINDING}{account}");
        let (home, nonce): (Principal, Hash) =
            self.workspace.store.meta(&key)?.ok_or(Error::NotFound)?;
        let ic = self.session()?.ic.clone();
        let accepted: Result<()> = ic
            .update(home, "accept_auth_binding", (account, nonce))
            .await?;
        accepted?;
        self.workspace.store.delete_meta(&key)?;
        self.enter().await
    }

    /// The same-device binding: `identity` is a second login, completed for
    /// this account. It waits here while a person approves binding it; the
    /// approval then accepts with it. Returns the approval request.
    ///
    /// # Errors
    /// Returns `Error::IdempotencyConflict` for a login that already has an
    /// account or is the current one.
    pub async fn bind_login(
        &mut self,
        source: &str,
        identity: Arc<dyn Identity>,
        expires_at: u64,
    ) -> Result<Json> {
        self.keys()?;
        let current = self.session()?.principal;
        let account = self.account()?.clone();
        let ic = Ic::connect(&self.config, identity).await?;
        let principal = ic.principal()?;
        dmsg_protocol::authenticated(principal)?;
        ensure(principal != current, Error::IdempotencyConflict)?;
        let homes = self.homes().await?;
        for home in homes {
            let found: Option<AccountId> = ic.query(home, "my_account", ()).await?;
            ensure(found.is_none(), Error::IdempotencyConflict)?;
        }
        let state = self.refresh(account.home, &account.id).await?;
        ensure(
            !state.info.auth_bindings.contains(&principal),
            Error::IdempotencyConflict,
        )?;
        let nonce = Hash::new(crate::crypto::random_id());
        self.binding = Some(PendingBinding {
            principal,
            nonce,
            ic,
            expires_at: expires_at.min(now_ms() + ACCEPT_MS),
        });
        self.account_request(
            source,
            AccountChange::BindLogin {
                principal: principal.to_text(),
                nonce,
            },
        )
        .await
    }

    /// Ask to change the account's security: bind or remove a login, set
    /// the sensitive-execution policy or the recovery delay, or dispute a
    /// pending recovery. A person approves it in the approval center.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a change this device cannot sign and
    /// `Error::InvalidInput` for one the home would refuse.
    pub async fn account_request(&mut self, source: &str, change: AccountChange) -> Result<Json> {
        let command = change.command()?;
        self.keys()?;
        let principal = self.session()?.principal;
        let account = self.account()?.clone();
        let state = self.refresh(account.home, &account.id).await?;
        let own = state.active(&self.workspace.meta.device_id)?;
        let admin = own.input.role == dmsg_types::user::ControllerRole::Administrator
            && own
                .input
                .capabilities
                .contains(&dmsg_types::user::Capability::RootManage);
        let info = &state.info;
        match &command {
            AccountCommand::BindAuth { principal, .. } => {
                dmsg_protocol::authenticated(*principal)?;
                ensure(admin, Error::Forbidden)?;
                ensure(
                    !info.auth_bindings.contains(principal),
                    Error::IdempotencyConflict,
                )?;
            }
            AccountCommand::RemoveAuth { principal: removed } => {
                ensure(admin, Error::Forbidden)?;
                ensure_valid(
                    info.auth_bindings.contains(removed) && info.auth_bindings.len() > 1,
                    "login",
                )?;
                // The login in use stays; log in with another one to remove it.
                ensure(*removed != principal, Error::Forbidden)?;
            }
            AccountCommand::SetPolicy { policy } => {
                ensure(admin, Error::Forbidden)?;
                check_policy(policy)?;
            }
            AccountCommand::SetRecoveryDelay { delay_ms } => {
                ensure(admin, Error::Forbidden)?;
                ensure_valid((DAY..=7 * DAY).contains(delay_ms), "recovery delay")?;
                ensure(info.pending_recovery.is_none(), Error::Pending)?;
            }
            AccountCommand::DisputeRecovery { op_id } => ensure(
                info.pending_recovery
                    .as_ref()
                    .is_some_and(|p| p.request.op_id == *op_id),
                Error::NotFound,
            )?,
            _ => return Err(Error::Forbidden),
        }
        let request = self.approval_create(
            source,
            Action::Account { change },
            now_ms() + HIGH_RISK_TTL_MS,
        )?;
        self.approval_view(&request)
    }

    /// Run an approved security change; a binding approved on this device
    /// is accepted right after it commits.
    pub(crate) async fn account_execute(&mut self, change: &AccountChange) -> Result<()> {
        let command = change.command()?;
        self.mutate(command.clone(), None).await?;
        self.accept_pending(&command).await
    }

    /// Settle an approved security change whose outcome is unknown.
    pub(crate) async fn account_reconcile(&mut self, change: &AccountChange) -> Result<()> {
        self.resume().await?;
        self.accept_pending(&change.command()?).await
    }

    /// Accept with the waiting login the binding this device just approved.
    async fn accept_pending(&mut self, command: &AccountCommand) -> Result<()> {
        let AccountCommand::BindAuth { principal, nonce } = command else {
            return Ok(());
        };
        let Some(binding) = self
            .binding
            .take_if(|b| b.principal == *principal && b.nonce == *nonce)
        else {
            return Ok(());
        };
        ensure(binding.expires_at > now_ms(), Error::Expired)?;
        let account = self.account()?.clone();
        let accepted: Result<()> = binding
            .ic
            .update(account.home, "accept_auth_binding", (account.id, *nonce))
            .await?;
        accepted
    }
}

/// The policy rules the home enforces.
fn check_policy(policy: &SensitivePolicy) -> Result<()> {
    ensure(
        policy.daily_executions <= 64
            && policy.allowed_purposes.len() <= 3
            && policy
                .allowed_purposes
                .iter()
                .enumerate()
                .all(|(i, p)| !policy.allowed_purposes[..i].contains(p)),
        Error::QuotaExceeded,
    )
}

#[cfg(test)]
#[path = "security_tests.rs"]
mod tests;
