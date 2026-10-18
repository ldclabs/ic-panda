//! The daemon methods of the vault (`vault/*`): items and their fields,
//! history, trash and conflict copies; one-time codes, SSH keys, the
//! security check and imports. Hidden values leave only through
//! `vault/reveal` (one field) or `vault/show` with `reveal`.

use crate::daemon::Daemon;
use crate::params::{absolute, optional, parse, text, to_json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use dmsg_client::vault::{
    Changes, Conflict, FIELD_TYPES, Filter, Keep, Side, TYPES, VaultEntry, otp, primary_secret,
    ssh, template,
};
use dmsg_types::{Error, Result};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// The methods this module serves.
pub(crate) const METHODS: [&str; 20] = [
    "vault/templates",
    "vault/create",
    "vault/edit",
    "vault/duplicate",
    "vault/delete",
    "vault/restore",
    "vault/tags",
    "vault/list",
    "vault/show",
    "vault/reveal",
    "vault/history",
    "vault/revert",
    "vault/conflicts",
    "vault/resolve",
    "vault/otp",
    "vault/otp_scan",
    "vault/ssh_keygen",
    "vault/health",
    "vault/import_preview",
    "vault/import",
];

/// The events a method's success announces.
pub(crate) fn events_of(method: &str) -> &'static [&'static str] {
    match method {
        "vault/create" | "vault/edit" | "vault/duplicate" | "vault/delete" | "vault/restore"
        | "vault/tags" | "vault/revert" | "vault/resolve" | "vault/import" => &["vault/changed"],
        _ => &[],
    }
}

fn masked_all(entries: Vec<VaultEntry>) -> Value {
    to_json(
        &entries
            .into_iter()
            .map(|e| e.masked(false))
            .collect::<Vec<_>>(),
    )
}

/// `ids`, else the single `id`.
fn ids(params: &Value) -> Result<Vec<String>> {
    match params["ids"].is_null() {
        true => Ok(vec![text(params, "id")?]),
        false => parse(params, "ids"),
    }
}

/// How `vault/resolve` settles a copy: `keep` is `current`, `theirs` or
/// `merge` with `picks` (part → side) and optional merged `notes`.
fn keep(params: &Value) -> Result<Keep> {
    match params["keep"].as_str() {
        Some("current") => Ok(Keep::Current),
        Some("theirs") => Ok(Keep::Theirs),
        Some("merge") => Ok(Keep::Merge {
            picks: optional(params, "picks", parse::<BTreeMap<String, Side>>)?.unwrap_or_default(),
            notes: optional(params, "notes", text)?,
        }),
        _ => Err(Error::InvalidInput("keep: current, theirs or merge".into())),
    }
}

/// The field `vault/otp` reads: the one named, else the item's first
/// one-time password.
fn otp_field(entry: &VaultEntry, params: &Value) -> Result<String> {
    match params["field"].as_str() {
        Some(field) => Ok(field.into()),
        None => entry
            .item
            .fields
            .iter()
            .find(|f| f.kind == "otp")
            .map(|f| f.id.clone())
            .ok_or(Error::NotFound),
    }
}

