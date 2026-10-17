//! Independent decoding of a wallet plan. The wallet canister builds every
//! transaction; this module parses the unsigned bytes on its own, recomputes
//! the messages to sign, and checks both against the approved intent before
//! anyone cosigns or broadcasts. A mismatch is refused, whichever side is
//! wrong.
use dmsg_protocol::wallet::*;
use dmsg_types::{Error, Hash, Result, ensure, invalid, wallet::*};
use sha2::{Digest, Sha256};
use sha3::Keccak256;

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.bytes.len());
        let end = end.ok_or(Error::IntegrityFailed)?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("4")))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("8")))
    }

    fn varint(&mut self) -> Result<usize> {
        Ok(match self.u8()? {
            0xfd => u16::from_le_bytes(self.take(2)?.try_into().expect("2")) as usize,
            0xfe => self.u32()? as usize,
            0xff => return Err(Error::IntegrityFailed),
            n => n as usize,
        })
    }

    fn short_vec(&mut self) -> Result<usize> {
        let mut n = 0usize;
        for shift in [0, 7, 14] {
            let b = self.u8()?;
            n |= usize::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(n);
            }
        }
        Err(Error::IntegrityFailed)
    }

    fn done(&self) -> Result<()> {
        ensure(self.at == self.bytes.len(), Error::IntegrityFailed)
    }
}

/// A Bitcoin transaction without witnesses.
pub struct BtcTx {
    /// Version.
    pub version: u32,
    /// Outpoints (internal txid order) and sequences.
    pub inputs: Vec<([u8; 32], u32, u32)>,
    /// Values and output scripts.
    pub outputs: Vec<(u64, Vec<u8>)>,
    /// Lock time.
    pub locktime: u32,
}

/// Parse a serialized transaction without witnesses.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for malformed bytes.
pub fn parse_btc(bytes: &[u8]) -> Result<BtcTx> {
    let mut r = Reader { bytes, at: 0 };
    let version = r.u32()?;
    let n = r.varint()?;
    ensure(n > 0 && n <= MAX_BTC_INPUTS, Error::IntegrityFailed)?;
    let mut inputs = vec![];
    for _ in 0..n {
        let txid: [u8; 32] = r.take(32)?.try_into().expect("32");
        let vout = r.u32()?;
        ensure(r.varint()? == 0, Error::IntegrityFailed)?;
        inputs.push((txid, vout, r.u32()?));
    }
    let m = r.varint()?;
    let mut outputs = vec![];
    for _ in 0..m {
        let value = r.u64()?;
        let len = r.varint()?;
        outputs.push((value, r.take(len)?.to_vec()));
    }
    let locktime = r.u32()?;
    r.done()?;
    Ok(BtcTx {
        version,
        inputs,
        outputs,
        locktime,
    })
}

