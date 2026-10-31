//! Wallet state frozen into an approval, including the pre-1.2 member shape.
//! Keep its original encoding: the job's approved digest covers that snapshot.

use crate::codec::{canonical, decode};
use crate::workspace::MAX_PRIVATE;
use cbor2::Value;
use dmsg_types::{Error, Result, ensure, wallet::WalletView};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser};

/// A wallet approval's immutable context and its validated display projection.
/// This is local journal compatibility, not a substitute for fresh wallet state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletSnapshot {
    encoded: Vec<u8>,
    view: WalletView,
}

impl From<WalletView> for WalletSnapshot {
    fn from(view: WalletView) -> Self {
        Self {
            encoded: canonical(&view),
            view,
        }
    }
}

impl WalletSnapshot {
    /// The state shown when the request was made; never execution authority.
    pub fn view(&self) -> &WalletView {
        &self.view
    }
}

impl Serialize for WalletSnapshot {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let value: Value = cbor2::from_slice(&self.encoded).map_err(ser::Error::custom)?;
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WalletSnapshot {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        let encoded = cbor2::to_canonical_vec(&value).map_err(de::Error::custom)?;
        project_members(&mut value).map_err(de::Error::custom)?;
        // Retain the normal strict typed check: unknown or changed fields,
        // invalid Principals, duplicate keys and malformed bindings still fail.
        let projected = cbor2::to_canonical_vec(&value).map_err(de::Error::custom)?;
        let view = decode(&projected, MAX_PRIVATE).map_err(de::Error::custom)?;
        Ok(Self { encoded, view })
    }
}

fn project_members(value: &mut Value) -> Result<()> {
    let Value::Map(fields) = value else {
        return Err(Error::IntegrityFailed);
    };
    let members = fields
        .iter_mut()
        .find(|(key, _)| key == &Value::Text("members".into()))
        .map(|(_, value)| value)
        .ok_or(Error::IntegrityFailed)?;
    let Value::Array(members) = members else {
        return Err(Error::IntegrityFailed);
    };
    for member in members {
        let Value::Map(fields) = member else {
            return Err(Error::IntegrityFailed);
        };
        let Some(at) = fields
            .iter()
            .position(|(key, _)| key == &Value::Text("principals".into()))
        else {
            continue;
        };
        ensure(
            !fields
                .iter()
                .any(|(key, _)| key == &Value::Text("bindings".into())),
            Error::IntegrityFailed,
        )?;
        let (key, principals) = &mut fields[at];
        let Value::Array(values) = principals else {
            return Err(Error::IntegrityFailed);
        };
        // Old snapshots recorded neither device IDs nor binding times.
        // The absent device and zero timestamp do not assert either fact.
        *principals = Value::Array(
            values
                .iter()
                .map(|principal| {
                    Value::Map(vec![
                        (Value::Text("principal".into()), principal.clone()),
                        (Value::Text("device".into()), Value::Null),
                        (Value::Text("bound_at_ms".into()), Value::Integer(0.into())),
                    ])
                })
                .collect(),
        );
        *key = Value::Text("bindings".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
