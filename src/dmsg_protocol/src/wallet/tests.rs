use super::*;
use ic_secp256k1::{PrivateKey, PublicKey};
use serde_bytes::ByteBuf;

fn principal(n: u8) -> Principal {
    Principal::from_slice(&[n; 10])
}

fn x_only_of(key: &PublicKey) -> [u8; 32] {
    x_only(&key.serialize_sec1(true)).unwrap()
}

#[test]
fn taproot_tweak_matches_the_chain_key_signer() {
    let secret = PrivateKey::generate_from_seed(&[7; 32]);
    let internal = x_only_of(&secret.public_key());
    let message = [9u8; 32];
    for root in [None, Some([3u8; 32])] {
        let (q, _) = taproot_output_key(&internal, root.as_ref()).unwrap();
        let tree: &[u8] = root.as_ref().map_or(&[], |r| &r[..]);
        let sig = secret
            .sign_message_with_bip341_no_rng(&message, tree)
            .unwrap();
        let key = k256::schnorr::VerifyingKey::from_bytes((&q).into()).unwrap();
        let sig = k256::schnorr::Signature::try_from(&sig[..]).unwrap();
        key.verify_raw(&message, &sig).unwrap();
        assert!(
            secret
                .public_key()
                .verify_bip341_signature(&message, &sig.to_bytes(), tree)
        );
    }
}

#[test]
fn derived_addresses_are_stable() {
    let master = PublicKey::deserialize_sec1(
        &PrivateKey::generate_from_seed(&[1; 32])
            .public_key()
            .serialize_sec1(true),
    )
    .unwrap();
    let path = derivation_path(Chain::Evm, &AddressKind::ChainKey, 0).unwrap();
    assert_eq!(path[0], b"dmsg-wallet/v1");
    assert_eq!(path[1], b"eth");
    assert_eq!(path[2], 0u32.to_be_bytes());
    let derivation = ic_secp256k1::DerivationPath::new(
        path.into_iter()
            .map(ic_secp256k1::DerivationIndex)
            .collect(),
    );
    let (child, _) = master.derive_subkey_with_chain_code(&derivation, &[0; 32]);
    let sec1 = child.serialize_sec1(true);
    let text = address_text(
        principal(1),
        BtcNetwork::Mainnet,
        0,
        Chain::Evm,
        &AddressKind::ChainKey,
        &sec1,
        None,
    )
    .unwrap();
    assert_eq!(
        parse_evm_address(&text).unwrap(),
        evm_address(&sec1).unwrap()
    );
    assert_eq!(text, eip55(&evm_address(&sec1).unwrap()));
    assert!(parse_evm_address(&text.to_lowercase()).is_ok());
    let mut broken = text.clone();
    // Flip one letter's case; the result stays mixed-case.
    let i = broken[2..].find(|c: char| c.is_ascii_lowercase()).unwrap() + 2;
    broken.replace_range(i..=i, &broken[i..=i].to_uppercase());
    assert!(parse_evm_address(&broken).is_err());
}

#[test]
fn eip55_reference_vector() {
    // EIP-55 test vector.
    let text = "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed";
    let parsed = parse_evm_address(text).unwrap();
    assert_eq!(eip55(&parsed), text);
}

