//! Versioned, integer-keyed local records; public certification bytes stay independent.
use crate::{
    checkout_model::{Balance, Order, Transfer},
    checkout_store::Asset,
    model::Subject,
    store::{Budget, Config},
};
use candid::Principal;
use cbor2::Cbor;
use dmsg_runtime::storage::StableCodec;
use dmsg_types::{
    billing::*, integration::*, integration_billing::*, membership::*, Environment, Hash,
};
use std::collections::BTreeMap;

const SCHEMA: u16 = 8;

macro_rules! record {
    ($repr:ident => $domain:ident { $($key:literal => $field:ident: $ty:ty),+ $(,)? }) => {
        #[derive(Clone, Cbor)]
        pub struct $repr {
            #[cbor(key = 0)]
            pub schema: u16,
            $(#[cbor(key = $key)] pub $field: $ty,)+
        }

        impl StableCodec for $domain {
            type Repr = $repr;

            fn to_repr(&self) -> Self::Repr {
                $repr { schema: SCHEMA, $($field: self.$field.clone(),)+ }
            }

            fn from_repr(repr: Self::Repr) -> Self {
                assert_eq!(repr.schema, SCHEMA, "incompatible development state");
                Self { $($field: repr.$field,)+ }
            }
        }
    };
}

record!(SubjectRepr => Subject {
    1 => beneficiary: Beneficiary,
    2 => created_at_ms: Option<u64>,
    3 => business_revision: u64,
    4 => lease_revision: u64,
    5 => contracts: Vec<MembershipContract>,
    6 => addons: Vec<StorageAddon>,
    9 => view: Option<EntitlementView>,
    10 => last_service_end_ms: Option<u64>,
    12 => retry_after_ms: u64,
});

record!(ConfigRepr => Config {
    1 => environment: Environment,
    2 => governance: Principal,
    3 => membership_canister: Principal,
    4 => user_homes: Vec<Principal>,
    5 => limits: CommerceLimits,
    6 => paused: bool,
    7 => day: u64,
    8 => orders: u32,
    9 => minute: u64,
    10 => reads: Budget,
    11 => refreshes: Budget,
    12 => authorizations: Budget,
});

record!(AssetRepr => Asset {
    1 => policy: SettlementAsset,
    2 => verified: bool,
    3 => fee: u128,
});

record!(OrderRepr => Order {
    1 => id: Hash,
    2 => quote: CheckoutQuote,
    3 => authorization: Option<ProductAuthorizationRequest>,
    4 => input_hash: Hash,
    5 => status: CheckoutStatus,
    6 => balances: BTreeMap<Principal, Balance>,
    7 => funding: Option<CashBlock>,
    8 => decision: Option<ProductDecision>,
    9 => receipt: Option<ProductReceipt>,
    10 => cancellation: Option<CashCancellationReceipt>,
    11 => cancellation_pending: bool,
    12 => reservation_released: bool,
    13 => next_transfer: u64,
    14 => earned_allocated: u128,
    15 => pending_transfers: u32,
    16 => updated_at_ms: u64,
});

record!(TransferRepr => Transfer {
    1 => view: CashTransfer,
    2 => updated_at_ms: u64,
});

#[cfg(test)]
mod tests {
    use super::*;
    use dmsg_protocol::billing::beneficiary;
    use dmsg_runtime::storage::{compact_bytes, compact_from_bytes};
    use dmsg_types::AccountId;

    #[test]
    fn local_subject_uses_integer_keys_without_changing_public_projection() {
        let mut subject = Subject::new(beneficiary(
            Principal::from_slice(&[1]),
            &AccountId([2; 12]),
        ));
        subject.retry_after_ms = 42;
        let bytes = compact_bytes(&subject);
        assert_eq!(compact_from_bytes::<Subject>(&bytes), subject);
        let cbor2::Value::Map(fields) = cbor2::from_slice(&bytes).unwrap() else {
            panic!("record")
        };
        assert!(fields
            .iter()
            .all(|(key, _)| matches!(key, cbor2::Value::Integer(_))));
        assert!(bytes.len() < cbor2::to_vec(&subject).unwrap().len());
    }
}
