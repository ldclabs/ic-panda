//! Reading the JSON parameters of the daemon methods and agent tools.

use dmsg_types::{AccountId, Error, Hash, Result};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::path::PathBuf;

/// A string.
pub(crate) fn text(params: &Value, name: &str) -> Result<String> {
    params[name]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| Error::InvalidInput(format!("missing {name}")))
}

/// A value of type `T`.
pub(crate) fn parse<T: DeserializeOwned>(params: &Value, name: &str) -> Result<T> {
    serde_json::from_value(params[name].clone())
        .map_err(|e| Error::InvalidInput(format!("{name}: {e}")))
}

/// An unsigned number that fits `T`.
pub(crate) fn number<T: TryFrom<u64>>(params: &Value, name: &str) -> Result<T> {
    params[name]
        .as_u64()
        .and_then(|n| T::try_from(n).ok())
        .ok_or_else(|| Error::InvalidInput(format!("missing or invalid {name}")))
}

/// An account ID.
pub(crate) fn account(params: &Value, name: &str) -> Result<AccountId> {
    text(params, name)?
        .parse()
        .map_err(|_| Error::InvalidInput(format!("{name}: account ID")))
}

/// A 32-byte identifier in hex.
pub(crate) fn hash(params: &Value, name: &str) -> Result<Hash> {
    hex::decode(text(params, name)?)
        .ok()
        .and_then(|b| b.try_into().ok())
        .map(Hash::new)
        .ok_or_else(|| Error::InvalidInput(format!("{name}: hex identifier")))
}

/// 32-byte identifiers in hex; absent means none.
pub(crate) fn hashes(params: &Value, name: &str) -> Result<Vec<Hash>> {
    let invalid = || Error::InvalidInput(format!("{name}: hex identifiers"));
    match &params[name] {
        Value::Null => Ok(vec![]),
        Value::Array(items) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .and_then(|s| hex::decode(s).ok())
                    .and_then(|b| b.try_into().ok())
                    .map(Hash::new)
                    .ok_or_else(invalid)
            })
            .collect(),
        _ => Err(invalid()),
    }
}

/// An absolute path: the daemon does not share the caller's directory.
pub(crate) fn absolute(params: &Value, name: &str) -> Result<PathBuf> {
    let path = PathBuf::from(text(params, name)?);
    match path.is_absolute() {
        true => Ok(path),
        false => Err(Error::InvalidInput(format!("{name} must be absolute"))),
    }
}

/// A parameter that may be absent or null.
pub(crate) fn optional<T>(
    params: &Value,
    name: &str,
    read: impl FnOnce(&Value, &str) -> Result<T>,
) -> Result<Option<T>> {
    match params[name].is_null() {
        true => Ok(None),
        false => read(params, name).map(Some),
    }
}

/// The local source of a request: the CLI or the desktop.
pub(crate) fn source(params: &Value) -> Result<&'static str> {
    match params["source"].as_str() {
        Some("gui") => Ok("gui"),
        Some("cli") | None => Ok("cli"),
        Some(_) => Err(Error::InvalidInput("source".into())),
    }
}

pub(crate) fn to_json<T: serde::Serialize + ?Sized>(value: &T) -> Value {
    serde_json::to_value(value).expect("serializable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_parameters() {
        let params = json!({"n": 256, "p": "relative/path", "h": "00", "source": "gui"});
        assert_eq!(number::<u64>(&params, "n").unwrap(), 256);
        assert!(number::<u8>(&params, "n").is_err());
        assert!(absolute(&params, "p").is_err());
        assert!(hash(&params, "h").is_err());
        assert_eq!(optional(&params, "x", number::<u8>).unwrap(), None);
        assert!(optional(&params, "n", number::<u8>).is_err());
        assert_eq!(source(&params).unwrap(), "gui");
        assert_eq!(source(&json!({})).unwrap(), "cli");
        assert!(source(&json!({"source": "web"})).is_err());
    }
}
