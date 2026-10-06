use crate::{model, store::*};
use candid::Principal;
use dmsg_protocol::{agent::*, *};
use dmsg_runtime::admin::{self, hex, validation, Validation};
use dmsg_types::{cose::*, *};
use ic_cdk_management_canister as mgmt;
use ic_cose_chain_key::{self as chain_key, Cost, FailureKind, Operation, PublicKey};

fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}

fn ready() -> Result<Config> {
    let c = cfg();
    ensure(
        c.state.initialization == Initialization::Ready,
        Error::Unavailable("chain keys are not ready".into()),
    )?;
    Ok(c)
}

fn check_admin(caller: Principal) -> Result<()> {
    admin::check_admin(caller, cfg().state.config.governance)
}

/// Only the user home that allocated an account may execute or read for it.
fn check_home(config: &CoseInit, caller: Principal, account_id: &AccountId) -> Result<()> {
    ensure(
        is_account_home(
            &config.environment,
            &config.issuer_namespace,
            &config.user_homes,
            caller,
            account_id,
        ),
        Error::Forbidden,
    )
}

/// A zero pin is accepted only where validation allows it, outside Production.
fn pinned(master: &MasterKey, fingerprint: &Hash) -> bool {
    master.expected_fingerprint == Hash::new([0; 32]) || master.expected_fingerprint == *fingerprint
}

#[ic_cdk::init]
fn init(args: CoseInit) {
    args.validate(ic_cdk::api::canister_self())
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
    });
}

/// Configuration changes go through administrative methods, never upgrades.
#[ic_cdk::post_upgrade]
fn post_upgrade() {
    let mut c = cfg();
    assert_eq!(c.schema, STABLE_SCHEMA, "incompatible development state");
    c.state
        .config
        .validate(ic_cdk::api::canister_self())
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
            assert!(pinned(master, fp), "configured key fingerprint mismatch");
        }
        cache_signing_roots(&c.state.config, &c.keys).expect("valid signing prefixes");
    }
    save_cfg(&c);
}

