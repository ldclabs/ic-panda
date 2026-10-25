//! This device's approval keys: P-256 keys its hardware keeps
//! ([`ApprovalKeys`]). The account registers one per device
//! (`SetApprovalKey`); for the purposes its hardware policy names, the user
//! home then requires the key's signature over each approval's hardware
//! commitment ([`dmsg_protocol::hardware_message`]).
//!
//! The person is asked once per decision, before the approval is put
//! together ([`Client::approval_answer`]): the answer confirms the decision
//! and signs, so the wait holds neither the client nor the device's
//! sequence. A key's handle is sealed under the LDK; it never leaves this
//! device and appears in no view.

use crate::approval::{Action, HIGH_RISK_TTL_MS};
use crate::client::Client;
use crate::codec::{b64, canonical, decode, unb64};
use crate::hold::Hold;
use crate::now_ms;
use crate::secure::{ApprovalKeys, PresenceKey};
use candid::Principal;
use dmsg_protocol::{
    approval_key_id, approval_key_message, hardware_message, validate_hardware_policy, verify_p256,
};
use dmsg_types::user::{
    AccountCommand, AccountInfo, ApprovalKeyInput, Capability, ControllerRole, HardwarePurpose,
};
use dmsg_types::{AccountId, Error, HardwareProof, Hash, P256Signature, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use serde_json::{Value as Json, json};
use std::sync::Arc;

/// The sealed list of this device's keys.
const KEYS: &str = "approval-keys";
/// Keys kept: the registered one and the newest made since.
const MAX_KEYS: usize = 4;
/// The format of a key another device asks an administrator to register.
const REQUEST: &str = "dmsg-approval-key/1";

/// The error of an approval the account protects, on a device without its
/// registered key.
pub const APPROVAL_KEY_REQUIRED: &str = "APPROVAL_KEY_REQUIRED";

/// An approval key this device made.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocalKey {
    key_id: Hash,
    public_key: ByteBuf,
    handle: ByteBuf,
    /// For a key that takes only the fingerprints enrolled when it was
    /// made: their digest then.
    biometry: Option<Hash>,
    created_at: u64,
}

/// What this device last read of its registered key and the policy.
#[derive(Clone, Debug)]
pub(crate) struct Seen {
    account: AccountId,
    key: Option<Hash>,
    policy: Vec<HardwarePurpose>,
}

/// A key another device made and proved, for an administrator to register.
#[derive(Serialize, Deserialize)]
struct KeyRequest {
    format: String,
    home: Principal,
    account: AccountId,
    device_id: Hash,
    key: ApprovalKeyInput,
}

/// What making a key needs, used without the client.
struct KeyTools {
    hardware: Arc<dyn ApprovalKeys>,
    presence: Option<Arc<dyn PresenceKey>>,
    account: crate::workspace::AccountRef,
}

/// A purpose's name in views, requests and questions.
pub fn purpose_name(purpose: HardwarePurpose) -> &'static str {
    match purpose {
        HardwarePurpose::AccountControl => "account_control",
        HardwarePurpose::DeviceControl => "device_control",
        HardwarePurpose::PrincipalControl => "principal_control",
        HardwarePurpose::Payment => "payment",
        HardwarePurpose::Attestation => "attestation",
    }
}

/// The purpose a name of [`purpose_name`] stands for.
///
/// # Errors
/// Returns `Error::InvalidInput` for any other name.
pub fn purpose_named(name: &str) -> Result<HardwarePurpose> {
    [
        HardwarePurpose::AccountControl,
        HardwarePurpose::DeviceControl,
        HardwarePurpose::PrincipalControl,
        HardwarePurpose::Payment,
        HardwarePurpose::Attestation,
    ]
    .into_iter()
    .find(|p| purpose_name(*p) == name)
    .ok_or_else(|| Error::InvalidInput(format!("purpose: {name}")))
}

fn names(purposes: &[HardwarePurpose]) -> Vec<&'static str> {
    purposes.iter().map(|p| purpose_name(*p)).collect()
}

