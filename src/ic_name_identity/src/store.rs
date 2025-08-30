use candid::{CandidType, Principal};
use ciborium::{from_reader, from_reader_with_buffer, into_writer};
use ic_canister_sig_creation::{
    signature_map::{CanisterSigInputs, SignatureMap, LABEL_SIG},
    DELEGATION_SIG_DOMAIN,
};
use ic_cdk::api::certified_data_set;
use ic_certification::labeled_hash;
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, Storable,
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, cell::RefCell, collections::BTreeSet};

type Memory = VirtualMemory<DefaultMemoryImpl>;

use crate::types;

const MAX_DELEGATIONS: usize = 8;

#[derive(CandidType, Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub name: String,
    pub session_expires_in_ms: u64,
    pub sign_in_count: u64,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Delegator {
    #[serde(rename = "o")]
    pub owner: Principal,
    #[serde(rename = "s")]
    pub sign_in_at: u64, // milliseconds since epoch
    #[serde(rename = "r")]
    pub role: i8, // -1: suspend; 0: member; 1: owner
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Delegations(Vec<Delegator>);

impl Delegations {
    pub fn has_permission(&self, delegator: &Principal, role: i8) -> bool {
        self.0
            .iter()
            .any(|d| &d.owner == delegator && d.role >= role)
    }

    pub fn add(&mut self, delegator: Principal, role: i8) {
        if let Some(d) = self.0.iter_mut().find(|d| d.owner == delegator) {
            d.role = role;
            return;
        }

        self.0.push(Delegator {
            owner: delegator,
            sign_in_at: 0,
            role,
        });
    }

    pub fn remove(&mut self, caller: &Principal, delegator: &Principal) -> Result<(), String> {
        if self.0.len() <= 1 {
            return Err("cannot remove the last delegator".to_string());
        }

        if caller != delegator {
            if let Some(d) = self.0.iter().find(|d| &d.owner == caller) {
                if d.role != 1 {
                    return Err("caller is not a manager".to_string());
                }
            }
            if let Some(d) = self.0.iter().find(|d| &d.owner == delegator) {
                if d.role == 1 {
                    return Err("manager can not be removed".to_string());
                }
            }
        }

        self.0.retain(|d| &d.owner != delegator);
        if self.0.len() == 1 {
            self.0[0].role = 1;
        }

        Ok(())
    }

    pub fn delegators(self) -> Vec<types::Delegator> {
        self.0
            .into_iter()
            .map(|d| types::Delegator {
                owner: d.owner,
                sign_in_at: d.sign_in_at,
                role: d.role,
            })
            .collect()
    }
}

impl Storable for Delegations {
    const BOUND: Bound = Bound::Unbounded;

    fn into_bytes(self) -> Vec<u8> {
        let mut buf = vec![];
        into_writer(&self, &mut buf).expect("failed to encode Delegations data");
        buf
    }

    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut buf = vec![];
        into_writer(&self, &mut buf).expect("failed to encode Delegations data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode Delegations data")
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Names(BTreeSet<String>);
impl Storable for Names {
    const BOUND: Bound = Bound::Unbounded;

    fn into_bytes(self) -> Vec<u8> {
        let mut buf = vec![];
        into_writer(&self.0, &mut buf).expect("failed to encode Names data");
        buf
    }

    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut buf = vec![];
        into_writer(&self.0, &mut buf).expect("failed to encode Names data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let obj: BTreeSet<String> = from_reader(&bytes[..]).expect("failed to decode Names data");
        Self(obj)
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
const NAME_DELEGATIONS_MEMORY_ID: MemoryId = MemoryId::new(1);
const MY_NAMES_MEMORY_ID: MemoryId = MemoryId::new(2);

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    static SIGNATURES : RefCell<SignatureMap> = RefCell::new(SignatureMap::default());


    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE_STORE: RefCell<StableCell<Vec<u8>, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            Vec::new()
        )
    );

    static NAME_DELEGATIONS_STORE: RefCell<StableBTreeMap<String, Delegations, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(NAME_DELEGATIONS_MEMORY_ID)),
        )
    );

    static MY_NAMES_STORE: RefCell<StableBTreeMap<Principal, Names, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(MY_NAMES_MEMORY_ID)),
        )
    );
}

pub mod state {
    use super::*;