#[test]
fn bech32_reference_vectors() {
    // BIP-350: the generator's x coordinate as a v1 program.
    let g = [
        0x79, 0xbe, 0x66, 0x7e, 0xf9, 0xdc, 0xbb, 0xac, 0x55, 0xa0, 0x62, 0x95, 0xce, 0x87, 0x0b,
        0x07, 0x02, 0x9b, 0xfc, 0xdb, 0x2d, 0xce, 0x28, 0xd9, 0x59, 0xf2, 0x81, 0x5b, 0x16, 0xf8,
        0x17, 0x98,
    ];
    let v1 = "bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0";
    assert_eq!(segwit_address(BtcNetwork::Mainnet, 1, &g), v1);
    assert_eq!(
        btc_script(BtcNetwork::Mainnet, v1).unwrap(),
        [&[0x51, 0x20][..], &g].concat()
    );
    // BIP-173: the hash160 of the generator's compressed key.
    let v0 = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
    let compressed = [&[0x02][..], &g].concat();
    assert_eq!(
        segwit_address(BtcNetwork::Mainnet, 0, &hash160(&compressed)),
        v0
    );
    assert_eq!(
        btc_script(BtcNetwork::Mainnet, &v0.to_uppercase()).unwrap(),
        [&[0x00, 0x14][..], &hash160(&compressed)].concat()
    );
    // Wrong network, mixed case and a broken checksum are refused.
    assert!(btc_script(BtcNetwork::Testnet, v1).is_err());
    assert!(
        btc_script(
            BtcNetwork::Mainnet,
            "bc1P0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0"
        )
        .is_err()
    );
    assert!(
        btc_script(
            BtcNetwork::Mainnet,
            "bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj1"
        )
        .is_err()
    );
    // A v1 program checksummed as bech32 instead of bech32m is refused.
    let mut values = vec![1u8];
    values.extend(convert(&g));
    assert!(btc_script(BtcNetwork::Mainnet, &bech32_v0_checksum("bc", &values)).is_err());
    // Base58check: the P2PKH address of hash160(G).
    let legacy = "1BgGZ9tcN4rm9KBzDn7KprQz87SZ26SAMH";
    assert_eq!(
        btc_script(BtcNetwork::Mainnet, legacy).unwrap(),
        [
            &[0x76, 0xa9, 0x14][..],
            &hash160(&compressed),
            &[0x88, 0xac]
        ]
        .concat()
    );
    for script in [
        btc_script(BtcNetwork::Mainnet, legacy).unwrap(),
        btc_script(BtcNetwork::Mainnet, v0).unwrap(),
        btc_script(BtcNetwork::Mainnet, v1).unwrap(),
    ] {
        let text = btc_script_address(BtcNetwork::Mainnet, &script).unwrap();
        assert_eq!(btc_script(BtcNetwork::Mainnet, &text).unwrap(), script);
    }
    assert_eq!(segwit_address(BtcNetwork::Regtest, 1, &g)[..6], *"bcrt1p");
}

fn convert(bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in bytes {
        acc = (acc << 8) | u32::from(*b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(((acc >> bits) & 31) as u8);
        }
    }
    if bits > 0 {
        out.push(((acc << (5 - bits)) & 31) as u8);
    }
    out
}

// A bech32 (not bech32m) string over `data`, built independently.
fn bech32_v0_checksum(hrp: &str, data: &[u8]) -> String {
    const CHARSET: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    let generator = [
        0x3b6a57b2u32,
        0x26508e6d,
        0x1ea119fa,
        0x3d4233dd,
        0x2a1462b3,
    ];
    let mut values: Vec<u8> = hrp.bytes().map(|c| c >> 5).collect();
    values.push(0);
    values.extend(hrp.bytes().map(|c| c & 31));
    values.extend_from_slice(data);
    values.extend_from_slice(&[0; 6]);
    let mut chk = 1u32;
    for v in values {
        let top = chk >> 25;
        chk = ((chk & 0x1ffffff) << 5) ^ u32::from(v);
        for (i, g) in generator.iter().enumerate() {
            if (top >> i) & 1 == 1 {
                chk ^= g;
            }
        }
    }
    let pm = chk ^ 1;
    let mut s = format!("{hrp}1");
    for d in data
        .iter()
        .copied()
        .chain((0..6).map(|i| ((pm >> (5 * (5 - i))) & 31) as u8))
    {
        s.push(CHARSET[d as usize] as char);
    }
    s
}

