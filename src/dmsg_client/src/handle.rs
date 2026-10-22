//! Names (`src/dmsg_handle/README.md`): resolving a name with a certified
//! proof, claiming a name frozen in the legacy snapshot, buying a new one and
//! transferring one to another account.
//!
//! Each operation is an account-authorized intent (`AuthorizeHandle`, a
//! device-approved account mutation) that the registry consumes. The intent
//! is journaled with a fixed operation ID: a lost reply is settled by the
//! certified owner or the registry's operation record, and an intent is
//! authorized again only after the name is proven absent.
//!
//! A transfer needs both accounts' authorizations at once, each valid for
//! ten minutes ([`TRANSFER_AUTHORIZATION_MS`]): the owner prepares a token
//! naming the target, the target account accepts it, and the owner then
//! authorizes its side and moves the name while the acceptance lasts. Either
//! side reads whether the other has authorized from the other's user home.

use crate::account::HandleConfig;
use crate::client::Client;
use crate::hold::Hold;
use crate::ic::Ic;
use crate::now_ms;
use crate::payer::{approve_call, memo};
use candid::Principal;
use dmsg_protocol::{
    HANDLE_BUCKET_BITS, digest, handle_bucket, normalize_handle, price, registration_terms_digest,
};
use dmsg_types::{
    AccountId, Error, Hash, Result, ensure,
    handle::{
        HandleAction, HandleIntent, HandleOperation, HandlePhase, HandleRecord, LegacyReservation,
        Registration, SnapshotProgress, TRANSFER_AUTHORIZATION_MS,
    },
    user::AccountCommand,
};
use icrc_ledger_types::{
    icrc1::account::Account,
    icrc2::{allowance::Allowance, approve::ApproveArgs},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

const JOURNAL: &str = "handle:";
/// Format of a transfer token.
pub const TRANSFER: &str = "dmsg-handle-transfer/1";
/// Most names the registry lists or proves in one call.
const PAGE: u16 = 64;

#[derive(Serialize, Deserialize)]
struct TransferPacket {
    format: String,
    from: HandleIntent,
    accept: HandleIntent,
}

/// What a name operation does.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Claim a name frozen in the legacy snapshot.
    Claim {
        /// The snapshot.
        snapshot: Hash,
    },
    /// Move a name from `from`'s account to `accept`'s: the owner commits
    /// with `from`, the target authorizes `accept`.
    Transfer {
        /// The owner's intent.
        from: Box<HandleIntent>,
        /// The target's intent.
        accept: Box<HandleIntent>,
    },
    /// Register a new name, paid from the payer.
    Register {
        /// The registration and its fixed charge.
        registration: Box<Registration>,
        /// The registry's ledger.
        ledger: Principal,
        /// What the registry charges, fee included.
        total: u128,
    },
}

/// Where a name operation stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Prepared; nothing authorized.
    Prepared,
    /// Sent; the registry's answer was not seen.
    Unknown,
    /// This account accepted a transfer; the owner has yet to move the name.
    Accepted,
    /// The name is this account's, or the owner's transfer committed.
    Done,
    /// The registry definitely refused it.
    Rejected,
}

/// A name operation's journal, sealed under the LDK.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HandleJob {
    /// Operation ID, hex.
    pub id: String,
    /// The name.
    pub name: String,
    /// The calling Principal: the legacy owner or the payer.
    pub caller: Principal,
    /// The intent the account authorizes.
    pub intent: HandleIntent,
    /// What it does.
    pub kind: Kind,
    /// Progress.
    pub phase: Phase,
    /// When this device prepared it, in Unix milliseconds.
    #[serde(default)]
    pub created_at_ms: Option<u64>,
    /// When an accepted transfer's authorization lapses, in Unix milliseconds.
    #[serde(default)]
    pub until_ms: Option<u64>,
}

impl HandleJob {
    /// The commitment a person approves.
    pub fn digest(&self) -> Hash {
        digest(
            "dmsg/handle-approval/v1",
            &(&self.intent, &self.kind, self.caller),
        )
    }

    /// The operation for display. A registration lists what the registry
    /// charges, the ledger fee in it, and at most what the payer spends with
    /// the allowance's own fee; a transfer names both accounts.
    pub fn view(&self) -> Json {
        let (total, fee) = match &self.kind {
            Kind::Register {
                total,
                registration,
                ..
            } => (Some(*total), Some(registration.fee)),
            _ => (None, None),
        };
        let (from, to) = match &self.kind {
            Kind::Transfer { from, accept } => (
                Some(from.account_id.to_string()),
                Some(accept.account_id.to_string()),
            ),
            _ => (None, None),
        };
        json!({
            "id": self.id,
            "name": match &self.kind {
                Kind::Register { registration, .. } => registration.display_handle.as_deref().unwrap_or(&self.name),
                _ => &self.name,
            },
            "action": format!("{:?}", self.intent.action),
            "caller": self.caller.to_text(),
            // Who calls the registry: the legacy holder claims, the payer
            // pays, the login moves a name.
            "caller_role": match self.kind {
                Kind::Claim { .. } => "holder",
                Kind::Register { .. } => "payer",
                Kind::Transfer { .. } => "login",
            },
            "total": total.map(|t| t.to_string()),
            "fee": fee.map(|f| f.to_string()),
            "most": total.zip(fee).map(|(t, f)| (t + f).to_string()),
            "from": from,
            "to": to,
            "phase": self.phase,
            "expired": self.phase == Phase::Accepted
                && self.until_ms.is_some_and(|until| until <= now_ms()),
            "until_ms": self.until_ms,
            "created_at_ms": self.created_at_ms,
            "digest": hex::encode(*self.digest()),
        })
    }

