//! Exact-message mobile signing, with relay pinning before execution dispatch.
use super::*;
use crate::account::AccountState;
use crate::approval_key::{HardwareProofAnswer, PreparedHardwareProof};
use crate::workspace::AccountRef;
use dmsg_types::user::HardwarePurpose;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(super) enum Kind {
    Attest(Box<crate::attest::Attestation>),
    Authenticate(Box<AuthJob>),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct Execution {
    review: Hash,
    version: u64,
    kind: Kind,
}

impl Execution {
    fn approval(&self) -> &Approval {
        match &self.kind {
            Kind::Attest(a) => a.request.approval(),
            Kind::Authenticate(a) => &a.approval,
        }
    }

    fn approval_mut(&mut self) -> &mut Approval {
        match &mut self.kind {
            Kind::Attest(a) => a.request.approval_mut(),
            Kind::Authenticate(a) => &mut a.approval,
        }
    }

    fn check(&self, state: &AccountState, now: u64) -> Result<()> {
        let approval = self.approval();
        ensure(now < approval.expires_at, Error::Expired)?;
        ensure(
            state.info.account_version == self.version
                && state.info.security_epoch == approval.security_epoch
                && state.active(&approval.device_id)?.next_sequence == approval.sequence,
            Error::PolicyStale,
        )
    }
}

/// The reviewed browser request and its originating account/session.
pub struct HardwareConnect {
    request: approval::Request,
    operation: Operation,
    account: AccountRef,
    ic: crate::ic::Ic,
    caller: Principal,
    environment: dmsg_types::Environment,
    generation: u64,
}

/// Immutable browser execution before its one hardware authentication.
pub struct PreparedConnect {
    pending: HardwareConnect,
    execution: Execution,
    hardware: PreparedHardwareProof,
}

/// The original execution and hardware answer; no alternate payload can be supplied.
pub struct ProvedConnect {
    pending: HardwareConnect,
    execution: Execution,
    answer: HardwareProofAnswer,
}

impl Client {
    /// Bind a browser signing decision to the original approved digest.
    /// # Errors
    /// Refuses payments, expired/changed requests or absent hardware confirmation.
    pub fn prepare_hardware_connect(&mut self, id: &str, shown: &str) -> Result<HardwareConnect> {
        let now = now_ms();
        self.expire_approvals(now)?;
        let mut request = self.approval_record(id)?;
        ensure(
            hex::encode(*request.digest()) == shown,
            Error::IntegrityFailed,
        )?;
        let Action::Connect {
            operation, digest, ..
        } = &request.action
        else {
            return Err(Error::UnsupportedProtocol);
        };
        let operation = self.operation(operation)?;
        ensure(
            operation.digest == *digest && operation.approval == id,
            Error::IntegrityFailed,
        )?;
        ensure(
            matches!(
                operation.command.method.as_str(),
                "authenticate" | "signDocument" | "signAction"
            ),
            Error::UnsupportedProtocol,
        )?;
        ensure(
            matches!(
                self.approval_presence(id)?,
                Some((approval::Ask::Hardware(..), _))
            ),
            Error::Forbidden,
        )?;
        let login = self.session()?;
        request.decided_via = Some("gui".into());
        let pending = HardwareConnect {
            request,
            operation,
            account: self.account()?.clone(),
            ic: login.ic.clone(),
            caller: login.principal,
            environment: self.config.environment.clone(),
            generation: self.session_generation(),
        };
        pending.check(self, now)?;
        Ok(pending)
    }
}

impl HardwareConnect {
    fn check(&self, client: &Client, now: u64) -> Result<()> {
        client.keys()?;
        ensure(
            client.account()?.id == self.account.id
                && client.account()?.home == self.account.home
                && client.session_generation() == self.generation,
            Error::Locked,
        )?;
        ensure(
            client.session()?.principal == self.caller,
            Error::AuthRequired,
        )?;
        let request = client.approval_record(&self.request.id)?;
        let op = client.operation(&self.operation.key)?;
        ensure(
            request.digest() == self.request.digest()
                && request.expires_at == self.request.expires_at
                && op.digest == self.operation.digest
                && op.app_version == self.operation.app_version,
            Error::IntegrityFailed,
        )?;
        ensure(
            request.state == State::AwaitingUser
                && op.state == OpState::AwaitingUser
                && now < request.expires_at
                && now < op.expires_at,
            Error::Expired,
        )
    }

    async fn admission<H: Hold>(&self, h: &H) -> Result<()> {
        h.with(|c| self.check(c, now_ms())).await?;
        let checks = reads::checks(h).await?;
        let channel = self
            .operation
            .channels
            .first()
            .ok_or(Error::IntegrityFailed)?;
        let pairing = checks
            .pairings
            .get(&channel.pairing_id)
            .ok_or(Error::Forbidden)?;
        ensure(pairing.public.len() == 91, Error::IntegrityFailed)?;
        let app = registration(
            &self.ic,
            self.account.home,
            &self.environment,
            &mut None,
            &self.operation.command.app_id,
        )
        .await?;
        ensure(
            app.config_version == self.operation.app_version
                && !app.paused
                && app.origins.contains(&self.operation.origin),
            Error::PolicyStale,
        )?;
        h.with(|c| self.check(c, now_ms())).await
    }

    /// Read admission and account state without Client, then fix the final message.
    /// # Errors
    /// No relay pin or executable journal is created before authentication.
    pub async fn prepare<H: Hold>(self, h: &H) -> Result<PreparedConnect> {
        let h = Unlocked::new(h).await?;
        self.admission(&h).await?;
        let payload = command_payload(&self.operation.command)?;
        if self.operation.command.method == "signAction" {
            let action: dmsg_types::app_action::AppAction =
                dmsg_protocol::decode_canonical(&payload)?;
            let result: Result<()> = self
                .ic
                .update(
                    self.account.home,
                    "inspect_app_action",
                    (self.account.id, &action),
                )
                .await?;
            result?;
        }
        let state =
            crate::account::read_account(&self.ic, self.account.home, &self.account.id).await?;
        h.with(|c| {
            let now = now_ms();
            self.check(c, now)?;
            c.observe(&self.account.id, &state.security)?;
            c.observe_hardware(&state.info);
            let (kind, message) = if self.operation.command.method == "authenticate" {
                let request: AuthenticationRequest = dmsg_protocol::decode_canonical(&payload)?;
                let device_id = c.workspace.meta.device_id;
                let device = state.active(&device_id)?;
                ensure(
                    device
                        .input
                        .capabilities
                        .contains(&Capability::FormalApprove),
                    Error::Forbidden,
                )?;
                let mut approval = Approval {
                    device_id,
                    security_epoch: state.info.security_epoch,
                    sequence: device.next_sequence,
                    request_id: request.operation_id,
                    expires_at: request.expires_at_ms.min(self.request.expires_at),
                    signature: Ed25519Signature::new([0; 64]),
                    hardware: None,
                };
                let message = approval_message(
                    self.account.home,
                    &self.account.id,
                    AUTH_DOMAIN,
                    &request,
                    &approval,
                );
                approval.signature = Ed25519Signature::new(crate::crypto::sign(
                    &c.keys()?.signing,
                    message.as_slice(),
                ));
                (
                    Kind::Authenticate(Box::new(AuthJob { request, approval })),
                    message,
                )
            } else {
                let subject = if self.operation.command.method == "signDocument" {
                    Subject::Statement {
                        statement: c.document_statement(&self.operation.command, &payload)?.0,
                        origin: self.operation.origin.clone(),
                    }
                } else {
                    Subject::Action(Box::new(dmsg_protocol::decode_canonical(&payload)?))
                };
                let (attestation, message) = c.attest_unsigned(
                    &self.request.id,
                    subject,
                    &state,
                    self.request.expires_at,
                    now,
                )?;
                (Kind::Attest(Box::new(attestation)), message)
            };
            let execution = Execution {
                review: self.request.digest(),
                version: state.info.account_version,
                kind,
            };
            let hardware = c.prepare_hardware_proof(
                &state.info,
                HardwarePurpose::Attestation,
                message,
                execution.approval().expires_at,
                now,
            )?;
            Ok(PreparedConnect {
                pending: self,
                execution,
                hardware,
            })
        })
        .await
    }
}

impl PreparedConnect {
    /// The async owner retains this guard across its blocking authentication.
    pub fn cancellation(&self) -> crate::secure::SigningCancellation {
        self.hardware.cancellation()
    }

    /// Authenticate only this fixed message outside Client.
    pub fn sign(self) -> ProvedConnect {
        ProvedConnect {
            pending: self.pending,
            execution: self.execution,
            answer: self.hardware.sign(),
        }
    }
}

impl ProvedConnect {
    /// Preserve the native hardware failure classification for the host.
    pub fn hardware_error(&self) -> Option<Error> {
        self.answer.error()
    }

    /// Recheck, journal the proved plan, pin the original relay request and execute.
    /// # Errors
    /// A lost pin or execution reply retains the original signed operation.
    pub async fn commit<H: Hold>(mut self, h: &H) -> Result<Json> {
        let h = Unlocked::new(h).await?;
        if self.answer.error().is_some() {
            return h
                .with(|c| {
                    let now = now_ms();
                    self.pending.check(c, now)?;
                    let error = c
                        .complete_hardware_proof(self.answer, now)
                        .err()
                        .ok_or(Error::IntegrityFailed)?;
                    c.approval_unconfirmed(&self.pending.request.id, &error)?;
                    Err(error)
                })
                .await;
        }
        self.pending.admission(&h).await?;
        let state = crate::account::read_account(
            &self.pending.ic,
            self.pending.account.home,
            &self.pending.account.id,
        )
        .await?;
        let key = self.pending.operation.key.clone();
        let device = self.execution.approval().device_id;
        h.with(|c| {
            let now = now_ms();
            self.pending.check(c, now)?;
            self.execution.check(&state, now)?;
            c.observe(&self.pending.account.id, &state.security)?;
            c.observe_hardware(&state.info);
            self.execution.approval_mut().hardware =
                Some(c.complete_hardware_proof(self.answer, now)?);
            let mut op = c.operation(&key)?;
            op.prepared = Some(self.execution);
            op.state = OpState::Authorizing;
            self.pending.request.state = State::Executing;
            self.pending.request.started = true;
            self.pending.request.decided_at = Some(now);
            self.pending.request.updated_at = now;
            self.pending.request.error = None;
            c.workspace.store.atomic(|_| {
                c.save_operation(&op)?;
                c.write_approval(&self.pending.request)
            })?;
            c.running.insert(self.pending.request.id.clone(), now);
            Ok(())
        })
        .await?;
        let channel = self
            .pending
            .operation
            .channels
            .first()
            .ok_or(Error::IntegrityFailed)?;
        let path = h
            .with(|c| {
                c.connect_path(&format!(
                    "/requests/{}/authorize",
                    hex::encode(*channel.request_id)
                ))
            })
            .await?;
        let command = AuthorizeCommand {
            request_id: channel.request_id,
            operation_digest: self.pending.operation.digest,
        };
        let id = Hash::new(random_id());
        let pin = h
            .send(|c| {
                let op = c.operation(&key)?;
                ensure(
                    op.state == OpState::Authorizing && op.prepared.is_some(),
                    Error::Expired,
                )?;
                c.signed_post(&path, AUTHORIZE_ACTION, &command, &id)
            })
            .await;
        let result = if pin.is_ok() {
            pin_state(&h, &key, "authorized", Some(device)).await?;
            execute(&h, &key).await
        } else {
            match reconcile_pin(&h, &key).await {
                Ok(true) => execute(&h, &key).await,
                Ok(false) => Ok(()),
                Err(_) => Err(Error::ExecutionUnknown),
            }
        };
        let result = result.map_err(|error| match error {
            Error::Unavailable(_) | Error::AuthRequired => Error::ExecutionUnknown,
            error => error,
        });
        h.with(|c| {
            c.running.remove(&self.pending.request.id);
            let mut request = c.approval_record(&self.pending.request.id)?;
            if request.state.terminal() {
                return c.approval_view(&request);
            }
            c.finish_approval(&mut request, result)
        })
        .await
    }
}

async fn pin_state<H: Hold>(h: &H, key: &str, state: &str, pinned: Option<Hash>) -> Result<bool> {
    h.with(|c| {
        let mut op = c.operation(key)?;
        if op.state != OpState::Authorizing {
            return Ok(op.state == OpState::Authorized);
        }
        let mut request = c.approval_record(&op.approval)?;
        let here = state == "authorized" && pinned == Some(c.workspace.meta.device_id);
        if here {
            op.state = OpState::Authorized;
            op.pinned = pinned;
        } else {
            op.prepared = None;
            match state {
                "pending" => {
                    op.state = OpState::AwaitingUser;
                    request.state = State::AwaitingUser;
                    request.started = false;
                }
                "cancelled" => {
                    op.state = OpState::Cancelled;
                    request.state = State::Cancelled;
                }
                "expired" => {
                    op.state = OpState::Expired;
                    request.state = State::Expired;
                }
                _ => {
                    op.state = OpState::Elsewhere;
                    request.state = State::Elsewhere;
                }
            }
        }
        request.updated_at = now_ms();
        c.workspace.store.atomic(|_| {
            c.save_operation(&op)?;
            c.write_approval(&request)
        })?;
        Ok(here)
    })
    .await
}

pub(super) async fn reconcile_pin<H: Hold>(h: &H, key: &str) -> Result<bool> {
    let path = h
        .with(|c| {
            let op = c.operation(key)?;
            let channel = op.channels.first().ok_or(Error::IntegrityFailed)?;
            c.connect_path(&format!("/requests/{}", hex::encode(*channel.request_id)))
        })
        .await?;
    let entry = Entry::parse(&h.get(&path).await?)?;
    pin_state(h, key, &entry.state, entry.pinned).await
}

pub(super) async fn execute<H: Hold>(h: &H, key: &str) -> Result<()> {
    let op = h.with(|c| c.operation(key)).await?;
    if op.state.terminal() {
        return if matches!(op.state, OpState::Signed | OpState::Returned) {
            Ok(())
        } else {
            Err(Error::Expired)
        };
    }
    ensure(op.state == OpState::Authorized, Error::ExecutionUnknown)?;
    let execution = op.prepared.ok_or(Error::ExecutionUnknown)?;
    let state = crate::account::refreshed(h).await?;
    let ready = h
        .with(|c| activate(c, key, &execution, &state, now_ms()))
        .await;
    let activation_failed = ready.is_err();
    let result = match ready {
        Ok(false) => return Err(Error::ExecutionUnknown),
        Err(error) => Err(error),
        Ok(true) => match &execution.kind {
            Kind::Attest(attestation) => {
                match crate::attest::dispatch(h, (**attestation).clone()).await {
                    Ok(verified) => attestation_result(&verified),
                    Err(error) => {
                        h.with(|c| {
                            let saved = c
                                .attestation(&attestation.execution_id())?
                                .ok_or(Error::IntegrityFailed)?;
                            Ok(if saved.stage == crate::attest::Stage::Unknown {
                                Err(Error::ExecutionUnknown)
                            } else {
                                Err(error)
                            })
                        })
                        .await?
                    }
                }
            }
            Kind::Authenticate(job) => {
                reads::authentication_result(h, job)
                    .await
                    .map_err(|error| match error {
                        Error::Unavailable(_) | Error::NotFound => Error::ExecutionUnknown,
                        error => error,
                    })
            }
        },
    };
    let result = h
        .with(|c| {
            let mut current = c.operation(key)?;
            if current.state.terminal() {
                return Ok(());
            }
            if activation_failed {
                // A concurrent runner may already have atomically claimed it.
                if current.prepared.is_none() {
                    return Err(Error::ExecutionUnknown);
                }
                current.prepared = None;
                c.save_operation(&current)?;
            }
            c.connect_outcome_local(key, result)
        })
        .await;
    let _ = reads::deliver(h, key).await;
    result
}

fn activate(
    c: &mut Client,
    key: &str,
    execution: &Execution,
    state: &AccountState,
    now: u64,
) -> Result<bool> {
    let mut op = c.operation(key)?;
    if op.prepared.is_none() {
        return Ok(false);
    }
    ensure(
        op.state == OpState::Authorized && op.pinned == Some(c.workspace.meta.device_id),
        Error::Forbidden,
    )?;
    ensure(
        op.prepared.as_ref() == Some(execution),
        Error::IntegrityFailed,
    )?;
    let mut request = c.approval_record(&op.approval)?;
    ensure(
        request.digest() == execution.review
            && matches!(request.state, State::Executing | State::Unknown)
            && request.started,
        Error::IntegrityFailed,
    )?;
    execution.check(state, now)?;
    c.workspace.store.atomic(|_| {
        match &execution.kind {
            Kind::Attest(attestation) => {
                c.attest_sequence_available(&attestation.execution_id())?;
                c.save_attestation(attestation)?;
                op.execution = Some(attestation.execution_id());
                request.execution = op.execution.clone();
            }
            Kind::Authenticate(job) => op.auth = Some((**job).clone()),
        }
        op.prepared = None;
        c.save_operation(&op)?;
        c.write_approval(&request)
    })?;
    Ok(true)
}

#[cfg(test)]
#[path = "connect_prepared_tests.rs"]
mod tests;
