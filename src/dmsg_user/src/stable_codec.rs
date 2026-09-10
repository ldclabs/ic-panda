use crate::{state::*, store::Config};
use cbor2::Cbor;
use dmsg_runtime::{stable_types::*, storage::StableCodec, Budget};
use dmsg_types::{cose::*, handle::*, user::*, *};
use ic_auth_types::XidGenerator;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct XidGeneratorRepr {
    #[cbor(key = 1)]
    pub profile_version: u8,
    #[cbor(key = 2)]
    pub fingerprint: [u8; 5],
    #[cbor(key = 3)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_second: Option<u32>,
    #[cbor(key = 4)]
    pub next_counter: u32,
}

fn allocator_to_repr(value: &XidGenerator) -> XidGeneratorRepr {
    XidGeneratorRepr {
        profile_version: value.profile_version,
        fingerprint: value.fingerprint,
        last_second: value.last_second,
        next_counter: value.next_counter,
    }
}

fn allocator_from_repr(repr: XidGeneratorRepr) -> XidGenerator {
    XidGenerator {
        profile_version: repr.profile_version,
        fingerprint: repr.fingerprint,
        last_second: repr.last_second,
        next_counter: repr.next_counter,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct ConfigRepr {
    #[cbor(key = 1)]
    pub schema: u16,
    #[cbor(key = 2)]
    pub init: UserInitRepr,
    #[cbor(key = 3)]
    pub allocator: XidGeneratorRepr,
    #[cbor(key = 4)]
    pub allocator_namespace_digest: Hash,
    #[cbor(key = 5)]
    pub day: u64,
    #[cbor(key = 6)]
    pub created_today: u32,
}

impl StableCodec for Config {
    type Repr = ConfigRepr;

    fn to_repr(&self) -> Self::Repr {
        ConfigRepr {
            schema: self.schema,
            init: self.init.to_repr(),
            allocator: allocator_to_repr(&self.allocator),
            allocator_namespace_digest: self.allocator_namespace_digest,
            day: self.day,
            created_today: self.created_today,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            schema: repr.schema,
            init: UserInit::from_repr(repr.init),
            allocator: allocator_from_repr(repr.allocator),
            allocator_namespace_digest: repr.allocator_namespace_digest,
            day: repr.day,
            created_today: repr.created_today,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct HandleAuthorizationRepr {
    #[cbor(key = 1)]
    pub intent: HandleIntentRepr,
    #[cbor(key = 2)]
    pub expires_at: u64,
}

impl StableCodec for HandleAuthorization {
    type Repr = HandleAuthorizationRepr;

    fn to_repr(&self) -> Self::Repr {
        HandleAuthorizationRepr {
            intent: self.intent.to_repr(),
            expires_at: self.expires_at,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            intent: HandleIntent::from_repr(repr.intent),
            expires_at: repr.expires_at,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct AccountStateRepr {
    #[cbor(key = 1)]
    pub account_id: AccountId,
    #[cbor(key = 2)]
    pub home_user: candid::Principal,
    #[cbor(key = 3)]
    pub home_cose: candid::Principal,
    #[cbor(key = 4)]
    pub auth_bindings: Vec<candid::Principal>,
    #[cbor(key = 5)]
    pub account_version: u64,
    #[cbor(key = 6)]
    pub security_epoch: u64,
    #[cbor(key = 7)]
    pub status: AccountStatus,
    #[cbor(key = 8)]
    pub devices: BTreeMap<Hash, DeviceRepr>,
    #[cbor(key = 9)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<RecoveryPolicyRepr>,
    #[cbor(key = 10)]
    pub recovery_checked: bool,
    #[cbor(key = 11)]
    pub recovery_nonce: u64,
    #[cbor(key = 12)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_recovery: Option<PendingRecoveryRepr>,
    #[cbor(key = 13)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_root: Option<ContentRootRefRepr>,
    #[cbor(key = 14)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_slot: Option<RootReservationRepr>,
    #[cbor(key = 15)]
    pub next_root_generation: u64,
    #[cbor(key = 16)]
    pub vault_write_state: VaultWriteState,
    #[cbor(key = 17)]
    pub sensitive_policy: SensitivePolicyRepr,
    #[cbor(key = 18)]
    pub budget: BudgetRepr,
    #[cbor(key = 19)]
    pub next_execution_sequence: u64,
    #[cbor(key = 20)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<OperationReceiptRepr>,
    #[cbor(key = 21)]
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub handle_authorizations: BTreeMap<OpId, HandleAuthorizationRepr>,
    #[cbor(key = 22)]
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub execution_expirations: BTreeMap<OpId, Option<u64>>,
}

impl StableCodec for AccountState {
    type Repr = AccountStateRepr;

    fn to_repr(&self) -> Self::Repr {
        AccountStateRepr {
            account_id: self.account_id.clone(),
            home_user: self.home_user,
            home_cose: self.home_cose,
            auth_bindings: self.auth_bindings.clone(),
            account_version: self.account_version,
            security_epoch: self.security_epoch,
            status: self.status.clone(),
            devices: map_to_repr(&self.devices),
            recovery: self.recovery.as_ref().map(StableCodec::to_repr),
            recovery_checked: self.recovery_checked,
            recovery_nonce: self.recovery_nonce,
            pending_recovery: self.pending_recovery.as_ref().map(StableCodec::to_repr),
            current_root: self.current_root.as_ref().map(StableCodec::to_repr),
            root_slot: self.root_slot.as_ref().map(StableCodec::to_repr),
            next_root_generation: self.next_root_generation,
            vault_write_state: self.vault_write_state.clone(),
            sensitive_policy: self.sensitive_policy.to_repr(),
            budget: self.budget.to_repr(),
            next_execution_sequence: self.next_execution_sequence,
            operations: self.operations.iter().map(StableCodec::to_repr).collect(),
            handle_authorizations: map_to_repr(&self.handle_authorizations),
            execution_expirations: self.execution_expirations.clone(),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            account_id: repr.account_id,
            home_user: repr.home_user,
            home_cose: repr.home_cose,
            auth_bindings: repr.auth_bindings,
            account_version: repr.account_version,
            security_epoch: repr.security_epoch,
            status: repr.status,
            devices: map_from_repr(repr.devices),
            recovery: repr.recovery.map(RecoveryPolicy::from_repr),
            recovery_checked: repr.recovery_checked,
            recovery_nonce: repr.recovery_nonce,
            pending_recovery: repr.pending_recovery.map(PendingRecovery::from_repr),
            current_root: repr.current_root.map(ContentRootRef::from_repr),
            root_slot: repr.root_slot.map(RootReservation::from_repr),
            next_root_generation: repr.next_root_generation,
            vault_write_state: repr.vault_write_state,
            sensitive_policy: SensitivePolicy::from_repr(repr.sensitive_policy),
            budget: Budget::from_repr(repr.budget),
            next_execution_sequence: repr.next_execution_sequence,
            operations: repr
                .operations
                .into_iter()
                .map(OperationReceipt::from_repr)
                .collect(),
            handle_authorizations: map_from_repr(repr.handle_authorizations),
            execution_expirations: repr.execution_expirations,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct AuthorizedExecutionRepr {
    #[cbor(key = 1)]
    pub grant: ExecutionGrantRepr,
    #[cbor(key = 2)]
    pub command_digest: Hash,
    #[cbor(key = 3)]
    pub result: ExecutionResultRepr,
}

impl StableCodec for AuthorizedExecution {
    type Repr = AuthorizedExecutionRepr;

    fn to_repr(&self) -> Self::Repr {
        AuthorizedExecutionRepr {
            grant: self.grant.to_repr(),
            command_digest: self.command_digest,
            result: self.result.to_repr(),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            grant: ExecutionGrant::from_repr(repr.grant),
            command_digest: repr.command_digest,
            result: ExecutionResult::from_repr(repr.result),
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

    fn device(n: u8) -> Device {
        Device {
            input: DeviceInput {
                device_id: Hash::new([n; 32]),
                signing_pub: Hash::new([n.wrapping_add(1); 32]),
                hpke_pub: Hash::new([n.wrapping_add(2); 32]),
                role: ControllerRole::Administrator,
                capabilities: vec![
                    Capability::ContentSign,
                    Capability::VaultUnlock,
                    Capability::RootManage,
                    Capability::FormalApprove,
                    Capability::PaymentOffer,
                ],
            },
            added_at: 1_700_000_000_000 + u64::from(n),
            added_by: (n != 1).then_some(Hash::new([1; 32])),
            revoked_at: (n == 16).then_some(1_700_000_100_000),
            next_sequence: 42,
        }
    }

    fn account(populated: bool) -> AccountState {
        let mut state = AccountState {
            account_id: AccountId([8; 12]),
            home_user: p(5),
            home_cose: p(6),
            auth_bindings: vec![p(1)],
            account_version: 0,
            security_epoch: 0,
            status: AccountStatus::Active,
            devices: BTreeMap::from([(Hash::new([1; 32]), device(1))]),
            recovery: None,
            recovery_checked: false,
            recovery_nonce: 0,
            pending_recovery: None,
            current_root: None,
            root_slot: None,
            next_root_generation: 1,
            vault_write_state: VaultWriteState::Uninitialized,
            sensitive_policy: SensitivePolicy::default(),
            budget: Budget::default(),
            next_execution_sequence: 1,
            operations: vec![],
            handle_authorizations: BTreeMap::new(),
            execution_expirations: BTreeMap::new(),
        };
        if !populated {
            return state;
        }
        state.auth_bindings = (1..=8).map(p).collect();
        state.devices = (1..=16).map(|n| (Hash::new([n; 32]), device(n))).collect();
        state.account_version = 1_000;
        state.security_epoch = 20;
        state.recovery = Some(RecoveryPolicy {
            generation: 3,
            signing_pub: Hash::new([31; 32]),
            hpke_pub: Hash::new([32; 32]),
            delay_ms: DAY,
        });
        state.recovery_checked = true;
        state.recovery_nonce = 9;
        state.pending_recovery = Some(PendingRecovery {
            request: RecoveryRequest {
                op_id: Hash::new([33; 32]),
                new_auth: p(9),
                device: device(9).input,
                generation: 3,
                expires_at: 1_700_086_400_000,
            },
            execute_after: 1_700_086_400_000,
            dispute: Some(Hash::new([34; 32])),
            reconfirmed: true,
            confirmation: Some(RecoveryConfirmation {
                request_id: Hash::new([35; 32]),
                dispute: Hash::new([34; 32]),
                expires_at: 1_700_172_800_000,
            }),
        });
        state.current_root = Some(ContentRootRef {
            generation: 7,
            suite: "dmsg-root-v1".into(),
            home_cose: state.home_cose,
            derivation_version: 2,
            key_generation: 7,
            bundle_digest: Hash::new([36; 32]),
            recovery_generation: 3,
        });
        state.root_slot = Some(RootReservation {
            op_id: Hash::new([37; 32]),
            expected_generation: 7,
            generation: 8,
            security_epoch: 20,
            expires_at: 1_700_000_900_000,
        });
        state.vault_write_state = VaultWriteState::Ready;
        state.operations = (0..64)
            .map(|n| OperationReceipt {
                id: Hash::new([n; 32]),
                digest: Hash::new([n.wrapping_add(1); 32]),
                account_version: u64::from(n),
            })
            .collect();
        state.handle_authorizations = (0..32)
            .map(|n| {
                let op_id = Hash::new([n + 64; 32]);
                (
                    op_id,
                    HandleAuthorization {
                        intent: HandleIntent {
                            handle_canister: p(7),
                            action: HandleAction::Register,
                            account_id: state.account_id.clone(),
                            target_account: Some(AccountId([2; 12])),
                            handle: format!("user-{n:02}"),
                            expected_version: u64::from(n),
                            op_id,
                            terms_digest: Hash::new([n + 96; 32]),
                        },
                        expires_at: 1_700_000_060_000,
                    },
                )
            })
            .collect();
        state.execution_expirations = (0..64)
            .map(|n| {
                (
                    Hash::new([n; 32]),
                    (n % 2 == 0).then_some(1_700_086_460_000),
                )
            })
            .collect();
        state
    }

    fn top_keys(bytes: &[u8]) -> Vec<i128> {
        let cbor2::Value::Map(entries) = cbor2::from_slice(bytes).unwrap() else {
            panic!("stable record must be a map")
        };
        entries
            .into_iter()
            .map(|(key, _)| match key {
                cbor2::Value::Integer(value) => value.into(),
                other => panic!("non-integer stable field key: {other:?}"),
            })
            .collect()
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
    fn account_state_round_trips_at_sparse_and_bounded_shapes() {
        let sparse = account(false);
        assert_public_text_keys(&sparse.devices.values().next().unwrap().input);
        let sparse_bytes = compact_bytes(&sparse);
        assert_eq!(compact_from_bytes::<AccountState>(&sparse_bytes), sparse);
        assert_eq!(
            top_keys(&sparse_bytes),
            (1..=8).chain(10..=11).chain(15..=19).collect::<Vec<_>>()
        );

        let full = account(true);
        let compact = compact_bytes(&full);
        assert_eq!(compact.len(), 17_642);
        assert_eq!(
            hex(&compact),
            "d223f6e9440aca419299e68281851295c280c92538465ac5b25d17b9e2efa74f"
        );
        let plain = cbor2::to_vec(&full).unwrap();
        assert_eq!(compact_from_bytes::<AccountState>(&compact), full);
        assert_eq!(top_keys(&compact), (1..=22).collect::<Vec<_>>());
        // The retention index consists of raw IDs/timestamps in both encodings;
        // integer field keys do not shrink those shared bytes.
        assert!(
            compact.len() * 100 <= plain.len() * 75,
            "{} !<= 75% of {}",
            compact.len(),
            plain.len()
        );
    }

    fn hex(bytes: &[u8]) -> String {
        let digest = dmsg_protocol::sha256(bytes);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn config_and_authorized_execution_round_trip() {
        let config = Config {
            schema: crate::store::STABLE_SCHEMA,
            init: UserInit {
                environment: Environment::Production,
                issuer_namespace: "https://dmsg.example/u/".into(),
                home_cose: p(2),
                handle_canister: p(3),
                payment_canister: p(4),
                max_accounts: 1_000_000,
                daily_new_accounts: 10_000,
            },
            allocator: XidGenerator::new([1, 2, 3, 4, 5]),
            allocator_namespace_digest: Hash::new([6; 32]),
            day: 42,
            created_today: 7,
        };
        assert_eq!(
            compact_from_bytes::<Config>(&compact_bytes(&config)).schema,
            crate::store::STABLE_SCHEMA
        );

        let state = account(false);
        let execution = AuthorizedExecution {
            grant: ExecutionGrant {
                account_id: state.account_id,
                home_user: state.home_user,
                home_cose: state.home_cose,
                request_id: Hash::new([7; 32]),
                execution_sequence: 9,
                security_epoch: 2,
                device_id: Hash::new([8; 32]),
                device_sequence: 4,
                approved_at: 100,
                expires_at: 200,
                kind: ExecutionKind::Derive {
                    generation: 3,
                    root_op_id: Some(Hash::new([9; 32])),
                    transport_key: vec![10; 48].into(),
                },
                max_cycles: 1_000,
            },
            command_digest: Hash::new([11; 32]),
            result: ExecutionResult {
                request_id: Hash::new([7; 32]),
                outcome: ExecutionOutcome::Executing,
                charged_cycles: 1_000,
            },
        };
        let bytes = compact_bytes(&execution);
        assert_eq!(compact_from_bytes::<AuthorizedExecution>(&bytes), execution);
        assert!(bytes.len() * 100 <= cbor2::to_vec(&execution).unwrap().len() * 65);
    }

    #[test]
    fn bounded_accounts_reduce_real_stable_btree_allocation() {
        use dmsg_runtime::storage::{CompactStored, Stored};
        use ic_stable_structures::{StableBTreeMap, VectorMemory};

        let plain_memory = VectorMemory::default();
        let compact_memory = VectorMemory::default();
        let mut plain =
            StableBTreeMap::<Vec<u8>, Stored<AccountState>, _>::init(plain_memory.clone());
        let mut compact =
            StableBTreeMap::<Vec<u8>, CompactStored<AccountState>, _>::init(compact_memory.clone());

        for n in 0..128_u64 {
            let mut state = account(true);
            let mut raw_id = [0_u8; 12];
            raw_id[4..].copy_from_slice(&n.to_be_bytes());
            state.account_id = AccountId(raw_id);
            let key = state.account_id.to_vec();
            plain.insert(key.clone(), Stored(state.clone()));
            compact.insert(key, CompactStored(state));
        }

        let last_key = {
            let mut key = vec![0_u8; 12];
            key[4..].copy_from_slice(&127_u64.to_be_bytes());
            key
        };
        assert_eq!(
            plain.get(&last_key).unwrap().0.account_id,
            compact.get(&last_key).unwrap().0.account_id
        );

        drop(plain);
        drop(compact);
        let plain_bytes = plain_memory.borrow().len();
        let compact_bytes = compact_memory.borrow().len();
        assert!(
            compact_bytes * 100 <= plain_bytes * 75,
            "compact allocation {compact_bytes} is not at least 25% below {plain_bytes}"
        );
    }
}
