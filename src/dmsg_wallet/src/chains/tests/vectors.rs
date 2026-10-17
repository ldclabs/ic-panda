//! `tests/vectors/wallet/plans-v1.json`: one plan per kind of external-chain
//! intent, built by [`build`] as `prepare_execution` builds it, with
//! deterministic signatures of synthetic keys and the signed transaction.
//! The client's decoder checks every plan against its intent and refuses
//! the tampered ones (`dmsg_client` `wallet::tests`). The file is compared
//! on every run and rewritten with
//! `DMSG_WRITE_WALLET_VECTORS=1 cargo test -p dmsg_wallet chains::tests::vectors`.
use super::*;
use dmsg_protocol::canonical;
use serde_json::{json, Value as Json};
use sha3::{Digest, Keccak256};

const VECTORS: &str = "../../tests/vectors/wallet/plans-v1.json";
const AT: u64 = 1_760_000_000_000;

/// How a test key signs digest `d` of a plan.
enum Signer {
    /// BIP-341 key path with an empty script tree.
    KeyPath(PrivateKey),
    /// BIP-340 over the sighash, as script-path spends sign.
    Bip340(PrivateKey),
    Ecdsa(PrivateKey),
    Ed25519(ic_ed25519::PrivateKey),
}

impl Signer {
    fn sign(&self, message: &[u8]) -> Vec<u8> {
        match self {
            Self::KeyPath(k) => k
                .sign_message_with_bip341_no_rng(message, &[])
                .unwrap()
                .to_vec(),
            Self::Bip340(k) => k.sign_message_with_bip340_no_rng(message).to_vec(),
            Self::Ecdsa(k) => k.sign_digest_with_ecdsa(message).to_vec(),
            Self::Ed25519(k) => k.sign_message(message).to_vec(),
        }
    }
}

fn secp(seed: u8) -> PrivateKey {
    PrivateKey::generate_from_seed(&[seed; 32])
}

fn ed(seed: u8) -> ic_ed25519::PrivateKey {
    ic_ed25519::PrivateKey::generate_from_seed(&[seed; 32])
}

/// The wallet every plan spends from: three Bitcoin kinds, one EVM key and
/// two Solana keys, one of them with a durable nonce account.
fn team() -> Wallet {
    let mut w = wallet();
    w.cosigners.push(Cosigner {
        id: 1,
        input: CosignerInput {
            member: MemberId::Principal(Principal::from_slice(&[7; 10])),
            kind: CosignerKind::Bip340,
            public_key: x_only(&secp(4).public_key().serialize_sec1(true))
                .unwrap()
                .to_vec()
                .into(),
            envelope: CosignerEnvelope::Raw,
            hardware: true,
            label: "ledger".into(),
        },
        counter: 0,
        added_at_ms: 0,
    });
    let control = TaprootControl {
        cosigner: 1,
        chain_key_after: 144,
        recovery: Some(TaprootRecovery {
            key: Hash::new(x_only(&secp(6).public_key().serialize_sec1(true)).unwrap()),
            after: 1_000,
        }),
    };
    let key = |seed| secp(seed).public_key().serialize_sec1(true);
    add(&mut w, 1, Chain::Bitcoin, AddressKind::ChainKey, key(1));
    add(&mut w, 2, Chain::Bitcoin, AddressKind::Segwit, key(2));
    add(
        &mut w,
        3,
        Chain::Bitcoin,
        AddressKind::Taproot(control),
        key(3),
    );
    add(&mut w, 4, Chain::Evm, AddressKind::ChainKey, key(8));
    let sol = |seed| ed(seed).public_key().serialize_raw().to_vec();
    add(&mut w, 5, Chain::Solana, AddressKind::ChainKey, sol(9));
    add(&mut w, 6, Chain::Solana, AddressKind::ChainKey, sol(10));
    let nonce = sol::nonce_account(&ed(10).public_key().serialize_raw());
    w.addresses[5].sol_nonce = Some(Hash::new(nonce));
    w
}

