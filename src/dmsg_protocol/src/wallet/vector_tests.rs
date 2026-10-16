//! `tests/vectors/wallet/protocol-v1.json`: every wallet commitment with its
//! exact preimage, an audit chain, and the address and destination
//! encodings. The file is compared on every run and rewritten with
//! `DMSG_WRITE_WALLET_VECTORS=1 cargo test -p dmsg_protocol wallet::vector_tests`.
use super::*;
use crate::{canonical, sha256};
use cbor2::{value::Integer, Value};
use dmsg_types::billing::PlanId;
use ic_secp256k1::PrivateKey;
use icrc_ledger_types::icrc1::account::Account;
use serde::Serialize;
use serde_bytes::ByteBuf;
use serde_json::{json, Value as Json};

const VECTORS: &str = "../../tests/vectors/wallet/protocol-v1.json";

fn principal(n: u8) -> Principal {
    Principal::from_slice(&[n; 10])
}

/// The JSON form of one CBOR item, as in `tests/vectors/client`.
fn typed(v: &Value) -> Json {
    match v {
        Value::Integer(i) => {
            let n = i128::from(*i);
            if n < 0 {
                json!({"int": n.to_string()})
            } else {
                json!({"uint": n.to_string()})
            }
        }
        Value::Bytes(b) => json!({"bytes": hex::encode(b)}),
        Value::Text(t) => json!({"text": t}),
        Value::Bool(b) => json!({"bool": b}),
        Value::Null => json!({"null": true}),
        Value::Tag(tag, inner) => json!({"tag": tag, "value": typed(inner)}),
        Value::Array(items) => json!({"array": items.iter().map(typed).collect::<Vec<_>>()}),
        Value::Map(entries) => json!({
            "map": entries.iter().map(|(k, v)| json!([typed(k), typed(v)])).collect::<Vec<_>>(),
        }),
        other => panic!("no wallet value encodes {other:?}"),
    }
}

/// The CBOR item of a typed JSON value.
fn untyped(json: &Json) -> Value {
    let (kind, inner) = json.as_object().unwrap().iter().next().unwrap();
    match kind.as_str() {
        "uint" | "int" => Value::Integer(
            Integer::try_from(inner.as_str().unwrap().parse::<i128>().unwrap()).unwrap(),
        ),
        "bytes" => Value::Bytes(hex::decode(inner.as_str().unwrap()).unwrap()),
        "text" => Value::Text(inner.as_str().unwrap().into()),
        "bool" => Value::Bool(inner.as_bool().unwrap()),
        "null" => Value::Null,
        "tag" => Value::Tag(inner.as_u64().unwrap(), Box::new(untyped(&json["value"]))),
        "array" => Value::Array(inner.as_array().unwrap().iter().map(untyped).collect()),
        "map" => Value::Map(
            inner
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| (untyped(&pair[0]), untyped(&pair[1])))
                .collect(),
        ),
        other => panic!("unknown typed value {other}"),
    }
}

/// A commitment `SHA256(CBOR([1, domain, value]))`: the value, the exact
/// preimage and the digest the library computed for the same inputs.
fn commitment<T: Serialize>(name: &str, domain: &str, value: &T, digest: Hash) -> Json {
    let preimage = canonical(&(1u8, domain, value));
    assert_eq!(sha256(&preimage), digest, "{name}");
    let Value::Array(items) = cbor2::from_slice::<Value>(&preimage).unwrap() else {
        unreachable!()
    };
    json!({
        "name": name,
        "domain": domain,
        "value": typed(&items[2]),
        "preimage_hex": hex::encode(&preimage),
        "digest_hex": hex::encode(*digest),
    })
}

