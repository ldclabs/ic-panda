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
use serde_bytes::ByteBuf;
use std::collections::BTreeMap;

macro_rules! stable_struct {
    ($repr:ident => $domain:ident { $($(#[$meta:meta])* $key:literal => $field:ident: $ty:ty),+ $(,)? }) => {
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
                    $($field: self.$field.clone(),)+
                }
            }

            fn from_repr(repr: Self::Repr) -> Self {
                Self {
                    $($field: repr.$field,)+
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

pub fn account_to_repr(value: &Account) -> AccountRepr {
    AccountRepr {
        owner: value.owner,
        subaccount: value.subaccount.map(Hash::new),
    }
}

pub fn account_from_repr(value: AccountRepr) -> Account {
    Account {
        owner: value.owner,
        subaccount: value.subaccount.map(Hash::into_array),
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

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct DeviceRepr {
    #[cbor(key = 1)]
    pub input: DeviceInputRepr,
    #[cbor(key = 2)]
    pub added_at: u64,
    #[cbor(key = 3)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_by: Option<Hash>,
    #[cbor(key = 4)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<u64>,
    #[cbor(key = 5)]
    pub next_sequence: u64,
}

impl StableCodec for Device {
    type Repr = DeviceRepr;

    fn to_repr(&self) -> Self::Repr {
        DeviceRepr {
            input: self.input.to_repr(),
            added_at: self.added_at,
            added_by: self.added_by,
            revoked_at: self.revoked_at,
            next_sequence: self.next_sequence,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            input: DeviceInput::from_repr(repr.input),
            added_at: repr.added_at,
            added_by: repr.added_by,
            revoked_at: repr.revoked_at,
            next_sequence: repr.next_sequence,
        }
    }
}

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

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct RecoveryRequestRepr {
    #[cbor(key = 1)]
    pub op_id: OpId,
    #[cbor(key = 2)]
    pub new_auth: Principal,
    #[cbor(key = 3)]
    pub device: DeviceInputRepr,
    #[cbor(key = 4)]
    pub generation: u64,
    #[cbor(key = 5)]
    pub expires_at: u64,
}

impl StableCodec for RecoveryRequest {
    type Repr = RecoveryRequestRepr;

    fn to_repr(&self) -> Self::Repr {
        RecoveryRequestRepr {
            op_id: self.op_id,
            new_auth: self.new_auth,
            device: self.device.to_repr(),
            generation: self.generation,
            expires_at: self.expires_at,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            op_id: repr.op_id,
            new_auth: repr.new_auth,
            device: DeviceInput::from_repr(repr.device),
            generation: repr.generation,
            expires_at: repr.expires_at,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct PendingRecoveryRepr {
    #[cbor(key = 1)]
    pub request: RecoveryRequestRepr,
    #[cbor(key = 2)]
    pub execute_after: u64,
    #[cbor(key = 3)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispute: Option<Hash>,
    #[cbor(key = 4)]
    pub reconfirmed: bool,
    #[cbor(key = 5)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<RecoveryConfirmationRepr>,
}

impl StableCodec for PendingRecovery {
    type Repr = PendingRecoveryRepr;

    fn to_repr(&self) -> Self::Repr {
        PendingRecoveryRepr {
            request: self.request.to_repr(),
            execute_after: self.execute_after,
            dispute: self.dispute,
            reconfirmed: self.reconfirmed,
            confirmation: self.confirmation.as_ref().map(StableCodec::to_repr),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            request: RecoveryRequest::from_repr(repr.request),
            execute_after: repr.execute_after,
            dispute: repr.dispute,
            reconfirmed: repr.reconfirmed,
            confirmation: repr.confirmation.map(RecoveryConfirmation::from_repr),
        }
    }
}

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

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct SnapshotProgressRepr {
    #[cbor(key = 1)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<LegacySnapshotRepr>,
    #[cbor(key = 2)]
    pub imported: u64,
    #[cbor(key = 3)]
    pub rolling_digest: Hash,
    #[cbor(key = 4)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_handle: Option<String>,
    #[cbor(key = 5)]
    pub sealed: bool,
}

impl StableCodec for SnapshotProgress {
    type Repr = SnapshotProgressRepr;

    fn to_repr(&self) -> Self::Repr {
        SnapshotProgressRepr {
            snapshot: self.snapshot.as_ref().map(StableCodec::to_repr),
            imported: self.imported,
            rolling_digest: self.rolling_digest,
            last_handle: self.last_handle.clone(),
            sealed: self.sealed,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            snapshot: repr.snapshot.map(LegacySnapshot::from_repr),
            imported: repr.imported,
            rolling_digest: repr.rolling_digest,
            last_handle: repr.last_handle,
            sealed: repr.sealed,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct RegistrationRepr {
    #[cbor(key = 1)]
    pub intent: HandleIntentRepr,
    #[cbor(key = 2)]
    pub payer: AccountRepr,
    #[cbor(key = 3)]
    pub fee: u128,
}

impl StableCodec for Registration {
    type Repr = RegistrationRepr;

    fn to_repr(&self) -> Self::Repr {
        RegistrationRepr {
            intent: self.intent.to_repr(),
            payer: account_to_repr(&self.payer),
            fee: self.fee,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            intent: HandleIntent::from_repr(repr.intent),
            payer: account_from_repr(repr.payer),
            fee: repr.fee,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct HandleOperationRepr {
    #[cbor(key = 1)]
    pub registration: RegistrationRepr,
    #[cbor(key = 2)]
    pub digest: Hash,
    #[cbor(key = 3)]
    pub phase: HandlePhase,
    #[cbor(key = 4)]
    pub amount: u128,
    #[cbor(key = 5)]
    pub created_at: u64,
    #[cbor(key = 6)]
    pub expires_at: u64,
    #[cbor(key = 7)]
    pub memo: Hash,
    #[cbor(key = 8)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ledger_block: Option<u64>,
}

impl StableCodec for HandleOperation {
    type Repr = HandleOperationRepr;

    fn to_repr(&self) -> Self::Repr {
        HandleOperationRepr {
            registration: self.registration.to_repr(),
            digest: self.digest,
            phase: self.phase.clone(),
            amount: self.amount,
            created_at: self.created_at,
            expires_at: self.expires_at,
            memo: self.memo,
            ledger_block: self.ledger_block,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            registration: Registration::from_repr(repr.registration),
            digest: repr.digest,
            phase: repr.phase,
            amount: repr.amount,
            created_at: repr.created_at,
            expires_at: repr.expires_at,
            memo: repr.memo,
            ledger_block: repr.ledger_block,
        }
    }
}

stable_struct!(ReceiptSignerRepr => ReceiptSigner {
    1 => epoch: u64,
    2 => public_key: Hash,
    3 => valid_from: u64,
    4 => valid_until: u64,
    5 => revoked: bool,
});

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct QuoteRepr {
    #[cbor(key = 21)]
    pub fee_policy_version: u64,
    #[cbor(key = 1)]
    pub quote_id: Hash,
    #[cbor(key = 2)]
    pub home_payment: Principal,
    #[cbor(key = 3)]
    pub payer: AccountRepr,
    #[cbor(key = 4)]
    pub offer_digest: Hash,
    #[cbor(key = 5)]
    pub quote_scope: Hash,
    #[cbor(key = 6)]
    pub ledger: Principal,
    #[cbor(key = 7)]
    pub recipient: AccountRepr,
    #[cbor(key = 8)]
    pub recipient_net: u128,
    #[cbor(key = 9)]
    pub platform: AccountRepr,
    #[cbor(key = 10)]
    pub service_fee: u128,
    #[cbor(key = 11)]
    pub fee_reserve: u128,
    #[cbor(key = 12)]
    pub amount: u128,
    #[cbor(key = 13)]
    pub max_network_fee: u128,
    #[cbor(key = 14)]
    pub max_bytes: u32,
    #[cbor(key = 15)]
    pub retain_ms: u64,
    #[cbor(key = 16)]
    pub envelope_digest: Hash,
    #[cbor(key = 17)]
    pub signer_epoch: u64,
    #[cbor(key = 18)]
    pub created_at: u64,
    #[cbor(key = 19)]
    pub fund_by: u64,
    #[cbor(key = 20)]
    pub accept_by: u64,
}

impl StableCodec for Quote {
    type Repr = QuoteRepr;

    fn to_repr(&self) -> Self::Repr {
        QuoteRepr {
            fee_policy_version: self.fee_policy_version,
            quote_id: self.quote_id,
            home_payment: self.home_payment,
            payer: account_to_repr(&self.payer),
            offer_digest: self.offer_digest,
            quote_scope: self.quote_scope,
            ledger: self.ledger,
            recipient: account_to_repr(&self.recipient),
            recipient_net: self.recipient_net,
            platform: account_to_repr(&self.platform),
            service_fee: self.service_fee,
            fee_reserve: self.fee_reserve,
            amount: self.amount,
            max_network_fee: self.max_network_fee,
            max_bytes: self.max_bytes,
            retain_ms: self.retain_ms,
            envelope_digest: self.envelope_digest,
            signer_epoch: self.signer_epoch,
            created_at: self.created_at,
            fund_by: self.fund_by,
            accept_by: self.accept_by,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            fee_policy_version: repr.fee_policy_version,
            quote_id: repr.quote_id,
            home_payment: repr.home_payment,
            payer: account_from_repr(repr.payer),
            offer_digest: repr.offer_digest,
            quote_scope: repr.quote_scope,
            ledger: repr.ledger,
            recipient: account_from_repr(repr.recipient),
            recipient_net: repr.recipient_net,
            platform: account_from_repr(repr.platform),
            service_fee: repr.service_fee,
            fee_reserve: repr.fee_reserve,
            amount: repr.amount,
            max_network_fee: repr.max_network_fee,
            max_bytes: repr.max_bytes,
            retain_ms: repr.retain_ms,
            envelope_digest: repr.envelope_digest,
            signer_epoch: repr.signer_epoch,
            created_at: repr.created_at,
            fund_by: repr.fund_by,
            accept_by: repr.accept_by,
        }
    }
}

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

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct PaymentInitRepr {
    #[cbor(key = 1)]
    pub home_user: Principal,
    #[cbor(key = 2)]
    pub ledger: Principal,
    #[cbor(key = 3)]
    pub platform: AccountRepr,
    #[cbor(key = 4)]
    pub fee_policy: DeliveryFeePolicyRepr,
    #[cbor(key = 11)]
    pub governance: Principal,
    #[cbor(key = 5)]
    pub ledger_fee: u128,
    #[cbor(key = 6)]
    pub max_fee: u128,
    #[cbor(key = 7)]
    pub signer: ReceiptSignerRepr,
    #[cbor(key = 8)]
    pub max_open_per_payer: u32,
    #[cbor(key = 9)]
    pub daily_orders: u32,
    #[cbor(key = 10)]
    pub enabled: bool,
}

impl StableCodec for PaymentInit {
    type Repr = PaymentInitRepr;

    fn to_repr(&self) -> Self::Repr {
        PaymentInitRepr {
            home_user: self.home_user,
            ledger: self.ledger,
            platform: account_to_repr(&self.platform),
            fee_policy: self.fee_policy.to_repr(),
            governance: self.governance,
            ledger_fee: self.ledger_fee,
            max_fee: self.max_fee,
            signer: self.signer.to_repr(),
            max_open_per_payer: self.max_open_per_payer,
            daily_orders: self.daily_orders,
            enabled: self.enabled,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            home_user: repr.home_user,
            ledger: repr.ledger,
            platform: account_from_repr(repr.platform),
            fee_policy: DeliveryFeePolicy::from_repr(repr.fee_policy),
            governance: repr.governance,
            ledger_fee: repr.ledger_fee,
            max_fee: repr.max_fee,
            signer: ReceiptSigner::from_repr(repr.signer),
            max_open_per_payer: repr.max_open_per_payer,
            daily_orders: repr.daily_orders,
            enabled: repr.enabled,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct DepositRepr {
    #[cbor(key = 1)]
    pub block: u64,
    #[cbor(key = 2)]
    pub from: AccountRepr,
    #[cbor(key = 3)]
    pub amount: u128,
    #[cbor(key = 4)]
    pub committed_at: u64,
    #[cbor(key = 5)]
    pub refundable: u128,
}

impl StableCodec for Deposit {
    type Repr = DepositRepr;

    fn to_repr(&self) -> Self::Repr {
        DepositRepr {
            block: self.block,
            from: account_to_repr(&self.from),
            amount: self.amount,
            committed_at: self.committed_at,
            refundable: self.refundable,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            block: repr.block,
            from: account_from_repr(repr.from),
            amount: repr.amount,
            committed_at: repr.committed_at,
            refundable: repr.refundable,
        }
    }
}

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

impl From<&LegKind> for LegKindRepr {
    fn from(value: &LegKind) -> Self {
        match value {
            LegKind::Recipient => Self::Recipient,
            LegKind::Platform => Self::Platform,
            LegKind::Refund { funding_block } => Self::Refund {
                funding_block: *funding_block,
            },
            LegKind::ReserveRefund => Self::ReserveRefund,
        }
    }
}

impl From<LegKindRepr> for LegKind {
    fn from(value: LegKindRepr) -> Self {
        match value {
            LegKindRepr::Recipient => Self::Recipient,
            LegKindRepr::Platform => Self::Platform,
            LegKindRepr::Refund { funding_block } => Self::Refund { funding_block },
            LegKindRepr::ReserveRefund => Self::ReserveRefund,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct TransferLegRepr {
    #[cbor(key = 1)]
    pub escrow_id: Hash,
    #[cbor(key = 2)]
    pub leg_id: u64,
    #[cbor(key = 3)]
    pub kind: LegKindRepr,
    #[cbor(key = 4)]
    pub to: AccountRepr,
    #[cbor(key = 5)]
    pub amount: u128,
    #[cbor(key = 6)]
    pub fee: u128,
    #[cbor(key = 7)]
    pub memo: Hash,
    #[cbor(key = 8)]
    pub created_at_time: u64,
    #[cbor(key = 9)]
    pub status: LegStatus,
    #[cbor(key = 10)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<u64>,
    #[cbor(key = 11)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_fee: Option<u128>,
    #[cbor(key = 12)]
    pub revision: u64,
    #[cbor(key = 13)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<u64>,
    #[cbor(key = 14)]
    pub history_digest: Hash,
    #[cbor(key = 15)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure: Option<TransferFailure>,
}

impl StableCodec for TransferLeg {
    type Repr = TransferLegRepr;

    fn to_repr(&self) -> Self::Repr {
        TransferLegRepr {
            escrow_id: self.escrow_id,
            leg_id: self.leg_id,
            kind: (&self.kind).into(),
            to: account_to_repr(&self.to),
            amount: self.amount,
            fee: self.fee,
            memo: self.memo,
            created_at_time: self.created_at_time,
            status: self.status.clone(),
            block: self.block,
            expected_fee: self.expected_fee,
            last_failure: self.last_failure.clone(),
            revision: self.revision,
            replaces: self.replaces,
            history_digest: self.history_digest,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            escrow_id: repr.escrow_id,
            leg_id: repr.leg_id,
            kind: repr.kind.into(),
            to: account_from_repr(repr.to),
            amount: repr.amount,
            fee: repr.fee,
            memo: repr.memo,
            created_at_time: repr.created_at_time,
            status: repr.status,
            block: repr.block,
            expected_fee: repr.expected_fee,
            last_failure: repr.last_failure,
            revision: repr.revision,
            replaces: repr.replaces,
            history_digest: repr.history_digest,
        }
    }
}

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
        transport_key: ByteBuf,
    },
}

impl From<&ExecutionKind> for ExecutionKindRepr {
    fn from(value: &ExecutionKind) -> Self {
        match value {
            ExecutionKind::Sign {
                key,
                to_be_signed,
                public_key_fingerprint,
                origin,
            } => Self::Sign {
                key: key.to_repr(),
                to_be_signed: to_be_signed.clone(),
                public_key_fingerprint: *public_key_fingerprint,
                origin: origin.clone(),
            },
            ExecutionKind::Derive {
                generation,
                root_op_id,
                transport_key,
            } => Self::Derive {
                generation: *generation,
                root_op_id: *root_op_id,
                transport_key: transport_key.clone(),
            },
        }
    }
}

impl From<ExecutionKindRepr> for ExecutionKind {
    fn from(value: ExecutionKindRepr) -> Self {
        match value {
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

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct ExecutionGrantRepr {
    #[cbor(key = 13)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commerce: Option<CommercialReservationRepr>,
    #[cbor(key = 1)]
    pub account_id: AccountId,
    #[cbor(key = 2)]
    pub home_user: Principal,
    #[cbor(key = 3)]
    pub home_cose: Principal,
    #[cbor(key = 4)]
    pub request_id: OpId,
    #[cbor(key = 5)]
    pub execution_sequence: u64,
    #[cbor(key = 6)]
    pub security_epoch: u64,
    #[cbor(key = 7)]
    pub device_id: Hash,
    #[cbor(key = 8)]
    pub device_sequence: u64,
    #[cbor(key = 9)]
    pub approved_at: u64,
    #[cbor(key = 10)]
    pub expires_at: u64,
    #[cbor(key = 11)]
    pub kind: ExecutionKindRepr,
    #[cbor(key = 12)]
    pub max_cycles: u128,
}

impl StableCodec for ExecutionGrant {
    type Repr = ExecutionGrantRepr;

    fn to_repr(&self) -> Self::Repr {
        ExecutionGrantRepr {
            commerce: self.commerce.to_repr(),
            account_id: self.account_id.clone(),
            home_user: self.home_user,
            home_cose: self.home_cose,
            request_id: self.request_id,
            execution_sequence: self.execution_sequence,
            security_epoch: self.security_epoch,
            device_id: self.device_id,
            device_sequence: self.device_sequence,
            approved_at: self.approved_at,
            expires_at: self.expires_at,
            kind: (&self.kind).into(),
            max_cycles: self.max_cycles,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            commerce: Option::<CommercialReservation>::from_repr(repr.commerce),
            account_id: repr.account_id,
            home_user: repr.home_user,
            home_cose: repr.home_cose,
            request_id: repr.request_id,
            execution_sequence: repr.execution_sequence,
            security_epoch: repr.security_epoch,
            device_id: repr.device_id,
            device_sequence: repr.device_sequence,
            approved_at: repr.approved_at,
            expires_at: repr.expires_at,
            kind: repr.kind.into(),
            max_cycles: repr.max_cycles,
        }
    }
}

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

impl From<&ExecutionOutput> for ExecutionOutputRepr {
    fn from(value: &ExecutionOutput) -> Self {
        match value {
            ExecutionOutput::Signature { artifact, key } => Self::Signature {
                artifact: artifact.to_repr(),
                key: key.to_repr(),
            },
            ExecutionOutput::EncryptedRootKey { encrypted_key, key } => Self::EncryptedRootKey {
                encrypted_key: encrypted_key.clone(),
                key: key.to_repr(),
            },
        }
    }
}

impl From<ExecutionOutputRepr> for ExecutionOutput {
    fn from(value: ExecutionOutputRepr) -> Self {
        match value {
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

impl From<&ExecutionOutcome> for ExecutionOutcomeRepr {
    fn from(value: &ExecutionOutcome) -> Self {
        match value {
            ExecutionOutcome::Authorized => Self::Authorized,
            ExecutionOutcome::Executing => Self::Executing,
            ExecutionOutcome::Completed(output) => Self::Completed(Box::new((&**output).into())),
            ExecutionOutcome::Failed(error) => Self::Failed(error.clone()),
            ExecutionOutcome::Unknown(error) => Self::Unknown(error.clone()),
            ExecutionOutcome::ResultExpired => Self::ResultExpired,
        }
    }
}

impl From<ExecutionOutcomeRepr> for ExecutionOutcome {
    fn from(value: ExecutionOutcomeRepr) -> Self {
        match value {
            ExecutionOutcomeRepr::Authorized => Self::Authorized,
            ExecutionOutcomeRepr::Executing => Self::Executing,
            ExecutionOutcomeRepr::Completed(output) => Self::Completed(Box::new((*output).into())),
            ExecutionOutcomeRepr::Failed(error) => Self::Failed(error),
            ExecutionOutcomeRepr::Unknown(error) => Self::Unknown(error),
            ExecutionOutcomeRepr::ResultExpired => Self::ResultExpired,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct ExecutionResultRepr {
    #[cbor(key = 1)]
    pub request_id: OpId,
    #[cbor(key = 2)]
    pub outcome: ExecutionOutcomeRepr,
    #[cbor(key = 3)]
    pub cycles_cost_upper_bound: u128,
}

impl StableCodec for ExecutionResult {
    type Repr = ExecutionResultRepr;

    fn to_repr(&self) -> Self::Repr {
        ExecutionResultRepr {
            request_id: self.request_id,
            outcome: (&self.outcome).into(),
            cycles_cost_upper_bound: self.cycles_cost_upper_bound,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            request_id: repr.request_id,
            outcome: repr.outcome.into(),
            cycles_cost_upper_bound: repr.cycles_cost_upper_bound,
        }
    }
}

stable_struct!(MasterKeyRepr => MasterKey {
    1 => algorithm: Algorithm,
    2 => key_name: String,
    3 => expected_fingerprint: Hash,
});

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct CoseInitRepr {
    #[cbor(key = 1)]
    pub issuer_namespace: String,
    #[cbor(key = 2)]
    pub environment: Environment,
    #[cbor(key = 3)]
    pub executing_canister: Principal,
    #[cbor(key = 4)]
    pub initial_home_user: Principal,
    #[cbor(key = 5)]
    pub derivation_version: u16,
    #[cbor(key = 6)]
    pub masters: Vec<MasterKeyRepr>,
    #[cbor(key = 7)]
    pub daily_executions: u32,
    #[cbor(key = 8)]
    pub daily_cycles: u128,
}

impl StableCodec for CoseInit {
    type Repr = CoseInitRepr;

    fn to_repr(&self) -> Self::Repr {
        CoseInitRepr {
            issuer_namespace: self.issuer_namespace.clone(),
            environment: self.environment.clone(),
            executing_canister: self.executing_canister,
            initial_home_user: self.initial_home_user,
            derivation_version: self.derivation_version,
            masters: self.masters.iter().map(StableCodec::to_repr).collect(),
            daily_executions: self.daily_executions,
            daily_cycles: self.daily_cycles,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            issuer_namespace: repr.issuer_namespace,
            environment: repr.environment,
            executing_canister: repr.executing_canister,
            initial_home_user: repr.initial_home_user,
            derivation_version: repr.derivation_version,
            masters: repr.masters.into_iter().map(MasterKey::from_repr).collect(),
            daily_executions: repr.daily_executions,
            daily_cycles: repr.daily_cycles,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct KeyStateRepr {
    #[cbor(key = 1)]
    pub config: CoseInitRepr,
    #[cbor(key = 2)]
    pub initialization: Initialization,
    #[cbor(key = 3)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fingerprints: Vec<Hash>,
    #[cbor(key = 4)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl StableCodec for KeyState {
    type Repr = KeyStateRepr;

    fn to_repr(&self) -> Self::Repr {
        KeyStateRepr {
            config: self.config.to_repr(),
            initialization: self.initialization.clone(),
            fingerprints: self.fingerprints.clone(),
            error: self.error.clone(),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            config: CoseInit::from_repr(repr.config),
            initialization: repr.initialization,
            fingerprints: repr.fingerprints,
            error: repr.error,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct UserInitRepr {
    #[cbor(key = 8)]
    pub commerce_canister: Principal,
    #[cbor(key = 9)]
    pub membership_canister: Principal,
    #[cbor(key = 1)]
    pub environment: Environment,
    #[cbor(key = 2)]
    pub issuer_namespace: String,
    #[cbor(key = 3)]
    pub home_cose: Principal,
    #[cbor(key = 4)]
    pub handle_canister: Principal,
    #[cbor(key = 5)]
    pub payment_canister: Principal,
    #[cbor(key = 6)]
    pub max_accounts: u64,
    #[cbor(key = 7)]
    pub daily_new_accounts: u32,
}

impl StableCodec for UserInit {
    type Repr = UserInitRepr;

    fn to_repr(&self) -> Self::Repr {
        UserInitRepr {
            commerce_canister: self.commerce_canister,
            membership_canister: self.membership_canister,
            environment: self.environment.clone(),
            issuer_namespace: self.issuer_namespace.clone(),
            home_cose: self.home_cose,
            handle_canister: self.handle_canister,
            payment_canister: self.payment_canister,
            max_accounts: self.max_accounts,
            daily_new_accounts: self.daily_new_accounts,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            commerce_canister: repr.commerce_canister,
            membership_canister: repr.membership_canister,
            environment: repr.environment,
            issuer_namespace: repr.issuer_namespace,
            home_cose: repr.home_cose,
            handle_canister: repr.handle_canister,
            payment_canister: repr.payment_canister,
            max_accounts: repr.max_accounts,
            daily_new_accounts: repr.daily_new_accounts,
        }
    }
}

stable_struct!(BeneficiaryRepr => Beneficiary {
    1 => product_id: String,
    2 => authority_canister: Principal,
    3 => subject_schema: String,
    4 => subject_bytes: ByteBuf,
});

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct MembershipIntentRepr {
    #[cbor(key = 1)]
    pub application_id: Hash,
    #[cbor(key = 2)]
    pub environment: Environment,
    #[cbor(key = 3)]
    pub service_canister: Principal,
    #[cbor(key = 4)]
    pub beneficiary: BeneficiaryRepr,
    #[cbor(key = 5)]
    pub actor: Principal,
    #[cbor(key = 6)]
    pub action_digest: Hash,
    #[cbor(key = 7)]
    pub nonce: Hash,
    #[cbor(key = 8)]
    pub valid_until_ms: u64,
}

impl StableCodec for MembershipIntent {
    type Repr = MembershipIntentRepr;

    fn to_repr(&self) -> Self::Repr {
        MembershipIntentRepr {
            application_id: self.application_id,
            environment: self.environment.clone(),
            service_canister: self.service_canister,
            beneficiary: self.beneficiary.to_repr(),
            actor: self.actor,
            action_digest: self.action_digest,
            nonce: self.nonce,
            valid_until_ms: self.valid_until_ms,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            application_id: repr.application_id,
            environment: repr.environment,
            service_canister: repr.service_canister,
            beneficiary: Beneficiary::from_repr(repr.beneficiary),
            actor: repr.actor,
            action_digest: repr.action_digest,
            nonce: repr.nonce,
            valid_until_ms: repr.valid_until_ms,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub enum ThresholdRepr {
    FixedPanda {
        #[cbor(key = 1)]
        atomic: u128,
    },
    AnnualPrice {
        #[cbor(key = 2)]
        price_cents: u64,
        #[cbor(key = 3)]
        r_num: u128,
        #[cbor(key = 4)]
        r_den: u128,
    },
}

impl From<&Threshold> for ThresholdRepr {
    fn from(value: &Threshold) -> Self {
        match value {
            Threshold::FixedPanda { atomic } => Self::FixedPanda { atomic: *atomic },
            Threshold::AnnualPrice {
                price_cents,
                r_num,
                r_den,
            } => Self::AnnualPrice {
                price_cents: *price_cents,
                r_num: *r_num,
                r_den: *r_den,
            },
        }
    }
}

impl From<ThresholdRepr> for Threshold {
    fn from(value: ThresholdRepr) -> Self {
        match value {
            ThresholdRepr::FixedPanda { atomic } => Self::FixedPanda { atomic },
            ThresholdRepr::AnnualPrice {
                price_cents,
                r_num,
                r_den,
            } => Self::AnnualPrice {
                price_cents,
                r_num,
                r_den,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct MembershipPolicyRepr {
    #[cbor(key = 1)]
    pub version: u64,
    #[cbor(key = 2)]
    pub product_id: String,
    #[cbor(key = 3)]
    pub benefit_id: Hash,
    #[cbor(key = 4)]
    pub threshold: ThresholdRepr,
    #[cbor(key = 5)]
    pub effective_at_ms: u64,
    #[cbor(key = 6)]
    pub subsidy_units: u64,
}

impl StableCodec for MembershipPolicy {
    type Repr = MembershipPolicyRepr;

    fn to_repr(&self) -> Self::Repr {
        MembershipPolicyRepr {
            version: self.version,
            product_id: self.product_id.clone(),
            benefit_id: self.benefit_id,
            threshold: (&self.threshold).into(),
            effective_at_ms: self.effective_at_ms,
            subsidy_units: self.subsidy_units,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            version: repr.version,
            product_id: repr.product_id,
            benefit_id: repr.benefit_id,
            threshold: repr.threshold.into(),
            effective_at_ms: repr.effective_at_ms,
            subsidy_units: repr.subsidy_units,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub enum TermRuleRepr {
    CalendarYear,
    Fixed {
        #[cbor(key = 1)]
        starts_at_ms: u64,
        #[cbor(key = 2)]
        expires_at_ms: u64,
    },
}

impl From<&TermRule> for TermRuleRepr {
    fn from(value: &TermRule) -> Self {
        match value {
            TermRule::CalendarYear => Self::CalendarYear,
            TermRule::Fixed {
                starts_at_ms,
                expires_at_ms,
            } => Self::Fixed {
                starts_at_ms: *starts_at_ms,
                expires_at_ms: *expires_at_ms,
            },
        }
    }
}

impl From<TermRuleRepr> for TermRule {
    fn from(value: TermRuleRepr) -> Self {
        match value {
            TermRuleRepr::CalendarYear => Self::CalendarYear,
            TermRuleRepr::Fixed {
                starts_at_ms,
                expires_at_ms,
            } => Self::Fixed {
                starts_at_ms,
                expires_at_ms,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub enum ClaimChangeRepr {
    Start,
    Renew {
        #[cbor(key = 1)]
        previous_claim: Hash,
    },
    Upgrade {
        #[cbor(key = 1)]
        previous_claim: Hash,
    },
    Replace {
        #[cbor(key = 1)]
        previous_claim: Hash,
    },
}

impl From<&ClaimChange> for ClaimChangeRepr {
    fn from(value: &ClaimChange) -> Self {
        match value {
            ClaimChange::Start => Self::Start,
            ClaimChange::Renew { previous_claim } => Self::Renew {
                previous_claim: *previous_claim,
            },
            ClaimChange::Upgrade { previous_claim } => Self::Upgrade {
                previous_claim: *previous_claim,
            },
            ClaimChange::Replace { previous_claim } => Self::Replace {
                previous_claim: *previous_claim,
            },
        }
    }
}

impl From<ClaimChangeRepr> for ClaimChange {
    fn from(value: ClaimChangeRepr) -> Self {
        match value {
            ClaimChangeRepr::Start => Self::Start,
            ClaimChangeRepr::Renew { previous_claim } => Self::Renew { previous_claim },
            ClaimChangeRepr::Upgrade { previous_claim } => Self::Upgrade { previous_claim },
            ClaimChangeRepr::Replace { previous_claim } => Self::Replace { previous_claim },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct ClaimRequestRepr {
    #[cbor(key = 1)]
    pub authorization: MembershipIntentRepr,
    #[cbor(key = 2)]
    pub neuron_id: Hash,
    #[cbor(key = 3)]
    pub policy_version: u64,
    #[cbor(key = 4)]
    pub benefit_id: Hash,
    #[cbor(key = 5)]
    pub expected_business_revision: u64,
    #[cbor(key = 6)]
    pub term: TermRuleRepr,
    #[cbor(key = 7)]
    pub change: ClaimChangeRepr,
}

impl StableCodec for ClaimRequest {
    type Repr = ClaimRequestRepr;

    fn to_repr(&self) -> Self::Repr {
        ClaimRequestRepr {
            authorization: self.authorization.to_repr(),
            neuron_id: self.neuron_id,
            policy_version: self.policy_version,
            benefit_id: self.benefit_id,
            expected_business_revision: self.expected_business_revision,
            term: (&self.term).into(),
            change: (&self.change).into(),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            authorization: MembershipIntent::from_repr(repr.authorization),
            neuron_id: repr.neuron_id,
            policy_version: repr.policy_version,
            benefit_id: repr.benefit_id,
            expected_business_revision: repr.expected_business_revision,
            term: repr.term.into(),
            change: repr.change.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct ClaimViewRepr {
    #[cbor(key = 1)]
    pub schema: u16,
    #[cbor(key = 2)]
    pub home_membership: Principal,
    #[cbor(key = 3)]
    pub claim_id: Hash,
    #[cbor(key = 4)]
    pub beneficiary: BeneficiaryRepr,
    #[cbor(key = 5)]
    pub benefit_id: Hash,
    #[cbor(key = 6)]
    pub policy_version: u64,
    #[cbor(key = 7)]
    pub status: ClaimStatus,
    #[cbor(key = 8)]
    pub eligibility: Eligibility,
    #[cbor(key = 9)]
    pub starts_at_ms: u64,
    #[cbor(key = 10)]
    pub expires_at_ms: u64,
    #[cbor(key = 11)]
    pub observed_at_ms: u64,
    #[cbor(key = 12)]
    pub valid_until_ms: u64,
    #[cbor(key = 13)]
    pub lease_revision: u64,
    #[cbor(key = 14)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_id: Option<Hash>,
    #[cbor(key = 15)]
    pub release_after_ms: u64,
}

impl StableCodec for ClaimView {
    type Repr = ClaimViewRepr;

    fn to_repr(&self) -> Self::Repr {
        ClaimViewRepr {
            schema: self.schema,
            home_membership: self.home_membership,
            claim_id: self.claim_id,
            beneficiary: self.beneficiary.to_repr(),
            benefit_id: self.benefit_id,
            policy_version: self.policy_version,
            status: self.status.clone(),
            eligibility: self.eligibility.clone(),
            starts_at_ms: self.starts_at_ms,
            expires_at_ms: self.expires_at_ms,
            observed_at_ms: self.observed_at_ms,
            valid_until_ms: self.valid_until_ms,
            lease_revision: self.lease_revision,
            decision_id: self.decision_id,
            release_after_ms: self.release_after_ms,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            schema: repr.schema,
            home_membership: repr.home_membership,
            claim_id: repr.claim_id,
            beneficiary: Beneficiary::from_repr(repr.beneficiary),
            benefit_id: repr.benefit_id,
            policy_version: repr.policy_version,
            status: repr.status,
            eligibility: repr.eligibility,
            starts_at_ms: repr.starts_at_ms,
            expires_at_ms: repr.expires_at_ms,
            observed_at_ms: repr.observed_at_ms,
            valid_until_ms: repr.valid_until_ms,
            lease_revision: repr.lease_revision,
            decision_id: repr.decision_id,
            release_after_ms: repr.release_after_ms,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct MembershipDecisionRepr {
    #[cbor(key = 1)]
    pub decision_id: Hash,
    #[cbor(key = 2)]
    pub claim_id: Hash,
    #[cbor(key = 3)]
    pub kind: DecisionKind,
    #[cbor(key = 4)]
    pub request: ClaimRequestRepr,
    #[cbor(key = 5)]
    pub policy: MembershipPolicyRepr,
    #[cbor(key = 6)]
    pub required_atomic: u128,
    #[cbor(key = 7)]
    pub starts_at_ms: u64,
    #[cbor(key = 8)]
    pub expires_at_ms: u64,
    #[cbor(key = 9)]
    pub apply_by_ms: u64,
    #[cbor(key = 10)]
    pub observed_at_ms: u64,
    #[cbor(key = 11)]
    pub qualification_until_ms: u64,
}

impl StableCodec for MembershipDecision {
    type Repr = MembershipDecisionRepr;

    fn to_repr(&self) -> Self::Repr {
        MembershipDecisionRepr {
            decision_id: self.decision_id,
            claim_id: self.claim_id,
            kind: self.kind.clone(),
            request: self.request.to_repr(),
            policy: self.policy.to_repr(),
            required_atomic: self.required_atomic,
            starts_at_ms: self.starts_at_ms,
            expires_at_ms: self.expires_at_ms,
            apply_by_ms: self.apply_by_ms,
            observed_at_ms: self.observed_at_ms,
            qualification_until_ms: self.qualification_until_ms,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            decision_id: repr.decision_id,
            claim_id: repr.claim_id,
            kind: repr.kind,
            request: ClaimRequest::from_repr(repr.request),
            policy: MembershipPolicy::from_repr(repr.policy),
            required_atomic: repr.required_atomic,
            starts_at_ms: repr.starts_at_ms,
            expires_at_ms: repr.expires_at_ms,
            apply_by_ms: repr.apply_by_ms,
            observed_at_ms: repr.observed_at_ms,
            qualification_until_ms: repr.qualification_until_ms,
        }
    }
}

stable_struct!(MembershipDecisionReceiptRepr => MembershipDecisionReceipt {
    1 => decision_id: Hash,
    2 => decision_digest: Hash,
    3 => outcome: DecisionOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    4 => contract_id: Option<Hash>,
    5 => starts_at_ms: u64,
    6 => expires_at_ms: u64,
    7 => business_revision: u64,
    8 => commitment_until_ms: u64,
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
                transport_key: vec![1; 48].into(),
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
