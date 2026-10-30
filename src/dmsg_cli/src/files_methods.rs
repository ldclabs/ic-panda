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
    Client,
    channel::{self, Draft},
    hold::Hold,
    library::{
        SearchFilter, VerifyReport, backup, catalog::ROOT, cloud, import, link, open, reading,
        storage,
    },
};
use dmsg_types::{Error, Result};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::Ordering;

/// The methods this module serves.
pub(crate) const METHODS: [&str; 45] = [
    "files/list",
    "files/tree",
    "files/show",
    "files/search",
    "files/tags",
    "files/mkdir",
    "files/rename",
    "files/move",
    "files/update",
    "files/import",
    "files/import_cancel",
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
    "files/local_only",
    // Deprecated cloud-space RPCs; retained for explicit callers, not desktop UI.
    "files/storage",
    "files/storage_refresh",
    "files/storage_action",
    "files/share_quote",
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
        | "files/tags"
        | "files/import_cancel"
        | "files/export"
        | "files/preview"
        | "files/open_list"
        | "files/verify"
        | "files/usage"
        | "files/local_only"
        | "files/backup_status"
        | "files/cloud_list"
        | "files/cloud_usage"
        | "files/share_links"
        | "files/storage"
        | "files/share_quote" => &[],
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
        // Cancellation does not wait for the import's next client step.
        if method == "files/import_cancel" {
            self.import_cancel.store(true, Ordering::SeqCst);
            return Ok(json!({"cancelled": true}));
        }
        if method == "files/import" {
            return self.import_files(params).await;
        }
        let h = self.hold();
        match method {
            "files/open" => {
                return Ok(to_json(
                    &open::open(&h, &text(params, "id")?, params["version"].as_str()).await?,
                ));
            }
            "files/usage" => return Ok(to_json(&reading::usage(&h).await?)),
            "files/local_only" => return Ok(to_json(&h.with(|c| c.local_only()).await?)),
            "files/export" => {
                return Ok(to_json(
                    &reading::export(
                        &h,
                        &text(params, "id")?,
                        params["version"].as_str(),
                        &absolute(params, "path")?,
                    )
                    .await?,
                ));
            }
            "files/preview" => {
                return Ok(to_json(
                    &reading::preview(&h, &text(params, "id")?, params["version"].as_str()).await?,
                ));
            }
            "files/verify" => {
                return Ok(to_json(
                    &reading::verify(&h, &text(params, "id")?, params["version"].as_str()).await?,
                ));
            }
            _ => {}
        }
        if let Some(answer) = self.call_files_cloud(method, params).await {
            return answer;
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
            "files/search" => {
                let filter = SearchFilter {
                    folder: match params["folder"].is_null() {
                        true => None,
                        false => Some(folder(&mut client, &params["folder"], false)?),
                    },
                    starred: params["starred"].as_bool().unwrap_or(false),
                    tag: params["tag"].as_str().map(str::to_string),
                };
                let query = params["query"].as_str().unwrap_or_default();
                Ok(to_json(&client.files_search(query, &filter)?))
            }
            "files/tags" => Ok(to_json(&client.files_tags()?)),
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
            "files/open_list" => Ok(to_json(&client.files_open_list()?)),
            "files/open_cleanup" => {
                let save = params["save"].as_bool().unwrap_or(false);
                // The copy saved as a version shows under its file's name.
                let name = match params["nonce"].as_str() {
                    Some(nonce) => client
                        .files_open_list()?
                        .into_iter()
                        .find(|copy| copy.nonce == nonce)
                        .map(|copy| copy.name),
                    None => None,
                };
                let mut progress = |done, total| {
                    self.emit(
                        "files/progress",
                        json!({"path": name, "done": done, "total": total}),
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
            #[allow(deprecated)] // Keep the deprecated RPC callable until its removal.
            "files/storage" => client.files_storage(),
            "files/share_links" => Ok(to_json(&client.files_share_links()?)),
            "files/backup_status" => Ok(to_json(&client.files_backup_status()?)),
            "files/cloud_list" => Ok(to_json(&client.files_cloud_list()?)),
            "files/evict" => Ok(to_json(&client.files_evict(&id()?, version)?)),
            _ => Err(Error::NotFound),
        }
    }

    /// Import in bounded steps; only imports take turns, so local reads and
    /// locking remain available while a large file is processed.
    async fn import_files(&self, params: &Value) -> Result<Value> {
        let h = self.hold();
        h.with(|c| Ok(c.session_state())).await?;
        let _turn = self.turn("import").await;
        let paths: Vec<PathBuf> = match params["paths"].as_array() {
            Some(list) => list
                .iter()
                .map(|p| {
                    let path = PathBuf::from(p.as_str().unwrap_or_default());
                    if path.is_absolute() {
                        Ok(path)
                    } else {
                        Err(Error::InvalidInput("paths must be absolute".into()))
                    }
                })
                .collect::<Result<_>>()?,
            None => vec![absolute(params, "path")?],
        };
        let target = h
            .with(|c| {
                let target = folder(c, &params["folder"], true)?;
                self.import_cancel.store(false, Ordering::SeqCst);
                c.files_import_cancel(Some(self.import_cancel.clone()));
                Ok(target)
            })
            .await?;
        let keep_both = params["keep_both"].as_bool().unwrap_or(false);
        let result = async {
            let mut imported = Vec::new();
            for path in &paths {
                let mut report = |file: &std::path::Path, done, total| {
                    self.emit(
                        "files/progress",
                        json!({"path": file.display().to_string(), "done": done, "total": total}),
                    );
                };
                if path.is_dir() {
                    imported.extend(
                        import::directory(&h, path, &target, keep_both, &mut report).await?,
                    );
                } else {
                    imported.push(
                        import::file(&h, path, &target, keep_both, &mut |done, total| {
                            report(path, done, total)
                        })
                        .await?,
                    );
                }
                self.emit("files/changed", json!({"method":"files/import"}));
            }
            Ok(to_json(&imported))
        }
        .await;
        let _ = h
            .with(|c| {
                c.files_import_cancel(None);
                Ok(())
            })
            .await;
        if result.is_err() {
            self.emit("files/changed", json!({"method":"files/import"}));
        }
        result
    }

    /// The methods of [`METHODS`] that reach the relay: uploads, downloads
    /// and the cloud's records, each waited on without the client; `None`
    /// for the others.
    async fn call_files_cloud(&self, method: &str, params: &Value) -> Option<Result<Value>> {
        let h = self.hold();
        let id = || text(params, "id");
        let version = params["version"].as_str();
        let progress = |job: &'static str, path: Value| {
            move |done: u64, total: u64| {
                self.emit(
                    "files/progress",
                    json!({"job": job, "path": path, "done": done, "total": total}),
                );
            }
        };
        let answer = async || -> Result<Option<Value>> {
            Ok(Some(match method {
                #[allow(deprecated)] // Deprecated RPC, retained for explicit callers.
                "files/storage_refresh" => {
                    let h = dmsg_client::hold::Unlocked::new(&h).await?;
                    storage::refresh(&h).await?
                }
                #[allow(deprecated)] // Deprecated RPC, retained for explicit callers.
                "files/storage_action" => {
                    storage::act(&h, &parse::<Vec<storage::Target>>(params, "targets")?).await?
                }
                "files/share_quote" => {
                    storage::quote(&h, &id()?, version, params["channel"].as_str()).await?
                }
                "files/share_channel" => {
                    let message_id = params["message_id"].as_str();
                    let _turn = self.message_turn(message_id).await;
                    let draft = Draft {
                        text: params["text"].as_str().unwrap_or_default().into(),
                        ..Default::default()
                    };
                    let channel = text(params, "channel")?;
                    channel::send_file(&h, &channel, &id()?, version, &draft, message_id).await?
                }
                "files/share_link" => {
                    let name = self.client().lock().await.files_show(&id()?)?.file.name;
                    let link = link::share_link(
                        &h,
                        &id()?,
                        version,
                        params["expires_in_ms"].as_u64(),
                        params["max_downloads"].as_u64().unwrap_or(0),
                        params["password"].as_str().filter(|p| !p.is_empty()),
                        &mut progress("share", name.into()),
                    )
                    .await?;
                    to_json(&link)
                }
                "files/share_links" if params["refresh"] == true => {
                    to_json(&link::share_links_refresh(&h).await?)
                }
                "files/share_revoke" => {
                    to_json(&link::share_revoke(&h, &text(params, "share_id")?).await?)
                }
                "files/backup" => {
                    let ids = ids(params)?;
                    let all = params["all_versions"].as_bool().unwrap_or(false);
                    h.with(|c| Ok(c.session_state())).await?;
                    let _turn = self.turn("backup-plan").await;
                    let queued = backup::backup(&h, &ids, all).await?;
                    self.wake();
                    to_json(&queued)
                }
                "files/backup_cancel" => {
                    backup::backup_cancel(&h, &id()?, &text(params, "version")?).await?;
                    json!({"cancelled": true})
                }
                "files/backup_run" => {
                    let turn = self.turn("backup").await;
                    to_json(&self.backup_pass(turn, None).await?)
                }
                "files/cloud_refresh" => to_json(&cloud::cloud_refresh(&h).await?),
                "files/cloud_fetch" => {
                    let upload = text(params, "upload_id")?;
                    let target = match params["folder"].is_null() {
                        true => None,
                        false => Some(folder(
                            &mut *self.client().lock().await,
                            &params["folder"],
                            true,
                        )?),
                    };
                    let mut progress = progress("fetch", upload.clone().into());
                    to_json(
                        &cloud::cloud_fetch(&h, &upload, target.as_deref(), &mut progress).await?,
                    )
                }
                "files/cloud_delete" => {
                    let uploads: Vec<String> = parse(params, "upload_ids")?;
                    json!({"accepted": cloud::cloud_delete(&h, &uploads).await?})
                }
                "files/cloud_verify" => {
                    let upload = text(params, "upload_id")?;
                    let mut progress = progress("verify", upload.clone().into());
                    to_json(&cloud::cloud_verify(&h, &upload, &mut progress).await?)
                }
                "files/cloud_usage" => to_json(&cloud::cloud_usage(&h).await?),
                "files/save_shared" => {
                    let seq = number(params, "seq")?;
                    let target = folder(&mut *self.client().lock().await, &params["folder"], true)?;
                    let channel = text(params, "channel")?;
                    let mut progress = progress("save", params["channel"].clone());
                    to_json(&channel::save_shared(&h, &channel, seq, &target, &mut progress).await?)
                }
                _ => return Ok(None),
            }))
        };
        answer().await.transpose()
    }
}