#[test]
fn native_taproot_leaves_and_control_blocks() {
    let ck = PrivateKey::generate_from_seed(&[2; 32]);
    let cs = PrivateKey::generate_from_seed(&[3; 32]);
    let control = TaprootControl {
        cosigner: 1,
        chain_key_after: 144,
        recovery: Some(TaprootRecovery {
            key: Hash::new(x_only_of(
                &PrivateKey::generate_from_seed(&[4; 32]).public_key(),
            )),
            after: 4_320,
        }),
    };
    let tree = taproot_tree(
        &x_only_of(&ck.public_key()),
        &x_only_of(&cs.public_key()),
        &control,
    );
    assert_eq!(tree.leaves.len(), 3);
    assert_eq!(tree.leaves[0].len(), 68);
    // 144 needs a two-byte push: 0x90 has its sign bit set.
    assert_eq!(&tree.leaves[1][..5], &[0x02, 0x90, 0x00, 0xb2, 0x75]);
    let (q, odd) = taproot_output_key(&NUMS_KEY, Some(&tree.root)).unwrap();
    for index in 0..3 {
        // Walk each control block's path back to the root.
        let block = taproot_control_block(&tree, index, odd);
        assert_eq!(block[0], 0xc0 | u8::from(odd));
        assert_eq!(&block[1..33], &NUMS_KEY);
        let mut node = tapleaf_hash(&tree.leaves[index]);
        for sibling in block[33..].chunks(32) {
            let (l, r) = if node[..] <= *sibling {
                (&node[..], sibling)
            } else {
                (sibling, &node[..])
            };
            node = tagged_hash("TapBranch", &[l, r].concat());
        }
        assert_eq!(node, tree.root);
    }
    let two = taproot_tree(
        &x_only_of(&ck.public_key()),
        &x_only_of(&cs.public_key()),
        &TaprootControl {
            recovery: None,
            ..control
        },
    );
    assert_ne!(two.root, tree.root);
    assert_eq!(script_number(16), vec![0x60]);
    assert_eq!(script_number(127), vec![0x01, 0x7f]);
    assert_eq!(script_number(128), vec![0x02, 0x80, 0x00]);
    assert_eq!(script_number(4320), vec![0x02, 0xe0, 0x10]);
    let text = segwit_address(BtcNetwork::Regtest, 1, &q);
    assert!(text.starts_with("bcrt1p"));
}

#[test]
fn icp_subaccounts() {
    assert_eq!(icp_subaccount(0), None);
    let sub = icp_subaccount(258).unwrap();
    assert_eq!(&sub[28..], &[0, 0, 1, 2]);
    assert_eq!(
        icp_account(principal(1), 0).to_string(),
        principal(1).to_text()
    );
}

fn cosigner(kind: CosignerKind, key: Vec<u8>, envelope: CosignerEnvelope) -> Cosigner {
    let input = CosignerInput {
        member: MemberId::Principal(principal(9)),
        kind,
        public_key: key.into(),
        envelope,
        hardware: true,
        label: "key".into(),
    };
    validate_cosigner(&input).unwrap();
    Cosigner {
        id: 1,
        input,
        counter: 0,
        added_at_ms: 0,
    }
}

fn sig(signature: Vec<u8>, data: Option<Vec<u8>>, client: Option<Vec<u8>>) -> CosignerSig {
    CosignerSig {
        cosigner: 1,
        digest_index: 0,
        signature: signature.into(),
        authenticator_data: data.map(ByteBuf::from),
        client_data_json: client.map(ByteBuf::from),
    }
}

#[test]
fn raw_cosigners_of_every_curve() {
    let message = cosign_message(principal(1), 7, 1, 0, &Hash::new([5; 32]));
    let other = cosign_message(principal(1), 7, 2, 0, &Hash::new([5; 32]));
    assert_ne!(message, other);

    use p256::ecdsa::signature::Signer;
    let p = p256::ecdsa::SigningKey::from_slice(&[7; 32]).unwrap();
    let c = cosigner(
        CosignerKind::P256,
        p.verifying_key().to_sec1_point(true).as_bytes().to_vec(),
        CosignerEnvelope::Raw,
    );
    let s: p256::ecdsa::Signature = p.sign(&message[..]);
    assert_eq!(
        verify_cosign(&c, &message, &sig(s.to_bytes().to_vec(), None, None)),
        Ok(0)
    );
    assert_eq!(
        verify_cosign(&c, &other, &sig(s.to_bytes().to_vec(), None, None)),
        Err(Error::IntegrityFailed)
    );

    let k = k256::ecdsa::SigningKey::from_slice(&[8; 32]).unwrap();
    let c = cosigner(
        CosignerKind::Secp256k1,
        k.verifying_key().to_sec1_point(true).as_bytes().to_vec(),
        CosignerEnvelope::Raw,
    );
    let s: k256::ecdsa::Signature = k.sign(&message[..]);
    assert!(verify_cosign(&c, &message, &sig(s.to_bytes().to_vec(), None, None)).is_ok());

    let b = PrivateKey::generate_from_seed(&[9; 32]);
    let c = cosigner(
        CosignerKind::Bip340,
        x_only_of(&b.public_key()).to_vec(),
        CosignerEnvelope::Raw,
    );
    let s = b.sign_message_with_bip340_no_rng(&message[..]);
    assert!(verify_cosign(&c, &message, &sig(s.to_vec(), None, None)).is_ok());

    let e = ed25519_dalek::SigningKey::from_bytes(&[10; 32]);
    let c = cosigner(
        CosignerKind::Ed25519,
        e.verifying_key().to_bytes().to_vec(),
        CosignerEnvelope::Raw,
    );
    let s = ed25519_dalek::Signer::sign(&e, &message[..]);
    assert!(verify_cosign(&c, &message, &sig(s.to_bytes().to_vec(), None, None)).is_ok());
}