fn policy() -> Policy {
    Policy {
        version: 2,
        approver_threshold: 2,
        admin_threshold: 2,
        tiers: vec![
            Tier {
                asset: AssetKey::Icp,
                up_to: 100_000_000,
                approvals: 1,
                delay_ms: 0,
                book_only: false,
            },
            Tier {
                asset: AssetKey::Btc,
                up_to: 1_000_000,
                approvals: 2,
                delay_ms: 3_600_000,
                book_only: true,
            },
        ],
        address_book_cooldown_ms: DAY,
        daily_limits: vec![AssetLimit {
            asset: AssetKey::Evm {
                chain_id: 8453,
                token: Some(Hash20::new([0x22; 20])),
            },
            amount: 5_000_000_000,
        }],
        spendable_assets: vec![
            AssetKey::Icp,
            AssetKey::Icrc(principal(3)),
            AssetKey::Btc,
            AssetKey::Evm {
                chain_id: 1,
                token: None,
            },
            AssetKey::Sol {
                mint: Some(Hash::new([0x44; 32])),
            },
        ],
        call_targets: vec![],
        evm_chains: vec![1, 8453],
        proposal_ttl_ms: 7 * DAY,
        unfreeze_delay_ms: DAY,
        daily_signatures: 50,
        refuel: Some(RefuelRule {
            from: 0,
            min_cycles: 2_000_000_000_000,
            icp_e8s: 50_000_000,
            max_per_day: 2,
        }),
        hardware_cosigners: true,
        public: false,
    }
}

fn commitments(wallet: Principal, hub: Principal) -> (Vec<Json>, Hash) {
    let account = AccountId([7; 12]);
    let transfer = Intent::Transfer {
        from: 0,
        asset: AssetKey::Icp,
        to: Destination::Icrc(Account {
            owner: principal(5),
            subaccount: Some([7; 32]),
        }),
        amount: 250_000_000,
        max_fee: 10_000,
        memo: Some(ByteBuf::from(b"invoice-42".to_vec())),
    };
    let batch = Intent::Batch {
        from: 1,
        asset: AssetKey::Btc,
        items: vec![
            BatchItem {
                to: Destination::Bitcoin("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4".into()),
                amount: 40_000,
            },
            BatchItem {
                to: Destination::Bitcoin(
                    "bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0".into(),
                ),
                amount: 60_000,
            },
        ],
        max_fee: 5_000,
    };
    let member = Intent::AddMember(MemberInput {
        id: MemberId::Account(AccountId([8; 12])),
        role: Role::Approver,
        label: "ops".into(),
        agent_ids: vec!["ag-ops".into()],
    });
    let mut out = vec![];
    let mut first = None;
    for (name, environment, intent, version, proposer, via_agent) in [
        (
            "proposal: ICP transfer with a memo",
            Environment::Local,
            transfer,
            3,
            MemberId::Account(account),
            None,
        ),
        (
            "proposal: Bitcoin batch through an agent",
            Environment::Production,
            batch,
            3,
            MemberId::Principal(principal(6)),
            Some("ag-treasury".to_string()),
        ),
        (
            "proposal: add a member",
            Environment::Staging,
            member,
            4,
            MemberId::Account(account),
            None,
        ),
        (
            "proposal: replace the policy",
            Environment::Local,
            Intent::SetPolicy(policy()),
            4,
            MemberId::Account(account),
            None,
        ),
    ] {
        let op_id = Hash::new([out.len() as u8 + 1; 32]);
        let d = proposal_digest(
            &environment,
            wallet,
            &op_id,
            &intent,
            version,
            &proposer,
            &via_agent,
        );
        first.get_or_insert(d);
        out.push(commitment(
            name,
            PROPOSAL_DOMAIN,
            &(
                &environment,
                wallet,
                &op_id,
                &intent,
                version,
                &proposer,
                &via_agent,
            ),
            d,
        ));
    }
    let proposal = first.unwrap();
    for decision in [Decision::Approve, Decision::Reject, Decision::Veto] {
        out.push(commitment(
            &format!("decision: {decision:?}"),
            DECISION_DOMAIN,
            &(&proposal, decision),
            decision_digest(&proposal, decision),
        ));
    }
    out.push(commitment(
        "join",
        JOIN_DOMAIN,
        &(wallet, &account, principal(11)),
        join_digest(wallet, &account, principal(11)),
    ));
    let quote_id = Hash::new([0x51; 32]);
    let deposit = deposit_subaccount(hub, &quote_id);
    out.push(commitment(
        "deposit subaccount",
        DEPOSIT_DOMAIN,
        &(hub, &quote_id),
        deposit,
    ));
    let quote = ActivationQuote {
        quote_id,
        account_id: account,
        payer: principal(11),
        plan: PlanId::Free,
        remaining: 2,
        price_e8s: 1_000_000_000,
        ledger_fee_e8s: 10_000,
        cycles_to_wallet: 3_000_000_000_000,
        deposit: Account {
            owner: hub,
            subaccount: Some(deposit.into_array()),
        },
        version: 1,
        issued_at_ms: 1_760_000_000_000,
        expires_at_ms: 1_760_000_000_000 + QUOTE_TTL_MS,
    };
    let spec = WalletSpec {
        name: "ICPanda DAO".into(),
        members: vec![
            MemberInput {
                id: MemberId::Account(AccountId([8; 12])),
                role: Role::Approver,
                label: "ops".into(),
                agent_ids: vec![],
            },
            MemberInput {
                id: MemberId::Principal(principal(12)),
                role: Role::Recovery,
                label: "sns".into(),
                agent_ids: vec![],
            },
        ],
        policy: policy(),
        channel_hint: None,
    };
    out.push(commitment(
        "activation",
        ACTIVATION_DOMAIN,
        &(&quote, &spec),
        activation_digest(&quote, &spec),
    ));
    let message = Hash::new([0x33; 32]);
    out.push(commitment(
        "cosign message",
        COSIGN_DOMAIN,
        &(wallet, 4u64, 1u32, 0u32, &message),
        cosign_message(wallet, 4, 1, 0, &message),
    ));
    (out, proposal)
}