/// A P-256 signature as the user home takes it: verified and with a low `s`.
fn low_s(public_key: &[u8], message: &Hash, raw: &[u8; 64]) -> Result<P256Signature> {
    let signature = p256::ecdsa::Signature::from_slice(raw)
        .map_err(|_| Error::IntegrityFailed)?
        .normalize_s();
    let signature = P256Signature::new(signature.to_bytes().into());
    verify_p256(public_key, message.as_slice(), &signature)?;
    Ok(signature)
}

/// The fingerprints a new `biometry_only` key will take.
///
/// # Errors
/// Returns `Error::Unavailable` without Touch ID.
fn biometry(presence: Option<Arc<dyn PresenceKey>>, biometry_only: bool) -> Result<Option<Hash>> {
    if !biometry_only {
        return Ok(None);
    }
    presence
        .and_then(|p| p.biometry_state())
        .map(Some)
        .ok_or_else(|| Error::Unavailable("Touch ID is not set up".into()))
}

impl Client {
    /// Keep this device's approval keys in `keys`, such as the Secure
    /// Enclave.
    pub fn set_approval_keys(&mut self, keys: Arc<dyn ApprovalKeys>) {
        self.approval_keys = Some(keys);
    }

    fn approval_hardware(&self) -> Result<Arc<dyn ApprovalKeys>> {
        self.approval_keys
            .clone()
            .ok_or_else(|| Error::Unavailable("no approval-key hardware".into()))
    }

    /// What making a key needs, taken from an unlocked client: the
    /// hardware, the presence hardware for the fingerprints, and the account.
    fn key_tools(&self) -> Result<KeyTools> {
        self.keys()?;
        Ok(KeyTools {
            hardware: self.approval_hardware()?,
            presence: self.presence.clone(),
            account: self.account()?.clone(),
        })
    }

    fn local_approval_keys(&self) -> Result<Vec<LocalKey>> {
        Ok(self
            .workspace
            .private(self.keys()?, KEYS)?
            .unwrap_or_default())
    }

    /// Keep a new key, the newest last; past [`MAX_KEYS`] the oldest that
    /// is not `registered` goes.
    fn keep_approval_key(&self, key: LocalKey, registered: Option<Hash>) -> Result<()> {
        let mut keys = self.local_approval_keys()?;
        keys.push(key);
        while keys.len() > MAX_KEYS {
            match keys.iter().position(|k| Some(k.key_id) != registered) {
                Some(at) => keys.remove(at),
                None => break,
            };
        }
        self.workspace.put_private(self.keys()?, KEYS, &keys)
    }

    /// Keep what `info` says of this device's key and the policy, for
    /// [`Self::approval_answer`].
    pub(crate) fn observe_hardware(&mut self, info: &AccountInfo) {
        self.hardware_seen = Some(Seen {
            account: info.account_id,
            key: info
                .approval_keys
                .get(&self.workspace.meta.device_id)
                .map(|k| k.key_id),
            policy: info.hardware_policy.clone(),
        });
    }

    /// This device's local key that the account registered as `registered`.
    fn registered_key(&self, registered: Option<Hash>) -> Result<Option<LocalKey>> {
        let Some(id) = registered else {
            return Ok(None);
        };
        Ok(self
            .local_approval_keys()?
            .into_iter()
            .find(|k| k.key_id == id))
    }

    /// Whether the person answers a request of `purpose` through this
    /// device's registered approval key, as last read: for a high-risk
    /// request, or for a purpose the account protects. Returns the
    /// hardware and whether the key takes only fingerprints; the answer
    /// then also signs ([`Self::hardware_proof`]).
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub(crate) fn approval_answer(
        &self,
        purpose: Option<HardwarePurpose>,
        high: bool,
    ) -> Result<Option<(Arc<dyn ApprovalKeys>, bool)>> {
        let (Some(purpose), Some(hardware), Some(seen)) =
            (purpose, self.approval_keys.clone(), &self.hardware_seen)
        else {
            return Ok(None);
        };
        if seen.account != self.account()?.id || !(high || seen.policy.contains(&purpose)) {
            return Ok(None);
        }
        Ok(self
            .registered_key(seen.key)?
            .map(|key| (hardware, key.biometry.is_some())))
    }

