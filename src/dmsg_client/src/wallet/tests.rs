use super::decode::*;
use super::*;

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
