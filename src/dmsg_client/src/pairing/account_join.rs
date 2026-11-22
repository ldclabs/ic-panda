//! A joining login accepts its own binding before signing the device request
//! at the resulting account version. Both replies use the original QR.
use super::*;
use crate::client::Slot;
use crate::codec::digest;
use crate::crypto::sign;
use crate::security::{AUTH_REQUEST, AuthPacket};
use dmsg_protocol::{DeviceInputExt, verify};
use dmsg_types::{Ed25519Signature, user::DeviceInput};

const DELIVERY: &str = "account-invitation";

#[derive(Serialize, Deserialize)]
struct LoginReply {
    token: String,
    device: DeviceInput,
    proof: Ed25519Signature,
}

impl LoginReply {
    fn message(&self, invite: &Invitation) -> Hash {
        digest(
            "dmsg/invited-login/v1",
            &(invite.invite_id, &self.token, &self.device),
        )
    }
}

pub(super) struct ReviewedLogin {
    token: String,
    pub device: DeviceInput,
    pub(super) principal: Principal,
    expires_at: u64,
    pub code: String,
}

impl ReviewedLogin {
    pub fn view(&self) -> Value {
        json!({"kind":"login_binding","token":self.token,"principal":self.principal.to_text(),
            "device_id":hex::encode(*self.device.device_id),"code":self.code,"expires_at":self.expires_at})
    }
}

pub(super) fn comparison(
    invite: &Invitation,
    principal: Option<Principal>,
    device: &DeviceInput,
) -> String {
    let hash = digest(
        "dmsg/join-comparison/v1",
        &(
            invite.home,
            invite.account,
            invite.invite_id,
            principal,
            device,
        ),
    );
    let code = hex::encode_upper(&hash.as_slice()[..4]);
    format!("{} {}", &code[..4], &code[4..])
}

pub(super) fn inspect_reply(
    c: &Client,
    invite: &Invitation,
    encoded: &str,
    now: u64,
) -> Result<ReviewedLogin> {
    let reply: LoginReply = decode(&unb64(encoded)?, 3072)?;
    let binding = c.inspect_binding_at(&reply.token, now)?;
    reply.device.validate()?;
    verify(
        &reply.device.signing_pub,
        reply.message(invite).as_slice(),
        reply.proof.as_slice(),
    )?;
    let code = comparison(invite, Some(binding.principal), &reply.device);
    Ok(ReviewedLogin {
        token: reply.token,
        device: reply.device,
        principal: binding.principal,
        expires_at: invite.expires_at.min(binding.expires_at),
        code,
    })
}

// Retain the exact ciphertext across retries. Each relay mailbox accepts only
// one distinct submission. The record moves with the login workspace at binding.
#[derive(Clone, Serialize, Deserialize)]
struct Delivery {
    invitation: String,
    principal: Principal,
    nonce: Option<Hash>,
    version: u64,
    #[serde(with = "serde_bytes")]
    sealed: Vec<u8>,
    code: String,
    sent: bool,
}

impl Delivery {
    fn load(c: &Client) -> Result<Option<Self>> {
        c.workspace.private(c.keys()?, DELIVERY)
    }

    fn save(&self, c: &Client) -> Result<()> {
        c.workspace.put_private(c.keys()?, DELIVERY, self)
    }

    fn stage(&self) -> &'static str {
        if self.nonce.is_some() {
            "waiting_login"
        } else {
            "waiting_device"
        }
    }
}

/// The unexpired invitation this login already agreed to continue.
/// # Errors
/// Returns a storage error if the encrypted workspace cannot be read.
pub fn saved_invitation(c: &Client) -> Result<Option<String>> {
    let (Ok(session), Ok(_)) = (c.session(), c.keys()) else {
        return Ok(None);
    };
    let Some(saved) = Delivery::load(c)? else {
        return Ok(None);
    };
    if Invitation::parse(&saved.invitation, now_ms()).is_err() || c.workspace.bound()? {
        c.workspace.delete_private(DELIVERY)?;
        return Ok(None);
    }
    Ok((saved.principal == session.principal).then_some(saved.invitation))
}