    /// The hardware to ask the person through for registering this
    /// device's new key `public_key`, which signs its proof with the answer.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub(crate) fn new_key_answer(
        &self,
        public_key: &[u8],
    ) -> Result<Option<(Arc<dyn ApprovalKeys>, bool)>> {
        let key = self
            .local_approval_keys()?
            .into_iter()
            .find(|k| k.public_key.as_slice() == public_key);
        Ok(match (key, &self.approval_keys) {
            (Some(key), Some(hardware)) => Some((hardware.clone(), key.biometry.is_some())),
            _ => None,
        })
    }

    /// The hardware to ask the person through where `info` requires an
    /// approval key for `purpose` and this device has its registered one,
    /// with whether the key takes only fingerprints.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub(crate) fn required_answer(
        &self,
        info: &AccountInfo,
        purpose: HardwarePurpose,
    ) -> Result<Option<(Arc<dyn ApprovalKeys>, bool)>> {
        if !info.hardware_policy.contains(&purpose) {
            return Ok(None);
        }
        let registered = info
            .approval_keys
            .get(&self.workspace.meta.device_id)
            .map(|k| k.key_id);
        Ok(
            match (self.registered_key(registered)?, &self.approval_keys) {
                (Some(key), Some(hardware)) => Some((hardware.clone(), key.biometry.is_some())),
                _ => None,
            },
        )
    }

    /// The approval-key proof of an approval of `purpose` whose
    /// `approval_message` is `message`, by the key `info` registers for
    /// this device: always where the account requires one, and otherwise
    /// when the person answered through the key for this decision. Without
    /// an answer the person is asked now.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput(APPROVAL_KEY_REQUIRED)` where the
    /// account requires a key this device lacks, `Error::Forbidden` when
    /// the person declines and `Error::IntegrityFailed` for a key the
    /// hardware lost.
    pub(crate) fn hardware_proof(
        &self,
        info: &AccountInfo,
        purpose: Option<HardwarePurpose>,
        message: Hash,
    ) -> Result<Option<HardwareProof>> {
        let Some(purpose) = purpose else {
            return Ok(None);
        };
        let required = info.hardware_policy.contains(&purpose);
        let registered = info
            .approval_keys
            .get(&self.workspace.meta.device_id)
            .map(|k| k.key_id);
        let Some(key) = self.registered_key(registered)? else {
            ensure(!required, Error::InvalidInput(APPROVAL_KEY_REQUIRED.into()))?;
            return Ok(None);
        };
        if !required && self.authorized.is_none() {
            return Ok(None);
        }
        let commitment = hardware_message(message, key.key_id);
        Ok(Some(HardwareProof {
            key_id: key.key_id,
            signature: self.approval_sign(&key, purpose_name(purpose), &commitment)?,
        }))
    }

    /// Sign `message` with `key`: with the person's answer for this
    /// decision, or by asking about `what` now.
    fn approval_sign(&self, key: &LocalKey, what: &str, message: &Hash) -> Result<P256Signature> {
        let signed = match &self.authorized {
            Some(answer) => answer.sign(&key.handle, message),
            None => self
                .approval_hardware()?
                .authorize(what, key.biometry.is_some())
                .and_then(|answer| answer.sign(&key.handle, message)),
        };
        low_s(&key.public_key, message, &self.lost(key, signed)?)
    }

    /// A failure of a fingerprint-only key after the fingerprints changed:
    /// the key is lost for good, not just unavailable now.
    fn lost<T>(&self, key: &LocalKey, result: Result<T>) -> Result<T> {
        match (result, key.biometry) {
            (Err(Error::Forbidden), _) => Err(Error::Forbidden),
            (Err(_), Some(state)) if self.biometry_changed(state) => Err(Error::IntegrityFailed),
            (result, _) => result,
        }
    }

    fn biometry_changed(&self, state: Hash) -> bool {
        self.presence.as_ref().and_then(|p| p.biometry_state()) != Some(state)
    }

    /// Register this device's own new key `public_key`, proving it with the
    /// person's answer, or another device's key with the proof it sent.
    pub(crate) async fn approval_key_execute(
        &mut self,
        device_id: Hash,
        public_key: ByteBuf,
        proof: Option<P256Signature>,
    ) -> Result<()> {
        let account = self.account()?.clone();
        let proof = match proof {
            Some(proof) => proof,
            None => {
                ensure(
                    device_id == self.workspace.meta.device_id,
                    Error::IntegrityFailed,
                )?;
                let key = self
                    .local_approval_keys()?
                    .into_iter()
                    .find(|k| k.public_key == public_key)
                    .ok_or(Error::NotFound)?;
                let message =
                    approval_key_message(account.home, &account.id, device_id, &public_key);
                self.approval_sign(&key, "approval_key", &message)?
            }
        };
        self.mutate(
            AccountCommand::SetApprovalKey {
                device_id,
                key: Some(ApprovalKeyInput { public_key, proof }),
            },
            None,
        )
        .await
        .map(|_| ())
    }

    /// Check a key request of another device of this account.
    fn inspect_key_request(&self, token: &str) -> Result<KeyRequest> {
        ensure(token.len() <= 1024, Error::QuotaExceeded)?;
        let request: KeyRequest = decode(&unb64(token)?, 1024)?;
        let account = self.account()?;
        ensure(
            request.format == REQUEST
                && request.home == account.home
                && request.account == account.id,
            Error::IntegrityFailed,
        )?;
        approval_key_id(&request.key.public_key)?;
        verify_p256(
            &request.key.public_key,
            approval_key_message(
                request.home,
                &request.account,
                request.device_id,
                &request.key.public_key,
            )
            .as_slice(),
            &request.key.proof,
        )?;
        Ok(request)
    }

    /// This device's key and the account's policy from `info`.
    fn approval_key_view(&self, info: &AccountInfo) -> Result<Json> {
        let device_id = self.workspace.meta.device_id;
        let local = self.local_approval_keys()?;
        let registered = info.approval_keys.get(&device_id);
        let own = registered.and_then(|r| local.iter().find(|k| k.key_id == r.key_id));
        Ok(json!({
            "available": self.approval_keys.is_some(),
            "device_bound": self.workspace.bound()?,
            "biometry": self.presence.as_ref().and_then(|p| p.biometry_state()).is_some(),
            "key": registered.map(|k| json!({
                "key_id": hex::encode(*k.key_id),
                "registered_at": k.registered_at,
                "registered_by": hex::encode(*k.registered_by),
                "here": own.is_some(),
                "biometry_only": own.map(|k| k.biometry.is_some()),
                "fingerprints_changed": own.and_then(|k| k.biometry).map(|s| self.biometry_changed(s)),
            })),
            "waiting": local
                .iter()
                .filter(|k| Some(k.key_id) != registered.map(|r| r.key_id))
                .map(|k| hex::encode(*k.key_id))
                .collect::<Vec<_>>(),
            "policy": names(&info.hardware_policy),
            "devices": info
                .devices
                .iter()
                .filter(|(_, d)| d.revoked_at.is_none())
                .map(|(id, _)| json!({
                    "device_id": hex::encode(**id),
                    "this_device": *id == device_id,
                    "key_id": info.approval_keys.get(id).map(|k| hex::encode(*k.key_id)),
                }))
                .collect::<Vec<_>>(),
        }))
    }
}

