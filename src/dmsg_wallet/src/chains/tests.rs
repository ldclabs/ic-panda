//! The encoders checked against independent implementations: `bitcoin` for
//! sighashes, Taproot trees and signed transactions, `alloy` for EIP-1559
//! decoding and signer recovery, and `solana-message` for message bytes.
use super::*;
use crate::store::{Settings, Wallet};
use candid::Principal;
use dmsg_protocol::wallet::*;
use ic_secp256k1::PrivateKey;

fn wallet() -> Wallet {
    let policy = Policy {
        version: 1,
        approver_threshold: 1,
        admin_threshold: 1,
        tiers: vec![],
        address_book_cooldown_ms: 0,
        daily_limits: vec![],
        spendable_assets: vec![],
        call_targets: vec![],
        evm_chains: vec![1, 8453],
        proposal_ttl_ms: DAY,
        unfreeze_delay_ms: 0,
        daily_signatures: 50,
        refuel: None,
        hardware_cosigners: false,
        public: false,
    };
    Wallet {
        schema: crate::store::STABLE_SCHEMA,
        settings: Settings {
            environment: Environment::Local,
            hub: Principal::from_slice(&[1; 10]),
            key_name: "dfx_test_key".into(),
            btc_network: BtcNetwork::Regtest,
            icp_ledger: Principal::from_slice(&[2; 10]),
            cmc: Principal::from_slice(&[3; 10]),
            issuer_namespace: "https://dmsg.test/u/".into(),
            user_homes: vec![],
            name: "test".into(),
            channel_hint: None,
        },
        created_at_ms: 0,
        members: vec![],
        policy,
        addresses: vec![],
        cosigners: vec![],
        next_address: 10,
        next_cosigner: 2,
        next_proposal: 1,
        frozen: false,
        code: CodeAcceptance::None,
        key_roots: KeyRoots::default(),
        audit_head: Hash::new([0; 32]),
        day: 0,
        signatures_today: 0,
        refuels_today: 0,
        evm_pending: vec![],
        sol_last: vec![],
        spends: vec![],
    }
}

fn add(w: &mut Wallet, id: u32, chain: Chain, kind: AddressKind, key: Vec<u8>) -> Address {
    let cosigner = match &kind {
        AddressKind::Taproot(control) => Some(btc::native_cosigner(w, control).unwrap()),
        _ => None,
    };
    let text = address_text(
        Principal::from_slice(&[9; 10]),
        w.settings.btc_network,
        id,
        chain,
        &kind,
        &key,
        cosigner.as_ref(),
    )
    .unwrap();
    let a = Address {
        id,
        chain,
        kind,
        public_key: key.into(),
        text,
        label: String::new(),
        cosign: None,
        sol_nonce: None,
        created_at_ms: 0,
    };
    w.addresses.push(a.clone());
    a
}

fn utxo(n: u8, value: u64, address: u32) -> Utxo {
    Utxo {
        txid: Hash::new([n; 32]),
        vout: u32::from(n),
        value,
        address,
    }
}

mod bitcoin_checks {
    use super::*;
    use bitcoin::{
        Amount, Network, ScriptBuf, Transaction, TxOut as BTxOut,
        consensus::deserialize,
        hashes::Hash as _,
        key::{Secp256k1, XOnlyPublicKey},
        secp256k1::{Message, schnorr},
        sighash::{EcdsaSighashType, Prevouts, SighashCache, TapSighashType},
        taproot::{LeafVersion, TapLeafHash, TaprootBuilder},
    };

    fn prevouts(script: &[u8], utxos: &[Utxo]) -> Vec<BTxOut> {
        utxos
            .iter()
            .map(|u| BTxOut {
                value: Amount::from_sat(u.value),
                script_pubkey: ScriptBuf::from_bytes(script.to_vec()),
            })
            .collect()
    }

    fn signed(raw: &[u8]) -> Transaction {
        deserialize(raw).unwrap()
    }

