use crate::{commerce::Month, principal::AgentPrincipal, state::*, store::Config};
use cbor2::Cbor;
use dmsg_runtime::{stable_types::*, storage::StableCodec, Budget};
use dmsg_types::{agent::*, billing::*, cose::*, handle::*, user::*, *};
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
    #[cbor(key = 7)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master_secret: Option<Hash>,
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
            master_secret: self.master_secret,
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
            master_secret: repr.master_secret,
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

// Flatten the small monthly ledger. Full resource projections and timeline
// segments are validated on refresh, not rewritten on every charge.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct MonthRepr {
    #[cbor(key = 1)]
    pub account_id: AccountId,
    #[cbor(key = 2)]
    pub month_utc: u32,
    #[cbor(key = 3)]
    pub month_revision: u64,
    #[cbor(key = 4)]
    pub business_revision: u64,
    #[cbor(key = 5)]
    pub lease_revision: u64,
    #[cbor(key = 6)]
    pub weight_policy_version: u64,
    #[cbor(key = 7)]
    pub allowed_units: u64,
    #[cbor(key = 9)]
    pub charged_units: u64,
    #[cbor(key = 10)]
    pub valid_until_ms: u64,
    #[cbor(key = 11)]
    pub ed25519_units: u64,
    #[cbor(key = 13)]
    pub entitlement_digest: Hash,
}

impl StableCodec for Month {
    type Repr = MonthRepr;