fn view(w: &Wallet) -> WalletView {
    WalletView {
        version: WALLET_VERSION,
        wallet: Principal::from_slice(&[9; 10]),
        hub: w.settings.hub,
        environment: w.settings.environment.clone(),
        name: w.settings.name.clone(),
        key_name: w.settings.key_name.clone(),
        btc_network: w.settings.btc_network,
        key_roots: w.key_roots.clone(),
        members: w.members.clone(),
        policy: w.policy.clone(),
        addresses: w.addresses.clone(),
        cosigners: w.cosigners.clone(),
        frozen: w.frozen,
        code: w.code.clone(),
        next_proposal: w.next_proposal,
        audit_len: 0,
        audit_head: w.audit_head,
        channel_hint: w.settings.channel_hint.clone(),
        created_at_ms: w.created_at_ms,
    }
}

struct Case {
    name: &'static str,
    intent: Intent,
    params: ExecutionParams,
    pending_nonce: Option<u64>,
    signer: Signer,
    cosigner: Option<Signer>,
}

fn transfer(from: u32, asset: AssetKey, to: Destination, amount: u128, max_fee: u128) -> Intent {
    Intent::Transfer {
        from,
        asset,
        to,
        amount,
        max_fee,
        memo: None,
    }
}

fn bitcoin(utxos: Vec<Utxo>, fee_rate: u64, timelock: bool) -> ExecutionParams {
    ExecutionParams::Bitcoin {
        utxos,
        fee_rate,
        locktime: 0,
        timelock,
    }
}

fn evm(gas_limit: u64) -> ExecutionParams {
    ExecutionParams::Evm {
        nonce: 7,
        max_fee_per_gas: 30_000_000_000,
        max_priority_fee_per_gas: 1_000_000_000,
        gas_limit,
    }
}

fn solana(
    blockhash: u8,
    price: u64,
    limit: u32,
    decimals: Option<u8>,
    ata: bool,
) -> ExecutionParams {
    ExecutionParams::Solana {
        blockhash: Hash::new([blockhash; 32]),
        compute_unit_price: price,
        compute_unit_limit: limit,
        decimals,
        create_ata: ata,
    }
}