fn auth_data(rp: &str, flags: u8, counter: u32) -> Vec<u8> {
    let mut d = sha2::Sha256::digest(rp.as_bytes()).to_vec();
    d.push(flags);
    d.extend_from_slice(&counter.to_be_bytes());
    d
}

use sha2::Digest;

#[test]
fn fido2_and_webauthn_envelopes() {
    use p256::ecdsa::signature::Signer;
    let message = cosign_message(principal(1), 1, 1, 0, &Hash::new([1; 32]));
    let p = p256::ecdsa::SigningKey::from_slice(&[11; 32]).unwrap();
    let key = p.verifying_key().to_sec1_point(true).as_bytes().to_vec();
    let rp_id_hash = Hash::new(sha2::Sha256::digest(b"wallet.dmsg.net").into());

    let mut c = cosigner(
        CosignerKind::P256,
        key.clone(),
        CosignerEnvelope::Fido2 {
            rp_id_hash,
            user_verification: true,
        },
    );
    let data = auth_data("wallet.dmsg.net", 0x05, 3);
    let s: p256::ecdsa::Signature = p.sign(&[&data[..], &message[..]].concat());
    assert_eq!(
        verify_cosign(
            &c,
            &message,
            &sig(s.to_bytes().to_vec(), Some(data.clone()), None)
        ),
        Ok(3)
    );
    // A replayed or lower counter is refused once one was seen.
    c.counter = 3;
    assert_eq!(
        verify_cosign(&c, &message, &sig(s.to_bytes().to_vec(), Some(data), None)),
        Err(Error::IntegrityFailed)
    );
    // Without user verification the policy refuses the assertion.
    let data = auth_data("wallet.dmsg.net", 0x01, 4);
    let s: p256::ecdsa::Signature = p.sign(&[&data[..], &message[..]].concat());
    assert_eq!(
        verify_cosign(&c, &message, &sig(s.to_bytes().to_vec(), Some(data), None)),
        Err(Error::Forbidden)
    );
    // Another relying party is refused.
    let data = auth_data("evil.example", 0x05, 5);
    let s: p256::ecdsa::Signature = p.sign(&[&data[..], &message[..]].concat());
    assert_eq!(
        verify_cosign(&c, &message, &sig(s.to_bytes().to_vec(), Some(data), None)),
        Err(Error::Forbidden)
    );

    let c = cosigner(
        CosignerKind::P256,
        key,
        CosignerEnvelope::WebAuthn {
            rp_id_hash,
            origins: vec!["https://connect.dmsg.net".into()],
            user_verification: false,
        },
    );
    let challenge = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        &message[..],
    );
    let client = |origin: &str, challenge: &str| {
        format!(r#"{{"type":"webauthn.get","challenge":"{challenge}","origin":"{origin}","crossOrigin":false}}"#)
            .into_bytes()
    };
    let assert_with = |client: Vec<u8>| {
        let data = auth_data("wallet.dmsg.net", 0x01, 0);
        let signed = [&data[..], &sha2::Sha256::digest(&client)[..]].concat();
        let s: p256::ecdsa::Signature = p.sign(&signed);
        verify_cosign(
            &c,
            &message,
            &sig(s.to_bytes().to_vec(), Some(data), Some(client)),
        )
    };
    assert_eq!(
        assert_with(client("https://connect.dmsg.net", &challenge)),
        Ok(0)
    );
    assert_eq!(
        assert_with(client("https://evil.example", &challenge)),
        Err(Error::Forbidden)
    );
    assert_eq!(
        assert_with(client("https://connect.dmsg.net", "AAAA")),
        Err(Error::IntegrityFailed)
    );
}

