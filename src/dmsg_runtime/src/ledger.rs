//! Adapter for the DFINITY ICRC ledger's ICRC-3 `1xfer`/`2xfer` blocks. Assets with
//! another block schema must have a separately reviewed adapter.
use crate::call;
use candid::{Nat, Principal};
use dmsg_types::*;
use icrc_ledger_types::{
    icrc::generic_value::ICRC3Value as Value,
    icrc1::account::Account,
    icrc3::blocks::{GetBlocksRequest, GetBlocksResult},
};
use num_traits::ToPrimitive;
use std::collections::BTreeMap;

/// Serde adapter for protocol accounts. ICRC's external Account type uses an
/// unannotated array for subaccounts; the dMsg CBOR contract uses fixed bytes.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedTransfer {
    pub block: u64,
    pub from: Account,
    pub to: Account,
    pub amount: u128,
    pub fee: Option<u128>,
    /// Ledger block timestamp converted to Unix milliseconds (floor).
    pub committed_at: u64,
    pub memo: Option<Vec<u8>>,
    /// Exact sender timestamp in Unix nanoseconds, used for reconciliation.
    pub created_at_time: Option<u64>,
    pub spender: Option<Account>,
}
fn map(v: &Value) -> Result<&BTreeMap<String, Value>> {
    if let Value::Map(m) = v {
        Ok(m)
    } else {
        Err(Error::IntegrityFailed)
    }
}
fn field<'a>(m: &'a BTreeMap<String, Value>, s: &str) -> Result<&'a Value> {
    m.get(s).ok_or(Error::IntegrityFailed)
}
fn amount(v: &Value) -> Result<u128> {
    match v {
        Value::Nat(n) => n.0.to_u128().ok_or(Error::IntegrityFailed),
        Value::Int(n) => n.0.to_u128().ok_or(Error::IntegrityFailed),
        _ => Err(Error::IntegrityFailed),
    }
}
fn uint(v: &Value) -> Result<u64> {
    amount(v)?.try_into().map_err(|_| Error::IntegrityFailed)
}
fn text_is(v: Option<&Value>, s: &str) -> bool {
    matches!(v,Some(Value::Text(t)) if t==s)
}
fn blob(v: &Value) -> Result<Vec<u8>> {
    if let Value::Blob(b) = v {
        Ok(b.to_vec())
    } else {
        Err(Error::IntegrityFailed)
    }
}
fn account(v: &Value) -> Result<Account> {
    let Value::Array(a) = v else {
        return Err(Error::IntegrityFailed);
    };
    ensure(a.len() == 1 || a.len() == 2, Error::IntegrityFailed)?;
    let p = blob(&a[0])?;
    ensure(!p.is_empty() && p.len() <= 29, Error::IntegrityFailed)?;
    let owner = Principal::from_slice(&p);
    let subaccount = if a.len() == 2 {
        Some(
            blob(&a[1])?
                .try_into()
                .map_err(|_| Error::IntegrityFailed)?,
        )
    } else {
        None
    };
    Ok(Account { owner, subaccount })
}
pub fn parse_transfer(block: u64, value: &Value) -> Result<VerifiedTransfer> {
    let b = map(value)?;
    let tx = map(field(b, "tx")?)?;
    // btype is authoritative when present; tx.op is only the legacy fallback.
    let is_transfer = match b.get("btype") {
        Some(kind) => text_is(Some(kind), "1xfer") || text_is(Some(kind), "2xfer"),
        None => text_is(tx.get("op"), "xfer"),
    };
    ensure(is_transfer, Error::UnsupportedProtocol)?;
    Ok(VerifiedTransfer {
        block,
        from: account(field(tx, "from")?)?,
        to: account(field(tx, "to")?)?,
        amount: amount(field(tx, "amt")?)?,
        fee: b.get("fee").or(tx.get("fee")).map(amount).transpose()?,
        committed_at: nanos_to_millis(uint(field(b, "ts")?)?),
        memo: tx.get("memo").map(blob).transpose()?,
        created_at_time: tx.get("ts").map(uint).transpose()?,
        spender: tx.get("spender").map(account).transpose()?,
    })
}