    #[test]
    fn taproot_key_path_matches_bitcoin() {
        let mut w = wallet();
        let sk = PrivateKey::generate_from_seed(&[1; 32]);
        let a = add(
            &mut w,
            1,
            Chain::Bitcoin,
            AddressKind::ChainKey,
            sk.public_key().serialize_sec1(true),
        );
        let secp = Secp256k1::verification_only();
        let internal = XOnlyPublicKey::from_slice(&x_only(&a.public_key).unwrap()).unwrap();
        let expected = bitcoin::Address::p2tr(&secp, internal, None, Network::Regtest);
        assert_eq!(a.text, expected.to_string());
        let dest = bitcoin::Address::p2wpkh(
            &bitcoin::CompressedPublicKey::from_slice(
                &PrivateKey::generate_from_seed(&[5; 32])
                    .public_key()
                    .serialize_sec1(true),
            )
            .unwrap(),
            Network::Regtest,
        );
        let utxos = [utxo(1, 50_000, 1), utxo(2, 70_000, 1)];
        let built = btc::build(
            &w,
            &a,
            &[(Destination::Bitcoin(dest.to_string()), 60_000)],
            10_000,
            &utxos,
            2,
            0,
            false,
        )
        .unwrap();
        let tx: Transaction = deserialize(&built.unsigned).unwrap();
        assert_eq!(tx.output[0].script_pubkey, dest.script_pubkey());
        assert_eq!(tx.output[1].script_pubkey, expected.script_pubkey());
        assert_eq!(tx.input[0].sequence.0, btc::SEQUENCE_RBF);
        let script = expected.script_pubkey().to_bytes();
        let outs = prevouts(&script, &utxos);
        let mut cache = SighashCache::new(&tx);
        for i in 0..2 {
            let h = cache
                .taproot_key_spend_signature_hash(i, &Prevouts::All(&outs), TapSighashType::Default)
                .unwrap();
            assert_eq!(built.digests[i][..], h.to_byte_array());
        }
        // Sign as the management canister does with an empty BIP-341 tree.
        let sigs: Vec<Vec<u8>> = built
            .digests
            .iter()
            .map(|d| {
                sk.sign_message_with_bip341_no_rng(&d[..], &[])
                    .unwrap()
                    .to_vec()
            })
            .collect();
        let (raw, txid) =
            assemble(&built.assembly, &built.unsigned, &built.digests, &sigs, &[]).unwrap();
        let tx = signed(&raw);
        assert_eq!(tx.compute_txid().to_string(), txid);
        let output_key = XOnlyPublicKey::from_slice(&script[2..]).unwrap();
        for (i, input) in tx.input.iter().enumerate() {
            let sig = schnorr::Signature::from_slice(&input.witness[0]).unwrap();
            let msg = Message::from_digest(built.digests[i].into_array());
            secp.verify_schnorr(&sig, &msg, &output_key).unwrap();
        }
        assert_eq!(
            tx.vsize() as u64,
            btc::vsize(&tx_of(&built.assembly), &spend_of(&built.assembly))
        );
        let fee = 120_000 - tx.output.iter().map(|o| o.value.to_sat()).sum::<u64>();
        assert_eq!(u128::from(fee), built.summary.fee);
        assert_eq!(fee, 2 * tx.vsize() as u64);
        assert_eq!(
            built.summary.change.as_ref().unwrap().amount,
            u128::from(120_000 - 60_000 - fee)
        );
        // A fee above the intent's ceiling is refused.
        assert!(
            btc::build(
                &w,
                &a,
                &[(Destination::Bitcoin(dest.to_string()), 60_000)],
                100,
                &utxos,
                2,
                0,
                false
            )
            .is_err()
        );
        // Change below dust goes to the fee.
        let built = btc::build(
            &w,
            &a,
            &[(Destination::Bitcoin(dest.to_string()), 119_500)],
            1_000,
            &utxos,
            2,
            0,
            false,
        )
        .unwrap();
        assert!(built.summary.change.is_none());
        assert_eq!(built.summary.fee, 500);
        // What is left pays the transaction without change even when it
        // could not also pay for a change output: 157 vB at 2 sat/vB is 314,
        // 200 vB with change would be 400.
        let pay = |amount| {
            btc::build(
                &w,
                &a,
                &[(Destination::Bitcoin(dest.to_string()), amount)],
                1_000,
                &utxos,
                2,
                0,
                false,
            )
        };
        let built = pay(119_650).unwrap();
        assert!(built.summary.change.is_none());
        assert_eq!(built.summary.fee, 350);
        assert_eq!(pay(119_700).err(), Some(Error::FeeBlocked));
    }

