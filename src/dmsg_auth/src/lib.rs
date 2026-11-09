//! First-party single-passkey identities and renewable hardware sessions.
mod state;
mod store;

use candid::Principal;
use dmsg_protocol::{authenticated, passkey::hardware_public, validate_ed25519_key};
use dmsg_runtime::admin::{self, Validation, validation};
use dmsg_types::{auth::*, *};
use ic_canister_sig_creation::{
    CanisterSigPublicKey, DELEGATION_SIG_DOMAIN, delegation_signature_msg,
    signature_map::{CanisterSigInputs, LABEL_SIG, SignatureMap},
};
use ic_certification::labeled_hash;
use std::{cell::RefCell, time::Duration};
use store::*;

thread_local! {
    static SIGNATURES: RefCell<SignatureMap> = RefCell::new(SignatureMap::default());
}

fn publish() {
    SIGNATURES
        .with_borrow(|s| ic_cdk::api::certified_data_set(labeled_hash(LABEL_SIG, &s.root_hash())));
}

fn initialize_salt() {
    ic_cdk_timers::set_timer(Duration::ZERO, async {
        let random = ic_cdk_management_canister::raw_rand().await;
        let mut cfg = config();
        if cfg.salt.is_some() {
            return;
        }
        match random.ok().and_then(|b| <[u8; 32]>::try_from(b).ok()) {
            Some(bytes) => {
                cfg.salt = Some(Hash::new(bytes));
                save_config(cfg);
            }
            None => {
                ic_cdk_timers::set_timer(Duration::from_secs(10), async {
                    initialize_salt();
                });
            }
        }
    });
}

#[ic_cdk::init]
fn init(input: AuthInit) {
    authenticated(input.governance).expect("governance");
    state::validate_limits(&input.limits).expect("limits");
    if let Some(key) = input.admission_key {
        validate_ed25519_key(key.as_slice()).expect("admission key");
    }
    save_config(Config {
        schema: 1,
        governance: input.governance,
        salt: None,
        nonce: 0,
        admission_key: input.admission_key,
        limits: input.limits.into(),
        day: 0,
        registered: 0,
    });
    publish();
    initialize_salt();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    assert_eq!(config().schema, 1, "auth stable schema");
    publish();
    if config().salt.is_none() {
        initialize_salt();
    }
}

#[ic_cdk::inspect_message]
fn inspect_message() {
    let caller = ic_cdk::api::msg_caller();
    let now = nanos_to_millis(ic_cdk::api::time());
    if ic_cdk::api::msg_arg_data().len() > 24 * 1024 {
        return;
    }
    let allowed = match ic_cdk::api::msg_method_name().as_str() {
        "renew" => session(caller).is_some_and(|s| now >= s.renewed_at.saturating_add(MINUTE)),
        "admin_set_limits" | "admin_set_admission_key" => {
            admin::check_admin(caller, config().governance).is_ok()
        }
        "prepare_challenge" | "register" | "login" | "logout" | "logout_all" => {
            authenticated(caller).is_ok()
        }
        _ => false,
    };
    if allowed {
        ic_cdk::api::accept_message();
    }
}

fn issue(session: Session) -> Result<u64> {
    let seed = state::seed(&config(), &session.identity)?;
    let message = delegation_signature_msg(&session.public, session.expiration, None);
    SIGNATURES.with_borrow_mut(|s| {
        s.add_signature(&CanisterSigInputs {
            domain: DELEGATION_SIG_DOMAIN,
            seed: seed.as_slice(),
            message: &message,
        })
    });
    publish();
    Ok(session.expiration)
}

#[ic_cdk::update]
fn prepare_challenge(input: ChallengeRequest) -> Result<AuthChallenge> {
    let caller = ic_cdk::api::msg_caller();
    let now = nanos_to_millis(ic_cdk::api::time());
    state::prepare(caller, ic_cdk::api::canister_self(), input, now)
}

