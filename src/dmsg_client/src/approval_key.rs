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
use std::time::{Duration, Instant};

/// One final protocol commitment, prepared without asking for the person.
pub(crate) struct PreparedHardwareProof {
    hardware: Arc<dyn ApprovalKeys>,
    key: LocalKey,
    reason: &'static str,
    message: Hash,
    expires_at: u64,
    deadline: Instant,
    signer: Option<Arc<dyn crate::secure::PreparedSignature>>,
}

/// The original plan and its hardware answer stay together until client validation.
pub(crate) struct HardwareProofAnswer {
    prepared: PreparedHardwareProof,
    signature: Result<[u8; 64]>,
}

impl PreparedHardwareProof {
    pub(crate) fn cancellation(&self) -> crate::secure::SigningCancellation {
        crate::secure::SigningCancellation(self.signer.clone())
    }

    fn check_time(&self, now: u64) -> Result<()> {
        ensure(
            now < self.expires_at && Instant::now() < self.deadline,
            Error::Expired,
        )
    }

    /// Blocking user interaction; the caller must run this outside every Hold step.
    pub(crate) fn sign(self) -> HardwareProofAnswer {
        let signature = (|| {
            self.check_time(now_ms())?;
            if let Some(signer) = &self.signer {
                let signature = signer.sign()?;
                self.check_time(now_ms())?;
                return Ok(signature);
            }
            let answer = self.hardware.authorize_keys(
                self.reason,
                self.key.biometry_only(),
                &[self.key.handle.to_vec()],
            )?;
            self.check_time(now_ms())?;
            let signature = answer.sign(&self.key.handle, &self.message)?;
            self.check_time(now_ms())?;
            Ok(signature)
        })();
        HardwareProofAnswer {
            prepared: self,
            signature,
        }
    }
}

impl HardwareProofAnswer {
    pub(crate) fn error(&self) -> Option<Error> {
        self.signature.as_ref().err().cloned()
    }
}

/// The sealed list of this device's keys.
const KEYS: &str = "approval-keys";
/// Keys kept: the registered one and the newest made since.
const MAX_KEYS: usize = 4;
/// The format of a key another device asks an administrator to register.
const REQUEST: &str = "dmsg-approval-key/1";

/// The error of an approval the account protects, on a device without its
/// registered key.
pub const APPROVAL_KEY_REQUIRED: &str = "APPROVAL_KEY_REQUIRED";
/// The host must prepare a fresh answer outside its client executor.
pub const HARDWARE_AUTHORIZATION_REQUIRED: &str = "hardware: authorization required";

/// An approval key this device made.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocalKey {
    key_id: Hash,
    public_key: ByteBuf,
    handle: ByteBuf,
    /// For a key that takes only the fingerprints enrolled when it was
    /// made: their digest then.
    biometry: Option<Hash>,
    #[serde(default)]
    biometry_only: bool,
    created_at: u64,
}

impl LocalKey {
    fn biometry_only(&self) -> bool {
        self.biometry_only || self.biometry.is_some()
    }
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

/// Own the provider object until its handle is durably referenced by the client.
struct PendingKey {
    hardware: Arc<dyn ApprovalKeys>,
    handle: Vec<u8>,
    kept: bool,
}

impl PendingKey {
    fn new(hardware: Arc<dyn ApprovalKeys>, handle: &[u8]) -> Self {
        Self {
            hardware,
            handle: handle.to_vec(),
            kept: false,
        }
    }
}

impl Drop for PendingKey {
    fn drop(&mut self) {
        if !self.kept {
            let _ = self.hardware.delete_key(&self.handle);
        }
    }
}

/// A new local key awaiting its proof of possession outside the client.
pub struct KeyRequestPreparation {
    hardware: Arc<dyn ApprovalKeys>,
    account: crate::workspace::AccountRef,
    device_id: Hash,
    generation: u64,
    key: LocalKey,
    pending: PendingKey,
    signer: Option<Arc<dyn crate::secure::PreparedSignature>>,
}

/// A proved key, not persisted until the originating session is rechecked.
pub struct ProvedKeyRequest {
    preparation: KeyRequestPreparation,
    proof: P256Signature,
}

impl KeyRequestPreparation {
    fn freeze(mut self) -> Result<Self> {
        let message = approval_key_message(
            self.account.home,
            &self.account.id,
            self.device_id,
            &self.key.public_key,
        );
        self.signer = self.hardware.prepare_signature(
            "approval_key",
            self.key.biometry_only(),
            &self.key.handle,
            &message,
        )?;
        Ok(self)
    }