    fn tx_of(a: &Assembly) -> btc::BtcTx {
        let Assembly::Bitcoin { tx, .. } = a else {
            panic!()
        };
        tx.clone()
    }

    fn spend_of(a: &Assembly) -> btc::BtcSpend {
        let Assembly::Bitcoin { spend, .. } = a else {
            panic!()
        };
        spend.clone()
    }

    #[test]
    fn segwit_spend_matches_bip143() {
        let mut w = wallet();
        let sk = PrivateKey::generate_from_seed(&[2; 32]);
        let pk = sk.public_key().serialize_sec1(true);
        let a = add(&mut w, 2, Chain::Bitcoin, AddressKind::Segwit, pk.clone());
        let compressed = bitcoin::CompressedPublicKey::from_slice(&pk).unwrap();
        let expected = bitcoin::Address::p2wpkh(&compressed, Network::Regtest);
        assert_eq!(a.text, expected.to_string());
        let utxos = [utxo(3, 100_000, 2)];
        let built = btc::build(
            &w,
            &a,
            &[(Destination::Bitcoin(expected.to_string()), 40_000)],
            5_000,
            &utxos,
            3,
            0,
            false,
        )
        .unwrap();
        let tx: Transaction = deserialize(&built.unsigned).unwrap();
        let h = SighashCache::new(&tx)
            .p2wpkh_signature_hash(
                0,
                &expected.script_pubkey(),
                Amount::from_sat(100_000),
                EcdsaSighashType::All,
            )
            .unwrap();
        assert_eq!(built.digests[0][..], h.to_byte_array());
        let sig = sk.sign_digest_with_ecdsa(&built.digests[0][..]).to_vec();
        let (raw, txid) = assemble(
            &built.assembly,
            &built.unsigned,
            &built.digests,
            &[sig],
            &[],
        )
        .unwrap();
        let tx = signed(&raw);
        assert_eq!(tx.compute_txid().to_string(), txid);
        let der = &tx.input[0].witness[0];
        assert_eq!(*der.last().unwrap(), 0x01);
        let sig = bitcoin::secp256k1::ecdsa::Signature::from_der(&der[..der.len() - 1]).unwrap();
        Secp256k1::verification_only()
            .verify_ecdsa(
                &Message::from_digest(h.to_byte_array()),
                &sig,
                &compressed.0,
            )
            .unwrap();
        assert!(
            tx.vsize() as u64 <= btc::vsize(&tx_of(&built.assembly), &spend_of(&built.assembly))
        );
    }

