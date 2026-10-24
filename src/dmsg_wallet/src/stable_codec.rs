//! Compact stable representation of the wallet record. Integer keys stand in
//! for the field names every member and address would otherwise repeat.
use crate::store::{EvmPending, Settings, SolLast, Spend, Wallet};
use cbor2::Cbor;
use dmsg_runtime::stable_types::{is_zero, wallet::*};
use dmsg_runtime::storage::StableCodec;
use dmsg_types::{wallet::*, *};

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub(crate) struct WalletRepr {
    #[cbor(key = 1)]
    schema: u16,
    #[cbor(key = 2)]
    settings: Settings,
    #[cbor(key = 3)]
    created_at_ms: u64,
    #[cbor(key = 4)]
    members: Vec<MemberRepr>,
    #[cbor(key = 5)]
    policy: Policy,
    #[cbor(key = 6)]
    addresses: Vec<AddressRepr>,
    #[cbor(key = 7)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    cosigners: Vec<CosignerRepr>,
    #[cbor(key = 8)]
    next_address: u32,
    #[cbor(key = 9)]
    next_cosigner: u32,
    #[cbor(key = 10)]
    next_proposal: u64,
    #[cbor(key = 11)]
    #[serde(default, skip_serializing_if = "is_zero")]
    frozen: bool,
    #[cbor(key = 12)]
    code: CodeAcceptance,
    #[cbor(key = 13)]
    key_roots: KeyRoots,
    #[cbor(key = 14)]
    audit_head: Hash,
    #[cbor(key = 15)]
    day: u64,
    #[cbor(key = 16)]
    #[serde(default, skip_serializing_if = "is_zero")]
    signatures_today: u32,
    #[cbor(key = 17)]
    #[serde(default, skip_serializing_if = "is_zero")]
    refuels_today: u8,
    #[cbor(key = 18)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    evm_pending: Vec<EvmPending>,
    #[cbor(key = 19)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sol_last: Vec<SolLast>,
    #[cbor(key = 20)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    spends: Vec<Spend>,
    #[cbor(key = 21)]
    #[serde(default, skip_serializing_if = "is_zero")]
    code_busy: bool,
}

impl StableCodec for Wallet {
    type Repr = WalletRepr;

