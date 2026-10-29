//! A planned device replacement. The successor, a new device, asks with a
//! token naming the device it replaces, the predecessor, which approves
//! `AddSuccessor` and hands over: the successor's approval key, its cosigner
//! keys in every wallet and its local content. The successor then takes over
//! the wallet bindings and revokes the predecessor. Each device keeps a task
//! of the steps it saw done, sealed under its LDK, so either stops and
//! continues; cancelling keeps what was already committed.

use crate::account::{APPROVAL_MS, PairingRequest, read_account};
use crate::approval::{Action, HIGH_RISK_TTL_MS};
use crate::client::Client;
use crate::codec::{b64, canonical, decode, unb64};
use crate::crypto::{random_id, sign};
use crate::hold::Hold;
use crate::now_ms;
use crate::wallet::rotate::{self, Successor as SuccessorKey};
use crate::workspace::AccountRef;
use candid::Principal;
use dmsg_protocol::{
    approval_key_id, approval_key_message, successor_device_message, verify, verify_p256,
};
use dmsg_types::user::{
    AccountCommand, AccountInfo, AccountMutation, ApprovalKeyInput, Capability, ControllerRole,
    HardwarePurpose,
};
use dmsg_types::wallet::MemberId;
use dmsg_types::{AccountId, Approval, Ed25519Signature, Error, Hash, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;

const FORMAT: &str = "dmsg-succession/1";
/// Sealed tasks, by the succession's request ID.
const TASK: &str = "succession:";
/// The largest token; a pairing alone stays under 4 KiB.
const MAX_TOKEN: usize = 8192;
/// Tasks a device lists.
const MAX_TASKS: usize = 16;

/// What the successor hands the predecessor.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Token {
    format: String,
    home: Principal,
    /// The signed `AddSuccessor` the predecessor approves.
    pairing: AccountMutation,
    /// The successor's new approval key, for the predecessor to register.
    approval_key: Option<ApprovalKeyInput>,
    /// The successor's new cosigner key, for the predecessor's keys to
    /// rotate to.
    cosigner: Option<SuccessorKey>,
}

/// A step of a succession, in order.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    /// The predecessor approves `AddSuccessor`, then rotates the root.
    Authorize,
    /// The successor's approval key is registered.
    ApprovalKey,
    /// The predecessor's cosigner keys rotate to the successor's.
    Cosigners,
    /// Local content goes to the successor, or the person leaves it.
    Content,
    /// The successor takes over the predecessor's wallet bindings.
    Wallets,
    /// Native addresses move to the successor's keys, or the person leaves
    /// them.
    NativeAddresses,
    /// The successor revokes the predecessor.
    Revoke,
}

impl Step {
    const ALL: [Step; 7] = [
        Step::Authorize,
        Step::ApprovalKey,
        Step::Cosigners,
        Step::Content,
        Step::Wallets,
        Step::NativeAddresses,
        Step::Revoke,
    ];

    /// The steps settled before this one runs. Rotation needs the
    /// predecessor's wallet identity, which taking over the bindings
    /// removes; revoking waits until nothing is left behind unnoticed.
    fn after(self) -> &'static [Step] {
        match self {
            Step::Authorize => &[],
            Step::ApprovalKey | Step::Cosigners | Step::Content => &[Step::Authorize],
            Step::Wallets => &[Step::Cosigners],
            Step::NativeAddresses => &[Step::Wallets],
            Step::Revoke => &[
                Step::ApprovalKey,
                Step::Content,
                Step::Wallets,
                Step::NativeAddresses,
            ],
        }
    }

    /// Whether a device on `side` runs it. The successor runs a cosigner
    /// step by confirming that the predecessor finished it.
    fn runs_on(self, side: Side) -> bool {
        match self {
            Step::Authorize => side == Side::Predecessor,
            Step::ApprovalKey | Step::Cosigners | Step::Content => true,
            Step::Wallets | Step::NativeAddresses | Step::Revoke => side == Side::Successor,
        }
    }
}

/// Which device of the succession keeps the task.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The device being replaced.
    Predecessor,
    /// The new device.
    Successor,
}

