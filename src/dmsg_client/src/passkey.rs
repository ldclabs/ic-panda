//! First-party login. Network work runs on Hold, outside the shared client.
use crate::{
    Client,
    cloud::Cloud,
    codec::{b64, canonical, decode},
    config::Config,
    crypto::{random_id, random_key},
    hold::Hold,
    ic::Ic,
    login::{Delegated, KeptLogin},
    now_ms,
    secure::HardwareAnchor,
};
use candid::Principal;
use dmsg_protocol::passkey::{MAX_PASSKEY_CALLBACK, hardware_der, hardware_public};
use dmsg_types::{auth::*, *};
use ic_agent::{
    Identity,
    agent::EnvelopeContent,
    identity::{BasicIdentity, Delegation, Signature, SignedDelegation},
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::sync::Arc;
use zeroize::Zeroizing;

/// First-party fragment protocol.
pub const PROTOCOL: &str = "dmsg-passkey/1";
/// Minimum remaining login lifetime for an online operation.
pub const MIN_VALIDITY: u64 = 10 * MINUTE;
const MARKER: &str = "passkey-login";

/// Stored only in the OS credential store, independently of the content LDK.
#[derive(Clone, Serialize, Deserialize)]
pub struct DeviceLogin {
    auth: Principal,
    handle: ByteBuf,
    public: ByteBuf,
    credential: ByteBuf,
    parent: Option<AuthDelegation>,
    targets: Vec<Principal>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Marker {
    auth: Principal,
    principal: Principal,
}

struct HardwareIdentity {
    key: DeviceLogin,
    hardware: Arc<dyn HardwareAnchor>,
}

/// Dependencies captured before leaving the client for interactive authentication.
pub struct PasskeySetup {
    config: Config,
    hardware: Arc<dyn HardwareAnchor>,
    previous: Option<DeviceLogin>,
    action: AuthAction,
}

impl PasskeySetup {
    /// Generate the hardware key and fetch the one-use challenge outside the client.
    /// # Errors
    /// Hardware, callback validation and service errors are preserved.
    pub async fn start(self, callback: String) -> Result<PasskeySession> {
        PasskeySession::start(
            self.config,
            self.hardware,
            self.action,
            callback,
            self.previous,
        )
        .await
    }
}

struct Renewal {
    config: Config,
    key: DeviceLogin,
    hardware: Arc<dyn HardwareAnchor>,
    device: Hash,
}

pub(crate) struct LogoutRequest {
    config: Config,
    key: DeviceLogin,
    hardware: Arc<dyn HardwareAnchor>,
}

impl Identity for HardwareIdentity {
    fn sender(&self) -> std::result::Result<Principal, String> {
        Ok(Principal::self_authenticating(&self.key.public))
    }

    fn public_key(&self) -> Option<Vec<u8>> {
        Some(self.key.public.to_vec())
    }

    fn sign(&self, content: &EnvelopeContent) -> std::result::Result<Signature, String> {
        self.sign_arbitrary(&content.to_request_id().signable())
    }

    fn sign_delegation(&self, content: &Delegation) -> std::result::Result<Signature, String> {
        self.sign_arbitrary(&content.signable())
    }

    fn sign_arbitrary(&self, message: &[u8]) -> std::result::Result<Signature, String> {
        let signed = self
            .hardware
            .sign_message(&self.key.handle, message)
            .map_err(|e| e.to_string())?;
        Ok(Signature {
            public_key: Some(self.key.public.to_vec()),
            signature: Some(signed.to_vec()),
            delegations: None,
        })
    }
}

impl DeviceLogin {
    fn new(auth: Principal, hardware: &dyn HardwareAnchor) -> Result<Self> {
        let (handle, public) = hardware.create()?;
        Ok(Self {
            auth,
            handle: handle.into(),
            public: hardware_der(&public)?.into(),
            credential: vec![].into(),
            parent: None,
            targets: vec![],
        })
    }

    fn principal(&self) -> Result<Principal> {
        Ok(Principal::self_authenticating(
            &self.parent.as_ref().ok_or(Error::AuthRequired)?.user_key,
        ))
    }

    fn identity(&self, hardware: Arc<dyn HardwareAnchor>) -> Arc<dyn Identity> {
        Arc::new(HardwareIdentity {
            key: self.clone(),
            hardware,
        })
    }

    fn derive(
        &self,
        hardware: &dyn HardwareAnchor,
        root_key: &[u8],
        targets: &[Principal],
        now: u64,
    ) -> Result<Delegated> {
        let parent = self.parent.as_ref().ok_or(Error::AuthRequired)?;
        let key = ic_auth_verifier::CanisterSigPublicKey::try_from(parent.user_key.as_slice())
            .map_err(|_| Error::IntegrityFailed)?;
        let expires_at = parent.expiration / NANOS_PER_MILLISECOND;
        ensure(
            key.canister_id == self.auth && parent.public_key == self.public,
            Error::IntegrityFailed,
        )?;
        ensure(
            expires_at > now && expires_at <= now.saturating_add(AUTH_TTL_MS + MINUTE),
            Error::Expired,
        )?;
        ensure(
            !targets.is_empty() && targets.len() <= crate::login::MAX_TARGETS,
            Error::IntegrityFailed,
        )?;
        let issued = Delegation {
            pubkey: self.public.to_vec(),
            expiration: parent.expiration,
            targets: None,
            permissions: None,
        };
        ic_auth_verifier::verify_canister_sig(
            &issued.signable(),
            &parent.signature,
            &parent.user_key,
            root_key,
            u128::from(now) * u128::from(NANOS_PER_MILLISECOND),
            Some(u128::from(AUTH_TTL_MS + MINUTE) * u128::from(NANOS_PER_MILLISECOND)),
        )
        .map_err(|_| Error::IntegrityFailed)?;
        let seed = Zeroizing::new(random_key());
        let ephemeral = BasicIdentity::from_raw_key(&seed);
        let scoped = Delegation {
            pubkey: ephemeral.public_key().ok_or(Error::IntegrityFailed)?,
            expiration: parent.expiration,
            targets: Some(targets.to_vec()),
            permissions: None,
        };
        let signature = hardware
            .sign_message(&self.handle, &scoped.signable())
            .map_err(key_error)?;
        let links = [
            SignedDelegation {
                delegation: issued,
                signature: parent.signature.to_vec(),
            },
            SignedDelegation {
                delegation: scoped,
                signature: signature.to_vec(),
            },
        ];
        let mut delegated =
            KeptLogin::new(parent.user_key.to_vec(), &seed, &links, expires_at).delegated()?;
        delegated.passkey = Some(self.clone());
        Ok(delegated)
    }
}

fn key_error(error: Error) -> Error {
    if error == Error::IntegrityFailed {
        Error::AuthRequired
    } else {
        error
    }
}

/// One interactive operation, retained only until its matching result arrives.
pub struct PasskeySession {
    config: Config,
    key: DeviceLogin,
    hardware: Arc<dyn HardwareAnchor>,
    request: PasskeyRequest,
    expected_credential: Option<ByteBuf>,
    admission: Option<dmsg_types::user::AdmissionTicket>,
    submitted: Option<Hash>,
    confirmed: bool,
    /// Exclusive callback deadline in milliseconds.
    pub deadline: u64,
}

impl PasskeySession {
    /// Prepare a server-issued challenge. The host supplies the fixed callback target.
    /// # Errors
    /// Refuses deployments or platforms without Passkey support and remote failures.
    async fn start(
        config: Config,
        hardware: Arc<dyn HardwareAnchor>,
        action: AuthAction,
        next_url: String,
        previous: Option<DeviceLogin>,
    ) -> Result<Self> {
        dmsg_protocol::login::callback_target(&next_url)?;
        let auth = config.auth.ok_or(Error::UnsupportedProtocol)?;
        let mut key = DeviceLogin::new(auth, hardware.as_ref())?;
        let expected_credential = if action == AuthAction::LogoutAll {
            previous.map(|key| key.credential)
        } else {
            None
        };
        let ic = Ic::connect(&config, key.identity(hardware.clone())).await?;
        // Finish registration preflight before asking the authenticator to save
        // a credential. A failed ticket request must not leave an orphan passkey.
        let admission = if action == AuthAction::Register {
            let stats: AuthStats = ic.query(auth, "auth_stats", ()).await?;
            ensure(
                stats.ready,
                Error::Unavailable("auth initialization".into()),
            )?;
            if stats.admission_required {
                Some(
                    Cloud::new(&config.relay, config.environment == Environment::Local)?
                        .admission(auth, ic.principal()?)
                        .await?,
                )
            } else {
                None
            }
        } else {
            None
        };
        // Discover deployment targets before the system saves a credential.
        // Finishing authentication must not depend on another configuration scan.
        key.targets = crate::account::login_targets(&config).await?;
        let challenge: Result<AuthChallenge> = ic
            .update(
                auth,
                "prepare_challenge",
                (ChallengeRequest {
                    public_key: key.public.clone(),
                    action,
                },),
            )
            .await?;
        let challenge = challenge?;
        let now = now_ms();
        dmsg_protocol::expiry(now, challenge.expires_at, CHALLENGE_TTL_MS + MINUTE)?;
        ensure(challenge.action == action, Error::IntegrityFailed)?;
        let deadline = admission.as_ref().map_or(challenge.expires_at, |ticket| {
            challenge.expires_at.min(ticket.expires_at)
        });
        ensure(now < deadline, Error::Expired)?;
        Ok(Self {
            config,
            key,
            hardware,
            deadline,
            admission,
            submitted: None,
            confirmed: false,
            expected_credential: expected_credential.clone(),
            request: PasskeyRequest {
                protocol: PROTOCOL.into(),
                state: Hash::new(random_id()),
                challenge,
                next_url,
                credential_id: expected_credential,
            },
        })
    }

    /// System-browser URL. No private key leaves the client.
    pub fn url(&self) -> String {
        format!(
            "{}#{}",
            self.config.login_url,
            b64(&canonical(&self.request))
        )
    }

    /// Challenge and creation user ID for a native authenticator.
    pub fn request(&self) -> &PasskeyRequest {
        &self.request
    }

    /// Opaque user handle for creating a credential.
    pub fn user_handle(&self) -> Vec<u8> {
        dmsg_protocol::sha256(&self.key.public).to_vec()
    }

    /// Restrict logout-all to the credential of the selected login.
    pub fn credential(&self) -> Option<Vec<u8>> {
        self.expected_credential.as_ref().map(|v| v.to_vec())
    }

    /// Validate the local callback before claiming it; network verification happens in complete.
    /// # Errors
    /// Foreign callbacks do not consume the pending operation.
    pub fn accept(&self, bytes: &[u8], now: u64) -> Result<PasskeyResponse> {
        ensure(now < self.deadline, Error::Expired)?;
        let result: PasskeyCallback = decode(bytes, MAX_PASSKEY_CALLBACK)?;
        ensure(
            result.protocol == PROTOCOL && result.state == self.request.state,
            Error::IntegrityFailed,
        )?;
        match (result.response, result.error) {
            (Some(response), None) => {
                if let Some(id) = &self.expected_credential {
                    ensure(*id == response.credential_id, Error::Forbidden)?;
                }
                Ok(response)
            }
            (None, Some(code))
                if ["user_cancelled", "passkey_error", "timeout"].contains(&code.as_str()) =>
            {
                Err(Error::Unavailable(format!("login: {code}")))
            }
            _ => Err(Error::IntegrityFailed),
        }
    }

    /// Finish authentication outside the client. None means logout-all succeeded.
    /// Retained attempts first retrieve the original grant; only an unconfirmed
    /// absent grant can redeliver the same proof and one-use challenge.
    /// # Errors
    /// Returns the service or verification error without extending its challenge.
    pub async fn complete(&mut self, response: PasskeyResponse) -> Result<Option<Delegated>> {
        ensure(now_ms() < self.deadline, Error::Expired)?;
        if let Some(id) = &self.expected_credential {
            ensure(*id == response.credential_id, Error::Forbidden)?;
        }
        let ic = Ic::connect(&self.config, self.key.identity(self.hardware.clone())).await?;
        let auth = self.key.auth;
        let challenge = self.request.challenge.challenge;
        let digest = dmsg_protocol::sha256(&canonical(&response));
        let retrying = self.submitted.is_some();
        let existing = if let Some(submitted) = self.submitted {
            ensure(submitted == digest, Error::IntegrityFailed)?;
            ensure(
                self.request.challenge.action != AuthAction::LogoutAll,
                Error::ExecutionUnknown,
            )?;
            let grant: Result<AuthDelegation> = ic
                .query(auth, "get_delegation", (self.key.public.clone(),))
                .await?;
            match grant {
                Ok(grant) => Some(grant),
                // A lost request may never have reached the issuer. Reuse its
                // exact proof and one-use challenge; never create another key.
                Err(Error::AuthRequired) if !self.confirmed => None,
                Err(error) => return Err(error),
            }
        } else {
            None
        };
        let mut rejection = None;
        if existing.is_none() {
            ensure(now_ms() < self.deadline, Error::Expired)?;
            self.key.credential = response.credential_id.clone();
            // The issuer atomically consumes the challenge and also prevents
            // registering a second identity for this caller. An identical
            // request racing its earlier delivery therefore has one effect.
            self.submitted = Some(digest);
            let result: Result<Result<u64>> = match self.request.challenge.action {
                AuthAction::Register => {
                    ic.update(
                        auth,
                        "register",
                        (RegisterPasskey {
                            challenge,
                            response,
                            admission: self.admission.clone(),
                        },),
                    )
                    .await
                }
                AuthAction::Login => {
                    ic.update(
                        auth,
                        "login",
                        (AuthenticatePasskey {
                            challenge,
                            response,
                        },),
                    )
                    .await
                }
                AuthAction::LogoutAll => {
                    let result: Result<()> = ic
                        .update(
                            auth,
                            "logout_all",
                            (AuthenticatePasskey {
                                challenge,
                                response,
                            },),
                        )
                        .await?;
                    result?;
                    return Ok(None);
                }
            };
            match result {
                Ok(Ok(_)) => self.confirmed = true,
                Ok(Err(error)) if retrying => rejection = Some(error),
                Ok(Err(error)) => return Err(error),
                Err(Error::ExecutionUnknown) => {}
                Err(error) => return Err(error),
            }
        }
        let parent: Result<AuthDelegation> = match existing {
            Some(grant) => Ok(grant),
            None => {
                ic.query(auth, "get_delegation", (self.key.public.clone(),))
                    .await?
            }
        };
        self.key.parent = Some(match (parent, rejection) {
            (Ok(grant), _) => grant,
            (Err(_), Some(error)) => return Err(error),
            (Err(Error::AuthRequired), None) if !self.confirmed => {
                return Err(Error::ExecutionUnknown);
            }
            (Err(error), None) => return Err(error),
        });
        self.confirmed = true;
        self.key
            .derive(
                self.hardware.as_ref(),
                &ic.root_key(),
                &self.key.targets,
                now_ms(),
            )
            .map(Some)
    }
}

impl Client {
    /// Whether a configured first-party login can use hardware on this host.
    pub fn passkey_available(&self) -> bool {
        self.config.auth.is_some()
            && self.secure().is_some()
            && self.anchor.is_some()
            && self.presence.as_ref().and_then(|p| p.presence()).is_some()
            && (self.workspace.provisional() || self.workspace.bound().unwrap_or(false))
    }

    /// Dependencies captured in a short step before an interactive login.
    /// # Errors
    /// Unsupported hardware or missing remembered credentials are refused.
    pub fn passkey_start(&mut self, action: AuthAction) -> Result<PasskeySetup> {
        ensure(self.passkey_available(), Error::UnsupportedProtocol)?;
        let previous = if action == AuthAction::LogoutAll {
            self.remembered_passkey()?
        } else {
            None
        };
        Ok(PasskeySetup {
            config: self.config.clone(),
            hardware: self.anchor.clone().ok_or(Error::UnsupportedProtocol)?,
            previous,
            action,
        })
    }

    fn passkey_slot(&self) -> Result<String> {
        Ok(format!("{}:passkey-login", self.workspace.name()?))
    }

    pub(crate) fn renewable(&self) -> bool {
        self.passkey.is_some()
            || self
                .workspace
                .store
                .meta::<Marker>(MARKER)
                .ok()
                .flatten()
                .is_some_and(|m| Some(m.auth) == self.config.auth)
    }

    pub(crate) fn remembered_principal(&self) -> Option<Principal> {
        self.passkey
            .as_ref()
            .and_then(|key| key.principal().ok())
            .or_else(|| {
                self.workspace
                    .store
                    .meta::<Marker>(MARKER)
                    .ok()
                    .flatten()
                    .filter(|m| Some(m.auth) == self.config.auth)
                    .map(|m| m.principal)
            })
    }

    pub(crate) fn remembered_passkey(&mut self) -> Result<Option<DeviceLogin>> {
        if let Some(key) = &self.passkey {
            return Ok(Some(key.clone()));
        }
        let Some(marker) = self.workspace.store.meta::<Marker>(MARKER)? else {
            return Ok(None);
        };
        ensure(
            self.workspace.bound()? && Some(marker.auth) == self.config.auth,
            Error::AuthRequired,
        )?;
        self.hardware_ready()?;
        let bytes = self
            .secure()
            .ok_or(Error::UnsupportedProtocol)?
            .get(&self.passkey_slot()?)?
            .ok_or(Error::AuthRequired)?;
        let key: DeviceLogin = decode(&bytes, 64 * 1024)?;
        ensure(
            key.auth == marker.auth && key.principal()? == marker.principal,
            Error::IntegrityFailed,
        )?;
        hardware_public(&key.public)?;
        Ok(Some(key))
    }

    pub(crate) fn remember_passkey(&self) -> Result<()> {
        let Some(key) = &self.passkey else {
            return Ok(());
        };
        if self.workspace.provisional() {
            return Ok(());
        }
        ensure(
            self.workspace.bound()? && self.passkey_available(),
            Error::Forbidden,
        )?;
        self.secure()
            .ok_or(Error::UnsupportedProtocol)?
            .set(&self.passkey_slot()?, &canonical(key))?;
        self.workspace.store.set_meta(
            MARKER,
            &Marker {
                auth: key.auth,
                principal: key.principal()?,
            },
        )
    }

    /// The remote end of a local logout, captured while the login is still remembered.
    pub(crate) fn passkey_logout(&mut self) -> Option<LogoutRequest> {
        let key = self.remembered_passkey().ok().flatten()?;
        Some(LogoutRequest {
            config: self.config.clone(),
            key,
            hardware: self.anchor.clone()?,
        })
    }

    fn login_valid(&self, min_validity: u64) -> bool {
        self.login
            .as_ref()
            .is_some_and(|l| l.expires_at > now_ms().saturating_add(min_validity))
    }

    pub(crate) fn forget_passkey(&mut self) -> Result<()> {
        // A marker left by another auth configuration still owns its slot.
        let remembered = self.passkey.is_some()
            || self
                .workspace
                .store
                .meta::<Marker>(MARKER)
                .ok()
                .flatten()
                .is_some();
        if remembered
            && !self.workspace.provisional()
            && let Some(secure) = self.secure()
        {
            secure.delete(&self.passkey_slot()?)?;
        }
        self.workspace.store.delete_meta(MARKER)?;
        self.passkey = None;
        Ok(())
    }
}

/// Obtain an online identity only for an operation that needs it. Concurrent
/// callers share one renewal gate; lock/account changes reject late results.
/// # Errors
/// Transient failures preserve the remembered grant; definitive revocation forgets it.
pub async fn ensure_login<H: Hold + ?Sized>(h: &H, min_validity: u64) -> Result<()> {
    // A current login never waits for the gate: whole-client operations call
    // this while holding the client a renewal needs to finish.
    let Some((gate, generation, sequence)) = h
        .with(|c| {
            Ok((!c.login_valid(min_validity)).then(|| {
                (
                    c.login_renewal.clone(),
                    c.session_generation,
                    c.login_renewal_sequence,
                )
            }))
        })
        .await?
    else {
        return Ok(());
    };
    let _turn = gate.lock().await;
    let prepared = h
        .with(|c| {
            ensure(c.session_generation == generation, Error::Locked)?;
            if c.login_valid(min_validity) {
                return Ok(None);
            }
            if c.login_renewal_sequence != sequence
                && let Some(error) = &c.login_renewal_failure
            {
                return Err(error.clone());
            }
            if !c.renewable() {
                c.session()?;
                return Ok(None);
            }
            let key = match c.remembered_passkey() {
                Ok(Some(key)) => key,
                Ok(None) | Err(Error::AuthRequired) => {
                    c.forget_passkey()?;
                    c.login = None;
                    return Err(Error::AuthRequired);
                }
                Err(error) => return Err(error),
            };
            let hardware = c.anchor.clone().ok_or(Error::UnsupportedProtocol)?;
            Ok(Some(Renewal {
                config: c.config.clone(),
                key,
                hardware,
                device: c.workspace.meta.device_id,
            }))
        })
        .await?;
    let Some(Renewal {
        config,
        mut key,
        hardware,
        device,
    }) = prepared
    else {
        return Ok(());
    };
    let expected = key.principal()?;
    let result = async {
        hardware
            .sign_message(&key.handle, b"dmsg/login-key-check/v1")
            .map_err(key_error)?;
        let ic = Ic::connect(&config, key.identity(hardware.clone())).await?;
        if key.parent.as_ref().is_none_or(|p| {
            p.expiration / NANOS_PER_MILLISECOND <= now_ms().saturating_add(min_validity)
        }) {
            let renewed: Result<Result<u64>> = ic.update(key.auth, "renew", ()).await;
            if matches!(renewed, Ok(Err(Error::AuthRequired))) {
                return Err(Error::AuthRequired);
            }
            let parent: Result<AuthDelegation> = match ic
                .query(key.auth, "get_delegation", (key.public.clone(),))
                .await
            {
                Ok(value) => value,
                Err(error) => return Err(renewed.err().unwrap_or(error)),
            };
            key.parent = Some(parent?);
            ensure(key.principal()? == expected, Error::IntegrityFailed)?;
            // A remembered device never logs in again, so each renewal keeps
            // the scope current; the previous one stays usable if this fails.
            if let Ok(targets) = crate::account::login_targets(&config).await {
                key.targets = targets;
            }
        }
        let delegated = key.derive(hardware.as_ref(), &ic.root_key(), &key.targets, now_ms())?;
        ensure(
            delegated.expires_at > now_ms().saturating_add(min_validity),
            Error::Expired,
        )?;
        let agent = Ic::connect(&config, delegated.identity.clone()).await?;
        Ok((delegated, agent))
    }
    .await;
    h.with(|c| {
        ensure(
            c.session_generation == generation && c.workspace.meta.device_id == device,
            Error::Locked,
        )?;
        c.login_renewal_sequence = c.login_renewal_sequence.wrapping_add(1);
        c.login_renewal_failure = result.as_ref().err().cloned();
        match result {
            Ok((login, ic)) => {
                c.passkey = login.passkey;
                c.login = Some(crate::client::Login {
                    ic,
                    principal: expected,
                    expires_at: login.expires_at,
                    kept: login.kept,
                });
                c.remember_passkey()
            }
            Err(Error::AuthRequired) => {
                c.forget_passkey()?;
                c.login = None;
                Err(Error::AuthRequired)
            }
            Err(error) => Err(error),
        }
    })
    .await
}

/// Local logout always completes independently of the optional remote attempt.
/// # Errors
/// Local storage errors are reported; network failure does not undo local logout.
pub async fn logout<H: Hold>(h: &H) -> Result<crate::Status> {
    let (status, remote) = h
        .with(|c| {
            let remote = c.passkey_logout();
            Ok((c.logout()?, remote))
        })
        .await?;
    if let Some(remote) = remote {
        remote.send().await;
    }
    Ok(status)
}

impl LogoutRequest {
    /// Best effort: the local logout stands whatever the service answers.
    pub(crate) async fn send(self) {
        let attempt = async {
            let ic = Ic::connect(&self.config, self.key.identity(self.hardware)).await?;
            let result: Result<()> = ic.update(self.key.auth, "logout", ()).await?;
            result
        };
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), attempt).await;
    }
}

#[cfg(test)]
#[path = "passkey_tests.rs"]
mod tests;