    fn to_repr(&self) -> Self::Repr {
        MonthRepr {
            account_id: self.usage.account_id,
            month_utc: self.usage.month_utc,
            month_revision: self.usage.month_revision,
            business_revision: self.usage.business_revision,
            lease_revision: self.usage.lease_revision,
            weight_policy_version: self.usage.weight_policy_version,
            allowed_units: self.usage.allowed_units,
            charged_units: self.usage.charged_units,
            valid_until_ms: self.usage.valid_until_ms,
            ed25519_units: self.weights.ed25519,
            entitlement_digest: self.entitlement_digest,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            usage: ExecutionUsage {
                account_id: repr.account_id,
                month_utc: repr.month_utc,
                month_revision: repr.month_revision,
                business_revision: repr.business_revision,
                lease_revision: repr.lease_revision,
                weight_policy_version: repr.weight_policy_version,
                allowed_units: repr.allowed_units,
                charged_units: repr.charged_units,
                valid_until_ms: repr.valid_until_ms,
            },
            weights: ExecutionWeights {
                version: repr.weight_policy_version,
                ed25519: repr.ed25519_units,
            },
            entitlement_digest: repr.entitlement_digest,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct RecoveryReceiptRepr {
    #[cbor(key = 1)]
    pub request_id: OpId,
    #[cbor(key = 2)]
    pub new_auth: candid::Principal,
    #[cbor(key = 3)]
    pub device_id: Hash,
    #[cbor(key = 4)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_generation: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct PendingBindingRepr {
    #[cbor(key = 1)]
    pub principal: candid::Principal,
    #[cbor(key = 2)]
    pub nonce: Hash,
    #[cbor(key = 3)]
    pub expires_at: u64,
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
    #[cbor(key = 8)]
    pub devices: BTreeMap<Hash, DeviceRepr>,
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
    #[cbor(key = 23)]
    pub created_at_ms: u64,
    #[cbor(key = 25)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_updated_at: Option<u64>,
    #[cbor(key = 27)]
    pub recovery_delay_ms: u64,
    #[cbor(key = 28)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_recovery: Option<RecoveryReceiptRepr>,
    #[cbor(key = 29)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_bindings: Vec<PendingBindingRepr>,
}

impl StableCodec for AccountState {
    type Repr = AccountStateRepr;

    fn to_repr(&self) -> Self::Repr {
        AccountStateRepr {
            created_at_ms: self.created_at_ms,
            account_id: self.account_id,
            home_user: self.home_user,
            home_cose: self.home_cose,
            auth_bindings: self.auth_bindings.clone(),
            pending_bindings: self
                .pending_bindings
                .iter()
                .map(|b| PendingBindingRepr {
                    principal: b.principal,
                    nonce: b.nonce,
                    expires_at: b.expires_at,
                })
                .collect(),
            account_version: self.account_version,
            security_epoch: self.security_epoch,
            devices: map_to_repr(&self.devices),
            recovery_delay_ms: self.recovery_delay_ms,
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
            principal_updated_at: self.principal_updated_at,
            completed_recovery: self.completed_recovery.as_ref().map(|r| RecoveryReceiptRepr {
                request_id: r.request_id,
                new_auth: r.new_auth,
                device_id: r.device_id,
                root_generation: r.root_generation,
            }),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            created_at_ms: repr.created_at_ms,
            account_id: repr.account_id,
            home_user: repr.home_user,
            home_cose: repr.home_cose,
            auth_bindings: repr.auth_bindings,
            pending_bindings: repr
                .pending_bindings
                .into_iter()
                .map(|b| PendingBinding {
                    principal: b.principal,
                    nonce: b.nonce,
                    expires_at: b.expires_at,
                })
                .collect(),
            account_version: repr.account_version,
            security_epoch: repr.security_epoch,
            devices: map_from_repr(repr.devices),
            recovery_delay_ms: repr.recovery_delay_ms,
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
            principal_updated_at: repr.principal_updated_at,
            completed_recovery: repr.completed_recovery.map(|r| RecoveryReceipt {
                request_id: r.request_id,
                new_auth: r.new_auth,
                device_id: r.device_id,
                root_generation: r.root_generation,
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct AgentPrincipalRepr {
    #[cbor(key = 1)]
    pub state: PrincipalStateRepr,
    #[cbor(key = 2)]
    pub published_version: u64,
}

impl StableCodec for AgentPrincipal {
    type Repr = AgentPrincipalRepr;

    fn to_repr(&self) -> Self::Repr {
        AgentPrincipalRepr {
            state: self.state.to_repr(),
            published_version: self.published_version,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            state: PrincipalState::from_repr(repr.state),
            published_version: repr.published_version,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct AttestationRepr {
    #[cbor(key = 1)]
    pub device_id: Hash,
    #[cbor(key = 2)]
    pub security_epoch: u64,
    #[cbor(key = 3)]
    pub approved_at: u64,
    #[cbor(key = 4)]
    pub expires_at: u64,
    #[cbor(key = 5)]
    pub origin: String,
    #[cbor(key = 6)]
    pub to_be_signed_digest: Hash,
    #[cbor(key = 7)]
    pub public_key_fingerprint: Hash,
    #[cbor(key = 8)]
    pub signature_digest: Hash,
    #[cbor(key = 9)]
    pub artifact: SignedArtifactRepr,
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
#[allow(clippy::large_enum_variant)]
pub enum ExecutionRecordRepr {
    Attestation(AttestationRepr),
    Derivation {
        #[cbor(key = 1)]
        grant: ExecutionGrantRepr,
        #[cbor(key = 2)]
        result: ExecutionResultRepr,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct AuthorizedExecutionRepr {
    #[cbor(key = 1)]
    pub account_id: AccountId,
    #[cbor(key = 2)]
    pub request_id: OpId,
    #[cbor(key = 3)]
    pub command_digest: Hash,
    #[cbor(key = 4)]
    pub record: ExecutionRecordRepr,
}

impl StableCodec for AuthorizedExecution {
    type Repr = AuthorizedExecutionRepr;

    fn to_repr(&self) -> Self::Repr {
        AuthorizedExecutionRepr {
            account_id: self.account_id,
            request_id: self.request_id,
            command_digest: self.command_digest,
            record: match &self.record {
                ExecutionRecord::Attestation(a) => ExecutionRecordRepr::Attestation(AttestationRepr {
                    device_id: a.device_id,
                    security_epoch: a.security_epoch,
                    approved_at: a.approved_at,
                    expires_at: a.expires_at,
                    origin: a.origin.clone(),
                    to_be_signed_digest: a.to_be_signed_digest,
                    public_key_fingerprint: a.public_key_fingerprint,
                    signature_digest: a.signature_digest,
                    artifact: a.artifact.to_repr(),
                }),
                ExecutionRecord::Derivation { grant, result } => ExecutionRecordRepr::Derivation {
                    grant: grant.to_repr(),
                    result: result.to_repr(),
                },
            },
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            account_id: repr.account_id,
            request_id: repr.request_id,
            command_digest: repr.command_digest,
            record: match repr.record {
                ExecutionRecordRepr::Attestation(a) => ExecutionRecord::Attestation(Attestation {
                    device_id: a.device_id,
                    security_epoch: a.security_epoch,
                    approved_at: a.approved_at,
                    expires_at: a.expires_at,
                    origin: a.origin,
                    to_be_signed_digest: a.to_be_signed_digest,
                    public_key_fingerprint: a.public_key_fingerprint,
                    signature_digest: a.signature_digest,
                    artifact: SignedArtifact::from_repr(a.artifact),
                }),
                ExecutionRecordRepr::Derivation { grant, result } => ExecutionRecord::Derivation {
                    grant: ExecutionGrant::from_repr(grant),
                    result: ExecutionResult::from_repr(result),
                },
            },
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
            created_at_ms: 1,
            account_id: AccountId([8; 12]),
            home_user: p(5),
            home_cose: p(6),
            auth_bindings: vec![p(1)],
            pending_bindings: vec![],
            account_version: 0,
            security_epoch: 0,
            devices: BTreeMap::from([(Hash::new([1; 32]), device(1))]),
            recovery_delay_ms: DEFAULT_RECOVERY_DELAY_MS,
            pending_recovery: None,
            completed_recovery: None,
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
            principal_updated_at: None,
        };
        if !populated {
            return state;
        }
        state.auth_bindings = (1..=8).map(p).collect();
        state.pending_bindings = (0..MAX_PENDING_BINDINGS as u8)
            .map(|n| PendingBinding {
                principal: p(n + 20),
                nonce: Hash::new([n + 120; 32]),
                expires_at: 1_700_000_600_000,
            })
            .collect();
        state.devices = (1..=16).map(|n| (Hash::new([n; 32]), device(n))).collect();
        state.account_version = 1_000;
        state.security_epoch = 20;
        state.recovery_delay_ms = DAY;
        state.pending_recovery = Some(PendingRecovery {
            request: RecoveryRequest {
                op_id: Hash::new([33; 32]),
                new_auth: p(9),
                device: device(9).input,
                expires_at: 1_700_086_400_000,
            },
            execute_after: 1_700_086_400_000,
        });
        state.completed_recovery = Some(RecoveryReceipt {
            request_id: Hash::new([44; 32]),
            new_auth: p(9),
            device_id: Hash::new([9; 32]),
            root_generation: Some(7),
        });
        state.current_root = Some(ContentRootRef {
            generation: 7,
            suite: "dmsg-root-v2".into(),
            bundle_digest: Hash::new([36; 32]),
            recipients_digest: Hash::new([38; 32]),
            body_digest: Hash::new([39; 32]),
        });
        state.root_slot = Some(RootReservation {
            op_id: Hash::new([37; 32]),
            expected_generation: 7,
            generation: 8,
            security_epoch: 20,
            expires_at: 1_700_000_900_000,
        });
        state.vault_write_state = VaultWriteState::Ready;
        state.principal_updated_at = Some(1_700_000_200_000);
        state.operations = (0..MAX_OPERATION_RECEIPTS as u8)
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
                            account_id: state.account_id,
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
            (1..=6)
                .chain([8])
                .chain(15..=19)
                .chain([23, 27])
                .collect::<Vec<_>>()
        );

        let full = account(true);
        let compact = compact_bytes(&full);
        assert!(compact.len() > 14_000);
        assert_eq!(
            hex(&compact),
            "2a5002d9b1af78b7abb38e2681dad52d0fc65f28a64df4e003aed28ee87b06d0"
        );
        let plain = cbor2::to_vec(&full).unwrap();
        assert_eq!(compact_from_bytes::<AccountState>(&compact), full);
        assert_eq!(
            top_keys(&compact),
            (1..=6)
                .chain([8])
                .chain(12..=23)
                .chain([25, 27, 28, 29])
                .collect::<Vec<_>>()
        );
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
    fn monthly_ledger_preserves_counters_and_weights_in_compact_storage() {
        let month = Month {
            usage: ExecutionUsage {
                account_id: AccountId([8; 12]),
                month_utc: 202609,
                month_revision: 11,
                business_revision: 12,
                lease_revision: 13,
                weight_policy_version: 14,
                allowed_units: 200,
                charged_units: 21,
                valid_until_ms: 1_790_000_000_000,
            },
            weights: ExecutionWeights {
                version: 14,
                ed25519: 1,
            },
            entitlement_digest: Hash::new([1; 32]),
        };
        let bytes = compact_bytes(&month);
        assert_eq!(compact_from_bytes::<Month>(&bytes), month);
        assert!(bytes.len() < 128, "monthly ledger: {} bytes", bytes.len());
    }

    #[test]
    fn config_and_both_execution_records_round_trip() {
        let config = Config {
            schema: crate::store::STABLE_SCHEMA,
            init: UserInit {
                commerce_canister: candid::Principal::from_slice(&[88]),
                membership_canister: candid::Principal::from_slice(&[89]),
                environment: Environment::Production,
                issuer_namespace: "https://dmsg.example/u/".into(),
                home_cose: p(2),
                handle_canister: p(3),
                payment_canister: p(4),
                max_accounts: MAX_HOME_ACCOUNTS,
                daily_new_accounts: 10_000,
                principal_origin: "https://id.dmsg.example".into(),
                directory_canister: p(5),
                governance: p(6),
                admission_key: Some(Hash::new([7; 32])),
            },
            allocator: XidGenerator::new([1, 2, 3, 4, 5]),
            allocator_namespace_digest: Hash::new([6; 32]),
            day: 42,
            created_today: 7,
            master_secret: Some(Hash::new([9; 32])),
        };
        let decoded = compact_from_bytes::<Config>(&compact_bytes(&config));
        assert_eq!(decoded.schema, crate::store::STABLE_SCHEMA);
        assert_eq!(decoded.master_secret, config.master_secret);
        assert_eq!(decoded.init, config.init);

        let state = account(false);
        let derivation = AuthorizedExecution {
            account_id: state.account_id,
            request_id: Hash::new([7; 32]),
            command_digest: Hash::new([11; 32]),
            record: ExecutionRecord::Derivation {
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
                    generation: 3,
                    transport_key: [10; 48].into(),
                    max_cycles: 1_000,
                },
                result: ExecutionResult {
                    request_id: Hash::new([7; 32]),
                    outcome: ExecutionOutcome::Executing,
                    cycles_cost_upper_bound: 1_000,
                    cycles_charged: 600,
                },
            },
        };
        let bytes = compact_bytes(&derivation);
        assert_eq!(
            compact_from_bytes::<AuthorizedExecution>(&bytes),
            derivation
        );
        assert!(bytes.len() * 100 <= cbor2::to_vec(&derivation).unwrap().len() * 65);
        let attestation = AuthorizedExecution {
            record: ExecutionRecord::Attestation(Attestation {
                device_id: Hash::new([8; 32]),
                security_epoch: 2,
                approved_at: 100,
                expires_at: 200,
                origin: "https://example.com".into(),
                to_be_signed_digest: Hash::new([12; 32]),
                public_key_fingerprint: Hash::new([13; 32]),
                signature_digest: Hash::new([14; 32]),
                artifact: SignedArtifact {
                    cose_sign1: vec![16; 256].into(),
                    cose_key: vec![17; 96].into(),
                },
            }),
            ..derivation
        };
        assert_eq!(
            compact_from_bytes::<AuthorizedExecution>(&compact_bytes(&attestation)),
            attestation
        );
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
            compact.insert(key, CompactStored::new(&state));
        }

        let last_key = {
            let mut key = vec![0_u8; 12];
            key[4..].copy_from_slice(&127_u64.to_be_bytes());
            key
        };
        assert_eq!(
            plain.get(&last_key).unwrap().0.account_id,
            compact.get(&last_key).unwrap().into_inner().account_id
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
