//! Exact-message hardware plans for account commands with no follow-up workflow.
use super::*;
use crate::agent::PrincipalChange;
use crate::approval::{Action, Ask, Request as ApprovalRequest, State as ApprovalState};
use crate::approval_key::{HardwareProofAnswer, PreparedHardwareProof};
use crate::hold::Unlocked;
use crate::security::AccountChange;
use serde_json::Value;

const APPROVED: &str = "approved-account:";

fn journal_key(id: &str) -> String {
    format!("{APPROVED}{id}")
}

#[derive(Clone, Serialize, Deserialize)]
struct ApprovedJournal {
    digest: Hash,
    journal: Journal,
    error: Option<Error>,
    followup_done: bool,
}

impl Client {
    fn approved_journal(&self, id: &str) -> Result<Option<ApprovedJournal>> {
        self.workspace.private(self.keys()?, &journal_key(id))
    }

    pub(crate) fn has_approved_account_journal(&self, id: &str) -> Result<bool> {
        Ok(self.approved_journal(id)?.is_some())
    }

    pub(crate) fn delete_approved_account_journal(&self, id: &str) -> Result<()> {
        self.workspace.delete_private(&journal_key(id))
    }

    fn save_approved_journal(&self, id: &str, journal: &ApprovedJournal) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &journal_key(id), journal)
    }

    fn settle_approved_account(
        &mut self,
        id: &str,
        expected: &Journal,
        stage: Stage,
        result: Result<()>,
        now: u64,
    ) -> Result<Result<Value>> {
        let mut stored = self.approved_journal(id)?.ok_or(Error::ExecutionUnknown)?;
        ensure(
            same_operation(&stored.journal, expected),
            Error::IntegrityFailed,
        )?;
        let mut request = self.approval_record(id)?;
        ensure(request.digest() == stored.digest, Error::IntegrityFailed)?;
        // A receipt resolved by a concurrent reader wins over a late network error.
        if matches!(stored.journal.stage, Stage::Confirmed | Stage::Rejected) {
            return Ok(stored
                .error
                .map_or_else(|| self.approval_view(&request), Err));
        }
        stored.journal.stage = stage;
        stored.error = result.clone().err();
        let followup = result.is_ok() && needs_followup(&request.action);
        let result = crate::approval::apply_outcome(
            &mut request,
            if followup {
                Err(Error::ExecutionUnknown)
            } else {
                result
            },
            now,
        );
        let current: Option<Journal> = self.workspace.private(self.keys()?, JOURNAL)?;
        self.workspace.store.atomic(|_| {
            // Another account operation may already own the global sequence lane.
            // Retain this approval's receipt without overwriting that operation.
            if current
                .as_ref()
                .is_some_and(|j| same_operation(j, expected))
            {
                self.save_journal(&stored.journal)?;
            }
            self.save_approved_journal(id, &stored)?;
            self.write_approval(&request)
        })?;
        self.running.remove(id);
        Ok(if followup {
            self.approval_view(&request)
        } else {
            result.and_then(|_| self.approval_view(&request))
        })
    }

    /// Finish a confirmed mutation's existing root/binding workflow. The mobile
    /// host uses its exclusive runner only for this legacy follow-up; no hardware
    /// prompt or new approval takes place inside it.
    /// # Errors
    /// Unfinished follow-up remains unknown and resumes the original root/binding.
    pub async fn finish_prepared_account_approval(&mut self, id: &str) -> Result<Value> {
        let mut stored = self.approved_journal(id)?.ok_or(Error::NotFound)?;
        let mut request = self.approval_record(id)?;
        ensure(request.digest() == stored.digest, Error::IntegrityFailed)?;
        if stored.journal.stage != Stage::Confirmed
            || stored.followup_done
            || !needs_followup(&request.action)
        {
            return self.approval_view(&request);
        }
        let result = match &request.action {
            Action::DeviceRevoke { .. } | Action::DevicePair { moved: None, .. } => {
                self.rotate_root().await.map(|_| ())
            }
            Action::Account { change } => self.finish_account_change(&change.command()?).await,
            Action::Principal { .. } => {
                let _ = self.principal_settled().await;
                Ok(())
            }
            _ => Err(Error::IntegrityFailed),
        };
        stored.followup_done = result.is_ok();
        // The account mutation is known committed even if rotation or accepting
        // the new login loses its reply. Do not turn it back into a new decision.
        let result = crate::approval::apply_outcome(
            &mut request,
            result.map_err(|_| Error::ExecutionUnknown),
            now_ms(),
        );
        self.workspace.store.atomic(|_| {
            self.save_approved_journal(id, &stored)?;
            self.write_approval(&request)
        })?;
        result?;
        self.approval_view(&request)
    }
}

