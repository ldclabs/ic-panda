use super::decode::*;
use super::*;

#[test]
fn bookkeeping_notes_are_local_wallet_scoped_and_bounded() {
    use crate::testing::{TempDir, unlocked};
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let wallet = Principal::from_slice(&[1; 10]);
    let other = Principal::from_slice(&[2; 10]);
    let saved = desktop::notes(&mut client, wallet, Some((7, " invoice 42 ".into()))).unwrap();
    assert_eq!(saved["7"], "invoice 42");
    assert!(
        desktop::notes(&mut client, other, None)
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty()
    );
    assert!(desktop::notes(&mut client, wallet, Some((7, "a".repeat(2001)))).is_err());
    assert_eq!(
        desktop::notes(&mut client, wallet, None).unwrap()["7"],
        "invoice 42"
    );
    assert!(
        desktop::notes(&mut client, wallet, Some((7, String::new())))
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn the_wallet_principal_is_the_one_wallet_calls_use() {
    use crate::testing::{TempDir, unlocked};
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    // Reading it makes no wallet key.
    assert_eq!(client.wallet_principal().unwrap(), None);
    assert_eq!(client.wallet_principal().unwrap(), None);
    let (actor, _) = identity(&client.hold()).await.unwrap();
    assert_eq!(client.wallet_principal().unwrap(), Some(actor));
}

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
            replaces: vec![],
            exclusive: false,
        },
        approval: None,
        result: None,
    };
    assert_ne!(
        job(Decision::Approve).digest(),
        job(Decision::Veto).digest()
    );
    assert_eq!(job(Decision::Approve).view()["action"], "join");
    // What the approval window shows binds the devices taken over or
    // removed.
    let lost = Hash::new([9; 32]);
    let takeover = WalletJob {
        id: "a".into(),
        kind: JobKind::Join {
            wallet,
            name: "team".into(),
            replaces: vec![lost],
            exclusive: false,
        },
        approval: None,
        result: None,
    };
    assert_ne!(takeover.digest(), job(Decision::Approve).digest());
    assert_eq!(takeover.view()["replaces"], json!([hex::encode(*lost)]));
    let unbind = WalletJob {
        id: "a".into(),
        kind: JobKind::Unbind {
            wallet,
            name: "team".into(),
            devices: vec![lost],
        },
        approval: None,
        result: None,
    };
    assert_eq!(unbind.view()["action"], "unbind");
    assert_eq!(unbind.view()["devices"], json!([hex::encode(*lost)]));
    // A job journaled before the takeover fields reads as a plain join.
    let old: JobKind =
        serde_json::from_value(json!({"Join": {"wallet": wallet, "name": "team"}})).unwrap();
    assert_eq!(
        old,
        JobKind::Join {
            wallet,
            name: "team".into(),
            replaces: vec![],
            exclusive: false,
        }
    );
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

/// Read one complete HTTP request while keeping its reply blocked.
async fn held_request(listener: &tokio::net::TcpListener) -> (tokio::net::TcpStream, Vec<u8>) {
    use tokio::io::AsyncReadExt;
    let (mut socket, _) =
        tokio::time::timeout(std::time::Duration::from_secs(3), listener.accept())
            .await
            .expect("independent wallet queries should start together")
            .unwrap();
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0; 8192];
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
            let length: usize = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .unwrap()
                .parse()
                .unwrap();
            if bytes.len() >= end + 4 + length {
                return (socket, bytes);
            }
        }
    }
}