/// How a step was settled.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Outcome {
    /// When it was recorded.
    pub at_ms: u64,
    /// What shows it done, such as an account version or wallet rotations.
    pub evidence: Json,
    /// What the person chose to leave behind, for a skipped step.
    pub skipped: Vec<String>,
}

/// One device's record of a succession.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Task {
    /// The `AddSuccessor` request ID.
    pub id: Hash,
    /// This device's side.
    pub side: Side,
    /// The account.
    pub account: AccountId,
    /// The device being replaced.
    pub predecessor: Hash,
    /// The new device.
    pub successor: Hash,
    /// The token, which the predecessor's later steps use.
    pub token: String,
    /// Settled steps.
    pub steps: BTreeMap<Step, Outcome>,
    /// When the person stopped it; settled steps stay.
    pub cancelled_at_ms: Option<u64>,
    /// When this device started it.
    pub created_at_ms: u64,
}

impl Task {
    fn new(side: Side, token: &Token, text: String, predecessor: Hash, now: u64) -> Result<Self> {
        let AccountCommand::AddSuccessor { device, .. } = &token.pairing.command else {
            return Err(Error::IntegrityFailed);
        };
        Ok(Self {
            id: token.pairing.approval.request_id,
            side,
            account: token.pairing.account_id,
            predecessor,
            successor: device.device_id,
            token: text,
            steps: BTreeMap::new(),
            cancelled_at_ms: None,
            created_at_ms: now,
        })
    }

    /// The steps this device runs now: not settled, after the steps they
    /// wait for, and none once cancelled.
    pub fn available(&self) -> Vec<Step> {
        if self.cancelled_at_ms.is_some() {
            return vec![];
        }
        Step::ALL
            .into_iter()
            .filter(|s| {
                !self.steps.contains_key(s)
                    && s.runs_on(self.side)
                    && s.after().iter().all(|p| self.steps.contains_key(p))
            })
            .collect()
    }

    /// Record `step` done, once.
    fn record(&mut self, step: Step, evidence: Json, now: u64) -> bool {
        if self.steps.contains_key(&step) {
            return false;
        }
        self.steps.insert(
            step,
            Outcome {
                at_ms: now,
                evidence,
                skipped: vec![],
            },
        );
        true
    }

    /// Leave local content or native addresses behind, listing what stays.
    fn skip(&mut self, step: Step, skipped: Vec<String>, now: u64) -> Result<()> {
        ensure(
            matches!(step, Step::Content | Step::NativeAddresses) && skipped.len() <= 64,
            Error::InvalidInput("only content and native addresses can be left".into()),
        )?;
        ensure(self.available().contains(&step), Error::Forbidden)?;
        self.steps.insert(
            step,
            Outcome {
                at_ms: now,
                evidence: Json::Null,
                skipped,
            },
        );
        Ok(())
    }

    /// Record what the account shows done: the successor's enrollment, its
    /// approval key and the predecessor's revocation. A revoked successor
    /// cancels the task. Returns whether anything changed.
    fn observe(&mut self, info: &AccountInfo, now: u64) -> bool {
        let mut changed = false;
        if let Some(d) = info.devices.get(&self.successor)
            && d.added_by == Some(self.predecessor)
        {
            changed |= self.record(
                Step::Authorize,
                json!({"account_version": info.account_version, "added_at": d.added_at}),
                now,
            );
            if d.revoked_at.is_some() && self.cancelled_at_ms.is_none() {
                self.cancelled_at_ms = Some(now);
                changed = true;
            }
        }
        if let Some(key) = info.approval_keys.get(&self.successor) {
            changed |= self.record(
                Step::ApprovalKey,
                json!({"key_id": hex::encode(*key.key_id)}),
                now,
            );
        }
        if let Some(at) = info
            .devices
            .get(&self.predecessor)
            .and_then(|d| d.revoked_at)
        {
            changed |= self.record(
                Step::Revoke,
                json!({
                    "revoked_at": at,
                    "root_generation": info.current_root.as_ref().map(|r| r.generation),
                }),
                now,
            );
        }
        changed
    }