    /// The token of a transfer this account offers, for the target to accept.
    pub fn token(&self) -> Option<String> {
        match &self.kind {
            Kind::Transfer { from, accept } if self.intent == **from => {
                Some(transfer_token(from, accept))
            }
            _ => None,
        }
    }
}

/// The token that hands a transfer's two intents to its target.
fn transfer_token(from: &HandleIntent, accept: &HandleIntent) -> String {
    crate::codec::b64(&crate::codec::canonical(&TransferPacket {
        format: TRANSFER.into(),
        from: from.clone(),
        accept: accept.clone(),
    }))
}

/// Whether `caller` may claim the frozen name `r`: its legacy owner, or for
/// a name owned by its own name principal one of the frozen admins.
fn may_claim(r: &LegacyReservation, caller: Principal) -> bool {
    if r.legacy_name_principal == Some(r.legacy_owner) {
        caller != r.legacy_owner && r.frozen_admins.contains(&caller)
    } else {
        caller == r.legacy_owner
    }
}

/// The certified records of `names`, in order; `None` for a name nobody owns.
/// An empty list asks the registry for nothing: it refuses an empty batch.
async fn certified_records(
    ic: &Ic,
    registry: Principal,
    names: &[String],
) -> Result<Vec<Option<HandleRecord>>> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let batch: Result<dmsg_types::CertifiedBatch> = ic
        .query(registry, "resolve_handle_certified", (names,))
        .await?;
    let batch = batch?;
    let root = ic.root_key();
    let now = now_ms();
    names
        .iter()
        .map(|name| {
            let bits = bucket_path(name);
            let mut path: Vec<&[u8]> = bits.iter().map(|b| b.as_slice()).collect();
            path.push(name.as_bytes());
            let value = crate::certified::certified_path(
                &batch,
                name.as_bytes(),
                &path,
                registry,
                &root,
                now,
            )?;
            let Some(value) = value else {
                return Ok(None);
            };
            let record: HandleRecord = crate::codec::decode(&value, 4096)?;
            ensure(
                record.handle == *name
                    && record.version > 0
                    && record.display_handle.as_ref().is_none_or(|display| {
                        normalize_handle(display).ok().as_deref() == Some(name)
                    }),
                Error::IntegrityFailed,
            )?;
            Ok(Some(record))
        })
        .collect()
}

/// When `account`'s authorization of the name operation `op_id` lapses, if
/// it is live, read from whichever user home holds the account.
async fn authorized_until(
    ic: &Ic,
    registry: Principal,
    account: AccountId,
    op_id: Hash,
) -> Result<Option<u64>> {
    let config: HandleConfig = ic.query(registry, "get_handle_config", ()).await?;
    for home in config.user_homes {
        let found: Result<Option<u64>> = ic
            .query(home, "get_handle_authorization", (account, op_id))
            .await?;
        match found {
            Err(Error::NotFound) => {}
            found => return found,
        }
    }
    Err(Error::NotFound)
}

/// The commitment both sides of a transfer authorize.
fn transfer_terms(
    registry: Principal,
    name: &str,
    from: &AccountId,
    to: &AccountId,
    version: u64,
    op_id: Hash,
) -> Hash {
    digest(
        "dmsg/handle-transfer/v1",
        &(registry, name, from, to, version, op_id),
    )
}

/// One `[0]` or `[1]` label per bucket bit, most significant first.
fn bucket_path(name: &str) -> Vec<[u8; 1]> {
    let bucket = handle_bucket(name);
    (0..HANDLE_BUCKET_BITS)
        .rev()
        .map(|bit| [((bucket >> bit) & 1) as u8])
        .collect()
}

impl Client {
    /// Pick a live holder identity, including one just used to bind a
    /// login. A binding alone never substitutes for its signing identity.
    fn claim_login(&self, reservation: &LegacyReservation, now: u64) -> Option<Principal> {
        self.login
            .as_ref()
            .filter(|l| l.expires_at > now && may_claim(reservation, l.principal))
            .map(|l| l.principal)
            .or_else(|| {
                self.bound_logins
                    .iter()
                    .find(|(p, (_, expires))| *expires > now && may_claim(reservation, **p))
                    .map(|(p, _)| *p)
            })
    }

    /// Use the holder approved in the job, even if the account's current
    /// login differs. An expired or discarded delegation needs a new login.
    fn claim_ic(&self, caller: Principal, now: u64) -> Result<Ic> {
        self.login
            .as_ref()
            .filter(|l| l.principal == caller && l.expires_at > now)
            .map(|l| l.ic.clone())
            .or_else(|| {
                self.bound_logins
                    .get(&caller)
                    .filter(|(_, expires)| *expires > now)
                    .map(|(ic, _)| ic.clone())
            })
            .ok_or(Error::AuthRequired)
    }

    fn handle_job(&self, id: &str) -> Result<HandleJob> {
        self.workspace
            .private(self.keys()?, &format!("{JOURNAL}{id}"))?
            .ok_or(Error::NotFound)
    }

