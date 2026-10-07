use crate::model;
use dmsg_runtime::storage::{compact_bytes, compact_from_bytes, CompactStored, MapExt};
use dmsg_types::{cose::*, *};
use ic_cose_chain_key::PublicKey;
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, RestrictedMemory, StableBTreeMap, StableCell,
};
use std::{cell::RefCell, collections::BTreeSet};

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) schema: u16,
    pub(crate) state: KeyState,
    pub(crate) keys: Vec<PublicKey>,
}

type Memory = VirtualMemory<DefaultMemoryImpl>;
type CellMemory = RestrictedMemory<Memory>;
// Share one default 128-page bucket: keep the small global cell (both budgets
// and the Unknown count) in its last page, without another 8 MiB bucket.
const BUDGET_PAGE: u64 = 127;
const CLEANUP_BATCH: usize = 64;
pub(crate) const MAX_HOMES: u64 = 1_000_000;

fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

thread_local! {
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, CellMemory>> =
        RefCell::new(StableCell::init(
            RestrictedMemory::new(memory(0), 0..BUDGET_PAGE),
            CompactStored::new(&None),
        ));
    // Keep encoded results so replacement/cleanup discards bytes without decoding
    // the old body returned by StableBTreeMap::insert/remove.
    static EXECUTIONS: RefCell<StableBTreeMap<Vec<u8>, Vec<u8>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(2)));
    // Derived only at initialization/upgrade, never persisted as another key authority.
    static SIGNING_ROOTS: RefCell<Vec<Option<PublicKey>>> = const { RefCell::new(Vec::new()) };
    static HOMES: RefCell<StableBTreeMap<Vec<u8>, CompactStored<model::Home>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(1)));
    static GLOBAL: RefCell<StableCell<CompactStored<model::Global>, CellMemory>> =
        RefCell::new(StableCell::init(
            RestrictedMemory::new(memory(0), BUDGET_PAGE..BUDGET_PAGE + 1),
            CompactStored::new(&model::Global::default()),
        ));
    // Executions whose management call this module instance awaits. An upgrade
    // empties it, so an InFlight record absent here lost its callback.
    static AWAITING: RefCell<BTreeSet<Vec<u8>>> = const { RefCell::new(BTreeSet::new()) };
}

pub(crate) fn cfg() -> Config {
    CONFIG.with_borrow(|t| t.get().value().expect("initialized"))
}

pub(crate) fn save_cfg(c: &Config) {
    CONFIG.with_borrow_mut(|t| t.set(CompactStored::some(c)));
}

pub(crate) fn cache_signing_roots(config: &CoseInit, keys: &[PublicKey]) -> Result<()> {
    use ic_cdk_management_canister::SchnorrAlgorithm;
    use ic_cose_chain_key::{derive_ecdsa_public_key, derive_schnorr_public_key};

    let roots = config
        .masters
        .iter()
        .zip(keys)
        .map(|(master, key)| match master.algorithm {
            Algorithm::Ed25519 => derive_schnorr_public_key(
                SchnorrAlgorithm::Ed25519,
                key,
                model::signing_prefix(config),
            )
            .map(Some),
            Algorithm::EcdsaSecp256k1 => {
                derive_ecdsa_public_key(key, model::signing_prefix(config)).map(Some)
            }
            Algorithm::VetKdBls12381 => Ok(None),
        })
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Error::Unavailable)?;
    SIGNING_ROOTS.with_borrow_mut(|cached| *cached = roots);
    Ok(())
}

pub(crate) fn signing_root(index: usize) -> Result<PublicKey> {
    SIGNING_ROOTS.with_borrow(|roots| {
        roots
            .get(index)
            .and_then(Option::as_ref)
            .cloned()
            .ok_or_else(|| Error::Unavailable("missing initialized signing prefix".into()))
    })
}

pub(crate) fn accounts() -> u64 {
    HOMES.with_borrow(|t| t.len())
}

