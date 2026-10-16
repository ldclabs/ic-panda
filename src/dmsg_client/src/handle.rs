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
//! about a minute: the owner prepares a token naming the target, the target
//! account accepts it, and the owner then authorizes its side and moves the
//! name within that minute.

use crate::client::Client;
use crate::now_ms;
use crate::payer::{approve_call, memo};
use candid::Principal;
use dmsg_protocol::{
    charge_terms_digest, digest, handle_bucket, normalize_handle, price, HANDLE_BUCKET_BITS,
};
use dmsg_types::{
    ensure,
    handle::{
        HandleAction, HandleInit, HandleIntent, HandleOperation, HandlePhase, HandleRecord,
        LegacyReservation, Registration, SnapshotProgress,
    },
    user::AccountCommand,
    AccountId, Error, Hash, Result,
};
use icrc_ledger_types::{icrc1::account::Account, icrc2::approve::ApproveArgs};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};

const JOURNAL: &str = "handle:";
/// Format of a transfer token.
pub const TRANSFER: &str = "dmsg-handle-transfer/1";

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
    /// The name is this account's.
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
}

impl HandleJob {
    /// The commitment a person approves.
    pub fn digest(&self) -> Hash {
        digest(
            "dmsg/handle-approval/v1",
            &(&self.intent, &self.kind, self.caller),
        )
    }

    /// The operation for display.
    pub fn view(&self) -> Json {
        json!({
            "id": self.id,
            "name": self.name,
            "action": format!("{:?}", self.intent.action),
            "caller": self.caller.to_text(),
            "total": match &self.kind {
                Kind::Register { total, .. } => Some(total.to_string()),
                _ => None,
            },
            "from": match &self.kind {
                Kind::Transfer { from, .. } => Some(from.account_id.to_string()),
                _ => None,
            },
            "to": match &self.kind {
                Kind::Transfer { accept, .. } => Some(accept.account_id.to_string()),
                _ => None,
            },
            "phase": self.phase,
            "digest": hex::encode(*self.digest()),
        })
    }
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
        let name = normalize_handle(name)?;
        let ic = self.reader().await?;
        let batch: Result<dmsg_types::CertifiedBatch> = ic
            .query(
                self.config.handle,
                "resolve_handle_certified",
                (vec![&name],),
            )
            .await?;
        let batch = batch?;
        let bits = bucket_path(&name);
        let mut path: Vec<&[u8]> = bits.iter().map(|b| b.as_slice()).collect();
        path.push(name.as_bytes());
        let value = crate::certified::certified_path(
            &batch,
            name.as_bytes(),
            &path,
            self.config.handle,
            &ic.root_key(),
            now_ms(),
        )?;
        let Some(value) = value else {
            return Ok(None);
        };
        let record: HandleRecord = crate::codec::decode(&value, 4096)?;
        ensure(
            record.handle == name && record.version > 0,
            Error::IntegrityFailed,
        )?;
        Ok(Some(record))
    }

    /// A name's price and whether it can be registered.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for an invalid name.
    pub async fn handle_price(&mut self, name: &str) -> Result<Json> {
        let name = normalize_handle(name)?;
        let ic = self.reader().await?;
        let reserved: Result<Option<LegacyReservation>> = ic
            .query(self.config.handle, "get_legacy_reservation", (&name,))
            .await?;
        let owner = self.handle_resolve(&name).await?;
        Ok(json!({
            "name": name,
            "price": price(&name).to_string(),
            "owner": owner.map(|r| r.owner_account.to_string()),
            "reserved": reserved?.is_some(),
        }))
    }

    /// Prepare claiming a name the login's Principal owned in the legacy
    /// registry; the claim runs once a person approves it.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` when the login is not the frozen owner and
    /// `Error::Pending` before the snapshot is sealed.
    pub async fn handle_claim_prepare(&mut self, name: &str) -> Result<HandleJob> {
        let name = normalize_handle(name)?;
        let account = self.account()?.id;
        let caller = self.session()?.principal;
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
        let owner_is_name = reservation.legacy_name_principal == Some(reservation.legacy_owner);
        ensure(
            if owner_is_name {
                caller != reservation.legacy_owner && reservation.frozen_admins.contains(&caller)
            } else {
                caller == reservation.legacy_owner
            },
            Error::Forbidden,
        )?;
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
        let config: HandleInit = ic
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
            terms_digest: charge_terms_digest(
                config.ledger,
                &account,
                total - config.ledger_fee,
                config.ledger_fee,
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
                }),
                ledger: config.ledger,
                total,
            },
            phase: Phase::Prepared,
        };
        self.save_handle_job(&job)?;
        Ok(job)
    }

    /// Prepare transferring a name this account owns to `target`: a token
    /// for the target account to accept. Once it has, the owner approves
    /// the transfer within a minute.
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
        let token = crate::codec::b64(&crate::codec::canonical(&TransferPacket {
            format: TRANSFER.into(),
            from: from.clone(),
            accept: accept.clone(),
        }));
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

    /// Every name operation of this account, for display and settling.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn handle_operations(&self) -> Result<Vec<Json>> {
        self.workspace
            .store
            .sealed_keys(JOURNAL)?
            .iter()
            .map(|key| {
                self.workspace
                    .private::<HandleJob>(self.keys()?, key)?
                    .map(|job| job.view())
                    .ok_or(Error::IntegrityFailed)
            })
            .collect()
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
        job.phase = Phase::Unknown;
        self.save_handle_job(&job)?;
        match job.kind.clone() {
            Kind::Transfer { .. } => Err(Error::IntegrityFailed),
            Kind::Claim { snapshot } => {
                let ic = self.session()?.ic.clone();
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
                        return Err(Error::ExecutionUnknown)
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
                let (_, ic) = self.payer().await?;
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
        if commit && record.as_ref().map(|r| r.owner_account) == Some(accept.account_id) {
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
            // Accepted: the owner has about a minute to move the name.
            job.phase = Phase::Done;
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
        let now_ns = now_ms() * 1_000_000;
        let allowance = u128::try_from(current.allowance.0.clone()).unwrap_or(0);
        if allowance >= total
            && current
                .expires_at
                .is_none_or(|t| t > now_ns + 60_000_000_000)
        {
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