    fn view(&self) -> Json {
        json!({
            "id": hex::encode(*self.id),
            "side": self.side,
            "account": self.account.to_string(),
            "predecessor": hex::encode(*self.predecessor),
            "successor": hex::encode(*self.successor),
            "steps": Step::ALL.iter().map(|s| {
                let outcome = self.steps.get(s);
                json!({
                    "step": s,
                    "settled": outcome.is_some(),
                    "at": outcome.map(|o| o.at_ms),
                    "evidence": outcome.map(|o| o.evidence.clone()),
                    "skipped": outcome.map(|o| o.skipped.clone()).unwrap_or_default(),
                })
            }).collect::<Vec<_>>(),
            "available": self.available(),
            "cancelled_at": self.cancelled_at_ms,
            "created_at": self.created_at_ms,
        })
    }
}

fn key(id: &Hash) -> String {
    format!("{TASK}{}", hex::encode(**id))
}

impl Client {
    fn succession(&self, id: &Hash) -> Result<Task> {
        self.workspace
            .private(self.keys()?, &key(id))?
            .ok_or(Error::NotFound)
    }

    fn save_succession(&self, task: &Task) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &key(&task.id), task)
    }

    /// Check a succession token for this device: of its account, unexpired,
    /// enrolling a successor of this very device, with valid proofs of
    /// possession for its keys.
    pub(crate) fn inspect_succession(&self, token: &str) -> Result<(PairingRequest, Token)> {
        ensure(token.len() <= MAX_TOKEN, Error::QuotaExceeded)?;
        let token: Token = decode(&unb64(token)?, MAX_TOKEN)?;
        let account = self.account()?;
        let request = &token.pairing;
        ensure(
            token.format == FORMAT
                && token.home == account.home
                && request.account_id == account.id,
            Error::IntegrityFailed,
        )?;
        let now = now_ms();
        ensure(
            request.approval.expires_at > now && request.approval.expires_at <= now + 300_000,
            Error::Expired,
        )?;
        let AccountCommand::AddSuccessor { device, proof } = &request.command else {
            return Err(Error::IntegrityFailed);
        };
        let message = successor_device_message(
            token.home,
            &request.account_id,
            device,
            request.expected_version,
            request.approval.request_id,
            self.workspace.meta.device_id,
        );
        verify(&device.signing_pub, message.as_slice(), proof.as_slice())?;
        if let Some(key) = &token.approval_key {
            approval_key_id(&key.public_key)?;
            let message = approval_key_message(
                token.home,
                &request.account_id,
                device.device_id,
                &key.public_key,
            );
            verify_p256(&key.public_key, message.as_slice(), &key.proof)?;
        }
        if let Some(cosigner) = &token.cosigner {
            cosigner.check(&MemberId::Account(request.account_id))?;
        }
        let pairing = PairingRequest {
            account: request.account_id,
            device: device.clone(),
            mutation: request.clone(),
        };
        Ok((pairing, token))
    }
}

