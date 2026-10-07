use crate::{model, store::*};
use candid::Principal;
use dmsg_protocol::{agent::*, *};
use dmsg_runtime::admin::{self, hex, validation, Validation};
use dmsg_types::{cose::*, *};
use ic_cose_chain_key::{self as chain_key, Cost, FailureKind, Operation};

// Half the 40B update limit: one more account fits even when its results are
// large and their removal rewrites their B-tree leaves.
const CLEANUP_INSTRUCTIONS: u64 = 20_000_000_000;

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
            fingerprint: None,
            error: None,
        },
        public_key: None,
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
        let key = c.public_key.as_ref().expect("cached key");
        let fp = sha256(key);
        assert_eq!(c.state.fingerprint, Some(fp), "cached key fingerprint mismatch");
        assert!(
            pinned(&c.state.config.master, &fp),
            "configured key fingerprint mismatch"
        );
    }
    save_cfg(&c);
}

async fn fetch_master(config: &CoseInit) -> Result<Vec<u8>> {
    let key = chain_key::vetkd_public_key(config.master.key_name.clone(), model::context(config))
        .await
        .map_err(Error::Unavailable)?;
    ensure(
        pinned(&config.master, &sha256(&key)),
        Error::IntegrityFailed,
    )?;
    Ok(key)
}

// Returns false when the key is already ready.
fn check_initialize(c: &Config) -> Result<bool> {
    ensure(
        c.state.initialization != Initialization::Initializing,
        Error::Pending,
    )?;
    Ok(c.state.initialization != Initialization::Ready)
}

