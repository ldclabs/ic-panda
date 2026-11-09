//! Synchronous auth transitions. Fallible checks precede stable writes.
use crate::store::*;
use candid::Principal;
use dmsg_protocol::{authenticated, canonical, digest, expiry, passkey::*, verify};
use dmsg_runtime::storage::Stored;
use dmsg_types::{auth::*, *};
use hmac::{KeyInit, Mac, SimpleHmac};
use sha2::Sha256;

pub(crate) fn validate_limits(l: &AuthLimits) -> Result<()> {
    ensure_valid(
        l.max_identities > 0
            && l.daily_registrations > 0
            && l.requests_per_minute > 0
            && l.renewals_per_minute > 0
            && (1..=100_000).contains(&l.max_challenges),
        "auth limits",
    )
}

fn rate(now: u64, limits: &AuthLimits, renewal: bool) -> Result<()> {
    TRANSIENT.with_borrow_mut(|t| {
        if t.minute != now / MINUTE {
            t.minute = now / MINUTE;
            t.requests = 0;
            t.renewals = 0;
        }
        let (used, limit) = if renewal {
            (&mut t.renewals, limits.renewals_per_minute)
        } else {
            (&mut t.requests, limits.requests_per_minute)
        };
        ensure(*used < limit, Error::QuotaExceeded)?;
        *used += 1;
        Ok(())
    })
}

pub(crate) fn prepare(
    caller: Principal,
    canister: Principal,
    input: ChallengeRequest,
    now: u64,
) -> Result<AuthChallenge> {
    authenticated(caller)?;
    hardware_public(&input.public_key)?;
    ensure(
        Principal::self_authenticating(&input.public_key) == caller,
        Error::Forbidden,
    )?;
    let mut cfg = config();
    let salt = cfg
        .salt
        .ok_or_else(|| Error::Unavailable("auth initialization".into()))?;
    let limits: AuthLimits = cfg.limits.clone().into();
    rate(now, &limits, false)?;
    TRANSIENT.with_borrow_mut(|t| {
        for _ in 0..50 {
            let Some(&(expires, who)) = t.expirations.first() else {
                break;
            };
            if expires > now {
                break;
            }
            t.expirations.remove(&(expires, who));
            t.pending.remove(&who);
        }
        ensure(
            t.pending.contains_key(&caller) || t.pending.len() < limits.max_challenges as usize,
            Error::QuotaExceeded,
        )
    })?;
    cfg.nonce = cfg.nonce.checked_add(1).ok_or(Error::QuotaExceeded)?;
    let expires_at = now.checked_add(CHALLENGE_TTL_MS).ok_or(Error::Expired)?;
    let mut mac = SimpleHmac::<Sha256>::new_from_slice(salt.as_slice()).expect("32-byte HMAC key");
    mac.update(&canonical(&(
        "dmsg/auth-challenge/v1",
        canister,
        caller,
        &input,
        cfg.nonce,
        expires_at,
    )));
    let challenge = AuthChallenge {
        challenge: Hash::new(mac.finalize().into_bytes().into()),
        expires_at,
        action: input.action,
    };
    let nonce = cfg.nonce;
    save_config(cfg);
    remove_pending(caller);
    TRANSIENT.with_borrow_mut(|t| {
        t.expirations.insert((expires_at, caller));
        t.pending.insert(
            caller,
            Pending {
                public: input.public_key,
                challenge: challenge.clone(),
                nonce,
            },
        );
    });
    Ok(challenge)
}

fn pending(caller: Principal, challenge: &Hash, action: AuthAction, now: u64) -> Result<Pending> {
    let cfg = config();
    rate(now, &cfg.limits.into(), false)?;
    let p = TRANSIENT
        .with_borrow(|t| t.pending.get(&caller).cloned())
        .ok_or(Error::AuthRequired)?;
    expiry(now, p.challenge.expires_at, CHALLENGE_TTL_MS)?;
    ensure(
        p.challenge.action == action && p.challenge.challenge == *challenge,
        Error::IntegrityFailed,
    )?;
    Ok(p)
}

fn remember(
    caller: Principal,
    id: Hash,
    mut identity: Identity,
    p: Pending,
    now: u64,
) -> Result<Session> {
    if let Some(previous) = session(caller) {
        ensure(previous.identity == id, Error::IdempotencyConflict)?;
    }
    if !identity.sessions.contains(&caller) {
        ensure(
            identity.sessions.len() < MAX_AUTH_SESSIONS,
            Error::SessionLimitExceeded,
        )?;
        identity.sessions.push(caller);
    }
    let session = Session {
        identity: id,
        renewed_at: now,
        expiration: millis_to_nanos(now.checked_add(AUTH_TTL_MS).ok_or(Error::Expired)?)?,
        public: p.public,
    };
    IDENTITIES.with_borrow_mut(|m| m.insert(*id, Stored(identity)));
    SESSIONS.with_borrow_mut(|m| m.insert(caller.as_slice().to_vec(), Stored(session.clone())));
    remove_pending(caller);
    Ok(session)
}