/// Ask to succeed `predecessor`, an administrator device of the account
/// this login found: a token for that device to approve. It carries this
/// device's enrollment with the predecessor's role and capabilities, and a
/// new approval key and cosigner key where this device's hardware keeps
/// them; the person is asked once for each. The token is valid as long as a
/// pairing's.
///
/// # Errors
/// Returns `Error::Forbidden` on a device that already joined an account or
/// for a predecessor that cannot approve enrollments, and `Error::NotFound`
/// when the login has no account.
pub async fn request<H: Hold>(h: &H, predecessor: Hash) -> Result<Json> {
    let (ic, located, device_id) = h
        .with(|c| {
            ensure(c.workspace.provisional(), Error::Forbidden)?;
            c.keys()?;
            Ok((
                c.session()?.ic.clone(),
                c.located,
                c.workspace.meta.device_id,
            ))
        })
        .await?;
    let (home, account) = located.ok_or(Error::NotFound)?;
    let state = read_account(&ic, home, &account).await?;
    ensure(
        state.device(&device_id).is_none(),
        Error::IdempotencyConflict,
    )?;
    let old = state.active(&predecessor)?.input.clone();
    ensure(
        old.role == ControllerRole::Administrator
            && old.capabilities.contains(&Capability::RootManage),
        Error::Forbidden,
    )?;
    let request_id = Hash::new(random_id());
    let pairing = h
        .with(|c| {
            let mut device = c.workspace.device_input(old.role.clone());
            device.capabilities = old.capabilities.clone();
            let message = successor_device_message(
                home,
                &account,
                &device,
                state.info.account_version,
                request_id,
                predecessor,
            );
            let proof = Ed25519Signature::new(sign(&c.keys()?.signing, message.as_slice()));
            Ok(AccountMutation {
                account_id: account,
                expected_version: state.info.account_version,
                command: AccountCommand::AddSuccessor { device, proof },
                approval: Approval {
                    device_id,
                    security_epoch: state.info.security_epoch,
                    sequence: 0,
                    request_id,
                    expires_at: now_ms() + APPROVAL_MS,
                    signature: Ed25519Signature::new([0; 64]),
                    hardware: None,
                },
            })
        })
        .await?;
    let reference = AccountRef {
        id: account,
        home,
        issuer: state.info.issuer.clone(),
    };
    let approval_key = crate::approval_key::successor_key(h, reference).await?;
    let label = format!("succession-{}", &hex::encode(*request_id)[..8]);
    let cosigner = match h.with(|c| c.wallet_cosigner_create(&label, true)).await {
        Ok(_) => Some(rotate::successor(h, MemberId::Account(account), &label).await?),
        Err(Error::Unavailable(_)) => None,
        Err(error) => return Err(error),
    };
    let token = Token {
        format: FORMAT.into(),
        home,
        pairing,
        approval_key,
        cosigner,
    };
    let text = b64(&canonical(&token));
    ensure(text.len() <= MAX_TOKEN, Error::QuotaExceeded)?;
    let task = Task::new(Side::Successor, &token, text.clone(), predecessor, now_ms())?;
    h.with(|c| c.save_succession(&task)).await?;
    Ok(json!({
        "id": hex::encode(*request_id),
        "token": text,
        "device_id": hex::encode(*device_id),
        "approval_key": token.approval_key.is_some(),
        "cosigner": token.cosigner.is_some(),
    }))
}

/// What a succession token asks of this device, the predecessor: the
/// successor's enrollment and keys, and the wallets the handover covers.
///
/// # Errors
/// As [`accept`], without creating anything.
pub async fn review<H: Hold>(h: &H, token: &str) -> Result<Json> {
    let (pairing, token) = h.with(|c| c.inspect_succession(token)).await?;
    let wallets = wallets(h).await?;
    Ok(json!({
        "id": hex::encode(*pairing.mutation.approval.request_id),
        "device_id": hex::encode(*pairing.device.device_id),
        "role": format!("{:?}", pairing.device.role),
        "capabilities": pairing.device.capabilities.iter().map(|c| format!("{c:?}")).collect::<Vec<_>>(),
        "approval_key": token.approval_key.is_some(),
        "cosigner": token.cosigner.is_some(),
        "expires_at": pairing.mutation.approval.expires_at,
        "wallets": wallets,
    }))
}

