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
    #[cbor(key = 4)]
    pub budget: BudgetRepr,
}

impl StableCodec for Config {
    type Repr = ConfigRepr;

    fn to_repr(&self) -> Self::Repr {
        ConfigRepr {
            schema: self.schema,
            state: self.state.to_repr(),
            keys: self.keys.iter().map(public_key_to_repr).collect(),
            budget: self.budget.to_repr(),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            schema: repr.schema,
            state: KeyState::from_repr(repr.state),
            keys: repr.keys.into_iter().map(public_key_from_repr).collect(),
            budget: Budget::from_repr(repr.budget),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct ExecutionRepr {
    #[cbor(key = 1)]
    pub grant: ExecutionGrantRepr,
    #[cbor(key = 2)]
    pub digest: Hash,
    #[cbor(key = 3)]
    pub result: ExecutionResultRepr,
}

impl StableCodec for model::Execution {
    type Repr = ExecutionRepr;

    fn to_repr(&self) -> Self::Repr {
        ExecutionRepr {
            grant: self.grant.to_repr(),
            digest: self.digest,
            result: self.result.to_repr(),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            grant: ExecutionGrant::from_repr(repr.grant),
            digest: repr.digest,
            result: ExecutionResult::from_repr(repr.result),
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
}

impl StableCodec for model::Home {
    type Repr = HomeRepr;

    fn to_repr(&self) -> Self::Repr {
        HomeRepr {
            home_user: self.home_user,
            terminal_sequence: self.terminal_sequence,
            budget: self.budget.to_repr(),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            home_user: repr.home_user,
            terminal_sequence: repr.terminal_sequence,
            executions: BTreeMap::new(),
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
        let home = model::Home::new(p(3));
        let home_bytes = compact_bytes(&home);
        assert_integer_top_keys(&home_bytes, 3);
        assert_eq!(compact_from_bytes::<model::Home>(&home_bytes), home);

        let derive_grant = grant(ExecutionKind::Derive {
            generation: 3,
            root_op_id: Some(Hash::new([6; 32])),
            transport_key: vec![7; 48].into(),
        });
        assert_public_text_keys(&derive_grant);
        let derive = model::Execution {
            grant: derive_grant.clone(),
            digest: Hash::new([11; 32]),
            result: ExecutionResult {
                request_id: derive_grant.request_id,
                outcome: ExecutionOutcome::Completed(Box::new(ExecutionOutput::EncryptedRootKey {
                    encrypted_key: vec![12; 128].into(),
                    key: descriptor(derive_grant.account_id.clone(), Algorithm::VetKdBls12381),
                })),
                charged_cycles: 100_000_000_000,
            },
        };
        let derive_bytes = compact_bytes(&derive);
        assert_eq!(derive_bytes.len(), 714);
        assert_eq!(
            hex(&derive_bytes),
            "e928815c8bc48a437fb3898c2b5282b0173d06da1c8f48e53ebd2f5c05d54b61"
        );
        assert_integer_top_keys(&derive_bytes, 3);
        assert_eq!(
            compact_from_bytes::<model::Execution>(&derive_bytes),
            derive
        );
        assert!(derive_bytes.len() * 100 <= cbor2::to_vec(&derive).unwrap().len() * 70);

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
        let signed = model::Execution {
            grant: sign_grant.clone(),
            digest: Hash::new([15; 32]),
            result: ExecutionResult {
                request_id: sign_grant.request_id,
                outcome: ExecutionOutcome::Completed(Box::new(ExecutionOutput::Signature {
                    artifact: SignedArtifact {
                        cose_sign1: vec![16; 256].into(),
                        cose_key: vec![17; 96].into(),
                    },
                    key: descriptor(sign_grant.account_id.clone(), Algorithm::Ed25519),
                })),
                charged_cycles: 100_000_000_000,
            },
        };
        assert_eq!(
            compact_from_bytes::<model::Execution>(&compact_bytes(&signed)),
            signed
        );

        for outcome in [
            ExecutionOutcome::Authorized,
            ExecutionOutcome::Executing,
            ExecutionOutcome::Failed(Error::IntegrityFailed),
            ExecutionOutcome::Unknown(Error::ExecutionUnknown),
            ExecutionOutcome::ResultExpired,
        ] {
            let execution = model::Execution {
                grant: sign_grant.clone(),
                digest: Hash::new([18; 32]),
                result: ExecutionResult {
                    request_id: sign_grant.request_id,
                    outcome,
                    charged_cycles: 1,
                },
            };
            assert_eq!(
                compact_from_bytes::<model::Execution>(&compact_bytes(&execution)),
                execution
            );
        }
    }

    fn hex(bytes: &[u8]) -> String {
        let digest = dmsg_protocol::sha256(bytes);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
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
            schema: 3,
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
            budget: Budget {
                day: 42,
                executions: 7,
                cycles: 123,
            },
        };
        let decoded = compact_from_bytes::<Config>(&compact_bytes(&config));
        assert_eq!(decoded.schema, config.schema);
        assert_eq!(decoded.state.config, config.state.config);
        assert_eq!(decoded.state.initialization, config.state.initialization);
        assert_eq!(decoded.state.fingerprints, config.state.fingerprints);
        assert_eq!(decoded.state.error, config.state.error);
        assert_eq!(decoded.keys, config.keys);
        assert_eq!(decoded.budget, config.budget);

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
