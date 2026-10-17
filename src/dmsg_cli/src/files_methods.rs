//! The daemon methods of the file library (`files/*`): folders, files and
//! versions on this device, imports and exports by absolute path, opened
//! copies, the trash, verification, sharing a version to a channel and
//! saving a shared file; and the cloud: backups a person asks for
//! (`files/backup*`), the account's backup inventory, fetching, deleting
//! and verifying a cloud copy (`files/cloud_*`), freeing a verified
//! version's local chunks (`files/evict`), and share links for people
//! outside dMsg (`files/share_link*`, `files/share_revoke`). Nothing here
//! uploads by itself.

use crate::daemon::Daemon;
use crate::params::{absolute, number, optional, parse, text, to_json};
use dmsg_client::{
    library::{catalog::ROOT, VerifyReport},
    Client,
};
use dmsg_types::{Error, Result};
use serde_json::{json, Value};
use std::path::PathBuf;

/// The methods this module serves.
pub(crate) const METHODS: [&str; 38] = [
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
    "files/backup",
    "files/backup_status",
    "files/backup_cancel",
    "files/backup_run",
    "files/cloud_list",
    "files/cloud_refresh",
    "files/cloud_fetch",
    "files/cloud_delete",
    "files/cloud_verify",
    "files/cloud_usage",
    "files/evict",
    "files/share_link",
    "files/share_links",
    "files/share_revoke",
];

/// The events a method's success announces.
pub(crate) fn events_of(method: &str) -> &'static [&'static str] {
    match method {
        "files/list"
        | "files/tree"
        | "files/show"
        | "files/search"
        | "files/export"
        | "files/preview"
        | "files/open_list"
        | "files/verify"
        | "files/usage"
        | "files/backup_status"
        | "files/cloud_list"
        | "files/cloud_usage"
        | "files/share_links" => &[],
        "files/share_channel" => &["channel/changed", "files/changed"],
        _ => &["files/changed"],
    }
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

impl Daemon {
    /// Run one of [`METHODS`].
    pub(crate) async fn call_files(&self, method: &str, params: &Value) -> Result<Value> {
        // The whole library is scanned beside the client, not under it.
        if method == "files/verify" && params["id"].is_null() {
            let (check, versions) = self.check_library().await?;
            return Ok(to_json(&VerifyReport {
                versions,
                blobs_checked: check.blobs_checked,
                blobs_bad: check.blobs_bad,
            }));
        }
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
                let tags: Option<Vec<String>> = optional(params, "tags", parse)?;
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
            "files/verify" => Ok(to_json(&client.files_verify(&id()?, version)?)),
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
            "files/share_link" => {
                let name = client.files_show(&id()?)?.file.name;
                let mut progress = |done, total| {
                    self.emit(
                        "files/progress",
                        json!({"job": "share", "path": name, "done": done, "total": total}),
                    );
                };
                let link = client
                    .files_share_link(
                        &id()?,
                        version,
                        params["expires_in_ms"].as_u64(),
                        params["max_downloads"].as_u64().unwrap_or(0),
                        params["password"].as_str().filter(|p| !p.is_empty()),
                        &mut progress,
                    )
                    .await?;
                Ok(to_json(&link))
            }
            "files/share_links" => {
                let links = match params["refresh"].as_bool().unwrap_or(false) {
                    true => client.files_share_links_refresh().await?,
                    false => client.files_share_links()?,
                };
                Ok(to_json(&links))
            }
            "files/share_revoke" => Ok(to_json(
                &client
                    .files_share_revoke(&text(params, "share_id")?)
                    .await?,
            )),
            "files/backup" => {
                let ids = ids(params)?;
                let all = params["all_versions"].as_bool().unwrap_or(false);
                let request = client.files_backup(&ids, all).await?;
                Ok(to_json(&request))
            }
            "files/backup_status" => Ok(to_json(&client.files_backup_status()?)),
            "files/backup_cancel" => {
                client
                    .files_backup_cancel(&id()?, &text(params, "version")?)
                    .await?;
                Ok(json!({"cancelled": true}))
            }
            "files/backup_run" => {
                let mut progress = |file: &str, done, total| {
                    self.emit(
                        "files/progress",
                        json!({"job": "backup", "path": file, "done": done, "total": total}),
                    );
                };
                Ok(to_json(
                    &client.files_backup_run(None, &mut progress).await?,
                ))
            }
            "files/cloud_list" => Ok(to_json(&client.files_cloud_list()?)),
            "files/cloud_refresh" => Ok(to_json(&client.files_cloud_refresh().await?)),
            "files/cloud_fetch" => {
                let upload = text(params, "upload_id")?;
                let target = match params["folder"].is_null() {
                    true => None,
                    false => Some(folder(&mut client, &params["folder"], true)?),
                };
                let mut progress = |done, total| {
                    self.emit(
                        "files/progress",
                        json!({"job": "fetch", "path": upload, "done": done, "total": total}),
                    );
                };
                let file = client
                    .files_cloud_fetch(&upload, target.as_deref(), &mut progress)
                    .await?;
                Ok(to_json(&file))
            }
            "files/cloud_delete" => {
                let uploads: Vec<String> = parse(params, "upload_ids")?;
                let deleted = client.files_cloud_delete(&uploads).await?;
                Ok(json!({"deleted": deleted}))
            }
            "files/cloud_verify" => {
                let upload = text(params, "upload_id")?;
                let mut progress = |done, total| {
                    self.emit(
                        "files/progress",
                        json!({"job": "verify", "path": upload, "done": done, "total": total}),
                    );
                };
                Ok(to_json(
                    &client.files_cloud_verify(&upload, &mut progress).await?,
                ))
            }
            "files/cloud_usage" => Ok(to_json(&client.files_cloud_usage().await?)),
            "files/evict" => Ok(to_json(&client.files_evict(&id()?, version)?)),
            "files/save_shared" => {
                let seq = number(params, "seq")?;
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