/// Ask to approve the succession of `token`; `device` is the ID the
/// successor shows, which the person names. Approving enrolls the successor
/// and rotates the content root to it; this device then approves only the
/// handover. Starts this device's task.
///
/// # Errors
/// Returns `Error::Forbidden` on a device that cannot approve account
/// changes, `Error::InvalidInput` when `device` is not the token's, and
/// refuses a token of another account, for another device, an expired one or
/// a bad proof of possession.
pub async fn accept<H: Hold>(h: &H, source: &str, token: &str, device: &str) -> Result<Json> {
    h.with(|c| c.keys().map(|_| ())).await?;
    let security = crate::account::refreshed_security(h).await?;
    h.with(|c| {
        let own = security.active(&c.workspace.meta.device_id)?;
        ensure(
            own.input.role == ControllerRole::Administrator
                && own.input.capabilities.contains(&Capability::RootManage),
            Error::Forbidden,
        )?;
        let (pairing, parsed) = c.inspect_succession(token)?;
        ensure(
            hex::encode(*pairing.device.device_id) == device,
            Error::InvalidInput("device does not match the request".into()),
        )?;
        let id = pairing.mutation.approval.request_id;
        if matches!(c.succession(&id), Err(Error::NotFound)) {
            let own = c.workspace.meta.device_id;
            let task = Task::new(Side::Predecessor, &parsed, token.into(), own, now_ms())?;
            c.save_succession(&task)?;
        }
        let expires_at = pairing
            .mutation
            .approval
            .expires_at
            .min(now_ms() + HIGH_RISK_TTL_MS);
        let request = c.approval_create(
            source,
            Action::DevicePair {
                token: token.into(),
                device: pairing.device,
                succession: true,
            },
            expires_at,
        )?;
        let mut view = c.approval_view(&request)?;
        view["succession"] = json!(hex::encode(*id));
        Ok(view)
    })
    .await
}

/// The account's team wallets; none where team wallets are not configured.
async fn wallets<H: Hold>(h: &H) -> Result<Json> {
    match crate::wallet::list(h).await {
        Err(Error::Unavailable(_)) => Ok(json!([])),
        wallets => wallets,
    }
}

/// The account as this device reads it: through its login before it joins,
/// otherwise as its own.
async fn account<H: Hold>(h: &H) -> Result<AccountInfo> {
    let (ic, located, provisional) = h
        .with(|c| {
            Ok((
                c.session()?.ic.clone(),
                c.located,
                c.workspace.provisional(),
            ))
        })
        .await?;
    match (provisional, located) {
        (true, Some((home, account))) => Ok(read_account(&ic, home, &account).await?.info),
        (true, None) => Err(Error::NotFound),
        (false, _) => Ok(crate::account::refreshed(h).await?.info),
    }
}

/// The task `id`, after recording what the account shows done.
///
/// # Errors
/// Returns `Error::NotFound` for an unknown task.
pub async fn status<H: Hold>(h: &H, id: Hash) -> Result<Json> {
    let info = account(h).await?;
    h.with(|c| {
        let mut task = c.succession(&id)?;
        if task.observe(&info, now_ms()) {
            c.save_succession(&task)?;
        }
        Ok(task.view())
    })
    .await
}

/// This device's successions, the newest last.
///
/// # Errors
/// Returns `Error::Locked` while locked.
pub async fn list<H: Hold>(h: &H) -> Result<Json> {
    h.with(|c| {
        let tasks: Vec<Task> = c.workspace.private_after(c.keys()?, TASK, "", MAX_TASKS)?;
        let mut views: Vec<(u64, Json)> =
            tasks.iter().map(|t| (t.created_at_ms, t.view())).collect();
        views.sort_by_key(|(at, _)| *at);
        Ok(Json::Array(views.into_iter().map(|(_, v)| v).collect()))
    })
    .await
}