fn same_operation(a: &Journal, b: &Journal) -> bool {
    a.home == b.home && a.caller == b.caller && a.account == b.account && a.request == b.request
}

/// The original reviewed request; no mutation or approval has been committed yet.
pub struct HardwareAccountApproval {
    request: ApprovalRequest,
    account: AccountRef,
    caller: Principal,
    ic: Ic,
    generation: u64,
    possession: Option<dmsg_types::P256Signature>,
}

/// Immutable account mutation and the exact commitment its hardware must sign.
pub struct PreparedAccountApproval {
    pending: HardwareAccountApproval,
    mutation: AccountMutation,
    hardware: Option<PreparedHardwareProof>,
}

/// Hardware result kept with the original request and mutation.
pub struct ProvedAccountApproval {
    pending: HardwareAccountApproval,
    mutation: AccountMutation,
    answer: Option<HardwareProofAnswer>,
}

/// Exact possession message of a new local key, before replacement is constructed.
pub struct PreparedAccountKey {
    request: Hash,
    hardware: PreparedHardwareProof,
}

/// One-shot possession answer still bound to the reviewed approval.
pub struct ProvedAccountKey {
    request: Hash,
    answer: HardwareProofAnswer,
}

impl PreparedAccountKey {
    /// Keep this guard in the async owner while its blocking signer runs.
    pub fn cancellation(&self) -> crate::secure::SigningCancellation {
        self.hardware.cancellation()
    }

    /// Blocking authentication outside the Client executor.
    pub fn sign(self) -> ProvedAccountKey {
        ProvedAccountKey {
            request: self.request,
            answer: self.hardware.sign(),
        }
    }
}

impl ProvedAccountKey {
    /// Hardware failures retain their phase-specific mobile error classification.
    pub fn hardware_error(&self) -> Option<Error> {
        self.answer.error()
    }
}

fn supported(action: &Action) -> bool {
    matches!(
        action,
        Action::DeviceCapabilities { .. }
            | Action::HardwarePolicy { .. }
            | Action::ApprovalKeyRetire { .. }
            | Action::Account { .. }
            | Action::Principal { .. }
            | Action::DeviceRevoke { .. }
            | Action::DevicePair { moved: None, .. }
            | Action::ApprovalKey { .. }
    )
}

fn needs_followup(action: &Action) -> bool {
    matches!(
        action,
        Action::DeviceRevoke { .. }
            | Action::DevicePair { .. }
            | Action::Principal { .. }
            | Action::Account {
                change: AccountChange::BindLogin { .. }
                    | AccountChange::RemoveLogin { .. }
                    | AccountChange::RecoveryPolicy { .. }
            }
    )
}