impl Daemon {
    /// Run one of [`METHODS`].
    pub(crate) async fn call_vault(&self, method: &str, params: &Value) -> Result<Value> {
        if method == "vault/templates" {
            let mut templates = Map::new();
            for kind in TYPES {
                let fields: Vec<Value> = template(kind)
                    .iter()
                    .map(|(id, kind)| json!({"id": id, "type": kind}))
                    .collect();
                templates.insert(
                    kind.into(),
                    json!({"fields": fields, "secret": primary_secret(kind)}),
                );
            }
            return Ok(json!({"types": TYPES, "field_types": FIELD_TYPES, "templates": templates}));
        }
        if method == "vault/ssh_keygen" {
            let comment = optional(params, "comment", text)?.unwrap_or_default();
            return Ok(to_json(&ssh::generate(&comment)));
        }
        if method == "vault/otp_scan" {
            let image = match optional(params, "image", text)? {
                Some(image) => STANDARD
                    .decode(image)
                    .map_err(|_| Error::InvalidInput("image".into()))?,
                None => std::fs::read(absolute(params, "path")?)
                    .map_err(|e| Error::InvalidInput(format!("path: {e}")))?,
            };
            let uri = otp::scan(&image)?;
            otp::parse(&uri)?;
            return Ok(json!({"uri": uri}));
        }
        let mut client = self.client().lock().await;
        let reveal = params["reveal"] == true;
        match method {
            "vault/create" => {
                let kind = text(params, "type")?;
                let changes: Changes = parse(params, "changes")?;
                Ok(to_json(
                    &client.vault_create(&kind, &changes)?.masked(false),
                ))
            }
            "vault/edit" => {
                let changes: Changes = parse(params, "changes")?;
                let entry =
                    client.vault_edit(&text(params, "id")?, params["base"].as_str(), &changes)?;
                Ok(to_json(&entry.masked(false)))
            }
            "vault/duplicate" => {
                let entry =
                    client.vault_duplicate(&text(params, "id")?, &text(params, "title")?)?;
                Ok(to_json(&entry.masked(false)))
            }
            "vault/delete" => {
                let mut deleted = Vec::new();
                for id in ids(params)? {
                    deleted.push(client.vault_delete(&id)?);
                }
                Ok(masked_all(deleted))
            }
            "vault/restore" => {
                let entry = client.vault_restore(&text(params, "id")?)?;
                Ok(to_json(&entry.masked(false)))
            }
            "vault/tags" => {
                let changes = Changes {
                    add_tags: optional(params, "add", parse)?,
                    remove_tags: optional(params, "remove", parse)?,
                    ..Changes::default()
                };
                let mut changed = Vec::new();
                for id in ids(params)? {
                    changed.push(client.vault_edit(&id, None, &changes)?);
                }
                Ok(masked_all(changed))
            }
            "vault/list" => {
                let filter = Filter {
                    trash: params["trash"] == true,
                    query: params["query"].as_str().map(str::to_string),
                };
                Ok(masked_all(client.vault_list(&filter)?))
            }
            "vault/show" => {
                let id = text(params, "id")?;
                let entry = match params["revision"].as_str() {
                    Some(revision) => client.vault_version(&id, revision)?,
                    None => client.vault_show(&id)?,
                };
                Ok(to_json(&entry.masked(reveal)))
            }
            "vault/reveal" => {
                let id = text(params, "id")?;
                let entry = match params["revision"].as_str() {
                    Some(revision) => client.vault_version(&id, revision)?,
                    None => client.vault_show(&id)?,
                };
                let field = match params["field"].as_str() {
                    Some(field) => field.to_string(),
                    None => primary_secret(&entry.item.kind)
                        .ok_or(Error::NotFound)?
                        .to_string(),
                };
                let found = entry.item.field(&field).ok_or(Error::NotFound)?;
                Ok(json!({"field": field, "type": found.kind, "value": found.value}))
            }
            "vault/history" => Ok(to_json(&client.vault_history(&text(params, "id")?)?)),
            "vault/revert" => {
                let entry =
                    client.vault_revert(&text(params, "id")?, &text(params, "revision")?)?;
                Ok(to_json(&entry.masked(false)))
            }
            "vault/conflicts" => Ok(to_json(
                &client
                    .vault_conflicts()?
                    .into_iter()
                    .map(|c| c.masked(false))
                    .collect::<Vec<Conflict>>(),
            )),
            "vault/resolve" => {
                let entry = client.vault_resolve(&text(params, "key")?, &keep(params)?)?;
                Ok(to_json(&entry.masked(false)))
            }
            "vault/otp" => {
                let entry = client.vault_show(&text(params, "id")?)?;
                let field = otp_field(&entry, params)?;
                let value = entry
                    .item
                    .field(&field)
                    .filter(|f| f.kind == "otp")
                    .ok_or(Error::NotFound)?;
                let code = otp::parse(&value.value)?.code(dmsg_client::now_ms());
                let mut reply = to_json(&code);
                reply["field"] = field.into();
                Ok(reply)
            }
            "vault/health" => Ok(to_json(&client.vault_health()?)),
            "vault/import_preview" => Ok(to_json(
                &client.vault_import_preview(&absolute(params, "path")?)?,
            )),
            "vault/import" => {
                let picks: Vec<usize> = parse(params, "picks")?;
                let ids = client.vault_import(
                    &absolute(params, "path")?,
                    &text(params, "digest")?,
                    &picks,
                )?;
                Ok(json!({"ids": ids}))
            }
            _ => Err(Error::InvalidInput(format!("unknown method {method}"))),
        }
    }
}