pub(crate) fn home(account_id: &AccountId) -> Option<model::Home> {
    HOMES.with_borrow(|t| t.load(account_id.as_slice()))
}

/// Load an account's home, or start one while under the fixed account capacity.
/// Executions an upgrade abandoned are first recorded as Unknown.
pub(crate) fn home_or_new(account_id: &AccountId) -> Result<model::Home> {
    let account = account_id.as_slice();
    HOMES.with_borrow_mut(|t| match t.load(account) {
        Some(mut h) => {
            if resolve_abandoned(account, &mut h) {
                t.put(account, &h);
            }
            Ok(h)
        }
        None => {
            ensure(t.len() < MAX_HOMES, Error::QuotaExceeded)?;
            Ok(model::Home::default())
        }
    })
}

/// Mark an execution as awaiting its management call until `end_call`.
pub(crate) fn begin_call(account_id: &AccountId, sequence: u64) {
    AWAITING.with_borrow_mut(|s| s.insert(execution_key(account_id.as_slice(), sequence)));
}

pub(crate) fn end_call(account_id: &AccountId, sequence: u64) {
    AWAITING.with_borrow_mut(|s| s.remove(&execution_key(account_id.as_slice(), sequence)));
}

pub(crate) fn awaiting(account: &[u8], sequence: u64) -> bool {
    AWAITING.with_borrow(|s| s.contains(&execution_key(account, sequence)))
}

/// Record as Unknown the InFlight executions this module instance does not
/// await, rewriting their stored results. Return whether the home changed;
/// the caller saves it.
fn resolve_abandoned(account: &[u8], h: &mut model::Home) -> bool {
    let abandoned = h.abandon(|sequence| awaiting(account, sequence));
    if abandoned.is_empty() {
        return false;
    }
    EXECUTIONS.with_borrow_mut(|t| {
        for sequence in &abandoned {
            let key = execution_key(account, *sequence);
            let mut result: ExecutionResult = compact_from_bytes(&t.get(&key).expect("result"));
            result.outcome = ExecutionOutcome::Unknown(Error::ExecutionUnknown);
            t.insert(key, compact_bytes(&result));
        }
    });
    count_unknown(abandoned.len() as u64);
    true
}

/// Commit the bounded metadata and the one result changed in this message.
pub(crate) fn save_execution(
    account_id: &AccountId,
    h: &model::Home,
    sequence: u64,
    result: &ExecutionResult,
    removed: &[u64],
) {
    let account = account_id.as_slice();
    EXECUTIONS.with_borrow_mut(|t| {
        for seq in removed {
            t.remove(&execution_key(account, *seq));
        }
        t.insert(execution_key(account, sequence), compact_bytes(result));
    });
    HOMES.with_borrow_mut(|t| t.put(account, h));
}

pub(crate) fn execution(account_id: &AccountId, sequence: u64) -> Result<ExecutionResult> {
    EXECUTIONS.with_borrow(|t| {
        t.get(&execution_key(account_id.as_slice(), sequence))
            .map(|bytes| compact_from_bytes(&bytes))
            .ok_or(Error::NotFound)
    })
}

fn update_global(f: impl FnOnce(&mut model::Global) -> Result<()>) -> Result<()> {
    GLOBAL.with_borrow_mut(|t| {
        let mut global = t.get().value();
        f(&mut global)?;
        t.set(CompactStored::new(&global));
        Ok(())
    })
}

pub(crate) fn reserve_budget(
    now: u64,
    cycles: u128,
    config: &CoseInit,
    formal: bool,
) -> Result<()> {
    update_global(|g| {
        g.budgets.reserve(
            now,
            cycles,
            config.daily_executions,
            config.daily_cycles,
            formal,
        )
    })
}

/// Settle a global reservation; see [`model::Budgets::settle`].
pub(crate) fn settle_budget(
    reserved_at: u64,
    reserved: u128,
    charged: u128,
    executed: bool,
    formal: bool,
) {
    update_global(|g| {
        g.budgets
            .settle(reserved_at, reserved, charged, executed, formal);
        Ok(())
    })
    .expect("settlement");
}