fn command(
    client: &mut Client,
    action: &Action,
    state: &AccountState,
    principal: Option<&dmsg_types::agent::PrincipalInfo>,
) -> Result<(AccountCommand, Option<(Hash, u64)>)> {
    let command = match action {
        Action::DeviceCapabilities {
            device_id,
            capabilities,
        } => AccountCommand::SetDeviceCapabilities {
            device_id: *device_id,
            capabilities: capabilities.clone(),
        },
        Action::HardwarePolicy {
            purposes, previous, ..
        } => {
            ensure(*previous == state.info.hardware_policy, Error::PolicyStale)?;
            AccountCommand::SetHardwarePolicy {
                purposes: purposes.clone(),
            }
        }
        Action::ApprovalKeyRetire { device_id } => AccountCommand::SetApprovalKey {
            device_id: *device_id,
            key: None,
        },
        Action::ApprovalKey {
            device_id,
            public_key,
            proof: Some(proof),
        } => AccountCommand::SetApprovalKey {
            device_id: *device_id,
            key: Some(dmsg_types::user::ApprovalKeyInput {
                public_key: public_key.clone(),
                proof: *proof,
            }),
        },
        Action::DeviceRevoke { device_id } => AccountCommand::RevokeDevice {
            device_id: *device_id,
        },
        Action::DevicePair {
            token,
            device,
            succession,
            moved: None,
        } => {
            let pairing = if *succession {
                client.inspect_succession(token)?.0
            } else {
                client.inspect_pairing(token)?
            };
            ensure(pairing.device == *device, Error::IntegrityFailed)?;
            return Ok((
                pairing.mutation.command,
                Some((
                    pairing.mutation.approval.request_id,
                    pairing.mutation.expected_version,
                )),
            ));
        }
        Action::Account { change } => {
            if let AccountChange::Policy {
                previous: Some(previous),
                ..
            } = change
            {
                ensure(*previous == state.info.sensitive_policy, Error::PolicyStale)?;
            }
            change.command()?
        }
        Action::Principal { change } => match change {
            PrincipalChange::Enable { principal_type } => AccountCommand::EnablePrincipal {
                principal_type: principal_type.clone(),
            },
            PrincipalChange::Retire { generation } => AccountCommand::RetireController {
                generation: *generation,
            },
            PrincipalChange::Compromise {
                generation,
                invalid_from,
            } => AccountCommand::MarkControllerCompromised {
                generation: *generation,
                invalid_from: *invalid_from,
            },
            PrincipalChange::Rename { generation, name } => AccountCommand::RenameController {
                generation: *generation,
                name: name.clone(),
            },
            PrincipalChange::Register {
                generation,
                name,
                delegation,
            } => {
                ensure(
                    *generation == crate::agent::next_generation(principal.ok_or(Error::NotFound)?),
                    Error::VersionConflict,
                )?;
                let account = client.account()?.clone();
                let seed = client.controller_seed(*generation, true)?;
                let id = Hash::new(random_id());
                let message = dmsg_protocol::controller_pop_message(
                    account.home,
                    &account.id,
                    *generation,
                    delegation,
                    id,
                );
                return Ok((
                    AccountCommand::RegisterController {
                        generation: *generation,
                        name: name.clone(),
                        delegation: delegation.clone(),
                        public_key: Hash::new(crate::crypto::signing_public(&seed)),
                        proof: Ed25519Signature::new(sign(&seed, message.as_slice())),
                    },
                    Some((id, state.info.account_version)),
                ));
            }
        },
        _ => return Err(Error::UnsupportedProtocol),
    };
    Ok((command, None))
}

impl Client {
    /// Whether this request has a complete exact-message account mutation path.
    /// # Errors
    /// Refuses a missing request or unavailable encrypted workspace.
    pub fn has_prepared_account_approval(&self, id: &str) -> Result<bool> {
        Ok(supported(&self.approval_record(id)?.action))
    }

    /// Bind preparation to one undecided request and the originating account/session.
    /// # Errors
    /// Refuses changed/expired requests or actions without a hardware-key answer.
    pub fn prepare_hardware_account_approval(
        &mut self,
        id: &str,
        shown: &str,
        via: &str,
    ) -> Result<HardwareAccountApproval> {
        ensure(matches!(via, "gui" | "cli"), Error::Forbidden)?;
        let now = now_ms();
        self.expire_approvals(now)?;
        let mut request = self.approval_record(id)?;
        ensure(
            hex::encode(*request.digest()) == shown,
            Error::IntegrityFailed,
        )?;
        ensure(supported(&request.action), Error::UnsupportedProtocol)?;
        ensure(
            matches!(self.approval_presence(id)?, Some((Ask::Hardware(..), _))),
            Error::Forbidden,
        )?;
        let login = self.session()?;
        let ic = login.ic.clone();
        let caller = login.principal;
        request.decided_via = Some(via.into());
        let pending = HardwareAccountApproval {
            request,
            account: self.account()?.clone(),
            caller,
            ic,
            generation: self.session_generation(),
            possession: None,
        };
        pending.check(self, now)?;
        Ok(pending)
    }
}