    fn save_handle_job(&self, job: &HandleJob) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{JOURNAL}{}", job.id), job)
    }

    /// The certified owner of a name, if any.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for an invalid name.
    pub async fn handle_resolve(&mut self, name: &str) -> Result<Option<HandleRecord>> {
        resolve(&self.hold(), name).await
    }

    /// Prepare claiming a name a live login owned in the legacy registry,
    /// including an identity just bound here; the claim runs on approval.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` without a live frozen holder identity and
    /// `Error::Pending` before the snapshot is sealed.
    pub async fn handle_claim_prepare(&mut self, name: &str) -> Result<HandleJob> {
        let name = normalize_handle(name)?;
        let account = self.account()?.id;
        let ic = self.session()?.ic.clone();
        let progress: SnapshotProgress = ic
            .query(self.config.handle, "snapshot_progress", ())
            .await?;
        let snapshot = progress
            .snapshot
            .filter(|_| progress.sealed)
            .ok_or(Error::Pending)?;
        let reservation: Result<Option<LegacyReservation>> = ic
            .query(self.config.handle, "get_legacy_reservation", (&name,))
            .await?;
        let reservation = reservation?.ok_or(Error::NotFound)?;
        ensure(reservation.handle == name, Error::IntegrityFailed)?;
        ensure(!reservation.quarantined, Error::Locked)?;
        let now = now_ms();
        let caller = self
            .claim_login(&reservation, now)
            .ok_or(Error::Forbidden)?;
        let intent = HandleIntent {
            handle_canister: self.config.handle,
            action: HandleAction::ClaimLegacy,
            account_id: account,
            target_account: None,
            handle: name.clone(),
            expected_version: 0,
            op_id: Hash::new(crate::crypto::random_id()),
            terms_digest: digest(
                "dmsg/legacy-claim/v1",
                &(snapshot.snapshot_id, &reservation, &account),
            ),
        };
        let job = HandleJob {
            id: hex::encode(*intent.op_id),
            name,
            caller,
            intent,
            kind: Kind::Claim {
                snapshot: snapshot.snapshot_id,
            },
            phase: Phase::Prepared,
            created_at_ms: Some(now),
            until_ms: None,
        };
        self.save_handle_job(&job)?;
        Ok(job)
    }

    /// Prepare registering a new name, paid from the payer; it runs once a
    /// person approves it.
    ///
    /// # Errors
    /// Returns `Error::IdempotencyConflict` for a name already owned and
    /// `Error::VersionConflict` for one reserved by the legacy snapshot.
    pub async fn handle_register_prepare(&mut self, name: &str) -> Result<HandleJob> {
        let display_handle = name.to_string();
        let name = normalize_handle(name)?;
        ensure(
            self.handle_resolve(&name).await?.is_none(),
            Error::IdempotencyConflict,
        )?;
        let ic = self.reader().await?;
        let progress: SnapshotProgress = ic
            .query(self.config.handle, "snapshot_progress", ())
            .await?;
        ensure(progress.sealed, Error::LegacyWriteDisabled)?;
        let reserved: Result<Option<LegacyReservation>> = ic
            .query(self.config.handle, "get_legacy_reservation", (&name,))
            .await?;
        ensure(reserved?.is_none(), Error::VersionConflict)?;
        let config: HandleConfig = ic
            .query(self.config.handle, "get_handle_config", ())
            .await?;
        ensure(
            config.user_homes.contains(&self.account()?.home),
            Error::IntegrityFailed,
        )?;
        let total = price(&name);
        ensure(total > config.ledger_fee, Error::FeeBlocked)?;
        let (payer, _) = self.payer().await?;
        let account = Account {
            owner: payer,
            subaccount: None,
        };
        let intent = HandleIntent {
            handle_canister: self.config.handle,
            action: HandleAction::Register,
            account_id: self.account()?.id,
            target_account: None,
            handle: name.clone(),
            expected_version: 0,
            op_id: Hash::new(crate::crypto::random_id()),
            terms_digest: registration_terms_digest(
                config.ledger,
                &account,
                total - config.ledger_fee,
                config.ledger_fee,
                Some(&display_handle),
            ),
        };
        let job = HandleJob {
            id: hex::encode(*intent.op_id),
            name,
            caller: payer,
            intent: intent.clone(),
            kind: Kind::Register {
                registration: Box::new(Registration {
                    intent,
                    payer: account,
                    fee: config.ledger_fee,
                    display_handle: Some(display_handle),
                }),
                ledger: config.ledger,
                total,
            },
            phase: Phase::Prepared,
            created_at_ms: Some(now_ms()),
            until_ms: None,
        };
        self.save_handle_job(&job)?;
        Ok(job)
    }

    /// Prepare transferring a name this account owns to `target`: a token
    /// for the target account to accept. Once it has, the owner approves
    /// the transfer within ten minutes.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a name this account does not own.
    pub async fn handle_transfer_prepare(
        &mut self,
        name: &str,
        target: &AccountId,
    ) -> Result<(HandleJob, String)> {
        let name = normalize_handle(name)?;
        let owner = self.account()?.id;
        let record = self.handle_resolve(&name).await?.ok_or(Error::NotFound)?;
        ensure(
            record.owner_account == owner && *target != owner,
            Error::Forbidden,
        )?;
        let op_id = Hash::new(crate::crypto::random_id());
        let registry = self.config.handle;
        let from = HandleIntent {
            handle_canister: registry,
            action: HandleAction::Transfer,
            account_id: owner,
            target_account: Some(*target),
            handle: name.clone(),
            expected_version: record.version,
            op_id,
            terms_digest: transfer_terms(registry, &name, &owner, target, record.version, op_id),
        };
        let accept = HandleIntent {
            action: HandleAction::AcceptTransfer,
            account_id: *target,
            target_account: Some(owner),
            ..from.clone()
        };
        let token = transfer_token(&from, &accept);
        let job = HandleJob {
            id: hex::encode(*op_id),
            name,
            caller: self.session()?.principal,
            intent: from.clone(),
            kind: Kind::Transfer {
                from: Box::new(from),
                accept: Box::new(accept),
            },
            phase: Phase::Prepared,
            created_at_ms: Some(now_ms()),
            until_ms: None,
        };
        self.save_handle_job(&job)?;
        Ok((job, token))
    }

    /// Prepare accepting a transfer token naming this account; it runs once
    /// a person approves it.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for a token that does not hold
    /// together and `Error::Forbidden` for one naming another account.
    pub async fn handle_accept_prepare(&mut self, token: &str) -> Result<HandleJob> {
        ensure(token.len() <= 4096, Error::QuotaExceeded)?;
        let packet: TransferPacket = crate::codec::decode(&crate::codec::unb64(token)?, 4096)?;
        let (from, accept) = (packet.from, packet.accept);
        let registry = self.config.handle;
        let terms = transfer_terms(
            registry,
            &from.handle,
            &from.account_id,
            &accept.account_id,
            from.expected_version,
            from.op_id,
        );
        ensure(
            packet.format == TRANSFER
                && from.action == HandleAction::Transfer
                && accept.action == HandleAction::AcceptTransfer
                && from.handle_canister == registry
                && accept.handle_canister == registry
                && normalize_handle(&from.handle)? == from.handle
                && from.handle == accept.handle
                && from.expected_version == accept.expected_version
                && from.op_id == accept.op_id
                && from.target_account == Some(accept.account_id)
                && accept.target_account == Some(from.account_id)
                && from.terms_digest == terms
                && accept.terms_digest == terms,
            Error::IntegrityFailed,
        )?;
        ensure(accept.account_id == self.account()?.id, Error::Forbidden)?;
        let record = self
            .handle_resolve(&from.handle)
            .await?
            .ok_or(Error::NotFound)?;
        ensure(
            record.owner_account == from.account_id && record.version == from.expected_version,
            Error::VersionConflict,
        )?;
        let job = HandleJob {
            id: hex::encode(*accept.op_id),
            name: from.handle.clone(),
            caller: self.session()?.principal,
            intent: accept.clone(),
            kind: Kind::Transfer {
                from: Box::new(from),
                accept: Box::new(accept),
            },
            phase: Phase::Prepared,
            created_at_ms: Some(now_ms()),
            until_ms: None,
        };
        self.save_handle_job(&job)?;
        Ok(job)
    }

    /// A name operation's journal.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown operation.
    pub fn handle_operation(&self, id: &str) -> Result<HandleJob> {
        self.handle_job(id)
    }

    /// Every name operation of this device, newest first.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn handle_jobs(&self) -> Result<Vec<HandleJob>> {
        let mut jobs = self
            .workspace
            .store
            .sealed_keys(JOURNAL)?
            .iter()
            .map(|key| {
                self.workspace
                    .private::<HandleJob>(self.keys()?, key)?
                    .ok_or(Error::IntegrityFailed)
            })
            .collect::<Result<Vec<_>>>()?;
        // Journals from before the field come last.
        jobs.sort_by_key(|job| std::cmp::Reverse(job.created_at_ms));
        Ok(jobs)
    }

    /// Every name operation of this device for display, newest first; a
    /// transfer this account offers carries its token again.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn handle_operations(&self) -> Result<Vec<Json>> {
        Ok(self.handle_jobs()?.iter().map(operation_view).collect())
    }

    /// The token of a transfer this account offered and has not moved yet.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for any other operation.
    pub fn handle_token(&self, id: &str) -> Result<String> {
        let job = self.handle_job(id)?;
        ensure(job.phase == Phase::Prepared, Error::Forbidden)?;
        job.token().ok_or(Error::Forbidden)
    }

    /// Forget an operation that moved nothing: one never authorized, a
    /// transfer offered or accepted that the owner never committed, or one
    /// the registry refused. An acceptance already authorized lapses by
    /// itself within [`TRANSFER_AUTHORIZATION_MS`].
    ///
    /// # Errors
    /// Returns `Error::Forbidden` while the outcome may still change.
    pub fn handle_forget(&self, id: &str) -> Result<()> {
        let job = self.handle_job(id)?;
        ensure(
            matches!(
                job.phase,
                Phase::Prepared | Phase::Accepted | Phase::Rejected
            ),
            Error::Forbidden,
        )?;
        self.workspace.delete_private(&format!("{JOURNAL}{id}"))
    }

    /// Settle a registration whose charge is unknown with the ledger block
    /// that paid it.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for a block that is not this
    /// registration's charge.
    pub async fn handle_reconcile(&mut self, id: &str, block: u64) -> Result<Json> {
        let job = self.handle_job(id)?;
        ensure(
            matches!(job.kind, Kind::Register { .. }) && job.phase == Phase::Unknown,
            Error::VersionConflict,
        )?;
        let (_, ic) = self.payer().await?;
        let operation: Result<HandleOperation> = ic
            .update(
                self.config.handle,
                "reconcile_handle_charge",
                (job.intent.account_id, job.intent.op_id, block),
            )
            .await?;
        self.handle_settle(job, operation?).await
    }

    /// Run an approved name operation, or settle one whose outcome is
    /// unknown: the certified owner first, then the registry's record.
    ///
    /// # Errors
    /// Returns `Error::VersionConflict` once the registry refused it and
    /// `Error::ExecutionUnknown` while the outcome is unknown.
    pub(crate) async fn handle_execute(&mut self, id: &str) -> Result<Json> {
        let mut job = self.handle_job(id)?;
        match job.phase {
            Phase::Done => return Ok(job.view()),
            Phase::Rejected => return Err(Error::VersionConflict),
            _ => {}
        }
        if let Kind::Transfer { .. } = job.kind {
            return self.handle_transfer(job).await;
        }
        if let Some(record) = self.handle_resolve(&job.name).await? {
            ensure(
                record.owner_account == job.intent.account_id,
                Error::IdempotencyConflict,
            )?;
            job.phase = Phase::Done;
            self.save_handle_job(&job)?;
            return Ok(job.view());
        }
        if let Kind::Register { .. } = job.kind {
            let (_, ic) = self.payer().await?;
            let found: Result<HandleOperation> = ic
                .query(
                    self.config.handle,
                    "get_handle_operation",
                    (job.intent.account_id, job.intent.op_id),
                )
                .await?;
            if let Ok(operation) = found {
                return self.handle_settle(job, operation).await;
            }
        }
        // The name is proven absent: authorize the same intent again.
        if let Kind::Claim { .. } = job.kind {
            self.claim_ic(job.caller, now_ms())?;
        }
        if self.pending()?.is_some() {
            self.resume().await?;
        }
        if let Kind::Register { ledger, total, .. } = &job.kind {
            self.handle_allowance(&job, *ledger, *total).await?;
        }
        self.mutate(
            AccountCommand::AuthorizeHandle {
                intent: job.intent.clone(),
            },
            None,
        )
        .await?;
        let ic = match &job.kind {
            Kind::Claim { .. } => self.claim_ic(job.caller, now_ms())?,
            Kind::Register { .. } => self.payer().await?.1,
            Kind::Transfer { .. } => return Err(Error::IntegrityFailed),
        };
        job.phase = Phase::Unknown;
        self.save_handle_job(&job)?;
        match job.kind.clone() {
            Kind::Transfer { .. } => Err(Error::IntegrityFailed),
            Kind::Claim { snapshot } => {
                let claimed: Result<Result<HandleRecord>> = ic
                    .update(
                        self.config.handle,
                        "claim_legacy_handle",
                        (&job.intent, snapshot),
                    )
                    .await;
                let record = match claimed {
                    Ok(Ok(record)) => record,
                    Ok(Err(Error::ExecutionUnknown)) | Err(_) => {
                        return Err(Error::ExecutionUnknown);
                    }
                    Ok(Err(error)) => {
                        job.phase = Phase::Rejected;
                        self.save_handle_job(&job)?;
                        return Err(error);
                    }
                };
                ensure(
                    record.handle == job.name
                        && record.version == 1
                        && record.owner_account == job.intent.account_id,
                    Error::IntegrityFailed,
                )?;
                job.phase = Phase::Done;
                self.save_handle_job(&job)?;
                Ok(job.view())
            }
            Kind::Register { registration, .. } => {
                let registered: Result<Result<HandleOperation>> = ic
                    .update(self.config.handle, "register_handle", (&registration,))
                    .await;
                match registered {
                    Ok(Ok(operation)) => self.handle_settle(job, operation).await,
                    Ok(Err(Error::ExecutionUnknown)) | Err(_) => Err(Error::ExecutionUnknown),
                    Ok(Err(error)) => {
                        job.phase = Phase::Rejected;
                        self.save_handle_job(&job)?;
                        Err(error)
                    }
                }
            }
        }
    }

    /// Authorize this account's side of a transfer; the owner's side then
    /// moves the name. A lost reply is settled by the certified owner or by
    /// calling again, which the registry answers from its record.
    async fn handle_transfer(&mut self, mut job: HandleJob) -> Result<Json> {
        let Kind::Transfer { from, accept } = job.kind.clone() else {
            return Err(Error::IntegrityFailed);
        };
        let commit = job.intent == *from;
        let record = self.handle_resolve(&job.name).await?;
        if record.as_ref().map(|r| r.owner_account) == Some(accept.account_id) {
            job.phase = Phase::Done;
            self.save_handle_job(&job)?;
            return Ok(job.view());
        }
        ensure(
            record.is_some_and(|r| {
                r.owner_account == from.account_id && r.version == from.expected_version
            }),
            Error::VersionConflict,
        )?;
        if commit {
            // The target accepts first; the owner's side alone moves nothing.
            let ic = self.reader().await?;
            let accepted =
                authorized_until(&ic, self.config.handle, accept.account_id, accept.op_id).await?;
            ensure(accepted.is_some(), Error::Pending)?;
        }
        if self.pending()?.is_some() {
            self.resume().await?;
        }
        self.mutate(
            AccountCommand::AuthorizeHandle {
                intent: job.intent.clone(),
            },
            None,
        )
        .await?;
        if !commit {
            // Accepted: the owner moves the name while the acceptance lasts.
            job.phase = Phase::Accepted;
            job.until_ms = Some(now_ms() + TRANSFER_AUTHORIZATION_MS);
            self.save_handle_job(&job)?;
            return Ok(job.view());
        }
        job.phase = Phase::Unknown;
        self.save_handle_job(&job)?;
        let ic = self.session()?.ic.clone();
        let moved: Result<Result<HandleRecord>> = ic
            .update(self.config.handle, "transfer_handle", (&*from, &*accept))
            .await;
        let record = match moved {
            Ok(Ok(record)) => record,
            Ok(Err(Error::ExecutionUnknown)) | Err(_) => return Err(Error::ExecutionUnknown),
            Ok(Err(error)) => {
                job.phase = Phase::Rejected;
                self.save_handle_job(&job)?;
                return Err(error);
            }
        };
        ensure(
            record.handle == job.name && record.owner_account == accept.account_id,
            Error::IntegrityFailed,
        )?;
        job.phase = Phase::Done;
        self.save_handle_job(&job)?;
        Ok(job.view())
    }

    /// Let the registry charge the payer `total` once, unless an allowance
    /// already covers it.
    async fn handle_allowance(
        &mut self,
        job: &HandleJob,
        ledger: Principal,
        total: u128,
    ) -> Result<()> {
        let (payer, ic) = self.payer().await?;
        let reader = crate::payer::Ledger {
            ic: &ic,
            canister: ledger,
        };
        let current = reader.allowance(payer, self.config.handle).await?;
        let allowance = u128::try_from(current.allowance.0.clone()).unwrap_or(0);
        if allowance_covers(&current, total, now_ms()) {
            return Ok(());
        }
        let fee = reader.fee().await?;
        let registry = self.config.handle;
        let create = || {
            approve_call(&ApproveArgs {
                from_subaccount: None,
                spender: Account {
                    owner: registry,
                    subaccount: None,
                },
                amount: total.into(),
                expected_allowance: Some(allowance.into()),
                expires_at: Some(now_ms() * 1_000_000 + 3_600_000_000_000),
                fee: Some(fee.into()),
                memo: Some(memo(digest("dmsg/handle-allowance/v1", &job.id).as_slice())),
                created_at_time: Some(now_ms() * 1_000_000),
            })
        };
        self.pay(&format!("handle-allowance:{}", job.id), ledger, fee, create)
            .await
            .map(|_| ())
    }

    /// Follow a registration until it commits or is refused.
    async fn handle_settle(
        &mut self,
        mut job: HandleJob,
        operation: HandleOperation,
    ) -> Result<Json> {
        let Kind::Register { registration, .. } = &job.kind else {
            return Err(Error::IntegrityFailed);
        };
        ensure(
            operation.registration == **registration,
            Error::IntegrityFailed,
        )?;
        let operation = match operation.phase {
            HandlePhase::Charging | HandlePhase::ChargeUnknown => {
                let (_, ic) = self.payer().await?;
                let committed: Result<HandleOperation> = ic
                    .update(
                        self.config.handle,
                        "commit_handle",
                        (job.intent.account_id, job.intent.op_id),
                    )
                    .await?;
                committed?
            }
            _ => operation,
        };
        match operation.phase {
            HandlePhase::Committed => {
                job.phase = Phase::Done;
                self.save_handle_job(&job)?;
                Ok(job.view())
            }
            HandlePhase::Rejected { reason } => {
                job.phase = Phase::Rejected;
                self.save_handle_job(&job)?;
                Err(Error::InvalidInput(reason))
            }
            _ => Err(Error::ExecutionUnknown),
        }
    }
}

