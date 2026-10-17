//! Compact stable representation of the hub record. Integer keys stand in
//! for the field names of the configuration and of every version.
use crate::store::Hub;
use candid::Principal;
use cbor2::Cbor;
use dmsg_runtime::stable_types::wallet::{HubInitRepr, WalletVersionRepr};
use dmsg_runtime::storage::StableCodec;
use dmsg_types::wallet::*;

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub(crate) struct HubRepr {
    #[cbor(key = 1)]
    schema: u16,
    #[cbor(key = 2)]
    init: HubInitRepr,
    #[cbor(key = 3)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    versions: Vec<WalletVersionRepr>,
    #[cbor(key = 4)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    current: Option<u32>,
    #[cbor(key = 5)]
    next_version: u32,
    #[cbor(key = 6)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    uploader: Option<Principal>,
}

impl StableCodec for Hub {
    type Repr = HubRepr;

    fn to_repr(&self) -> Self::Repr {
        HubRepr {
            schema: self.schema,
            init: self.init.to_repr(),
            versions: self.versions.to_repr(),
            current: self.current,
            next_version: self.next_version,
            uploader: self.uploader,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            schema: repr.schema,
            init: HubInit::from_repr(repr.init),
            versions: StableCodec::from_repr(repr.versions),
            current: repr.current,
            next_version: repr.next_version,
            uploader: repr.uploader,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{OrderRecord, Payout, PayoutTransfer, STABLE_SCHEMA};
    use dmsg_runtime::storage::{compact_bytes, compact_from_bytes};
    use dmsg_types::{billing::PlanId, *};
    use icrc_ledger_types::icrc1::account::Account;

    fn p(n: u8, len: usize) -> Principal {
        Principal::from_slice(&vec![n; len])
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

    fn init() -> HubInit {
        HubInit {
            environment: Environment::Production,
            governance: p(11, 10),
            icp_ledger: p(12, 10),
            cmc: p(13, 10),
            commerce: p(14, 10),
            issuer_namespace: "https://dmsg.net/u/".into(),
            user_homes: (0..4).map(|i| p(20 + i, 10)).collect(),
            key_name: "key_1".into(),
            btc_network: BtcNetwork::Mainnet,
            pricing: HubPricing {
                price_e8s: 250_000_000,
                cycles_to_wallet: 3_000_000_000_000,
                cycles_share_e8s: 100_000_000,
                quotas: vec![
                    PlanQuota {
                        plan: PlanId::Plus,
                        wallets: 1,
                    },
                    PlanQuota {
                        plan: PlanId::Pro,
                        wallets: 3,
                    },
                    PlanQuota {
                        plan: PlanId::Max,
                        wallets: 10,
                    },
                ],
                paused: false,
            },
            revenue: Account {
                owner: p(15, 10),
                subaccount: None,
            },
        }
    }

    fn quote() -> ActivationQuote {
        ActivationQuote {
            quote_id: Hash::new([7; 32]),
            account_id: AccountId([3; 12]),
            payer: p(9, 29),
            plan: PlanId::Pro,
            remaining: 3,
            price_e8s: 250_000_000,
            ledger_fee_e8s: 10_000,
            cycles_to_wallet: 3_000_000_000_000,
            deposit: Account {
                owner: p(1, 10),
                subaccount: Some([5; 32]),
            },
            version: 3,
            issued_at_ms: 1_790_000_000_000,
            expires_at_ms: 1_790_000_900_000,
        }
    }

    fn order() -> ActivationOrder {
        let member = |id, role, label: &str| MemberInput {
            id,
            role,
            label: label.into(),
            agent_ids: vec![],
        };
        ActivationOrder {
            quote: quote(),
            spec: WalletSpec {
                name: "ICPanda DAO".into(),
                members: vec![
                    member(MemberId::Account(AccountId([4; 12])), Role::Approver, "ops"),
                    member(MemberId::Account(AccountId([5; 12])), Role::Admin, "lead"),
                    member(MemberId::Principal(p(6, 29)), Role::Recovery, "recovery"),
                ],
                policy: Policy {
                    version: 1,
                    approver_threshold: 2,
                    admin_threshold: 1,
                    tiers: vec![Tier {
                        asset: AssetKey::Icp,
                        up_to: 1_000_000_000,
                        approvals: 2,
                        delay_ms: 0,
                        book_only: false,
                    }],
                    address_book_cooldown_ms: 0,
                    daily_limits: vec![],
                    spendable_assets: vec![AssetKey::Icp, AssetKey::Btc],
                    call_targets: vec![],
                    evm_chains: vec![1],
                    proposal_ttl_ms: 7 * DAY,
                    unfreeze_delay_ms: DAY,
                    daily_signatures: 50,
                    refuel: None,
                    hardware_cosigners: false,
                    public: false,
                },
                channel_hint: None,
            },
            status: ActivationStatus::Ready,
            wallet: Some(p(2, 10)),
            collected: false,
            updated_at_ms: 1_790_000_100_000,
        }
    }

    #[test]
    fn hub_record_round_trips_with_integer_keys() {
        let hub = Hub {
            schema: STABLE_SCHEMA,
            init: init(),
            versions: (1..=3)
                .map(|n| WalletVersion {
                    version: n,
                    module_hash: Hash::new([n as u8; 32]),
                    size: 950_206,
                    notes: format!("release {n}"),
                    registered_at_ms: 1_790_000_000_000,
                    retired: n == 1,
                })
                .collect(),
            current: Some(3),
            next_version: 4,
            uploader: Some(p(16, 29)),
        };
        let bytes = compact_bytes(&hub);
        assert_eq!(compact_from_bytes::<Hub>(&bytes), hub);
        assert_eq!(top_keys(&bytes), (1..=6).collect::<Vec<_>>());
        let plain = cbor2::to_vec(&(&hub.init, &hub.versions)).unwrap();
        assert!(
            bytes.len() * 100 <= plain.len() * 70,
            "hub {} !<= 70% of {}",
            bytes.len(),
            plain.len()
        );

        let fresh = Hub {
            versions: vec![],
            current: None,
            next_version: 1,
            uploader: None,
            ..hub
        };
        let bytes = compact_bytes(&fresh);
        assert_eq!(compact_from_bytes::<Hub>(&bytes), fresh);
        assert_eq!(top_keys(&bytes), vec![1, 2, 5]);
    }

    #[test]
    fn order_records_store_the_quote_once() {
        let order = order();
        let record = cbor2::to_vec(&OrderRecord::new(&order)).unwrap();
        let back: OrderRecord = cbor2::from_slice(&record).unwrap();
        assert_eq!(back.order(order.quote.clone()), order);
        assert_eq!(top_keys(&record), vec![1, 2, 3, 5]);
        // The quote and the order record replace the order that embedded the
        // quote and the quote stored on its own.
        let stored = record.len() + compact_bytes(&order.quote).len();
        let plain =
            cbor2::to_vec(&order).unwrap().len() + cbor2::to_vec(&order.quote).unwrap().len();
        assert!(
            stored * 100 <= plain * 55,
            "activation {stored} !<= 55% of {plain}"
        );
    }

    #[test]
    fn payouts_round_trip_with_integer_keys() {
        let payout = Payout {
            refund: false,
            created_at_time: 1_790_000_000_000_000_000,
            transfers: vec![
                PayoutTransfer {
                    to: Account {
                        owner: p(13, 10),
                        subaccount: Some([1; 32]),
                    },
                    amount: 100_000_000,
                    memo: Some(0x5055_5054u64.to_le_bytes().to_vec().into()),
                    block: Some(42),
                },
                PayoutTransfer {
                    to: Account {
                        owner: p(15, 10),
                        subaccount: None,
                    },
                    amount: 149_980_000,
                    memo: None,
                    block: None,
                },
            ],
            cycles: Some(1_000_000_000_000),
        };
        let bytes = cbor2::to_vec(&payout).unwrap();
        assert_eq!(cbor2::from_slice::<Payout>(&bytes).unwrap(), payout);
        assert_eq!(top_keys(&bytes), vec![2, 3, 4]);
    }
}