pub(crate) fn count_unknown(executions: u64) {
    update_global(|g| {
        g.unknown = g.unknown.saturating_add(executions);
        Ok(())
    })
    .expect("unknown count");
}

pub(crate) fn stats(now: u64) -> CoseStats {
    let global = GLOBAL.with_borrow(|t| t.get().value());
    let day = now / DAY;
    // A budget last used on an earlier day has nothing counted today.
    let today = |b: &dmsg_runtime::Budget| {
        if b.day == day {
            (b.executions, b.cycles)
        } else {
            (0, 0)
        }
    };
    let (executions_today, cycles_today) = today(&global.budgets.total);
    let (formal_executions_today, formal_cycles_today) = today(&global.budgets.formal);
    CoseStats {
        accounts: accounts(),
        max_accounts: MAX_HOMES,
        results: EXECUTIONS.with_borrow(|t| t.len()),
        in_flight: AWAITING.with_borrow(|s| s.len() as u64),
        unknown: global.unknown,
        budget_day: day,
        executions_today,
        cycles_today,
        formal_executions_today,
        formal_cycles_today,
        stable_pages: ic_cdk::api::stable_size(),
        cycles: ic_cdk::api::canister_cycle_balance(),
    }
}

/// Each page visits at most 64 homes and removes at most 64 * WINDOW results.
/// A full page returns its last key as the cursor; a shorter page ends the pass.
/// Empty homes keep their sequence high-water mark and budget counters.
pub(crate) fn prune_executions(after: Option<AccountId>, now: u64) -> ExecutionCleanup {
    let mut homes =
        HOMES.with_borrow(|t| t.page(after.map_or_else(Vec::new, |id| id.to_vec()), CLEANUP_BATCH));
    let next_after = homes
        .last()
        .filter(|_| homes.len() == CLEANUP_BATCH)
        .map(|(id, _)| AccountId::try_from(id.as_slice()).expect("account key"));
    let mut results_removed = 0;
    for (account, h) in &mut homes {
        // Abandoned calls must not keep an idle account's window closed.
        let resolved = resolve_abandoned(account, h);
        let removed = h.prune(now);
        if removed.is_empty() && !resolved {
            continue;
        }
        EXECUTIONS.with_borrow_mut(|t| {
            for seq in &removed {
                t.remove(&execution_key(account, *seq));
            }
        });
        results_removed += removed.len() as u32;
        HOMES.with_borrow_mut(|t| t.put(account, h));
    }
    ExecutionCleanup {
        next_after,
        homes_scanned: homes.len() as u32,
        results_removed,
    }
}

pub(crate) const STABLE_SCHEMA: u16 = 9;

