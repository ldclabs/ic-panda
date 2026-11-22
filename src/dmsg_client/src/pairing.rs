//! `dmsg-device-invite/2`: the desktop displays a one-time invitation and the
//! new device returns its signed pairing proof through an opaque relay mailbox.
//! Only the normal approval center can enroll the device.

use crate::Client;
use crate::account::{self, refreshed_security};

mod account_join;
mod host;
use crate::codec::{b64, canonical, decode, sha256, unb64};
use crate::crypto::{aes_gcm_open, aes_gcm_seal, random_id, random_key};
use crate::hold::{Hold, Session, Unlocked};
use crate::now_ms;
pub use account_join::{account_invitation, saved_invitation};
use candid::Principal;
use dmsg_types::user::{Capability, ControllerRole};
use dmsg_types::{AccountId, Error, Hash, Result, ensure};
pub use host::InvitationHost;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const PREFIX: &str = "dmsg-device-invite/2:";
const ACCOUNT_PREFIX: &str = "dmsg-device-invite/3:";
const CREATE: &str = "dmsg/device-invite/create/v2";
const CANCEL: &str = "dmsg/device-invite/cancel/v2";
const LIFETIME: u64 = 300_000;
const MAX_SEALED: usize = 4096 + 12 + 16;

#[derive(Clone, Serialize, Deserialize)]
struct Invitation {
    home: Principal,
    account: AccountId,
    relay: String,
    invite_id: Hash,
    expires_at: u64,
    key: Hash,
    submit_token: Hash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    binding: Option<BindingMailbox>,
}

#[derive(Clone, Serialize, Deserialize)]
struct BindingMailbox {
    invite_id: Hash,
    key: Hash,
    submit_token: Hash,
}

#[derive(Serialize)]
struct Registration {
    invite_id: Hash,
    submit_hash: Hash,
    expires_at: u64,
}

#[derive(Serialize)]
struct Cancellation {
    invite_id: Hash,
}

impl Invitation {
    fn parse(token: &str, now: u64) -> Result<Self> {
        ensure(token.len() <= 4096, Error::QuotaExceeded)?;
        let (text, dual) = if let Some(text) = token.strip_prefix(ACCOUNT_PREFIX) {
            (text, true)
        } else {
            (
                token.strip_prefix(PREFIX).ok_or(Error::IntegrityFailed)?,
                false,
            )
        };
        let invite: Self = decode(&unb64(text)?, 4096)?;
        ensure(invite.binding.is_some() == dual, Error::IntegrityFailed)?;
        if let Some(login) = &invite.binding {
            ensure(login.invite_id != invite.invite_id, Error::IntegrityFailed)?;
        }
        invite.check(now)?;
        Ok(invite)
    }

    fn binding_slot(&self) -> Option<Self> {
        let slot = self.binding.as_ref()?;
        Some(Self {
            invite_id: slot.invite_id,
            key: slot.key,
            submit_token: slot.submit_token,
            binding: None,
            ..self.clone()
        })
    }

    fn check(&self, now: u64) -> Result<()> {
        ensure(
            self.expires_at > now && self.expires_at <= now + LIFETIME,
            Error::Expired,
        )
    }

    fn registration(&self) -> Registration {
        Registration {
            invite_id: self.invite_id,
            submit_hash: sha256(self.submit_token.as_slice()),
            expires_at: self.expires_at,
        }
    }

    fn path(&self) -> String {
        format!(
            "/v1/accounts/{}/device-invites/{}",
            self.account,
            hex::encode(*self.invite_id)
        )
    }

    fn aad(&self) -> Vec<u8> {
        canonical(&(
            PREFIX,
            self.home,
            self.account,
            &self.relay,
            self.invite_id,
            self.expires_at,
        ))
    }

    fn seal(&self, token: &str, nonce: &[u8; 12]) -> Result<Vec<u8>> {
        ensure(token.len() <= 4096, Error::QuotaExceeded)?;
        let mut sealed = nonce.to_vec();
        sealed.extend(aes_gcm_seal(
            &self.key,
            nonce,
            token.as_bytes(),
            &self.aad(),
        ));
        Ok(sealed)
    }

    fn open(&self, sealed: &[u8]) -> Result<String> {
        ensure(
            (28..=MAX_SEALED).contains(&sealed.len()),
            Error::QuotaExceeded,
        )?;
        let nonce = sealed[..12].try_into().unwrap();
        let text = aes_gcm_open(&self.key, &nonce, &sealed[12..], &self.aad())?;
        String::from_utf8(text.to_vec()).map_err(|_| Error::IntegrityFailed)
    }
}

/// A desktop invitation held only by its original unlocked session.
pub struct Receiver {
    invitation: Invitation,
    generation: u64,
}

