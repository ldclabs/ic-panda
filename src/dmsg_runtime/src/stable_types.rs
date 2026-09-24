//! Compact representations for public domain types stored by dMsg canisters.
//!
//! These representations are deliberately separate from `dmsg_types`: the
//! latter defines protocol CBOR and digest inputs, while this module defines
//! only stable-memory bytes.
//! Integer assignments are stable schema: key 0 is reserved, existing keys
//! must never be reused, and new fields receive new positive keys.

use crate::{storage::StableCodec, Budget};
use candid::Principal;
use cbor2::Cbor;
use dmsg_types::{
    billing::CommercialReservation, cose::*, handle::*, membership::*, payment::*,
    profiles::delivery::Quote, user::*, *,
};
use icrc_ledger_types::icrc1::account::Account;
use serde_bytes::{ByteArray, ByteBuf};
use std::collections::BTreeMap;

/// Declare a compact representation. Fields marked `as codec` convert through
/// their own [`StableCodec`]; all other fields are cloned unchanged.
macro_rules! stable_struct {
    (@to $value:expr) => {
        $value.clone()
    };
    (@to $value:expr, codec) => {
        StableCodec::to_repr(&$value)
    };
    (@from $value:expr) => {
        $value
    };
    (@from $value:expr, codec) => {
        StableCodec::from_repr($value)
    };
    ($repr:ident => $domain:ident { $($(#[$meta:meta])* $key:literal => $field:ident: $ty:ty $(as $codec:ident)?),+ $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Cbor)]
        pub struct $repr {
            $(
                #[cbor(key = $key)]
                $(#[$meta])*
                pub $field: $ty,
            )+
        }

        impl StableCodec for $domain {
            type Repr = $repr;

            fn to_repr(&self) -> Self::Repr {
                $repr {
                    $($field: stable_struct!(@to self.$field $(, $codec)?),)+
                }
            }

            fn from_repr(repr: Self::Repr) -> Self {
                Self {
                    $($field: stable_struct!(@from repr.$field $(, $codec)?),)+
                }
            }
        }
    };
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct AccountRepr {
    #[cbor(key = 1)]
    pub owner: Principal,
    #[cbor(key = 2)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subaccount: Option<Hash>,
}

impl StableCodec for Account {
    type Repr = AccountRepr;

    fn to_repr(&self) -> Self::Repr {
        AccountRepr {
            owner: self.owner,
            subaccount: self.subaccount.map(Hash::new),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            owner: repr.owner,
            subaccount: repr.subaccount.map(Hash::into_array),
        }
    }
}

stable_struct!(BudgetRepr => Budget {
    1 => day: u64,
    2 => executions: u32,
    3 => cycles: u128,
});

stable_struct!(DeviceInputRepr => DeviceInput {
    1 => device_id: Hash,
    2 => signing_pub: Hash,
    3 => hpke_pub: Hash,
    4 => role: ControllerRole,
    5 => capabilities: Vec<Capability>,
});

stable_struct!(DeviceRepr => Device {
    1 => input: DeviceInputRepr as codec,
    2 => added_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    3 => added_by: Option<Hash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    4 => revoked_at: Option<u64>,
    5 => next_sequence: u64,
});

stable_struct!(RecoveryPolicyRepr => RecoveryPolicy {
    1 => generation: u64,
    2 => signing_pub: Hash,
    3 => hpke_pub: Hash,
    4 => delay_ms: u64,
});

stable_struct!(ContentRootRefRepr => ContentRootRef {
    1 => generation: u64,
    2 => suite: String,
    3 => home_cose: Principal,
    4 => derivation_version: u16,
    5 => key_generation: u64,
    6 => bundle_digest: Hash,
    7 => recovery_generation: u64,
});

stable_struct!(RootReservationRepr => RootReservation {
    1 => op_id: OpId,
    2 => expected_generation: u64,
    3 => generation: u64,
    4 => security_epoch: u64,
    5 => expires_at: u64,
});

stable_struct!(SensitivePolicyRepr => SensitivePolicy {
    1 => frozen: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    2 => allowed_purposes: Vec<KeyPurpose>,
    3 => daily_executions: u32,
    4 => daily_cycles: u128,
});

stable_struct!(OperationReceiptRepr => OperationReceipt {
    1 => id: OpId,
    2 => digest: Hash,
    3 => account_version: u64,
});

stable_struct!(RecoveryConfirmationRepr => RecoveryConfirmation {
    1 => request_id: OpId,
    2 => dispute: Hash,
    3 => expires_at: u64,
});

stable_struct!(RecoveryRequestRepr => RecoveryRequest {
    1 => op_id: OpId,
    2 => new_auth: Principal,
    3 => device: DeviceInputRepr as codec,
    4 => generation: u64,
    5 => expires_at: u64,
});

stable_struct!(PendingRecoveryRepr => PendingRecovery {
    1 => request: RecoveryRequestRepr as codec,
    2 => execute_after: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    3 => dispute: Option<Hash>,
    4 => reconfirmed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    5 => confirmation: Option<RecoveryConfirmationRepr> as codec,
});

stable_struct!(HandleIntentRepr => HandleIntent {
    1 => handle_canister: Principal,
    2 => action: HandleAction,
    3 => account_id: AccountId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    4 => target_account: Option<AccountId>,
    5 => handle: String,
    6 => expected_version: u64,
    7 => op_id: OpId,
    8 => terms_digest: Hash,
});

stable_struct!(HandleInitRepr => HandleInit {
    1 => home_user: Principal,
    2 => ledger: Principal,
    3 => ledger_fee: u128,
    4 => max_pending: u32,
});

stable_struct!(LegacySnapshotRepr => LegacySnapshot {
    1 => source_canister: Principal,
    2 => snapshot_id: Hash,
    3 => freeze_version: u64,
    4 => event_tip: Hash,
    5 => count: u64,
    6 => entries_digest: Hash,
});

stable_struct!(LegacyReservationRepr => LegacyReservation {
    1 => handle: String,
    2 => legacy_owner: Principal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    3 => legacy_name_principal: Option<Principal>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    4 => frozen_admins: Vec<Principal>,
    5 => quarantined: bool,
});

stable_struct!(HandleRecordRepr => HandleRecord {
    1 => handle: String,
    2 => owner_account: AccountId,
    3 => version: u64,
    4 => event_tip: Hash,
});

stable_struct!(HandleEventRepr => HandleEvent {
    1 => sequence: u64,
    2 => previous: Hash,
    3 => handle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    4 => from: Option<AccountId>,
    5 => to: AccountId,
    6 => version: u64,
    7 => at: u64,
    8 => legacy_snapshot: Hash,
});

stable_struct!(SnapshotProgressRepr => SnapshotProgress {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    1 => snapshot: Option<LegacySnapshotRepr> as codec,
    2 => imported: u64,
    3 => rolling_digest: Hash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    4 => last_handle: Option<String>,
    5 => sealed: bool,
});

stable_struct!(RegistrationRepr => Registration {
    1 => intent: HandleIntentRepr as codec,
    2 => payer: AccountRepr as codec,
    3 => fee: u128,
});

stable_struct!(HandleOperationRepr => HandleOperation {
    1 => registration: RegistrationRepr as codec,
    2 => digest: Hash,
    3 => phase: HandlePhase,
    4 => amount: u128,
    5 => created_at: u64,
    7 => memo: Hash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    8 => ledger_block: Option<u64>,
});

stable_struct!(ReceiptSignerRepr => ReceiptSigner {
    1 => epoch: u64,
    2 => public_key: Hash,
    3 => valid_from: u64,
    4 => valid_until: u64,
    5 => revoked: bool,
});

stable_struct!(QuoteRepr => Quote {
    21 => fee_policy_version: u64,
    1 => quote_id: Hash,
    2 => home_payment: Principal,
    3 => payer: AccountRepr as codec,
    4 => offer_digest: Hash,
    5 => quote_scope: Hash,
    6 => ledger: Principal,
    7 => recipient: AccountRepr as codec,
    8 => recipient_net: u128,
    9 => platform: AccountRepr as codec,
    10 => service_fee: u128,
    11 => fee_reserve: u128,
    12 => amount: u128,
    13 => max_network_fee: u128,
    14 => max_bytes: u32,
    15 => retain_ms: u64,
    16 => envelope_digest: Hash,
    17 => signer_epoch: u64,
    18 => created_at: u64,
    19 => fund_by: u64,
    20 => accept_by: u64,
});

stable_struct!(DeliveryFeePolicyRepr => DeliveryFeePolicy {
    1 => version: u64,
    2 => effective_at_ms: u64,
    3 => rate_bps: u16,
    4 => minimum_atomic: u128,
});

stable_struct!(CommercialReservationRepr => CommercialReservation {
    1 => reservation_id: Hash,
    2 => month_utc: u32,
    3 => units: u64,
    4 => weight_policy_version: u64,
    5 => business_revision: u64,
    6 => lease_revision: u64,
    7 => valid_until_ms: u64,
});

stable_struct!(PaymentInitRepr => PaymentInit {
    1 => home_user: Principal,
    2 => ledger: Principal,
    3 => platform: AccountRepr as codec,
    4 => fee_policy: DeliveryFeePolicyRepr as codec,
    11 => governance: Principal,
    5 => ledger_fee: u128,
    6 => max_fee: u128,
    7 => signer: ReceiptSignerRepr as codec,
    8 => max_open_per_payer: u32,
    9 => daily_orders: u32,
    10 => enabled: bool,
});

stable_struct!(DepositRepr => Deposit {
    1 => block: u64,
    2 => from: AccountRepr as codec,
    3 => amount: u128,
    4 => committed_at: u64,
    5 => refundable: u128,
});

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub enum LegKindRepr {
    Recipient,
    Platform,
    Refund {
        #[cbor(key = 1)]
        funding_block: u64,
    },
    ReserveRefund,
}

impl StableCodec for LegKind {
    type Repr = LegKindRepr;

    fn to_repr(&self) -> Self::Repr {
        match self {
            Self::Recipient => LegKindRepr::Recipient,
            Self::Platform => LegKindRepr::Platform,
            Self::Refund { funding_block } => LegKindRepr::Refund {
                funding_block: *funding_block,
            },
            Self::ReserveRefund => LegKindRepr::ReserveRefund,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        match repr {
            LegKindRepr::Recipient => Self::Recipient,
            LegKindRepr::Platform => Self::Platform,
            LegKindRepr::Refund { funding_block } => Self::Refund { funding_block },
            LegKindRepr::ReserveRefund => Self::ReserveRefund,
        }
    }
}

stable_struct!(TransferLegRepr => TransferLeg {
    1 => escrow_id: Hash,
    2 => leg_id: u64,
    3 => kind: LegKindRepr as codec,
    4 => to: AccountRepr as codec,
    5 => amount: u128,
    6 => fee: u128,
    7 => memo: Hash,
    8 => created_at_time: u64,
    9 => status: LegStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    10 => block: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    11 => expected_fee: Option<u128>,
    12 => revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    13 => replaces: Option<u64>,
    14 => history_digest: Hash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    15 => last_failure: Option<TransferFailure>,
});

stable_struct!(KeyRequestRepr => KeyRequest {
    1 => purpose: KeyPurpose,
    2 => algorithm: Algorithm,
    3 => generation: u64,
});

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub enum ExecutionKindRepr {
    Sign {
        #[cbor(key = 1)]
        key: KeyRequestRepr,
        #[cbor(key = 2)]
        to_be_signed: ByteBuf,
        #[cbor(key = 3)]
        public_key_fingerprint: Hash,
        #[cbor(key = 4)]
        origin: String,
    },
    Derive {
        #[cbor(key = 5)]
        generation: u64,
        #[cbor(key = 6)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root_op_id: Option<OpId>,
        #[cbor(key = 7)]
        transport_key: ByteArray<48>,
    },
}

impl StableCodec for ExecutionKind {
    type Repr = ExecutionKindRepr;

    fn to_repr(&self) -> Self::Repr {
        match self {
            Self::Sign {
                key,
                to_be_signed,
                public_key_fingerprint,
                origin,
            } => ExecutionKindRepr::Sign {
                key: key.to_repr(),
                to_be_signed: to_be_signed.clone(),
                public_key_fingerprint: *public_key_fingerprint,
                origin: origin.clone(),
            },
            Self::Derive {
                generation,
                root_op_id,
                transport_key,
            } => ExecutionKindRepr::Derive {
                generation: *generation,
                root_op_id: *root_op_id,
                transport_key: *transport_key,
            },
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        match repr {
            ExecutionKindRepr::Sign {
                key,
                to_be_signed,
                public_key_fingerprint,
                origin,
            } => Self::Sign {
                key: KeyRequest::from_repr(key),
                to_be_signed,
                public_key_fingerprint,
                origin,
            },
            ExecutionKindRepr::Derive {
                generation,
                root_op_id,
                transport_key,
            } => Self::Derive {
                generation,
                root_op_id,
                transport_key,
            },
        }
    }
}

stable_struct!(ExecutionGrantRepr => ExecutionGrant {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    13 => commerce: Option<CommercialReservationRepr> as codec,
    1 => account_id: AccountId,
    2 => home_user: Principal,
    3 => home_cose: Principal,
    4 => request_id: OpId,
    5 => execution_sequence: u64,
    6 => security_epoch: u64,
    7 => device_id: Hash,
    8 => device_sequence: u64,
    9 => approved_at: u64,
    10 => expires_at: u64,
    11 => kind: ExecutionKindRepr as codec,
    12 => max_cycles: u128,
});

stable_struct!(SignedArtifactRepr => SignedArtifact {
    1 => cose_sign1: ByteBuf,
    2 => cose_key: ByteBuf,
});

stable_struct!(KeyDescriptorRepr => KeyDescriptor {
    1 => key_id: ByteBuf,
    2 => account_id: AccountId,
    3 => purpose: KeyPurpose,
    4 => algorithm: Algorithm,
    5 => home_cose: Principal,
    6 => master_key_name: String,
    7 => environment: Environment,
    8 => derivation_version: u16,
    9 => key_generation: u64,
    10 => public_key: ByteBuf,
    11 => public_key_fingerprint: Hash,
});

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub enum ExecutionOutputRepr {
    Signature {
        #[cbor(key = 1)]
        artifact: SignedArtifactRepr,
        #[cbor(key = 2)]
        key: KeyDescriptorRepr,
    },
    EncryptedRootKey {
        #[cbor(key = 3)]
        encrypted_key: ByteBuf,
        #[cbor(key = 2)]
        key: KeyDescriptorRepr,
    },
}

impl StableCodec for ExecutionOutput {
    type Repr = ExecutionOutputRepr;

    fn to_repr(&self) -> Self::Repr {
        match self {
            Self::Signature { artifact, key } => ExecutionOutputRepr::Signature {
                artifact: artifact.to_repr(),
                key: key.to_repr(),
            },
            Self::EncryptedRootKey { encrypted_key, key } => {
                ExecutionOutputRepr::EncryptedRootKey {
                    encrypted_key: encrypted_key.clone(),
                    key: key.to_repr(),
                }
            }
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        match repr {
            ExecutionOutputRepr::Signature { artifact, key } => Self::Signature {
                artifact: SignedArtifact::from_repr(artifact),
                key: KeyDescriptor::from_repr(key),
            },
            ExecutionOutputRepr::EncryptedRootKey { encrypted_key, key } => {
                Self::EncryptedRootKey {
                    encrypted_key,
                    key: KeyDescriptor::from_repr(key),
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub enum ExecutionOutcomeRepr {
    Authorized,
    Executing,
    Completed(Box<ExecutionOutputRepr>),
    Failed(Error),
    Unknown(Error),
    ResultExpired,
}

impl StableCodec for ExecutionOutcome {
    type Repr = ExecutionOutcomeRepr;

    fn to_repr(&self) -> Self::Repr {
        match self {
            Self::Authorized => ExecutionOutcomeRepr::Authorized,
            Self::Executing => ExecutionOutcomeRepr::Executing,
            Self::Completed(output) => ExecutionOutcomeRepr::Completed(Box::new(output.to_repr())),
            Self::Failed(error) => ExecutionOutcomeRepr::Failed(error.clone()),
            Self::Unknown(error) => ExecutionOutcomeRepr::Unknown(error.clone()),
            Self::ResultExpired => ExecutionOutcomeRepr::ResultExpired,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        match repr {
            ExecutionOutcomeRepr::Authorized => Self::Authorized,
            ExecutionOutcomeRepr::Executing => Self::Executing,
            ExecutionOutcomeRepr::Completed(output) => {
                Self::Completed(Box::new(ExecutionOutput::from_repr(*output)))
            }
            ExecutionOutcomeRepr::Failed(error) => Self::Failed(error),
            ExecutionOutcomeRepr::Unknown(error) => Self::Unknown(error),
            ExecutionOutcomeRepr::ResultExpired => Self::ResultExpired,
        }
    }
}

stable_struct!(ExecutionResultRepr => ExecutionResult {
    1 => request_id: OpId,
    2 => outcome: ExecutionOutcomeRepr as codec,
    3 => cycles_cost_upper_bound: u128,
});

stable_struct!(MasterKeyRepr => MasterKey {
    1 => algorithm: Algorithm,
    2 => key_name: String,
    3 => expected_fingerprint: Hash,
});

stable_struct!(CoseInitRepr => CoseInit {
    1 => issuer_namespace: String,
    2 => environment: Environment,
    3 => executing_canister: Principal,
    4 => initial_home_user: Principal,
    5 => derivation_version: u16,
    6 => masters: Vec<MasterKeyRepr> as codec,
    7 => daily_executions: u32,
    8 => daily_cycles: u128,
});

stable_struct!(KeyStateRepr => KeyState {
    1 => config: CoseInitRepr as codec,
    2 => initialization: Initialization,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    3 => fingerprints: Vec<Hash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    4 => error: Option<String>,
});

stable_struct!(UserInitRepr => UserInit {
    8 => commerce_canister: Principal,
    9 => membership_canister: Principal,
    1 => environment: Environment,
    2 => issuer_namespace: String,
    3 => home_cose: Principal,
    4 => handle_canister: Principal,
    5 => payment_canister: Principal,
    6 => max_accounts: u64,
    7 => daily_new_accounts: u32,
});

stable_struct!(BeneficiaryRepr => Beneficiary {
    1 => product_id: String,
    2 => authority_canister: Principal,
    3 => subject_schema: String,
    4 => subject_bytes: ByteBuf,
});

pub fn map_to_repr<K, V>(values: &BTreeMap<K, V>) -> BTreeMap<K, V::Repr>
where
    K: Clone + Ord,
    V: StableCodec,
{
    values
        .iter()
        .map(|(key, value)| (key.clone(), value.to_repr()))
        .collect()
}

pub fn map_from_repr<K, V>(values: BTreeMap<K, V::Repr>) -> BTreeMap<K, V>
where
    K: Ord,
    V: StableCodec,
{
    values
        .into_iter()
        .map(|(key, value)| (key, V::from_repr(value)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{compact_bytes, compact_from_bytes};

    fn integer_fields(value: &cbor2::Value) {
        let cbor2::Value::Map(fields) = value else {
            panic!("expected a compact record")
        };
        assert!(fields
            .iter()
            .all(|(key, _)| matches!(key, cbor2::Value::Integer(_))));
    }

    fn field(value: &cbor2::Value, key: u64) -> Option<&cbor2::Value> {
        let cbor2::Value::Map(fields) = value else {
            panic!("expected a compact record")
        };
        fields
            .iter()
            .find(|(k, _)| *k == key.into())
            .map(|(_, v)| v)
    }

    #[test]
    fn commercial_reservation_is_recursive_compact_and_optional() {
        let reservation = CommercialReservation {
            reservation_id: Hash::new([1; 32]),
            month_utc: 202609,
            units: 1,
            weight_policy_version: 1,
            business_revision: 1,
            lease_revision: 2,
            valid_until_ms: 1_800_000_060_000,
        };
        assert_eq!(compact_bytes(&reservation).len(), 60);
        assert_eq!(
            compact_from_bytes::<CommercialReservation>(&compact_bytes(&reservation)),
            reservation
        );
        let mut grant = ExecutionGrant {
            account_id: AccountId([1; 12]),
            home_user: Principal::from_slice(&[1]),
            home_cose: Principal::from_slice(&[2]),
            request_id: Hash::new([2; 32]),
            execution_sequence: 1,
            security_epoch: 1,
            device_id: Hash::new([3; 32]),
            device_sequence: 1,
            approved_at: 1,
            expires_at: 2,
            commerce: Some(reservation),
            max_cycles: 100,
            kind: ExecutionKind::Derive {
                generation: 1,
                root_op_id: None,
                transport_key: ByteArray::new([1; 48]),
            },
        };
        let value: cbor2::Value = cbor2::from_slice(&compact_bytes(&grant)).unwrap();
        integer_fields(field(&value, 13).unwrap());
        assert_eq!(
            compact_from_bytes::<ExecutionGrant>(&compact_bytes(&grant)),
            grant
        );
        grant.commerce = None;
        let value: cbor2::Value = cbor2::from_slice(&compact_bytes(&grant)).unwrap();
        assert!(field(&value, 13).is_none());
        assert_eq!(
            compact_from_bytes::<ExecutionGrant>(&compact_bytes(&grant)),
            grant
        );
    }

    #[test]
    fn payment_configuration_uses_compact_fee_policy() {
        let fee_policy = DeliveryFeePolicy {
            version: 1,
            effective_at_ms: 1_800_000_000_000,
            rate_bps: 500,
            minimum_atomic: 20_000,
        };
        assert_eq!(compact_bytes(&fee_policy).len(), 21);
        let init = PaymentInit {
            home_user: Principal::from_slice(&[1]),
            ledger: Principal::from_slice(&[2]),
            platform: Account {
                owner: Principal::from_slice(&[3]),
                subaccount: None,
            },
            governance: Principal::from_slice(&[4]),
            fee_policy,
            ledger_fee: 10,
            max_fee: 20,
            signer: ReceiptSigner {
                epoch: 1,
                public_key: Hash::new([1; 32]),
                valid_from: 1,
                valid_until: 2,
                revoked: false,
            },
            max_open_per_payer: 10,
            daily_orders: 100,
            enabled: true,
        };
        let value: cbor2::Value = cbor2::from_slice(&compact_bytes(&init)).unwrap();
        integer_fields(field(&value, 4).unwrap());
        assert_eq!(
            compact_from_bytes::<PaymentInit>(&compact_bytes(&init)),
            init
        );
    }
}
