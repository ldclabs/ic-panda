//! The daemon methods of the file library (`files/*`): folders, files and
//! versions on this device, imports and exports by absolute path, opened
//! copies, the trash, verification, sharing a version to a channel and
//! saving a shared file. Nothing here uploads by itself; a backup is a
//! separate, explicit action (the `files/backup*` and `files/cloud_*`
//! methods).

use crate::daemon::Daemon;
use dmsg_client::{library::catalog::ROOT, Client};
use dmsg_types::{Error, Result};
use serde_json::{json, Value};
use std::path::PathBuf;

/// The methods this module serves.
pub(crate) const METHODS: [&str; 24] = [
    "files/list",
    "files/tree",
    "files/show",
    "files/search",
    "files/mkdir",
    "files/rename",
    "files/move",
    "files/update",
    "files/import",
    "files/export",
    "files/preview",
    "files/open",
    "files/open_list",
    "files/open_cleanup",
    "files/versions",
    "files/version_use",
    "files/version_delete",
    "files/trash",
    "files/restore",
    "files/purge",
    "files/verify",
    "files/usage",
    "files/share_channel",
    "files/save_shared",
];

/// The events a method's success announces.
pub(crate) fn events_of(method: &str) -> &'static [&'static str] {
    match method {
        "files/list" | "files/tree" | "files/show" | "files/search" | "files/export"
        | "files/preview" | "files/open_list" | "files/verify" | "files/usage" => &[],
        "files/share_channel" => &["channel/changed", "files/changed"],
        _ => &["files/changed"],
    }
}

fn text(params: &Value, name: &str) -> Result<String> {
    params[name]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| Error::InvalidInput(format!("missing {name}")))
}

/// `ids`, else the single `id`.
fn ids(params: &Value) -> Result<Vec<String>> {
    if let Some(list) = params["ids"].as_array() {
        return list
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| Error::InvalidInput("ids".into()))
            })
            .collect();
    }
    Ok(vec![text(params, "id")?])
}

fn absolute(params: &Value, name: &str) -> Result<PathBuf> {
    let path = PathBuf::from(text(params, name)?);
    match path.is_absolute() {
        true => Ok(path),
        false => Err(Error::InvalidInput(format!("{name} must be absolute"))),
    }
}