impl Receiver {
    fn context(&self, c: &Client) -> Result<()> {
        c.keys()?;
        ensure(
            c.session_generation == self.generation && c.account()?.id == self.invitation.account,
            Error::Locked,
        )?;
        ensure(
            c.config.relay.trim_end_matches('/') == self.invitation.relay,
            Error::VersionConflict,
        )
    }

    fn check_client(&self, c: &Client, now: u64) -> Result<()> {
        self.context(c)?;
        self.invitation.check(now)
    }

    /// Register the device reply mailbox of a single QR for an unlocked root
    /// administrator, plus a separate login reply mailbox when `with_login`.
    /// The relay receives only the hash of each submit credential, never the QR key.
    /// # Errors
    /// Refuses a locked, stale or unauthorized device and relay registration failures.
    async fn register<H: Hold>(h: &H, with_login: bool) -> Result<Self> {
        let h = Unlocked::new(h).await?;
        let security = refreshed_security(&h).await?;
        let receiver = h
            .with(|c| {
                let own = security.active(&c.workspace.meta.device_id)?;
                ensure(
                    own.input.role == ControllerRole::Administrator
                        && own.input.capabilities.contains(&Capability::RootManage),
                    Error::Forbidden,
                )?;
                let account = c.account()?;
                Ok(Self {
                    invitation: Invitation {
                        home: account.home,
                        account: account.id,
                        relay: c.config.relay.trim_end_matches('/').into(),
                        invite_id: Hash::new(random_id()),
                        expires_at: now_ms() + account::APPROVAL_MS,
                        key: Hash::new(random_key()),
                        submit_token: Hash::new(random_key()),
                        binding: with_login.then(|| BindingMailbox {
                            invite_id: Hash::new(random_id()),
                            key: Hash::new(random_key()),
                            submit_token: Hash::new(random_key()),
                        }),
                    },
                    generation: c.session_generation,
                })
            })
            .await?;
        for slot in
            std::iter::once(receiver.invitation.clone()).chain(receiver.invitation.binding_slot())
        {
            let path = format!("/v1/accounts/{}/device-invites", slot.account);
            let result = h
                .send(|c| {
                    receiver.check_client(c, now_ms())?;
                    c.signed_post(&path, CREATE, &slot.registration(), &slot.invite_id)
                })
                .await;
            let registered = match result {
                Ok(value) => Self::check_metadata(&slot, &value),
                Err(Error::ExecutionUnknown) => receiver.read(&h, &slot).await.map(|_| ()),
                Err(error) => Err(error),
            };
            if let Err(error) = registered {
                let _ = receiver.cancel(&h).await;
                return Err(error);
            }
        }
        h.with(|c| receiver.check_client(c, now_ms())).await?;
        Ok(receiver)
    }

    /// The relay mailbox's public identifier; it is not a submit credential.
    pub fn id(&self) -> String {
        hex::encode(*self.invitation.invite_id)
    }

    /// The invitation text and QR, for the current desktop dialog only.
    /// # Errors
    /// Returns the QR renderer's error if the invitation cannot be encoded.
    pub fn view(&self) -> Result<Value> {
        let prefix = if self.invitation.binding.is_some() {
            ACCOUNT_PREFIX
        } else {
            PREFIX
        };
        let token = format!("{prefix}{}", b64(&canonical(&self.invitation)));
        Ok(
            json!({ "id": self.id(), "qr": crate::qr::png_data(&token)?, "token": token,
            "expires_at": self.invitation.expires_at, "candidate": null }),
        )
    }

    fn check_metadata(slot: &Invitation, value: &Value) -> Result<()> {
        ensure(
            value["id"].as_str() == Some(hex::encode(*slot.invite_id).as_str())
                && value["expires_at"].as_u64() == Some(slot.expires_at),
            Error::IntegrityFailed,
        )
    }

    fn candidate_with_device(
        &self,
        c: &Client,
        value: &Value,
        now: u64,
    ) -> Result<(Value, Option<dmsg_types::user::DeviceInput>)> {
        self.check_client(c, now)?;
        Self::check_metadata(&self.invitation, value)?;
        let (candidate, device) = match value.get("sealed").ok_or(Error::IntegrityFailed)? {
            Value::Null => (Value::Null, None),
            Value::String(text) => {
                ensure(text.len() <= MAX_SEALED * 4 / 3 + 4, Error::QuotaExceeded)?;
                let token = self.invitation.open(&unb64(text)?)?;
                let pairing = c.inspect_pairing_at(&token, now)?;
                let preview = json!({
                    "device_id": hex::encode(*pairing.device.device_id),
                    "role": pairing.device.role,
                    "capabilities": pairing.device.capabilities,
                    "account": pairing.account.to_string(),
                    "expires_at": pairing.mutation.approval.expires_at,
                    "token": token,
                });
                (preview, Some(pairing.device))
            }
            _ => return Err(Error::IntegrityFailed),
        };
        Ok((
            json!({"id": self.id(), "expires_at": self.invitation.expires_at, "candidate": candidate}),
            device,
        ))
    }