impl HardwareAccountApproval {
    /// A local registration needs the new key's proof first; other approvals
    /// proceed directly to their mutation plan.
    /// # Errors
    /// Refuses changed sessions, expired requests or a missing local key.
    pub async fn prepare_key_possession<H: Hold>(
        &self,
        h: &H,
    ) -> Result<Option<PreparedAccountKey>> {
        h.with(|c| {
            let now = now_ms();
            self.check(c, now)?;
            let Action::ApprovalKey {
                device_id,
                public_key,
                proof: None,
            } = &self.request.action
            else {
                return Ok(None);
            };
            Ok(Some(PreparedAccountKey {
                request: self.request.digest(),
                hardware: c.prepare_key_possession(
                    *device_id,
                    public_key,
                    self.request.expires_at,
                    now,
                )?,
            }))
        })
        .await
    }

    /// Verify the possession answer before using it in an immutable mutation.
    /// # Errors
    /// Cancelled/late answers leave the original approval undecided.
    pub async fn complete_key_possession<H: Hold>(
        mut self,
        h: &H,
        proof: ProvedAccountKey,
    ) -> Result<Self> {
        h.with(|c| {
            let now = now_ms();
            self.check(c, now)?;
            ensure(
                proof.request == self.request.digest(),
                Error::IntegrityFailed,
            )?;
            match c.complete_hardware_proof(proof.answer, now) {
                Ok(proof) => self.possession = Some(proof.signature),
                Err(error) => {
                    c.approval_unconfirmed(&self.request.id, &error)?;
                    return Err(error);
                }
            }
            Ok(self)
        })
        .await
    }

    fn check(&self, client: &Client, now: u64) -> Result<()> {
        client.keys()?;
        let account = client.account()?;
        ensure(
            account.id == self.account.id
                && account.home == self.account.home
                && client.session_generation() == self.generation,
            Error::Locked,
        )?;
        let current = client.approval_record(&self.request.id)?;
        ensure(
            current.state == ApprovalState::AwaitingUser && now < current.expires_at,
            Error::Expired,
        )?;
        ensure(
            current.digest() == self.request.digest()
                && current.expires_at == self.request.expires_at,
            Error::IntegrityFailed,
        )
    }

    /// Read certified state and freeze the protocol mutation before authentication.
    /// # Errors
    /// Refuses stale requests, another pending account operation or unavailable keys.
    pub async fn prepare<H: Hold>(self, h: &H) -> Result<PreparedAccountApproval> {
        let h = Unlocked::new(h).await?;
        h.with(|c| self.check(c, now_ms())).await?;
        let principal = if matches!(
            self.request.action,
            Action::Principal {
                change: PrincipalChange::Register { .. }
            }
        ) {
            crate::agent::principal(&h).await?
        } else {
            None
        };
        let state = read_account(&self.ic, self.account.home, &self.account.id).await?;
        h.with(|c| {
            let now = now_ms();
            self.check(c, now)?;
            ensure(c.pending()?.is_none(), Error::Pending)?;
            c.observe(&self.account.id, &state.security)?;
            c.observe_hardware(&state.info);
            let mut action = self.request.action.clone();
            if let Action::ApprovalKey {
                proof: proof @ None,
                ..
            } = &mut action
            {
                *proof = Some(self.possession.ok_or(Error::Forbidden)?);
            }
            let (command, fixed) = command(c, &action, &state, principal.as_ref())?;
            let (mutation, message, purpose) =
                c.prepare_unsigned_mutation(&state, command, fixed, self.request.expires_at, now)?;
            let hardware = match purpose {
                Some(purpose) => Some(c.prepare_hardware_proof(
                    &state.info,
                    purpose,
                    message,
                    mutation.approval.expires_at,
                    now,
                )?),
                None => {
                    ensure(self.possession.is_some(), Error::PolicyStale)?;
                    None
                }
            };
            Ok(PreparedAccountApproval {
                pending: self,
                mutation,
                hardware,
            })
        })
        .await
    }
}

