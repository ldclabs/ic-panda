use candid::{CandidType, Principal};
use dmsg_types::{cose::*, stable::Table, user::Budget, *};
use ic_cdk::call::{CallErrorExt, CallFailed, RejectCode};
use ic_cdk_management_canister as mgmt;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::cell::RefCell;
mod model;

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct KeyState {
    pub config: CoseInit,
    pub initialization: Initialization,
    pub fingerprints: Vec<Hash>,
    pub error: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Config {
    schema: u16,
    state: KeyState,
    budget: Budget,
}
thread_local! {
    static CONFIG:RefCell<Table>=RefCell::new(Table::new(0));
    static HOMES:RefCell<Table>=RefCell::new(Table::new(1));
    static KEYS:RefCell<Table>=RefCell::new(Table::new(2));
}
fn cfg() -> Config {
    CONFIG.with_borrow(|t| t.get(b"config").expect("initialized"))
}
fn save_cfg(c: &Config) {
    CONFIG.with_borrow_mut(|t| t.put(b"config", c));
}
fn home(subject: &Hash) -> Result<model::Home> {
    HOMES.with_borrow(|t| t.get(subject.as_slice()).ok_or(Error::NotFound))
}
fn save_home(subject: &Hash, h: &model::Home) {
    HOMES.with_borrow_mut(|t| t.put(subject.as_slice(), h));
}
fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}
fn me() -> Principal {
    ic_cdk::api::canister_self()
}
fn caller() -> Principal {
    ic_cdk::api::msg_caller()
}
fn controller() -> Result<()> {
    ensure(ic_cdk::api::is_controller(&caller()), Error::Forbidden)
}
fn ready() -> Result<CoseInit> {
    let c = cfg();
    ensure(
        c.state.initialization == Initialization::Ready,
        Error::Unavailable("chain keys are not ready".into()),
    )?;
    Ok(c.state.config)
}
#[ic_cdk::init]
fn init(args: CoseInit) {
    args.validate(me())
        .expect("invalid chain-key configuration");
    save_cfg(&Config {
        schema: STABLE_SCHEMA,
        state: KeyState {
            config: args,
            initialization: Initialization::Uninitialized,
            fingerprints: vec![],
            error: None,
        },
        budget: Budget::default(),
    });
}
#[ic_cdk::post_upgrade]
fn post_upgrade(args: Option<CoseInit>) {
    let mut c = cfg();
    assert_eq!(
        c.schema, STABLE_SCHEMA,
        "explicit stable-state migration required"
    );
    if let Some(args) = args {
        assert_eq!(
            args, c.state.config,
            "key descriptions are immutable; explicit migration required"
        );
    }
    c.state
        .config
        .validate(me())
        .expect("invalid chain-key configuration");
    if c.state.initialization == Initialization::Initializing {
        c.state.initialization = Initialization::Uninitialized;
    }
    if c.state.initialization == Initialization::Ready {
        assert_eq!(c.state.fingerprints.len(), c.state.config.masters.len());
        for (k, fp) in c.state.config.masters.iter().zip(&c.state.fingerprints) {
            assert!(
                k.expected_fingerprint == Hash::new([0; 32]) || k.expected_fingerprint == *fp,
                "cached key fingerprint mismatch"
            );
        }
    }
    save_cfg(&c);
}
fn master(config: &CoseInit, alg: &Algorithm) -> Result<MasterKey> {
    config
        .masters
        .iter()
        .find(|m| &m.algorithm == alg)
        .cloned()
        .ok_or(Error::UnsupportedProtocol)
}
fn schnorr(k: &MasterKey) -> mgmt::SchnorrKeyId {
    mgmt::SchnorrKeyId {
        algorithm: if k.algorithm == Algorithm::Ed25519 {
            mgmt::SchnorrAlgorithm::Ed25519
        } else {
            mgmt::SchnorrAlgorithm::Bip340secp256k1
        },
        name: k.key_name.clone(),
    }
}
fn ecdsa(k: &MasterKey) -> mgmt::EcdsaKeyId {
    mgmt::EcdsaKeyId {
        curve: mgmt::EcdsaCurve::Secp256k1,
        name: k.key_name.clone(),
    }
}
fn vetkd(k: &MasterKey) -> mgmt::VetKDKeyId {
    mgmt::VetKDKeyId {
        curve: mgmt::VetKDCurve::Bls12_381_G2,
        name: k.key_name.clone(),
    }
}
async fn public_key(config: &CoseInit, k: &MasterKey, path: Vec<Vec<u8>>) -> Result<ByteBuf> {
    let bytes = match k.algorithm {
        Algorithm::Ed25519 | Algorithm::Bip340 => {
            mgmt::schnorr_public_key(&mgmt::SchnorrPublicKeyArgs {
                canister_id: None,
                derivation_path: path,
                key_id: schnorr(k),
            })
            .await
            .map(|r| r.public_key)
        }
        Algorithm::EcdsaSecp256k1 => mgmt::ecdsa_public_key(&mgmt::EcdsaPublicKeyArgs {
            canister_id: None,
            derivation_path: path,
            key_id: ecdsa(k),
        })
        .await
        .map(|r| r.public_key),
        Algorithm::VetKdBls12381 => mgmt::vetkd_public_key(&mgmt::VetKDPublicKeyArgs {
            canister_id: None,
            context: model::context(config),
            key_id: vetkd(k),
        })
        .await
        .map(|r| r.public_key),
    }
    .map_err(|_| Error::Unavailable("management public key".into()))?;
    Ok(bytes.into())
}
#[ic_cdk::update]
async fn initialize_keys() -> Result<KeyState> {
    controller()?;
    let mut c = cfg();
    if c.state.initialization == Initialization::Ready {
        return Ok(c.state);
    }
    ensure(
        c.state.initialization != Initialization::Initializing,
        Error::Pending,
    )?;
    c.state.initialization = Initialization::Initializing;
    c.state.error = None;
    save_cfg(&c);
    let mut fingerprints = vec![];
    for k in &c.state.config.masters {
        let fp = match public_key(&c.state.config, k, vec![]).await {
            Ok(key) => sha256(&key),
            Err(e) => {
                c.state.initialization = Initialization::Uninitialized;
                c.state.error = Some("public key initialization failed".into());
                save_cfg(&c);
                return Err(e);
            }
        };
        if k.expected_fingerprint != Hash::new([0; 32]) && fp != k.expected_fingerprint {
            c.state.initialization = Initialization::Uninitialized;
            c.state.error = Some("public key fingerprint mismatch".into());
            save_cfg(&c);
            return Err(Error::IntegrityFailed);
        }
        fingerprints.push(fp);
    }
    c.state.fingerprints = fingerprints;
    c.state.initialization = Initialization::Ready;
    save_cfg(&c);
    Ok(c.state)
}
#[ic_cdk::query]
fn key_state() -> KeyState {
    cfg().state
}
#[ic_cdk::update]
fn register_subject(subject: Hash) -> Result<()> {
    nonzero(&subject)?;
    ensure(
        caller() == cfg().state.config.initial_home_user,
        Error::Forbidden,
    )?;
    if let Ok(h) = home(&subject) {
        return ensure(h.home_user == caller(), Error::Forbidden);
    }
    ensure(
        HOMES.with_borrow(|t| t.len()) < 1_000_000,
        Error::QuotaExceeded,
    )?;
    save_home(&subject, &model::Home::new(caller()));
    Ok(())
}
async fn describe(config: &CoseInit, subject: Hash, key: KeyRequest) -> Result<KeyDescriptor> {
    key.validate()?;
    let id = model::key_id(config, subject, &key);
    if let Some(d) = KEYS.with_borrow(|t| t.get::<KeyDescriptor>(id.as_slice())) {
        return Ok(d);
    }
    let k = master(config, &key.algorithm)?;
    let public_key = public_key(config, &k, model::path(config, subject, &key)).await?;
    let d = KeyDescriptor {
        key_id: id,
        subject,
        purpose: key.purpose,
        algorithm: key.algorithm,
        home_cose: me(),
        master_key_name: k.key_name,
        environment: config.environment.clone(),
        derivation_version: config.derivation_version,
        key_generation: key.generation,
        provider: key.provider,
        public_key_fingerprint: sha256(&public_key),
        public_key,
    };
    if d.purpose != KeyPurpose::ContentRoot {
        KEYS.with_borrow_mut(|t| t.put(id.as_slice(), &d));
    }
    Ok(d)
}
#[ic_cdk::query]
fn describe_key(key_id: Hash) -> Result<KeyDescriptor> {
    KEYS.with_borrow(|t| t.get(key_id.as_slice()).ok_or(Error::NotFound))
}

