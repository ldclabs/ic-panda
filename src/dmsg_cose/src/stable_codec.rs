use crate::{model, store::Config};
use cbor2::Cbor;
use dmsg_runtime::{stable_types::*, storage::StableCodec, Budget};
use dmsg_types::{cose::*, *};
use ic_cose_chain_key::PublicKey;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct PublicKeyRepr {
    #[cbor(key = 1)]
    #[serde(with = "serde_bytes")]
    pub public_key: Vec<u8>,
    #[cbor(key = 2)]
    #[serde(with = "serde_bytes")]
    pub chain_code: Vec<u8>,
}

fn public_key_to_repr(value: &PublicKey) -> PublicKeyRepr {
    PublicKeyRepr {
        public_key: value.public_key.clone(),
        chain_code: value.chain_code.clone(),
    }
}

fn public_key_from_repr(repr: PublicKeyRepr) -> PublicKey {
    PublicKey {
        public_key: repr.public_key,
        chain_code: repr.chain_code,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct ConfigRepr {
    #[cbor(key = 1)]
    pub schema: u16,
    #[cbor(key = 2)]
    pub state: KeyStateRepr,
    #[cbor(key = 3)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<PublicKeyRepr>,
}

impl StableCodec for Config {
    type Repr = ConfigRepr;

    fn to_repr(&self) -> Self::Repr {
        ConfigRepr {
            schema: self.schema,
            state: self.state.to_repr(),
            keys: self.keys.iter().map(public_key_to_repr).collect(),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            schema: repr.schema,
            state: KeyState::from_repr(repr.state),
            keys: repr.keys.into_iter().map(public_key_from_repr).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct ExecutionRepr {
    #[cbor(key = 1)]
    pub request_id: Hash,
    #[cbor(key = 2)]
    pub digest: Hash,
    #[cbor(key = 3)]
    pub expires_at: u64,
    #[cbor(key = 4)]
    pub terminal: bool,
}

impl StableCodec for model::Execution {
    type Repr = ExecutionRepr;

    fn to_repr(&self) -> Self::Repr {
        ExecutionRepr {
            request_id: self.request_id,
            digest: self.digest,
            expires_at: self.expires_at,
            terminal: self.terminal,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            request_id: repr.request_id,
            digest: repr.digest,
            expires_at: repr.expires_at,
            terminal: repr.terminal,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct HomeRepr {
    #[cbor(key = 1)]
    pub home_user: candid::Principal,
    #[cbor(key = 2)]
    pub terminal_sequence: u64,
    #[cbor(key = 3)]
    pub budget: BudgetRepr,
    #[cbor(key = 4)]
    pub executions: BTreeMap<u64, ExecutionRepr>,
}

impl StableCodec for model::Home {
    type Repr = HomeRepr;

    fn to_repr(&self) -> Self::Repr {
        HomeRepr {
            home_user: self.home_user,
            terminal_sequence: self.terminal_sequence,
            budget: self.budget.to_repr(),
            executions: self
                .executions
                .iter()
                .map(|(seq, e)| (*seq, e.to_repr()))
                .collect(),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            home_user: repr.home_user,
            terminal_sequence: repr.terminal_sequence,
            executions: repr
                .executions
                .into_iter()
                .map(|(seq, e)| (seq, model::Execution::from_repr(e)))
                .collect(),
            budget: Budget::from_repr(repr.budget),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candid::Principal;
    use dmsg_runtime::storage::{compact_bytes, compact_from_bytes};

    fn p(n: u8) -> Principal {
        Principal::from_slice(&[n, 1])
    }

    fn descriptor(account_id: AccountId, algorithm: Algorithm) -> KeyDescriptor {
        KeyDescriptor {
            key_id: vec![8; 32].into(),
            account_id,
            purpose: if algorithm == Algorithm::VetKdBls12381 {
                KeyPurpose::ContentRoot
            } else {
                KeyPurpose::Statement
            },
            algorithm,
            home_cose: p(4),
            master_key_name: "key_1".into(),
            environment: Environment::Production,
            derivation_version: 2,
            key_generation: 3,
            public_key: vec![9; 96].into(),
            public_key_fingerprint: Hash::new([10; 32]),
        }
    }

    fn grant(kind: ExecutionKind) -> ExecutionGrant {
        ExecutionGrant {
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
            kind,
            max_cycles: 100_000_000_000,
        }
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
    fn cose_home_and_all_execution_shapes_round_trip() {
        let mut home = model::Home::new(p(3));
        // Both unresolved holes and terminal entries must survive upgrades.
        for sequence in 1..=64 {
            home.executions.insert(
                sequence,
                model::Execution {
                    request_id: Hash::new([sequence as u8; 32]),
                    digest: Hash::new([7; 32]),
                    expires_at: 1_700_000_300_000,
                    terminal: sequence % 2 == 0,
                },
            );
        }
        let home_bytes = compact_bytes(&home);
        assert!(home_bytes.len() < 6 * 1024);
        assert_integer_top_keys(&home_bytes, 4);
        assert_eq!(compact_from_bytes::<model::Home>(&home_bytes), home);

        let derive_grant = grant(ExecutionKind::Derive {
            generation: 3,
            root_op_id: Some(Hash::new([6; 32])),
            transport_key: vec![7; 48].into(),
        });
        assert_public_text_keys(&derive_grant);
        let derive = ExecutionResult {
            request_id: derive_grant.request_id,
            outcome: ExecutionOutcome::Completed(Box::new(ExecutionOutput::EncryptedRootKey {
                encrypted_key: vec![12; 128].into(),
                key: descriptor(derive_grant.account_id.clone(), Algorithm::VetKdBls12381),
            })),
            charged_cycles: 100_000_000_000,
        };
        let derive_bytes = compact_bytes(&derive);
        assert_integer_top_keys(&derive_bytes, 3);
        assert_eq!(compact_from_bytes::<ExecutionResult>(&derive_bytes), derive);
        assert!(derive_bytes.len() < cbor2::to_vec(&derive).unwrap().len());

        let sign_grant = grant(ExecutionKind::Sign {
            key: KeyRequest {
                purpose: KeyPurpose::Statement,
                algorithm: Algorithm::Ed25519,
                generation: 1,
            },
            to_be_signed: vec![13; 192].into(),
            public_key_fingerprint: Hash::new([14; 32]),
            origin: "https://example.com".into(),
        });
        let signed = ExecutionResult {
            request_id: sign_grant.request_id,
            outcome: ExecutionOutcome::Completed(Box::new(ExecutionOutput::Signature {
                artifact: SignedArtifact {
                    cose_sign1: vec![16; 256].into(),
                    cose_key: vec![17; 96].into(),
                },
                key: descriptor(sign_grant.account_id.clone(), Algorithm::Ed25519),
            })),
            charged_cycles: 100_000_000_000,
        };
        assert_eq!(
            compact_from_bytes::<ExecutionResult>(&compact_bytes(&signed)),
            signed
        );

        for outcome in [
            ExecutionOutcome::Authorized,
            ExecutionOutcome::Executing,
            ExecutionOutcome::Failed(Error::IntegrityFailed),
            ExecutionOutcome::Unknown(Error::ExecutionUnknown),
            ExecutionOutcome::ResultExpired,
        ] {
            let execution = ExecutionResult {
                request_id: sign_grant.request_id,
                outcome,
                charged_cycles: 1,
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
            initial_home_user: p(3),
            derivation_version: 2,
            masters: vec![MasterKey {
                algorithm: Algorithm::Ed25519,
                key_name: "key_1".into(),
                expected_fingerprint: Hash::new([1; 32]),
            }],
            daily_executions: 100,
            daily_cycles: 1_000_000_000_000,
        };
        let config = Config {
            schema: crate::store::STABLE_SCHEMA,
            state: KeyState {
                config: init,
                initialization: Initialization::Ready,
                fingerprints: vec![Hash::new([2; 32])],
                error: Some("last transient error".into()),
            },
            keys: vec![PublicKey {
                public_key: vec![3; 32],
                chain_code: vec![4; 32],
            }],
        };
        let decoded = compact_from_bytes::<Config>(&compact_bytes(&config));
        assert_eq!(decoded.schema, config.schema);
        assert_eq!(decoded.state.config, config.state.config);
        assert_eq!(decoded.state.initialization, config.state.initialization);
        assert_eq!(decoded.state.fingerprints, config.state.fingerprints);
        assert_eq!(decoded.state.error, config.state.error);
        assert_eq!(decoded.keys, config.keys);

        let mut sparse = config;
        sparse.state.initialization = Initialization::Uninitialized;
        sparse.state.fingerprints.clear();
        sparse.state.error = None;
        sparse.keys.clear();
        let sparse_decoded = compact_from_bytes::<Config>(&compact_bytes(&sparse));
        assert_eq!(sparse_decoded.state.fingerprints, sparse.state.fingerprints);
        assert_eq!(sparse_decoded.state.error, sparse.state.error);
        assert_eq!(sparse_decoded.keys, sparse.keys);
    }
}