    /// Keep this in the async owner so cancellation revokes only this prompt.
    pub fn cancellation(&self) -> crate::secure::SigningCancellation {
        crate::secure::SigningCancellation(self.signer.clone())
    }

    /// Ask once and prove the new key without borrowing the client.
    ///
    /// # Errors
    /// Returns cancellation, unavailable hardware or an invalid signature.
    pub fn prove(self) -> Result<ProvedKeyRequest> {
        let message = approval_key_message(
            self.account.home,
            &self.account.id,
            self.device_id,
            &self.key.public_key,
        );
        let raw = match &self.signer {
            Some(signer) => signer.sign()?,
            None => self
                .hardware
                .authorize_keys(
                    "approval_key",
                    self.key.biometry_only(),
                    &[self.key.handle.to_vec()],
                )?
                .sign(&self.key.handle, &message)?,
        };
        let proof = low_s(&self.key.public_key, &message, &raw)?;
        Ok(ProvedKeyRequest {
            preparation: self,
            proof,
        })
    }
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
    let presence = presence.ok_or_else(|| Error::Unavailable("biometry unavailable".into()))?;
    ensure(
        presence.supports_biometry_only(),
        Error::Unavailable("biometry unavailable".into()),
    )?;
    Ok(presence.biometry_state())
}

impl Client {
    /// Hardware handles for the displayed decision, prepared before prompting.
    /// No future action can add a signing key to this set.
    pub fn approval_signing_handles(&self, id: &str) -> Result<Vec<Vec<u8>>> {
        let request = self.approval_record(id)?;
        let mut handles = Vec::new();
        if let Some(seen) = &self.hardware_seen
            && let Some(key) = self.registered_key(seen.key)?
        {
            handles.push(key.handle.to_vec());
        }
        if let Action::ApprovalKey { public_key, .. } = &request.action
            && let Some(key) = self
                .local_approval_keys()?
                .into_iter()
                .find(|k| k.public_key == *public_key)
            && !handles.contains(&key.handle.to_vec())
        {
            handles.push(key.handle.to_vec());
        }
        ensure(
            !handles.is_empty() && handles.len() <= 2,
            Error::IntegrityFailed,
        )?;
        Ok(handles)
    }

    /// Keep this device's approval keys in `keys`, such as the Secure
    /// Enclave.
    pub fn set_approval_keys(&mut self, keys: Arc<dyn ApprovalKeys>) {
        self.approval_keys = Some(keys);
    }

    /// For suspendable hosts: never start an unprepared system prompt inside a
    /// client operation. A raced policy change requires a new explicit attempt.
    pub fn require_prepared_approval(&mut self) {
        self.require_prepared_approval = true;
    }