pub(crate) fn register(
    caller: Principal,
    canister: Principal,
    input: RegisterPasskey,
    now: u64,
) -> Result<Session> {
    let p = pending(caller, &input.challenge, AuthAction::Register, now)?;
    ensure(
        !REGISTRATIONS.with_borrow(|m| m.contains_key(&caller.as_slice().to_vec())),
        Error::IdempotencyConflict,
    )?;
    let public = verify_registration(&input.response, &input.challenge)?;
    let id = credential_hash(&input.response.credential_id)?;
    ensure(identity(&id).is_none(), Error::IdempotencyConflict)?;
    let mut cfg = config();
    let limits: AuthLimits = cfg.limits.clone().into();
    if let Some(key) = cfg.admission_key {
        let ticket = input.admission.ok_or(Error::Forbidden)?;
        expiry(now, ticket.expires_at, 10 * MINUTE)?;
        verify(
            &key,
            dmsg_protocol::account_admission_message(canister, caller, ticket.expires_at)
                .as_slice(),
            ticket.signature.as_slice(),
        )?;
    }
    ensure(
        IDENTITIES.with_borrow(|m| m.len()) < limits.max_identities,
        Error::QuotaExceeded,
    )?;
    if cfg.day != now / DAY {
        cfg.day = now / DAY;
        cfg.registered = 0;
    }
    ensure(
        cfg.registered < limits.daily_registrations,
        Error::QuotaExceeded,
    )?;
    let result = remember(
        caller,
        id,
        Identity {
            public: public.into(),
            sessions: vec![],
            logout_nonce: 0,
        },
        p,
        now,
    )?;
    cfg.registered += 1;
    REGISTRATIONS.with_borrow_mut(|m| m.insert(caller.as_slice().to_vec(), *id));
    save_config(cfg);
    Ok(result)
}

pub(crate) fn login(caller: Principal, input: AuthenticatePasskey, now: u64) -> Result<Session> {
    let p = pending(caller, &input.challenge, AuthAction::Login, now)?;
    let id = credential_hash(&input.response.credential_id)?;
    let identity = identity(&id).ok_or(Error::NotFound)?;
    ensure(p.nonce > identity.logout_nonce, Error::AuthRequired)?;
    verify_assertion(&input.response, &input.challenge, &identity.public)?;
    remember(caller, id, identity, p, now)
}

pub(crate) fn renew(caller: Principal, now: u64) -> Result<Session> {
    let mut session = session(caller).ok_or(Error::AuthRequired)?;
    ensure(
        now >= session.renewed_at.saturating_add(MINUTE),
        Error::QuotaExceeded,
    )?;
    let cfg = config();
    rate(now, &cfg.limits.into(), true)?;
    session.expiration = millis_to_nanos(now.checked_add(AUTH_TTL_MS).ok_or(Error::Expired)?)?;
    session.renewed_at = now;
    SESSIONS.with_borrow_mut(|m| m.insert(caller.as_slice().to_vec(), Stored(session.clone())));
    Ok(session)
}

pub(crate) fn logout(caller: Principal) -> Result<()> {
    if let Some(session) = session(caller) {
        let mut identity = identity(&session.identity).ok_or(Error::IntegrityFailed)?;
        identity.sessions.retain(|p| *p != caller);
        IDENTITIES.with_borrow_mut(|m| m.insert(*session.identity, Stored(identity)));
        SESSIONS.with_borrow_mut(|m| m.remove(&caller.as_slice().to_vec()));
    }
    remove_pending(caller);
    Ok(())
}

pub(crate) fn logout_all(caller: Principal, input: AuthenticatePasskey, now: u64) -> Result<()> {
    let p = pending(caller, &input.challenge, AuthAction::LogoutAll, now)?;
    let id = credential_hash(&input.response.credential_id)?;
    let mut identity = identity(&id).ok_or(Error::NotFound)?;
    ensure(p.nonce > identity.logout_nonce, Error::AuthRequired)?;
    verify_assertion(&input.response, &input.challenge, &identity.public)?;
    for who in &identity.sessions {
        SESSIONS.with_borrow_mut(|m| m.remove(&who.as_slice().to_vec()));
        remove_pending(*who);
    }
    identity.sessions.clear();
    // Also retire assertions prepared by devices that had not logged in yet.
    identity.logout_nonce = config().nonce;
    IDENTITIES.with_borrow_mut(|m| m.insert(*id, Stored(identity)));
    remove_pending(caller);
    Ok(())
}

pub(crate) fn seed(cfg: &Config, id: &Hash) -> Result<Hash> {
    let salt = cfg
        .salt
        .ok_or_else(|| Error::Unavailable("auth initialization".into()))?;
    Ok(digest("dmsg/auth-seed/v1", &(salt, id)))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