fn cases(w: &Wallet) -> Vec<Case> {
    let text = |id: u32| {
        w.addresses
            .iter()
            .find(|a| a.id == id)
            .unwrap()
            .text
            .clone()
    };
    let evm_asset = |token| AssetKey::Evm {
        chain_id: 8453,
        token,
    };
    vec![
        Case {
            name: "bitcoin taproot key path with change",
            intent: transfer(
                1,
                AssetKey::Btc,
                Destination::Bitcoin(text(2)),
                60_000,
                10_000,
            ),
            params: bitcoin(vec![utxo(1, 50_000, 1), utxo(2, 70_000, 1)], 2, false),
            pending_nonce: None,
            signer: Signer::KeyPath(secp(1)),
            cosigner: None,
        },
        Case {
            name: "bitcoin p2wpkh batch",
            intent: Intent::Batch {
                from: 2,
                asset: AssetKey::Btc,
                items: vec![
                    BatchItem {
                        to: Destination::Bitcoin(text(1)),
                        amount: 40_000,
                    },
                    BatchItem {
                        to: Destination::Bitcoin(text(3)),
                        amount: 25_000,
                    },
                ],
                max_fee: 5_000,
            },
            params: bitcoin(vec![utxo(3, 100_000, 2)], 3, false),
            pending_nonce: None,
            signer: Signer::Ecdsa(secp(2)),
            cosigner: None,
        },
        Case {
            name: "bitcoin taproot cosigned script path",
            intent: transfer(
                3,
                AssetKey::Btc,
                Destination::Bitcoin(text(1)),
                50_000,
                5_000,
            ),
            params: bitcoin(vec![utxo(4, 90_000, 3), utxo(5, 20_000, 3)], 2, false),
            pending_nonce: None,
            signer: Signer::Bip340(secp(3)),
            cosigner: Some(Signer::Bip340(secp(4))),
        },
        Case {
            name: "bitcoin taproot chain key after the delay",
            intent: transfer(
                3,
                AssetKey::Btc,
                Destination::Bitcoin(text(1)),
                50_000,
                5_000,
            ),
            params: bitcoin(vec![utxo(4, 90_000, 3), utxo(5, 20_000, 3)], 2, true),
            pending_nonce: None,
            signer: Signer::Bip340(secp(3)),
            cosigner: None,
        },
        Case {
            name: "evm native transfer",
            intent: transfer(
                4,
                evm_asset(None),
                Destination::Evm(Hash20::new([0x11; 20])),
                10u128.pow(18),
                10u128.pow(16),
            ),
            params: evm(21_000),
            pending_nonce: None,
            signer: Signer::Ecdsa(secp(8)),
            cosigner: None,
        },
        Case {
            name: "evm erc-20 transfer",
            intent: transfer(
                4,
                evm_asset(Some(Hash20::new([0x22; 20]))),
                Destination::Evm(Hash20::new([0x11; 20])),
                5_000_000,
                10u128.pow(16),
            ),
            params: evm(60_000),
            pending_nonce: None,
            signer: Signer::Ecdsa(secp(8)),
            cosigner: None,
        },
        Case {
            name: "evm nonce cancellation",
            intent: Intent::CancelEvmNonce {
                address: 4,
                chain_id: 8453,
                max_fee: 10u128.pow(16),
            },
            params: evm(21_000),
            pending_nonce: Some(7),
            signer: Signer::Ecdsa(secp(8)),
            cosigner: None,
        },
        Case {
            name: "solana batch of SOL",
            intent: Intent::Batch {
                from: 5,
                asset: AssetKey::Sol { mint: None },
                items: vec![
                    BatchItem {
                        to: Destination::Solana(Hash::new([0x31; 32])),
                        amount: 1_000_000,
                    },
                    BatchItem {
                        to: Destination::Solana(Hash::new([0x32; 32])),
                        amount: 2_000_000,
                    },
                ],
                max_fee: 1_000_000,
            },
            params: solana(7, 1_000, 200_000, None, false),
            pending_nonce: None,
            signer: Signer::Ed25519(ed(9)),
            cosigner: None,
        },
        Case {
            name: "solana spl transfer creating the token account",
            intent: transfer(
                5,
                AssetKey::Sol {
                    mint: Some(Hash::new([0x44; 32])),
                },
                Destination::Solana(Hash::new([0x31; 32])),
                5,
                1_000_000,
            ),
            params: solana(8, 0, 100_000, Some(6), true),
            pending_nonce: None,
            signer: Signer::Ed25519(ed(9)),
            cosigner: None,
        },
        Case {
            name: "solana nonce account creation",
            intent: Intent::InitSolNonce {
                address: 5,
                max_fee: 100_000,
            },
            params: solana(1, 0, 10_000, None, false),
            pending_nonce: None,
            signer: Signer::Ed25519(ed(9)),
            cosigner: None,
        },
        Case {
            name: "solana transfer on a durable nonce",
            intent: transfer(
                6,
                AssetKey::Sol { mint: None },
                Destination::Solana(Hash::new([0x55; 32])),
                9,
                100_000,
            ),
            params: solana(2, 0, 10_000, None, false),
            pending_nonce: None,
            signer: Signer::Ed25519(ed(10)),
            cosigner: None,
        },
    ]
}

fn summary_json(s: &DecodedSummary) -> Json {
    let output = |o: &SummaryOutput| json!({"to": o.to, "amount": o.amount.to_string()});
    json!({
        "chain": s.chain,
        "from": s.from,
        "asset": s.asset,
        "outputs": s.outputs.iter().map(output).collect::<Vec<_>>(),
        "change": s.change.as_ref().map(output),
        "fee": s.fee.to_string(),
        "nonce": s.nonce,
    })
}

fn hexes(items: &[Vec<u8>]) -> Vec<String> {
    items.iter().map(hex::encode).collect()
}

fn plan_of(index: usize, case: &Case, built: &Built) -> ExecutionPlan {
    ExecutionPlan {
        proposal: index as u64 + 1,
        seq: 1,
        params: case.params.clone(),
        unsigned: built.unsigned.clone().into(),
        digests: built.digests.clone(),
        summary: built.summary.clone(),
        created_at_ms: AT,
        expires_at_ms: AT + PLAN_TTL_MS,
    }
}

