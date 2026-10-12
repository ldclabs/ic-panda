//! Recovery after every device is lost (`docs/protocol/account_root_zh.md`,
//! "全设备丢失恢复"). A new device, logged in with a login bound to the
//! account, asks for a delayed takeover; during the account's delay any
//! active device can dispute it (`crate::security`). Afterwards the new
//! device completes the takeover, which replaces every device and login,
//! derives the committed root's vetKD key once to open the bundle's
//! recovery envelope, and rotates to a root only it holds.
//!
//! The request is sealed in the workspace before it is sent and resent
//! unchanged after a lost reply. The derivation's approval and transport
//! key are journaled the same way, and a lost result is read back by its
//! execution ID; a new approval is signed only once the old one provably
//! can no longer start.

use crate::client::{Client, Status};
use crate::codec::{b64, unb64};
use crate::crypto::sign;
use crate::now_ms;
use crate::root::{open_recovered, parse_bundle, recovery_identity};
use candid::Principal;
use dmsg_protocol::{
    approval_message, derive_approval_command, execution_request_id, recovery_device_message,
    DERIVE_APPROVAL_DOMAIN,
};
use dmsg_types::{
    cose::{DeriveRootRequest, ExecutionOutcome, ExecutionResult},
    ensure,
    user::{ControllerRole, PendingRecovery, RecoveryRequest},
    AccountId, Approval, Ed25519Signature, Error, Hash, Result, DAY, MINUTE,
};
use ic_vetkeys::{DerivedPublicKey, EncryptedVetKey, TransportSecretKey};
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};
use serde_json::{json, Value as Json};

/// The recovery request of this device, sealed in its workspace.
const REQUEST: &str = "recovery-request";
/// The derivation journal of one committed root.
const DERIVE: &str = "recover-root:";
/// The cycles one derivation may cost at most; never raised automatically.
pub const DERIVE_MAX_CYCLES: u128 = 70_000_000_000;

#[derive(Serialize, Deserialize)]
struct DeriveJob {
    /// Seed of the vetKD transport key.
    seed: ByteArray<32>,
    /// The signed request, once approved.
    request: Option<DeriveRootRequest>,
}

impl Client {
    /// The account recovery is about: the bound one, or the login's
    /// account this new device waits to join.
    fn recovery_target(&self) -> Result<(Principal, AccountId)> {
        match &self.workspace.meta.account {
            Some(account) => Ok((account.home, account.id)),
            None => self.located.ok_or(Error::NotFound),
        }
    }

    /// The pending recovery, checked against the certified leaf.
    async fn pending_recovery(&mut self) -> Result<Option<PendingRecovery>> {
        let (home, account) = self.recovery_target()?;
        let ic = self.session()?.ic.clone();
        let pending: Result<Option<PendingRecovery>> =
            ic.query(home, "get_recovery_request", (account,)).await?;
        let pending = pending?;
        let security = self.refresh_security(home, &account).await?;
        ensure(
            security.snapshot.pending_recovery_digest
                == pending
                    .as_ref()
                    .map(|p| dmsg_protocol::digest("dmsg/pending-recovery/v1", p)),
            Error::PolicyStale,
        )?;
        Ok(pending)
    }

    /// Where recovery of the account stands: the pending request, if any,
    /// whether it belongs to this device and when it can complete.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` without a login and `Error::NotFound`
    /// when the login has no account.
    pub async fn recovery_status(&mut self) -> Result<Json> {
        let pending = self.pending_recovery().await?;
        let (_, account) = self.recovery_target()?;
        let security = self.security.clone().ok_or(Error::IntegrityFailed)?;
        let device = self.workspace.meta.device_id;
        Ok(json!({
            "account": account.to_string(),
            "recovery_delay_ms": security.snapshot.recovery_delay_ms,
            "member": security.active(&device).is_ok(),
            "pending": pending.as_ref().map(|p| crate::security::recovery_view(p, &device)),
            "ready": pending.as_ref().is_some_and(|p| {
                p.request.device.device_id == device && now_ms() >= p.execute_after
            }),
        }))
    }