fn policy() -> Policy {
    Policy {
        version: 1,
        approver_threshold: 2,
        admin_threshold: 2,
        tiers: vec![
            Tier {
                asset: AssetKey::Icp,
                up_to: 100,
                approvals: 1,
                delay_ms: 0,
                book_only: false,
            },
            Tier {
                asset: AssetKey::Icp,
                up_to: 1_000,
                approvals: 2,
                delay_ms: HOUR,
                book_only: true,
            },
        ],
        address_book_cooldown_ms: DAY,
        daily_limits: vec![AssetLimit {
            asset: AssetKey::Icp,
            amount: 5_000,
        }],
        spendable_assets: vec![AssetKey::Icp, AssetKey::Btc],
        call_targets: vec![],
        evm_chains: vec![1],
        proposal_ttl_ms: 7 * DAY,
        unfreeze_delay_ms: DAY,
        daily_signatures: 50,
        refuel: None,
        hardware_cosigners: false,
        public: false,
    }
}

const HOUR: u64 = 60 * MINUTE;

fn transfer(asset: AssetKey, amount: u128) -> Intent {
    Intent::Transfer {
        from: 0,
        to: match asset {
            AssetKey::Btc => Destination::Bitcoin("bc1q".into()),
            _ => Destination::Icrc(principal(5).into()),
        },
        asset,
        amount,
        max_fee: 10_000,
        memo: None,
    }
}

#[test]
fn requirements_follow_tiers_windows_and_roles() {
    let p = policy();
    validate_policy(&p, 3, 4).unwrap();
    let voters = Voters {
        admins: 2,
        approvers: 4,
    };
    let r =
        |intent: &Intent, role, window, book| requirement(&p, intent, role, voters, window, book);
    let small = transfer(AssetKey::Icp, 50);
    assert_eq!(r(&small, Role::Proposer, 50, false).unwrap().approvals, 1);
    // The rolling window, not the single amount, selects the band.
    assert_eq!(r(&small, Role::Proposer, 500, true).unwrap().approvals, 2);
    assert_eq!(r(&small, Role::Proposer, 500, true).unwrap().delay_ms, HOUR);
    assert_eq!(r(&small, Role::Proposer, 500, false), Err(Error::Forbidden));
    // Above the top band: one more approval, capped by the approvers.
    assert_eq!(r(&small, Role::Proposer, 2_000, true).unwrap().approvals, 3);
    // Above the daily limit: every approver.
    assert_eq!(r(&small, Role::Proposer, 6_000, true).unwrap().approvals, 4);
    // No tiers for BTC: the default threshold.
    assert_eq!(
        r(&transfer(AssetKey::Btc, 1), Role::Approver, 1, false)
            .unwrap()
            .approvals,
        2
    );
    // Not spendable.
    assert_eq!(
        r(
            &transfer(AssetKey::Icrc(principal(3)), 1),
            Role::Admin,
            1,
            false
        ),
        Err(Error::Forbidden)
    );
    // Viewers cannot propose; Recovery only proposes membership changes.
    assert_eq!(r(&small, Role::Viewer, 50, false), Err(Error::Forbidden));
    assert_eq!(r(&small, Role::Recovery, 50, false), Err(Error::Forbidden));
    let add = Intent::AddMember(MemberInput {
        id: MemberId::Principal(principal(8)),
        role: Role::Admin,
        label: "new".into(),
        agent_ids: vec![],
    });
    let recovery = r(&add, Role::Recovery, 0, false).unwrap();
    assert_eq!(
        (recovery.approvals, recovery.delay_ms),
        (0, RECOVERY_DELAY_MS)
    );
    let admin = r(&add, Role::Proposer, 0, false).unwrap();
    assert!(admin.admin && admin.approvals == 2 && admin.delay_ms == 0);
    let sensitive = r(&Intent::SetPolicy(p.clone()), Role::Admin, 0, false).unwrap();
    assert_eq!(sensitive.delay_ms, SENSITIVE_DELAY_MS);
    assert!(counts(Role::Admin, &sensitive) && !counts(Role::Approver, &sensitive));
}

#[test]
fn policy_bounds() {
    let mut p = policy();
    assert!(
        validate_policy(&p, 1, 4).is_err(),
        "admin threshold above admins"
    );
    p.tiers.swap(0, 1);
    assert!(validate_policy(&p, 3, 4).is_err(), "tiers must ascend");
    let mut p = policy();
    p.proposal_ttl_ms = MINUTE;
    assert!(validate_policy(&p, 3, 4).is_err());
    let mut single = policy();
    single.admin_threshold = 1;
    single.approver_threshold = 1;
    single.tiers.clear();
    validate_policy(&single, 1, 1).unwrap();
}

