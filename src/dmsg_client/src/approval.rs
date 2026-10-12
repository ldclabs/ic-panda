//! The approval center (plan §6.6): every operation this device signs on its
//! owner's behalf waits here until a person decides, in the desktop approval
//! window or an interactive terminal. Requests come from the desktop, the
//! CLI, agents or web applications (dmsg-connect/1); none of them can
//! answer one.
//!
//! A request's content is sealed under the LDK; only its ID, state, source
//! class and times stay in plain text, so a locked device can count what
//! waits without learning what it is. Answering checks the digest the person
//! was shown, the deadline and the state, then executes once: a lost reply
//! is reconciled by its execution ID, never signed again.

use crate::client::Client;
use crate::now_ms;
use crate::store::ApprovalRow;
use dmsg_protocol::digest;
use dmsg_types::{
    ensure,
    user::{AccountCommand, Capability},
    Error, Hash, Result, Statement, MINUTE,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};

/// How long a request from this device's own clients waits.
pub const APPROVAL_TTL_MS: u64 = 10 * MINUTE;
/// How long a high-risk request, such as an account change, waits.
pub const HIGH_RISK_TTL_MS: u64 = 5 * MINUTE;
/// Most requests waiting for a decision at once.
pub const MAX_PENDING: usize = 20;
/// Most web requests from one origin waiting at once.
pub const MAX_PER_ORIGIN: usize = 5;

const PREFIX: &str = "approval:";

/// Where a request is in its life.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Waiting for a person.
    AwaitingUser,
    /// The person refused, or the device refused it before showing it.
    Rejected,
    /// Its deadline passed undecided.
    Expired,
    /// Its source withdrew it.
    Cancelled,
    /// Approved and running.
    Executing,
    /// Approved; the outcome is not known yet and is being reconciled.
    Unknown,
    /// Executed.
    Done,
    /// Approved, and the execution definitely failed.
    Failed,
    /// Another device of the account took it.
    Elsewhere,
}

impl State {
    /// The name the index and clients use.
    pub fn name(self) -> &'static str {
        match self {
            State::AwaitingUser => "awaiting_user",
            State::Rejected => "rejected",
            State::Expired => "expired",
            State::Cancelled => "cancelled",
            State::Executing => "executing",
            State::Unknown => "unknown",
            State::Done => "done",
            State::Failed => "failed",
            State::Elsewhere => "elsewhere",
        }
    }

    /// No further change.
    pub fn terminal(self) -> bool {
        !matches!(
            self,
            State::AwaitingUser | State::Executing | State::Unknown
        )
    }
}

/// What a request asks the device to do.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Attest a statement with this device; `origin` is the checked
    /// application origin the execution receipt records.
    Attest {
        /// The statement, issuer included.
        statement: Statement,
        /// `https://dmsg.net` for this device's own clients.
        origin: String,
        /// The file a file statement or digest describes, re-hashed on approval.
        file: Option<String>,
    },
    /// Replace a device's capabilities.
    DeviceCapabilities {
        /// The device.
        device_id: Hash,
        /// The complete new set.
        capabilities: Vec<Capability>,
    },
    /// Revoke a device, then rotate the content root without it.
    DeviceRevoke {
        /// The device.
        device_id: Hash,
    },
    /// A web request (dmsg-connect/1).
    Connect {
        /// Key of the operation record.
        operation: String,
        /// The operation's commitment.
        digest: Hash,
    },
    /// Approve a quoted checkout for the wallet and pay it; with `fresh`,
    /// approve a PANDA claim again after its cooling period.
    Checkout {
        /// Order or claim ID.
        id: String,
        /// The terms' commitment.
        digest: Hash,
        /// A post-cooling approval.
        fresh: bool,
    },
    /// Pay for a quoted letter from the wallet.
    Letter {
        /// The recipient.
        recipient: String,
        /// Order ID.
        order: String,
        /// The quote's commitment.
        digest: Hash,
        /// What the wallet pays, fees and refundable reserve included.
        amount: String,
        /// The ledger.
        ledger: String,
    },
    /// Authorize and run a name operation.
    Handle {
        /// Operation ID.
        id: String,
        /// The intent's commitment.
        digest: Hash,
    },
    /// Change the account's Agent Delegation principal.
    Principal {
        /// The change.
        change: crate::agent::PrincipalChange,
    },
    /// Sign a grant or revocation with a controller key and submit it.
    Delegation {
        /// Journal ID.
        job: String,
        /// The event's commitment.
        digest: Hash,
    },
}