fn audit(proposal: u64) -> Json {
    let creator = MemberId::Account(AccountId([7; 12]));
    let kinds = [
        (Some(creator.clone()), None, AuditKind::Created),
        (Some(creator.clone()), Some(proposal), AuditKind::Proposed),
        (
            Some(MemberId::Principal(principal(6))),
            Some(proposal),
            AuditKind::Decided(Decision::Approve),
        ),
        (None, Some(proposal), AuditKind::Approved),
        (
            Some(creator),
            Some(proposal),
            AuditKind::Signed {
                plan: 1,
                txid: "0x5c504ed432cb51138bcf09aa5e8a410dd4a1e204ef84bfed1be16dfba1b22060".into(),
            },
        ),
        (None, Some(proposal), AuditKind::Settled),
    ];
    let mut prev = Hash::new([0; 32]);
    let mut events = vec![];
    let mut records = vec![];
    for (seq, (actor, proposal, kind)) in kinds.into_iter().enumerate() {
        let seq = seq as u64;
        let at_ms = 1_760_000_000_000 + seq * 60_000;
        let hash = audit_hash(&prev, seq, at_ms, &actor, proposal, &kind);
        let mut record = commitment(
            &format!("audit {seq}"),
            AUDIT_DOMAIN,
            &(&prev, seq, at_ms, &actor, proposal, &kind),
            hash,
        );
        record["prev_hex"] = hex::encode(*prev).into();
        records.push(record);
        events.push(AuditEvent {
            seq,
            at_ms,
            actor,
            proposal,
            kind,
            prev,
            hash,
        });
        prev = hash;
    }
    assert_eq!(verify_audit(Hash::new([0; 32]), 0, &events), Ok(prev));
    json!({"events": records, "head_hex": hex::encode(*prev)})
}

fn path_hex(chain: Chain, kind: &AddressKind, id: u32) -> Vec<String> {
    derivation_path(chain, kind, id)
        .unwrap()
        .iter()
        .map(hex::encode)
        .collect()
}

fn secp(seed: u8) -> Vec<u8> {
    PrivateKey::generate_from_seed(&[seed; 32])
        .public_key()
        .serialize_sec1(true)
}