/// Inter-canister calls authenticate the ledger reply; no caller-supplied block
/// or worker timestamp is accepted. Follow only callbacks returned by that
/// ledger, keeping the original requested index and a bounded redirect depth.
pub async fn read_transfer(ledger: Principal, index: u64) -> Result<VerifiedTransfer> {
    let requests = vec![GetBlocksRequest {
        start: Nat::from(index),
        length: Nat::from(1u8),
    }];
    let mut target = ledger;
    let mut method = "icrc3_get_blocks".to_string();
    for _ in 0..4 {
        let response: GetBlocksResult = call(target, &method, (requests.clone(),)).await?;
        ensure(
            response.blocks.len() <= 1 && response.archived_blocks.len() <= 4,
            Error::IntegrityFailed,
        )?;
        if let Some(b) = response.blocks.first() {
            ensure(b.id == index, Error::IntegrityFailed)?;
            return parse_transfer(index, &b.block);
        }
        let archive = response
            .archived_blocks
            .iter()
            .find(|a| {
                a.args.iter().any(|r| {
                    r.as_start_and_length()
                        .is_ok_and(|(start, len)| index >= start && index - start < len)
                })
            })
            .ok_or(Error::NotFound)?;
        ensure(
            archive.callback.method == "icrc3_get_blocks",
            Error::UnsupportedProtocol,
        )?;
        target = archive.callback.canister_id;
        method = archive.callback.method.clone();
    }
    Err(Error::Unavailable("archive redirect limit".into()))
}
pub fn block_index(n: Nat) -> Result<u64> {
    n.0.to_u64().ok_or(Error::IntegrityFailed)
}
pub fn token_amount(n: Nat) -> Result<u128> {
    n.0.to_u128().ok_or(Error::IntegrityFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn n(v: u64) -> Value {
        Value::Nat(v.into())
    }
    fn a(p: u8) -> Value {
        Value::Array(vec![Value::Blob(vec![p].into())])
    }
    #[test]
    fn committed_time_is_not_sender_time() {
        let mut tx = BTreeMap::from([
            ("op".into(), Value::Text("xfer".into())),
            ("from".into(), a(1)),
            ("to".into(), a(2)),
            ("amt".into(), n(100)),
            ("ts".into(), n(1)),
        ]);
        let mut b = BTreeMap::from([
            ("ts".into(), n(999_999_999)),
            ("tx".into(), Value::Map(tx.clone())),
        ]);
        assert_eq!(
            parse_transfer(7, &Value::Map(b.clone()))
                .unwrap()
                .committed_at,
            999
        );
        b.remove("ts");
        assert!(parse_transfer(7, &Value::Map(b.clone())).is_err());
        tx.insert("op".into(), Value::Text("mint".into()));
        b.insert("ts".into(), n(999_999_999));
        b.insert("tx".into(), Value::Map(tx));
        assert!(parse_transfer(7, &Value::Map(b)).is_err());
    }
    #[test]
    fn transfer_from_and_legacy_blocks_use_the_authoritative_type() {
        let tx = Value::Map(BTreeMap::from([
            ("from".into(), a(1)),
            ("to".into(), a(2)),
            ("spender".into(), a(3)),
            ("amt".into(), n(100)),
        ]));
        let mut block = BTreeMap::from([
            ("ts".into(), n(999_999_999)),
            ("btype".into(), Value::Text("2xfer".into())),
            ("tx".into(), tx),
        ]);
        let parsed = parse_transfer(1, &Value::Map(block.clone())).unwrap();
        assert_eq!(parsed.spender.unwrap().owner, Principal::from_slice(&[3]));
        let Value::Map(tx) = block.get_mut("tx").unwrap() else {
            unreachable!()
        };
        tx.insert("op".into(), Value::Text("ignored-legacy-op".into()));
        assert!(parse_transfer(1, &Value::Map(block.clone())).is_ok());
        block.insert("btype".into(), Value::Text("2approve".into()));
        assert!(parse_transfer(1, &Value::Map(block.clone())).is_err());
        block.remove("btype");
        let Value::Map(tx) = block.get_mut("tx").unwrap() else {
            unreachable!()
        };
        tx.insert("op".into(), Value::Text("xfer".into()));
        assert!(parse_transfer(1, &Value::Map(block)).is_ok());
    }
}