impl Action {
    fn class(&self) -> &'static str {
        match self {
            Action::Attest { .. } => "attest",
            Action::DeviceCapabilities { .. }
            | Action::DeviceRevoke { .. }
            | Action::Principal { .. } => "account",
            Action::Delegation { .. } => "agent",
            Action::Connect { .. } => "connect",
            Action::Checkout { .. } | Action::Letter { .. } | Action::Handle { .. } => "payment",
        }
    }
}

/// A request in the approval center, sealed under the LDK.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// Random request ID, hex.
    pub id: String,
    /// `gui`, `cli`, `agent:<did>`, `client:<id>` or `connect:<origin>`.
    pub source: String,
    /// The operation.
    pub action: Action,
    /// Where it is.
    pub state: State,
    /// Unix milliseconds.
    pub created_at: u64,
    /// The decision deadline.
    pub expires_at: u64,
    /// Last change.
    pub updated_at: u64,
    /// Execution ID (attestations) once approved, hex.
    pub execution: Option<String>,
    /// Error code of a refusal or failure.
    pub error: Option<String>,
    /// Past the point where it may have executed: a later failure is final.
    pub started: bool,
}

impl Request {
    /// The commitment a person approves: the request as shown.
    pub fn digest(&self) -> Hash {
        digest("dmsg/approval/v1", &(&self.id, &self.source, &self.action))
    }
}

/// Check a source string.
///
/// # Errors
/// Returns `Error::InvalidInput` for anything else.
pub fn validate_source(source: &str) -> Result<()> {
    let ok = match source.split_once(':') {
        None => matches!(source, "gui" | "cli"),
        Some(("agent", rest)) => rest.starts_with("did:agent:") && rest.len() <= 200,
        Some(("client", rest)) => {
            !rest.is_empty()
                && rest.len() <= 64
                && rest
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        }
        Some(("connect", origin)) => origin.starts_with("http"),
        _ => false,
    };
    ensure(
        ok && !source.chars().any(char::is_control),
        dmsg_types::invalid("source"),
    )
}

impl Client {
    fn approval_key(id: &str) -> String {
        format!("{PREFIX}{id}")
    }

    /// One request, as stored.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::NotFound` for an unknown ID.
    pub(crate) fn approval_record(&self, id: &str) -> Result<Request> {
        self.workspace
            .private(self.keys()?, &Self::approval_key(id))?
            .ok_or(Error::NotFound)
    }

    /// Store a request and its index row together.
    pub(crate) fn save_approval(&self, request: &Request) -> Result<()> {
        let keys = self.keys()?;
        self.workspace.store.atomic(|store| {
            store.put_approval(&ApprovalRow {
                id: request.id.clone(),
                state: request.state.name().into(),
                class: request.action.class().into(),
                created_at: request.created_at,
                expires_at: request.expires_at,
            })?;
            self.workspace
                .put_private(keys, &Self::approval_key(&request.id), request)
        })
    }

    /// Add a request from `source`, waiting until `expires_at`.
    ///
    /// # Errors
    /// Returns `Error::QuotaExceeded` beyond [`MAX_PENDING`] waiting requests,
    /// or [`MAX_PER_ORIGIN`] from one web origin.
    pub(crate) fn approval_create(
        &mut self,
        source: &str,
        action: Action,
        expires_at: u64,
    ) -> Result<Request> {
        validate_source(source)?;
        let now = now_ms();
        self.expire_approvals(now)?;
        let waiting: Vec<Request> = self
            .workspace
            .store
            .approvals(&[State::AwaitingUser.name()])?
            .iter()
            .map(|row| self.approval_record(&row.id))
            .collect::<Result<_>>()?;
        ensure(
            waiting.len() < MAX_PENDING
                && (!source.starts_with("connect:")
                    || waiting.iter().filter(|r| r.source == source).count() < MAX_PER_ORIGIN),
            Error::QuotaExceeded,
        )?;
        let request = Request {
            id: hex::encode(crate::crypto::random_id()),
            source: source.into(),
            action,
            state: State::AwaitingUser,
            created_at: now,
            expires_at,
            updated_at: now,
            execution: None,
            error: None,
            started: false,
        };
        self.save_approval(&request)?;
        Ok(request)
    }