#[test]
fn intent_shapes() {
    assert!(validate_intent(&transfer(AssetKey::Icp, 0)).is_err());
    let mismatched = Intent::Transfer {
        from: 0,
        asset: AssetKey::Btc,
        to: Destination::Icrc(principal(1).into()),
        amount: 1,
        max_fee: 0,
        memo: None,
    };
    assert!(validate_intent(&mismatched).is_err());
    let evm_batch = Intent::Batch {
        from: 0,
        asset: AssetKey::Evm {
            chain_id: 1,
            token: None,
        },
        items: vec![BatchItem {
            to: Destination::Evm(Hash20::new([1; 20])),
            amount: 1,
        }],
        max_fee: 1,
    };
    assert!(validate_intent(&evm_batch).is_err());
    assert!(
        validate_intent(&Intent::AddAddress {
            chain: Chain::Icp,
            kind: AddressKind::Segwit,
            label: String::new(),
            cosign: None,
        })
        .is_err()
    );
}

#[test]
fn digests_bind_their_inputs() {
    let intent = transfer(AssetKey::Icp, 50);
    let d = |version, agent: Option<String>| {
        proposal_digest(
            &Environment::Local,
            principal(1),
            &Hash::new([1; 32]),
            &intent,
            version,
            &MemberId::Principal(principal(2)),
            &agent,
        )
    };
    assert_ne!(d(1, None), d(2, None));
    assert_ne!(d(1, None), d(1, Some("agent".into())));
    assert_ne!(
        decision_digest(&d(1, None), Decision::Approve),
        decision_digest(&d(1, None), Decision::Veto)
    );
    let account = AccountId([1; 12]);
    assert_ne!(
        join_digest(principal(1), &account, principal(2)),
        join_digest(principal(1), &account, principal(3))
    );
}

#[test]
fn audit_chain() {
    let mut prev = Hash::new([0; 32]);
    let mut events = vec![];
    for (seq, kind) in [AuditKind::Created, AuditKind::Proposed, AuditKind::Settled]
        .into_iter()
        .enumerate()
    {
        let hash = audit_hash(&prev, seq as u64, 10, &None, Some(1), &kind);
        events.push(AuditEvent {
            seq: seq as u64,
            at_ms: 10,
            actor: None,
            proposal: Some(1),
            kind,
            prev,
            hash,
        });
        prev = hash;
    }
    assert_eq!(verify_audit(Hash::new([0; 32]), 0, &events), Ok(prev));
    events[1].at_ms = 11;
    assert_eq!(
        verify_audit(Hash::new([0; 32]), 0, &events),
        Err(Error::IntegrityFailed)
    );
}

#[test]
fn approvals_must_match_the_consumer() {
    let hub = principal(1);
    let wallet = principal(2);
    let account = AccountId([3; 12]);
    let approval = ApplicationApproval {
        version: 1,
        environment: Environment::Local,
        app_id: WALLET_APP_ID.into(),
        app_config_version: 1,
        origin: "https://dmsg.net".into(),
        approving_account: account,
        service: wallet,
        beneficiary: wallet_beneficiary(hub, wallet),
        actor: principal(4),
        purpose: ApprovalPurpose::WalletProposal,
        action_digest: Hash::new([5; 32]),
        operation_id: Hash::new([6; 32]),
        nonce: Hash::new([7; 32]),
        expires_at_ms: 1_000 + MINUTE,
    };
    let expected = ExpectedApproval {
        environment: &Environment::Local,
        hub,
        service: wallet,
        purpose: ApprovalPurpose::WalletProposal,
        action_digest: Hash::new([5; 32]),
        account: &account,
        actor: principal(4),
    };
    check_wallet_approval(&approval, &expected, 1_000).unwrap();
    assert_eq!(
        check_wallet_approval(&approval, &expected, 1_000 + MINUTE),
        Err(Error::Expired)
    );
    for change in [
        |a: &mut ApplicationApproval| a.service = principal(9),
        |a: &mut ApplicationApproval| a.purpose = ApprovalPurpose::WalletJoin,
        |a: &mut ApplicationApproval| a.action_digest = Hash::new([0; 32]),
        |a: &mut ApplicationApproval| a.actor = principal(9),
        |a: &mut ApplicationApproval| {
            a.beneficiary = wallet_beneficiary(principal(9), principal(2))
        },
    ] {
        let mut a = approval.clone();
        change(&mut a);
        assert_eq!(
            check_wallet_approval(&a, &expected, 1_000),
            Err(Error::Forbidden)
        );
    }
}