fn execution_key(account: &[u8], sequence: u64) -> Vec<u8> {
    [account, &sequence.to_be_bytes()].concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use candid::Principal;
    use dmsg_protocol::canonical;
    use ic_cdk_management_canister::SchnorrAlgorithm;
    use ic_cose_chain_key::{derive_ecdsa_public_key, derive_schnorr_public_key};

    #[test]
    fn cleanup_discards_encoded_bodies_and_retains_unknown_results() {
        let account = AccountId([71; 12]);
        let mut h = model::Home {
            closed_sequence: 2,
            ..Default::default()
        };
        for sequence in 1..=2 {
            h.executions.insert(
                sequence,
                model::Execution {
                    request_id: Hash::new([sequence as u8; 32]),
                    digest: Hash::new([7; 32]),
                    expires_at: MINUTE,
                    state: if sequence == 1 {
                        model::ExecutionState::Unknown
                    } else {
                        model::ExecutionState::Terminal
                    },
                    formal: true,
                },
            );
        }
        let unknown = ExecutionResult {
            request_id: Hash::new([1; 32]),
            cycles_cost_upper_bound: 5,
            cycles_charged: 0,
            outcome: ExecutionOutcome::Unknown(Error::ExecutionUnknown),
        };
        save_execution(&account, &h, 1, &unknown, &[]);
        // Cleanup must not even parse a retired body: invalid CBOR would trap
        // through CompactStored::from_bytes if remove tried to decode it.
        EXECUTIONS.with_borrow_mut(|t| {
            t.insert(execution_key(account.as_slice(), 2), vec![0xff; 4096]);
        });
        let cleaned = prune_executions(None, 2 * DAY);
        assert_eq!(cleaned.results_removed, 1);
        assert_eq!(execution(&account, 1), Ok(unknown));
        assert_eq!(execution(&account, 2), Err(Error::NotFound));
        let remaining = home(&account).unwrap();
        assert_eq!(remaining.closed_sequence, 2);
        assert_eq!(remaining.executions.len(), 1);
    }

    #[test]
    fn cached_prefixes_preserve_the_full_derivation_for_both_signing_algorithms() {
        // Public curve generators with fixed chain codes, not secret key material.
        let mut ed25519 = vec![0x66; 32];
        ed25519[0] = 0x58;
        let secp256k1 = vec![
            0x02, 0x79, 0xbe, 0x66, 0x7e, 0xf9, 0xdc, 0xbb, 0xac, 0x55, 0xa0, 0x62, 0x95, 0xce,
            0x87, 0x0b, 0x07, 0x02, 0x9b, 0xfc, 0xdb, 0x2d, 0xce, 0x28, 0xd9, 0x59, 0xf2, 0x81,
            0x5b, 0x16, 0xf8, 0x17, 0x98,
        ];
        let keys = [ed25519, secp256k1].map(|public_key| PublicKey {
            public_key,
            chain_code: vec![7; 32],
        });
        for environment in [
            Environment::Local,
            Environment::Staging,
            Environment::Production,
        ] {
            let config = CoseInit {
                environment,
                executing_canister: Principal::from_slice(&[1]),
                user_homes: vec![Principal::from_slice(&[2])],
                governance: Principal::from_slice(&[3]),
                issuer_namespace: "https://dmsg.test/u/".into(),
                derivation_version: 2,
                daily_cycles: 100,
                daily_executions: 10,
                masters: [Algorithm::Ed25519, Algorithm::EcdsaSecp256k1]
                    .map(|algorithm| MasterKey {
                        algorithm,
                        key_name: "key_1".into(),
                        expected_fingerprint: Hash::new([1; 32]),
                    })
                    .into(),
            };
            cache_signing_roots(&config, &keys).unwrap();
            for (index, master) in config.masters.iter().enumerate() {
                let derive = |root: &PublicKey, path| match master.algorithm {
                    Algorithm::Ed25519 => {
                        derive_schnorr_public_key(SchnorrAlgorithm::Ed25519, root, path)
                    }
                    Algorithm::EcdsaSecp256k1 => derive_ecdsa_public_key(root, path),
                    _ => unreachable!(),
                };
                for purpose in [
                    KeyPurpose::Statement,
                    KeyPurpose::FileAttestation,
                    KeyPurpose::AppAction,
                    KeyPurpose::AgentController,
                ] {
                    let key = KeyRequest {
                        generation: if purpose == KeyPurpose::AgentController {
                            7
                        } else {
                            1
                        },
                        purpose,
                        algorithm: master.algorithm.clone(),
                    };
                    for account in [AccountId([1; 12]), AccountId([2; 12])] {
                        let original = vec![
                            b"dmsg/formal/v2".to_vec(),
                            canonical(&config.environment),
                            account.to_vec(),
                            canonical(&key.purpose),
                            key.generation.to_be_bytes().to_vec(),
                        ];
                        assert_eq!(model::path(&config, &account, &key), original);
                        assert_eq!(
                            derive(
                                &signing_root(index).unwrap(),
                                model::signing_suffix(&account, &key)
                            )
                            .unwrap(),
                            derive(&keys[index], original).unwrap(),
                        );
                    }
                }
            }
        }
    }
}