    #[test]
    fn native_taproot_matches_bitcoin_tree_and_script_sighash() {
        let mut w = wallet();
        let ck = PrivateKey::generate_from_seed(&[3; 32]);
        let cs = PrivateKey::generate_from_seed(&[4; 32]);
        let rec = PrivateKey::generate_from_seed(&[6; 32]);
        w.cosigners.push(Cosigner {
            id: 1,
            input: CosignerInput {
                member: MemberId::Principal(Principal::from_slice(&[7; 10])),
                kind: CosignerKind::Bip340,
                public_key: x_only(&cs.public_key().serialize_sec1(true))
                    .unwrap()
                    .to_vec()
                    .into(),
                envelope: CosignerEnvelope::Raw,
                hardware: true,
                label: String::new(),
            },
            counter: 0,
            added_at_ms: 0,
        });
        let control = TaprootControl {
            cosigner: 1,
            chain_key_after: 144,
            recovery: Some(TaprootRecovery {
                key: Hash::new(x_only(&rec.public_key().serialize_sec1(true)).unwrap()),
                after: 1_000,
            }),
        };
        let a = add(
            &mut w,
            3,
            Chain::Bitcoin,
            AddressKind::Taproot(control.clone()),
            ck.public_key().serialize_sec1(true),
        );
        let tree = taproot_tree(
            &x_only(&a.public_key).unwrap(),
            &x_only(&cs.public_key().serialize_sec1(true)).unwrap(),
            &control,
        );
        let secp = Secp256k1::new();
        let leaves: Vec<ScriptBuf> = tree
            .leaves
            .iter()
            .map(|l| ScriptBuf::from_bytes(l.clone()))
            .collect();
        let info = TaprootBuilder::new()
            .add_leaf(1, leaves[0].clone())
            .unwrap()
            .add_leaf(2, leaves[1].clone())
            .unwrap()
            .add_leaf(2, leaves[2].clone())
            .unwrap()
            .finalize(&secp, XOnlyPublicKey::from_slice(&NUMS_KEY).unwrap())
            .unwrap();
        let expected = bitcoin::Address::p2tr_tweaked(info.output_key(), Network::Regtest);
        assert_eq!(a.text, expected.to_string());
        for (i, leaf) in leaves.iter().enumerate() {
            let block = info
                .control_block(&(leaf.clone(), LeafVersion::TapScript))
                .unwrap();
            let (_, odd) = taproot_output_key(&NUMS_KEY, Some(&tree.root)).unwrap();
            assert_eq!(block.serialize(), taproot_control_block(&tree, i, odd));
        }
        let script = expected.script_pubkey().to_bytes();
        let utxos = [utxo(4, 90_000, 3), utxo(5, 20_000, 3)];
        let dest = Destination::Bitcoin(expected.to_string());
        for timelock in [false, true] {
            let built = btc::build(
                &w,
                &a,
                &[(dest.clone(), 50_000)],
                5_000,
                &utxos,
                2,
                0,
                timelock,
            )
            .unwrap();
            let tx: Transaction = deserialize(&built.unsigned).unwrap();
            let leaf = &leaves[usize::from(timelock)];
            let outs = prevouts(&script, &utxos);
            let mut cache = SighashCache::new(&tx);
            for i in 0..2 {
                let h = cache
                    .taproot_script_spend_signature_hash(
                        i,
                        &Prevouts::All(&outs),
                        TapLeafHash::from_script(leaf, LeafVersion::TapScript),
                        TapSighashType::Default,
                    )
                    .unwrap();
                assert_eq!(built.digests[i][..], h.to_byte_array());
                assert_eq!(
                    tx.input[i].sequence.0,
                    if timelock { 144 } else { btc::SEQUENCE_RBF }
                );
            }
            let sigs: Vec<Vec<u8>> = built
                .digests
                .iter()
                .map(|d| ck.sign_message_with_bip340_no_rng(&d[..]).to_vec())
                .collect();
            let cosigs: Vec<Option<Vec<u8>>> = built
                .digests
                .iter()
                .map(|d| (!timelock).then(|| cs.sign_message_with_bip340_no_rng(&d[..]).to_vec()))
                .collect();
            let (raw, txid) = assemble(
                &built.assembly,
                &built.unsigned,
                &built.digests,
                &sigs,
                &cosigs,
            )
            .unwrap();
            let tx = signed(&raw);
            assert_eq!(tx.compute_txid().to_string(), txid);
            let witness: Vec<Vec<u8>> = tx.input[0].witness.iter().map(|x| x.to_vec()).collect();
            let block = info
                .control_block(&(leaf.clone(), LeafVersion::TapScript))
                .unwrap()
                .serialize();
            if timelock {
                assert_eq!(witness, vec![sigs[0].clone(), leaf.to_bytes(), block]);
            } else {
                assert_eq!(
                    witness,
                    vec![
                        cosigs[0].clone().unwrap(),
                        sigs[0].clone(),
                        leaf.to_bytes(),
                        block
                    ]
                );
            }
            assert!(
                tx.vsize() as u64
                    <= btc::vsize(&tx_of(&built.assembly), &spend_of(&built.assembly))
            );
        }
    }