    fn approval_hardware(&self) -> Result<Arc<dyn ApprovalKeys>> {
        self.approval_keys
            .clone()
            .filter(|hardware| hardware.available())
            .ok_or_else(|| Error::Unavailable("hardware: no approval-key hardware".into()))
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
        let mut discarded = Vec::new();
        keys.push(key);
        while keys.len() > MAX_KEYS {
            match keys.iter().position(|k| Some(k.key_id) != registered) {
                Some(at) => discarded.push(keys.remove(at)),
                None => break,
            }
        }
        self.workspace.put_private(self.keys()?, KEYS, &keys)?;
        if let Some(hardware) = &self.approval_keys {
            for key in discarded {
                // Only after storage commits: a failed write keeps the old keys usable.
                if keys.iter().all(|retained| retained.handle != key.handle) {
                    let _ = hardware.delete_key(&key.handle);
                }
            }
        }
        Ok(())
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
        let (Some(purpose), Some(seen)) = (purpose, &self.hardware_seen) else {
            return Ok(None);
        };
        // A device still waiting to join has no key the account registered.
        let Ok(account) = self.account() else {
            return Ok(None);
        };
        if seen.account != account.id || !(high || seen.policy.contains(&purpose)) {
            return Ok(None);
        }
        let key = self.registered_key(seen.key)?;
        if seen.policy.contains(&purpose) {
            ensure(
                key.is_some(),
                Error::InvalidInput(APPROVAL_KEY_REQUIRED.into()),
            )?;
            ensure(
                self.approval_keys.is_some(),
                Error::Unavailable("hardware: no approval-key hardware".into()),
            )?;
        }
        Ok(match (key, &self.approval_keys) {
            (Some(key), Some(hardware)) => Some((hardware.clone(), key.biometry_only())),
            _ => None,
        })
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
        // A required replacement signs with both the new key (possession) and
        // the currently registered key (authority). Ask for the stronger ACL
        // once, rather than letting an old biometry-only key ask a second time.
        let old_biometry = match &self.hardware_seen {
            Some(seen)
                if seen.account == self.account()?.id
                    && seen.policy.contains(&HardwarePurpose::AccountControl) =>
            {
                let old = self
                    .registered_key(seen.key)?
                    .ok_or_else(|| Error::InvalidInput(APPROVAL_KEY_REQUIRED.into()))?;
                old.biometry_only()
            }
            _ => false,
        };
        Ok(match (key, &self.approval_keys) {
            (Some(key), Some(hardware)) => {
                Some((hardware.clone(), old_biometry || key.biometry_only()))
            }
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
                (Some(key), Some(hardware)) => {
                    ensure(
                        !self.require_prepared_approval,
                        Error::Unavailable(HARDWARE_AUTHORIZATION_REQUIRED.into()),
                    )?;
                    Some((hardware.clone(), key.biometry_only()))
                }
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
        &mut self,
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

    /// Freeze the exact hardware message before the host starts authentication.
    pub(crate) fn prepare_hardware_proof(
        &self,
        info: &AccountInfo,
        purpose: HardwarePurpose,
        message: Hash,
        expires_at: u64,
        now: u64,
    ) -> Result<PreparedHardwareProof> {
        self.keys()?;
        let registered = info
            .approval_keys
            .get(&self.workspace.meta.device_id)
            .map(|key| key.key_id);
        let key = self
            .registered_key(registered)?
            .ok_or_else(|| Error::InvalidInput(APPROVAL_KEY_REQUIRED.into()))?;
        let remaining = expires_at
            .checked_sub(now)
            .filter(|ms| *ms > 0)
            .ok_or(Error::Expired)?;
        let hardware = self.approval_hardware()?;
        let message = hardware_message(message, key.key_id);
        let signer = hardware.prepare_signature(
            purpose_name(purpose),
            key.biometry_only(),
            &key.handle,
            &message,
        )?;
        Ok(PreparedHardwareProof {
            hardware,
            reason: purpose_name(purpose),
            message,
            key,
            expires_at,
            deadline: Instant::now() + Duration::from_millis(remaining.min(90_000)),
            signer,
        })
    }

    /// This device's new key proves only its fixed registration context. The
    /// mutation authorizing replacement is prepared after this proof is known.
    pub(crate) fn prepare_key_possession(
        &self,
        device: Hash,
        public_key: &[u8],
        expires_at: u64,
        now: u64,
    ) -> Result<PreparedHardwareProof> {
        ensure(
            device == self.workspace.meta.device_id,
            Error::IntegrityFailed,
        )?;
        let account = self.account()?;
        let key = self
            .local_approval_keys()?
            .into_iter()
            .find(|k| k.public_key.as_slice() == public_key)
            .ok_or(Error::NotFound)?;
        let remaining = expires_at
            .checked_sub(now)
            .filter(|ms| *ms > 0)
            .ok_or(Error::Expired)?;
        let hardware = self.approval_hardware()?;
        let message = approval_key_message(account.home, &account.id, device, public_key);
        let signer = hardware.prepare_signature(
            "approval_key",
            key.biometry_only(),
            &key.handle,
            &message,
        )?;
        Ok(PreparedHardwareProof {
            hardware,
            reason: "approval_key",
            message,
            key,
            expires_at,
            deadline: Instant::now() + Duration::from_millis(remaining.min(90_000)),
            signer,
        })
    }

    /// Verify a hardware answer against its frozen message and this device's key state.
    pub(crate) fn complete_hardware_proof(
        &mut self,
        answer: HardwareProofAnswer,
        now: u64,
    ) -> Result<HardwareProof> {
        let HardwareProofAnswer {
            prepared,
            signature,
        } = answer;
        // Classify actual key failures even if the request expired while asking.
        let signature = self.approval_signature(&prepared.key, &prepared.message, signature)?;
        prepared.check_time(now)?;
        Ok(HardwareProof {
            key_id: prepared.key.key_id,
            signature,
        })
    }

    /// Sign `message` with `key`: with the person's answer for this
    /// decision, or by asking about `what` now.
    fn approval_sign(
        &mut self,
        key: &LocalKey,
        what: &str,
        message: &Hash,
    ) -> Result<P256Signature> {
        let signed = match &self.authorized {
            Some(answer) => answer.sign(&key.handle, message),
            None if self.require_prepared_approval => {
                return Err(Error::Unavailable(HARDWARE_AUTHORIZATION_REQUIRED.into()));
            }
            None => self
                .approval_hardware()?
                .authorize_keys(what, key.biometry_only(), &[key.handle.to_vec()])
                .and_then(|answer| answer.sign(&key.handle, message)),
        };
        self.approval_signature(key, message, signed)
    }

    fn approval_signature(
        &mut self,
        key: &LocalKey,
        message: &Hash,
        signed: Result<[u8; 64]>,
    ) -> Result<P256Signature> {
        // A key made after a late anchor says nothing about the old keys
        // copied here before it. Only those may settle that anchor, by what
        // the enclave answered: a key it cannot open here, not a
        // fingerprint-only key that this Mac's new fingerprints retired.
        let signed = if self
            .workspace
            .meta
            .anchor
            .as_ref()
            .is_some_and(|a| key.created_at > a.created_at)
        {
            signed
        } else {
            self.hardware_key(signed)
        };
        let signed = self.lost(key, signed);
        low_s(&key.public_key, message, &signed?)
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
        self.presence
            .as_ref()
            .and_then(|p| p.biometry_state())
            .is_some_and(|current| current != state)
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
            "available": self.approval_keys.as_ref().is_some_and(|hardware| hardware.available()),
            "device_bound": self.workspace.bound()?,
            "biometry": self.presence.as_ref().is_some_and(|p| p.supports_biometry_only()),
            "key": registered.map(|k| json!({
                "key_id": hex::encode(*k.key_id),
                "registered_at": k.registered_at,
                "registered_by": hex::encode(*k.registered_by),
                "here": own.is_some(),
                "biometry_only": own.map(|k| k.biometry_only()),
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

/// Preview either signed request accepted by the device scanner. The approval
/// entry points still verify the original token and current account state.
///
/// # Errors
/// Rejects a locked workspace, invalid proofs and unavailable account state.
pub async fn inspect_device_request<H: Hold>(h: &H, token: &str) -> Result<Json> {
    ensure(token.len() <= 4096, Error::QuotaExceeded)?;
    let h = crate::hold::Unlocked::new(h).await?;
    let state = crate::account::refreshed(&h).await?;
    h.with(|c| c.device_request_view(&state, token)).await
}

impl Client {
    /// Fingerprints to compare on the originating device and the reviewing administrator.
    /// # Errors
    /// Rejects an invalid token or a request for another account.
    pub fn enrollment_fingerprints(&self, token: &str) -> Result<Json> {
        let request = self.inspect_key_request(token)?;
        Ok(
            json!({"token": token, "device_id": hex::encode(*request.device_id),
            "key_id": hex::encode(*approval_key_id(&request.key.public_key)?)}),
        )
    }

    fn device_request_view(
        &self,
        state: &crate::account::AccountState,
        token: &str,
    ) -> Result<Json> {
        let own = self.workspace.meta.device_id;
        state.active(&own)?;
        let (mut view, purpose, complete, stale, self_enrollment) = if let Ok(key) =
            self.inspect_key_request(token)
        {
            state.active(&key.device_id)?;
            let id = approval_key_id(&key.key.public_key)?;
            let complete = state
                .info
                .approval_keys
                .get(&key.device_id)
                .is_some_and(|k| k.key_id == id);
            (
                json!({"kind":"approval_key", "device_id":hex::encode(*key.device_id),
                    "key_id":hex::encode(*id)}),
                HardwarePurpose::AccountControl,
                complete,
                false,
                key.device_id == own,
            )
        } else if let Ok(login) = self.inspect_binding(token) {
            (
                json!({"kind":"login_binding","principal":login.principal.to_text(),"expires_at":login.expires_at}),
                HardwarePurpose::AccountControl,
                state.info.auth_bindings.contains(&login.principal),
                false,
                false,
            )
        } else {
            let pair = self.inspect_pairing(token)?;
            let device = &pair.device;
            let complete = state
                .active(&device.device_id)
                .is_ok_and(|d| d.input == *device);
            (
                json!({"kind":"device_pair", "device_id":hex::encode(*device.device_id),
                    "role":device.role, "capabilities":device.capabilities, "account":pair.account.to_string(),
                    "expires_at":pair.mutation.approval.expires_at}),
                HardwarePurpose::DeviceControl,
                complete,
                state.info.account_version != pair.mutation.expected_version,
                false,
            )
        };
        self.approval_route(state, purpose, complete, stale, self_enrollment, &mut view)?;
        Ok(view)
    }

    pub(crate) fn invitation_route(&self, state: &crate::account::AccountState) -> Result<Json> {
        let mut view = json!({"account":state.info.account_id.to_string()});
        self.approval_route(
            state,
            HardwarePurpose::DeviceControl,
            false,
            false,
            false,
            &mut view,
        )?;
        Ok(view)
    }

    fn approval_route(
        &self,
        state: &crate::account::AccountState,
        purpose: HardwarePurpose,
        complete: bool,
        stale: bool,
        self_enrollment: bool,
        view: &mut Json,
    ) -> Result<()> {
        let own = self.workspace.meta.device_id;
        let protected = state.info.hardware_policy.contains(&purpose);
        let registered = state.info.approval_keys.get(&own).map(|k| k.key_id);
        let key = self.registered_key(registered)?;
        let hardware = self.approval_keys.as_ref().is_some_and(|h| h.available());
        let usable = key.is_some_and(|k| !k.biometry.is_some_and(|s| self.biometry_changed(s)));
        let admin = administrator(state, &own).is_ok();
        let route = if complete {
            "done"
        } else if stale {
            "refresh"
        } else if admin && (!protected || usable && hardware) {
            "approve"
        } else {
            "handoff"
        };
        let approvers: Vec<_> = state
            .security
            .devices
            .iter()
            .filter(|(id, device)| {
                **id != own
                    && device.revoked_at.is_none()
                    && device.input.role == ControllerRole::Administrator
                    && device.input.capabilities.contains(&Capability::RootManage)
                    && (!protected || state.info.approval_keys.contains_key(*id))
            })
            .map(|(id, _)| hex::encode(**id))
            .collect();
        view["route"] = json!(route);
        view["can_enroll"] = json!(
            route == "handoff"
                && !self_enrollment
                && admin
                && hardware
                && !usable
                && !approvers.is_empty()
        );
        view["approvers"] = json!(approvers);
        Ok(())
    }
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

/// Combined owner settings view. Keep the existing approval-key view's public
/// shape stable; device-local binding details belong to this explicit view.
///
/// # Errors
/// As [`status`], or a local credential-store error.
pub async fn owner_security_status<H: Hold>(h: &H) -> Result<Json> {
    h.with(|c| c.keys().map(|_| ())).await?;
    let state = crate::account::refreshed(h).await?;
    h.with(|c| {
        let mut view = c.approval_key_view(&state.info)?;
        let binding = c.device_binding_status()?;
        for name in ["bound_biometry_only", "bound_key_available", "unprotected"] {
            view[name] = binding[name].clone();
        }
        view["can_manage_policy"] = state
            .info
            .devices
            .get(&c.workspace.meta.device_id)
            .is_some_and(|device| {
                device.revoked_at.is_none()
                    && device.input.role == ControllerRole::Administrator
                    && device.input.capabilities.contains(&Capability::RootManage)
            })
            .into();
        Ok(view)
    })
    .await
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
    let mut pending = PendingKey::new(hardware, &handle);
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
                biometry_only,
                created_at: now_ms(),
            },
            state.info.approval_keys.get(&device_id).map(|k| k.key_id),
        )?;
        pending.kept = true;
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
    let prepared = prepare_key_request(h, biometry_only).await?;
    complete_key_request(h, prepared.prove()?).await
}

/// Prepare a cross-device registration without asking on the client executor.
///
/// # Errors
/// As [`key_request`].
pub async fn prepare_key_request<H: Hold>(
    h: &H,
    biometry_only: bool,
) -> Result<KeyRequestPreparation> {
    let KeyTools {
        hardware,
        presence,
        account,
    } = h.with(|c| c.key_tools()).await?;
    let state = crate::account::refreshed(h).await?;
    let (device_id, generation) = h
        .with(|c| Ok((c.workspace.meta.device_id, c.session_generation())))
        .await?;
    state.active(&device_id)?;
    let biometry = biometry(presence, biometry_only)?;
    let (public_key, handle) = hardware.create(biometry_only)?;
    let pending = PendingKey::new(hardware.clone(), &handle);
    let key_id = approval_key_id(&public_key)?;
    KeyRequestPreparation {
        hardware,
        account,
        device_id,
        generation,
        pending,
        signer: None,
        key: LocalKey {
            key_id,
            public_key: public_key.into(),
            handle: handle.into(),
            biometry,
            biometry_only,
            created_at: now_ms(),
        },
    }
    .freeze()
}

/// Save a proved registration only in its original account/session, using a
/// fresh account read to preserve the currently registered local key.
///
/// # Errors
/// Returns `Locked` after a session change and the account read's errors.
pub async fn complete_key_request<H: Hold>(h: &H, proved: ProvedKeyRequest) -> Result<String> {
    let ProvedKeyRequest { preparation, proof } = proved;
    let KeyRequestPreparation {
        account,
        device_id,
        generation,
        key,
        mut pending,
        ..
    } = preparation;
    h.with(|c| {
        ensure(
            c.session_generation() == generation && c.account()?.id == account.id,
            Error::Locked,
        )
    })
    .await?;
    let state = crate::account::refreshed(h).await?;
    h.with(|c| {
        ensure(
            c.session_generation() == generation && c.account()?.id == account.id,
            Error::Locked,
        )?;
        state.active(&device_id)?;
        let public_key = key.public_key.clone();
        c.keep_approval_key(
            key,
            state.info.approval_keys.get(&device_id).map(|k| k.key_id),
        )?;
        pending.kept = true;
        Ok(b64(&canonical(&KeyRequest {
            format: REQUEST.into(),
            home: account.home,
            account: account.id,
            device_id,
            key: ApprovalKeyInput { public_key, proof },
        })))
    })
    .await
}

/// What making a successor's key needs. A struct, not a tuple: a trait
/// object in a tuple the client returns would tie the futures that hold it
/// to the client borrow, and they would no longer be `Send`.
struct SuccessorTools {
    hardware: Option<Arc<dyn ApprovalKeys>>,
    device_id: Hash,
    generation: u64,
}

/// A new approval key of this device, proved for `account` before the device
/// joins it, for the device it succeeds to register; None without approval-key
/// hardware. The person is asked once, and the key is kept as
/// [`key_request`]'s.
///
/// # Errors
/// Returns `Error::Locked` when the device locks meanwhile, and
/// `Error::Forbidden` when the person declines.
pub(crate) async fn successor_key<H: Hold>(
    h: &H,
    account: crate::workspace::AccountRef,
) -> Result<Option<ApprovalKeyInput>> {
    let SuccessorTools {
        hardware,
        device_id,
        generation,
    } = h
        .with(|c| {
            c.keys()?;
            Ok(SuccessorTools {
                hardware: c.approval_hardware().ok(),
                device_id: c.workspace.meta.device_id,
                generation: c.session_generation(),
            })
        })
        .await?;
    let Some(hardware) = hardware else {
        return Ok(None);
    };
    let (public_key, handle) = hardware.create(false)?;
    let pending = PendingKey::new(hardware.clone(), &handle);
    let preparation = KeyRequestPreparation {
        hardware,
        account,
        device_id,
        generation,
        pending,
        signer: None,
        key: LocalKey {
            key_id: approval_key_id(&public_key)?,
            public_key: public_key.into(),
            handle: handle.into(),
            biometry: None,
            biometry_only: false,
            created_at: now_ms(),
        },
    }
    .freeze()?;
    let _signature = preparation.cancellation();
    let ProvedKeyRequest {
        mut preparation,
        proof,
    } = tokio::task::spawn_blocking(move || preparation.prove())
        .await
        .map_err(|_| Error::Unavailable("hardware: the request stopped".into()))??;
    h.with(|c| {
        ensure(c.session_generation() == generation, Error::Locked)?;
        let public_key = preparation.key.public_key.clone();
        c.keep_approval_key(preparation.key, None)?;
        preparation.pending.kept = true;
        Ok(Some(ApprovalKeyInput { public_key, proof }))
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

#[cfg(test)]
#[path = "approval_key_mobile_tests.rs"]
mod mobile_tests;