    /// Ask to take the login's account over with this new device after the
    /// account's recovery delay. The login is the authority; the device
    /// only proves possession of its keys.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` on a device that already belongs to an
    /// account and `Error::Pending` while another recovery is pending.
    pub async fn recovery_request(&mut self) -> Result<Json> {
        ensure(self.workspace.provisional(), Error::Forbidden)?;
        let (home, account) = self.located.ok_or(Error::NotFound)?;
        let caller = self.session()?.principal;
        let device = self.workspace.meta.device_id;
        let security = self.refresh_security(home, &account).await?;
        ensure(
            security.device(&device).is_none(),
            Error::IdempotencyConflict,
        )?;
        let delay = security.snapshot.recovery_delay_ms;
        let now = now_ms();
        let saved: Option<RecoveryRequest> = self.workspace.private(self.keys()?, REQUEST)?;
        // A lost reply is retried with the same request, which never starts
        // a new delay; a request too close to its deadline is replaced.
        let request = match saved {
            Some(r) if r.new_auth == caller && r.expires_at > now + delay + DAY => r,
            _ => RecoveryRequest {
                op_id: Hash::new(crate::crypto::random_id()),
                new_auth: caller,
                device: self.workspace.device_input(ControllerRole::Administrator),
                // At most 14 days past the home's clock; leave a skew margin.
                expires_at: now + delay + 7 * DAY - 10 * MINUTE,
            },
        };
        self.workspace
            .put_private(self.keys()?, REQUEST, &request)?;
        let proof = sign(
            &self.keys()?.signing,
            recovery_device_message(home, &account, &request).as_slice(),
        );
        let ic = self.session()?.ic.clone();
        let requested: Result<()> = ic
            .update(
                home,
                "request_recovery",
                (account, &request, ByteBuf::from(proof.to_vec())),
            )
            .await?;
        requested?;
        self.recovery_status().await
    }

    /// Complete this device's recovery once the delay has passed: take the
    /// account over, derive and open the committed root, and rotate to a
    /// new root only this device holds. Run again, it resumes where an
    /// interruption left it.
    ///
    /// # Errors
    /// Returns `Error::Pending` before the delay ends and `Error::NotFound`
    /// without this device's recovery.
    pub async fn recovery_complete(&mut self) -> Result<Status> {
        if self.workspace.provisional() {
            let (home, account) = self.located.ok_or(Error::NotFound)?;
            let device = self.workspace.meta.device_id;
            let pending = self.pending_recovery().await?;
            match pending {
                Some(p) if p.request.device.device_id == device => {
                    ensure(
                        p.request.new_auth == self.session()?.principal,
                        Error::AuthRequired,
                    )?;
                    ensure(now_ms() >= p.execute_after, Error::Pending)?;
                    let ic = self.session()?.ic.clone();
                    let completed: Result<()> = ic
                        .update(home, "complete_recovery", (account, p.request.op_id))
                        .await?;
                    completed?;
                }
                // Completed before a lost reply: the device is already in.
                _ => {
                    let security = self.refresh_security(home, &account).await?;
                    security.active(&device).map_err(|_| Error::NotFound)?;
                }
            }
            self.join(home, account).await?;
            self.workspace.delete_private(REQUEST)?;
        }
        if self.root.is_none() {
            self.recover_root().await?;
        }
        let account = self.account()?.clone();
        let state = self.refresh(account.home, &account.id).await?;
        // The recovered root is known to the replaced devices.
        if state.info.recovered_device.is_some() {
            self.rotate_root().await?;
        }
        // The outcome is recorded in the status; it never fails recovery.
        let _ = self.sync().await;
        self.status()
    }

    /// Derive the committed root's vetKD key for this recovered device and
    /// open the bundle's recovery envelope.
    async fn recover_root(&mut self) -> Result<()> {
        let account = self.account()?.clone();
        let device = self.workspace.meta.device_id;
        let state = self.refresh(account.home, &account.id).await?;
        let current = state
            .info
            .current_root
            .clone()
            .ok_or(Error::RecoveryIncomplete)?;
        let generation = current.generation;
        ensure(
            state.info.recovered_device == Some((device, generation)),
            Error::DeviceNotApproved,
        )?;
        let (bundles, upload_id) = self.root_bundles(generation).await?;
        let bundle = parse_bundle(&bundles[0])?;
        let recovery = self
            .recovery_key(&account.id, generation, state.info.home_cose)
            .await?;
        ensure(recovery == bundle.body.recovery_key, Error::IntegrityFailed)?;
        let key = format!("{DERIVE}{}", hex::encode(*current.bundle_digest));
        let mut job = match self.workspace.private::<DeriveJob>(self.keys()?, &key)? {
            Some(job) => job,
            None => DeriveJob {
                seed: ByteArray::new(crate::crypto::random_key()),
                request: None,
            },
        };
        self.workspace.put_private(self.keys()?, &key, &job)?;
        let transport =
            TransportSecretKey::from_seed(job.seed.to_vec()).map_err(|_| Error::IntegrityFailed)?;
        if job.request.is_none() {
            job.request = Some(self.derive_request(generation, &transport).await?);
            self.workspace.put_private(self.keys()?, &key, &job)?;
        }
        let result = self.derivation(&mut job, &transport, &key).await?;
        let ExecutionOutcome::Completed(output) = result.outcome else {
            return Err(match result.outcome {
                ExecutionOutcome::Failed(error) => error,
                ExecutionOutcome::ResultExpired => Error::ResultExpired,
                _ => Error::ExecutionUnknown,
            });
        };
        ensure(
            output.key.home_cose.to_text() == recovery.home_cose
                && output.key.master_key_name == recovery.key_name
                && b64(&output.key.public_key) == recovery.public_key
                && output.key.key_generation == generation
                && output.key.account_id == account.id,
            Error::IntegrityFailed,
        )?;
        let public = DerivedPublicKey::deserialize(&unb64(&recovery.public_key)?)
            .map_err(|_| Error::IntegrityFailed)?;
        let vetkey = EncryptedVetKey::deserialize(&output.encrypted_key)
            .and_then(|k| {
                k.decrypt_and_verify(
                    &transport,
                    &public,
                    &recovery_identity(&account.id, generation),
                )
            })
            .map_err(|_| Error::IntegrityFailed)?;
        let environment = crate::config::environment_name(&self.config.environment);
        let (root, history) = open_recovered(
            &bundles,
            &current.bundle_digest,
            (&account.id.to_string(), environment, generation),
            &vetkey,
        )?;
        self.keep_root(
            generation,
            current.bundle_digest,
            &bundles[0],
            upload_id,
            root,
            history,
        )?;
        self.workspace.delete_private(&key)
    }