impl PreparedAccountApproval {
    /// Keep this guard in the async owner while its blocking signer runs.
    pub fn cancellation(&self) -> crate::secure::SigningCancellation {
        self.hardware
            .as_ref()
            .map(PreparedHardwareProof::cancellation)
            .unwrap_or_default()
    }

    /// Blocking authentication; this consumes its fixed plan outside Client.
    pub fn sign(self) -> ProvedAccountApproval {
        ProvedAccountApproval {
            pending: self.pending,
            mutation: self.mutation,
            answer: self.hardware.map(PreparedHardwareProof::sign),
        }
    }
}

fn check_state(mutation: &AccountMutation, state: &AccountState) -> Result<()> {
    let device = state.active(&mutation.approval.device_id)?;
    ensure(
        state.info.account_id == mutation.account_id
            && state.info.account_version == mutation.expected_version
            && state.info.security_epoch == mutation.approval.security_epoch
            && device.next_sequence == mutation.approval.sequence,
        Error::PolicyStale,
    )
}

fn outcome(
    mutation: &AccountMutation,
    reply: Result<Result<OperationReceipt>>,
) -> (Stage, Result<()>) {
    match reply {
        Ok(Ok(receipt))
            if receipt.id == mutation.approval.request_id
                && receipt.digest == digest("dmsg/account-operation/v2", mutation)
                && mutation.expected_version.checked_add(1) == Some(receipt.account_version) =>
        {
            (Stage::Confirmed, Ok(()))
        }
        Ok(Err(error)) if !matches!(error, Error::Expired | Error::ResultExpired) => {
            (Stage::Rejected, Err(error))
        }
        // No valid receipt, expired/pruned receipt or transport failure cannot
        // prove absence. The original signed journal remains the only retry.
        _ => (Stage::Unknown, Err(Error::ExecutionUnknown)),
    }
}

impl ProvedAccountApproval {
    /// Distinguish a hardware refusal from network or policy errors at later steps.
    pub fn hardware_error(&self) -> Option<Error> {
        self.answer.as_ref().and_then(HardwareProofAnswer::error)
    }

    /// Recheck state, atomically journal, send without Client, and settle together.
    /// # Errors
    /// Stale state never reconstructs the mutation; uncertain delivery retains its ID.
    pub async fn commit<H: Hold>(mut self, h: &H) -> Result<Value> {
        let h = Unlocked::new(h).await?;
        if self.hardware_error().is_some() {
            return h
                .with(|c| {
                    let now = now_ms();
                    self.pending.check(c, now)?;
                    let error = c
                        .complete_hardware_proof(self.answer.ok_or(Error::IntegrityFailed)?, now)
                        .err()
                        .ok_or(Error::IntegrityFailed)?;
                    c.approval_unconfirmed(&self.pending.request.id, &error)?;
                    Err(error)
                })
                .await;
        }
        let state = read_account(
            &self.pending.ic,
            self.pending.account.home,
            &self.pending.account.id,
        )
        .await?;
        let journal = h
            .with(|c| {
                let now = now_ms();
                self.pending.check(c, now)?;
                ensure(c.pending()?.is_none(), Error::Pending)?;
                ensure(
                    c.session()?.principal == self.pending.caller,
                    Error::AuthRequired,
                )?;
                c.observe(&self.pending.account.id, &state.security)?;
                c.observe_hardware(&state.info);
                check_state(&self.mutation, &state)?;
                self.mutation.approval.hardware = self
                    .answer
                    .map(|answer| c.complete_hardware_proof(answer, now))
                    .transpose()?;
                ensure(now < self.mutation.approval.expires_at, Error::Expired)?;
                self.pending.request.state = ApprovalState::Executing;
                self.pending.request.started = true;
                self.pending.request.decided_at = Some(now);
                self.pending.request.updated_at = now;
                self.pending.request.error = None;
                let journal = Journal {
                    home: self.pending.account.home,
                    caller: self.pending.caller,
                    account: Some(self.pending.account.id),
                    request: Request::Mutate(self.mutation.clone()),
                    stage: Stage::Unknown,
                };
                c.workspace.store.atomic(|_| {
                    c.save_journal(&journal)?;
                    c.save_approved_journal(
                        &self.pending.request.id,
                        &ApprovedJournal {
                            digest: self.pending.request.digest(),
                            journal: journal.clone(),
                            error: None,
                            followup_done: false,
                        },
                    )?;
                    c.write_approval(&self.pending.request)
                })?;
                c.running.insert(self.pending.request.id.clone(), now);
                Ok(journal)
            })
            .await?;
        let reply = self
            .pending
            .ic
            .update::<_, Result<OperationReceipt>>(
                journal.home,
                "mutate_account",
                (&self.mutation,),
            )
            .await;
        let (stage, result) = outcome(&self.mutation, reply);
        let settled = h
            .with(|c| {
                c.settle_approved_account(
                    &self.pending.request.id,
                    &journal,
                    stage,
                    result,
                    now_ms(),
                )
            })
            .await?;
        // The verified mutation receipt is the outcome. A later display refresh
        // failing does not turn a confirmed mutation into a failed execution.
        if settled.is_ok() {
            let _ = refreshed(&h).await;
        }
        settled
    }
}