    pub fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        STATE.with_borrow(f)
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE.with_borrow_mut(f)
    }

    pub fn load() {
        let mut scratch = [0; 4096];
        STATE_STORE.with(|r| {
            STATE.with(|h| {
                let v: State = from_reader_with_buffer(&r.borrow().get()[..], &mut scratch)
                    .expect("failed to decode STATE_STORE data");
                *h.borrow_mut() = v;
            });
        });
    }

    pub fn save() {
        STATE.with(|h| {
            STATE_STORE.with(|r| {
                let mut buf = vec![];
                into_writer(&(*h.borrow()), &mut buf).expect("failed to encode STATE_STORE data");
                r.borrow_mut().set(buf);
            });
        });
    }

    pub fn add_signature(seed: &[u8], message: &[u8]) {
        SIGNATURES.with_borrow_mut(|sigs| {
            let sig_inputs = CanisterSigInputs {
                domain: DELEGATION_SIG_DOMAIN,
                seed,
                message,
            };
            sigs.add_signature(&sig_inputs);

            certified_data_set(labeled_hash(LABEL_SIG, &sigs.root_hash()));
        });
    }

    pub fn get_signature(seed: &[u8], message: &[u8]) -> Result<Vec<u8>, String> {
        SIGNATURES.with_borrow(|sigs| {
            let sig_inputs = CanisterSigInputs {
                domain: DELEGATION_SIG_DOMAIN,
                seed,
                message,
            };
            sigs.get_signature_as_cbor(&sig_inputs, None)
                .map_err(|err| format!("failed to get signature: {:?}", err))
        })
    }

    pub fn add_delegator(
        name: &String,
        delegator: &Principal,
        role: i8,
    ) -> Result<Vec<types::Delegator>, String> {
        let res = NAME_DELEGATIONS_STORE.with_borrow_mut(|store| {
            let mut delegations = store.get(name).unwrap_or_else(|| Delegations(Vec::new()));
            if delegations.0.len() >= MAX_DELEGATIONS {
                return Err("max delegations reached".to_string());
            }
            delegations.add(*delegator, role);
            let res = delegations.clone().delegators();
            store.insert(name.clone(), delegations);
            Ok(res)
        })?;

        MY_NAMES_STORE.with_borrow_mut(|store| {
            let mut names = store
                .get(delegator)
                .unwrap_or_else(|| Names(BTreeSet::new()));
            names.0.insert(name.clone());
            store.insert(*delegator, names);
        });
        Ok(res)
    }

    pub fn remove_delegator(
        name: &String,
        caller: &Principal,
        delegator: &Principal,
    ) -> Result<(), String> {
        NAME_DELEGATIONS_STORE.with_borrow_mut(|store| {
            let mut delegations = store
                .get(name)
                .ok_or_else(|| "name not found".to_string())?;
            delegations.remove(caller, delegator)?;
            store.insert(name.clone(), delegations);

            MY_NAMES_STORE.with_borrow_mut(|store| {
                if let Some(mut names) = store.get(delegator) {
                    names.0.remove(name);
                    if names.0.is_empty() {
                        store.remove(delegator);
                    } else {
                        store.insert(*delegator, names);
                    }
                }
            });
            Ok(())
        })
    }

    pub fn reset_delegators(name: &String, delegators: BTreeSet<Principal>) -> Result<(), String> {
        NAME_DELEGATIONS_STORE.with_borrow_mut(|store| {
            if let Some(delegations) = store.get(name) {
                MY_NAMES_STORE.with_borrow_mut(|store| {
                    for delegator in delegations.0 {
                        if let Some(mut names) = store.get(&delegator.owner) {
                            names.0.remove(name);
                            if names.0.is_empty() {
                                store.remove(&delegator.owner);
                            } else {
                                store.insert(delegator.owner, names);
                            }
                        }
                    }
                });
            }
            let delegations = Delegations(
                delegators
                    .into_iter()
                    .map(|p| Delegator {
                        owner: p,
                        sign_in_at: 0,
                        role: 1,
                    })
                    .collect(),
            );
            store.insert(name.clone(), delegations);
            Ok(())
        })
    }

    pub fn delegator_sign_in(
        name: &String,
        delegator: &Principal,
        now_ms: u64,
    ) -> Result<(), String> {
        NAME_DELEGATIONS_STORE.with_borrow_mut(|store| {
            if let Some(mut delegations) = store.get(name) {
                if let Some(d) = delegations.0.iter_mut().find(|d| &d.owner == delegator) {
                    if d.role == -1 {
                        return Err("delegator is suspended".to_string());
                    }

                    d.sign_in_at = now_ms;
                    store.insert(name.clone(), delegations);
                    return Ok(());
                }
            }
            Err("caller is not authorized".to_string())
        })
    }

    pub fn get_delegations(name: &String) -> Option<Delegations> {
        NAME_DELEGATIONS_STORE.with_borrow(|store| store.get(name))
    }

    pub fn get_names(delegator: &Principal) -> Option<BTreeSet<String>> {
        MY_NAMES_STORE.with_borrow(|store| store.get(delegator).map(|names| names.0))
    }
}
