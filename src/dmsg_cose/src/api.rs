use crate::{model, store::*};
use candid::Principal;
use dmsg_protocol::*;
use dmsg_runtime::Budget;
use dmsg_types::{cose::*, *};
use ic_cdk_management_canister as mgmt;
use ic_cose_chain_key::{self as chain_key, Cost, FailureKind, Operation, PublicKey};

fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}
fn me() -> Principal {
    ic_cdk::api::canister_self()
}
fn caller() -> Principal {
    ic_cdk::api::msg_caller()
}
fn ready() -> Result<Config> {
    let c = cfg();
    ensure(
        c.state.initialization == Initialization::Ready,
        Error::Unavailable("chain keys are not ready".into()),
    )?;
    Ok(c)
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
        keys: vec![],
        budget: Budget::default(),
    });
}
#[ic_cdk::post_upgrade]
fn post_upgrade(args: Option<CoseInit>) {
    let mut c = cfg();
    assert_eq!(c.schema, STABLE_SCHEMA, "incompatible development state");
    if let Some(args) = args {
        assert_eq!(args, c.state.config, "key descriptions are immutable");
    }
    c.state
        .config
        .validate(me())
        .expect("invalid chain-key configuration");
    if c.state.initialization == Initialization::Initializing {
        c.state.initialization = Initialization::Uninitialized;
    }
    if c.state.initialization == Initialization::Ready {
        assert_eq!(c.keys.len(), c.state.config.masters.len());
        assert_eq!(c.state.fingerprints.len(), c.keys.len());
        for ((master, key), fp) in c
            .state
            .config
            .masters
            .iter()
            .zip(&c.keys)
            .zip(&c.state.fingerprints)
        {
            assert_eq!(
                sha256(&key.public_key),
                *fp,
                "cached key fingerprint mismatch"
            );
            assert!(
                master.expected_fingerprint == Hash::new([0; 32])
                    || master.expected_fingerprint == *fp,
                "configured key fingerprint mismatch"
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
fn schnorr(alg: &Algorithm) -> mgmt::SchnorrAlgorithm {
    assert_eq!(*alg, Algorithm::Ed25519);
    mgmt::SchnorrAlgorithm::Ed25519
}
async fn fetch_master(config: &CoseInit, key: &MasterKey) -> Result<PublicKey> {
    match key.algorithm {
        Algorithm::Ed25519 => {
            chain_key::schnorr_public_key(key.key_name.clone(), schnorr(&key.algorithm), vec![])
                .await
        }
        Algorithm::EcdsaSecp256k1 => {
            chain_key::ecdsa_public_key(key.key_name.clone(), vec![]).await
        }
        Algorithm::VetKdBls12381 => {
            chain_key::vetkd_public_key(key.key_name.clone(), model::context(config))
                .await
                .map(|public_key| PublicKey {
                    public_key,
                    chain_code: vec![],
                })
        }
    }
    .map_err(Error::Unavailable)
}
#[ic_cdk::update]
async fn initialize_keys() -> Result<KeyState> {
    ensure(ic_cdk::api::is_controller(&caller()), Error::Forbidden)?;
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
    let mut keys = vec![];
    for master in &c.state.config.masters {
        let result = fetch_master(&c.state.config, master).await.and_then(|key| {
            ensure(
                master.expected_fingerprint == Hash::new([0; 32])
                    || master.expected_fingerprint == sha256(&key.public_key),
                Error::IntegrityFailed,
            )?;
            Ok(key)
        });
        match result {
            Ok(key) => keys.push(key),
            Err(e) => {
                c.state.initialization = Initialization::Uninitialized;
                c.state.error = Some(format!("{e:?}"));
                save_cfg(&c);
                return Err(e);
            }
        }
    }
    c.state.fingerprints = keys.iter().map(|k| sha256(&k.public_key)).collect();
    c.keys = keys;
    c.state.initialization = Initialization::Ready;
    save_cfg(&c);
    Ok(c.state)
}
#[ic_cdk::query]
fn key_state() -> KeyState {
    cfg().state
}

/// Public-key math is independent of account existence or execution permission.
/// No management call, registration or stable write is needed for this query.
#[ic_cdk::query]
fn public_key(account_id: AccountId, key: KeySelector) -> Result<KeyDescriptor> {
    describe(&ready()?, account_id, key.into())
}
fn describe(c: &Config, account_id: AccountId, key: KeyRequest) -> Result<KeyDescriptor> {
    nonzero(account_id.as_slice())?;
    key.validate()?;
    let config = &c.state.config;
    let index = config
        .masters
        .iter()
        .position(|m| m.algorithm == key.algorithm)
        .ok_or(Error::UnsupportedProtocol)?;
    let root = c
        .keys
        .get(index)
        .ok_or_else(|| Error::Unavailable("missing initialized key".into()))?;
    let path = model::path(config, account_id, &key);
    let public_key = match key.algorithm {
        Algorithm::Ed25519 => {
            chain_key::derive_schnorr_public_key(schnorr(&key.algorithm), root, path)
                .map(|p| p.public_key)
                .map_err(Error::Unavailable)?
        }
        Algorithm::EcdsaSecp256k1 => chain_key::derive_ecdsa_public_key(root, path)
            .map(|p| p.public_key)
            .map_err(Error::Unavailable)?,
        Algorithm::VetKdBls12381 => root.public_key.clone(),
    };
    let public_key_fingerprint = if key.algorithm == Algorithm::VetKdBls12381 {
        sha256(&public_key)
    } else {
        key_thumbprint(&public_cose_key(&key.algorithm, &[], &public_key)?)?
    };
    let key_id = if key.algorithm == Algorithm::VetKdBls12381 {
        model::key_id(config, account_id, &key).to_vec().into()
    } else {
        public_key_fingerprint.to_vec().into()
    };
    Ok(KeyDescriptor {
        key_id,
        account_id,
        purpose: key.purpose,
        algorithm: key.algorithm,
        home_cose: me(),
        master_key_name: config.masters[index].key_name.clone(),
        environment: config.environment.clone(),
        derivation_version: config.derivation_version,
        key_generation: key.generation,

        public_key_fingerprint,
        public_key: public_key.into(),
    })
}
fn prepare(c: &Config, g: &ExecutionGrant) -> Result<(Operation, Cost)> {
    let config = &c.state.config;
    let operation = match &g.kind {
        ExecutionKind::Sign {
            key,
            to_be_signed,
            public_key_fingerprint,
            origin,
        } => {
            key.validate()?;
            validate_origin(origin)?;
            let prepared = parse_signing_input(to_be_signed)?;
            ensure(
                prepared.algorithm == key.algorithm
                    && statement_purpose(&prepared.statement) == key.purpose,
                Error::UnsupportedProtocol,
            )?;
            ensure(
                prepared.statement.issuer
                    == account_issuer(&config.issuer_namespace, g.account_id)?,
                Error::IntegrityFailed,
            )?;
            let descriptor = describe(c, g.account_id, key.clone())?;
            ensure(
                descriptor.key_id.as_slice() == prepared.kid
                    && descriptor.public_key_fingerprint == *public_key_fingerprint,
                Error::IntegrityFailed,
            )?;
            let master = master(config, &key.algorithm)?;
            let path = model::path(config, g.account_id, key);
            match key.algorithm {
                Algorithm::Ed25519 => Operation::schnorr(
                    master.key_name,
                    schnorr(&key.algorithm),
                    path,
                    to_be_signed.to_vec(),
                ),
                Algorithm::EcdsaSecp256k1 => {
                    Operation::ecdsa(master.key_name, path, sha256(to_be_signed).into_array())
                }
                _ => return Err(Error::UnsupportedProtocol),
            }
        }
        ExecutionKind::Derive {
            generation,
            transport_key,
            ..
        } => {
            ensure(*generation > 0, invalid("vetKD generation"))?;
            validate_transport_key(transport_key)?;
            let master = master(config, &Algorithm::VetKdBls12381)?;
            Operation::vetkd(
                master.key_name,
                model::context(config),
                model::root_input(g.account_id, *generation),
                transport_key.to_vec(),
            )
        }
    };
    let cost = operation.cost().map_err(Error::Unavailable)?;
    ensure(
        cost.total().map_err(Error::Unavailable)? <= g.max_cycles,
        Error::QuotaExceeded,
    )?;
    Ok((operation, cost))
}
#[ic_cdk::update]
async fn execute(grant: ExecutionGrant) -> Result<ExecutionResult> {
    let c = ready()?;
    let config = &c.state.config;
    nonzero(grant.account_id.as_slice())?;
    ensure(
        grant.home_cose == me()
            && caller() == config.initial_home_user
            && caller() == grant.home_user,
        Error::Forbidden,
    )?;
    let mut h = match home(&grant.account_id) {
        Ok(h) => h,
        Err(Error::NotFound) => {
            ensure(
                HOMES.with_borrow(|t| t.len()) < 1_000_000,
                Error::QuotaExceeded,
            )?;
            model::Home::new(caller())
        }
        Err(e) => return Err(e),
    };
    if let Some(e) = h
        .executions
        .values()
        .find(|e| e.grant.request_id == grant.request_id)
    {
        ensure(
            e.digest == digest("dmsg/cose-execution/v2", &grant),
            Error::IdempotencyConflict,
        )?;
        return Ok(e.result.clone());
    }
    let expired = now() >= grant.expires_at;
    let prepared = prepare(&c, &grant);
    let reserved = if expired {
        0
    } else {
        prepared
            .as_ref()
            .map_or(0, |(_, cost)| cost.total().expect("validated cost"))
    };
    h.prepare(caller(), &grant, now(), reserved)?;
    let mut current_config = cfg();
    current_config.budget.reserve(
        now(),
        reserved,
        config.daily_executions,
        config.daily_cycles,
    )?;
    save_cfg(&current_config);
    save_home(&grant.account_id, &h);
    let mut result = ExecutionResult {
        request_id: grant.request_id,
        charged_cycles: 0,
        outcome: ExecutionOutcome::Executing,
    };
    result.outcome = if expired {
        ExecutionOutcome::ResultExpired
    } else {
        match prepared {
            Err(e) => ExecutionOutcome::Failed(e),
            Ok((operation, cost)) => {
                let key = match &grant.kind {
                    ExecutionKind::Sign { key, .. } => key.clone(),
                    ExecutionKind::Derive { generation, .. } => KeySelector::ContentRoot {
                        generation: *generation,
                    }
                    .into(),
                };
                match describe(&c, grant.account_id, key) {
                    Err(e) => ExecutionOutcome::Failed(e),
                    Ok(key) => {
                        let response = operation.execute().await;
                        result.charged_cycles = chain_key::charged_cycles(
                            cost,
                            response.as_ref().err(),
                            ic_cdk::api::msg_cycles_refunded(),
                        );
                        match response {
                            Ok(bytes) => match &grant.kind {
                                ExecutionKind::Sign { to_be_signed, .. } => {
                                    match finish_cose(to_be_signed, &key.public_key, bytes) {
                                        Ok(artifact) => ExecutionOutcome::Completed(Box::new(
                                            ExecutionOutput::Signature { artifact, key },
                                        )),
                                        // The management call has already run. Never retry it
                                        // because its response could not be packaged.
                                        Err(error) => ExecutionOutcome::Unknown(error),
                                    }
                                }
                                ExecutionKind::Derive { .. } => ExecutionOutcome::Completed(
                                    Box::new(ExecutionOutput::EncryptedRootKey {
                                        encrypted_key: bytes.into(),
                                        key,
                                    }),
                                ),
                            },
                            Err(e) => {
                                let detail = Error::Unavailable(format!("{e:?}"));
                                if chain_key::classify_failure(&e) == FailureKind::Unknown {
                                    ExecutionOutcome::Unknown(detail)
                                } else {
                                    ExecutionOutcome::Failed(detail)
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    let mut current = home(&grant.account_id)?;
    current.finish(grant.execution_sequence, result.clone());
    save_home(&grant.account_id, &current);
    Ok(result)
}
#[ic_cdk::query]
fn get_execution(account_id: AccountId, request_id: Hash) -> Result<ExecutionResult> {
    ensure(
        caller() == cfg().state.config.initial_home_user,
        Error::Forbidden,
    )?;
    let h = home(&account_id)?;
    h.executions
        .values()
        .find(|e| e.grant.request_id == request_id)
        .map(|e| e.result.clone())
        .ok_or(Error::NotFound)
}