/// Active devices of `info` without a registered key.
fn without_keys(info: &AccountInfo) -> Vec<Hash> {
    info.devices
        .iter()
        .filter(|(id, d)| d.revoked_at.is_none() && !info.approval_keys.contains_key(*id))
        .map(|(id, _)| *id)
        .collect()
}

/// The account administrator rights account changes need.
fn administrator(state: &crate::account::AccountState, device_id: &Hash) -> Result<()> {
    let own = state.active(device_id)?;
    ensure(
        own.input.role == ControllerRole::Administrator
            && own.input.capabilities.contains(&Capability::RootManage),
        Error::Forbidden,
    )
}

/// This device's approval key and the account's hardware policy, read now.
///
/// # Errors
/// Returns `Error::AuthRequired` without a login.
pub async fn status<H: Hold>(h: &H) -> Result<Json> {
    h.with(|c| c.keys().map(|_| ())).await?;
    let state = crate::account::refreshed(h).await?;
    h.with(|c| c.approval_key_view(&state.info)).await
}

/// Ask to register a new approval key of this device; the key is made now
/// without asking, and proves itself when the request is approved. While
/// the account requires keys, approving needs the key it replaces, if any.
///
/// # Errors
/// Returns `Error::Unavailable` without approval-key hardware, or without
/// Touch ID for a `biometry_only` key.
pub async fn register_request<H: Hold>(h: &H, source: &str, biometry_only: bool) -> Result<Json> {
    let KeyTools {
        hardware, presence, ..
    } = h.with(|c| c.key_tools()).await?;
    let state = crate::account::refreshed(h).await?;
    let biometry = biometry(presence, biometry_only)?;
    let (public_key, handle) = hardware.create(biometry_only)?;
    let key_id = approval_key_id(&public_key)?;
    h.with(|c| {
        let device_id = c.workspace.meta.device_id;
        state.active(&device_id)?;
        c.keep_approval_key(
            LocalKey {
                key_id,
                public_key: public_key.clone().into(),
                handle: handle.into(),
                biometry,
                created_at: now_ms(),
            },
            state.info.approval_keys.get(&device_id).map(|k| k.key_id),
        )?;
        let request = c.approval_create(
            source,
            Action::ApprovalKey {
                device_id,
                public_key: public_key.into(),
                proof: None,
            },
            now_ms() + HIGH_RISK_TTL_MS,
        )?;
        c.approval_view(&request)
    })
    .await
}

