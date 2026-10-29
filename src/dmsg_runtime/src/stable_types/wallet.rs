//! Compact representations of the team-wallet records a wallet canister and
//! the wallet hub store. Nested enums (intents, outcomes, audit kinds,
//! activation states) and policies keep their protocol encoding; the integer
//! keys replace the field names repeated in every member, address, proposal,
//! audit event, quote and wallet record.
use super::{AccountRepr, StableCodec, is_zero};
use candid::Principal;
use cbor2::Cbor;
use dmsg_types::{billing::PlanId, wallet::*, *};
use serde_bytes::ByteBuf;

stable_struct!(BindingRepr => Binding {
    1 => principal: Principal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    2 => device: Option<Hash>,
    3 => bound_at_ms: u64,
});

stable_struct!(MemberRepr => Member {
    1 => id: MemberId,
    2 => role: Role,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    3 => label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    4 => agent_ids: Vec<String>,
    5 => status: MemberStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    6 => bindings: Vec<BindingRepr> as codec,
    7 => added_at_ms: u64,
});

stable_struct!(AddressRepr => Address {
    1 => id: u32,
    2 => chain: Chain,
    3 => kind: AddressKind,
    #[serde(default, skip_serializing_if = "is_zero")]
    4 => public_key: ByteBuf,
    5 => text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    6 => label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    7 => cosign: Option<CosignPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    8 => sol_nonce: Option<Hash>,
    9 => created_at_ms: u64,
});

stable_struct!(CosignerInputRepr => CosignerInput {
    1 => member: MemberId,
    2 => kind: CosignerKind,
    3 => public_key: ByteBuf,
    4 => envelope: CosignerEnvelope,
    #[serde(default, skip_serializing_if = "is_zero")]
    5 => hardware: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    6 => label: String,
});

stable_struct!(CosignerRepr => Cosigner {
    1 => id: u32,
    2 => input: CosignerInputRepr as codec,
    #[serde(default, skip_serializing_if = "is_zero")]
    3 => counter: u32,
    4 => added_at_ms: u64,
});

stable_struct!(RequirementRepr => Requirement {
    1 => approvals: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    2 => admin: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    3 => delay_ms: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    4 => window_amount: u128,
});

stable_struct!(VoteRepr => Vote {
    1 => member: MemberId,
    2 => decision: Decision,
    3 => at_ms: u64,
});

stable_struct!(SignedTxRepr => SignedTx {
    1 => plan: u32,
    2 => raw: ByteBuf,
    3 => txid: String,
    4 => signed_at_ms: u64,
});

stable_struct!(BroadcastReportRepr => BroadcastReport {
    1 => plan: u32,
    2 => outcome: ChainOutcome,
    3 => txid: String,
    4 => member: MemberId,
    5 => at_ms: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    6 => note: String,
});

stable_struct!(ProposalRepr => Proposal {
    1 => id: u64,
    2 => op_id: OpId,
    3 => intent: Intent,
    4 => policy_version: u64,
    5 => proposer: MemberId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    6 => via_agent: Option<String>,
    7 => digest: Hash,
    8 => created_at_ms: u64,
    9 => expires_at_ms: u64,
    10 => requirement: RequirementRepr as codec,
    11 => status: ProposalStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    12 => votes: Vec<VoteRepr> as codec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    13 => ready_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "is_zero")]
    14 => plans: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    15 => signed: Vec<SignedTxRepr> as codec,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    16 => broadcasts: Vec<BroadcastReportRepr> as codec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    17 => outcome: Option<Outcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    18 => failure: Option<String>,
    19 => updated_at_ms: u64,
});

stable_struct!(BookEntryRepr => BookEntry {
    1 => destination: Destination,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    2 => label: String,
    3 => added_at_ms: u64,
});

stable_struct!(AuditEventRepr => AuditEvent {
    1 => seq: u64,
    2 => at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    3 => actor: Option<MemberId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    4 => proposal: Option<u64>,
    5 => kind: AuditKind,
    6 => prev: Hash,
    7 => hash: Hash,
});

stable_struct!(MemberInputRepr => MemberInput {
    1 => id: MemberId,
    2 => role: Role,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    3 => label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    4 => agent_ids: Vec<String>,
});

stable_struct!(WalletSpecRepr => WalletSpec {
    1 => name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    2 => members: Vec<MemberInputRepr> as codec,
    3 => policy: Policy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    4 => channel_hint: Option<String>,
});

stable_struct!(ActivationQuoteRepr => ActivationQuote {
    1 => quote_id: Hash,
    2 => account_id: AccountId,
    3 => payer: Principal,
    4 => plan: PlanId,
    5 => remaining: u32,
    6 => price_e8s: u64,
    7 => ledger_fee_e8s: u64,
    8 => cycles_to_wallet: u128,
    9 => deposit: AccountRepr as codec,
    10 => version: u32,
    11 => issued_at_ms: u64,
    12 => expires_at_ms: u64,
});

