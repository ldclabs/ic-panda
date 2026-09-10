use crate::store::{Config, TransferReceipt};
use cbor2::Cbor;
use dmsg_runtime::{stable_types::*, storage::StableCodec};
use dmsg_types::{handle::*, *};

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct ConfigRepr {
    #[cbor(key = 1)]
    pub schema: u16,
    #[cbor(key = 2)]
    pub init: HandleInitRepr,
    #[cbor(key = 3)]
    pub progress: SnapshotProgressRepr,
    #[cbor(key = 4)]
    pub event_count: u64,
    #[cbor(key = 5)]
    pub event_tip: Hash,
    #[cbor(key = 6)]
    pub pending: u32,
}

impl StableCodec for Config {
    type Repr = ConfigRepr;

    fn to_repr(&self) -> Self::Repr {
        ConfigRepr {
            schema: self.schema,
            init: self.init.to_repr(),
            progress: self.progress.to_repr(),
            event_count: self.event_count,
            event_tip: self.event_tip,
            pending: self.pending,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            schema: repr.schema,
            init: HandleInit::from_repr(repr.init),
            progress: SnapshotProgress::from_repr(repr.progress),
            event_count: repr.event_count,
            event_tip: repr.event_tip,
            pending: repr.pending,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct TransferReceiptRepr {
    #[cbor(key = 1)]
    pub digest: Hash,
    #[cbor(key = 2)]
    pub record: HandleRecordRepr,
}

impl StableCodec for TransferReceipt {
    type Repr = TransferReceiptRepr;

    fn to_repr(&self) -> Self::Repr {
        TransferReceiptRepr {
            digest: self.digest,
            record: self.record.to_repr(),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            digest: repr.digest,
            record: HandleRecord::from_repr(repr.record),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candid::Principal;
    use dmsg_runtime::storage::{compact_bytes, compact_from_bytes};
    use icrc_ledger_types::icrc1::account::Account;

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
    fn every_handle_record_round_trips_with_integer_keys() {
        let intent = HandleIntent {
            handle_canister: p(7),
            action: HandleAction::Transfer,
            account_id: AccountId([1; 12]),
            target_account: Some(AccountId([2; 12])),
            handle: "example-user".into(),
            expected_version: 42,
            op_id: Hash::new([3; 32]),
            terms_digest: Hash::new([4; 32]),
        };
        let operation = HandleOperation {
            registration: Registration {
                intent,
                payer: Account {
                    owner: p(1),
                    subaccount: Some([5; 32]),
                },
                fee: 1_000_000,
            },
            digest: Hash::new([6; 32]),
            phase: HandlePhase::Charging,
            amount: 1_000_000,
            created_at: 1_700_000_000_000,
            expires_at: 1_700_000_900_000,
            memo: Hash::new([7; 32]),
            ledger_block: Some(99),
        };
        let record = HandleRecord {
            handle: "example-user".into(),
            owner_account: AccountId([1; 12]),
            version: 42,
            event_tip: Hash::new([8; 32]),
        };
        let legacy = LegacyReservation {
            handle: "example-user".into(),
            legacy_owner: p(1),
            legacy_name_principal: Some(p(2)),
            frozen_admins: vec![p(3), p(4), p(5)],
            quarantined: false,
        };
        let event = HandleEvent {
            sequence: 42,
            previous: Hash::new([9; 32]),
            handle: "example-user".into(),
            from: Some(AccountId([1; 12])),
            to: AccountId([2; 12]),
            version: 43,
            at: 1_700_000_000_000,
            legacy_snapshot: Hash::new([10; 32]),
        };

        let record_bytes = compact_bytes(&record);
        assert_public_text_keys(&record);
        assert_integer_top_keys(&record_bytes, 4);
        assert_eq!(compact_from_bytes::<HandleRecord>(&record_bytes), record);
        assert!(record_bytes.len() * 100 <= cbor2::to_vec(&record).unwrap().len() * 70);

        let legacy_bytes = compact_bytes(&legacy);
        assert_integer_top_keys(&legacy_bytes, 5);
        assert_eq!(
            compact_from_bytes::<LegacyReservation>(&legacy_bytes),
            legacy
        );

        let operation_bytes = compact_bytes(&operation);
        assert_eq!(operation_bytes.len(), 290);
        assert_eq!(
            hex(&operation_bytes),
            "3e445e35c8a5d0ffbc876bbfe42cd48abff47dd02ab06dedf9d9483a1b9c2a29"
        );
        assert_integer_top_keys(&operation_bytes, 8);
        assert_eq!(
            compact_from_bytes::<HandleOperation>(&operation_bytes),
            operation
        );
        assert!(operation_bytes.len() * 100 <= cbor2::to_vec(&operation).unwrap().len() * 65);

        let event_bytes = compact_bytes(&event);
        assert_integer_top_keys(&event_bytes, 8);
        assert_eq!(compact_from_bytes::<HandleEvent>(&event_bytes), event);

        let receipt = TransferReceipt {
            digest: Hash::new([11; 32]),
            record,
        };
        assert_eq!(
            compact_from_bytes::<TransferReceipt>(&compact_bytes(&receipt)),
            receipt
        );

        let sparse_legacy = LegacyReservation {
            handle: "quarantined".into(),
            legacy_owner: p(1),
            legacy_name_principal: None,
            frozen_admins: vec![],
            quarantined: true,
        };
        assert_eq!(
            compact_from_bytes::<LegacyReservation>(&compact_bytes(&sparse_legacy)),
            sparse_legacy
        );
        let mut sparse_event = event;
        sparse_event.from = None;
        assert_eq!(
            compact_from_bytes::<HandleEvent>(&compact_bytes(&sparse_event)),
            sparse_event
        );
    }

    fn hex(bytes: &[u8]) -> String {
        let digest = dmsg_protocol::sha256(bytes);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn handle_config_round_trips() {
        let config = Config {
            schema: 3,
            init: HandleInit {
                home_user: p(1),
                ledger: p(2),
                ledger_fee: 10_000,
                max_pending: 1_000,
            },
            progress: SnapshotProgress {
                snapshot: Some(LegacySnapshot {
                    source_canister: p(3),
                    snapshot_id: Hash::new([1; 32]),
                    freeze_version: 2,
                    event_tip: Hash::new([2; 32]),
                    count: 100,
                    entries_digest: Hash::new([3; 32]),
                }),
                imported: 50,
                rolling_digest: Hash::new([4; 32]),
                last_handle: Some("halfway".into()),
                sealed: false,
            },
            event_count: 4,
            event_tip: Hash::new([5; 32]),
            pending: 2,
        };
        let decoded = compact_from_bytes::<Config>(&compact_bytes(&config));
        assert_eq!(decoded.schema, config.schema);
        assert_eq!(decoded.init, config.init);
        assert_eq!(decoded.progress.snapshot, config.progress.snapshot);
        assert_eq!(decoded.progress.imported, config.progress.imported);
        assert_eq!(
            decoded.progress.rolling_digest,
            config.progress.rolling_digest
        );
        assert_eq!(decoded.progress.last_handle, config.progress.last_handle);
        assert_eq!(decoded.progress.sealed, config.progress.sealed);
        assert_eq!(decoded.event_count, config.event_count);
        assert_eq!(decoded.event_tip, config.event_tip);
        assert_eq!(decoded.pending, config.pending);
    }
}