/// A request for an administrator device to register a new approval key
/// of this device, for when the account requires keys and this device has
/// none that works: the key is made and proves itself now, so the person
/// is asked here. Returns the base64url token to approve elsewhere.
///
/// # Errors
/// As [`register_request`]; `Error::Forbidden` when the person declines.
pub async fn key_request<H: Hold>(h: &H, biometry_only: bool) -> Result<String> {
    let KeyTools {
        hardware,
        presence,
        account,
    } = h.with(|c| c.key_tools()).await?;
    let state = crate::account::refreshed(h).await?;
    let device_id = h.with(|c| Ok(c.workspace.meta.device_id)).await?;
    state.active(&device_id)?;
    let biometry = biometry(presence, biometry_only)?;
    let (public_key, handle) = hardware.create(biometry_only)?;
    let key_id = approval_key_id(&public_key)?;
    let message = approval_key_message(account.home, &account.id, device_id, &public_key);
    let raw = hardware
        .authorize("approval_key", biometry_only)?
        .sign(&handle, &message)?;
    let proof = low_s(&public_key, &message, &raw)?;
    h.with(|c| {
        c.keep_approval_key(
            LocalKey {
                key_id,
                public_key: public_key.clone().into(),
                handle: handle.into(),
                biometry,
                created_at: now_ms(),
            },
            state.info.approval_keys.get(&device_id).map(|k| k.key_id),
        )?;
        Ok(b64(&canonical(&KeyRequest {
            format: REQUEST.into(),
            home: account.home,
            account: account.id,
            device_id,
            key: ApprovalKeyInput {
                public_key: public_key.into(),
                proof,
            },
        })))
    })
    .await
}

/// Ask to register the key another device of the account requested with
/// `token`; `device` is the ID that device shows, which the person names.
/// Only an administrator device approves it.
///
/// # Errors
/// Returns `Error::Forbidden` on a device that cannot approve account
/// changes, `Error::InvalidInput` when `device` is not the token's, and
/// refuses a token of another account or a bad proof of possession.
pub async fn approve_request<H: Hold>(
    h: &H,
    source: &str,
    token: &str,
    device: &str,
) -> Result<Json> {
    h.with(|c| c.keys().map(|_| ())).await?;
    let state = crate::account::refreshed(h).await?;
    h.with(|c| {
        administrator(&state, &c.workspace.meta.device_id)?;
        let request = c.inspect_key_request(token)?;
        ensure(
            hex::encode(*request.device_id) == device,
            Error::InvalidInput("device does not match the request".into()),
        )?;
        state.active(&request.device_id)?;
        let request = c.approval_create(
            source,
            Action::ApprovalKey {
                device_id: request.device_id,
                public_key: request.key.public_key,
                proof: Some(request.key.proof),
            },
            now_ms() + HIGH_RISK_TTL_MS,
        )?;
        c.approval_view(&request)
    })
    .await
}

