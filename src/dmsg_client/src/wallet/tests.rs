use super::decode::*;
use super::*;

#[tokio::test]
async fn an_activation_already_sent_can_be_reviewed_without_funding_it_again() {
    use crate::payer::{Call, Payment};
    use crate::testing::{TempDir, unlocked};

    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let hub = Principal::from_slice(&[1; 10]);
    client.config.wallet.hub = Some(hub);
    let quote = ActivationQuote {
        quote_id: Hash::new([2; 32]),
        account_id: client.account().unwrap().id,
        payer: Principal::from_slice(&[3; 10]),
        plan: PlanId::Plus,
        remaining: 1,
        price_e8s: 1_000_000_000,
        ledger_fee_e8s: 10_000,
        cycles_to_wallet: 1_000_000_000_000,
        deposit: Account::from(hub),
        version: 1,
        issued_at_ms: now_ms(),
        expires_at_ms: now_ms() + 60_000,
    };
    let id = hex::encode(*quote.quote_id);
    client
        .workspace
        .put_private(client.keys().unwrap(), &format!("{QUOTE}{id}"), &quote)
        .unwrap();
    for stage in [Stage::Confirmed, Stage::Unknown] {
        let payment = Payment {
            ledger: Principal::from_slice(&[4; 10]),
            payer: quote.payer,
            call: Call::Transfer(vec![].into()),
            stage,
            block: (stage == Stage::Confirmed).then_some(1),
            error: None,
        };
        client
            .workspace
            .put_private(
                client.keys().unwrap(),
                &format!("payer:{ACTIVATION}{id}"),
                &payment,
            )
            .unwrap();
        let request = client
            .wallet_create_request(
                "cli",
                &id,
                WalletSpec {
                    name: "Already paid".into(),
                    members: vec![],
                    policy: default_policy(1, 1),
                    channel_hint: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(request["type"], "wallet");
        assert_eq!(
            client
                .payment_journal(&format!("{ACTIVATION}{id}"))
                .unwrap(),
            Some(payment)
        );
    }
}

#[test]
fn bitcoin_transactions_parse_exactly() {
    // Version 2, one input, two outputs, lock time 7.
    let mut tx = 2u32.to_le_bytes().to_vec();
    tx.push(1);
    tx.extend_from_slice(&[9; 32]);
    tx.extend_from_slice(&3u32.to_le_bytes());
    tx.push(0);
    tx.extend_from_slice(&0xffff_fffdu32.to_le_bytes());
    tx.push(2);
    for (value, script) in [(1_000u64, vec![0x51, 0x20, 1]), (2_000, vec![0x00, 0x14])] {
        tx.extend_from_slice(&value.to_le_bytes());
        tx.push(script.len() as u8);
        tx.extend_from_slice(&script);
    }
    tx.extend_from_slice(&7u32.to_le_bytes());
    let parsed = parse_btc(&tx).unwrap();
    assert_eq!(parsed.version, 2);
    assert_eq!(parsed.inputs, vec![([9; 32], 3, 0xffff_fffd)]);
    assert_eq!(parsed.outputs[1], (2_000, vec![0x00, 0x14]));
    assert_eq!(parsed.locktime, 7);
    // Trailing or missing bytes are refused.
    assert!(parse_btc(&[tx.clone(), vec![0]].concat()).is_err());
    assert!(parse_btc(&tx[..tx.len() - 1]).is_err());
}

#[test]
fn views_render_every_intent_kind() {
    let transfer = Intent::Transfer {
        from: 1,
        asset: AssetKey::Btc,
        to: Destination::Bitcoin("bc1q".into()),
        amount: 5,
        max_fee: 1,
        memo: None,
    };
    let view = intent_view(&transfer);
    assert_eq!(view["kind"], "transfer");
    assert_eq!(view["asset"]["asset"], "BTC");
    assert_eq!(view["outputs"][0]["amount"], "5");
    let call = Intent::CanisterCall {
        canister: Principal::from_slice(&[1; 10]),
        method: "icrc1_transfer".into(),
        arg: candid::encode_one(42u64).unwrap().into(),
        cycles: 0,
        consent: None,
    };
    assert_eq!(intent_view(&call)["arg_text"], "(42 : nat64)");
}

#[test]
fn job_digests_bind_the_decision() {
    let wallet = Principal::from_slice(&[2; 10]);
    let job = |decision| WalletJob {
        id: "a".into(),
        kind: JobKind::Join {
            wallet,
            name: format!("{decision:?}"),
        },
        approval: None,
        result: None,
    };
    assert_ne!(
        job(Decision::Approve).digest(),
        job(Decision::Veto).digest()
    );
    assert_eq!(job(Decision::Approve).view()["action"], "join");
    assert_eq!(
        external_source(&Intent::InitSolNonce {
            address: 3,
            max_fee: 1
        }),
        Some((3, Chain::Solana))
    );
}

/// The plans in `tests/vectors/wallet/plans-v1.json`, built by the wallet
/// canister's code, pass this decoder with exactly their summaries, and the
/// tampered ones are refused.
#[test]
fn decodes_the_wallet_plan_vectors() {
    use sha2::{Digest, Sha256};
    use sha3::Keccak256;
    let path = format!(
        "{}/../../tests/vectors/wallet/plans-v1.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let vectors: Json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let bytes = |v: &Json| hex::decode(v.as_str().unwrap()).unwrap();
    let view: WalletView =
        dmsg_protocol::decode_canonical(&bytes(&vectors["view_cbor_hex"])).unwrap();
    let mut intents = vec![];
    for case in vectors["plans"].as_array().unwrap() {
        let name = &case["name"];
        let intent: Intent =
            dmsg_protocol::decode_canonical(&bytes(&case["intent_cbor_hex"])).unwrap();
        let plan: ExecutionPlan =
            dmsg_protocol::decode_canonical(&bytes(&case["plan_cbor_hex"])).unwrap();
        assert_eq!(
            plan.unsigned.to_vec(),
            bytes(&case["unsigned_hex"]),
            "{name}"
        );
        let summary =
            verify_plan(&view, &intent, &plan).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(summary.fee.to_string(), case["summary"]["fee"], "{name}");
        assert_eq!(
            summary
                .outputs
                .iter()
                .map(|o| o.to.as_str())
                .collect::<Vec<_>>(),
            case["summary"]["outputs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|o| o["to"].as_str().unwrap())
                .collect::<Vec<_>>(),
            "{name}"
        );
        // The transaction ID follows from the signed bytes alone.
        let signed = bytes(&case["signed_hex"]);
        let txid = case["txid"].as_str().unwrap();
        match plan.params {
            ExecutionParams::Bitcoin { .. } => {
                // The unsigned bytes are the transaction without witnesses.
                let mut id: [u8; 32] = Sha256::digest(Sha256::digest(&plan.unsigned)).into();
                id.reverse();
                assert_eq!(hex::encode(id), txid, "{name}");
            }
            ExecutionParams::Evm { .. } => {
                assert_eq!(
                    format!("0x{}", hex::encode(Keccak256::digest(&signed))),
                    txid,
                    "{name}"
                );
            }
            ExecutionParams::Solana { .. } => {
                assert_eq!(
                    (signed[0], &signed[65..]),
                    (1, &plan.unsigned[..]),
                    "{name}"
                );
                let from = external_source(&intent).unwrap().0;
                let key = &view
                    .addresses
                    .iter()
                    .find(|a| a.id == from)
                    .unwrap()
                    .public_key;
                ic_ed25519::PublicKey::deserialize_raw(key)
                    .unwrap()
                    .verify_signature(&plan.unsigned, &signed[1..65])
                    .unwrap();
            }
        }
        intents.push(intent);
    }
    for case in vectors["rejected"].as_array().unwrap() {
        let plan: ExecutionPlan =
            dmsg_protocol::decode_canonical(&bytes(&case["plan_cbor_hex"])).unwrap();
        let intent = &intents[case["base"].as_u64().unwrap() as usize];
        assert!(
            verify_plan(&view, intent, &plan).is_err(),
            "{}",
            case["name"]
        );
    }
}