    /// Mark undecided requests past their deadline expired.
    fn expire_approvals(&mut self, now: u64) -> Result<()> {
        for row in self
            .workspace
            .store
            .approvals(&[State::AwaitingUser.name()])?
        {
            if row.expires_at <= now {
                let mut request = self.approval_record(&row.id)?;
                request.state = State::Expired;
                request.updated_at = now;
                self.save_approval(&request)?;
                if let Action::Connect { operation, .. } = &request.action {
                    self.connect_expire(operation)?;
                }
            }
        }
        Ok(())
    }

    /// Requests waiting for a decision, or every request with `all`.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn approvals(&mut self, all: bool) -> Result<Vec<Json>> {
        self.keys()?;
        self.expire_approvals(now_ms())?;
        let states: &[&str] = if all {
            &[]
        } else {
            &["awaiting_user", "executing", "unknown"]
        };
        self.workspace
            .store
            .approvals(states)?
            .iter()
            .map(|row| self.approval_view(&self.approval_record(&row.id)?))
            .collect()
    }

    /// Approved requests whose outcome is not settled; readable while locked.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn approvals_unsettled(&self) -> Result<u64> {
        Ok(self
            .workspace
            .store
            .approvals(&[State::Executing.name(), State::Unknown.name()])?
            .len() as u64)
    }

    /// Requests waiting for a decision; readable while locked.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn approvals_waiting(&self) -> Result<u64> {
        let now = now_ms();
        Ok(self
            .workspace
            .store
            .approvals(&[State::AwaitingUser.name()])?
            .iter()
            .filter(|r| r.expires_at > now)
            .count() as u64)
    }

    /// One request with everything a person needs to decide.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown ID.
    pub fn approval(&mut self, id: &str) -> Result<Json> {
        self.expire_approvals(now_ms())?;
        self.approval_view(&self.approval_record(id)?)
    }

    /// Withdraw a request this device's own clients made, before a decision.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a web request and `Error::Expired`
    /// once it is decided.
    pub fn approval_cancel(&mut self, id: &str) -> Result<Json> {
        let mut request = self.approval_record(id)?;
        ensure(
            !matches!(request.action, Action::Connect { .. }),
            Error::Forbidden,
        )?;
        ensure(request.state == State::AwaitingUser, Error::Expired)?;
        request.state = State::Cancelled;
        request.updated_at = now_ms();
        self.save_approval(&request)?;
        self.approval_view(&request)
    }

    /// A person's decision on `id`, after seeing the request whose digest
    /// is `shown`. Only the desktop approval window and an interactive
    /// terminal call this; approving executes the operation once.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` when the request changed since it was
    /// shown, `Error::Expired` past its deadline or once decided, and the
    /// execution's errors.
    pub async fn approval_respond(&mut self, id: &str, shown: &str, approve: bool) -> Result<Json> {
        let now = now_ms();
        self.expire_approvals(now)?;
        let mut request = self.approval_record(id)?;
        ensure(
            hex::encode(*request.digest()) == shown,
            Error::IntegrityFailed,
        )?;
        ensure(
            request.state == State::AwaitingUser && now < request.expires_at,
            Error::Expired,
        )?;
        if !approve {
            request.state = State::Rejected;
            request.updated_at = now;
            self.save_approval(&request)?;
            if let Action::Connect { operation, .. } = &request.action {
                self.connect_reject(operation).await?;
            }
            return self.approval_view(&request);
        }
        request.state = State::Executing;
        request.updated_at = now;
        self.save_approval(&request)?;
        let outcome = self.execute_approval(&mut request).await;
        self.finish_approval(&mut request, outcome)
    }

    async fn execute_approval(&mut self, request: &mut Request) -> Result<()> {
        match request.action.clone() {
            Action::Attest {
                statement,
                origin,
                file,
            } => {
                if let Some(path) = &file {
                    self.check_statement_file(&statement, std::path::Path::new(path))?;
                }
                let attestation = self
                    .attest_prepare(
                        &request.id,
                        crate::attest::Subject::Statement { statement, origin },
                    )
                    .await?;
                request.execution = Some(attestation.execution_id());
                request.started = true;
                self.save_approval(request)?;
                self.attest_dispatch(attestation).await.map(|_| ())
            }
            Action::DeviceCapabilities {
                device_id,
                capabilities,
            } => self
                .mutate(
                    AccountCommand::SetDeviceCapabilities {
                        device_id,
                        capabilities,
                    },
                    None,
                )
                .await
                .map(|_| ()),
            Action::DeviceRevoke { device_id } => {
                self.mutate(AccountCommand::RevokeDevice { device_id }, None)
                    .await?;
                // A new root no revoked device can open (AT21).
                request.started = true;
                self.rotate_root().await.map(|_| ())
            }
            Action::Connect { operation, .. } => self.connect_approve(&operation, request).await,
            Action::Checkout { id, digest, fresh } => {
                ensure(
                    self.plan_checkout(&id)?.digest() == digest,
                    Error::IntegrityFailed,
                )?;
                request.started = true;
                self.save_approval(request)?;
                match fresh {
                    true => self.plan_advance(&id).await.map(|_| ()),
                    false => self.plan_execute(&id).await.map(|_| ()),
                }
            }
            Action::Letter {
                recipient,
                order,
                digest,
                ..
            } => {
                let recipient = recipient.parse().map_err(|_| Error::IntegrityFailed)?;
                ensure(
                    self.letter_digest(&recipient, &order).await?.0 == digest,
                    Error::IntegrityFailed,
                )?;
                request.started = true;
                self.save_approval(request)?;
                self.inbox_pay(&recipient, &order).await.map(|_| ())
            }
            Action::Handle { id, digest } => {
                ensure(
                    self.handle_operation(&id)?.digest() == digest,
                    Error::IntegrityFailed,
                )?;
                request.started = true;
                self.save_approval(request)?;
                self.handle_execute(&id).await.map(|_| ())
            }
            Action::Principal { change } => self.principal_execute(&change).await,
            // The job's journal decides: approving again only resubmits
            // what was signed, so nothing here is past the point of no return.
            Action::Delegation { job, digest } => {
                ensure(
                    self.delegation(&job)?.digest() == digest,
                    Error::IntegrityFailed,
                )?;
                self.delegation_execute(&job).await.map(|_| ())
            }
        }
    }

    fn finish_approval(&mut self, request: &mut Request, outcome: Result<()>) -> Result<Json> {
        request.updated_at = now_ms();
        let result = match outcome {
            // An executor may settle the request itself, such as a web
            // request another device took first.
            Ok(()) if !matches!(request.state, State::Executing | State::Unknown) => Ok(()),
            Ok(()) => {
                request.state = State::Done;
                request.error = None;
                Ok(())
            }
            Err(Error::ExecutionUnknown) => {
                request.state = State::Unknown;
                Err(Error::ExecutionUnknown)
            }
            // Nothing ran: no login, no network or stale certified data.
            // The request keeps waiting for the person to try again.
            Err(error @ (Error::AuthRequired | Error::Unavailable(_) | Error::PolicyStale))
                if !request.started =>
            {
                request.state = State::AwaitingUser;
                request.error = Some(crate::error_code(&error));
                Err(error)
            }
            Err(error) => {
                request.state = State::Failed;
                request.error = Some(crate::error_code(&error));
                Err(error)
            }
        };
        self.save_approval(request)?;
        result?;
        self.approval_view(request)
    }

    /// Reconcile every approved request whose outcome is unknown.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked; each request's own failure is
    /// kept on it.
    pub async fn approvals_reconcile(&mut self) -> Result<()> {
        for row in self
            .workspace
            .store
            .approvals(&[State::Executing.name(), State::Unknown.name()])?
        {
            let mut request = self.approval_record(&row.id)?;
            let outcome = match (&request.action, &request.execution) {
                (Action::Attest { .. }, Some(execution)) => {
                    let execution = execution.clone();
                    self.attest_reconcile(&execution).await.map(|_| ())
                }
                // Account changes reconcile through the account journal.
                (Action::DeviceCapabilities { .. }, _) => self.resume().await.map(|_| ()),
                (Action::DeviceRevoke { .. }, _) => match self.resume().await {
                    Ok(_) => self.rotate_root().await.map(|_| ()),
                    Err(error) => Err(error),
                },
                (Action::Connect { operation, .. }, _) => {
                    let operation = operation.clone();
                    self.connect_reconcile(&operation).await
                }
                (Action::Checkout { id, fresh, .. }, _) => {
                    let (id, fresh) = (id.clone(), *fresh);
                    match fresh {
                        true => self.plan_advance(&id).await.map(|_| ()),
                        false => self.plan_execute(&id).await.map(|_| ()),
                    }
                }
                (
                    Action::Letter {
                        recipient, order, ..
                    },
                    _,
                ) => {
                    let order = order.clone();
                    match recipient.parse() {
                        Ok(recipient) => self.inbox_pay(&recipient, &order).await.map(|_| ()),
                        Err(_) => Err(Error::IntegrityFailed),
                    }
                }
                (Action::Handle { id, .. }, _) => {
                    let id = id.clone();
                    self.handle_execute(&id).await.map(|_| ())
                }
                (Action::Principal { .. }, _) => self.resume().await.map(|_| ()),
                (Action::Delegation { job, .. }, _) => {
                    let job = job.clone();
                    self.delegation_execute(&job).await.map(|_| ())
                }
                // Approved, but the attestation was never prepared.
                (Action::Attest { .. }, None) => Err(Error::Expired),
            };
            // A step that could not run says nothing about the outcome:
            // the request stays where it is for the next round.
            match outcome {
                Err(
                    error @ (Error::ExecutionUnknown
                    | Error::Unavailable(_)
                    | Error::AuthRequired
                    | Error::PolicyStale
                    | Error::Locked),
                ) => {
                    request.error = Some(crate::error_code(&error));
                    self.save_approval(&request)?;
                }
                outcome => {
                    let _ = self.finish_approval(&mut request, outcome);
                }
            }
        }
        Ok(())
    }

    /// Ask to replace a device's capabilities, such as turning on
    /// `FormalApprove` for this device; an administrator device holding
    /// `RootManage` approves it, its own included.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` on a device that cannot approve account
    /// changes and `Error::DeviceNotApproved` for an unknown or revoked device.
    pub async fn capabilities_request(
        &mut self,
        source: &str,
        device_id: Hash,
        capabilities: Vec<Capability>,
    ) -> Result<Json> {
        self.keys()?;
        let account = self.account()?.clone();
        let security = self.refresh_security(account.home, &account.id).await?;
        let own = security.active(&self.workspace.meta.device_id)?;
        ensure(
            own.input.role == dmsg_types::user::ControllerRole::Administrator
                && own.input.capabilities.contains(&Capability::RootManage),
            Error::Forbidden,
        )?;
        let target = security.active(&device_id)?;
        let mut input = target.input.clone();
        input.capabilities = capabilities.clone();
        dmsg_protocol::DeviceInputExt::validate(&input)?;
        let request = self.approval_create(
            source,
            Action::DeviceCapabilities {
                device_id,
                capabilities,
            },
            now_ms() + HIGH_RISK_TTL_MS,
        )?;
        self.approval_view(&request)
    }

    /// Ask to revoke another device of the account; approving also rotates
    /// the content root, so the revoked device cannot read what comes next.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` on a device that cannot approve account
    /// changes or for this device itself.
    pub async fn revoke_request(&mut self, source: &str, device_id: Hash) -> Result<Json> {
        self.keys()?;
        let account = self.account()?.clone();
        let security = self.refresh_security(account.home, &account.id).await?;
        let own = security.active(&self.workspace.meta.device_id)?;
        ensure(
            own.input.role == dmsg_types::user::ControllerRole::Administrator
                && own.input.capabilities.contains(&Capability::RootManage)
                && device_id != self.workspace.meta.device_id,
            Error::Forbidden,
        )?;
        security.active(&device_id)?;
        let request = self.approval_create(
            source,
            Action::DeviceRevoke { device_id },
            now_ms() + HIGH_RISK_TTL_MS,
        )?;
        self.approval_view(&request)
    }

    /// This device's capabilities with `FormalApprove` turned on or off.
    ///
    /// # Errors
    /// As [`Self::capabilities_request`].
    pub async fn formal_approve_request(&mut self, source: &str, enable: bool) -> Result<Json> {
        let account = self.account()?.clone();
        let security = self.refresh_security(account.home, &account.id).await?;
        let device_id = self.workspace.meta.device_id;
        let mut capabilities = security.active(&device_id)?.input.capabilities.clone();
        capabilities.retain(|c| *c != Capability::FormalApprove);
        if enable {
            capabilities.push(Capability::FormalApprove);
        }
        self.capabilities_request(source, device_id, capabilities)
            .await
    }

    /// Ask to approve a quoted checkout and pay it from the wallet; with
    /// `fresh`, to approve a PANDA claim again after cooling.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown checkout.
    pub fn checkout_request(&mut self, source: &str, id: &str, fresh: bool) -> Result<Json> {
        let digest = self.plan_checkout(id)?.digest();
        let request = self.approval_create(
            source,
            Action::Checkout {
                id: id.into(),
                digest,
                fresh,
            },
            now_ms() + HIGH_RISK_TTL_MS,
        )?;
        self.approval_view(&request)
    }

    /// Ask to pay for a quoted letter from the wallet.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an order without a quote.
    pub async fn letter_request(
        &mut self,
        source: &str,
        recipient: &dmsg_types::AccountId,
        order: &str,
    ) -> Result<Json> {
        let (digest, quote) = self.letter_digest(recipient, order).await?;
        let request = self.approval_create(
            source,
            Action::Letter {
                recipient: recipient.to_string(),
                order: order.into(),
                digest,
                amount: quote.amount,
                ledger: quote.ledger,
            },
            now_ms() + HIGH_RISK_TTL_MS,
        )?;
        self.approval_view(&request)
    }

    /// Ask to authorize and run a prepared name operation.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown operation.
    pub fn handle_request(&mut self, source: &str, id: &str) -> Result<Json> {
        let digest = self.handle_operation(id)?.digest();
        let request = self.approval_create(
            source,
            Action::Handle {
                id: id.into(),
                digest,
            },
            now_ms() + HIGH_RISK_TTL_MS,
        )?;
        self.approval_view(&request)
    }

    /// The view a client renders: the request, its digest and a summary of
    /// what approving does.
    pub(crate) fn approval_view(&self, request: &Request) -> Result<Json> {
        let detail = match &request.action {
            Action::Attest {
                statement,
                origin,
                file,
            } => json!({
                "statement": crate::attest::statement_view(statement),
                "origin": origin,
                "file": file,
                "file_checked": file.as_ref().map(|path| {
                    self.check_statement_file(statement, std::path::Path::new(path)).is_ok()
                }),
            }),
            Action::DeviceCapabilities {
                device_id,
                capabilities,
            } => json!({
                "device_id": hex::encode(**device_id),
                "capabilities": capabilities.iter().map(|c| format!("{c:?}")).collect::<Vec<_>>(),
                "this_device": *device_id == self.workspace.meta.device_id,
            }),
            Action::DeviceRevoke { device_id } => json!({
                "device_id": hex::encode(**device_id),
                "this_device": *device_id == self.workspace.meta.device_id,
            }),
            Action::Connect { operation, .. } => self.connect_view(operation)?,
            Action::Checkout { id, fresh, .. } => {
                let mut view = self.plan_checkout(id)?.view();
                view["fresh"] = (*fresh).into();
                view
            }
            Action::Letter {
                recipient,
                order,
                amount,
                ledger,
                ..
            } => {
                json!({"recipient": recipient, "order": order, "amount": amount, "ledger": ledger})
            }
            Action::Handle { id, .. } => self.handle_operation(id)?.view(),
            Action::Principal { change } => json!(change),
            Action::Delegation { job, .. } => self.delegation(job)?.view(),
        };
        Ok(json!({
            "id": request.id,
            "source": request.source,
            "kind": request.action.class(),
            "type": match &request.action {
                Action::Attest { .. } => "attest",
                Action::DeviceCapabilities { .. } => "device_capabilities",
                Action::DeviceRevoke { .. } => "device_revoke",
                Action::Connect { .. } => "connect",
                Action::Checkout { .. } => "checkout",
                Action::Letter { .. } => "letter",
                Action::Handle { .. } => "handle",
                Action::Principal { .. } => "principal",
                Action::Delegation { .. } => "delegation",
            },
            "state": request.state.name(),
            "created_at": request.created_at,
            "expires_at": request.expires_at,
            "digest": hex::encode(*request.digest()),
            "execution": request.execution,
            "error": request.error,
            "detail": detail,
        }))
    }
}
