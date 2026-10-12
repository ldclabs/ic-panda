//! The daemon methods of legacy migration: personal archives
//! (`dmsg-legacy-pair/2`, import, offline reading, frozen comparison) and
//! the official successor of shared old channels. Approvals by an old
//! identity happen on the old site; this daemon only checks them.

use crate::daemon::Daemon;
use dmsg_client::legacy_proof::{Approval, MemberClaim};
use dmsg_client::shared_migration::{Draft, SharedView, SourceFile};
use dmsg_types::{Error, Result};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::path::PathBuf;

/// The methods this module serves.
pub(crate) const METHODS: [&str; 22] = [
    "legacy/pair",
    "legacy/pairs",
    "legacy/cancel",
    "legacy/import",
    "legacy/list",
    "legacy/report",
    "legacy/file",
    "legacy/frozen",
    "legacy/shared_source",
    "legacy/shared_prepare",
    "legacy/shared_check",
    "legacy/shared_request",
    "legacy/shared_propose",
    "legacy/shared_consent",
    "legacy/shared_view",
    "legacy/shared_commit",
    "legacy/shared_activate",
    "legacy/shared_claim_request",
    "legacy/shared_claim",
    "legacy/shared_joined",
    "legacy/shared_grant",
    "legacy/shared_receive",
];

/// The events a method's success announces.
pub(crate) fn events_of(method: &str) -> &'static [&'static str] {
    match method {
        "legacy/pair" | "legacy/cancel" | "legacy/import" | "legacy/frozen" => &["legacy/changed"],
        "legacy/shared_receive" => &["legacy/changed", "channel/changed"],
        "legacy/shared_activate" | "legacy/shared_joined" | "legacy/shared_grant" => {
            &["channel/changed"]
        }
        _ => &[],
    }
}

fn text(params: &Value, name: &str) -> Result<String> {
    params[name]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| Error::InvalidInput(format!("missing {name}")))
}

fn parse<T: DeserializeOwned>(params: &Value, name: &str) -> Result<T> {
    serde_json::from_value(params[name].clone())
        .map_err(|e| Error::InvalidInput(format!("{name}: {e}")))
}

fn absolute(params: &Value, name: &str) -> Result<PathBuf> {
    let path = PathBuf::from(text(params, name)?);
    match path.is_absolute() {
        true => Ok(path),
        false => Err(Error::InvalidInput(format!("{name} must be absolute"))),
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("serializable")
}

impl Daemon {
    /// The directory record a request names: a `view` handed over by another
    /// participant, or the relay's record of `key`.
    async fn shared_view_of(&self, params: &Value) -> Result<SharedView> {
        let mut client = self.client().lock().await;
        match params["view"].is_object() {
            true => client.shared_check_view(parse(params, "view")?).await,
            false => client.shared_view(&text(params, "key")?).await,
        }
    }

    /// Run one of [`METHODS`].
    pub(crate) async fn call_legacy(&self, method: &str, params: &Value) -> Result<Value> {
        if matches!(
            method,
            "legacy/shared_claim_request" | "legacy/shared_claim"
        ) {
            let view = self.shared_view_of(params).await?;
            let mut client = self.client().lock().await;
            return match method {
                "legacy/shared_claim_request" => {
                    client
                        .shared_claim_request(view, &text(params, "member")?)
                        .await
                }
                _ => {
                    let claim: MemberClaim = parse(params, "claim")?;
                    let approval: Approval = parse(params, "approval")?;
                    Ok(to_json(
                        &client.shared_claim(view, &claim, &approval).await?,
                    ))
                }
            };
        }
        let mut client = self.client().lock().await;
        match method {
            "legacy/pair" => client.legacy_pair(&text(params, "origin")?),
            "legacy/pairs" => Ok(json!(client.legacy_pairs()?)),
            "legacy/cancel" => {
                client.legacy_cancel(&text(params, "nonce")?)?;
                Ok(json!({"cancelled": true}))
            }
            "legacy/import" => client.legacy_import(
                &absolute(params, "path")?,
                &text(params, "nonce")?,
                &text(params, "fingerprint")?,
                &text(params, "principal")?,
            ),
            "legacy/list" => Ok(json!(client.legacy_list()?)),
            "legacy/report" => client.legacy_report(
                &text(params, "id")?,
                params["offset"].as_u64().unwrap_or(0) as usize,
                params["limit"].as_u64().unwrap_or(100) as usize,
            ),
            "legacy/file" => client.legacy_file(
                &text(params, "id")?,
                &text(params, "source")?,
                &absolute(params, "path")?,
            ),
            "legacy/frozen" => {
                client
                    .legacy_frozen(&text(params, "id")?, &absolute(params, "path")?)
                    .await
            }
            "legacy/shared_source" => {
                let file: SourceFile = parse(params, "source")?;
                Ok(to_json(&client.shared_source(&file).await?))
            }
            "legacy/shared_prepare" => {
                let file: SourceFile = parse(params, "source")?;
                let version = params["version"].as_u64().unwrap_or(1);
                Ok(to_json(&client.shared_prepare(&file, version).await?))
            }
            "legacy/shared_check" => {
                let draft: Draft = parse(params, "draft")?;
                Ok(to_json(&client.shared_check_draft(&draft).await?))
            }
            "legacy/shared_request" => {
                let draft: Draft = parse(params, "draft")?;
                client
                    .shared_manager_request(&draft, &text(params, "manager")?)
                    .await
            }
            "legacy/shared_propose" | "legacy/shared_consent" => {
                let draft: Draft = parse(params, "draft")?;
                let approval: Approval = parse(params, "approval")?;
                let manager = text(params, "manager")?;
                let view = match method {
                    "legacy/shared_propose" => {
                        client.shared_propose(&draft, &manager, &approval).await?
                    }
                    _ => client.shared_consent(&draft, &manager, &approval).await?,
                };
                Ok(to_json(&view))
            }
            "legacy/shared_view" => {
                drop(client);
                Ok(to_json(&self.shared_view_of(params).await?))
            }
            "legacy/shared_commit" => {
                Ok(to_json(&client.shared_commit(&text(params, "key")?).await?))
            }
            "legacy/shared_activate" => Ok(to_json(
                &client.shared_activate(&text(params, "key")?).await?,
            )),
            "legacy/shared_joined" => {
                Ok(to_json(&client.shared_joined(&text(params, "key")?).await?))
            }
            "legacy/shared_grant" => {
                let view = client
                    .shared_grant(
                        &text(params, "key")?,
                        &text(params, "archive")?,
                        &text(params, "member")?,
                    )
                    .await?;
                Ok(to_json(&view))
            }
            "legacy/shared_receive" => {
                client
                    .shared_receive(&text(params, "key")?, &text(params, "grant")?)
                    .await
            }
            _ => Err(Error::InvalidInput(format!("unknown method {method}"))),
        }
    }
}