fn is_id(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The folder a request names: an ID, `root` or nothing for the root, or
/// a `/`-separated path from the root, created on the way when `create`.
fn folder(client: &mut Client, value: &Value, create: bool) -> Result<String> {
    match value.as_str() {
        None | Some("") | Some(ROOT) => Ok(ROOT.into()),
        Some(id) if is_id(id) => Ok(id.into()),
        Some(path) if create => Ok(client.files_mkdir_path(path)?.id),
        Some(path) => client.files_folder_by_path(path),
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("serializable")
}

impl Daemon {
    /// Run one of [`METHODS`].
    pub(crate) async fn call_files(&self, method: &str, params: &Value) -> Result<Value> {
        let mut client = self.client().lock().await;
        let id = || text(params, "id");
        let version = params["version"].as_str();
        match method {
            "files/list" => {
                let trash = params["trash"].as_bool().unwrap_or(false);
                let folder = folder(&mut client, &params["folder"], false)?;
                Ok(to_json(&client.files_list(&folder, trash)?))
            }
            "files/tree" => Ok(to_json(&client.files_tree()?)),
            "files/show" => Ok(to_json(&client.files_show(&id()?)?)),
            "files/search" => Ok(to_json(&client.files_search(&text(params, "query")?)?)),
            "files/mkdir" => match params["path"].as_str() {
                Some(path) => Ok(to_json(&client.files_mkdir_path(path)?)),
                None => {
                    let parent = folder(&mut client, &params["parent"], false)?;
                    Ok(to_json(
                        &client.files_mkdir(&parent, &text(params, "name")?)?,
                    ))
                }
            },
            "files/rename" => {
                let id = id()?;
                client.files_rename(&id, &text(params, "name")?)?;
                Ok(json!({"id": id}))
            }
            "files/move" => {
                let target = folder(&mut client, &params["folder"], false)?;
                let ids = ids(params)?;
                client.files_move(&ids, &target)?;
                Ok(json!({"moved": ids.len(), "folder": target}))
            }
            "files/update" => {
                let tags: Option<Vec<String>> = match &params["tags"] {
                    Value::Null => None,
                    value => Some(
                        serde_json::from_value(value.clone())
                            .map_err(|_| Error::InvalidInput("tags".into()))?,
                    ),
                };
                let note = params["note"].as_str().map(str::to_string);
                let starred = params["starred"].as_bool();
                Ok(to_json(&client.files_update(
                    &id()?,
                    tags,
                    note,
                    starred,
                )?))
            }
            "files/import" => {
                let paths: Vec<PathBuf> = match params["paths"].as_array() {
                    Some(list) => list
                        .iter()
                        .map(|p| {
                            let path = PathBuf::from(p.as_str().unwrap_or_default());
                            match path.is_absolute() {
                                true => Ok(path),
                                false => Err(Error::InvalidInput("paths must be absolute".into())),
                            }
                        })
                        .collect::<Result<_>>()?,
                    None => vec![absolute(params, "path")?],
                };
                let target = folder(&mut client, &params["folder"], true)?;
                let keep_both = params["keep_both"].as_bool().unwrap_or(false);
                let mut imported = Vec::new();
                for path in &paths {
                    let name = path.display().to_string();
                    if path.is_dir() {
                        let mut progress = |file: &std::path::Path, done, total| {
                            self.emit(
                                "files/progress",
                                json!({"path": file.display().to_string(), "done": done, "total": total}),
                            );
                        };
                        imported.extend(client.files_import_dir(
                            path,
                            &target,
                            keep_both,
                            &mut progress,
                        )?);
                    } else {
                        let mut progress = |done, total| {
                            self.emit(
                                "files/progress",
                                json!({"path": name, "done": done, "total": total}),
                            );
                        };
                        imported.push(client.files_import(
                            path,
                            &target,
                            keep_both,
                            &mut progress,
                        )?);
                    }
                }
                Ok(to_json(&imported))
            }
            "files/export" => {
                let out = absolute(params, "path")?;
                Ok(to_json(&client.files_export(&id()?, version, &out)?))
            }
            "files/preview" => Ok(to_json(&client.files_preview(&id()?, version)?)),
            "files/open" => Ok(to_json(&client.files_open(&id()?, version)?)),
            "files/open_list" => Ok(to_json(&client.files_open_list()?)),
            "files/open_cleanup" => {
                let save = params["save"].as_bool().unwrap_or(false);
                let mut progress = |done, total| {
                    self.emit(
                        "files/progress",
                        json!({"path": params["nonce"], "done": done, "total": total}),
                    );
                };
                Ok(to_json(&client.files_open_cleanup(
                    params["nonce"].as_str(),
                    save,
                    &mut progress,
                )?))
            }
            "files/versions" => Ok(to_json(&client.files_versions(&id()?)?)),
            "files/version_use" => Ok(to_json(
                &client.files_version_use(&id()?, &text(params, "version")?)?,
            )),
            "files/version_delete" => Ok(to_json(
                &client.files_version_delete(&id()?, &text(params, "version")?)?,
            )),
            "files/trash" => {
                let ids = ids(params)?;
                client.files_trash(&ids)?;
                Ok(json!({"trashed": ids.len()}))
            }
            "files/restore" => {
                let ids = ids(params)?;
                client.files_restore(&ids)?;
                Ok(json!({"restored": ids.len()}))
            }
            "files/purge" => {
                let ids = match params["ids"].is_null() && params["id"].is_null() {
                    true => None,
                    false => Some(ids(params)?),
                };
                Ok(to_json(&client.files_purge(ids.as_deref())?))
            }
            "files/verify" => Ok(to_json(
                &client.files_verify(params["id"].as_str(), version)?,
            )),
            "files/usage" => Ok(to_json(&client.files_usage()?)),
            "files/share_channel" => {
                client
                    .channel_send_file(
                        &text(params, "channel")?,
                        &id()?,
                        version,
                        params["text"].as_str().unwrap_or_default(),
                        params["message_id"].as_str(),
                    )
                    .await
            }
            "files/save_shared" => {
                let seq = params["seq"]
                    .as_u64()
                    .ok_or_else(|| Error::InvalidInput("seq".into()))?;
                let target = folder(&mut client, &params["folder"], true)?;
                let mut progress = |done, total| {
                    self.emit(
                        "files/progress",
                        json!({"path": params["channel"], "done": done, "total": total}),
                    );
                };
                let imported = client
                    .files_save_shared(&text(params, "channel")?, seq, &target, &mut progress)
                    .await?;
                Ok(to_json(&imported))
            }
            _ => Err(Error::NotFound),
        }
    }
}
