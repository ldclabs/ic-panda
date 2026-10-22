//! Personal legacy archives: pairing, import, offline reading, deletion
//! and frozen-history comparison.

use crate::daemon::Daemon;
use crate::params::{absolute, text};
use dmsg_types::{Error, Result};
use serde_json::{Value, json};

/// The methods this module serves.
pub(crate) const METHODS: [&str; 10] = [
    "legacy/pair",
    "legacy/pairs",
    "legacy/cancel",
    "legacy/import",
    "legacy/list",
    "legacy/channels",
    "legacy/report",
    "legacy/delete_channel",
    "legacy/file",
    "legacy/frozen",
];

/// The events a method's success announces.
pub(crate) fn events_of(method: &str) -> &'static [&'static str] {
    match method {
        "legacy/pair" | "legacy/cancel" | "legacy/import" | "legacy/frozen" => &["legacy/changed"],
        "legacy/delete_channel" => &["legacy/changed", "sync/changed"],
        _ => &[],
    }
}

impl Daemon {
    /// Run one of [`METHODS`].
    pub(crate) async fn call_legacy(&self, method: &str, params: &Value) -> Result<Value> {
        if method == "legacy/frozen" {
            // The frozen proof verifier still reads the IC under the client.
            return self
                .under_lock(async |client| {
                    client
                        .legacy_frozen(&text(params, "id")?, &absolute(params, "path")?)
                        .await
                })
                .await;
        }
        let mut client = self.client().lock().await;
        Ok(match method {
            "legacy/pair" => client.legacy_pair(&text(params, "origin")?)?,
            "legacy/pairs" => json!(client.legacy_pairs()?),
            "legacy/cancel" => {
                client.legacy_cancel(&text(params, "nonce")?)?;
                json!({"cancelled": true})
            }
            "legacy/import" => client.legacy_import(
                &absolute(params, "path")?,
                &text(params, "nonce")?,
                &text(params, "fingerprint")?,
                &text(params, "principal")?,
            )?,
            "legacy/list" => json!(client.legacy_list()?),
            "legacy/channels" => client.legacy_channels()?,
            "legacy/report" => client.legacy_channel_report(
                &text(params, "id")?,
                params["channel"].as_str(),
                params["offset"].as_u64().unwrap_or(0) as usize,
                params["limit"].as_u64().unwrap_or(100) as usize,
            )?,
            "legacy/delete_channel" => {
                client.legacy_delete_channel(&text(params, "id")?, &text(params, "channel")?)?;
                json!({"deleted": true})
            }
            "legacy/file" => client.legacy_file(
                &text(params, "id")?,
                &text(params, "source")?,
                &absolute(params, "path")?,
            )?,
            _ => return Err(Error::NotFound),
        })
    }
}