    #[cfg(test)]
    fn candidate(&self, c: &Client, value: &Value, now: u64) -> Result<Value> {
        self.candidate_with_device(c, value, now)
            .map(|(view, _)| view)
    }

    fn resolve_dual(
        &self,
        c: &Client,
        slot: &Invitation,
        device: &Value,
        login: &Value,
        bound: Option<&[Principal]>,
        now: u64,
    ) -> Result<Value> {
        let (mut result, parsed_device) = self.candidate_with_device(c, device, now)?;
        let encoded = login.get("sealed").ok_or(Error::IntegrityFailed)?;
        if let Some(text) = encoded.as_str() {
            ensure(text.len() <= MAX_SEALED * 4 / 3 + 4, Error::QuotaExceeded)?;
            let binding =
                account_join::inspect_reply(c, &self.invitation, &slot.open(&unb64(text)?)?, now)?;
            let candidate = &mut result["candidate"];
            if let Some(parsed_device) = parsed_device {
                ensure(parsed_device == binding.device, Error::IntegrityFailed)?;
                if bound.is_some_and(|logins| logins.contains(&binding.principal)) {
                    candidate["code"] = json!(binding.code);
                } else {
                    *candidate = binding.view();
                }
            } else {
                *candidate = binding.view();
            }
        } else {
            ensure(encoded.is_null(), Error::IntegrityFailed)?;
            if let Some(parsed_device) = parsed_device {
                result["candidate"]["code"] = json!(account_join::comparison(
                    &self.invitation,
                    None,
                    &parsed_device,
                ));
            }
        }
        Ok(result)
    }

    /// Read and decrypt the single candidate with fresh device authentication.
    /// # Errors
    /// Rejects expired or cancelled invitations, changed sessions and bad proofs.
    pub async fn status<H: Hold>(&self, h: &H) -> Result<Value> {
        let device = self.read(h, &self.invitation).await?;
        let Some(slot) = self.invitation.binding_slot() else {
            return h
                .with(|c| {
                    self.candidate_with_device(c, &device, now_ms())
                        .map(|(view, _)| view)
                })
                .await;
        };
        let login = self.read(h, &slot).await?;
        let bound = if device["sealed"].is_string() && login["sealed"].is_string() {
            Some(account::refreshed(h).await?.info.auth_bindings)
        } else {
            None
        };
        h.with(|c| self.resolve_dual(c, &slot, &device, &login, bound.as_deref(), now_ms()))
            .await
    }

    async fn read<H: Hold>(&self, h: &H, slot: &Invitation) -> Result<Value> {
        let value = h
            .send(|c| {
                self.check_client(c, now_ms())?;
                c.signed_get(&slot.path())
            })
            .await?;
        Self::check_metadata(slot, &value)?;
        Ok(value)
    }

    /// Cancel the relay mailbox after the host has already discarded it locally.
    /// A locked/offline host may fail to notify the relay; its deadline still
    /// removes it, and the discarded local key cannot process further requests.
    /// # Errors
    /// Returns session or relay errors; no new invitation is affected.
    pub async fn cancel<H: Hold>(&self, h: &H) -> Result<()> {
        let mut result = Ok(());
        for slot in std::iter::once(self.invitation.clone()).chain(self.invitation.binding_slot()) {
            let path = format!("{}/cancel", slot.path());
            let payload = Cancellation {
                invite_id: slot.invite_id,
            };
            let id = Hash::new(random_id());
            if let Err(error) = h
                .send(|c| {
                    self.context(c)?;
                    c.signed_post(&path, CANCEL, &payload, &id)
                })
                .await
            {
                result = Err(error);
            }
        }
        result
    }
}

async fn submit<H: Hold>(h: &H, invite: &Invitation, sealed: &[u8]) -> Result<()> {
    for attempt in 0..2 {
        let relay = h
            .with(|c| {
                invite.check(now_ms())?;
                ensure(
                    c.config.relay.trim_end_matches('/') == invite.relay,
                    Error::IntegrityFailed,
                )?;
                Ok(c.relay())
            })
            .await?;
        let reply = relay
            .pairing_submit(
                &invite.account,
                &invite.invite_id,
                &invite.submit_token,
                sealed,
            )
            .await;
        match reply {
            Err(Error::ExecutionUnknown) if attempt == 0 => continue,
            result => {
                let value = result?;
                return h
                    .with(|_| {
                        invite.check(now_ms())?;
                        ensure(
                            value["digest"].as_str() == Some(hex::encode(*sha256(sealed)).as_str()),
                            Error::IntegrityFailed,
                        )?;
                        Ok(())
                    })
                    .await;
            }
        }
    }
    unreachable!("the last attempt returns")
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;