/// An operation for the list: a transfer this account offers carries its
/// token while it can still be accepted.
fn operation_view(job: &HandleJob) -> Json {
    let mut view = job.view();
    if job.phase == Phase::Prepared
        && let Some(token) = job.token()
    {
        view["token"] = token.into();
    }
    view
}

/// The certified owner of a name, if any, read without the client.
///
/// # Errors
/// Returns `Error::InvalidInput` for an invalid name.
pub async fn resolve<H: Hold>(h: &H, name: &str) -> Result<Option<HandleRecord>> {
    let name = normalize_handle(name)?;
    let registry = h.with(|c| Ok(c.config.handle)).await?;
    let ic = h.reader().await?;
    Ok(
        certified_records(&ic, registry, std::slice::from_ref(&name))
            .await?
            .pop()
            .flatten(),
    )
}

/// The spelling to show for a name quote: its certified display spelling, the
/// requested spelling while the name is still available for registration, or
/// the canonical name when it is owned without one or reserved by the legacy
/// snapshot.
fn price_display<'a>(
    owner: Option<&'a HandleRecord>,
    reserved: bool,
    requested: &'a str,
    canonical: &'a str,
) -> &'a str {
    owner
        .and_then(|record| record.display_handle.as_deref())
        .or_else(|| (!reserved && owner.is_none()).then_some(requested))
        .unwrap_or(canonical)
}

