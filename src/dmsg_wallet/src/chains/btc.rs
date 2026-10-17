//! Bitcoin transactions: version 2, RBF inputs, Taproot key-path and
//! native two-key script-path spends (BIP-341/342) and P2WPKH (BIP-143).
use super::{dest_text, Built};
use crate::store::{SignSpec, Wallet};
use dmsg_protocol::wallet::*;
use dmsg_types::{wallet::*, *};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use sha2::{Digest, Sha256};

/// Opt-in replace-by-fee without a relative lock.
pub(crate) const SEQUENCE_RBF: u32 = 0xffff_fffd;
/// Outputs below this are refused and change below it goes to the fee.
pub(crate) const DUST: u64 = 546;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TxIn {
    pub(crate) txid: Hash,
    pub(crate) vout: u32,
    pub(crate) sequence: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TxOut {
    pub(crate) value: u64,
    pub(crate) script: ByteBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BtcTx {
    pub(crate) version: u32,
    pub(crate) inputs: Vec<TxIn>,
    pub(crate) outputs: Vec<TxOut>,
    pub(crate) locktime: u32,
}

/// How an input is witnessed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum BtcSpend {
    /// Taproot key path: the tweaked chain key's signature.
    KeyPath,
    /// P2WPKH: DER signature and public key.
    Segwit { public_key: ByteBuf },
    /// Native Taproot script path; `cosigner` is set for the two-key leaf.
    Script {
        leaf: ByteBuf,
        control: ByteBuf,
        cosigner: Option<u32>,
    },
}

fn output_bytes(outputs: &[TxOut]) -> Vec<u8> {
    let mut b = vec![];
    for o in outputs {
        b.extend_from_slice(&o.value.to_le_bytes());
        b.extend(compact_size(o.script.len()));
        b.extend_from_slice(&o.script);
    }
    b
}

impl BtcTx {
    /// Serialization with witnesses (segwit) or without (txid, unsigned).
    pub(crate) fn serialize(&self, witnesses: Option<&[Vec<Vec<u8>>]>) -> Vec<u8> {
        let mut b = self.version.to_le_bytes().to_vec();
        if witnesses.is_some() {
            b.extend_from_slice(&[0x00, 0x01]);
        }
        b.extend(compact_size(self.inputs.len()));
        for i in &self.inputs {
            b.extend_from_slice(&i.txid[..]);
            b.extend_from_slice(&i.vout.to_le_bytes());
            b.push(0);
            b.extend_from_slice(&i.sequence.to_le_bytes());
        }
        b.extend(compact_size(self.outputs.len()));
        b.extend(output_bytes(&self.outputs));
        if let Some(ws) = witnesses {
            for items in ws {
                b.extend(compact_size(items.len()));
                for item in items {
                    b.extend(compact_size(item.len()));
                    b.extend_from_slice(item);
                }
            }
        }
        b.extend_from_slice(&self.locktime.to_le_bytes());
        b
    }

    /// Displayed transaction ID: reversed double SHA-256 of the
    /// serialization without witnesses.
    pub(crate) fn txid(&self) -> String {
        let mut h = sha256d(&self.serialize(None)).into_array();
        h.reverse();
        h.iter().map(|b| format!("{b:02x}")).collect()
    }
}

fn sha(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// BIP-341 signature message hash with SIGHASH_DEFAULT; `leaf` selects a
/// BIP-342 script-path spend.
pub(crate) fn taproot_sighash(
    tx: &BtcTx,
    amounts: &[u64],
    scripts: &[Vec<u8>],
    index: usize,
    leaf: Option<&[u8]>,
) -> [u8; 32] {
    let mut prevouts = vec![];
    let mut sequences = vec![];
    for i in &tx.inputs {
        prevouts.extend_from_slice(&i.txid[..]);
        prevouts.extend_from_slice(&i.vout.to_le_bytes());
        sequences.extend_from_slice(&i.sequence.to_le_bytes());
    }
    let amounts: Vec<u8> = amounts.iter().flat_map(|a| a.to_le_bytes()).collect();
    let scripts: Vec<u8> = scripts
        .iter()
        .flat_map(|s| [compact_size(s.len()), s.clone()].concat())
        .collect();
    let mut m = vec![0x00, 0x00];
    m.extend_from_slice(&tx.version.to_le_bytes());
    m.extend_from_slice(&tx.locktime.to_le_bytes());
    m.extend_from_slice(&sha(&prevouts));
    m.extend_from_slice(&sha(&amounts));
    m.extend_from_slice(&sha(&scripts));
    m.extend_from_slice(&sha(&sequences));
    m.extend_from_slice(&sha(&output_bytes(&tx.outputs)));
    m.push(if leaf.is_some() { 2 } else { 0 });
    m.extend_from_slice(&(index as u32).to_le_bytes());
    if let Some(leaf) = leaf {
        m.extend_from_slice(&tapleaf_hash(leaf));
        m.push(0x00);
        m.extend_from_slice(&0xffff_ffffu32.to_le_bytes());
    }
    tagged_hash("TapSighash", &m)
}

/// BIP-143 SIGHASH_ALL of a P2WPKH input.
pub(crate) fn segwit_sighash(tx: &BtcTx, amount: u64, public_key: &[u8], index: usize) -> [u8; 32] {
    let mut prevouts = vec![];
    let mut sequences = vec![];
    for i in &tx.inputs {
        prevouts.extend_from_slice(&i.txid[..]);
        prevouts.extend_from_slice(&i.vout.to_le_bytes());
        sequences.extend_from_slice(&i.sequence.to_le_bytes());
    }
    let input = &tx.inputs[index];
    let mut m = tx.version.to_le_bytes().to_vec();
    m.extend_from_slice(&sha256d(&prevouts)[..]);
    m.extend_from_slice(&sha256d(&sequences)[..]);
    m.extend_from_slice(&input.txid[..]);
    m.extend_from_slice(&input.vout.to_le_bytes());
    m.extend_from_slice(&[0x19, 0x76, 0xa9, 0x14]);
    m.extend_from_slice(&hash160(public_key));
    m.extend_from_slice(&[0x88, 0xac]);
    m.extend_from_slice(&amount.to_le_bytes());
    m.extend_from_slice(&input.sequence.to_le_bytes());
    m.extend_from_slice(&sha256d(&output_bytes(&tx.outputs))[..]);
    m.extend_from_slice(&tx.locktime.to_le_bytes());
    m.extend_from_slice(&1u32.to_le_bytes());
    sha256d(&m).into_array()
}

/// DER encoding of a low-s ECDSA signature followed by SIGHASH_ALL.
pub(crate) fn der_signature(sig: &[u8]) -> Result<Vec<u8>> {
    let sig = k256::ecdsa::Signature::from_slice(sig).map_err(|_| Error::IntegrityFailed)?;
    let sig = sig.normalize_s();
    let bytes = sig.to_bytes();
    let int = |v: &[u8]| {
        let mut v: Vec<u8> = v.iter().copied().skip_while(|b| *b == 0).collect();
        if v.first().is_none_or(|b| b & 0x80 != 0) {
            v.insert(0, 0);
        }
        [vec![0x02, v.len() as u8], v].concat()
    };
    let body = [int(&bytes[..32]), int(&bytes[32..])].concat();
    Ok([vec![0x30, body.len() as u8], body, vec![0x01]].concat())
}

fn witness_size(spend: &BtcSpend) -> usize {
    match spend {
        BtcSpend::KeyPath => 1 + 65,
        BtcSpend::Segwit { .. } => 1 + 73 + 34,
        BtcSpend::Script {
            leaf,
            control,
            cosigner,
        } => {
            1 + 65
                + usize::from(cosigner.is_some()) * 65
                + compact_size(leaf.len()).len()
                + leaf.len()
                + 1
                + control.len()
        }
    }
}

/// Virtual size of the signed transaction.
pub(crate) fn vsize(tx: &BtcTx, spends: &[BtcSpend]) -> u64 {
    let base = tx.serialize(None).len();
    let witness = 2 + spends.iter().map(witness_size).sum::<usize>();
    ((base * 4 + witness) as u64).div_ceil(4)
}

/// The cosigner x-only key of a native Taproot address.
pub(crate) fn native_cosigner(w: &Wallet, control: &TaprootControl) -> Result<[u8; 32]> {
    let c = w.cosigner(control.cosigner)?;
    ensure_valid(
        c.input.kind == CosignerKind::Bip340,
        "taproot cosigner kind",
    )?;
    c.input
        .public_key
        .as_slice()
        .try_into()
        .map_err(|_| Error::IntegrityFailed)
}

/// The output script of a wallet Bitcoin address.
pub(crate) fn address_script(w: &Wallet, a: &Address) -> Result<Vec<u8>> {
    let cosigner = match &a.kind {
        AddressKind::Taproot(control) => Some(native_cosigner(w, control)?),
        _ => None,
    };
    btc_address_script(w.settings.btc_network, a, cosigner.as_ref())
}

/// Spend of an input of `a`; `timelock` takes the chain-key-only leaf of a
/// native address.
fn spend(w: &Wallet, a: &Address, timelock: bool) -> Result<(BtcSpend, SignSpec, u32)> {
    let path = crate::keys::path_bytes(&derivation_path(a.chain, &a.kind, a.id)?);
    Ok(match &a.kind {
        AddressKind::ChainKey => (
            BtcSpend::KeyPath,
            SignSpec::Bip340 {
                path,
                tweak: Some(ByteBuf::new()),
            },
            SEQUENCE_RBF,
        ),
        AddressKind::Segwit => (
            BtcSpend::Segwit {
                public_key: a.public_key.clone(),
            },
            SignSpec::Ecdsa { path },
            SEQUENCE_RBF,
        ),
        AddressKind::Taproot(control) => {
            let tree = taproot_tree(
                &x_only(&a.public_key)?,
                &native_cosigner(w, control)?,
                control,
            );
            let (_, odd) = taproot_output_key(&NUMS_KEY, Some(&tree.root))?;
            let leaf = usize::from(timelock);
            (
                BtcSpend::Script {
                    leaf: tree.leaves[leaf].clone().into(),
                    control: taproot_control_block(&tree, leaf, odd).into(),
                    cosigner: (!timelock).then_some(control.cosigner),
                },
                SignSpec::Bip340 { path, tweak: None },
                if timelock {
                    u32::from(control.chain_key_after)
                } else {
                    SEQUENCE_RBF
                },
            )
        }
    })
}

/// Build a payment from address `from` with the executor's UTXOs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build(
    w: &Wallet,
    from: &Address,
    outputs: &[(Destination, u128)],
    max_fee: u128,
    utxos: &[Utxo],
    fee_rate: u64,
    locktime: u32,
    timelock: bool,
) -> Result<Built> {
    let network = w.settings.btc_network;
    ensure_valid(from.chain == Chain::Bitcoin, "bitcoin address")?;
    ensure_valid(
        !utxos.is_empty() && utxos.len() <= MAX_BTC_INPUTS,
        "bitcoin inputs",
    )?;
    ensure_valid(
        !timelock || matches!(from.kind, AddressKind::Taproot(_)),
        "timelock path",
    )?;
    ensure_valid(
        locktime < 500_000_000 && (1..=10_000).contains(&fee_rate),
        "bitcoin params",
    )?;
    ensure_valid(
        from.cosign.is_none() || !matches!(from.kind, AddressKind::Taproot(_)),
        "native cosign",
    )?;
    let script = address_script(w, from)?;
    let (spend_kind, sign, sequence) = spend(w, from, timelock)?;
    let mut inputs = vec![];
    let mut total_in: u64 = 0;
    for (i, u) in utxos.iter().enumerate() {
        ensure_valid(u.address == from.id, "utxo address")?;
        ensure_valid(
            !utxos[..i]
                .iter()
                .any(|x| x.txid == u.txid && x.vout == u.vout),
            "duplicate utxo",
        )?;
        total_in = total_in
            .checked_add(u.value)
            .ok_or_else(|| invalid("utxo value"))?;
        inputs.push(TxIn {
            txid: u.txid,
            vout: u.vout,
            sequence,
        });
    }
    let mut outs = vec![];
    let mut summary = vec![];
    let mut total_out: u64 = 0;
    for (to, amount) in outputs {
        let Destination::Bitcoin(text) = to else {
            return Err(invalid("bitcoin destination"));
        };
        let value = u64::try_from(*amount).map_err(|_| invalid("amount"))?;
        ensure_valid(value >= DUST, "dust output")?;
        total_out = total_out
            .checked_add(value)
            .ok_or_else(|| invalid("amount"))?;
        outs.push(TxOut {
            value,
            script: btc_script(network, text)?.into(),
        });
        summary.push(SummaryOutput {
            to: dest_text(to),
            amount: *amount,
        });
    }
    let spends = vec![spend_kind; inputs.len()];
    let mut tx = BtcTx {
        version: 2,
        inputs,
        outputs: outs,
        locktime,
    };
    // What is left must pay the transaction without change; change is
    // added when what remains after its own fee is at least dust.
    let left = total_in.checked_sub(total_out).ok_or(Error::FeeBlocked)?;
    ensure(left >= fee_rate * vsize(&tx, &spends), Error::FeeBlocked)?;
    tx.outputs.push(TxOut {
        value: 0,
        script: script.clone().into(),
    });
    let mut change = left.saturating_sub(fee_rate * vsize(&tx, &spends));
    if change < DUST {
        tx.outputs.pop();
        change = 0;
    } else {
        tx.outputs.last_mut().expect("change").value = change;
    }
    let fee = left - change;
    ensure(u128::from(fee) <= max_fee, Error::FeeBlocked)?;
    let amounts: Vec<u64> = utxos.iter().map(|u| u.value).collect();
    let scripts = vec![script; utxos.len()];
    let digests = spends
        .iter()
        .enumerate()
        .map(|(i, s)| {
            Hash::new(match s {
                BtcSpend::KeyPath => taproot_sighash(&tx, &amounts, &scripts, i, None),
                BtcSpend::Segwit { public_key } => segwit_sighash(&tx, amounts[i], public_key, i),
                BtcSpend::Script { leaf, .. } => {
                    taproot_sighash(&tx, &amounts, &scripts, i, Some(leaf))
                }
            })
        })
        .collect();
    Ok(Built {
        unsigned: tx.serialize(None),
        digests,
        signs: vec![sign; tx.inputs.len()],
        summary: DecodedSummary {
            chain: format!("bitcoin:{}", btc_hrp(network)),
            from: from.text.clone(),
            asset: "BTC".into(),
            outputs: summary,
            change: (change > 0).then(|| SummaryOutput {
                to: from.text.clone(),
                amount: u128::from(change),
            }),
            fee: u128::from(fee),
            nonce: String::new(),
        },
        assembly: super::Assembly::Bitcoin { tx, spends },
    })
}

/// Witnesses from threshold signatures and native cosigner signatures.
pub(crate) fn assemble(
    tx: &BtcTx,
    spends: &[BtcSpend],
    sigs: &[Vec<u8>],
    cosigs: &[Option<Vec<u8>>],
) -> Result<(Vec<u8>, String)> {
    let mut witnesses = vec![];
    for (i, s) in spends.iter().enumerate() {
        let sig = sigs.get(i).ok_or(Error::IntegrityFailed)?;
        witnesses.push(match s {
            BtcSpend::KeyPath => vec![sig.clone()],
            BtcSpend::Segwit { public_key } => vec![der_signature(sig)?, public_key.to_vec()],
            BtcSpend::Script {
                leaf,
                control,
                cosigner,
            } => {
                let mut items = vec![];
                if cosigner.is_some() {
                    items.push(
                        cosigs
                            .get(i)
                            .cloned()
                            .flatten()
                            .ok_or(Error::IntegrityFailed)?,
                    );
                }
                items.extend([sig.clone(), leaf.to_vec(), control.to_vec()]);
                items
            }
        });
    }
    Ok((tx.serialize(Some(&witnesses)), tx.txid()))
}