stable_struct!(WalletRecordRepr => WalletRecord {
    1 => wallet: Principal,
    2 => creator: AccountId,
    3 => name: String,
    4 => version: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    5 => locked: bool,
    6 => members: Vec<MemberId>,
    7 => created_at_ms: u64,
});

stable_struct!(WalletVersionRepr => WalletVersion {
    1 => version: u32,
    2 => module_hash: Hash,
    3 => size: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    4 => notes: String,
    5 => registered_at_ms: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    6 => retired: bool,
});

stable_struct!(PlanQuotaRepr => PlanQuota {
    1 => plan: PlanId,
    2 => wallets: u32,
});

stable_struct!(HubPricingRepr => HubPricing {
    1 => price_e8s: u64,
    2 => cycles_to_wallet: u128,
    3 => cycles_share_e8s: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    4 => quotas: Vec<PlanQuotaRepr> as codec,
    #[serde(default, skip_serializing_if = "is_zero")]
    5 => paused: bool,
});

stable_struct!(HubInitRepr => HubInit {
    1 => environment: Environment,
    2 => governance: Principal,
    3 => icp_ledger: Principal,
    4 => cmc: Principal,
    5 => commerce: Principal,
    6 => issuer_namespace: String,
    7 => user_homes: Vec<Principal>,
    8 => key_name: String,
    9 => btc_network: BtcNetwork,
    10 => pricing: HubPricingRepr as codec,
    11 => revenue: AccountRepr as codec,
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{compact_bytes, compact_from_bytes};
    use icrc_ledger_types::icrc1::account::Account;

    fn proposal() -> Proposal {
        Proposal {
            id: 42,
            op_id: Hash::new([1; 32]),
            intent: Intent::Transfer {
                from: 0,
                asset: AssetKey::Icp,
                to: Destination::Icrc(Account {
                    owner: Principal::from_slice(&[7; 10]),
                    subaccount: None,
                }),
                amount: 1_000_000_000,
                max_fee: 10_000,
                memo: None,
            },
            policy_version: 3,
            proposer: MemberId::Account(AccountId([1; 12])),
            via_agent: None,
            digest: Hash::new([2; 32]),
            created_at_ms: 1_790_000_000_000,
            expires_at_ms: 1_790_600_000_000,
            requirement: Requirement {
                approvals: 2,
                admin: false,
                delay_ms: 0,
                window_amount: 1_000_000_000,
            },
            status: ProposalStatus::Settled,
            votes: (0..2)
                .map(|i| Vote {
                    member: MemberId::Account(AccountId([i; 12])),
                    decision: Decision::Approve,
                    at_ms: 1_790_000_100_000,
                })
                .collect(),
            ready_at_ms: Some(1_790_000_100_000),
            plans: 0,
            signed: vec![],
            broadcasts: vec![],
            outcome: Some(Outcome::Ledger {
                blocks: vec![123_456_789],
            }),
            failure: None,
            updated_at_ms: 1_790_000_200_000,
        }
    }

    #[test]
    fn wallet_records_round_trip_and_shrink() {
        let p = proposal();
        let compact = compact_bytes(&p);
        assert_eq!(compact_from_bytes::<Proposal>(&compact), p);
        let plain = cbor2::to_vec(&p).unwrap();
        assert!(
            compact.len() * 100 <= plain.len() * 60,
            "proposal {} !<= 60% of {}",
            compact.len(),
            plain.len()
        );

        let member = Member {
            id: MemberId::Account(AccountId([3; 12])),
            role: Role::Approver,
            label: String::new(),
            agent_ids: vec![],
            status: MemberStatus::Active,
            bindings: vec![
                Binding {
                    principal: Principal::from_slice(&[4; 29]),
                    device: Some(Hash::new([5; 32])),
                    bound_at_ms: 1_790_000_000_000,
                },
                Binding {
                    principal: Principal::from_slice(&[6; 29]),
                    device: None,
                    bound_at_ms: 1_790_000_000_000,
                },
            ],
            added_at_ms: 1_790_000_000_000,
        };
        assert_eq!(
            compact_from_bytes::<Member>(&compact_bytes(&member)),
            member
        );
        let address = Address {
            id: 2,
            chain: Chain::Evm,
            kind: AddressKind::ChainKey,
            public_key: vec![3; 33].into(),
            text: format!("0x{}", "ab".repeat(20)),
            label: "EVM".into(),
            cosign: Some(CosignPolicy {
                cosigners: vec![1],
                threshold: 1,
            }),
            sol_nonce: None,
            created_at_ms: 1_790_000_000_000,
        };
        assert_eq!(
            compact_from_bytes::<Address>(&compact_bytes(&address)),
            address
        );
        let cosigner = Cosigner {
            id: 1,
            input: CosignerInput {
                member: member.id.clone(),
                kind: CosignerKind::P256,
                public_key: vec![2; 33].into(),
                envelope: CosignerEnvelope::Raw,
                hardware: true,
                label: "enclave".into(),
            },
            counter: 0,
            added_at_ms: 1,
        };
        assert_eq!(
            compact_from_bytes::<Cosigner>(&compact_bytes(&cosigner)),
            cosigner
        );
        let event = AuditEvent {
            seq: 9,
            at_ms: 1_790_000_000_000,
            actor: None,
            proposal: Some(42),
            kind: AuditKind::Signed {
                plan: 1,
                txid: "ab".into(),
            },
            prev: Hash::new([5; 32]),
            hash: Hash::new([6; 32]),
        };
        assert_eq!(
            compact_from_bytes::<AuditEvent>(&compact_bytes(&event)),
            event
        );
        let entry = BookEntry {
            destination: Destination::Evm(Hash20::new([9; 20])),
            label: String::new(),
            added_at_ms: 1,
        };
        assert_eq!(
            compact_from_bytes::<BookEntry>(&compact_bytes(&entry)),
            entry
        );
    }

    #[test]
    fn hub_records_round_trip_and_shrink() {
        let quote = ActivationQuote {
            quote_id: Hash::new([7; 32]),
            account_id: AccountId([3; 12]),
            payer: Principal::from_slice(&[9; 29]),
            plan: PlanId::Pro,
            remaining: 3,
            price_e8s: 250_000_000,
            ledger_fee_e8s: 10_000,
            cycles_to_wallet: 3_000_000_000_000,
            deposit: Account {
                owner: Principal::from_slice(&[1; 10]),
                subaccount: Some([5; 32]),
            },
            version: 3,
            issued_at_ms: 1_790_000_000_000,
            expires_at_ms: 1_790_000_900_000,
        };
        let compact = compact_bytes(&quote);
        assert_eq!(compact_from_bytes::<ActivationQuote>(&compact), quote);
        let plain = cbor2::to_vec(&quote).unwrap();
        assert!(
            compact.len() * 100 <= plain.len() * 60,
            "quote {} !<= 60% of {}",
            compact.len(),
            plain.len()
        );

        let spec = WalletSpec {
            name: "ICPanda DAO".into(),
            members: vec![
                MemberInput {
                    id: MemberId::Account(AccountId([4; 12])),
                    role: Role::Approver,
                    label: "ops".into(),
                    agent_ids: vec![],
                },
                MemberInput {
                    id: MemberId::Principal(Principal::from_slice(&[6; 29])),
                    role: Role::Recovery,
                    label: String::new(),
                    agent_ids: vec!["agent-1".into()],
                },
            ],
            policy: Policy {
                version: 1,
                approver_threshold: 1,
                admin_threshold: 1,
                tiers: vec![],
                address_book_cooldown_ms: 0,
                daily_limits: vec![],
                spendable_assets: vec![AssetKey::Icp],
                call_targets: vec![],
                evm_chains: vec![],
                proposal_ttl_ms: 7 * DAY,
                unfreeze_delay_ms: DAY,
                daily_signatures: 50,
                refuel: None,
                hardware_cosigners: false,
                public: false,
            },
            channel_hint: Some("channel".into()),
        };
        assert_eq!(
            compact_from_bytes::<WalletSpec>(&compact_bytes(&spec)),
            spec
        );
        let record = WalletRecord {
            wallet: Principal::from_slice(&[2; 10]),
            creator: AccountId([3; 12]),
            name: "ICPanda DAO".into(),
            version: 3,
            locked: false,
            members: (0..5u8)
                .map(|i| MemberId::Account(AccountId([i; 12])))
                .collect(),
            created_at_ms: 1_790_000_100_000,
        };
        assert_eq!(
            compact_from_bytes::<WalletRecord>(&compact_bytes(&record)),
            record
        );
        let init = HubInit {
            environment: Environment::Production,
            governance: Principal::from_slice(&[11; 10]),
            icp_ledger: Principal::from_slice(&[12; 10]),
            cmc: Principal::from_slice(&[13; 10]),
            commerce: Principal::from_slice(&[14; 10]),
            issuer_namespace: "https://dmsg.net/u/".into(),
            user_homes: (0..4)
                .map(|i| Principal::from_slice(&[20 + i; 10]))
                .collect(),
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
                        plan: PlanId::Max,
                        wallets: 10,
                    },
                ],
                paused: true,
            },
            revenue: Account {
                owner: Principal::from_slice(&[15; 10]),
                subaccount: None,
            },
        };
        let compact = compact_bytes(&init);
        assert_eq!(compact_from_bytes::<HubInit>(&compact), init);
        let plain = cbor2::to_vec(&init).unwrap();
        assert!(
            compact.len() * 100 <= plain.len() * 60,
            "hub init {} !<= 60% of {}",
            compact.len(),
            plain.len()
        );
        let version = WalletVersion {
            version: 2,
            module_hash: Hash::new([8; 32]),
            size: 950_206,
            notes: String::new(),
            registered_at_ms: 1_790_000_000_000,
            retired: true,
        };
        assert_eq!(
            compact_from_bytes::<WalletVersion>(&compact_bytes(&version)),
            version
        );
    }
}
