//! EIP-1559 transactions: native transfers, ERC-20 `transfer` and nonce
//! cancellations.
use super::Built;
use crate::store::SignSpec;
use dmsg_protocol::wallet::*;
use dmsg_types::{wallet::*, *};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use sha3::{Digest, Keccak256};

/// Gas of a plain value transfer.
pub(crate) const TRANSFER_GAS: u64 = 21_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Eip1559 {
    pub(crate) chain_id: u64,
    pub(crate) nonce: u64,
    pub(crate) max_priority_fee_per_gas: u128,
    pub(crate) max_fee_per_gas: u128,
    pub(crate) gas_limit: u64,
    pub(crate) to: Hash20,
    pub(crate) value: u128,
    pub(crate) data: ByteBuf,
}

fn rlp_bytes(b: &[u8]) -> Vec<u8> {
    match b.len() {
        1 if b[0] < 0x80 => b.to_vec(),
        n if n <= 55 => [vec![0x80 + n as u8], b.to_vec()].concat(),
        n => {
            let len = uint_bytes(n as u128);
            [vec![0xb7 + len.len() as u8], len, b.to_vec()].concat()
        }
    }
}

fn uint_bytes(v: u128) -> Vec<u8> {
    v.to_be_bytes()
        .iter()
        .copied()
        .skip_while(|b| *b == 0)
        .collect()
}

fn rlp_uint(v: u128) -> Vec<u8> {
    rlp_bytes(&uint_bytes(v))
}

fn rlp_list(items: &[Vec<u8>]) -> Vec<u8> {
    let body = items.concat();
    if body.len() <= 55 {
        [vec![0xc0 + body.len() as u8], body].concat()
    } else {
        let len = uint_bytes(body.len() as u128);
        [vec![0xf7 + len.len() as u8], len, body].concat()
    }
}

impl Eip1559 {
    fn fields(&self) -> Vec<Vec<u8>> {
        vec![
            rlp_uint(u128::from(self.chain_id)),
            rlp_uint(u128::from(self.nonce)),
            rlp_uint(self.max_priority_fee_per_gas),
            rlp_uint(self.max_fee_per_gas),
            rlp_uint(u128::from(self.gas_limit)),
            rlp_bytes(&self.to[..]),
            rlp_uint(self.value),
            rlp_bytes(&self.data),
            rlp_list(&[]),
        ]
    }

    /// `0x02 || rlp(fields)`, whose Keccak-256 is signed.
    pub(crate) fn payload(&self) -> Vec<u8> {
        [vec![0x02], rlp_list(&self.fields())].concat()
    }

    /// The signed transaction and its hash.
    pub(crate) fn signed(&self, y_odd: bool, r: &[u8], s: &[u8]) -> (Vec<u8>, String) {
        let strip = |v: &[u8]| {
            v.iter()
                .copied()
                .skip_while(|b| *b == 0)
                .collect::<Vec<u8>>()
        };
        let mut fields = self.fields();
        fields.push(rlp_uint(u128::from(y_odd)));
        fields.push(rlp_bytes(&strip(r)));
        fields.push(rlp_bytes(&strip(s)));
        let raw = [vec![0x02], rlp_list(&fields)].concat();
        let hash = Keccak256::digest(&raw);
        let txid = format!(
            "0x{}",
            hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
        (raw, txid)
    }
}

/// `transfer(address,uint256)` call data.
pub(crate) fn erc20_transfer(to: &Hash20, amount: u128) -> Vec<u8> {
    let mut data = vec![0xa9, 0x05, 0x9c, 0xbb];
    data.extend_from_slice(&[0; 12]);
    data.extend_from_slice(&to[..]);
    data.extend_from_slice(&[0; 16]);
    data.extend_from_slice(&amount.to_be_bytes());
    data
}

/// What to send: recipient, value and data.
pub(crate) enum Payment {
    Transfer {
        token: Option<Hash20>,
        to: Hash20,
        amount: u128,
    },
    /// Zero value to itself, replacing a pending nonce.
    Cancel,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build(
    from: &Address,
    chain_id: u64,
    payment: Payment,
    max_fee: u128,
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
    gas_limit: u64,
) -> Result<Built> {
    ensure_valid(from.chain == Chain::Evm, "evm address")?;
    ensure_valid(
        max_priority_fee_per_gas <= max_fee_per_gas
            && max_fee_per_gas > 0
            && (TRANSFER_GAS..=1_000_000).contains(&gas_limit),
        "evm fee parameters",
    )?;
    let fee = u128::from(gas_limit)
        .checked_mul(max_fee_per_gas)
        .ok_or(Error::FeeBlocked)?;
    ensure(fee <= max_fee, Error::FeeBlocked)?;
    let own = Hash20::new(evm_address(&from.public_key)?);
    let (to, value, data, asset, outputs) = match payment {
        Payment::Transfer {
            token: None,
            to,
            amount,
        } => (to, amount, vec![], "ETH".to_string(), vec![(to, amount)]),
        Payment::Transfer {
            token: Some(token),
            to,
            amount,
        } => (
            token,
            0,
            erc20_transfer(&to, amount),
            format!("ERC20:{}", eip55(&token.into_array())),
            vec![(to, amount)],
        ),
        Payment::Cancel => (own, 0, vec![], "ETH".to_string(), vec![(own, 0)]),
    };
    let tx = Eip1559 {
        chain_id,
        nonce,
        max_priority_fee_per_gas,
        max_fee_per_gas,
        gas_limit,
        to,
        value,
        data: data.into(),
    };
    let payload = tx.payload();
    let digest: [u8; 32] = Keccak256::digest(&payload).into();
    Ok(Built {
        unsigned: payload,
        digests: vec![Hash::new(digest)],
        signs: vec![SignSpec::Ecdsa {
            path: crate::keys::path_bytes(&derivation_path(
                Chain::Evm,
                &AddressKind::ChainKey,
                from.id,
            )?),
        }],
        summary: DecodedSummary {
            chain: format!("evm:{chain_id}"),
            from: from.text.clone(),
            asset,
            outputs: outputs
                .into_iter()
                .map(|(to, amount)| SummaryOutput {
                    to: eip55(&to.into_array()),
                    amount,
                })
                .collect(),
            change: None,
            fee,
            nonce: nonce.to_string(),
        },
        assembly: super::Assembly::Evm {
            tx,
            public_key: from.public_key.clone(),
        },
    })
}

/// Normalize the threshold signature to low s and find its recovery parity.
pub(crate) fn assemble(tx: &Eip1559, public_key: &[u8], sig: &[u8]) -> Result<(Vec<u8>, String)> {
    let sig = k256::ecdsa::Signature::from_slice(sig).map_err(|_| Error::IntegrityFailed)?;
    let sig = sig.normalize_s();
    let key = k256::ecdsa::VerifyingKey::from_sec1_bytes(public_key)
        .map_err(|_| Error::IntegrityFailed)?;
    let digest = Keccak256::digest(tx.payload());
    let id = k256::ecdsa::RecoveryId::trial_recovery_from_prehash(&key, &digest, &sig)
        .map_err(|_| Error::IntegrityFailed)?;
    let bytes = sig.to_bytes();
    Ok(tx.signed(id.is_y_odd(), &bytes[..32], &bytes[32..]))
}
