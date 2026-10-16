//! Solana legacy messages: SOL and SPL transfers, compute budget, durable
//! nonces and nonce-account creation. The wallet address is the only signer.
use super::{dest_text, Built};
use crate::store::SignSpec;
use dmsg_protocol::wallet::*;
use dmsg_types::{wallet::*, *};
use serde_bytes::ByteBuf;
use sha2::{Digest, Sha256};

/// Fee of one signature in lamports.
pub(crate) const SIGNATURE_FEE: u128 = 5_000;
/// Rent-exempt balance of an 80-byte nonce account.
pub(crate) const NONCE_RENT: u64 = 1_447_680;
/// Seed of the nonce account derived from the address.
pub(crate) const NONCE_SEED: &str = "dmsg-wallet-nonce";

const SYSTEM: [u8; 32] = [0; 32];

fn key(text: &str) -> [u8; 32] {
    bs58::decode(text)
        .into_vec()
        .ok()
        .and_then(|v| v.try_into().ok())
        .expect("program address")
}

fn compute_budget() -> [u8; 32] {
    key("ComputeBudget111111111111111111111111111111")
}

pub(crate) fn token_program() -> [u8; 32] {
    key("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
}

pub(crate) fn ata_program() -> [u8; 32] {
    key("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
}

fn recent_blockhashes() -> [u8; 32] {
    key("SysvarRecentB1ockHashes11111111111111111111")
}

fn rent() -> [u8; 32] {
    key("SysvarRent111111111111111111111111111111111")
}

/// One instruction: program, accounts as (key, signer, writable), data.
pub(crate) struct Ix {
    pub(crate) program: [u8; 32],
    pub(crate) accounts: Vec<([u8; 32], bool, bool)>,
    pub(crate) data: Vec<u8>,
}

/// Solana short vector length.
pub(crate) fn short_vec(n: usize) -> Vec<u8> {
    let mut n = n;
    let mut out = vec![];
    loop {
        let mut b = (n & 0x7f) as u8;
        n >>= 7;
        if n > 0 {
            b |= 0x80;
        }
        out.push(b);
        if n == 0 {
            return out;
        }
    }
}

/// Compile a legacy message with `payer` as fee payer and sole signer.
pub(crate) fn compile(payer: [u8; 32], ixs: &[Ix], blockhash: [u8; 32]) -> Result<Vec<u8>> {
    let mut metas: Vec<([u8; 32], bool, bool)> = vec![(payer, true, true)];
    let mut add = |k: [u8; 32], signer: bool, writable: bool| {
        if let Some(m) = metas.iter_mut().find(|m| m.0 == k) {
            m.1 |= signer;
            m.2 |= writable;
        } else {
            metas.push((k, signer, writable));
        }
    };
    for ix in ixs {
        for (k, s, w) in &ix.accounts {
            add(*k, *s, *w);
        }
        add(ix.program, false, false);
    }
    // The canonical order: the payer, then each group sorted by key.
    let group = |s: bool, w: bool| {
        let mut keys: Vec<[u8; 32]> = metas
            .iter()
            .filter(|m| m.1 == s && m.2 == w && m.0 != payer)
            .map(|m| m.0)
            .collect();
        keys.sort();
        keys
    };
    let mut keys: Vec<[u8; 32]> = vec![payer];
    keys.extend(group(true, true));
    keys.extend(group(true, false));
    keys.extend(group(false, true));
    keys.extend(group(false, false));
    let signers = metas.iter().filter(|m| m.1).count();
    ensure_valid(signers == 1 && keys.len() <= 64, "solana accounts")?;
    let mut m = vec![
        1,
        metas.iter().filter(|m| m.1 && !m.2).count() as u8,
        metas.iter().filter(|m| !m.1 && !m.2).count() as u8,
    ];
    m.extend(short_vec(keys.len()));
    for k in &keys {
        m.extend_from_slice(k);
    }
    m.extend_from_slice(&blockhash);
    m.extend(short_vec(ixs.len()));
    let index = |k: &[u8; 32]| keys.iter().position(|x| x == k).expect("listed") as u8;
    for ix in ixs {
        m.push(index(&ix.program));
        m.extend(short_vec(ix.accounts.len()));
        for (k, _, _) in &ix.accounts {
            m.push(index(k));
        }
        m.extend(short_vec(ix.data.len()));
        m.extend_from_slice(&ix.data);
    }
    Ok(m)
}

fn on_curve(bytes: &[u8; 32]) -> bool {
    ed25519_dalek::VerifyingKey::from_bytes(bytes).is_ok()
}

/// `find_program_address`.
pub(crate) fn program_address(seeds: &[&[u8]], program: &[u8; 32]) -> [u8; 32] {
    for bump in (0..=255u8).rev() {
        let mut h = Sha256::new();
        for s in seeds {
            h.update(s);
        }
        h.update([bump]);
        h.update(program);
        h.update(b"ProgramDerivedAddress");
        let out: [u8; 32] = h.finalize().into();
        if !on_curve(&out) {
            return out;
        }
    }
    unreachable!("a bump seed exists")
}

/// The associated token account of `owner` for `mint`.
pub(crate) fn ata(owner: &[u8; 32], mint: &[u8; 32]) -> [u8; 32] {
    program_address(&[owner, &token_program(), mint], &ata_program())
}

/// `create_with_seed` address of the nonce account of `base`.
pub(crate) fn nonce_account(base: &[u8; 32]) -> [u8; 32] {
    Sha256::new()
        .chain_update(base)
        .chain_update(NONCE_SEED.as_bytes())
        .chain_update(SYSTEM)
        .finalize()
        .into()
}

fn system(index: u32, rest: &[u8]) -> Vec<u8> {
    [&index.to_le_bytes()[..], rest].concat()
}

fn budget(limit: u32, price: u64) -> [Ix; 2] {
    [
        Ix {
            program: compute_budget(),
            accounts: vec![],
            data: [&[2u8][..], &limit.to_le_bytes()].concat(),
        },
        Ix {
            program: compute_budget(),
            accounts: vec![],
            data: [&[3u8][..], &price.to_le_bytes()].concat(),
        },
    ]
}

fn advance(nonce: [u8; 32], authority: [u8; 32]) -> Ix {
    Ix {
        program: SYSTEM,
        accounts: vec![
            (nonce, false, true),
            (recent_blockhashes(), false, false),
            (authority, true, false),
        ],
        data: system(4, &[]),
    }
}

/// Signature and priority fee in lamports.
pub(crate) fn fee(limit: u32, price: u64) -> u128 {
    SIGNATURE_FEE + (u128::from(limit) * u128::from(price)).div_ceil(1_000_000)
}

/// What a Solana plan does.
pub(crate) enum Payment<'a> {
    Transfer {
        mint: Option<Hash>,
        outputs: &'a [(Destination, u128)],
    },
    InitNonce,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build(
    from: &Address,
    payment: Payment<'_>,
    max_fee: u128,
    blockhash: Hash,
    price: u64,
    limit: u32,
    decimals: Option<u8>,
    create_ata: bool,
) -> Result<Built> {
    ensure_valid(from.chain == Chain::Solana, "solana address")?;
    ensure_valid((1_000..=1_400_000).contains(&limit), "compute unit limit")?;
    let payer: [u8; 32] = from
        .public_key
        .as_slice()
        .try_into()
        .map_err(|_| Error::IntegrityFailed)?;
    let fee = fee(limit, price);
    ensure(fee <= max_fee, Error::FeeBlocked)?;
    let mut ixs = vec![];
    if let Some(nonce) = from.sol_nonce {
        ixs.push(advance(nonce.into_array(), payer));
    }
    ixs.extend(budget(limit, price));
    let mut outputs = vec![];
    let asset;
    match payment {
        Payment::Transfer {
            mint: None,
            outputs: outs,
        } => {
            asset = "SOL".to_string();
            for (to, amount) in outs {
                let Destination::Solana(to_key) = to else {
                    return Err(invalid("solana destination"));
                };
                let lamports = u64::try_from(*amount).map_err(|_| invalid("amount"))?;
                ixs.push(Ix {
                    program: SYSTEM,
                    accounts: vec![(payer, true, true), (to_key.into_array(), false, true)],
                    data: system(2, &lamports.to_le_bytes()),
                });
                outputs.push(SummaryOutput {
                    to: dest_text(to),
                    amount: *amount,
                });
            }
        }
        Payment::Transfer {
            mint: Some(mint),
            outputs: outs,
        } => {
            let mint = mint.into_array();
            let decimals = decimals.ok_or_else(|| invalid("token decimals"))?;
            asset = format!("SPL:{}", sol_address(&mint));
            let source = ata(&payer, &mint);
            for (to, amount) in outs {
                let Destination::Solana(owner) = to else {
                    return Err(invalid("solana destination"));
                };
                let owner = owner.into_array();
                let target = ata(&owner, &mint);
                if create_ata {
                    ixs.push(Ix {
                        program: ata_program(),
                        accounts: vec![
                            (payer, true, true),
                            (target, false, true),
                            (owner, false, false),
                            (mint, false, false),
                            (SYSTEM, false, false),
                            (token_program(), false, false),
                        ],
                        data: vec![1],
                    });
                }
                let units = u64::try_from(*amount).map_err(|_| invalid("amount"))?;
                ixs.push(Ix {
                    program: token_program(),
                    accounts: vec![
                        (source, false, true),
                        (mint, false, false),
                        (target, false, true),
                        (payer, true, false),
                    ],
                    data: [&[12u8][..], &units.to_le_bytes(), &[decimals]].concat(),
                });
                outputs.push(SummaryOutput {
                    to: dest_text(to),
                    amount: *amount,
                });
            }
        }
        Payment::InitNonce => {
            ensure_valid(from.sol_nonce.is_none(), "nonce account exists")?;
            asset = "SOL".to_string();
            let nonce = nonce_account(&payer);
            let mut create = payer.to_vec();
            create.extend_from_slice(&(NONCE_SEED.len() as u64).to_le_bytes());
            create.extend_from_slice(NONCE_SEED.as_bytes());
            create.extend_from_slice(&NONCE_RENT.to_le_bytes());
            create.extend_from_slice(&80u64.to_le_bytes());
            create.extend_from_slice(&SYSTEM);
            ixs.push(Ix {
                program: SYSTEM,
                accounts: vec![(payer, true, true), (nonce, false, true)],
                data: system(3, &create),
            });
            ixs.push(Ix {
                program: SYSTEM,
                accounts: vec![
                    (nonce, false, true),
                    (recent_blockhashes(), false, false),
                    (rent(), false, false),
                ],
                data: system(6, &payer),
            });
            outputs.push(SummaryOutput {
                to: sol_address(&nonce),
                amount: u128::from(NONCE_RENT),
            });
        }
    }
    let message = compile(payer, &ixs, blockhash.into_array())?;
    ensure(message.len() <= 1_232 - 65, Error::QuotaExceeded)?;
    Ok(Built {
        unsigned: message.clone(),
        digests: vec![Hash::new(Sha256::digest(&message).into())],
        signs: vec![SignSpec::Ed25519 {
            path: crate::keys::path_bytes(&derivation_path(
                Chain::Solana,
                &AddressKind::ChainKey,
                from.id,
            )?),
        }],
        summary: DecodedSummary {
            chain: "solana".into(),
            from: from.text.clone(),
            asset,
            outputs,
            change: None,
            fee,
            nonce: sol_address(&blockhash.into_array()),
        },
        assembly: super::Assembly::Solana {
            message: ByteBuf::from(message),
        },
    })
}

/// The signed transaction and its ID, the base58 signature.
pub(crate) fn assemble(message: &[u8], sig: &[u8]) -> Result<(Vec<u8>, String)> {
    ensure(sig.len() == 64, Error::IntegrityFailed)?;
    let raw = [short_vec(1), sig.to_vec(), message.to_vec()].concat();
    Ok((raw, bs58::encode(sig).into_string()))
}
