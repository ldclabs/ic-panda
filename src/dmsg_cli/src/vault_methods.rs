//! The daemon methods of the vault (`vault/*`): items and their fields,
//! history, trash and conflict copies; one-time codes, SSH keys, the
//! security check and imports. Hidden values leave only through
//! `vault/reveal` (one field) or `vault/show` with `reveal`.

use crate::daemon::Daemon;
use crate::params::{absolute, optional, parse, text, to_json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use dmsg_client::hold::Hold;
use dmsg_client::vault::{
    Changes, Conflict, FIELD_TYPES, Filter, Keep, Side, TYPES, VaultEntry, import, otp,
    primary_secret, reading, ssh, template,
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
        let h = self.hold();
        match method {
            "vault/list" => {
                let filter = Filter {
                    trash: params["trash"] == true,
                    query: params["query"].as_str().map(str::to_string),
                };
                return Ok(masked_all(reading::list(&h, &filter).await?));
            }
            "vault/health" => return Ok(to_json(&reading::health(&h).await?)),
            "vault/import_preview" | "vault/import" => {
                let session = h.with(|c| reading::Session::start(c)).await?;
                let path = absolute(params, "path")?;
                // File I/O, archive decoding and parsing do not hold the
                // client or occupy an async executor thread.
                let imported = tokio::task::spawn_blocking(move || import::read(&path))
                    .await
                    .map_err(|e| Error::Unavailable(e.to_string()))??;
                h.with(|c| session.check(c)).await?;
                if method == "vault/import_preview" {
                    let entries = reading::list(&h, &Filter::default()).await?;
                    let preview = imported.preview(&entries);
                    h.with(|c| session.check(c)).await?;
                    return Ok(to_json(&preview));
                }
                let digest = text(params, "digest")?;
                let picks = parse::<Vec<usize>>(params, "picks")?;
                let attachments =
                    optional(params, "attachments", parse::<Vec<String>>)?.unwrap_or_default();
                let folder = optional(params, "folder", text)?.unwrap_or_else(|| "root".into());
                use sha2::{Digest, Sha256};
                let receipt = hex::encode(Sha256::digest(dmsg_client::codec::canonical(&(
                    "dmsg/external-import/1",
                    &digest,
                    &picks,
                    &attachments,
                    &folder,
                ))));
                if let Some(ids) = h
                    .with(|c| {
                        session.check(c)?;
                        c.secret_import_receipt(&receipt)
                    })
                    .await?
                {
                    return Ok(
                        json!({"ids":ids,"attachments":attachments.len(),"files_local_only":true,"duplicate":true}),
                    );
                }
                let (send, mut receive) = tokio::sync::mpsc::channel(1);
                let worker = tokio::task::spawn_blocking(move || {
                    imported.stream_selected(&digest, &picks, &attachments, |event| {
                        send.blocking_send(event).map_err(|_| Error::Expired)
                    })
                });
                let mut staged = Vec::new();
                let mut links = Vec::new();
                let mut current = None;
                while let Some(event) = receive.recv().await {
                    use import::AttachmentEvent;
                    match event {
                        AttachmentEvent::Start {
                            item_index,
                            name,
                            size,
                        } => {
                            if current.is_some() {
                                return Err(Error::IntegrityFailed);
                            }
                            let file = h
                                .with(|c| {
                                    session.check(c)?;
                                    c.secret_stage_start(&name, "application/octet-stream", size)
                                })
                                .await?;
                            links.push((item_index, file.id().to_owned()));
                            current = Some(file);
                        }
                        AttachmentEvent::Chunk(bytes) => {
                            let file = current.as_mut().ok_or(Error::IntegrityFailed)?;
                            h.with(|c| {
                                session.check(c)?;
                                c.secret_stage_part(file, &bytes)
                            })
                            .await?;
                        }
                        AttachmentEvent::End => {
                            let mut file = current.take().ok_or(Error::IntegrityFailed)?;
                            h.with(|c| {
                                session.check(c)?;
                                c.secret_stage_finish(&mut file, None)
                            })
                            .await?;
                            staged.push(file);
                        }
                    }
                }
                let mut items = worker
                    .await
                    .map_err(|_| Error::Unavailable("import worker".into()))??;
                dmsg_types::ensure(current.is_none(), Error::IntegrityFailed)?;
                for (index, id) in &links {
                    items
                        .get_mut(*index)
                        .ok_or(Error::IntegrityFailed)?
                        .files
                        .push(id.clone());
                }
                let ids = h
                    .with(|c| {
                        session.check(c)?;
                        c.secret_import_commit_once(&receipt, items, staged, &folder)
                    })
                    .await?;
                return Ok(json!({"ids":ids,"attachments":links.len(),"files_local_only":true}));
            }
            _ => {}
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
            _ => Err(Error::InvalidInput(format!("unknown method {method}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dmsg_client::testing::{TempDir, unlocked};
    use std::sync::Arc;

    fn daemon(dir: &TempDir) -> Arc<Daemon> {
        let home = crate::home::Home {
            dir: dir.0.join("home"),
        };
        Arc::new(Daemon::new(
            "owner-token".into(),
            0,
            home.settings().unwrap(),
            unlocked(&dir.0.join("workspace"), false, None),
            dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
            crate::mcp_events::McpEvents::new(&[]).unwrap(),
        ))
    }

    #[tokio::test]
    async fn imports_check_the_preview_digest_and_keep_hidden_values_out_of_lists() {
        let dir = TempDir::new();
        let daemon = daemon(&dir);
        let path = dir.0.join("passwords.csv");
        std::fs::write(&path, "name,password\nMail,very-secret\n").unwrap();
        let preview = daemon
            .call_vault("vault/import_preview", &json!({"path":path}))
            .await
            .unwrap();
        assert!(!preview.to_string().contains("very-secret"));
        let params = json!({"path":path, "digest":preview["digest"], "picks":[0]});
        std::fs::write(&path, "name,password\nMail,changed-secret\n").unwrap();
        assert_eq!(
            daemon.call_vault("vault/import", &params).await,
            Err(Error::VersionConflict)
        );
        assert_eq!(
            daemon.call_vault("vault/list", &Value::Null).await.unwrap(),
            json!([])
        );
        std::fs::write(&path, "name,password\nMail,very-secret\n").unwrap();
        let added = daemon.call_vault("vault/import", &params).await.unwrap();
        assert_eq!(added["ids"].as_array().unwrap().len(), 1);
        let list = daemon.call_vault("vault/list", &Value::Null).await.unwrap();
        assert!(!list.to_string().contains("very-secret"));
        let preview = daemon
            .call_vault("vault/import_preview", &json!({"path":path}))
            .await
            .unwrap();
        assert_eq!(preview["items"][0]["duplicate"], true);
    }

    #[tokio::test]
    async fn locking_remains_available_while_an_import_is_parsed() {
        let dir = TempDir::new();
        let daemon = daemon(&dir);
        let path = dir.0.join("large.json");
        // A supported export with ample whitespace makes actual parsing
        // slow enough to observe, without importing thousands of secrets.
        let content = " ".repeat(32 * 1024 * 1024) + "{\"encrypted\":false,\"items\":[]}";
        std::fs::write(&path, content).unwrap();
        let work = tokio::spawn({
            let daemon = daemon.clone();
            async move {
                daemon
                    .call_vault("vault/import_preview", &json!({"path":path}))
                    .await
            }
        });
        tokio::task::yield_now().await;
        assert!(
            !work.is_finished(),
            "the async executor must not wait for parsing"
        );
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            daemon.call_vault("vault/list", &Value::Null).await.unwrap();
            daemon.client().lock().await.lock();
        })
        .await
        .expect("local reads and lock must remain available");
        assert_eq!(work.await.unwrap(), Err(Error::Locked));
    }
}
