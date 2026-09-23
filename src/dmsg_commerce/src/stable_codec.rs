//! Versioned, integer-keyed local records; public certification bytes stay independent.
use crate::{model::Subject, store::Config};
use candid::Principal;
use cbor2::Cbor;
use dmsg_runtime::storage::StableCodec;
use dmsg_types::{billing::*, membership::*, *};
use std::collections::BTreeMap;

const SCHEMA: u16 = 2;

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
    7 => first_cash_order: Option<Hash>,
    8 => self_refund_used: bool,
    9 => view: Option<EntitlementView>,
    10 => busy_until_ms: u64,
    11 => generation: u64,
    12 => retry_after_ms: u64,
});

record!(ConfigRepr => Config {
    1 => init: CommerceInit,
    2 => paused: bool,
    3 => ledger_verified: bool,
    4 => day: u64,
    5 => orders: u32,
    6 => minute: u64,
    7 => reads: u32,
    8 => refreshes: u32,
    9 => authorizations: BTreeMap<Principal, u32>,
    10 => ledger_fee: u128,
});

#[cfg(test)]
mod tests {
    use super::*;
    use dmsg_protocol::billing::beneficiary;
    use dmsg_runtime::storage::{compact_bytes, compact_from_bytes};

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