#[tokio::test]
async fn wallet_state_and_stats_start_together_and_reject_invalid_evidence() {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    crate::testing::ic_to(
        &mut client,
        &format!("http://{}", listener.local_addr().unwrap()),
    );
    let work =
        tokio::spawn(async move { show(&client.hold(), Principal::from_slice(&[7; 10])).await });
    let mut sockets = Vec::new();
    let mut requests = String::new();
    // Each query also starts a subnet-proof read.
    for _ in 0..4 {
        let (socket, bytes) = held_request(&listener).await;
        requests.push_str(&String::from_utf8_lossy(&bytes));
        sockets.push(socket);
    }
    assert!(requests.contains("get_wallet_certified") && requests.contains("wallet_stats"));
    for mut socket in sockets {
        let _ = socket
            .write_all(
                b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await;
    }
    assert!(work.await.unwrap().is_err());
}

#[tokio::test]
async fn evm_fee_reads_run_together_and_preserve_the_estimate() {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let settings = crate::config::WalletSettings {
        evm_rpc: vec![(1, format!("http://{}", listener.local_addr().unwrap()))],
        ..Default::default()
    };
    let chains = rpc::Chains::new(&settings).unwrap();
    let work = tokio::spawn(async move { chains.evm_fees(1).await });
    let mut replies = Vec::new();
    for _ in 0..2 {
        let (socket, bytes) = held_request(&listener).await;
        let end = bytes.windows(4).position(|b| b == b"\r\n\r\n").unwrap() + 4;
        let request: Json = serde_json::from_slice(&bytes[end..]).unwrap();
        let result = match request["method"].as_str().unwrap() {
            "eth_getBlockByNumber" => json!({"baseFeePerGas":"0x64"}),
            "eth_maxPriorityFeePerGas" => json!("0xa"),
            method => panic!("unexpected RPC {method}"),
        };
        replies.push((
            socket,
            json!({"jsonrpc":"2.0","id":request["id"],"result":result}).to_string(),
        ));
    }
    for (mut socket, body) in replies {
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    }
    assert_eq!(work.await.unwrap().unwrap(), (210, 10));
}

#[tokio::test]
async fn native_evm_execution_estimates_the_exact_call_instead_of_assuming_21000() {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    client.config.wallet.evm_rpc =
        vec![(8453, format!("http://{}", listener.local_addr().unwrap()))];
    let vectors: Json = serde_json::from_str(include_str!(
        "../../../../tests/vectors/wallet/plans-v1.json"
    ))
    .unwrap();
    let view: WalletView = dmsg_protocol::decode_canonical(
        &hex::decode(vectors["view_cbor_hex"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    let intent: Intent = dmsg_protocol::decode_canonical(
        &hex::decode(vectors["plans"][4]["intent_cbor_hex"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    let address = view
        .addresses
        .iter()
        .find(|a| a.chain == Chain::Evm)
        .unwrap()
        .clone();
    let expected = ops::evm_call(&intent, &address).unwrap();
    let p = Proposal {
        id: 1,
        op_id: Hash::new([1; 32]),
        intent,
        policy_version: 1,
        proposer: MemberId::Account(client.account().unwrap().id),
        via_agent: None,
        digest: Hash::new([2; 32]),
        created_at_ms: 0,
        expires_at_ms: u64::MAX,
        requirement: Requirement {
            approvals: 1,
            admin: false,
            delay_ms: 0,
            window_amount: 0,
        },
        status: ProposalStatus::Ready,
        votes: vec![],
        ready_at_ms: Some(0),
        plans: 0,
        signed: vec![],
        broadcasts: vec![],
        outcome: None,
        failure: None,
        updated_at_ms: 0,
    };
    let task =
        tokio::spawn(
            async move { ops::params(&client.hold(), &view, &p, &address, Chain::Evm).await },
        );
    let mut estimated = false;
    for _ in 0..4 {
        let (mut socket, bytes) = held_request(&listener).await;
        let at = bytes.windows(4).position(|b| b == b"\r\n\r\n").unwrap() + 4;
        let request: Json = serde_json::from_slice(&bytes[at..]).unwrap();
        let result = match request["method"].as_str().unwrap() {
            "eth_getTransactionCount" => json!("0x7"),
            "eth_getBlockByNumber" => json!({"baseFeePerGas":"0x64"}),
            "eth_maxPriorityFeePerGas" => json!("0xa"),
            "eth_estimateGas" => {
                assert_eq!(request["params"][0]["to"], expected.1);
                assert_eq!(request["params"][0]["data"], "0x");
                assert_eq!(request["params"][0]["value"], format!("0x{:x}", expected.3));
                estimated = true;
                json!("0x9c40")
            }
            method => panic!("unexpected {method}"),
        };
        let body = json!({"jsonrpc":"2.0","id":1,"result":result}).to_string();
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    }
    assert!(estimated);
    let ExecutionParams::Evm { gas_limit, .. } = task.await.unwrap().unwrap() else {
        panic!()
    };
    assert_eq!(gas_limit, 48_000);
}

#[test]
fn cancellation_plan_must_use_the_nonce_approved_in_the_intent() {
    let vectors: Json = serde_json::from_str(include_str!(
        "../../../../tests/vectors/wallet/plans-v1.json"
    ))
    .unwrap();
    let decode = |value: &Json| hex::decode(value.as_str().unwrap()).unwrap();
    let view: WalletView =
        dmsg_protocol::decode_canonical(&decode(&vectors["view_cbor_hex"])).unwrap();
    let case = vectors["plans"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "evm nonce cancellation")
        .unwrap();
    let mut intent: Intent =
        dmsg_protocol::decode_canonical(&decode(&case["intent_cbor_hex"])).unwrap();
    let plan: ExecutionPlan =
        dmsg_protocol::decode_canonical(&decode(&case["plan_cbor_hex"])).unwrap();
    verify_plan(&view, &intent, &plan).unwrap();
    let Intent::CancelEvmNonce { nonce, .. } = &mut intent else {
        panic!("cancellation vector");
    };
    *nonce += 1;
    assert_eq!(
        verify_plan(&view, &intent, &plan),
        Err(Error::IntegrityFailed)
    );
}