fn addresses(wallet: Principal) -> Vec<Json> {
    let mut out = vec![];
    for id in [0, 7] {
        let kind = AddressKind::ChainKey;
        out.push(json!({
            "chain": "Icp",
            "kind": "ChainKey",
            "id": id,
            "derivation_path_hex": path_hex(Chain::Icp, &kind, id),
            "subaccount_hex": icp_subaccount(id).map(hex::encode),
            "text": address_text(wallet, BtcNetwork::Mainnet, id, Chain::Icp, &kind, &[], None).unwrap(),
        }));
    }
    let cosigner = x_only(&secp(4)).unwrap();
    let control = TaprootControl {
        cosigner: 1,
        chain_key_after: 144,
        recovery: Some(TaprootRecovery {
            key: Hash::new(x_only(&secp(6)).unwrap()),
            after: 1_000,
        }),
    };
    for network in [BtcNetwork::Mainnet, BtcNetwork::Regtest] {
        let net = format!("{network:?}");
        let key = secp(1);
        let internal = x_only(&key).unwrap();
        let (output, _) = taproot_output_key(&internal, None).unwrap();
        let text = address_text(
            wallet,
            network,
            1,
            Chain::Bitcoin,
            &AddressKind::ChainKey,
            &key,
            None,
        )
        .unwrap();
        out.push(json!({
            "chain": "Bitcoin",
            "kind": "ChainKey",
            "network": net,
            "id": 1,
            "derivation_path_hex": path_hex(Chain::Bitcoin, &AddressKind::ChainKey, 1),
            "public_key_hex": hex::encode(&key),
            "internal_key_hex": hex::encode(internal),
            "output_key_hex": hex::encode(output),
            "script_hex": hex::encode(btc_script(network, &text).unwrap()),
            "text": text,
        }));
        let key = secp(2);
        let text = address_text(
            wallet,
            network,
            2,
            Chain::Bitcoin,
            &AddressKind::Segwit,
            &key,
            None,
        )
        .unwrap();
        out.push(json!({
            "chain": "Bitcoin",
            "kind": "Segwit",
            "network": net,
            "id": 2,
            "derivation_path_hex": path_hex(Chain::Bitcoin, &AddressKind::Segwit, 2),
            "public_key_hex": hex::encode(&key),
            "hash160_hex": hex::encode(hash160(&key)),
            "script_hex": hex::encode(btc_script(network, &text).unwrap()),
            "text": text,
        }));
        let key = secp(3);
        let kind = AddressKind::Taproot(control.clone());
        let tree = taproot_tree(&x_only(&key).unwrap(), &cosigner, &control);
        let (output, odd) = taproot_output_key(&NUMS_KEY, Some(&tree.root)).unwrap();
        let text = address_text(
            wallet,
            network,
            3,
            Chain::Bitcoin,
            &kind,
            &key,
            Some(&cosigner),
        )
        .unwrap();
        out.push(json!({
            "chain": "Bitcoin",
            "kind": "Taproot",
            "network": net,
            "id": 3,
            "derivation_path_hex": path_hex(Chain::Bitcoin, &kind, 3),
            "public_key_hex": hex::encode(&key),
            "cosigner_x_only_hex": hex::encode(cosigner),
            "chain_key_after": control.chain_key_after,
            "recovery": {
                "key_hex": hex::encode(x_only(&secp(6)).unwrap()),
                "after": 1_000,
            },
            "internal_key_hex": hex::encode(NUMS_KEY),
            "leaves_hex": tree.leaves.iter().map(hex::encode).collect::<Vec<_>>(),
            "leaf_hashes_hex": tree.leaves.iter().map(|l| hex::encode(tapleaf_hash(l))).collect::<Vec<_>>(),
            "merkle_root_hex": hex::encode(tree.root),
            "output_key_hex": hex::encode(output),
            "output_odd": odd,
            "control_blocks_hex": (0..tree.leaves.len())
                .map(|i| hex::encode(taproot_control_block(&tree, i, odd)))
                .collect::<Vec<_>>(),
            "script_hex": hex::encode(btc_script(network, &text).unwrap()),
            "text": text,
        }));
    }
    let key = secp(8);
    out.push(json!({
        "chain": "Evm",
        "kind": "ChainKey",
        "id": 4,
        "derivation_path_hex": path_hex(Chain::Evm, &AddressKind::ChainKey, 4),
        "public_key_hex": hex::encode(&key),
        "address_hex": hex::encode(evm_address(&key).unwrap()),
        "text": address_text(wallet, BtcNetwork::Mainnet, 4, Chain::Evm, &AddressKind::ChainKey, &key, None).unwrap(),
    }));
    let key = ed25519_dalek::SigningKey::from_bytes(&[9; 32])
        .verifying_key()
        .to_bytes();
    out.push(json!({
        "chain": "Solana",
        "kind": "ChainKey",
        "id": 5,
        "derivation_path_hex": path_hex(Chain::Solana, &AddressKind::ChainKey, 5),
        "public_key_hex": hex::encode(key),
        "text": address_text(wallet, BtcNetwork::Mainnet, 5, Chain::Solana, &AddressKind::ChainKey, &key, None).unwrap(),
    }));
    out
}