/// Plans the decoder must refuse, each one change away from a valid plan.
fn rejected(plans: &[(Intent, ExecutionPlan)]) -> Vec<Json> {
    let mut out = vec![];
    let mut reject = |name: &str, base: usize, plan: ExecutionPlan| {
        out.push(json!({
            "name": name,
            "base": base,
            "plan_cbor_hex": hex::encode(canonical(&plan)),
        }));
    };
    let (_, btc) = &plans[0];
    let mut p = btc.clone();
    p.summary.fee -= 1;
    reject("the summary hides part of the fee", 0, p);
    let mut p = btc.clone();
    p.summary.outputs[0].to = p.summary.from.clone();
    reject("the summary names another recipient", 0, p);
    let mut p = btc.clone();
    p.digests[1] = Hash::new([0; 32]);
    reject("a digest of another transaction", 0, p);
    let mut p = btc.clone();
    // The first output's value, after version, one input marker and two
    // 41-byte inputs, and the output count.
    let at = 4 + 1 + 2 * 41 + 1;
    p.unsigned[at] ^= 1;
    reject("the transaction pays a different amount", 0, p);
    let (_, eth) = &plans[4];
    let mut p = eth.clone();
    let to = p
        .unsigned
        .windows(20)
        .position(|w| w == [0x11; 20])
        .unwrap();
    p.unsigned[to] = 0x12;
    p.digests[0] = Hash::new(Keccak256::digest(&p.unsigned).into());
    reject(
        "an EVM payload to another address, consistently hashed",
        4,
        p,
    );
    let (_, sol) = &plans[7];
    let mut p = sol.clone();
    p.summary.outputs.swap(0, 1);
    reject("the summary reorders the Solana outputs", 7, p);
    out
}

fn build_vectors() -> Json {
    let w = team();
    let view = view(&w);
    let mut plans = vec![];
    let mut records = vec![];
    for (index, case) in cases(&w).iter().enumerate() {
        let built = build(&w, &case.intent, &case.params, case.pending_nonce).unwrap();
        let plan = plan_of(index, case, &built);
        // As `sign_plan` signs: Ed25519 the whole message, others the digest.
        let messages: Vec<Vec<u8>> = built
            .digests
            .iter()
            .map(|d| match built.sign {
                SignSpec::Ed25519 { .. } => built.unsigned.clone(),
                _ => d.to_vec(),
            })
            .collect();
        let sigs: Vec<Vec<u8>> = messages.iter().map(|m| case.signer.sign(m)).collect();
        let cosigs: Vec<Option<Vec<u8>>> = messages
            .iter()
            .map(|m| case.cosigner.as_ref().map(|c| c.sign(m)))
            .collect();
        let (raw, txid) = assemble(
            &built.assembly,
            &built.unsigned,
            &built.digests,
            &sigs,
            &cosigs,
        )
        .unwrap();
        records.push(json!({
            "name": case.name,
            "intent_cbor_hex": hex::encode(canonical(&case.intent)),
            "plan_cbor_hex": hex::encode(canonical(&plan)),
            "unsigned_hex": hex::encode(&built.unsigned),
            "digests_hex": built.digests.iter().map(|d| hex::encode(**d)).collect::<Vec<_>>(),
            "summary": summary_json(&built.summary),
            "signatures_hex": hexes(&sigs),
            "cosignatures_hex": case.cosigner.as_ref().map(|_| {
                hexes(&cosigs.iter().flatten().cloned().collect::<Vec<_>>())
            }),
            "signed_hex": hex::encode(&raw),
            "txid": txid,
        }));
        plans.push((case.intent.clone(), plan));
    }
    json!({
        "schema": 1,
        "test_only": true,
        "generator": "src/dmsg_wallet/src/chains/tests/vectors.rs",
        "description": "Wallet plans for Bitcoin (Taproot key path, P2WPKH, Taproot script paths), EVM (EIP-1559 native, ERC-20, nonce cancellation) and Solana (SOL batch, SPL with token-account creation, nonce-account creation, durable nonce). `view_cbor_hex` is the WalletView, `intent_cbor_hex` and `plan_cbor_hex` the Intent and ExecutionPlan, all canonical CBOR. Signatures are deterministic signatures of synthetic test keys: BIP-341 with an empty tree for the key path, BIP-340 over the sighash for script paths (the cosigner signs the same sighash), RFC 6979 ECDSA, and Ed25519 over the whole Solana message (its digest is the message's SHA-256). A decoder must accept every plan with exactly its `summary`, and refuse every item of `rejected`, which changes one thing of the plan at index `base`.",
        "view_cbor_hex": hex::encode(canonical(&view)),
        "plans": records,
        "rejected": rejected(&plans),
    })
}

#[test]
fn plan_vectors_match() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(VECTORS);
    let mut text = serde_json::to_string_pretty(&build_vectors()).unwrap();
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
