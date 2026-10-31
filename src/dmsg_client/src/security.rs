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

use crate::account::AccountState;
use crate::approval::{Action, HIGH_RISK_TTL_MS};
use crate::client::{Client, LoginUnlock, Status};
use crate::codec::{b64, canonical, decode, principal_input, unb64};
use crate::hold::{Hold, Unlocked};
use crate::ic::Ic;
use crate::now_ms;
use candid::Principal;
use dmsg_types::{
    AccountId, DAY, Error, Hash, Result, ensure, ensure_valid,
    user::{AccountCommand, PendingRecovery, SensitivePolicy},
};
use ic_agent::Identity;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
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
#[derive(Clone)]
pub(crate) struct PendingBinding {
    principal: Principal,
    nonce: Hash,
    ic: Ic,
    expires_at: u64,
    accept_until: u64,
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
        /// The policy it replaces, as read when asked, so the review shows
        /// what changes.
        previous: Option<SensitivePolicy>,
    },
    /// Set the recovery delay.
    RecoveryDelay {
        /// Milliseconds, one to seven days.
        delay_ms: u64,
    },
    /// Change on-chain recovery. A mode change also rotates the content root.
    RecoveryPolicy {
        /// Whether login-authorized recovery is allowed.
        enabled: bool,
        /// Waiting period in milliseconds, one to seven days.
        delay_ms: u64,
    },
    /// Cancel a pending recovery.
    DisputeRecovery {
        /// Its operation ID.
        op_id: Hash,
    },
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
                principal: principal_input(p)?,
                nonce: *nonce,
            },
            AccountChange::RemoveLogin { principal: p } => AccountCommand::RemoveAuth {
                principal: principal_input(p)?,
            },
            AccountChange::Policy { policy, .. } => AccountCommand::SetPolicy {
                policy: policy.clone(),
            },
            AccountChange::RecoveryDelay { delay_ms } => AccountCommand::SetRecoveryDelay {
                delay_ms: *delay_ms,
            },
            AccountChange::RecoveryPolicy { enabled, delay_ms } => {
                AccountCommand::SetRecoveryPolicy {
                    enabled: *enabled,
                    delay_ms: *delay_ms,
                }
            }
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
            AccountChange::Policy { policy, previous } => {
                let purposes = |p: &SensitivePolicy| {
                    p.allowed_purposes
                        .iter()
                        .map(|p| format!("{p:?}"))
                        .collect::<Vec<_>>()
                };
                json!({
                    "change": "policy",
                    "frozen": policy.frozen,
                    "allowed_purposes": purposes(policy),
                    "daily_executions": policy.daily_executions,
                    "previous": previous.as_ref().map(|p| json!({
                        "frozen": p.frozen,
                        "allowed_purposes": purposes(p),
                        "daily_executions": p.daily_executions,
                    })),
                })
            }
            AccountChange::RecoveryDelay { delay_ms } => json!({
                "change": "recovery_delay",
                "delay_ms": delay_ms,
            }),
            AccountChange::RecoveryPolicy { enabled, delay_ms } => json!({
                "change": "recovery_policy", "enabled": enabled, "delay_ms": delay_ms,
            }),
            AccountChange::DisputeRecovery { op_id } => json!({
                "change": "dispute_recovery",
                "op_id": hex::encode(**op_id),
            }),
        }
    }
}

impl Client {
    /// The view [`account_security`] returns of `state`.
    fn security_view(&self, state: &AccountState) -> Result<Json> {
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
            "recovery_enabled": info.recovery_enabled.unwrap_or(true),
            "recovery_updating": info.current_root.as_ref().is_some_and(|root|
                root.recovery_enabled.unwrap_or(true) != info.recovery_enabled.unwrap_or(true)),
            "pending_recovery": info.pending_recovery.as_ref().map(|p| recovery_view(p, &self.workspace.meta.device_id)),
            "notification": state.security.snapshot.pending_recovery_digest.map(|hash| json!({
                "id": crate::notifications::item_id(&info.account_id.to_string(), &crate::notifications::Target::Recovery(hex::encode(*hash))),
                "version": state.security.snapshot.account_version.to_string(),
            })),
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
        match self.prepare_bind_accept(account).await? {
            Some(prepared) => {
                let key = prepared.open();
                self.complete_login_unlock(prepared, key).await
            }
            None => self.status(),
        }
    }