/// Preview the destination, or continue under the login explicitly confirmed by
/// the person. A preview never grants consent. Repeated continuations reuse the
/// exact reply and may only select the account belonging to the confirmed login.
/// # Errors
/// Rejects expired invitations, session changes, unknown homes and bad proofs.
pub async fn account_invitation<H: Hold>(
    h: &H,
    token: &str,
    consent: Option<&str>,
    bind_login: bool,
) -> Result<Value> {
    let invite = Invitation::parse(token, now_ms())?;
    let mut view = json!({"account":invite.account.to_string(),"expires_at":invite.expires_at,"stage":"sign_in"});
    let signed_in = h
        .with(|c| {
            ensure(
                c.config.relay.trim_end_matches('/') == invite.relay,
                Error::IntegrityFailed,
            )?;
            Ok(c.session().is_ok())
        })
        .await?;
    if !signed_in {
        ensure(consent.is_none(), Error::AuthRequired)?;
        return Ok(view);
    }
    let (generation, ic, principal, handle, pins, saved) = h
        .with(|c| {
            let login = c.session()?;
            ensure(
                consent.is_none_or(|s| s == login.principal.to_text()),
                Error::AuthRequired,
            )?;
            let saved = if c.keys().is_ok() {
                Delivery::load(c)?
                    .filter(|d| d.invitation == token && d.principal == login.principal)
            } else {
                None
            };
            Ok((
                c.session_generation,
                login.ic.clone(),
                login.principal,
                c.config.handle,
                c.config.user_homes.clone(),
                saved,
            ))
        })
        .await?;
    view["principal"] = json!(principal.to_text());
    let homes = account::user_homes(&ic, handle, &pins).await?;
    ensure(
        homes.user_homes.contains(&invite.home),
        Error::IntegrityFailed,
    )?;
    let found = account::find_account(&ic, &homes.user_homes).await?;
    if let Some((home, account)) = found
        && (home, account) != (invite.home, invite.account)
    {
        view["stage"] = json!("other_account");
        view["current_account"] = json!(account.to_string());
        return Ok(view);
    }
    let mut peers = h
        .with(|c| {
            ensure(
                c.session_generation == generation && c.session()?.principal == principal,
                Error::Locked,
            )?;
            Ok(c.peers_of(&[invite.account]))
        })
        .await?;
    let (_, security) = peers.read().await?.pop().ok_or(Error::NotFound)?;
    ensure(
        security.snapshot.home_user == invite.home,
        Error::IntegrityFailed,
    )?;
    let mut device = h
        .with(|c| {
            ensure(
                c.session_generation == generation
                    && c.session()?.principal == principal
                    && c.config.relay.trim_end_matches('/') == invite.relay,
                Error::Locked,
            )?;
            invite.check(now_ms())?;
            c.peers_met(peers);
            Ok(c.workspace.device_input(ControllerRole::Administrator))
        })
        .await?;
    let version = security.snapshot.account_version;
    let binding = found.is_none();
    let mailbox = if binding {
        let Some(slot) = invite.binding_slot() else {
            view["stage"] = json!("linked_login_required");
            return Ok(view);
        };
        slot
    } else {
        invite.clone()
    };

    view["device_id"] = json!(hex::encode(*device.device_id));
    let linked = binding || saved.as_ref().is_some_and(|d| d.nonce.is_some());
    let mut code = saved
        .as_ref()
        .map(|s| s.code.clone())
        .unwrap_or_else(|| comparison(&invite, linked.then_some(principal), &device));
    view["code"] = json!(code);
    let previous = saved.as_ref().filter(|d| d.nonce.is_some() == binding);
    if consent.is_none() || (binding && previous.is_none() && !bind_login) {
        view["stage"] = json!(if let Some(reply) = previous.filter(|reply| reply.sent) {
            reply.stage()
        } else if binding {
            "bind_login"
        } else if security.active(&device.device_id).is_ok() {
            "authorized"
        } else if linked {
            "waiting_login"
        } else {
            "device"
        });
        return Ok(view);
    }
    if !binding {
        // A new account on this installation inherits the provisional device.
        // An existing workspace uses its own keys and may already be authorized.
        let (actual, ready) = h
            .with(|c| {
                ensure(
                    c.session_generation == generation && c.session()?.principal == principal,
                    Error::Locked,
                )?;
                c.select(Slot::Account(invite.account))?;
                c.located = found;
                if c.workspace.provisional() && c.keys.is_none() {
                    c.keys = Some(c.workspace.unlock_provisional()?);
                }
                let actual = c.workspace.device_input(ControllerRole::Administrator);
                let ready = security.active(&actual.device_id).is_ok();
                Ok((actual, ready))
            })
            .await?;
        if ready {
            view["device_id"] = json!(hex::encode(*actual.device_id));
            view["stage"] = json!("authorized");
            return Ok(view);
        }
        if actual != device {
            // An earlier login reply committed to a different provisional device.
            ensure(!linked, Error::VersionConflict)?;
            device = actual;
            code = comparison(&invite, None, &device);
            view["device_id"] = json!(hex::encode(*device.device_id));
            view["code"] = json!(code);
        }
    }
    let session = Session::new(h).await?;
    session
        .with(|c| {
            ensure(
                c.session_generation == generation && c.session()?.principal == principal,
                Error::Locked,
            )
        })
        .await?;
    let reply = if let Some(reply) = previous {
        reply.clone()
    } else {
        let (plaintext, nonce) = if binding {
            let nonce = Hash::new(random_id());
            let packet = AuthPacket {
                format: AUTH_REQUEST.into(),
                home: invite.home,
                account: invite.account,
                principal,
                nonce,
                expires_at: invite.expires_at,
            };
            let mut reply = LoginReply {
                token: b64(&canonical(&packet)),
                device,
                proof: Ed25519Signature::new([0; 64]),
            };
            reply.proof = session
                .with(|c| {
                    Ok(Ed25519Signature::new(sign(
                        &c.keys()?.signing,
                        reply.message(&invite).as_slice(),
                    )))
                })
                .await?;
            (b64(&canonical(&reply)), Some(nonce))
        } else {
            let request = account::pair_request(&session, ControllerRole::Administrator).await?;
            ensure(
                (request.home, request.account) == (invite.home, invite.account),
                Error::IntegrityFailed,
            )?;
            (request.token, None)
        };
        let iv: [u8; 12] = random_id()[..12].try_into().unwrap();
        let draft = Delivery {
            invitation: token.into(),
            principal,
            nonce,
            version,
            sealed: mailbox.seal(&plaintext, &iv)?,
            code,
            sent: false,
        };
        session
            .with(|c| {
                // Two callbacks starting the same phase must submit identical bytes.
                if let Some(existing) = Delivery::load(c)?
                    && existing.invitation == token
                    && existing.principal == principal
                    && existing.nonce.is_some() == binding
                {
                    return Ok(existing);
                }
                draft.save(c)?;
                Ok(draft)
            })
            .await?
    };
    view["code"] = json!(reply.code);
    view["stage"] = json!(reply.stage());
    if !reply.sent {
        submit(&session, &mailbox, &reply.sealed).await?;
        session
            .with(|c| {
                if let Some(mut current) = Delivery::load(c)?
                    && current.sealed == reply.sealed
                {
                    current.sent = true;
                    current.save(c)?;
                }
                Ok(())
            })
            .await?;
    } else if let Some(nonce) = reply.nonce
        && version > reply.version
    {
        session
            .with(|c| {
                ensure(c.session()?.principal == principal, Error::Locked)?;
                invite.check(now_ms())
            })
            .await?;
        let accepted: Result<()> = ic
            .update(invite.home, "accept_auth_binding", (&invite.account, nonce))
            .await?;
        match accepted {
            Ok(()) | Err(Error::NotFound) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(view)
}