fn allowance_covers(allowance: &Allowance, total: u128, now: u64) -> bool {
    allowance.allowance >= total
        && allowance
            .expires_at
            .is_none_or(|t| t > (now + dmsg_types::MINUTE) * 1_000_000)
}

/// Ask to approve a name operation, checking a registration's full cost first.
/// Public ledger queries run without the shared client or login delegation.
pub async fn request<H: Hold>(h: &H, source: &str, id: &str) -> Result<Json> {
    let job = h.with(|c| c.handle_operation(id)).await?;
    let funds = if let Kind::Register {
        registration,
        ledger,
        total,
    } = &job.kind
    {
        let (payer, _) = crate::payer::payer(h).await?;
        ensure(payer == job.caller, Error::Forbidden)?;
        let ic = h.anonymous().await?;
        let reader = crate::payer::Ledger {
            ic: &ic,
            canister: *ledger,
        };
        let (balance, fee, allowance) = futures_util::try_join!(
            reader.balance(payer),
            reader.fee(),
            reader.allowance(payer, job.intent.handle_canister)
        )?;
        ensure(fee == registration.fee, Error::FeeBlocked)?;
        Some((balance, fee, allowance, *total))
    } else {
        None
    };
    h.with(|c| {
        ensure(c.handle_operation(id)? == job, Error::VersionConflict)?;
        ensure(
            c.account()?.id == job.intent.account_id
                && c.config.handle == job.intent.handle_canister,
            Error::Forbidden,
        )?;
        let now = now_ms();
        if let Some((balance, fee, allowance, total)) = funds {
            c.ensure_payer(job.caller)?;
            let needed = total
                .checked_add(if allowance_covers(&allowance, total, now) {
                    0
                } else {
                    fee
                })
                .ok_or(Error::IntegrityFailed)?;
            ensure(balance >= needed, crate::insufficient_funds())?;
        }
        let request = c.approval_create_at(
            source,
            crate::approval::Action::Handle {
                id: id.into(),
                digest: job.digest(),
            },
            now,
            now + crate::approval::HIGH_RISK_TTL_MS,
        )?;
        c.approval_view(&request)
    })
    .await
}