    /// Accept the login binding without waiting for a bound workspace's
    /// hardware on the client executor.
    ///
    /// # Errors
    /// As [`Self::bind_accept`].
    pub async fn prepare_bind_accept(
        &mut self,
        account: &AccountId,
    ) -> Result<Option<LoginUnlock>> {
        let key = format!("{BINDING}{account}");
        let (home, nonce): (Principal, Hash) =
            self.workspace.store.meta(&key)?.ok_or(Error::NotFound)?;
        let ic = self.session()?.ic.clone();
        let accepted: Result<()> = ic
            .update(home, "accept_auth_binding", (account, nonce))
            .await?;
        accepted?;
        self.workspace.store.delete_meta(&key)?;
        self.prepare_enter().await
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
        ensure(
            crate::account::find_account(&ic, &homes).await?.is_none(),
            Error::IdempotencyConflict,
        )?;
        let state = self.refresh(account.home, &account.id).await?;
        ensure(
            !state.info.auth_bindings.contains(&principal),
            Error::IdempotencyConflict,
        )?;
        let nonce = Hash::new(crate::crypto::random_id());
        let now = now_ms();
        ensure(expires_at > now, Error::Expired)?;
        self.binding = Some(PendingBinding {
            principal,
            nonce,
            ic,
            expires_at,
            accept_until: expires_at.min(now + ACCEPT_MS),
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

    /// [`account_request`], for callers that hold the client throughout.
    ///
    /// # Errors
    /// As [`account_request`].
    pub async fn account_request(&mut self, source: &str, change: AccountChange) -> Result<Json> {
        account_request(&self.hold(), source, change).await
    }

    /// [`account_request`] with the account's state read.
    fn account_change_request(
        &mut self,
        source: &str,
        change: AccountChange,
        command: &AccountCommand,
        state: &AccountState,
    ) -> Result<Json> {
        let principal = self.session()?.principal;
        let own = state.active(&self.workspace.meta.device_id)?;
        let admin = own.input.role == dmsg_types::user::ControllerRole::Administrator
            && own
                .input
                .capabilities
                .contains(&dmsg_types::user::Capability::RootManage);
        let info = &state.info;
        match command {
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
            AccountCommand::SetRecoveryPolicy { enabled, delay_ms } => {
                ensure(
                    admin
                        && own
                            .input
                            .capabilities
                            .contains(&dmsg_types::user::Capability::VaultUnlock),
                    Error::Forbidden,
                )?;
                self.roots()?;
                ensure_valid((DAY..=7 * DAY).contains(delay_ms), "recovery delay")?;
                ensure(!*enabled || info.pending_recovery.is_none(), Error::Pending)?;
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
        self.finish_account_change(&command).await
    }

    /// Settle an approved security change whose outcome is unknown.
    pub(crate) async fn account_reconcile(&mut self, change: &AccountChange) -> Result<()> {
        self.resume().await?;
        self.finish_account_change(&change.command()?).await
    }

    async fn finish_account_change(&mut self, command: &AccountCommand) -> Result<()> {
        if let AccountCommand::SetRecoveryPolicy { enabled, delay_ms } = command {
            let account = self.account()?.clone();
            let state = self.refresh(account.home, &account.id).await?;
            ensure(
                state.info.recovery_enabled == Some(*enabled)
                    && state.info.recovery_delay_ms == *delay_ms,
                Error::PolicyStale,
            )?;
            if state.info.vault_write_state == dmsg_types::user::VaultWriteState::RekeyRequired {
                self.load_root().await?;
                self.rotate_root().await?;
            }
        }
        self.accept_pending(command).await
    }

    /// Accept with the waiting login the binding this device just approved.
    async fn accept_pending(&mut self, command: &AccountCommand) -> Result<()> {
        if let AccountCommand::RemoveAuth { principal } = command {
            self.bound_logins.remove(principal);
            return Ok(());
        }
        let AccountCommand::BindAuth { principal, nonce } = command else {
            return Ok(());
        };
        let Some(binding) = self
            .binding
            .as_ref()
            .filter(|b| b.principal == *principal && b.nonce == *nonce)
            .cloned()
        else {
            return Ok(());
        };
        ensure(binding.accept_until > now_ms(), Error::Expired)?;
        let account = self.account()?.clone();
        let accepted: Result<()> = binding
            .ic
            .update(account.home, "accept_auth_binding", (account.id, *nonce))
            .await?;
        accepted?;
        self.binding = None;
        let now = now_ms();
        self.bound_logins.retain(|_, (_, expires)| *expires > now);
        if binding.expires_at > now {
            self.bound_logins
                .insert(binding.principal, (binding.ic, binding.expires_at));
        }
        Ok(())
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

/// The account's security as the login reads it: bound logins, the
/// sensitive-execution policy, the recovery delay and any pending recovery.
/// The user home is waited on without the client.
///
/// # Errors
/// Returns `Error::Locked` without an open account and `Error::AuthRequired`
/// without a login.
pub async fn account_security<H: Hold>(h: &H) -> Result<Json> {
    let h = Unlocked::new(h).await?;
    let state = crate::account::refreshed(&h).await?;
    h.with(|c| c.security_view(&state)).await
}

/// Prepare a policy edit from one fresh account read, retaining its previous
/// policy for the approval-time conflict check.
/// # Errors
/// As [`account_request`].
pub async fn policy_request<H: Hold>(
    h: &H,
    source: &str,
    frozen: Option<bool>,
    daily: Option<u32>,
    purposes: Option<Vec<dmsg_types::KeyPurpose>>,
) -> Result<Json> {
    let h = Unlocked::new(h).await?;
    let state = crate::account::refreshed(&h).await?;
    h.with(|c| {
        let previous = state.info.sensitive_policy.clone();
        let mut policy = previous.clone();
        if let Some(value) = frozen {
            policy.frozen = value;
        }
        if let Some(value) = daily {
            policy.daily_executions = value;
        }
        if let Some(value) = purposes {
            policy.allowed_purposes = value;
        }
        let change = AccountChange::Policy {
            policy,
            previous: Some(previous),
        };
        c.account_change_request(source, change.clone(), &change.command()?, &state)
    })
    .await
}

/// Ask to change the account's security: bind or remove a login, set the
/// sensitive-execution policy or the recovery delay, or dispute a pending
/// recovery. A person approves it in the approval center. The user home is
/// waited on without the client.
///
/// # Errors
/// Returns `Error::Forbidden` for a change this device cannot sign and
/// `Error::InvalidInput` for one the home would refuse.
pub async fn account_request<H: Hold>(h: &H, source: &str, change: AccountChange) -> Result<Json> {
    let command = change.command()?;
    let h = Unlocked::new(h).await?;
    let state = crate::account::refreshed(&h).await?;
    h.with(|c| c.account_change_request(source, change, &command, &state))
        .await
}

#[cfg(test)]
#[path = "security_tests.rs"]
mod tests;