async fn fetch_master(config: &CoseInit, key: &MasterKey) -> Result<PublicKey> {
    match key.algorithm {
        Algorithm::Ed25519 => {
            chain_key::schnorr_public_key(
                key.key_name.clone(),
                mgmt::SchnorrAlgorithm::Ed25519,
                vec![],
            )
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

/// Fetch every configured master key in order and check it against its pin.
async fn fetch_masters(config: &CoseInit) -> Result<Vec<PublicKey>> {
    let mut keys = Vec::with_capacity(config.masters.len());
    for master in &config.masters {
        let key = fetch_master(config, master).await?;
        ensure(
            pinned(master, &sha256(&key.public_key)),
            Error::IntegrityFailed,
        )?;
        keys.push(key);
    }
    Ok(keys)
}

// Returns false when the keys are already ready.
fn check_initialize(c: &Config) -> Result<bool> {
    ensure(
        c.state.initialization != Initialization::Initializing,
        Error::Pending,
    )?;
    Ok(c.state.initialization != Initialization::Ready)
}

/// Fetch every configured master public key and check it against its pin.
#[ic_cdk::update]
async fn initialize_keys() -> Result<KeyState> {
    check_admin(ic_cdk::api::msg_caller())?;
    let mut c = cfg();
    if !check_initialize(&c)? {
        return Ok(c.state);
    }
    c.state.initialization = Initialization::Initializing;
    c.state.error = None;
    save_cfg(&c);
    let fetched = fetch_masters(&c.state.config).await;
    // Commit onto the current record, not the snapshot taken before the calls.
    let mut c = cfg();
    let fetched = fetched.and_then(|keys| {
        cache_signing_roots(&c.state.config, &keys)?;
        Ok(keys)
    });
    let result = match fetched {
        Ok(keys) => {
            c.state.fingerprints = keys.iter().map(|k| sha256(&k.public_key)).collect();
            c.keys = keys;
            c.state.initialization = Initialization::Ready;
            Ok(())
        }
        Err(e) => {
            c.state.initialization = Initialization::Uninitialized;
            c.state.error = Some(format!("{e:?}"));
            Err(e)
        }
    };
    save_cfg(&c);
    result.map(|()| c.state)
}

#[ic_cdk::query]
fn validate_initialize_keys() -> Validation {
    let c = cfg();
    validation(check_initialize(&c).map(|fresh| {
        let masters: Vec<String> = c
            .state
            .config
            .masters
            .iter()
            .map(|m| {
                format!(
                    "{:?} {} pinned to {}",
                    m.algorithm,
                    m.key_name,
                    hex(m.expected_fingerprint.as_slice()),
                )
            })
            .collect();
        format!(
            "Initialize {:?} chain keys: {}.{}",
            c.state.config.environment,
            masters.join("; "),
            admin::unchanged(fresh, "Already ready"),
        )
    }))
}

/// Append a user home. Its `dmsg_user` must name this canister as
/// `home_cose` and share the environment and issuer namespace.
#[ic_cdk::update]
fn admin_add_user_home(home: Principal) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    let mut c = cfg();
    let config = &mut c.state.config;
    if check_user_home(
        &config.environment,
        &config.issuer_namespace,
        &config.user_homes,
        home,
    )? {
        config.user_homes.push(home);
        save_cfg(&c);
    }
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_add_user_home(home: Principal) -> Validation {
    let config = cfg().state.config;
    validation(
        check_user_home(
            &config.environment,
            &config.issuer_namespace,
            &config.user_homes,
            home,
        )
        .map(|fresh| {
            admin::user_home_payload(&config.environment, &config.issuer_namespace, home, fresh)
        }),
    )
}

fn check_budget(daily_executions: u32, daily_cycles: u128) -> Result<()> {
    ensure_valid(daily_executions > 0 && daily_cycles > 0, "hard budgets")
}

/// Set the global daily limits. Counts already used today are retained.
#[ic_cdk::update]
fn admin_set_daily_budget(daily_executions: u32, daily_cycles: u128) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    check_budget(daily_executions, daily_cycles)?;
    let mut c = cfg();
    c.state.config.daily_executions = daily_executions;
    c.state.config.daily_cycles = daily_cycles;
    save_cfg(&c);
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_set_daily_budget(daily_executions: u32, daily_cycles: u128) -> Validation {
    let config = cfg().state.config;
    validation(check_budget(daily_executions, daily_cycles).map(|()| {
        format!(
            "Set the COSE daily budget to {daily_executions} executions and {daily_cycles} cycles (from {} and {}).{}",
            config.daily_executions,
            config.daily_cycles,
            admin::unchanged(
                (config.daily_executions, config.daily_cycles) != (daily_executions, daily_cycles),
                "Same budget",
            ),
        )
    }))
}

#[ic_cdk::query]
fn key_state() -> KeyState {
    cfg().state
}

/// Public-key math is independent of account existence or execution permission.
/// No management call, registration or stable write is needed for this query.
#[ic_cdk::query]
fn public_key(account_id: AccountId, key: KeySelector) -> Result<KeyDescriptor> {
    describe(
        &ready()?,
        &account_id,
        &key.into(),
        ic_cdk::api::canister_self(),
    )
}

fn describe(
    c: &Config,
    account_id: &AccountId,
    key: &KeyRequest,
    canister_id: Principal,
) -> Result<KeyDescriptor> {
    nonzero(account_id.as_slice())?;
    key.validate()?;
    let config = &c.state.config;
    let index = config
        .masters
        .iter()
        .position(|m| m.algorithm == key.algorithm)
        .ok_or(Error::UnsupportedProtocol)?;
    let public_key = match key.algorithm {
        Algorithm::Ed25519 => chain_key::derive_schnorr_public_key(
            mgmt::SchnorrAlgorithm::Ed25519,
            &signing_root(index)?,
            model::signing_suffix(account_id, key),
        )
        .map(|p| p.public_key)
        .map_err(Error::Unavailable)?,
        Algorithm::EcdsaSecp256k1 => chain_key::derive_ecdsa_public_key(
            &signing_root(index)?,
            model::signing_suffix(account_id, key),
        )
        .map(|p| p.public_key)
        .map_err(Error::Unavailable)?,
        Algorithm::VetKdBls12381 => c
            .keys
            .get(index)
            .ok_or_else(|| Error::Unavailable("missing initialized key".into()))?
            .public_key
            .clone(),
    };
    let public_key_fingerprint = if key.algorithm == Algorithm::VetKdBls12381 {
        sha256(&public_key)
    } else {
        key_thumbprint(&public_cose_key(&key.algorithm, &[], &public_key)?)?
    };
    let key_id = if key.algorithm == Algorithm::VetKdBls12381 {
        model::key_id(config, account_id, key).to_vec().into()
    } else {
        public_key_fingerprint.to_vec().into()
    };
    Ok(KeyDescriptor {
        key_id,
        account_id: *account_id,
        purpose: key.purpose.clone(),
        algorithm: key.algorithm.clone(),
        home_cose: canister_id,
        master_key_name: config.masters[index].key_name.clone(),
        environment: config.environment.clone(),
        derivation_version: config.derivation_version,
        key_generation: key.generation,

        public_key_fingerprint,
        public_key: public_key.into(),
    })
}

/// How a successful management response becomes the execution output.
enum Finish {
    Document(PreparedSignature),
    RootKey,
    AgentEvent(Hash),
}

struct PreparedExecution {
    operation: Operation,
    cost: Cost,
    /// Complete cost bound reserved from the per-account and global budgets.
    reserved: u128,
    key: KeyDescriptor,
    finish: Finish,
}

/// Validate a grant and build its management call. Called only before
/// `g.expires_at`; `describe` is the one place that validates the key request.
fn prepare(c: &Config, g: &ExecutionGrant, canister_id: Principal) -> Result<PreparedExecution> {
    let config = &c.state.config;
    match (&g.kind, &g.commerce) {
        // Callers are before the grant deadline, so outlasting it means valid now.
        (ExecutionKind::Sign { .. } | ExecutionKind::AgentEvent { .. }, Some(r)) => ensure(
            r.reservation_id == g.request_id
                && r.units > 0
                && r.weight_policy_version > 0
                && r.valid_until_ms >= g.expires_at,
            Error::MembershipStale,
        )?,
        (ExecutionKind::Derive { .. }, None) => {}
        _ => return Err(Error::IntegrityFailed),
    }
    let (operation, key, finish) = match &g.kind {
        ExecutionKind::Sign {
            key,
            to_be_signed,
            public_key_fingerprint,
            origin,
        } => {
            validate_origin(origin, &config.environment)?;
            let prepared = parse_signing_input(to_be_signed)?;
            ensure(
                *prepared.algorithm() == key.algorithm
                    && statement_purpose(prepared.statement()) == key.purpose,
                Error::UnsupportedProtocol,
            )?;
            ensure(
                prepared.statement().issuer
                    == account_issuer(&config.issuer_namespace, &g.account_id),
                Error::IntegrityFailed,
            )?;
            let descriptor = describe(c, &g.account_id, key, canister_id)?;
            ensure(
                descriptor.key_id.as_slice() == prepared.kid()
                    && descriptor.public_key_fingerprint == *public_key_fingerprint,
                Error::IntegrityFailed,
            )?;
            let path = model::path(config, &g.account_id, key);
            let operation = match key.algorithm {
                Algorithm::Ed25519 => Operation::schnorr(
                    descriptor.master_key_name.clone(),
                    mgmt::SchnorrAlgorithm::Ed25519,
                    path,
                    to_be_signed.to_vec(),
                ),
                Algorithm::EcdsaSecp256k1 => Operation::ecdsa(
                    descriptor.master_key_name.clone(),
                    path,
                    sha256(to_be_signed).into_array(),
                ),
                _ => return Err(Error::UnsupportedProtocol),
            };
            let finish = Finish::Document(prepared.into_signature(&descriptor.public_key)?);
            (operation, descriptor, finish)
        }
        ExecutionKind::AgentEvent {
            key,
            event,
            principal_id,
            origin,
        } => {
            validate_origin(origin, &config.environment)?;
            ensure(
                key.purpose == KeyPurpose::AgentController,
                Error::UnsupportedProtocol,
            )?;
            // The user home checked the controller binding and policy; COSE
            // independently refuses to sign anything but this key's own
            // delegation event for the named principal.
            let parsed = dmsg_protocol::agent::parse_delegation_event(event)?;
            let descriptor = describe(c, &g.account_id, key, canister_id)?;
            ensure(
                parsed.actor.as_slice() == descriptor.public_key.as_slice()
                    && parsed.principal_id == *principal_id,
                Error::IntegrityFailed,
            )?;
            let operation = Operation::schnorr(
                descriptor.master_key_name.clone(),
                mgmt::SchnorrAlgorithm::Ed25519,
                model::path(config, &g.account_id, key),
                parsed.hash.to_vec(),
            );
            (operation, descriptor, Finish::AgentEvent(parsed.hash))
        }
        ExecutionKind::Derive {
            generation,
            transport_key,
            ..
        } => {
            validate_transport_key(transport_key)?;
            let descriptor = describe(
                c,
                &g.account_id,
                &KeySelector::ContentRoot {
                    generation: *generation,
                }
                .into(),
                canister_id,
            )?;
            let operation = Operation::vetkd(
                descriptor.master_key_name.clone(),
                model::context(config),
                model::root_input(&g.account_id, *generation),
                transport_key.to_vec(),
            );
            (operation, descriptor, Finish::RootKey)
        }
    };
    let cost = operation.cost().map_err(Error::Unavailable)?;
    let reserved = cost.total().map_err(Error::Unavailable)?;
    ensure(reserved <= g.max_cycles, Error::QuotaExceeded)?;
    Ok(PreparedExecution {
        operation,
        cost,
        reserved,
        key,
        finish,
    })
}

/// Package a successful management response. The call has already run and is
/// never retried, so an unpackageable response is a known failure.
fn finish(finish: Finish, key: KeyDescriptor, bytes: Vec<u8>) -> Result<ExecutionOutput> {
    match finish {
        Finish::Document(signature) => Ok(ExecutionOutput::Signature {
            artifact: signature.finish(bytes)?,
            key,
        }),
        Finish::RootKey => Ok(ExecutionOutput::EncryptedRootKey {
            encrypted_key: bytes.into(),
            key,
        }),
        Finish::AgentEvent(event_hash) => {
            let public_key: Hash = key
                .public_key
                .as_slice()
                .try_into()
                .map(Hash::new)
                .map_err(|_| Error::IntegrityFailed)?;
            verify(&public_key, event_hash.as_slice(), &bytes)?;
            Ok(ExecutionOutput::AgentSignature {
                event_hash,
                signature: bytes
                    .try_into()
                    .map(Ed25519Signature::new)
                    .map_err(|_| Error::IntegrityFailed)?,
                key,
            })
        }
    }
}

#[ic_cdk::update]
async fn execute(grant: ExecutionGrant) -> Result<ExecutionResult> {
    let canister_id = ic_cdk::api::canister_self();
    let caller = ic_cdk::api::msg_caller();
    let c = ready()?;
    let config = &c.state.config;
    nonzero(grant.account_id.as_slice())?;
    check_home(config, caller, &grant.account_id)?;
    ensure(
        grant.home_cose == canister_id && caller == grant.home_user,
        Error::Forbidden,
    )?;
    let mut h = home_or_new(&grant.account_id)?;
    let at = now();
    if let Some(sequence) = h.check(&grant, at)? {
        return execution(&grant.account_id, sequence);
    }
    // Expired requests still close their sequence, without parsing or deriving keys.
    let prepared = if at >= grant.expires_at {
        Err(Error::Expired)
    } else {
        prepare(&c, &grant, canister_id)
    };
    // Only a management call consumes budget; failures close their sequence.
    let cycles = prepared.as_ref().ok().map(|p| p.reserved);
    let removed = h.prepare(&grant, at, cycles)?;
    if let Some(cycles) = cycles {
        // Last fallible check before committing. An Err must not consume a sequence.
        reserve_budget(at, cycles, config, grant.kind.is_formal())?;
    }
    let mut result = ExecutionResult {
        request_id: grant.request_id,
        cycles_cost_upper_bound: 0,
        outcome: ExecutionOutcome::Executing,
    };
    let PreparedExecution {
        operation,
        cost,
        reserved,
        key,
        finish: finishing,
    } = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            result.outcome = ExecutionOutcome::Failed(error);
            h.finish(grant.execution_sequence, &result);
            save_execution(
                &grant.account_id,
                &h,
                grant.execution_sequence,
                &result,
                &removed,
            );
            return Ok(result);
        }
    };
    result.cycles_cost_upper_bound = reserved;
    save_execution(
        &grant.account_id,
        &h,
        grant.execution_sequence,
        &result,
        &removed,
    );
    let account_id = grant.account_id;
    let sequence = grant.execution_sequence;
    let formal = grant.kind.is_formal();
    drop(grant);
    drop(h);
    drop(c);
    let response = operation.execute().await;
    let failure = response.as_ref().err().map(chain_key::classify_failure);
    let unsent = failure == Some(FailureKind::NotSent);
    // An unsent call returns synchronously in update mode, where the refund API
    // traps. Only an actual reply/reject callback may inspect refunded cycles.
    result.cycles_cost_upper_bound = if unsent {
        0
    } else {
        chain_key::cost_upper_bound(
            cost,
            response.as_ref().err(),
            ic_cdk::api::msg_cycles_refunded(),
        )
    };
    result.outcome = match response {
        Ok(bytes) => match finish(finishing, key, bytes) {
            Ok(output) => ExecutionOutcome::Completed(Box::new(output)),
            Err(error) => ExecutionOutcome::Failed(error),
        },
        Err(e) => {
            let detail = Error::Unavailable(format!("{e:?}"));
            if failure == Some(FailureKind::Unknown) {
                ExecutionOutcome::Unknown(detail)
            } else {
                ExecutionOutcome::Failed(detail)
            }
        }
    };
    // Other executions can finish while this management call is in flight.
    let mut current = home(&account_id).expect("prepared home");
    if unsent {
        current.budgets.release_unsent(reserved, formal);
        release_unsent_budget(reserved, formal);
    }
    current.finish(sequence, &result);
    save_execution(&account_id, &current, sequence, &result, &[]);
    Ok(result)
}

#[ic_cdk::query]
fn get_execution(account_id: AccountId, request_id: Hash) -> Result<ExecutionResult> {
    check_home(&cfg().state.config, ic_cdk::api::msg_caller(), &account_id)?;
    let h = home(&account_id).ok_or(Error::NotFound)?;
    let sequence = h.sequence(&request_id).ok_or(Error::NotFound)?;
    execution(&account_id, sequence)
}

/// Prune one bounded page of expired terminal results, retaining replay guards.
/// Public maintenance is safe: a page inspects at most eight accounts.
#[ic_cdk::update]
fn prune_executions(after: Option<AccountId>) -> ExecutionCleanup {
    crate::store::prune_executions(after, now())
}
