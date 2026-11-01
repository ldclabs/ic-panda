//! `dmsg-device-invite/2`: the desktop displays a one-time invitation and the
//! new device returns its signed pairing proof through an opaque relay mailbox.
//! Only the normal approval center can enroll the device.

use crate::Client;
use crate::account::{self, refreshed_security};
use crate::codec::{b64, canonical, decode, sha256, unb64};
use crate::crypto::{aes_gcm_open, aes_gcm_seal, random_id, random_key};
use crate::hold::{Hold, Session, Unlocked};
use crate::now_ms;
use candid::Principal;
use dmsg_types::user::{Capability, ControllerRole};
use dmsg_types::{AccountId, Error, Hash, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const PREFIX: &str = "dmsg-device-invite/2:";
const CREATE: &str = "dmsg/device-invite/create/v2";
const CANCEL: &str = "dmsg/device-invite/cancel/v2";
const LIFETIME: u64 = 300_000;
const MAX_SEALED: usize = 4096 + 12 + 16;

#[derive(Serialize, Deserialize)]
struct Invitation {
    home: Principal,
    account: AccountId,
    relay: String,
    invite_id: Hash,
    expires_at: u64,
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
        let text = token.strip_prefix(PREFIX).ok_or(Error::IntegrityFailed)?;
        let invite: Self = decode(&unb64(text)?, 4096)?;
        invite.check(now)?;
        Ok(invite)
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

    /// Register a five-minute mailbox for an unlocked root administrator. The
    /// relay receives only the hash of a submit credential, never the QR key.
    /// # Errors
    /// Refuses a locked, stale or unauthorized device and relay registration failures.
    pub async fn new<H: Hold>(h: &H) -> Result<Self> {
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
                    },
                    generation: c.session_generation,
                })
            })
            .await?;
        let path = format!(
            "/v1/accounts/{}/device-invites",
            receiver.invitation.account
        );
        let registration = receiver.invitation.registration();
        let result = h
            .send(|c| {
                receiver.check_client(c, now_ms())?;
                c.signed_post(&path, CREATE, &registration, &receiver.invitation.invite_id)
            })
            .await;
        // A lost create reply is reconciled against the same mailbox, not a
        // second invitation. It contains no candidate authority of its own.
        match result {
            Ok(value) => {
                h.with(|c| {
                    receiver.check_client(c, now_ms())?;
                    receiver.check_metadata(&value)
                })
                .await?
            }
            Err(Error::ExecutionUnknown) => {
                receiver.status(&h).await?;
            }
            Err(error) => return Err(error),
        }
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
        let token = format!("{PREFIX}{}", b64(&canonical(&self.invitation)));
        Ok(
            json!({ "id": self.id(), "qr": crate::qr::png_data(&token)?, "token": token,
            "expires_at": self.invitation.expires_at, "candidate": null }),
        )
    }

    fn check_metadata(&self, value: &Value) -> Result<()> {
        ensure(
            value["id"].as_str() == Some(self.id().as_str())
                && value["expires_at"].as_u64() == Some(self.invitation.expires_at),
            Error::IntegrityFailed,
        )
    }

    fn candidate(&self, c: &Client, value: &Value, now: u64) -> Result<Value> {
        self.check_client(c, now)?;
        self.check_metadata(value)?;
        let candidate = match value.get("sealed").ok_or(Error::IntegrityFailed)? {
            Value::Null => Value::Null,
            Value::String(text) => {
                ensure(text.len() <= MAX_SEALED * 4 / 3 + 4, Error::QuotaExceeded)?;
                let token = self.invitation.open(&unb64(text)?)?;
                let mut preview = c.pairing_preview_at(&token, now)?;
                preview["token"] = json!(token);
                preview
            }
            _ => return Err(Error::IntegrityFailed),
        };
        Ok(
            json!({"id": self.id(), "expires_at": self.invitation.expires_at, "candidate": candidate}),
        )
    }

    /// Read and decrypt the single candidate with fresh device authentication.
    /// # Errors
    /// Rejects expired or cancelled invitations, changed sessions and bad proofs.
    pub async fn status<H: Hold>(&self, h: &H) -> Result<Value> {
        let value = h
            .send(|c| {
                self.check_client(c, now_ms())?;
                c.signed_get(&self.invitation.path())
            })
            .await?;
        h.with(|c| self.candidate(c, &value, now_ms())).await
    }

    /// Cancel the relay mailbox after the host has already discarded it locally.
    /// A locked/offline host may fail to notify the relay; its deadline still
    /// removes it, and the discarded local key cannot process further requests.
    /// # Errors
    /// Returns session or relay errors; no new invitation is affected.
    pub async fn cancel<H: Hold>(&self, h: &H) -> Result<()> {
        let path = format!("{}/cancel", self.invitation.path());
        let payload = Cancellation {
            invite_id: self.invitation.invite_id,
        };
        let id = Hash::new(random_id());
        h.send(|c| {
            self.context(c)?;
            c.signed_post(&path, CANCEL, &payload, &id)
        })
        .await
        .map(|_| ())
    }
}

/// Return a signed device request via the configured relay after scanning an
/// invitation. A relay receipt never means the desktop approved the device.
/// # Errors
/// Refuses another account/relay, expired invitations and changed sessions.
pub async fn join<H: Hold>(h: &H, invitation: &str) -> Result<Value> {
    let invite = Invitation::parse(invitation, now_ms())?;
    let h = Session::new(h).await?;
    h.with(|c| {
        ensure(
            c.config.relay.trim_end_matches('/') == invite.relay,
            Error::IntegrityFailed,
        )
    })
    .await?;
    let request = account::pair_request(&h, ControllerRole::Administrator).await?;
    ensure(
        (request.home, request.account) == (invite.home, invite.account),
        Error::IntegrityFailed,
    )?;
    let nonce: [u8; 12] = random_id()[..12].try_into().unwrap();
    let sealed = invite.seal(&request.token, &nonce)?;
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
                &sealed,
            )
            .await;
        match reply {
            Err(Error::ExecutionUnknown) if attempt == 0 => continue,
            result => {
                let value = result?;
                return h.with(|_| {
                    invite.check(now_ms())?;
                    ensure(value["digest"].as_str() == Some(hex::encode(*sha256(&sealed)).as_str()), Error::IntegrityFailed)?;
                    Ok(json!({"device_id": hex::encode(*request.device_id), "expires_at": invite.expires_at.min(request.expires_at)}))
                }).await;
            }
        }
    }
    unreachable!("the last attempt returns")
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;