/// Ask to retire the approval key of `device_id`: this device's own, the
/// default, or as an administrator another's.
///
/// # Errors
/// Returns `Error::NotFound` for a device without a registered key and
/// `Error::Forbidden` for another device's key on a device that cannot
/// approve account changes.
pub async fn retire_request<H: Hold>(h: &H, source: &str, device_id: Option<Hash>) -> Result<Json> {
    h.with(|c| c.keys().map(|_| ())).await?;
    let state = crate::account::refreshed(h).await?;
    h.with(|c| {
        let own = c.workspace.meta.device_id;
        let device_id = device_id.unwrap_or(own);
        state.active(&own)?;
        if device_id != own {
            administrator(&state, &own)?;
        }
        ensure(
            state.info.approval_keys.contains_key(&device_id),
            Error::NotFound,
        )?;
        let request = c.approval_create(
            source,
            Action::ApprovalKeyRetire { device_id },
            now_ms() + HIGH_RISK_TTL_MS,
        )?;
        c.approval_view(&request)
    })
    .await
}

/// Ask to require approval keys for `purposes`, which then take the three
/// control purposes too; empty turns the requirement off. Turning it on
/// needs this device's registered key, and lists the devices that will
/// lose those operations for want of one.
///
/// # Errors
/// Returns `Error::Forbidden` on a device that cannot approve account
/// changes and `Error::InvalidInput(APPROVAL_KEY_REQUIRED)` for turning it
/// on without a key here.
pub async fn policy_request<H: Hold>(
    h: &H,
    source: &str,
    purposes: Vec<HardwarePurpose>,
) -> Result<Json> {
    h.with(|c| c.keys().map(|_| ())).await?;
    let state = crate::account::refreshed(h).await?;
    let mut purposes = purposes;
    if !purposes.is_empty() {
        for p in dmsg_types::user::CONTROL_PURPOSES {
            if !purposes.contains(&p) {
                purposes.push(p);
            }
        }
    }
    purposes.sort();
    purposes.dedup();
    validate_hardware_policy(&purposes)?;
    h.with(|c| {
        let own = c.workspace.meta.device_id;
        administrator(&state, &own)?;
        let registered = state.info.approval_keys.get(&own).map(|k| k.key_id);
        ensure(
            purposes.is_empty() || c.registered_key(registered)?.is_some(),
            Error::InvalidInput(APPROVAL_KEY_REQUIRED.into()),
        )?;
        let request = c.approval_create(
            source,
            Action::HardwarePolicy {
                purposes,
                previous: state.info.hardware_policy.clone(),
                without_keys: without_keys(&state.info),
            },
            now_ms() + HIGH_RISK_TTL_MS,
        )?;
        c.approval_view(&request)
    })
    .await
}

/// The views of the approval-key requests.
pub(crate) fn detail(client: &Client, action: &Action) -> Option<Json> {
    let this = client.workspace.meta.device_id;
    Some(match action {
        Action::ApprovalKey {
            device_id,
            public_key,
            ..
        } => json!({
            "device_id": hex::encode(**device_id),
            "this_device": *device_id == this,
            "key_id": approval_key_id(public_key).ok().map(|id| hex::encode(*id)),
        }),
        Action::ApprovalKeyRetire { device_id } => json!({
            "device_id": hex::encode(**device_id),
            "this_device": *device_id == this,
        }),
        Action::HardwarePolicy {
            purposes,
            previous,
            without_keys,
        } => json!({
            "purposes": names(purposes),
            "previous": names(previous),
            "without_keys": without_keys.iter().map(|id| hex::encode(**id)).collect::<Vec<_>>(),
        }),
        _ => return None,
    })
}

#[cfg(test)]
#[path = "approval_key_tests.rs"]
mod tests;