enum ManagementCall {
    Schnorr(mgmt::SignWithSchnorrArgs),
    Ecdsa(mgmt::SignWithEcdsaArgs),
    VetKd(mgmt::VetKDDeriveKeyArgs),
}
fn management_call(config: &CoseInit, g: &ExecutionGrant) -> Result<(ManagementCall, u128)> {
    let call = match &g.kind {
        ExecutionKind::Sign {
            key,
            canonical_payload,
        } => {
            validate_payload(
                canonical_payload,
                g.subject,
                g.request_id,
                key,
                g.expires_at,
            )?;
            let k = master(config, &key.algorithm)?;
            let path = model::path(config, g.subject, key);
            if key.algorithm == Algorithm::EcdsaSecp256k1 {
                ManagementCall::Ecdsa(mgmt::SignWithEcdsaArgs {
                    message_hash: sha256(canonical_payload).to_vec(),
                    derivation_path: path,
                    key_id: ecdsa(&k),
                })
            } else {
                ManagementCall::Schnorr(mgmt::SignWithSchnorrArgs {
                    message: if key.algorithm == Algorithm::Bip340 {
                        sha256(canonical_payload).to_vec()
                    } else {
                        canonical_payload.to_vec()
                    },
                    derivation_path: path,
                    key_id: schnorr(&k),
                    aux: None,
                })
            }
        }
        ExecutionKind::Derive {
            generation,
            transport_key,
            ..
        } => {
            ensure(*generation > 0, invalid("vetKD generation"))?;
            validate_transport_key(transport_key)?;
            let k = master(config, &Algorithm::VetKdBls12381)?;
            ManagementCall::VetKd(mgmt::VetKDDeriveKeyArgs {
                input: model::root_input(g.subject, *generation),
                context: model::context(config),
                transport_public_key: transport_key.to_vec(),
                key_id: vetkd(&k),
            })
        }
    };
    let cost = match &call {
        ManagementCall::Schnorr(a) => mgmt::cost_sign_with_schnorr(a),
        ManagementCall::Ecdsa(a) => mgmt::cost_sign_with_ecdsa(a),
        ManagementCall::VetKd(a) => mgmt::cost_vetkd_derive_key(a),
    }
    .map_err(|_| Error::Unavailable("chain-key cost".into()))?;
    Ok((call, cost))
}
fn management_failure_status(error: &mgmt::SignCallError) -> ExecutionStatus {
    let known_failure = match error {
        mgmt::SignCallError::SignCostError(_) => true,
        mgmt::SignCallError::CallFailed(failure) => {
            failure.is_clean_reject()
                || matches!(failure,CallFailed::CallRejected(r) if r.raw_reject_code()==RejectCode::CanisterReject as u32)
        }
        mgmt::SignCallError::CandidDecodeFailed(_) => false,
    };
    // Management methods explicitly rejecting the request did not produce a
    // signature/key. Decode failures and ambiguous transport remain Unknown.
    if known_failure {
        ExecutionStatus::Failed
    } else {
        ExecutionStatus::Unknown
    }
}
#[ic_cdk::update]
async fn execute(grant: ExecutionGrant) -> Result<ExecutionResult> {
    let config = ready()?;
    let mut h = home(&grant.subject)?;
    ensure(
        grant.home_cose == me() && caller() == h.home_user && caller() == grant.home_user,
        Error::Forbidden,
    )?;
    // Dedup before any management call, including public-key retrieval.
    if let Some(e) = h
        .executions
        .values()
        .find(|e| e.grant.request_id == grant.request_id)
    {
        ensure(
            e.digest == digest("dmsg/cose-execution/v1", &grant),
            Error::IdempotencyConflict,
        )?;
        return Ok(e.result.clone());
    }
    let expired = now() >= grant.expires_at;
    let prepared_call = management_call(&config, &grant).and_then(|(call, cost)| {
        ensure(cost <= grant.max_cycles, Error::QuotaExceeded)?;
        Ok((call, cost))
    });
    let cost = prepared_call.as_ref().map_or(0, |(_, cost)| *cost);
    h.prepare(caller(), &grant, now(), if expired { 0 } else { cost })?;
    let mut c = cfg();
    c.budget.reserve(
        now(),
        if expired { 0 } else { cost },
        config.daily_executions,
        config.daily_cycles,
    )?;
    save_cfg(&c);
    save_home(&grant.subject, &h);
    let mut result = ExecutionResult {
        status: ExecutionStatus::Executing,
        result: None,
        key: None,
        charged_cycles: if expired { 0 } else { cost },
    };
    if expired {
        result.status = ExecutionStatus::ResultExpired;
    } else if let Ok((call, _)) = prepared_call {
        let key = match &grant.kind {
            ExecutionKind::Sign { key, .. } => key.clone(),
            ExecutionKind::Derive { generation, .. } => KeyRequest {
                purpose: KeyPurpose::ContentRoot,
                algorithm: Algorithm::VetKdBls12381,
                generation: *generation,
                provider: None,
            },
        };
        match describe(&config, grant.subject, key).await {
            Err(_) => {
                result.status = ExecutionStatus::Failed;
                result.charged_cycles = 0;
            }
            Ok(descriptor) => {
                result.key = Some(descriptor);
                // No management signing/derivation was sent before this await.
                if now() >= grant.expires_at {
                    result.status = ExecutionStatus::ResultExpired;
                    result.charged_cycles = 0;
                } else {
                    let response = match call {
                        ManagementCall::Schnorr(a) => {
                            mgmt::sign_with_schnorr(&a).await.map(|r| r.signature)
                        }
                        ManagementCall::Ecdsa(a) => {
                            mgmt::sign_with_ecdsa(&a).await.map(|r| r.signature)
                        }
                        ManagementCall::VetKd(a) => {
                            mgmt::vetkd_derive_key(&a).await.map(|r| r.encrypted_key)
                        }
                    };
                    result.charged_cycles = cost.saturating_sub(ic_cdk::api::msg_cycles_refunded());
                    match response {
                        Ok(bytes) => {
                            result.status = ExecutionStatus::Completed;
                            result.result = Some(bytes.into());
                        }
                        Err(error) => {
                            if matches!(
                                &error,
                                mgmt::SignCallError::SignCostError(_)
                                    | mgmt::SignCallError::CallFailed(
                                        CallFailed::InsufficientLiquidCycleBalance(_)
                                            | CallFailed::CallPerformFailed(_)
                                    )
                            ) {
                                result.charged_cycles = 0;
                            }
                            result.status = management_failure_status(&error);
                        }
                    }
                }
            }
        }
    } else {
        // A disabled algorithm or malformed grant must not leave a permanent
        // sequence hole that blocks unrelated later executions.
        result.status = ExecutionStatus::Failed;
    }
    let mut current = home(&grant.subject)?;
    current.finish(grant.execution_sequence, result.clone());
    save_home(&grant.subject, &current);
    Ok(result)
}
#[ic_cdk::query]
fn get_execution(subject: Hash, request_id: Hash) -> Result<ExecutionResult> {
    let h = home(&subject)?;
    ensure(caller() == h.home_user, Error::Forbidden)?;
    h.executions
        .values()
        .find(|e| e.grant.request_id == request_id)
        .map(|e| e.result.clone())
        .ok_or(Error::NotFound)
}
ic_cdk::export_candid!();

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distinguish_clean_rejection_from_unknown_outcome() {
        for code in [
            RejectCode::SysFatal,
            RejectCode::SysTransient,
            RejectCode::DestinationInvalid,
            RejectCode::CanisterReject,
        ] {
            let error = mgmt::SignCallError::CallFailed(CallFailed::CallRejected(
                ic_cdk::call::CallRejected::with_rejection(code as u32, "rejected".into()),
            ));
            assert_eq!(management_failure_status(&error), ExecutionStatus::Failed);
        }
        for code in [RejectCode::CanisterError, RejectCode::SysUnknown] {
            let error = mgmt::SignCallError::CallFailed(CallFailed::CallRejected(
                ic_cdk::call::CallRejected::with_rejection(code as u32, "ambiguous".into()),
            ));
            assert_eq!(management_failure_status(&error), ExecutionStatus::Unknown);
        }
        let unsent = mgmt::SignCallError::CallFailed(CallFailed::InsufficientLiquidCycleBalance(
            ic_cdk::call::InsufficientLiquidCycleBalance {
                available: 0,
                required: 1,
            },
        ));
        assert_eq!(management_failure_status(&unsent), ExecutionStatus::Failed);
    }
}