    #[test]
    fn inputs_must_belong_to_the_source() {
        let mut w = wallet();
        let sk = PrivateKey::generate_from_seed(&[1; 32]);
        let a = add(
            &mut w,
            1,
            Chain::Bitcoin,
            AddressKind::ChainKey,
            sk.public_key().serialize_sec1(true),
        );
        let to = Destination::Bitcoin(a.text.clone());
        assert!(
            btc::build(
                &w,
                &a,
                &[(to.clone(), 1_000)],
                5_000,
                &[utxo(1, 9_000, 2)],
                1,
                0,
                false
            )
            .is_err()
        );
        assert!(
            btc::build(
                &w,
                &a,
                &[(to.clone(), 1_000)],
                5_000,
                &[utxo(1, 9_000, 1), utxo(1, 9_000, 1)],
                1,
                0,
                false
            )
            .is_err()
        );
        assert!(
            btc::build(
                &w,
                &a,
                &[(to.clone(), 100)],
                5_000,
                &[utxo(1, 9_000, 1)],
                1,
                0,
                false
            )
            .is_err(),
            "dust"
        );
        assert!(
            btc::build(
                &w,
                &a,
                &[(to, 1_000)],
                5_000,
                &[utxo(1, 9_000, 1)],
                1,
                0,
                true
            )
            .is_err(),
            "no timelock leaf"
        );
    }
}

mod evm_checks {
    use super::*;
    use alloy_consensus::{Transaction as _, TxEnvelope};
    use alloy_eips::eip2718::Decodable2718;

    #[test]
    fn eip1559_transfers_decode_and_recover() {
        let mut w = wallet();
        let sk = PrivateKey::generate_from_seed(&[8; 32]);
        let a = add(
            &mut w,
            2,
            Chain::Evm,
            AddressKind::ChainKey,
            sk.public_key().serialize_sec1(true),
        );
        let to = Hash20::new([0x11; 20]);
        let token = Hash20::new([0x22; 20]);
        for (payment, value, data_len) in [
            (
                evm::Payment::Transfer {
                    token: None,
                    to,
                    amount: 10u128.pow(18),
                },
                10u128.pow(18),
                0,
            ),
            (
                evm::Payment::Transfer {
                    token: Some(token),
                    to,
                    amount: 5_000_000,
                },
                0,
                68,
            ),
            (evm::Payment::Cancel, 0, 0),
        ] {
            let built = evm::build(
                &a,
                8453,
                payment,
                10u128.pow(16),
                7,
                30_000_000_000,
                1_000_000_000,
                60_000,
            )
            .unwrap();
            let sig = sk.sign_digest_with_ecdsa(&built.digests[0][..]).to_vec();
            let (raw, txid) = assemble(
                &built.assembly,
                &built.unsigned,
                &built.digests,
                &[sig],
                &[],
            )
            .unwrap();
            let TxEnvelope::Eip1559(signed) = TxEnvelope::decode_2718(&mut raw.as_slice()).unwrap()
            else {
                panic!("not EIP-1559")
            };
            let tx = signed.tx();
            assert_eq!(tx.chain_id, 8453);
            assert_eq!(tx.nonce, 7);
            assert_eq!(tx.gas_limit, 60_000);
            assert_eq!(tx.value(), alloy_primitives::U256::from(value));
            assert_eq!(tx.input().len(), data_len);
            assert_eq!(format!("{:#x}", signed.hash()), txid);
            let signer = signed.recover_signer().unwrap();
            assert_eq!(signer.0.0, evm_address(&a.public_key).unwrap());
            assert_eq!(built.summary.fee, 60_000 * 30_000_000_000);
        }
        assert!(
            evm::build(&a, 1, evm::Payment::Cancel, 100, 0, 30, 31, 21_000).is_err(),
            "priority above max"
        );
        assert_eq!(
            evm::build(&a, 1, evm::Payment::Cancel, 100, 0, 30, 1, 21_000).err(),
            Some(Error::FeeBlocked)
        );
    }
}

mod solana_checks {
    use super::*;
    use solana_address::Address as Pubkey;
    use solana_instruction::{AccountMeta, Instruction};

    fn pk(b: [u8; 32]) -> Pubkey {
        Pubkey::new_from_array(b)
    }

    fn budget(limit: u32, price: u64) -> Vec<Instruction> {
        let program = pk(bs58::decode("ComputeBudget111111111111111111111111111111")
            .into_vec()
            .unwrap()
            .try_into()
            .unwrap());
        vec![
            Instruction::new_with_bytes(
                program,
                &[&[2u8][..], &limit.to_le_bytes()].concat(),
                vec![],
            ),
            Instruction::new_with_bytes(
                program,
                &[&[3u8][..], &price.to_le_bytes()].concat(),
                vec![],
            ),
        ]
    }

