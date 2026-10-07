use crate::{model, store::Config};
use cbor2::Cbor;
use dmsg_runtime::{stable_types::*, storage::StableCodec, Budget};
use dmsg_types::cose::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct ConfigRepr {
    #[cbor(key = 1)]
    pub schema: u16,
    #[cbor(key = 2)]
    pub state: KeyStateRepr,
    #[cbor(key = 4)]
    #[serde(default, skip_serializing_if = "Option::is_none", with = "serde_bytes")]
    pub public_key: Option<Vec<u8>>,
}

impl StableCodec for Config {
    type Repr = ConfigRepr;

    fn to_repr(&self) -> Self::Repr {
        ConfigRepr {
            schema: self.schema,
            state: self.state.to_repr(),
            public_key: self.public_key.clone(),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            schema: repr.schema,
            state: KeyState::from_repr(repr.state),
            public_key: repr.public_key,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct GlobalRepr {
    #[cbor(key = 1)]
    budget: BudgetRepr,
    #[cbor(key = 3)]
    #[serde(default, skip_serializing_if = "is_zero")]
    unknown: u64,
}

impl StableCodec for model::Global {
    type Repr = GlobalRepr;

    fn to_repr(&self) -> Self::Repr {
        GlobalRepr {
            budget: self.budget.to_repr(),
            unknown: self.unknown,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            budget: Budget::from_repr(repr.budget),
            unknown: repr.unknown,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct HomeRepr {
    #[cbor(key = 2)]
    pub closed_sequence: u64,
    #[cbor(key = 5)]
    pub budget: BudgetRepr,
    /// Execution metadata is private and already uses integer keys.
    #[cbor(key = 4)]
    pub executions: BTreeMap<u64, model::Execution>,
}

impl StableCodec for model::Home {
    type Repr = HomeRepr;

    fn to_repr(&self) -> Self::Repr {
        HomeRepr {
            closed_sequence: self.closed_sequence,
            budget: self.budget.to_repr(),
            executions: self.executions.clone(),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            closed_sequence: repr.closed_sequence,
            budget: Budget::from_repr(repr.budget),
            executions: repr.executions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candid::Principal;
    use dmsg_runtime::storage::{compact_bytes, compact_from_bytes};
    use dmsg_types::*;

    fn p(n: u8) -> Principal {
        Principal::from_slice(&[n, 1])
    }

    fn assert_integer_top_keys(bytes: &[u8], count: usize) {
        let cbor2::Value::Map(entries) = cbor2::from_slice(bytes).unwrap() else {
            panic!("stable record must be a map")
        };
        assert_eq!(entries.len(), count);
        assert!(entries
            .iter()
            .all(|(key, _)| matches!(key, cbor2::Value::Integer(_))));
    }

    fn assert_public_text_keys<T: serde::Serialize>(value: &T) {
        let cbor2::Value::Map(entries) = cbor2::from_slice(&cbor2::to_vec(value).unwrap()).unwrap()
        else {
            panic!("public record must be a map")
        };
        assert!(entries
            .iter()
            .all(|(key, _)| matches!(key, cbor2::Value::Text(_))));
    }

    #[test]
    fn cose_home_and_execution_shapes_round_trip() {
        let mut home = model::Home::default();
        // Both unresolved holes and terminal entries must survive upgrades.
        for sequence in 1..=64 {
            home.executions.insert(
                sequence,
                model::Execution {
                    request_id: Hash::new([sequence as u8; 32]),
                    digest: Hash::new([7; 32]),
                    expires_at: 1_700_000_300_000,
                    state: match sequence % 3 {
                        0 => model::ExecutionState::InFlight,
                        1 => model::ExecutionState::Unknown,
                        _ => model::ExecutionState::Terminal,
                    },
                },
            );
        }
        home.budget.reserve(2 * DAY, 20, 10, 100).unwrap();
        let home_bytes = compact_bytes(&home);
        assert!(home_bytes.len() < 6 * 1024);
        assert_integer_top_keys(&home_bytes, 3);
        assert_integer_top_keys(&cbor2::to_vec(&home.executions[&1]).unwrap(), 4);
        assert_eq!(compact_from_bytes::<model::Home>(&home_bytes), home);

        let grant = ExecutionGrant {
            account_id: AccountId([1; 12]),
            home_user: p(3),
            home_cose: p(4),
            request_id: Hash::new([2; 32]),
            execution_sequence: 42,
            security_epoch: 7,
            device_id: Hash::new([5; 32]),
            device_sequence: 11,
            approved_at: 1_700_000_000_000,
            expires_at: 1_700_000_300_000,
            generation: 3,
            transport_key: [7; 48].into(),
            max_cycles: 70_000_000_000,
        };
        assert_public_text_keys(&grant);
        let derive = ExecutionResult {
            request_id: grant.request_id,
            outcome: ExecutionOutcome::Completed(EncryptedRootKey {
                encrypted_key: vec![12; 128].into(),
                key: KeyDescriptor {
                    account_id: grant.account_id,
                    home_cose: p(4),
                    master_key_name: "key_1".into(),
                    environment: Environment::Production,
                    derivation_version: 2,
                    key_generation: 3,
                    public_key: vec![9; 96].into(),
                    public_key_fingerprint: Hash::new([10; 32]),
                },
            }),
            cycles_cost_upper_bound: 70_000_000_000,
            cycles_charged: 26_000_000_000,
        };
        let derive_bytes = compact_bytes(&derive);
        assert_integer_top_keys(&derive_bytes, 4);
        assert_eq!(compact_from_bytes::<ExecutionResult>(&derive_bytes), derive);
        assert!(derive_bytes.len() < cbor2::to_vec(&derive).unwrap().len());
        for outcome in [
            ExecutionOutcome::Authorized,
            ExecutionOutcome::Executing,
            ExecutionOutcome::Failed(Error::IntegrityFailed),
            ExecutionOutcome::Unknown(Error::ExecutionUnknown),
            ExecutionOutcome::ResultExpired,
        ] {
            let execution = ExecutionResult {
                request_id: grant.request_id,
                outcome,
                cycles_cost_upper_bound: 1,
                cycles_charged: 0,
            };
            assert_eq!(
                compact_from_bytes::<ExecutionResult>(&compact_bytes(&execution)),
                execution
            );
        }
    }

    #[test]
    fn cose_config_round_trips() {
        let init = CoseInit {
            issuer_namespace: "https://dmsg.example/u/".into(),
            environment: Environment::Production,
            executing_canister: p(4),
            user_homes: vec![p(3), p(5)],
            derivation_version: 2,
            master: MasterKey {
                key_name: "key_1".into(),
                expected_fingerprint: Hash::new([1; 32]),
            },
            daily_executions: 100,
            daily_cycles: 1_000_000_000_000,
            governance: p(6),
        };
        let config = Config {
            schema: crate::store::STABLE_SCHEMA,
            state: KeyState {
                config: init,
                initialization: Initialization::Ready,
                fingerprint: Some(Hash::new([2; 32])),
                error: Some("last transient error".into()),
            },
            public_key: Some(vec![3; 96]),
        };
        let decoded = compact_from_bytes::<Config>(&compact_bytes(&config));
        assert_eq!(decoded.schema, config.schema);
        assert_eq!(decoded.state, config.state);
        assert_eq!(decoded.public_key, config.public_key);

        let mut sparse = config;
        sparse.state.initialization = Initialization::Uninitialized;
        sparse.state.fingerprint = None;
        sparse.state.error = None;
        sparse.public_key = None;
        let sparse_decoded = compact_from_bytes::<Config>(&compact_bytes(&sparse));
        assert_eq!(sparse_decoded.state, sparse.state);
        assert_eq!(sparse_decoded.public_key, None);
    }

    #[test]
    fn global_state_fits_the_existing_page() {
        let mut budget = Budget::default();
        budget.reserve(2 * DAY, 20, 10, 100).unwrap();
        let global = model::Global {
            budget,
            unknown: u64::MAX,
        };
        let encoded = compact_bytes(&global);
        assert!(encoded.len() < 128);
        assert_eq!(compact_from_bytes::<model::Global>(&encoded), global);
    }
}