fn sha(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

fn sha_d(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(Sha256::digest(bytes)).into()
}

fn outputs_bytes(tx: &BtcTx) -> Vec<u8> {
    tx.outputs
        .iter()
        .flat_map(|(v, s)| [v.to_le_bytes().to_vec(), compact_size(s.len()), s.clone()].concat())
        .collect()
}

/// BIP-341 SIGHASH_DEFAULT, written apart from the wallet's encoder.
fn bip341(
    tx: &BtcTx,
    amounts: &[u64],
    script: &[u8],
    index: usize,
    leaf: Option<&[u8]>,
) -> [u8; 32] {
    let outpoints: Vec<u8> = tx
        .inputs
        .iter()
        .flat_map(|(t, v, _)| [t.to_vec(), v.to_le_bytes().to_vec()].concat())
        .collect();
    let sequences: Vec<u8> = tx
        .inputs
        .iter()
        .flat_map(|(_, _, s)| s.to_le_bytes())
        .collect();
    let values: Vec<u8> = amounts.iter().flat_map(|a| a.to_le_bytes()).collect();
    let script_entry = [compact_size(script.len()), script.to_vec()].concat();
    let scripts: Vec<u8> = tx
        .inputs
        .iter()
        .flat_map(|_| script_entry.clone())
        .collect();
    let mut msg = Vec::with_capacity(250);
    msg.push(0); // epoch
    msg.push(0); // SIGHASH_DEFAULT
    msg.extend_from_slice(&tx.version.to_le_bytes());
    msg.extend_from_slice(&tx.locktime.to_le_bytes());
    for part in [
        &outpoints,
        &values,
        &scripts,
        &sequences,
        &outputs_bytes(tx),
    ] {
        msg.extend_from_slice(&sha(&[part]));
    }
    msg.push(if leaf.is_some() { 2 } else { 0 });
    msg.extend_from_slice(&(index as u32).to_le_bytes());
    if let Some(leaf) = leaf {
        msg.extend_from_slice(&tapleaf_hash(leaf));
        msg.push(0);
        msg.extend_from_slice(&[0xff; 4]);
    }
    tagged_hash("TapSighash", &msg)
}

/// BIP-143 SIGHASH_ALL of a P2WPKH input.
fn bip143(tx: &BtcTx, amount: u64, key: &[u8], index: usize) -> [u8; 32] {
    let outpoints: Vec<u8> = tx
        .inputs
        .iter()
        .flat_map(|(t, v, _)| [t.to_vec(), v.to_le_bytes().to_vec()].concat())
        .collect();
    let sequences: Vec<u8> = tx
        .inputs
        .iter()
        .flat_map(|(_, _, s)| s.to_le_bytes())
        .collect();
    let (txid, vout, sequence) = tx.inputs[index];
    let code = [&[0x19, 0x76, 0xa9, 0x14][..], &hash160(key), &[0x88, 0xac]].concat();
    sha_d(
        &[
            &tx.version.to_le_bytes()[..],
            &sha_d(&outpoints),
            &sha_d(&sequences),
            &txid,
            &vout.to_le_bytes(),
            &code,
            &amount.to_le_bytes(),
            &sequence.to_le_bytes(),
            &sha_d(&outputs_bytes(tx)),
            &tx.locktime.to_le_bytes(),
            &1u32.to_le_bytes(),
        ]
        .concat(),
    )
}

fn cosigner_key(view: &WalletView, id: u32) -> Result<[u8; 32]> {
    let c = view
        .cosigners
        .iter()
        .find(|c| c.id == id)
        .ok_or(Error::NotFound)?;
    c.input
        .public_key
        .as_slice()
        .try_into()
        .map_err(|_| Error::IntegrityFailed)
}

/// What an intent pays: its source, asset, outputs and fee ceiling.
struct Paying {
    from: Address,
    asset: Option<AssetKey>,
    outputs: Vec<(Destination, u128)>,
    max_fee: u128,
}

fn paying(view: &WalletView, intent: &Intent) -> Result<Paying> {
    let address = |id: &u32| {
        view.addresses
            .iter()
            .find(|a| a.id == *id)
            .cloned()
            .ok_or(Error::NotFound)
    };
    Ok(match intent {
        Intent::Transfer {
            from,
            asset,
            to,
            amount,
            max_fee,
            ..
        } => Paying {
            from: address(from)?,
            asset: Some(asset.clone()),
            outputs: vec![(to.clone(), *amount)],
            max_fee: *max_fee,
        },
        Intent::Batch {
            from,
            asset,
            items,
            max_fee,
        } => Paying {
            from: address(from)?,
            asset: Some(asset.clone()),
            outputs: items.iter().map(|i| (i.to.clone(), i.amount)).collect(),
            max_fee: *max_fee,
        },
        Intent::CancelEvmNonce {
            address: id,
            max_fee,
            ..
        }
        | Intent::InitSolNonce {
            address: id,
            max_fee,
        } => Paying {
            from: address(id)?,
            asset: None,
            outputs: vec![],
            max_fee: *max_fee,
        },
        _ => return Err(invalid("not an external-chain intent")),
    })
}

/// Check a plan against the intent it executes; returns the summary to
/// show, decoded here.
///
/// # Errors
/// Returns `Error::IntegrityFailed` when the bytes, the messages to sign or
/// the wallet's summary differ from what the intent allows.
pub fn verify_plan(
    view: &WalletView,
    intent: &Intent,
    plan: &ExecutionPlan,
) -> Result<DecodedSummary> {
    let pay = paying(view, intent)?;
    let summary = match (&plan.params, &pay.from.chain) {
        (
            ExecutionParams::Bitcoin {
                utxos, timelock, ..
            },
            Chain::Bitcoin,
        ) => verify_btc(view, &pay, plan, utxos, *timelock)?,
        (ExecutionParams::Evm { .. }, Chain::Evm) => verify_evm(intent, &pay, plan)?,
        (ExecutionParams::Solana { .. }, Chain::Solana) => verify_sol(intent, &pay, plan)?,
        _ => return Err(Error::IntegrityFailed),
    };
    ensure(summary == plan.summary, Error::IntegrityFailed)?;
    ensure(summary.fee <= pay.max_fee, Error::IntegrityFailed)?;
    Ok(summary)
}

fn verify_btc(
    view: &WalletView,
    pay: &Paying,
    plan: &ExecutionPlan,
    utxos: &[Utxo],
    timelock: bool,
) -> Result<DecodedSummary> {
    let network = view.btc_network;
    let tx = parse_btc(&plan.unsigned)?;
    let cosigner = match &pay.from.kind {
        AddressKind::Taproot(control) => Some(cosigner_key(view, control.cosigner)?),
        _ => None,
    };
    let own = btc_address_script(network, &pay.from, cosigner.as_ref())?;
    ensure(
        tx.version == 2 && tx.inputs.len() == utxos.len(),
        Error::IntegrityFailed,
    )?;
    for ((txid, vout, _), u) in tx.inputs.iter().zip(utxos) {
        ensure(
            *txid == u.txid.into_array() && *vout == u.vout && u.address == pay.from.id,
            Error::IntegrityFailed,
        )?;
    }
    // Payments in intent order, then at most one change output.
    ensure(
        tx.outputs.len() == pay.outputs.len() || tx.outputs.len() == pay.outputs.len() + 1,
        Error::IntegrityFailed,
    )?;
    let mut outputs = vec![];
    for ((value, script), (to, amount)) in tx.outputs.iter().zip(&pay.outputs) {
        let Destination::Bitcoin(text) = to else {
            return Err(Error::IntegrityFailed);
        };
        ensure(
            *script == btc_script(network, text)? && u128::from(*value) == *amount,
            Error::IntegrityFailed,
        )?;
        outputs.push(SummaryOutput {
            to: text.clone(),
            amount: *amount,
        });
    }
    let change = match tx.outputs.get(pay.outputs.len()) {
        Some((value, script)) => {
            ensure(*script == own, Error::IntegrityFailed)?;
            Some(SummaryOutput {
                to: pay.from.text.clone(),
                amount: u128::from(*value),
            })
        }
        None => None,
    };
    let total_in: u64 = utxos.iter().map(|u| u.value).sum();
    let total_out: u64 = tx.outputs.iter().map(|(v, _)| v).sum();
    let fee = total_in
        .checked_sub(total_out)
        .ok_or(Error::IntegrityFailed)?;
    let amounts: Vec<u64> = utxos.iter().map(|u| u.value).collect();
    let digests: Vec<Hash> = (0..tx.inputs.len())
        .map(|i| {
            Ok(Hash::new(match &pay.from.kind {
                AddressKind::ChainKey => bip341(&tx, &amounts, &own, i, None),
                AddressKind::Segwit => bip143(&tx, amounts[i], &pay.from.public_key, i),
                AddressKind::Taproot(control) => {
                    let tree = taproot_tree(
                        &x_only(&pay.from.public_key)?,
                        &cosigner.expect("native"),
                        control,
                    );
                    bip341(
                        &tx,
                        &amounts,
                        &own,
                        i,
                        Some(&tree.leaves[usize::from(timelock)]),
                    )
                }
            }))
        })
        .collect::<Result<_>>()?;
    ensure(digests == plan.digests, Error::IntegrityFailed)?;
    Ok(DecodedSummary {
        chain: format!("bitcoin:{}", btc_hrp(network)),
        from: pay.from.text.clone(),
        asset: "BTC".into(),
        outputs,
        change,
        fee: u128::from(fee),
        nonce: String::new(),
    })
}

/// One RLP item: a byte string or a list of items.
enum Rlp {
    Bytes(Vec<u8>),
    List(Vec<Rlp>),
}

fn rlp_len(r: &mut Reader<'_>, n: usize) -> Result<usize> {
    let bytes = r.take(n)?;
    ensure(n <= 8 && bytes.first() != Some(&0), Error::IntegrityFailed)?;
    Ok(bytes
        .iter()
        .fold(0usize, |acc, b| (acc << 8) | usize::from(*b)))
}

fn rlp_item(r: &mut Reader<'_>) -> Result<Rlp> {
    let b = r.u8()?;
    Ok(match b {
        0x00..=0x7f => Rlp::Bytes(vec![b]),
        0x80..=0xb7 => Rlp::Bytes(r.take(usize::from(b - 0x80))?.to_vec()),
        0xb8..=0xbf => {
            let n = rlp_len(r, usize::from(b - 0xb7))?;
            Rlp::Bytes(r.take(n)?.to_vec())
        }
        _ => {
            let n = if b <= 0xf7 {
                usize::from(b - 0xc0)
            } else {
                rlp_len(r, usize::from(b - 0xf7))?
            };
            let body = r.take(n)?;
            let mut inner = Reader { bytes: body, at: 0 };
            let mut items = vec![];
            while inner.at < body.len() {
                items.push(rlp_item(&mut inner)?);
            }
            Rlp::List(items)
        }
    })
}

fn rlp_uint(item: &Rlp) -> Result<u128> {
    let Rlp::Bytes(b) = item else {
        return Err(Error::IntegrityFailed);
    };
    ensure(
        b.len() <= 16 && b.first() != Some(&0),
        Error::IntegrityFailed,
    )?;
    Ok(b.iter().fold(0u128, |acc, x| (acc << 8) | u128::from(*x)))
}

fn rlp_bytes(item: &Rlp) -> Result<&[u8]> {
    match item {
        Rlp::Bytes(b) => Ok(b),
        Rlp::List(_) => Err(Error::IntegrityFailed),
    }
}

fn verify_evm(intent: &Intent, pay: &Paying, plan: &ExecutionPlan) -> Result<DecodedSummary> {
    let bytes = &plan.unsigned;
    ensure(bytes.first() == Some(&0x02), Error::IntegrityFailed)?;
    let mut r = Reader {
        bytes: &bytes[1..],
        at: 0,
    };
    let Rlp::List(f) = rlp_item(&mut r)? else {
        return Err(Error::IntegrityFailed);
    };
    r.done()?;
    ensure(f.len() == 9, Error::IntegrityFailed)?;
    ensure(
        matches!(&f[8], Rlp::List(l) if l.is_empty()),
        Error::IntegrityFailed,
    )?;
    let chain_id = u64::try_from(rlp_uint(&f[0])?).map_err(|_| Error::IntegrityFailed)?;
    let nonce = rlp_uint(&f[1])?;
    let max_fee_per_gas = rlp_uint(&f[3])?;
    let gas = rlp_uint(&f[4])?;
    let to: [u8; 20] = rlp_bytes(&f[5])?
        .try_into()
        .map_err(|_| Error::IntegrityFailed)?;
    let value = rlp_uint(&f[6])?;
    let data = rlp_bytes(&f[7])?;
    let own = evm_address(&pay.from.public_key)?;
    let (expected_chain, asset, recipient, amount) = match (intent, &pay.asset) {
        (Intent::CancelEvmNonce { chain_id, .. }, None) => {
            ensure(
                to == own && value == 0 && data.is_empty(),
                Error::IntegrityFailed,
            )?;
            (*chain_id, "ETH".to_string(), own, 0)
        }
        (
            _,
            Some(AssetKey::Evm {
                chain_id,
                token: None,
            }),
        ) => {
            ensure(data.is_empty(), Error::IntegrityFailed)?;
            (*chain_id, "ETH".to_string(), to, value)
        }
        (
            _,
            Some(AssetKey::Evm {
                chain_id,
                token: Some(token),
            }),
        ) => {
            ensure(
                to == token.into_array()
                    && value == 0
                    && data.len() == 68
                    && data[..4] == [0xa9, 0x05, 0x9c, 0xbb]
                    && data[4..16] == [0; 12]
                    && data[36..52] == [0; 16],
                Error::IntegrityFailed,
            )?;
            let recipient: [u8; 20] = data[16..36].try_into().expect("20");
            let amount = u128::from_be_bytes(data[52..68].try_into().expect("16"));
            (
                *chain_id,
                format!("ERC20:{}", eip55(&to)),
                recipient,
                amount,
            )
        }
        _ => return Err(Error::IntegrityFailed),
    };
    ensure(chain_id == expected_chain, Error::IntegrityFailed)?;
    if let Some((Destination::Evm(dest), want)) = pay.outputs.first() {
        ensure(
            pay.outputs.len() == 1 && recipient == dest.into_array() && amount == *want,
            Error::IntegrityFailed,
        )?;
    }
    let digest: [u8; 32] = Keccak256::digest(bytes.as_slice()).into();
    ensure(
        plan.digests == vec![Hash::new(digest)],
        Error::IntegrityFailed,
    )?;
    Ok(DecodedSummary {
        chain: format!("evm:{chain_id}"),
        from: pay.from.text.clone(),
        asset,
        outputs: vec![SummaryOutput {
            to: eip55(&recipient),
            amount,
        }],
        change: None,
        fee: gas
            .checked_mul(max_fee_per_gas)
            .ok_or(Error::IntegrityFailed)?,
        nonce: nonce.to_string(),
    })
}

fn program(text: &str) -> [u8; 32] {
    parse_sol_address(text).expect("program address")
}

fn verify_sol(intent: &Intent, pay: &Paying, plan: &ExecutionPlan) -> Result<DecodedSummary> {
    let bytes = &plan.unsigned;
    let mut r = Reader { bytes, at: 0 };
    let header = r.take(3)?;
    ensure(header[0] == 1, Error::IntegrityFailed)?;
    let n = r.short_vec()?;
    let keys: Vec<[u8; 32]> = (0..n)
        .map(|_| Ok(r.take(32)?.try_into().expect("32")))
        .collect::<Result<_>>()?;
    let payer: [u8; 32] = pay
        .from
        .public_key
        .as_slice()
        .try_into()
        .map_err(|_| Error::IntegrityFailed)?;
    ensure(keys.first() == Some(&payer), Error::IntegrityFailed)?;
    let blockhash: [u8; 32] = r.take(32)?.try_into().expect("32");
    let system = [0u8; 32];
    let budget = program("ComputeBudget111111111111111111111111111111");
    let token = program("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
    let ata = program("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
    let (mut limit, mut price) = (200_000u128, 0u128);
    let mut paid: Vec<([u8; 32], u128)> = vec![];
    let mut created = vec![];
    let mut advanced = false;
    let ixs = r.short_vec()?;
    for i in 0..ixs {
        let program_id = *keys
            .get(usize::from(r.u8()?))
            .ok_or(Error::IntegrityFailed)?;
        let accounts: Vec<[u8; 32]> = (0..r.short_vec()?)
            .map(|_| {
                keys.get(usize::from(r.u8()?))
                    .copied()
                    .ok_or(Error::IntegrityFailed)
            })
            .collect::<Result<_>>()?;
        let len = r.short_vec()?;
        let data = r.take(len)?;
        let kind = data.first().copied();
        match program_id {
            p if p == budget && kind == Some(2) && data.len() == 5 => {
                limit = u128::from(u32::from_le_bytes(data[1..].try_into().expect("4")));
            }
            p if p == budget && kind == Some(3) && data.len() == 9 => {
                price = u128::from(u64::from_le_bytes(data[1..].try_into().expect("8")));
            }
            p if p == system && data.len() == 12 && data[..4] == 2u32.to_le_bytes() => {
                ensure(
                    accounts.len() == 2 && accounts[0] == payer,
                    Error::IntegrityFailed,
                )?;
                paid.push((
                    accounts[1],
                    u128::from(u64::from_le_bytes(data[4..].try_into().expect("8"))),
                ));
            }
            p if p == system && data == 4u32.to_le_bytes() => {
                // The durable nonce advance comes first and only then.
                ensure(
                    i == 0 && accounts.get(2) == Some(&payer),
                    Error::IntegrityFailed,
                )?;
                ensure(
                    pay.from
                        .sol_nonce
                        .is_some_and(|n| n.into_array() == accounts[0]),
                    Error::IntegrityFailed,
                )?;
                advanced = true;
            }
            p if p == system
                && kind == Some(3)
                && matches!(intent, Intent::InitSolNonce { .. }) =>
            {
                let lamports_at = 4
                    + 32
                    + 8
                    + data
                        .get(36..44)
                        .map_or(0, |l| u64::from_le_bytes(l.try_into().expect("8")) as usize);
                let lamports = data
                    .get(lamports_at..lamports_at + 8)
                    .map(|l| u64::from_le_bytes(l.try_into().expect("8")))
                    .ok_or(Error::IntegrityFailed)?;
                ensure(
                    accounts.len() == 2 && accounts[0] == payer,
                    Error::IntegrityFailed,
                )?;
                created.push((accounts[1], u128::from(lamports)));
            }
            p if p == system
                && kind == Some(6)
                && matches!(intent, Intent::InitSolNonce { .. }) =>
            {
                ensure(
                    data.len() == 36 && data[4..] == payer,
                    Error::IntegrityFailed,
                )?;
            }
            p if p == ata && data == [1] => {
                ensure(accounts.first() == Some(&payer), Error::IntegrityFailed)?;
            }
            p if p == token && kind == Some(12) && data.len() == 10 => {
                ensure(
                    accounts.len() == 4 && accounts[3] == payer,
                    Error::IntegrityFailed,
                )?;
                paid.push((
                    accounts[2],
                    u128::from(u64::from_le_bytes(data[1..9].try_into().expect("8"))),
                ));
            }
            _ => return Err(Error::IntegrityFailed),
        }
    }
    r.done()?;
    ensure(
        advanced == pay.from.sol_nonce.is_some() || matches!(intent, Intent::InitSolNonce { .. }),
        Error::IntegrityFailed,
    )?;
    let (asset, outputs) = match (&pay.asset, intent) {
        (None, Intent::InitSolNonce { .. }) => {
            ensure(
                created.len() == 1 && paid.is_empty(),
                Error::IntegrityFailed,
            )?;
            (
                "SOL".to_string(),
                vec![SummaryOutput {
                    to: sol_address(&created[0].0),
                    amount: created[0].1,
                }],
            )
        }
        (Some(AssetKey::Sol { mint }), _) => {
            ensure(
                paid.len() == pay.outputs.len() && created.is_empty(),
                Error::IntegrityFailed,
            )?;
            let mut outputs = vec![];
            for ((target, amount), (to, want)) in paid.iter().zip(&pay.outputs) {
                let Destination::Solana(owner) = to else {
                    return Err(Error::IntegrityFailed);
                };
                // An SPL payment goes to the owner's associated token account.
                let expected = match mint {
                    None => owner.into_array(),
                    Some(mint) => associated_token_account(&owner.into_array(), &mint.into_array()),
                };
                ensure(
                    *target == expected && amount == want,
                    Error::IntegrityFailed,
                )?;
                outputs.push(SummaryOutput {
                    to: sol_address(&owner.into_array()),
                    amount: *amount,
                });
            }
            let asset = match mint {
                None => "SOL".to_string(),
                Some(mint) => format!("SPL:{}", sol_address(&mint.into_array())),
            };
            (asset, outputs)
        }
        _ => return Err(Error::IntegrityFailed),
    };
    ensure(
        plan.digests == vec![Hash::new(Sha256::digest(bytes.as_slice()).into())],
        Error::IntegrityFailed,
    )?;
    Ok(DecodedSummary {
        chain: "solana".into(),
        from: pay.from.text.clone(),
        asset,
        outputs,
        change: None,
        fee: 5_000 + (limit * price).div_ceil(1_000_000),
        nonce: sol_address(&blockhash),
    })
}

/// The associated token account of `owner` for `mint`: the first
/// off-curve program address from bump 255 down.
pub fn associated_token_account(owner: &[u8; 32], mint: &[u8; 32]) -> [u8; 32] {
    let token = program("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
    let ata = program("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
    for bump in (0..=255u8).rev() {
        let h: [u8; 32] = sha(&[owner, &token, mint, &[bump], &ata, b"ProgramDerivedAddress"]);
        if ed25519_dalek::VerifyingKey::from_bytes(&h).is_err() {
            return h;
        }
    }
    unreachable!("a bump seed exists")
}