    fn reference(payer: [u8; 32], ixs: &[Instruction], blockhash: [u8; 32]) -> Vec<u8> {
        solana_message::legacy::Message::new_with_blockhash(
            ixs,
            Some(&pk(payer)),
            &solana_hash::Hash::new_from_array(blockhash),
        )
        .serialize()
    }

    #[test]
    fn program_ids_match_their_addresses() {
        for (key, text) in [
            (
                sol::COMPUTE_BUDGET,
                "ComputeBudget111111111111111111111111111111",
            ),
            (
                sol::TOKEN_PROGRAM,
                "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
            ),
            (
                sol::ATA_PROGRAM,
                "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
            ),
            (
                sol::RECENT_BLOCKHASHES,
                "SysvarRecentB1ockHashes11111111111111111111",
            ),
            (sol::RENT, "SysvarRent111111111111111111111111111111111"),
        ] {
            assert_eq!(sol_address(&key), text);
        }
    }

    #[test]
    fn sol_and_spl_messages_match_solana() {
        let mut w = wallet();
        let sk = ic_ed25519::PrivateKey::generate_from_seed(&[9; 32]);
        let payer = sk.public_key().serialize_raw();
        let a = add(
            &mut w,
            3,
            Chain::Solana,
            AddressKind::ChainKey,
            payer.to_vec(),
        );
        assert_eq!(a.text, pk(payer).to_string());
        let (r1, r2) = ([0x31; 32], [0x32; 32]);
        let outputs = [
            (Destination::Solana(Hash::new(r1)), 1_000_000u128),
            (Destination::Solana(Hash::new(r2)), 2_000_000),
        ];
        let built = sol::build(
            &a,
            sol::Payment::Transfer {
                mint: None,
                outputs: &outputs,
            },
            1_000_000,
            Hash::new([7; 32]),
            1_000,
            200_000,
            None,
            false,
        )
        .unwrap();
        let mut ixs = budget(200_000, 1_000);
        for (to, amount) in [(r1, 1_000_000u64), (r2, 2_000_000)] {
            ixs.push(Instruction::new_with_bytes(
                pk([0; 32]),
                &[&2u32.to_le_bytes()[..], &amount.to_le_bytes()].concat(),
                vec![
                    AccountMeta::new(pk(payer), true),
                    AccountMeta::new(pk(to), false),
                ],
            ));
        }
        assert_eq!(built.unsigned, reference(payer, &ixs, [7; 32]));
        assert_eq!(built.summary.fee, 5_000 + 200);
        let sig = sk.sign_message(&built.unsigned);
        let (raw, txid) = assemble(
            &built.assembly,
            &built.unsigned,
            &built.digests,
            &[sig.to_vec()],
            &[],
        )
        .unwrap();
        assert_eq!(raw[0], 1);
        assert_eq!(&raw[65..], &built.unsigned[..]);
        assert_eq!(txid, bs58::encode(sig).into_string());
        sk.public_key()
            .verify_signature(&raw[65..], &raw[1..65])
            .unwrap();

        // An SPL transfer creating the recipient's token account.
        let mint = [0x44; 32];
        let token = pk(sol::TOKEN_PROGRAM);
        let ata_program = pk(sol::ATA_PROGRAM);
        let ata = |owner: [u8; 32]| {
            Pubkey::find_program_address(&[&owner, token.as_ref(), &mint], &ata_program).0
        };
        assert_eq!(pk(sol::ata(&r1, &mint)), ata(r1));
        let outputs = [(Destination::Solana(Hash::new(r1)), 5u128)];
        let built = sol::build(
            &a,
            sol::Payment::Transfer {
                mint: Some(Hash::new(mint)),
                outputs: &outputs,
            },
            1_000_000,
            Hash::new([8; 32]),
            0,
            100_000,
            Some(6),
            true,
        )
        .unwrap();
        let mut ixs = budget(100_000, 0);
        ixs.push(Instruction::new_with_bytes(
            ata_program,
            &[1],
            vec![
                AccountMeta::new(pk(payer), true),
                AccountMeta::new(ata(r1), false),
                AccountMeta::new_readonly(pk(r1), false),
                AccountMeta::new_readonly(pk(mint), false),
                AccountMeta::new_readonly(pk([0; 32]), false),
                AccountMeta::new_readonly(token, false),
            ],
        ));
        ixs.push(Instruction::new_with_bytes(
            token,
            &[&[12u8][..], &5u64.to_le_bytes(), &[6]].concat(),
            vec![
                AccountMeta::new(ata(payer), false),
                AccountMeta::new_readonly(pk(mint), false),
                AccountMeta::new(ata(r1), false),
                AccountMeta::new_readonly(pk(payer), true),
            ],
        ));
        assert_eq!(built.unsigned, reference(payer, &ixs, [8; 32]));
    }