/// A name's price and who may take it, read without the client: its owner,
/// what registering it costs the payer, and for a name frozen in the legacy
/// snapshot whether a live login can sign as its holder.
///
/// # Errors
/// Returns `Error::InvalidInput` for an invalid name.
pub async fn price_view<H: Hold>(h: &H, name: &str) -> Result<Json> {
    let requested = name;
    let name = normalize_handle(name)?;
    let registry = h.with(|c| Ok(c.config.handle)).await?;
    let ic = h.reader().await?;
    let reserved: Result<Option<LegacyReservation>> = ic
        .query(registry, "get_legacy_reservation", (&name,))
        .await?;
    let reserved = reserved?;
    let config: HandleConfig = ic.query(registry, "get_handle_config", ()).await?;
    let owner = certified_records(&ic, registry, std::slice::from_ref(&name))
        .await?
        .pop()
        .flatten();
    let total = price(&name);
    let display_handle = price_display(owner.as_ref(), reserved.is_some(), requested, &name);
    let mut view = json!({
        "name": name,
        "display_handle": display_handle,
        "price": total.to_string(),
        "fee": config.ledger_fee.to_string(),
        // The allowance the registry charges from costs one more fee.
        "most": (total + config.ledger_fee).to_string(),
        "ledger": config.ledger.to_text(),
        "owner": owner.map(|r| r.owner_account.to_string()),
        "reserved": reserved.is_some(),
    });
    if let Some(r) = reserved {
        let (login, caller) = h
            .with(|c| {
                let now = now_ms();
                let login = c.login.as_ref().filter(|l| l.expires_at > now);
                Ok((login.map(|l| l.principal), c.claim_login(&r, now)))
            })
            .await?;
        let by_admins = r.legacy_name_principal == Some(r.legacy_owner);
        view["claim"] = json!({
            "claimable": !r.quarantined && login.is_some() && caller.is_some(),
            "quarantined": r.quarantined,
            "login": login.map(|p| p.to_text()),
            // Who must sign in to claim it: the legacy owner, or one of the
            // name's frozen admins.
            "holders": if by_admins {
                r.frozen_admins.iter().map(|p| p.to_text()).collect::<Vec<_>>()
            } else {
                vec![r.legacy_owner.to_text()]
            },
        });
    }
    Ok(view)
}