#[ic_cdk::update]
fn register(input: RegisterPasskey) -> Result<u64> {
    let caller = ic_cdk::api::msg_caller();
    let now = nanos_to_millis(ic_cdk::api::time());
    issue(state::register(
        caller,
        ic_cdk::api::canister_self(),
        input,
        now,
    )?)
}

#[ic_cdk::update]
fn login(input: AuthenticatePasskey) -> Result<u64> {
    let caller = ic_cdk::api::msg_caller();
    let now = nanos_to_millis(ic_cdk::api::time());
    issue(state::login(caller, input, now)?)
}

#[ic_cdk::update]
fn renew() -> Result<u64> {
    let caller = ic_cdk::api::msg_caller();
    let now = nanos_to_millis(ic_cdk::api::time());
    issue(state::renew(caller, now)?)
}

#[ic_cdk::update]
fn logout() -> Result<()> {
    state::logout(ic_cdk::api::msg_caller())
}

#[ic_cdk::update]
fn logout_all(input: AuthenticatePasskey) -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    let now = nanos_to_millis(ic_cdk::api::time());
    state::logout_all(caller, input, now)
}

#[ic_cdk::query]
fn get_delegation(public_key: serde_bytes::ByteBuf) -> Result<AuthDelegation> {
    hardware_public(&public_key)?;
    let session =
        session(Principal::self_authenticating(&public_key)).ok_or(Error::AuthRequired)?;
    let now = ic_cdk::api::time();
    ensure(
        now < session.expiration && session.public == public_key,
        Error::Expired,
    )?;
    let seed = state::seed(&config(), &session.identity)?;
    let user_key = CanisterSigPublicKey::new(ic_cdk::api::canister_self(), seed.to_vec());
    let message = delegation_signature_msg(&public_key, session.expiration, None);
    let signature = SIGNATURES
        .with_borrow(|s| {
            s.get_signature_as_cbor(
                &CanisterSigInputs {
                    domain: DELEGATION_SIG_DOMAIN,
                    seed: seed.as_slice(),
                    message: &message,
                },
                None,
            )
        })
        .map_err(|_| Error::Unavailable("delegation signature expired; renew".into()))?;
    Ok(AuthDelegation {
        user_key: user_key.to_der().into(),
        public_key,
        expiration: session.expiration,
        signature: signature.into(),
    })
}

#[ic_cdk::query]
fn auth_stats() -> AuthStats {
    let cfg = config();
    AuthStats {
        identities: IDENTITIES.with_borrow(|m| m.len()),
        sessions: SESSIONS.with_borrow(|m| m.len()),
        challenges: TRANSIENT.with_borrow(|t| t.pending.len() as u64),
        ready: cfg.salt.is_some(),
        admission_required: cfg.admission_key.is_some(),
        limits: cfg.limits.into(),
        stable_pages: ic_cdk::api::stable_size(),
        cycles: ic_cdk::api::canister_cycle_balance(),
    }
}

#[ic_cdk::update]
fn admin_set_limits(limits: AuthLimits) -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    let mut cfg = config();
    admin::check_admin(caller, cfg.governance)?;
    state::validate_limits(&limits)?;
    cfg.limits = limits.into();
    save_config(cfg);
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_set_limits(limits: AuthLimits) -> Validation {
    validation(
        state::validate_limits(&limits).map(|()| format!("Set authentication budgets: {limits:?}")),
    )
}

#[ic_cdk::update]
fn admin_set_admission_key(key: Option<Hash>) -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    let mut cfg = config();
    admin::check_admin(caller, cfg.governance)?;
    if let Some(key) = key {
        validate_ed25519_key(key.as_slice())?;
    }
    cfg.admission_key = key;
    save_config(cfg);
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_set_admission_key(key: Option<Hash>) -> Validation {
    validation((|| {
        if let Some(key) = key {
            validate_ed25519_key(key.as_slice())?;
        }
        Ok(format!("Set auth registration admission key: {key:?}"))
    })())
}

ic_cdk::export_candid!();