    /// A derivation request for `generation`, approved by this device.
    async fn derive_request(
        &mut self,
        generation: u64,
        transport: &TransportSecretKey,
    ) -> Result<DeriveRootRequest> {
        let account = self.account()?.clone();
        let device_id = self.workspace.meta.device_id;
        let state = self.refresh(account.home, &account.id).await?;
        let device = state.active(&device_id)?;
        let mut request = DeriveRootRequest {
            account_id: account.id,
            generation,
            transport_public_key: ByteArray::new(
                transport
                    .public_key()
                    .try_into()
                    .map_err(|_| Error::IntegrityFailed)?,
            ),
            max_cycles: DERIVE_MAX_CYCLES,
            approval: Approval {
                device_id,
                security_epoch: state.info.security_epoch,
                sequence: device.next_sequence,
                request_id: execution_request_id(
                    &account.id,
                    state.info.security_epoch,
                    device_id,
                    device.next_sequence,
                ),
                expires_at: now_ms() + crate::account::APPROVAL_MS,
                signature: Ed25519Signature::new([0; 64]),
            },
        };
        let message = approval_message(
            account.home,
            &account.id,
            DERIVE_APPROVAL_DOMAIN,
            &derive_approval_command(&request),
            &request.approval,
        );
        request.approval.signature =
            Ed25519Signature::new(sign(&self.keys()?.signing, message.as_slice()));
        Ok(request)
    }

    /// The derivation's result: read back by its execution ID, submitted
    /// once, or approved again only when the old approval can never start.
    async fn derivation(
        &mut self,
        job: &mut DeriveJob,
        transport: &TransportSecretKey,
        key: &str,
    ) -> Result<ExecutionResult> {
        let account = self.account()?.clone();
        let ic = self.session()?.ic.clone();
        let mut request = job.request.clone().ok_or(Error::IntegrityFailed)?;
        let found: Result<ExecutionResult> = ic
            .query(
                account.home,
                "get_execution",
                (account.id, request.approval.request_id),
            )
            .await?;
        let result = match found {
            Ok(result) => result,
            Err(Error::ResultExpired) => {
                if request.approval.expires_at <= now_ms() {
                    let state = self.refresh(account.home, &account.id).await?;
                    let device = state.active(&request.approval.device_id)?;
                    // A certified time past the deadline with the sequence
                    // unused proves the old approval never started.
                    ensure(
                        state.security.certified_at >= request.approval.expires_at
                            && device.next_sequence == request.approval.sequence,
                        Error::ResultExpired,
                    )?;
                    request = self.derive_request(request.generation, transport).await?;
                    job.request = Some(request.clone());
                    self.workspace.put_private(self.keys()?, key, &*job)?;
                }
                let derived: Result<ExecutionResult> =
                    ic.update(account.home, "derive_root", (&request,)).await?;
                derived?
            }
            Err(error) => return Err(error),
        };
        if result.is_terminal() {
            return Ok(result);
        }
        let reconciled: Result<ExecutionResult> = ic
            .update(
                account.home,
                "reconcile_execution",
                (account.id, request.approval.request_id),
            )
            .await?;
        reconciled
    }
}