/// This account's names as the registry lists them, each proven by its
/// certified record, so every device of the account sees them.
///
/// # Errors
/// Returns `Error::AuthRequired` without an account.
pub async fn mine<H: Hold>(h: &H) -> Result<Vec<Json>> {
    let (registry, account) = h.with(|c| Ok((c.config.handle, c.account()?.id))).await?;
    let ic = h.reader().await?;
    let mut names = Vec::new();
    let mut after: Option<String> = None;
    loop {
        let page: Result<Vec<HandleRecord>> = ic
            .query(registry, "account_handles", (account, &after, PAGE))
            .await?;
        let page: Vec<String> = page?.into_iter().map(|r| r.handle).collect();
        let full = page.len() == usize::from(PAGE);
        after = page.last().cloned();
        // The list is uncertified: only a certified record counts.
        for record in certified_records(&ic, registry, &page)
            .await?
            .into_iter()
            .flatten()
        {
            if record.owner_account == account {
                names.push(json!({"name": record.handle, "display_handle": record.display_handle.as_ref().unwrap_or(&record.handle), "version": record.version}));
            }
        }
        if !full {
            break;
        }
    }
    Ok(names)
}

/// Every name operation of this device, newest first, brought up to date
/// without the client: an accepted transfer whose name arrived is done, and
/// a transfer this account offers says until when the target's acceptance
/// lasts, so the owner knows to commit.
///
/// # Errors
/// Returns `Error::Locked` while locked.
pub async fn operations<H: Hold>(h: &H) -> Result<Vec<Json>> {
    let (jobs, registry) = h.with(|c| Ok((c.handle_jobs()?, c.config.handle))).await?;
    let ic = h.reader().await?;
    let mut views = Vec::with_capacity(jobs.len());
    for mut job in jobs {
        let mut accepted = None;
        if let Kind::Transfer { from, accept } = &job.kind {
            let offered = job.intent == **from;
            if job.phase == Phase::Accepted || (offered && job.phase == Phase::Prepared) {
                let moved = certified_records(&ic, registry, std::slice::from_ref(&job.name))
                    .await?
                    .pop()
                    .flatten()
                    .is_some_and(|r| r.owner_account == accept.account_id);
                if moved && job.phase == Phase::Accepted {
                    job.phase = Phase::Done;
                    let done = job.clone();
                    h.with(|c| match c.handle_job(&done.id) {
                        Ok(saved) if saved.phase == Phase::Accepted => c.save_handle_job(&done),
                        _ => Ok(()),
                    })
                    .await?;
                } else if offered && !moved {
                    accepted = authorized_until(&ic, registry, accept.account_id, accept.op_id)
                        .await
                        .ok()
                        .flatten();
                }
            }
        }
        let mut view = operation_view(&job);
        if let Some(until) = accepted {
            view["accepted_until_ms"] = until.into();
        }
        views.push(view);
    }
    Ok(views)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TempDir, unlocked};

    #[test]
    fn registration_allowance_must_cover_the_charge_and_execution_window() {
        let now = 1_000_000;
        let mut allowance = Allowance {
            allowance: 100u128.into(),
            expires_at: None,
        };
        assert!(allowance_covers(&allowance, 100, now));
        assert!(!allowance_covers(&allowance, 101, now));
        allowance.expires_at = Some((now + dmsg_types::MINUTE) * 1_000_000);
        assert!(!allowance_covers(&allowance, 100, now));
        allowance.expires_at = allowance.expires_at.map(|t| t + 1);
        assert!(allowance_covers(&allowance, 100, now));
    }

    #[test]
    fn price_shows_certified_casing_and_only_echoes_available_requests() {
        let record = |display_handle: Option<&str>| HandleRecord {
            handle: "alice".into(),
            display_handle: display_handle.map(str::to_string),
            owner_account: AccountId([1; 12]),
            version: 1,
            event_tip: Hash::new([2; 32]),
        };
        let registered = record(Some("Alice"));
        assert_eq!(
            price_display(Some(&registered), false, "ALICE", "alice"),
            "Alice"
        );
        let legacy = record(None);
        assert_eq!(
            price_display(Some(&legacy), false, "ALICE", "alice"),
            "alice"
        );
        assert_eq!(
            price_display(None, true, "LegacyName", "legacyname"),
            "legacyname"
        );
        assert_eq!(price_display(None, false, "NewName", "newname"), "NewName");
    }

    #[tokio::test]
    async fn bound_holder_signs_claims_only_while_its_delegation_is_live() {
        let dir = TempDir::new();
        let mut client = unlocked(&dir.0, false, None);
        let now = now_ms();
        let expires = now + 60_000;
        let delegated = crate::login::kept_for_tests(42, expires)
            .delegated()
            .unwrap();
        let holder = delegated.principal;
        let ic = Ic::connect(&client.config, delegated.identity)
            .await
            .unwrap();
        let mut reservation = LegacyReservation {
            handle: "oldpanda".into(),
            legacy_owner: holder,
            legacy_name_principal: None,
            frozen_admins: vec![],
            quarantined: false,
        };
        assert_eq!(client.claim_login(&reservation, now), None);
        client.bound_logins.insert(holder, (ic.clone(), expires));
        assert_eq!(client.claim_login(&reservation, now), Some(holder));
        assert_eq!(
            client.claim_ic(holder, now).unwrap().principal().unwrap(),
            holder
        );
        assert_eq!(client.claim_login(&reservation, expires), None);
        assert!(matches!(
            client.claim_ic(holder, expires),
            Err(Error::AuthRequired)
        ));

        // An ordinary bound login cannot act as another frozen owner or
        // a name principal; only a listed frozen admin can claim the latter.
        reservation.legacy_owner = Principal::from_slice(&[8; 29]);
        assert_eq!(client.claim_login(&reservation, now), None);
        reservation.legacy_name_principal = Some(reservation.legacy_owner);
        assert_eq!(client.claim_login(&reservation, now), None);
        reservation.frozen_admins.push(holder);
        assert_eq!(client.claim_login(&reservation, now), Some(holder));
        assert!(matches!(
            client.claim_ic(reservation.legacy_owner, now),
            Err(Error::AuthRequired)
        ));

        client.lock();
        assert!(client.bound_logins.is_empty());
        assert_eq!(client.claim_login(&reservation, now), None);
        assert!(matches!(
            client.claim_ic(holder, now),
            Err(Error::AuthRequired)
        ));

        client.bound_logins.insert(holder, (ic, expires));
        client.select(crate::client::Slot::Login(holder)).unwrap();
        assert!(client.bound_logins.is_empty());
    }

    /// A page with no names asks the registry for nothing: it refuses an
    /// empty batch with `QuotaExceeded`.
    #[tokio::test]
    async fn an_empty_page_asks_for_no_certified_records() {
        let ic = Ic::anonymous(&crate::testing::config(false)).await.unwrap();
        let records = certified_records(&ic, Principal::anonymous(), &[])
            .await
            .unwrap();
        assert!(records.is_empty());
    }
}
