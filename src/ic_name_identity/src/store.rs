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

pub(crate) const MAX_DELEGATIONS: usize = 8;

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
pub(crate) struct Delegations(Vec<Delegator>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Upsert {
    Unchanged,
    Updated,
    Inserted,
}

impl Delegations {
    fn is_manager(&self, delegator: &Principal) -> bool {
        self.0.iter().any(|d| &d.owner == delegator && d.role == 1)
    }

    fn upsert(&mut self, delegator: Principal, role: i8) -> Result<Upsert, String> {
        if let Some(index) = self.0.iter().position(|d| d.owner == delegator) {
            if self.0[index].role == role {
                return Ok(Upsert::Unchanged);
            }
            if self.0[index].role == 1
                && role != 1
                && !self
                    .0
                    .iter()
                    .enumerate()
                    .any(|(other, d)| other != index && d.role == 1)
            {
                return Err("cannot demote the last manager".to_string());
            }
            self.0[index].role = role;
            return Ok(Upsert::Updated);
        }
        if self.0.len() >= MAX_DELEGATIONS {
            return Err("max delegations reached".to_string());
        }

        self.0.push(Delegator {
            owner: delegator,
            sign_in_at: 0,
            role,
        });
        Ok(Upsert::Inserted)
    }

    fn remove(&mut self, caller: &Principal, delegator: &Principal) -> Result<(), String> {
        if self.0.len() <= 1 {
            return Err("cannot remove the last delegator".to_string());
        }

        let target_index = self
            .0
            .iter()
            .position(|d| &d.owner == delegator)
            .ok_or_else(|| "delegator not found".to_string())?;
        if caller != delegator {
            let caller_role = self
                .0
                .iter()
                .find(|d| &d.owner == caller)
                .map(|d| d.role)
                .ok_or_else(|| "caller is not a manager".to_string())?;
            if caller_role != 1 {
                return Err("caller is not a manager".to_string());
            }
            if self.0[target_index].role == 1 {
                return Err("manager can not be removed".to_string());
            }
        }

        self.0.remove(target_index);
        if !self.0.iter().any(|d| d.role == 1) {
            let promote = self.0.iter().position(|d| d.role == 0).unwrap_or(0);
            self.0[promote].role = 1;
        }

        Ok(())
    }

    pub fn delegators(&self) -> Vec<types::Delegator> {
        self.0
            .iter()
            .map(|d| types::Delegator {
                owner: d.owner,
                sign_in_at: d.sign_in_at,
                role: d.role,
            })
            .collect()
    }

    pub fn record_sign_in(&mut self, delegator: &Principal, now_ms: u64) -> Result<(), String> {
        let delegator = self
            .0
            .iter_mut()
            .find(|d| &d.owner == delegator)
            .ok_or_else(|| "caller is not authorized".to_string())?;
        if delegator.role == -1 {
            return Err("delegator is suspended".to_string());
        }
        delegator.sign_in_at = now_ms;
        Ok(())
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

    fn add_name(
        store: &mut StableBTreeMap<Principal, Names, Memory>,
        delegator: &Principal,
        name: &str,
    ) {
        let mut names = store
            .get(delegator)
            .unwrap_or_else(|| Names(BTreeSet::new()));
        if names.0.insert(name.to_owned()) {
            store.insert(*delegator, names);
        }
    }

    fn remove_name(
        store: &mut StableBTreeMap<Principal, Names, Memory>,
        delegator: &Principal,
        name: &String,
    ) {
        let Some(mut names) = store.get(delegator) else {
            return;
        };
        if !names.0.remove(name) {
            return;
        }
        if names.0.is_empty() {
            store.remove(delegator);
        } else {
            store.insert(*delegator, names);
        }
    }

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
        caller: &Principal,
        delegator: &Principal,
        role: i8,
    ) -> Result<Vec<types::Delegator>, String> {
        let result = NAME_DELEGATIONS_STORE.with_borrow_mut(|store| {
            let mut delegations = store
                .get(name)
                .ok_or_else(|| "name not found".to_string())?;
            if !delegations.is_manager(caller) {
                return Err("caller is not a manager".to_string());
            }
            let change = delegations.upsert(*delegator, role)?;
            let result = delegations.delegators();
            if change != Upsert::Unchanged {
                store.insert(name.clone(), delegations);
            }
            Ok::<_, String>(result)
        })?;

        // This also lazily repairs reverse-index entries missing from older versions.
        MY_NAMES_STORE.with_borrow_mut(|store| add_name(store, delegator, name));
        Ok(result)
    }

    pub fn activate_name(
        name: &String,
        owner: &Principal,
    ) -> Result<Vec<types::Delegator>, String> {
        let result = NAME_DELEGATIONS_STORE.with_borrow_mut(|store| {
            if store.contains_key(name) {
                return Err("name is already activated".to_string());
            }
            let delegations = Delegations(vec![Delegator {
                owner: *owner,
                sign_in_at: 0,
                role: 1,
            }]);
            let result = delegations.delegators();
            store.insert(name.clone(), delegations);
            Ok::<_, String>(result)
        })?;
        MY_NAMES_STORE.with_borrow_mut(|store| add_name(store, owner, name));
        Ok(result)
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
            Ok::<_, String>(())
        })?;
        MY_NAMES_STORE.with_borrow_mut(|store| remove_name(store, delegator, name));
        Ok(())
    }

    pub fn reset_delegators(name: &String, delegators: BTreeSet<Principal>) -> Result<(), String> {
        if delegators.is_empty() {
            return Err("delegators is empty".to_string());
        }
        if delegators.len() > MAX_DELEGATIONS {
            return Err("max delegations reached".to_string());
        }

        let previous = NAME_DELEGATIONS_STORE
            .with_borrow(|store| store.get(name))
            .unwrap_or_else(|| Delegations(Vec::new()));
        MY_NAMES_STORE.with_borrow_mut(|store| {
            for old in &previous.0 {
                if !delegators.contains(&old.owner) {
                    remove_name(store, &old.owner, name);
                }
            }
            // Also repairs reverse-index entries missing from older reset operations.
            for delegator in &delegators {
                add_name(store, delegator, name);
            }
        });

        let delegations = Delegations(
            delegators
                .into_iter()
                .map(|owner| Delegator {
                    owner,
                    sign_in_at: 0,
                    role: 1,
                })
                .collect(),
        );
        NAME_DELEGATIONS_STORE.with_borrow_mut(|store| {
            store.insert(name.clone(), delegations);
        });
        Ok(())
    }

    pub fn get_delegations(name: &String) -> Option<Delegations> {
        NAME_DELEGATIONS_STORE.with_borrow(|store| store.get(name))
    }

    pub fn name_exists(name: &String) -> bool {
        NAME_DELEGATIONS_STORE.with_borrow(|store| store.contains_key(name))
    }

    pub fn set_delegations(name: &str, delegations: Delegations) {
        NAME_DELEGATIONS_STORE.with_borrow_mut(|store| {
            store.insert(name.to_owned(), delegations);
        });
    }

    pub fn get_names(delegator: &Principal) -> Option<BTreeSet<String>> {
        MY_NAMES_STORE.with_borrow(|store| store.get(delegator).map(|names| names.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn principal(id: u8) -> Principal {
        Principal::from_slice(&[id])
    }

    fn delegations(entries: &[(u8, i8)]) -> Delegations {
        Delegations(
            entries
                .iter()
                .map(|(id, role)| Delegator {
                    owner: principal(*id),
                    sign_in_at: 0,
                    role: *role,
                })
                .collect(),
        )
    }

    #[test]
    fn unauthorized_principal_cannot_remove_a_delegator() {
        let mut value = delegations(&[(1, 1), (2, 0)]);

        assert_eq!(
            value.remove(&principal(3), &principal(2)).unwrap_err(),
            "caller is not a manager"
        );
        assert_eq!(value.0.len(), 2);
    }

    #[test]
    fn manager_can_remove_member_but_not_another_manager() {
        let mut value = delegations(&[(1, 1), (2, 1), (3, 0)]);

        assert_eq!(
            value.remove(&principal(1), &principal(2)).unwrap_err(),
            "manager can not be removed"
        );
        value.remove(&principal(1), &principal(3)).unwrap();
        assert_eq!(value.0.len(), 2);
    }

    #[test]
    fn leaving_promotes_the_only_remaining_delegator() {
        let mut value = delegations(&[(1, 1), (2, 0)]);

        value.remove(&principal(1), &principal(1)).unwrap();
        assert_eq!(value.0[0].owner, principal(2));
        assert_eq!(value.0[0].role, 1);
    }

    #[test]
    fn leaving_last_manager_promotes_an_active_member() {
        let mut value = delegations(&[(1, 1), (2, -1), (3, 0)]);

        value.remove(&principal(1), &principal(1)).unwrap();
        assert_eq!(value.0[0].role, -1);
        assert_eq!(value.0[1].owner, principal(3));
        assert_eq!(value.0[1].role, 1);
    }

    #[test]
    fn last_manager_cannot_be_demoted() {
        let mut value = delegations(&[(1, 1), (2, 0)]);

        assert_eq!(
            value.upsert(principal(1), 0).unwrap_err(),
            "cannot demote the last manager"
        );
        value.upsert(principal(2), 1).unwrap();
        assert_eq!(value.upsert(principal(1), 0).unwrap(), Upsert::Updated);
    }

    #[test]
    fn full_delegation_set_still_allows_role_updates() {
        let mut value = Delegations(Vec::new());
        for id in 1..=MAX_DELEGATIONS as u8 {
            assert_eq!(value.upsert(principal(id), 0).unwrap(), Upsert::Inserted);
        }

        assert_eq!(value.upsert(principal(1), 1).unwrap(), Upsert::Updated);
        assert_eq!(value.upsert(principal(1), 1).unwrap(), Upsert::Unchanged);
        assert_eq!(
            value.upsert(principal(99), 0).unwrap_err(),
            "max delegations reached"
        );
    }

    #[test]
    fn sign_in_updates_only_authorized_active_delegators() {
        let mut value = delegations(&[(1, 0), (2, -1)]);

        value.record_sign_in(&principal(1), 42).unwrap();
        assert_eq!(value.0[0].sign_in_at, 42);
        assert_eq!(
            value.record_sign_in(&principal(2), 42).unwrap_err(),
            "delegator is suspended"
        );
        assert_eq!(
            value.record_sign_in(&principal(3), 42).unwrap_err(),
            "caller is not authorized"
        );
    }

    #[test]
    fn stable_delegation_encoding_remains_round_trip_compatible() {
        let value = delegations(&[(1, 1), (2, 0)]);
        let bytes = value.to_bytes();
        let decoded = Delegations::from_bytes(bytes);

        assert_eq!(decoded.0.len(), 2);
        assert_eq!(decoded.0[0].owner, principal(1));
        assert_eq!(decoded.0[1].role, 0);
    }
}