    fn to_repr(&self) -> Self::Repr {
        WalletRepr {
            schema: self.schema,
            settings: self.settings.clone(),
            created_at_ms: self.created_at_ms,
            members: self.members.to_repr(),
            policy: self.policy.clone(),
            addresses: self.addresses.to_repr(),
            cosigners: self.cosigners.to_repr(),
            next_address: self.next_address,
            next_cosigner: self.next_cosigner,
            next_proposal: self.next_proposal,
            frozen: self.frozen,
            code: self.code.clone(),
            code_busy: self.code_busy,
            key_roots: self.key_roots.clone(),
            audit_head: self.audit_head,
            day: self.day,
            signatures_today: self.signatures_today,
            refuels_today: self.refuels_today,
            evm_pending: self.evm_pending.clone(),
            sol_last: self.sol_last.clone(),
            spends: self.spends.clone(),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            schema: repr.schema,
            settings: repr.settings,
            created_at_ms: repr.created_at_ms,
            members: StableCodec::from_repr(repr.members),
            policy: repr.policy,
            addresses: StableCodec::from_repr(repr.addresses),
            cosigners: StableCodec::from_repr(repr.cosigners),
            next_address: repr.next_address,
            next_cosigner: repr.next_cosigner,
            next_proposal: repr.next_proposal,
            frozen: repr.frozen,
            code: repr.code,
            code_busy: repr.code_busy,
            key_roots: repr.key_roots,
            audit_head: repr.audit_head,
            day: repr.day,
            signatures_today: repr.signatures_today,
            refuels_today: repr.refuels_today,
            evm_pending: repr.evm_pending,
            sol_last: repr.sol_last,
            spends: repr.spends,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::STABLE_SCHEMA;
    use candid::Principal;
    use dmsg_runtime::storage::{CompactStored, compact_bytes, compact_from_bytes};
    use ic_stable_structures::{StableCell, VectorMemory};

    fn p(n: u32) -> Principal {
        Principal::from_slice(&[n as u8, (n >> 8) as u8, 7, 7, 7, 7, 7, 7, 1, 1])
    }

    fn policy() -> Policy {
        Policy {
            version: 3,
            approver_threshold: 2,
            admin_threshold: 2,
            tiers: (0..4)
                .map(|i| Tier {
                    asset: AssetKey::Icp,
                    up_to: 10u128.pow(8 + i),
                    approvals: 1 + i as u8 / 2,
                    delay_ms: u64::from(i) * 3_600_000,
                    book_only: i > 1,
                })
                .collect(),
            address_book_cooldown_ms: DAY,
            daily_limits: vec![AssetLimit {
                asset: AssetKey::Icp,
                amount: 10u128.pow(12),
            }],
            spendable_assets: vec![AssetKey::Icp, AssetKey::Btc],
            call_targets: vec![p(500)],
            evm_chains: vec![1, 8453],
            proposal_ttl_ms: 7 * DAY,
            unfreeze_delay_ms: DAY,
            daily_signatures: 100,
            refuel: None,
            hardware_cosigners: false,
            public: false,
        }
    }

    /// A wallet of `members` members with four Principals each and
    /// `addresses` addresses.
    fn wallet(members: u32, addresses: u32) -> Wallet {
        Wallet {
            schema: STABLE_SCHEMA,
            settings: Settings {
                environment: Environment::Production,
                hub: p(900),
                key_name: "key_1".into(),
                btc_network: BtcNetwork::Mainnet,
                icp_ledger: p(901),
                cmc: p(902),
                issuer_namespace: "https://dmsg.net/u/".into(),
                user_homes: vec![p(903), p(904)],
                name: "Team treasury".into(),
                channel_hint: None,
            },
            created_at_ms: 1_790_000_000_000,
            members: (0..members)
                .map(|n| Member {
                    id: MemberId::Account(AccountId([n as u8; 12])),
                    role: if n == 0 { Role::Admin } else { Role::Approver },
                    label: format!("member {n}"),
                    agent_ids: vec![],
                    status: MemberStatus::Active,
                    principals: (0..4).map(|i| p(n * 8 + i)).collect(),
                    added_at_ms: 1_790_000_000_000,
                })
                .collect(),
            policy: policy(),
            addresses: (0..addresses)
                .map(|id| {
                    let (chain, text, key) = match id % 4 {
                        0 => (Chain::Icp, format!("{}-abcdefg.{id}", p(id)), vec![]),
                        1 => (
                            Chain::Bitcoin,
                            format!("bc1p{}", "q".repeat(58)),
                            vec![2; 33],
                        ),
                        2 => (Chain::Evm, format!("0x{}", "Ab".repeat(20)), vec![3; 33]),
                        _ => (Chain::Solana, "S".repeat(44), vec![4; 32]),
                    };
                    Address {
                        id,
                        chain,
                        kind: AddressKind::ChainKey,
                        public_key: key.into(),
                        text,
                        label: format!("address {id}"),
                        cosign: None,
                        sol_nonce: None,
                        created_at_ms: 1_790_000_000_000,
                    }
                })
                .collect(),
            cosigners: vec![],
            next_address: addresses,
            next_cosigner: 1,
            next_proposal: 1_000,
            frozen: false,
            code: CodeAcceptance::None,
            code_busy: false,
            key_roots: KeyRoots::default(),
            audit_head: Hash::new([5; 32]),
            day: 20_000,
            signatures_today: 3,
            refuels_today: 0,
            evm_pending: vec![EvmPending {
                address: 2,
                chain_id: 1,
                nonce: 7,
                proposal: 9,
            }],
            sol_last: vec![],
            spends: (0..10)
                .map(|i| Spend {
                    asset: AssetKey::Icp,
                    amount: 1_000_000,
                    at_ms: 1_790_000_000_000,
                    proposal: i,
                    executed_at_ms: None,
                })
                .collect(),
        }
    }

    #[test]
    fn wallet_record_round_trips_compactly() {
        for (members, addresses) in [(5, 4), (64, 256)] {
            let w = wallet(members, addresses);
            let compact = compact_bytes(&w);
            assert_eq!(compact_from_bytes::<Wallet>(&compact), w);
            // Members and addresses are most of the record; their integer
            // keys replace the protocol's repeated field names.
            for (name, compact, plain) in [
                (
                    "members",
                    compact_bytes(&w.members).len(),
                    cbor2::to_vec(&w.members).unwrap().len(),
                ),
                (
                    "addresses",
                    compact_bytes(&w.addresses).len(),
                    cbor2::to_vec(&w.addresses).unwrap().len(),
                ),
            ] {
                assert!(
                    compact * 100 <= plain * 75,
                    "{members} members: {name} {compact} !<= 75% of {plain}"
                );
            }
        }
        let mut cell = StableCell::init(VectorMemory::default(), CompactStored::new(&None));
        let w = wallet(5, 4);
        cell.set(CompactStored::some(&w));
        assert_eq!(cell.get().value(), Some(w));
    }
}