    #[test]
    fn durable_nonce_messages_match_solana() {
        let mut w = wallet();
        let sk = ic_ed25519::PrivateKey::generate_from_seed(&[10; 32]);
        let payer = sk.public_key().serialize_raw();
        let mut a = add(
            &mut w,
            3,
            Chain::Solana,
            AddressKind::ChainKey,
            payer.to_vec(),
        );
        let system = pk([0; 32]);
        let nonce = Pubkey::create_with_seed(&pk(payer), sol::NONCE_SEED, &system).unwrap();
        assert_eq!(pk(sol::nonce_account(&payer)), nonce);
        let recent = pk(bs58::decode("SysvarRecentB1ockHashes11111111111111111111")
            .into_vec()
            .unwrap()
            .try_into()
            .unwrap());
        let rent = pk(bs58::decode("SysvarRent111111111111111111111111111111111")
            .into_vec()
            .unwrap()
            .try_into()
            .unwrap());
        let built = sol::build(
            &a,
            sol::Payment::InitNonce,
            100_000,
            Hash::new([1; 32]),
            0,
            10_000,
            None,
            false,
        )
        .unwrap();
        let mut ixs = budget(10_000, 0);
        let mut create = payer.to_vec();
        create.extend_from_slice(&(sol::NONCE_SEED.len() as u64).to_le_bytes());
        create.extend_from_slice(sol::NONCE_SEED.as_bytes());
        create.extend_from_slice(&sol::NONCE_RENT.to_le_bytes());
        create.extend_from_slice(&80u64.to_le_bytes());
        create.extend_from_slice(&[0; 32]);
        ixs.push(Instruction::new_with_bytes(
            system,
            &[&3u32.to_le_bytes()[..], &create].concat(),
            vec![
                AccountMeta::new(pk(payer), true),
                AccountMeta::new(nonce, false),
            ],
        ));
        ixs.push(Instruction::new_with_bytes(
            system,
            &[&6u32.to_le_bytes()[..], &payer].concat(),
            vec![
                AccountMeta::new(nonce, false),
                AccountMeta::new_readonly(recent, false),
                AccountMeta::new_readonly(rent, false),
            ],
        ));
        assert_eq!(built.unsigned, reference(payer, &ixs, [1; 32]));

        // With the nonce account, the advance instruction comes first.
        a.sol_nonce = Some(Hash::new(nonce.to_bytes()));
        let outputs = [(Destination::Solana(Hash::new([0x55; 32])), 9u128)];
        let built = sol::build(
            &a,
            sol::Payment::Transfer {
                mint: None,
                outputs: &outputs,
            },
            100_000,
            Hash::new([2; 32]),
            0,
            10_000,
            None,
            false,
        )
        .unwrap();
        let mut ixs = vec![Instruction::new_with_bytes(
            system,
            &4u32.to_le_bytes(),
            vec![
                AccountMeta::new(nonce, false),
                AccountMeta::new_readonly(recent, false),
                AccountMeta::new_readonly(pk(payer), true),
            ],
        )];
        ixs.extend(budget(10_000, 0));
        ixs.push(Instruction::new_with_bytes(
            system,
            &[&2u32.to_le_bytes()[..], &9u64.to_le_bytes()].concat(),
            vec![
                AccountMeta::new(pk(payer), true),
                AccountMeta::new(pk([0x55; 32]), false),
            ],
        ));
        assert_eq!(built.unsigned, reference(payer, &ixs, [2; 32]));
        assert!(
            sol::build(
                &a,
                sol::Payment::InitNonce,
                100_000,
                Hash::new([2; 32]),
                0,
                10_000,
                None,
                false
            )
            .is_err()
        );
    }
}

mod vectors;
