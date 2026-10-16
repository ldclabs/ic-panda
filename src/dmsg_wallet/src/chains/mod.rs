//! External-chain plans. The wallet builds every transaction from the
//! approved intent and the executor's chain parameters, checks the
//! parameters against the intent's fee ceiling, and signs only what it built.
pub(crate) mod btc;
pub(crate) mod evm;
pub(crate) mod sol;

use crate::store::{SignSpec, Wallet};
use dmsg_types::{wallet::*, *};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// How to turn signatures into the signed transaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Assembly {
    Bitcoin {
        tx: btc::BtcTx,
        spends: Vec<btc::BtcSpend>,
    },
    Evm {
        tx: evm::Eip1559,
        public_key: ByteBuf,
    },
    Solana {
        message: ByteBuf,
    },
}

/// A built plan before it is stored.
pub(crate) struct Built {
    pub(crate) unsigned: Vec<u8>,
    pub(crate) digests: Vec<Hash>,
    pub(crate) signs: Vec<SignSpec>,
    pub(crate) summary: DecodedSummary,
    pub(crate) assembly: Assembly,
}

pub(crate) fn dest_text(to: &Destination) -> String {
    dmsg_protocol::wallet::destination_text(to)
}

/// The source address and the chain an intent signs on, if any.
pub(crate) fn chain_source(intent: &Intent) -> Option<(u32, Chain)> {
    match intent {
        Intent::Transfer { from, asset, .. } | Intent::Batch { from, asset, .. } => {
            let chain = dmsg_protocol::wallet::asset_chain(asset);
            (chain != Chain::Icp).then_some((*from, chain))
        }
        Intent::CancelEvmNonce { address, .. } => Some((*address, Chain::Evm)),
        Intent::InitSolNonce { address, .. } => Some((*address, Chain::Solana)),
        _ => None,
    }
}

/// Build the plan of an approved external-chain intent.
pub(crate) fn build(
    w: &Wallet,
    intent: &Intent,
    params: &ExecutionParams,
    pending_nonce: Option<u64>,
) -> Result<Built> {
    let (from, _) = chain_source(intent).ok_or_else(|| invalid("not an external-chain intent"))?;
    let from = w.address(from)?;
    let (asset, outputs, max_fee) = match intent {
        Intent::Transfer {
            asset,
            to,
            amount,
            max_fee,
            ..
        } => (Some(asset), vec![(to.clone(), *amount)], *max_fee),
        Intent::Batch {
            asset,
            items,
            max_fee,
            ..
        } => (
            Some(asset),
            items.iter().map(|i| (i.to.clone(), i.amount)).collect(),
            *max_fee,
        ),
        Intent::CancelEvmNonce { max_fee, .. } | Intent::InitSolNonce { max_fee, .. } => {
            (None, vec![], *max_fee)
        }
        _ => unreachable!("external-chain intent"),
    };
    match (intent, asset, params) {
        (
            _,
            Some(AssetKey::Btc),
            ExecutionParams::Bitcoin {
                utxos,
                fee_rate,
                locktime,
                timelock,
            },
        ) => btc::build(
            w, from, &outputs, max_fee, utxos, *fee_rate, *locktime, *timelock,
        ),
        (
            _,
            Some(AssetKey::Evm { chain_id, token }),
            ExecutionParams::Evm {
                nonce,
                max_fee_per_gas,
                max_priority_fee_per_gas,
                gas_limit,
            },
        ) => {
            ensure(w.policy.evm_chains.contains(chain_id), Error::Forbidden)?;
            let [(Destination::Evm(to), amount)] = outputs.as_slice() else {
                return Err(invalid("evm transfers have one recipient"));
            };
            evm::build(
                from,
                *chain_id,
                evm::Payment::Transfer {
                    token: *token,
                    to: *to,
                    amount: *amount,
                },
                max_fee,
                *nonce,
                *max_fee_per_gas,
                *max_priority_fee_per_gas,
                *gas_limit,
            )
        }
        (
            Intent::CancelEvmNonce { chain_id, .. },
            None,
            ExecutionParams::Evm {
                nonce,
                max_fee_per_gas,
                max_priority_fee_per_gas,
                gas_limit,
            },
        ) => {
            ensure(
                w.policy.evm_chains.contains(chain_id) && pending_nonce == Some(*nonce),
                Error::Forbidden,
            )?;
            evm::build(
                from,
                *chain_id,
                evm::Payment::Cancel,
                max_fee,
                *nonce,
                *max_fee_per_gas,
                *max_priority_fee_per_gas,
                *gas_limit,
            )
        }
        (
            _,
            Some(AssetKey::Sol { mint }),
            ExecutionParams::Solana {
                blockhash,
                compute_unit_price,
                compute_unit_limit,
                decimals,
                create_ata,
            },
        ) => sol::build(
            from,
            sol::Payment::Transfer {
                mint: *mint,
                outputs: &outputs,
            },
            max_fee,
            *blockhash,
            *compute_unit_price,
            *compute_unit_limit,
            *decimals,
            *create_ata,
        ),
        (
            Intent::InitSolNonce { .. },
            None,
            ExecutionParams::Solana {
                blockhash,
                compute_unit_price,
                compute_unit_limit,
                ..
            },
        ) => sol::build(
            from,
            sol::Payment::InitNonce,
            max_fee,
            *blockhash,
            *compute_unit_price,
            *compute_unit_limit,
            None,
            false,
        ),
        _ => Err(invalid("execution parameters do not match the chain")),
    }
}

/// The signed transaction and its ID.
pub(crate) fn assemble(
    a: &Assembly,
    sigs: &[Vec<u8>],
    cosigs: &[Option<Vec<u8>>],
) -> Result<(Vec<u8>, String)> {
    match a {
        Assembly::Bitcoin { tx, spends } => btc::assemble(tx, spends, sigs, cosigs),
        Assembly::Evm { tx, public_key } => {
            evm::assemble(tx, public_key, sigs.first().ok_or(Error::IntegrityFailed)?)
        }
        Assembly::Solana { message } => {
            sol::assemble(message, sigs.first().ok_or(Error::IntegrityFailed)?)
        }
    }
}

#[cfg(test)]
mod tests;