/// Recipients as members type them, with the bytes the wallet pays to.
fn destinations() -> Vec<Json> {
    let mut out = vec![];
    for (network, text) in [
        (BtcNetwork::Mainnet, "1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2"),
        (BtcNetwork::Mainnet, "3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy"),
        (
            BtcNetwork::Mainnet,
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4",
        ),
        (
            BtcNetwork::Mainnet,
            "bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0",
        ),
        (
            BtcNetwork::Testnet,
            "tb1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3q0sl5k7",
        ),
    ] {
        out.push(json!({
            "chain": "Bitcoin",
            "network": format!("{network:?}"),
            "text": text,
            "script_hex": hex::encode(btc_script(network, text).unwrap()),
        }));
    }
    let evm = "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed";
    out.push(json!({
        "chain": "Evm",
        "text": evm,
        "address_hex": hex::encode(parse_evm_address(evm).unwrap()),
    }));
    let sol = "Vote111111111111111111111111111111111111111";
    out.push(json!({
        "chain": "Solana",
        "text": sol,
        "address_hex": hex::encode(parse_sol_address(sol).unwrap()),
    }));
    out
}

fn build() -> Json {
    let wallet = principal(9);
    let hub = principal(1);
    let (commitments, _) = commitments(wallet, hub);
    json!({
        "schema": 1,
        "test_only": true,
        "generator": "src/dmsg_protocol/src/wallet/vector_tests.rs",
        "description": "dmsg-wallet/1 commitments, audit chain and address encodings. Each commitment is SHA256 of the canonical CBOR array [1, domain, value]; `value` is the typed JSON form of that CBOR item (see tests/vectors/client/README.md), `preimage_hex` the exact array bytes. Readers rebuild the preimage from `value` and the domain, and compare both hexes. Addresses derive from synthetic secp256k1 and Ed25519 keys; Taproot leaves, control blocks and output scripts are given byte for byte.",
        "wallet": wallet.to_text(),
        "hub": hub.to_text(),
        "nums_key_hex": hex::encode(NUMS_KEY),
        "commitments": commitments,
        "audit": audit(1),
        "addresses": addresses(wallet),
        "destinations": destinations(),
    })
}

#[test]
fn protocol_vectors_match() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(VECTORS);
    let mut text = serde_json::to_string_pretty(&build()).unwrap();
    text.push('\n');
    if std::env::var_os("DMSG_WRITE_WALLET_VECTORS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        text,
        "rerun with DMSG_WRITE_WALLET_VECTORS=1"
    );
}

#[test]
fn typed_values_rebuild_the_preimages() {
    let vectors = build();
    let records = vectors["commitments"]
        .as_array()
        .unwrap()
        .iter()
        .chain(vectors["audit"]["events"].as_array().unwrap());
    for record in records {
        let preimage = cbor2::to_canonical_vec(&Value::Array(vec![
            Value::Integer(1u8.into()),
            Value::Text(record["domain"].as_str().unwrap().into()),
            untyped(&record["value"]),
        ]))
        .unwrap();
        assert_eq!(
            hex::encode(&preimage),
            record["preimage_hex"],
            "{}",
            record["name"]
        );
        assert_eq!(
            hex::encode(*sha256(&preimage)),
            record["digest_hex"],
            "{}",
            record["name"]
        );
    }
}