/// Reconcile only the exact mutation stored with this approval. Network waits
/// release Client; later account operations cannot stand in for this receipt.
pub(crate) async fn reconcile_one<H: Hold>(h: &H, id: &str) -> Result<Value> {
    let h = Unlocked::new(h).await?;
    let (stored, ic) = h
        .with(|c| {
            let stored = c.approved_journal(id)?.ok_or(Error::NotFound)?;
            ensure(
                c.approval_record(id)?.digest() == stored.digest,
                Error::IntegrityFailed,
            )?;
            ensure(
                c.account()?.id == stored.journal.account.ok_or(Error::IntegrityFailed)?,
                Error::Locked,
            )?;
            let login = c.session()?;
            ensure(
                login.principal == stored.journal.caller,
                Error::AuthRequired,
            )?;
            Ok((stored, login.ic.clone()))
        })
        .await?;
    let journal = &stored.journal;
    let Request::Mutate(mutation) = &journal.request else {
        return Err(Error::IntegrityFailed);
    };
    let (stage, result) = match journal.stage {
        Stage::Confirmed => (Stage::Confirmed, Ok(())),
        Stage::Rejected => (
            Stage::Rejected,
            Err(stored.error.unwrap_or(Error::ExecutionUnknown)),
        ),
        Stage::Prepared | Stage::Unknown => {
            let found: Result<Result<OperationReceipt>> = ic
                .query(
                    journal.home,
                    "get_operation",
                    (&mutation.account_id, &mutation.approval.request_id),
                )
                .await;
            match found {
                Ok(Err(Error::ResultExpired)) => {
                    let state = read_account(&ic, journal.home, &mutation.account_id).await?;
                    if state.security.certified_at >= mutation.approval.expires_at
                        && state.info.account_version == mutation.expected_version
                        && state
                            .device(&mutation.approval.device_id)
                            .map(|d| d.next_sequence)
                            == Some(mutation.approval.sequence)
                    {
                        (Stage::Rejected, Err(Error::Expired))
                    } else if now_ms() < mutation.approval.expires_at {
                        // Same bytes, ID, proof and deadline. Never authenticate or
                        // reconstruct a different operation during reconciliation.
                        h.with(|c| {
                            ensure(
                                c.approval_record(id)?.digest() == stored.digest,
                                Error::IntegrityFailed,
                            )?;
                            Ok(())
                        })
                        .await?;
                        outcome(
                            mutation,
                            ic.update(journal.home, "mutate_account", (mutation,)).await,
                        )
                    } else {
                        (Stage::Unknown, Err(Error::ExecutionUnknown))
                    }
                }
                Ok(Ok(receipt)) => outcome(mutation, Ok(Ok(receipt))),
                // A query failure does not prove that an earlier update failed.
                _ => (Stage::Unknown, Err(Error::ExecutionUnknown)),
            }
        }
    };
    h.with(|c| c.settle_approved_account(id, journal, stage, result, now_ms()))
        .await?
}

#[cfg(test)]
mod tests;