/// Fetch the content-root vetKD public key and check it against its pin.
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
    let fetched = fetch_master(&c.state.config).await;
    // Commit onto the current record, not the snapshot taken before the call.
    let mut c = cfg();
    let result = match fetched {
        Ok(key) => {
            c.state.fingerprint = Some(sha256(&key));
            c.public_key = Some(key);
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
        format!(
            "Initialize {:?} content-root vetKD key {} pinned to {}.{}",
            c.state.config.environment,
            c.state.config.master.key_name,
            hex(c.state.config.master.expected_fingerprint.as_slice()),
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
            format!(
                "{} All homes share this executor's {} of {MAX_HOMES} accounts.{}",
                admin::user_home_payload(&config.environment, &config.issuer_namespace, home, true),
                accounts(),
                admin::unchanged(fresh, "Already listed"),
            )
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
            "Set the COSE daily budget to {daily_executions} derivations and {daily_cycles} cycles (from {} and {}).{}",
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

/// Account capacity, retained results, Unknown executions, today's global
/// budget usage, stable pages and the cycle balance.
#[ic_cdk::query]
fn cose_stats() -> CoseStats {
    stats(now())
}

/// Refuse, before execution, ingress that the method would reject: `execute`
/// from a Principal that is not a configured user home, and administrative
/// methods from anyone but a controller or governance.
#[ic_cdk::inspect_message]
fn inspect_message() {
    let caller = ic_cdk::api::msg_caller();
    let allowed = match ic_cdk::api::msg_method_name().as_str() {
        "execute" => cfg().state.config.user_homes.contains(&caller),
        "initialize_keys" | "admin_add_user_home" | "admin_set_daily_budget" => {
            check_admin(caller).is_ok()
        }
        _ => true,
    };
    if allowed {
        ic_cdk::api::accept_message();
    }
}

/// The content-root vetKD public key, described for one account and root
/// generation. Clients encrypt that generation's recovery envelope to the
/// identity `canonical((account_id, generation))` under this key. No
/// management call, registration or stable write is needed for this query.
#[ic_cdk::query]
fn root_public_key(account_id: AccountId, generation: u64) -> Result<KeyDescriptor> {
    describe(
        &ready()?,
        &account_id,
        generation,
        ic_cdk::api::canister_self(),
    )
}

fn describe(
    c: &Config,
    account_id: &AccountId,
    generation: u64,
    canister_id: Principal,
) -> Result<KeyDescriptor> {
    nonzero(account_id.as_slice())?;
    ensure_valid(generation > 0, "generation")?;
    let config = &c.state.config;
    let public_key = c
        .public_key
        .clone()
        .ok_or_else(|| Error::Unavailable("missing initialized key".into()))?;
    Ok(KeyDescriptor {
        account_id: *account_id,
        home_cose: canister_id,
        master_key_name: config.master.key_name.clone(),
        environment: config.environment.clone(),
        derivation_version: config.derivation_version,
        key_generation: generation,
        public_key_fingerprint: sha256(&public_key),
        public_key: public_key.into(),
    })
}

struct PreparedExecution {
    operation: Operation,
    cost: Cost,
    /// Complete cost bound reserved from the per-account and global budgets.
    reserved: u128,
    key: KeyDescriptor,
}

/// Validate a grant and build its management call. Called only before
/// `g.expires_at`.
fn prepare(c: &Config, g: &ExecutionGrant, canister_id: Principal) -> Result<PreparedExecution> {
    let config = &c.state.config;
    validate_transport_key(&g.transport_key)?;
    let key = describe(c, &g.account_id, g.generation, canister_id)?;
    let operation = Operation::vetkd(
        key.master_key_name.clone(),
        model::context(config),
        model::root_input(&g.account_id, g.generation),
        g.transport_key.to_vec(),
    );
    let cost = operation.cost().map_err(Error::Unavailable)?;
    let reserved = cost.total().map_err(Error::Unavailable)?;
    ensure(reserved <= g.max_cycles, Error::QuotaExceeded)?;
    Ok(PreparedExecution {
        operation,
        cost,
        reserved,
        key,
    })
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
    // Expired requests still close their sequence, without deriving keys.
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
        reserve_budget(at, cycles, config)?;
    }
    let mut result = ExecutionResult {
        request_id: grant.request_id,
        cycles_cost_upper_bound: 0,
        cycles_charged: 0,
        outcome: ExecutionOutcome::Executing,
    };
    let PreparedExecution {
        operation,
        cost,
        reserved,
        key,
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
    drop(h);
    drop(c);
    let call = AwaitingCall::begin(&account_id, sequence);
    let response = operation.execute().await;
    drop(call);
    let failure = response.as_ref().err().map(chain_key::classify_failure);
    let unsent = failure == Some(FailureKind::NotSent);
    // An unsent call returns synchronously in update mode, where the refund API
    // traps. Only an actual reply/reject callback may inspect refunded cycles.
    let refunded = if unsent {
        0
    } else {
        ic_cdk::api::msg_cycles_refunded()
    };
    result.cycles_cost_upper_bound =
        chain_key::cost_upper_bound(cost, response.as_ref().err(), refunded);
    result.outcome = match response {
        Ok(bytes) => ExecutionOutcome::Completed(EncryptedRootKey {
            encrypted_key: bytes.into(),
            key,
        }),
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
    if matches!(result.outcome, ExecutionOutcome::Unknown(_)) {
        // The call may have run: keep the whole reservation.
        count_unknown(1);
    } else {
        // Settle the reservation made at `at` to the fee actually consumed. An
        // unsent or rejected call ran nothing and also returns its execution.
        result.cycles_charged = if unsent {
            0
        } else {
            cost.request_cycles.saturating_sub(refunded)
        };
        let executed = failure.is_none();
        current
            .budget
            .settle(at, reserved, result.cycles_charged, executed);
        settle_budget(at, reserved, result.cycles_charged, executed);
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
    let mut result = execution(&account_id, sequence)?;
    // An upgrade dropped the callback; the next update records the same.
    if result.outcome == ExecutionOutcome::Executing && !awaiting(account_id.as_slice(), sequence) {
        result.outcome = ExecutionOutcome::Unknown(Error::ExecutionUnknown);
    }
    Ok(result)
}

/// Prune one bounded page of expired terminal results, retaining replay guards.
/// Public maintenance is safe: a page inspects at most 64 accounts and stops
/// between accounts after half the 40B update instruction limit.
#[ic_cdk::update]
fn prune_executions(after: Option<AccountId>) -> ExecutionCleanup {
    crate::store::prune_executions(after, now(), || {
        ic_cdk::api::instruction_counter() < CLEANUP_INSTRUCTIONS
    })
}