/// Run `step` of task `id` here: an approval for the steps the person
/// approves, the cosigner rotation itself, or the successor's confirmation
/// that the predecessor finished it. Local content and native addresses are
/// left with [`skip`] until their own steps exist.
///
/// # Errors
/// Returns `Error::Forbidden` for a step this device cannot run now, and the
/// step's own errors.
pub async fn run<H: Hold>(h: &H, source: &str, id: Hash, step: Step) -> Result<Json> {
    let info = account(h).await?;
    let task = h
        .with(|c| {
            let mut task = c.succession(&id)?;
            if task.observe(&info, now_ms()) {
                c.save_succession(&task)?;
            }
            Ok(task)
        })
        .await?;
    ensure(task.available().contains(&step), Error::Forbidden)?;
    let token: Token = decode(&unb64(&task.token)?, MAX_TOKEN)?;
    match (task.side, step) {
        (Side::Predecessor, Step::Authorize) => {
            accept(h, source, &task.token, &hex::encode(*task.successor)).await
        }
        (side, Step::ApprovalKey) => {
            // The successor has no hardware for one.
            let Some(key) = token.approval_key else {
                return record(h, id, step, json!({"approval_key": null})).await;
            };
            // Under a policy that guards account control, only a device
            // with a key registers another's.
            ensure(
                side == Side::Predecessor
                    || !info
                        .hardware_policy
                        .contains(&HardwarePurpose::AccountControl),
                Error::Forbidden,
            )?;
            h.with(|c| {
                let request = c.approval_create(
                    source,
                    Action::ApprovalKey {
                        device_id: task.successor,
                        public_key: key.public_key,
                        proof: Some(key.proof),
                    },
                    now_ms() + HIGH_RISK_TTL_MS,
                )?;
                c.approval_view(&request)
            })
            .await
        }
        (Side::Predecessor, Step::Cosigners) => match token.cosigner {
            None => record(h, id, step, json!({"cosigner": null})).await,
            Some(successor) => {
                let result = rotate::rotate(h, &successor).await?;
                let rotated = result["rotations"]
                    .as_array()
                    .is_some_and(|r| r.iter().all(|w| w["rotated"] == true));
                match rotated {
                    true => record(h, id, step, result).await,
                    // A retry finishes what remains.
                    false => Ok(result),
                }
            }
        },
        (Side::Successor, Step::Cosigners) => record(h, id, step, json!({"confirmed": true})).await,
        (Side::Successor, Step::Wallets) => {
            let wallets = wallets(h).await?;
            let predecessor = json!(hex::encode(*task.predecessor));
            let mut requests = vec![];
            for record in wallets.as_array().into_iter().flatten() {
                let wallet = crate::wallet::parse_wallet(record["wallet"].as_str().unwrap_or(""))?;
                // Done where this device is bound and the predecessor is
                // not; a wallet reads only for bound devices.
                let done = match crate::wallet::bindings(h, wallet).await {
                    Ok(view) => !view["bindings"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|b| b["device"] == predecessor),
                    Err(Error::Forbidden) => false,
                    Err(error) => return Err(error),
                };
                if !done {
                    let replaces = vec![task.predecessor];
                    requests.push(
                        crate::wallet::ops::join_request(h, source, wallet, replaces, false)
                            .await?,
                    );
                }
            }
            match requests.is_empty() {
                true => record(h, id, step, json!({"wallets": wallets})).await,
                false => Ok(json!({"requests": requests})),
            }
        }
        (Side::Successor, Step::Revoke) => {
            crate::approval::revoke_request(h, source, task.predecessor).await
        }
        (_, Step::Content | Step::NativeAddresses) => Err(Error::Unavailable(
            "this step is not available yet; leave it with skip".into(),
        )),
        _ => Err(Error::Forbidden),
    }
}

/// Record `step` of task `id` done with `evidence`.
async fn record<H: Hold>(h: &H, id: Hash, step: Step, evidence: Json) -> Result<Json> {
    h.with(|c| {
        let mut task = c.succession(&id)?;
        task.record(step, evidence, now_ms());
        c.save_succession(&task)?;
        Ok(task.view())
    })
    .await
}

/// Leave local content or native addresses behind on task `id`, recording
/// what stays.
///
/// # Errors
/// Returns `Error::InvalidInput` for another step and `Error::Forbidden`
/// when it cannot be settled yet.
pub async fn skip<H: Hold>(h: &H, id: Hash, step: Step, skipped: Vec<String>) -> Result<Json> {
    h.with(|c| {
        let mut task = c.succession(&id)?;
        task.skip(step, skipped, now_ms())?;
        c.save_succession(&task)?;
        Ok(task.view())
    })
    .await
}

/// Stop task `id`. What was committed stays: revoke the successor to undo
/// its enrollment.
///
/// # Errors
/// Returns `Error::NotFound` for an unknown task.
pub async fn cancel<H: Hold>(h: &H, id: Hash) -> Result<Json> {
    h.with(|c| {
        let mut task = c.succession(&id)?;
        task.cancelled_at_ms.get_or_insert(now_ms());
        c.save_succession(&task)?;
        Ok(task.view())
    })
    .await
}

#[cfg(test)]
#[path = "succession_tests.rs"]
mod tests;
