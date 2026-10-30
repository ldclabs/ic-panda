//! `dmsg files`: the encrypted file library on this device.

use crate::locale::tr;
use crate::remote::{Failure, Remote};
use crate::{absolute, pretty, print, short};
use clap::Subcommand;
use serde_json::{Value, json};
use std::path::PathBuf;

/// The file library: folders, files and their versions, encrypted on this
/// device. Nothing leaves it unless you back it up or share it.
#[derive(Subcommand)]
pub enum FilesCommand {
    /// List a folder (ID or path; the root by default), or the trash.
    Ls {
        /// Folder ID or path, such as `docs/2026`.
        folder: Option<String>,
        /// The trash instead.
        #[arg(long)]
        trash: bool,
    },
    /// Every folder with its path.
    Tree,
    /// A file with its versions.
    Show {
        /// File ID.
        id: String,
    },
    /// Files whose name, tags or note contain the text.
    Search {
        /// The text.
        query: String,
    },
    /// Import files and folders; a folder comes in as a folder of the same
    /// name. A file of the same name in the target folder takes the import
    /// as a new version, and a folder of the same name the folder's files,
    /// unless --keep-both.
    Add {
        /// Files or folders.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Target folder: an ID or a path, created if missing; the root by default.
        #[arg(long)]
        to: Option<String>,
        /// Keep a same-named file and import beside it as `name (2)`.
        #[arg(long)]
        keep_both: bool,
    },
    /// Create a folder path under the root.
    Mkdir {
        /// Such as `docs/2026/q4`.
        path: String,
    },
    /// Move files and folders into a folder: `mv <id>... <folder>`.
    Mv {
        /// IDs to move, then the target folder (ID or path).
        #[arg(required = true, num_args = 2..)]
        ids: Vec<String>,
    },
    /// Rename a file or a folder.
    Rename {
        /// File or folder ID.
        id: String,
        /// The new name.
        name: String,
    },
    /// Add (`+tag`) and remove (`-tag`) a file's tags.
    Tag {
        /// File ID.
        id: String,
        /// `+tag` or `-tag`, any number.
        #[arg(required = true)]
        tags: Vec<String>,
    },
    /// Set a file's note; no text clears it.
    Note {
        /// File ID.
        id: String,
        /// The note.
        text: Option<String>,
    },
    /// Star a file, or unstar it with --off.
    Star {
        /// File ID.
        id: String,
        /// Remove the star.
        #[arg(long)]
        off: bool,
    },
    /// Verify a version and write it to a new path.
    Get {
        /// File ID.
        id: String,
        /// A version; the current one by default.
        #[arg(long)]
        version: Option<String>,
        /// Where to write it; must not exist.
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Export a version to a temporary copy for another program; the copy
    /// is removed after 30 minutes, on lock and on exit, and a changed
    /// copy is offered as a new version first.
    Open {
        /// File ID.
        id: String,
        /// A version; the current one by default.
        #[arg(long)]
        version: Option<String>,
    },
    /// Opened copies, or clean them up with --cleanup (--save keeps a
    /// changed copy as a new version).
    Opened {
        /// Remove the copies; one by its nonce, else all.
        #[arg(long)]
        cleanup: Option<Option<String>>,
        /// Save changed copies as new versions before removing them.
        #[arg(long)]
        save: bool,
    },
    /// Check a file's versions, or with --all every chunk file and, while
    /// unlocked, every version.
    Verify {
        /// File ID.
        #[arg(required_unless_present = "all")]
        id: Option<String>,
        /// The whole library.
        #[arg(long)]
        all: bool,
    },
    /// A file's versions.
    Versions {
        /// File ID.
        id: String,
    },
    /// Make a version the one the file reads as.
    VersionUse {
        /// File ID.
        id: String,
        /// Version ID.
        version: String,
    },
    /// Delete a version on this device; its backup, if any, stays.
    VersionRm {
        /// File ID.
        id: String,
        /// Version ID.
        version: String,
    },
    /// Move files and folders to the trash; backups stay.
    Rm {
        /// File or folder IDs.
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Take files and folders out of the trash.
    Restore {
        /// File or folder IDs.
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Delete trashed files and folders for good; none given empties the trash.
    Purge {
        /// File or folder IDs.
        ids: Vec<String>,
    },
    /// What the library occupies on this device.
    Usage,
    /// What only this device holds: file versions without a cloud backup,
    /// changes waiting to sync and when a sync last succeeded.
    LocalOnly,
    /// Share a version to a channel as a new edition under its own key.
    Share {
        /// Channel ID.
        channel: String,
        /// File ID.
        id: String,
        /// A version; the current one by default.
        #[arg(long)]
        version: Option<String>,
        /// The message text.
        #[arg(default_value = "")]
        text: String,
        /// Message ID, to retry a send.
        #[arg(long)]
        message_id: Option<String>,
    },
    /// Save a channel message's file into the library.
    Save {
        /// Channel ID.
        channel: String,
        /// The message's sequence.
        seq: u64,
        /// Target folder: an ID or a path, created if missing; the root by default.
        #[arg(long)]
        to: Option<String>,
    },
    /// Back up files and folders to the cloud: each file's current version,
    /// or every version with --all-versions. Nothing is backed up unless
    /// you ask; a copy deleted from the cloud is not uploaded again.
    Backup {
        /// File or folder IDs.
        #[arg(required = true)]
        ids: Vec<String>,
        /// Every version, not only the current one.
        #[arg(long)]
        all_versions: bool,
    },
    /// Backups in flight.
    BackupStatus,
    /// Cancel a backup in flight.
    BackupCancel {
        /// File ID.
        id: String,
        /// Version ID.
        version: String,
    },
    /// The account's backups in the cloud, from every device.
    #[command(subcommand)]
    Cloud(CloudCommand),
    /// Free a version's chunks on this device, keeping only its verified
    /// backup; reading it then needs a fetch.
    Evict {
        /// File ID.
        id: String,
        /// A version; the current one by default.
        #[arg(long)]
        version: Option<String>,
    },
    /// Share a version by link with someone outside dMsg: a fresh edition
    /// goes to the cloud under its own key, which the link carries in its
    /// fragment; with --password the link needs the password instead.
    Link {
        /// File ID.
        id: String,
        /// A version; the current one by default.
        #[arg(long)]
        version: Option<String>,
        /// How many days the link lasts, 1 to 30.
        #[arg(long, default_value_t = 7)]
        days: u64,
        /// How many downloads may start; 0 for no limit.
        #[arg(long, default_value_t = 0)]
        max_downloads: u64,
        /// Ask for a password the link needs; the key then travels wrapped under it.
        #[arg(long)]
        password: bool,
    },
    /// The links this device made; --refresh reads the relay's counts and
    /// lists links other devices made.
    Links {
        /// Read the relay first.
        #[arg(long)]
        refresh: bool,
    },
    /// Revoke a link: the cloud copy goes at once.
    LinkRevoke {
        /// Share ID.
        share_id: String,
    },
    /// Mount the library as a disk through the daemon (needs
    /// `files_mount = true` in config.toml, or the desktop's settings):
    /// reading decrypts, writing makes versions, locking unmounts.
    /// --status only shows the mount.
    Mount {
        /// Show the mount instead.
        #[arg(long)]
        status: bool,
    },
    /// Unmount the disk.
    Unmount,
}

/// The cloud: backups of every device of the account.
#[derive(Subcommand)]
pub enum CloudCommand {
    /// Backups this device knows of; --refresh reads the relay first.
    Ls {
        /// Read the relay's inventory first.
        #[arg(long)]
        refresh: bool,
    },
    /// Read the relay's inventory from this device's cursor on.
    Refresh,
    /// Fetch a backup into the library, where it was or into --to.
    Get {
        /// The backup's upload ID.
        upload: String,
        /// Target folder: an ID or a path, created if missing.
        #[arg(long)]
        to: Option<String>,
    },
    /// Delete cloud copies for good; files on this device stay.
    Rm {
        /// Upload IDs.
        #[arg(required = true)]
        uploads: Vec<String>,
    },
    /// Download and check a backup against its manifest.
    Verify {
        /// The backup's upload ID.
        upload: String,
    },
    /// The cloud's room and what the backups take of it.
    Usage,
}

fn file_line(f: &Value) -> String {
    let (id, name, size, status) = (
        f["id"].as_str().unwrap_or_default(),
        f["name"].as_str().unwrap_or_default(),
        &f["size"],
        f["status"].as_str().unwrap_or_default(),
    );
    let star = if f["starred"] == true { "★ " } else { "" };
    let versions = f["versions"].as_u64().unwrap_or(1);
    let versions = if versions > 1 {
        tr!(" ({versions} versions)", "（{versions} 个版本）")
    } else {
        String::new()
    };
    let tags: Vec<String> = f["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| t.as_str().map(|t| format!("#{t}")))
        .collect();
    let tags = if tags.is_empty() {
        String::new()
    } else {
        format!(" {}", tags.join(" "))
    };
    tr!(
        "{id} {star}{name} {size} bytes [{status}]{versions}{tags}",
        "{id} {star}{name} {size} 字节 [{status}]{versions}{tags}"
    )
}

fn folder_line(f: &Value) -> String {
    let (id, name) = (
        f["id"].as_str().unwrap_or_default(),
        f["name"].as_str().unwrap_or_default(),
    );
    format!("{id} {name}/")
}

fn listing_lines(listing: &Value) -> String {
    listing["folders"]
        .as_array()
        .into_iter()
        .flatten()
        .map(folder_line)
        .chain(
            listing["files"]
                .as_array()
                .into_iter()
                .flatten()
                .map(file_line),
        )
        .collect::<Vec<_>>()
        .join("\n")
}

fn version_lines(versions: &Value) -> String {
    versions
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| {
            let mark = if v["current"] == true { "*" } else { " " };
            let (id, size, status, at) = (
                v["id"].as_str().unwrap_or_default(),
                &v["size"],
                v["status"].as_str().unwrap_or_default(),
                &v["created_at"],
            );
            let origin = v["origin"]["type"].as_str().unwrap_or_default();
            tr!(
                "{mark} {id} {size} bytes [{status}] {origin} at {at}",
                "{mark} {id} {size} 字节 [{status}] {origin}，时间 {at}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn imported_lines(list: &Value) -> String {
    list.as_array()
        .into_iter()
        .flatten()
        .map(|i| {
            let file = &i["file"];
            let duplicates = i["duplicate_of"].as_array().map_or(0, Vec::len);
            let note = match (i["resumed"] == true, duplicates) {
                (true, 0) => tr!(" (resumed)", "（续传）"),
                (_, 0) => String::new(),
                (_, n) => tr!(
                    " (same content as {n} other file(s))",
                    "（与 {n} 个已有文件内容相同）"
                ),
            };
            format!("{}{note}", file_line(file))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The folder a flag names, for the daemon: an ID, a path or nothing.
fn folder_param(folder: Option<String>) -> Value {
    folder.map_or(Value::Null, Value::String)
}

pub async fn files(remote: &Remote, json: bool, command: FilesCommand) -> Result<(), Failure> {
    match command {
        FilesCommand::Ls { folder, trash } => {
            let listing = remote
                .call(
                    "files/list",
                    json!({"folder": folder_param(folder), "trash": trash}),
                )
                .await?;
            print(json, &listing, |l| {
                let lines = listing_lines(l);
                if lines.is_empty() {
                    tr!("(empty)", "（空）")
                } else {
                    lines
                }
            });
        }
        FilesCommand::Tree => {
            let folders = remote.call("files/tree", Value::Null).await?;
            print(json, &folders, |list| {
                list.as_array()
                    .into_iter()
                    .flatten()
                    .map(|f| {
                        let mut path: Vec<&str> = f["path"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .collect();
                        path.push(f["name"].as_str().unwrap_or_default());
                        format!(
                            "{} /{}",
                            f["id"].as_str().unwrap_or_default(),
                            path.join("/")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        FilesCommand::Show { id } => {
            let details = remote.call("files/show", json!({"id": id})).await?;
            print(json, &details, |d| {
                let file = &d["file"];
                let path: Vec<&str> = file["path"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                let note = file["note"].as_str().unwrap_or_default();
                let mut lines = vec![
                    file_line(file),
                    tr!("folder: /{}", "目录：/{}", path.join("/")),
                    tr!(
                        "type: {} · sha256 {}",
                        "类型：{} · sha256 {}",
                        file["mime"].as_str().unwrap_or_default(),
                        file["sha256"].as_str().unwrap_or_default()
                    ),
                ];
                if !note.is_empty() {
                    lines.push(tr!("note: {note}", "备注：{note}"));
                }
                lines.push(tr!("versions:", "版本："));
                lines.push(version_lines(&d["versions"]));
                lines.join("\n")
            });
        }
        FilesCommand::Search { query } => {
            let files = remote.call("files/search", json!({"query": query})).await?;
            print(json, &files, |list| {
                list.as_array()
                    .into_iter()
                    .flatten()
                    .map(file_line)
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        FilesCommand::Add {
            paths,
            to,
            keep_both,
        } => {
            let paths: Vec<String> = paths.into_iter().map(absolute).collect::<Result<_, _>>()?;
            let imported = remote
                .call(
                    "files/import",
                    json!({"paths": paths, "folder": folder_param(to), "keep_both": keep_both}),
                )
                .await?;
            print(json, &imported, imported_lines);
        }
        FilesCommand::Mkdir { path } => {
            let folder = remote.call("files/mkdir", json!({"path": path})).await?;
            print(json, &folder, folder_line);
        }
        FilesCommand::Mv { mut ids } => {
            let folder = ids.pop().unwrap_or_default();
            let moved = remote
                .call("files/move", json!({"ids": ids, "folder": folder}))
                .await?;
            print(json, &moved, |m| {
                tr!(
                    "moved {} into {}",
                    "已移动 {} 项到 {}",
                    m["moved"],
                    m["folder"]
                )
            });
        }
        FilesCommand::Rename { id, name } => {
            let renamed = remote
                .call("files/rename", json!({"id": id, "name": name}))
                .await?;
            print(json, &renamed, |_| tr!("renamed", "已重命名"));
        }
        FilesCommand::Tag { id, tags } => {
            let details = remote.call("files/show", json!({"id": id})).await?;
            let mut current: Vec<String> = details["file"]["tags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect();
            for tag in tags {
                if let Some(removed) = tag.strip_prefix('-') {
                    current.retain(|t| t != removed);
                } else {
                    let added = tag.strip_prefix('+').unwrap_or(&tag).to_string();
                    if !current.contains(&added) {
                        current.push(added);
                    }
                }
            }
            let file = remote
                .call("files/update", json!({"id": id, "tags": current}))
                .await?;
            print(json, &file, file_line);
        }
        FilesCommand::Note { id, text } => {
            let file = remote
                .call(
                    "files/update",
                    json!({"id": id, "note": text.unwrap_or_default()}),
                )
                .await?;
            print(json, &file, file_line);
        }
        FilesCommand::Star { id, off } => {
            let file = remote
                .call("files/update", json!({"id": id, "starred": !off}))
                .await?;
            print(json, &file, file_line);
        }
        FilesCommand::Get { id, version, out } => {
            let manifest = remote
                .call(
                    "files/export",
                    json!({"id": id, "version": version, "path": absolute(out)?}),
                )
                .await?;
            print(json, &manifest, crate::verified);
        }
        FilesCommand::Open { id, version } => {
            let opened = remote
                .call("files/open", json!({"id": id, "version": version}))
                .await?;
            print(json, &opened, |o| {
                tr!(
                    "{} (removed after 30 minutes, on lock or on exit; a changed copy is offered as a new version first)",
                    "{}（30 分钟后、锁定或退出时删除；被改动的副本会先询问是否另存为新版本）",
                    o["path"].as_str().unwrap_or_default()
                )
            });
        }
        FilesCommand::Opened { cleanup, save } => match cleanup {
            None => {
                let copies = remote.call("files/open_list", Value::Null).await?;
                print(json, &copies, |list| {
                    list.as_array()
                        .into_iter()
                        .flatten()
                        .map(|c| {
                            let changed = if c["changed"] == true {
                                tr!(" (changed)", "（已改动）")
                            } else {
                                String::new()
                            };
                            format!(
                                "{} {}{changed}",
                                c["nonce"].as_str().unwrap_or_default(),
                                c["path"].as_str().unwrap_or_default()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                });
            }
            Some(nonce) => {
                let removed = remote
                    .call("files/open_cleanup", json!({"nonce": nonce, "save": save}))
                    .await?;
                print(json, &removed, |list| {
                    tr!(
                        "removed {} copies",
                        "已删除 {} 个副本",
                        list.as_array().map_or(0, Vec::len)
                    )
                });
            }
        },
        FilesCommand::Verify { id, all: _ } => {
            let report = remote.call("files/verify", json!({"id": id})).await?;
            print(json, &report, |r| {
                let versions = r["versions"].as_array().map_or(0, Vec::len);
                let bad = r["versions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|v| v["ok"] != true)
                    .count();
                let blobs = &r["blobs_checked"];
                let damaged = r["blobs_bad"].as_array().map_or(0, Vec::len);
                tr!(
                    "{versions} versions checked, {bad} failed; {blobs} chunk files checked, {damaged} damaged",
                    "已核对 {versions} 个版本，{bad} 个失败；已核对 {blobs} 个密文块文件，{damaged} 个损坏"
                )
            });
        }
        FilesCommand::Versions { id } => {
            let versions = remote.call("files/versions", json!({"id": id})).await?;
            print(json, &versions, version_lines);
        }
        FilesCommand::VersionUse { id, version } => {
            let file = remote
                .call("files/version_use", json!({"id": id, "version": version}))
                .await?;
            print(json, &file, file_line);
        }
        FilesCommand::VersionRm { id, version } => {
            let file = remote
                .call(
                    "files/version_delete",
                    json!({"id": id, "version": version}),
                )
                .await?;
            print(json, &file, file_line);
        }
        FilesCommand::Rm { ids } => {
            let result = remote.call("files/trash", json!({"ids": ids})).await?;
            print(json, &result, |r| {
                tr!(
                    "moved {} to the trash",
                    "已把 {} 项移到回收站",
                    r["trashed"]
                )
            });
        }
        FilesCommand::Restore { ids } => {
            let result = remote.call("files/restore", json!({"ids": ids})).await?;
            print(json, &result, |r| {
                tr!("restored {}", "已恢复 {} 项", r["restored"])
            });
        }
        FilesCommand::Purge { ids } => {
            let params = match ids.is_empty() {
                true => Value::Null,
                false => json!({"ids": ids}),
            };
            let report = remote.call("files/purge", params).await?;
            print(json, &report, |r| {
                tr!(
                    "deleted {} files and {} folders for good; {} chunk files freed",
                    "已彻底删除 {} 个文件和 {} 个文件夹；释放 {} 个密文块文件",
                    r["purged_files"],
                    r["purged_folders"],
                    r["collected"]
                )
            });
        }
        FilesCommand::Usage => {
            let usage = remote.call("files/usage", Value::Null).await?;
            print(json, &usage, pretty);
        }
        FilesCommand::LocalOnly => {
            let only = remote.call("files/local_only", Value::Null).await?;
            print(json, &only, |o| {
                tr!(
                    "{} of {} versions here ({} of {} bytes) have no cloud backup; {} changes wait to sync",
                    "本机 {1} 个版本中有 {0} 个（{3} 字节中的 {2} 字节）没有云端备份；{4} 项改动等待同步",
                    o["unbacked_versions"],
                    o["versions"],
                    o["unbacked_bytes"],
                    o["bytes"],
                    o["pending_sync"]
                )
            });
        }
        FilesCommand::Share {
            channel,
            id,
            version,
            text,
            message_id,
        } => {
            let sent = remote
                .call(
                    "files/share_channel",
                    json!({"channel": channel, "id": id, "version": version, "text": text, "message_id": message_id}),
                )
                .await?;
            print(json, &sent, |s| {
                tr!(
                    "shared as message {}",
                    "已分享，消息 {}",
                    s["message_id"].as_str().unwrap_or_default()
                )
            });
        }
        FilesCommand::Save { channel, seq, to } => {
            let imported = remote
                .call(
                    "files/save_shared",
                    json!({"channel": channel, "seq": seq, "folder": folder_param(to)}),
                )
                .await?;
            print(json, &imported, |i| file_line(&i["file"]));
        }
        FilesCommand::Backup { ids, all_versions } => {
            let request = remote
                .call(
                    "files/backup",
                    json!({"ids": ids, "all_versions": all_versions}),
                )
                .await?;
            print(json, &request, |r| {
                let started = r["started"].as_array().map_or(0, Vec::len);
                let bytes = &r["bytes"];
                let mut lines = vec![tr!(
                    "{started} backup(s) started, {bytes} bytes to upload",
                    "已开始 {started} 个备份，共 {bytes} 字节待上传"
                )];
                for s in r["skipped"].as_array().into_iter().flatten() {
                    lines.push(tr!(
                        "skipped {}@{}: {}",
                        "跳过 {}@{}：{}",
                        s["file"].as_str().unwrap_or_default(),
                        s["version"].as_str().unwrap_or_default(),
                        s["reason"].as_str().unwrap_or_default()
                    ));
                }
                lines.join("\n")
            });
        }
        FilesCommand::BackupStatus => {
            let jobs = remote.call("files/backup_status", Value::Null).await?;
            print(json, &jobs, |list| {
                let lines: Vec<String> = list
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|j| {
                        let state = match (j["committed"] == true, j["error"].as_str()) {
                            (true, _) => tr!("committed", "已提交"),
                            (false, Some(error)) => tr!("failed: {error}", "失败：{error}"),
                            (false, None) => tr!("uploading", "上传中"),
                        };
                        format!(
                            "{}@{} {}/{} {state}",
                            j["file"].as_str().unwrap_or_default(),
                            j["version"].as_str().unwrap_or_default(),
                            j["sent"],
                            j["chunks"]
                        )
                    })
                    .collect();
                if lines.is_empty() {
                    tr!("no backups in flight", "没有进行中的备份")
                } else {
                    lines.join("\n")
                }
            });
        }
        FilesCommand::BackupCancel { id, version } => {
            let result = remote
                .call("files/backup_cancel", json!({"id": id, "version": version}))
                .await?;
            print(json, &result, |_| tr!("cancelled", "已取消"));
        }
        FilesCommand::Cloud(CloudCommand::Ls { refresh }) => {
            if refresh {
                remote.call("files/cloud_refresh", Value::Null).await?;
            }
            let list = remote.call("files/cloud_list", Value::Null).await?;
            print(json, &list, |list| {
                let lines: Vec<String> = list
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(cloud_line)
                    .collect();
                if lines.is_empty() {
                    tr!(
                        "no backups known; try --refresh",
                        "没有已知备份；试试 --refresh"
                    )
                } else {
                    lines.join("\n")
                }
            });
        }
        FilesCommand::Cloud(CloudCommand::Refresh) => {
            let report = remote.call("files/cloud_refresh", Value::Null).await?;
            print(json, &report, |r| {
                tr!(
                    "{} new, {} deleted, {} need a root this device lacks; cursor {}",
                    "新增 {}，删除 {}，{} 个需要本机没有的根；游标 {}",
                    r["added"],
                    r["deleted"],
                    r["needs_root"],
                    r["cursor"]
                )
            });
        }
        FilesCommand::Cloud(CloudCommand::Get { upload, to }) => {
            let file = remote
                .call(
                    "files/cloud_fetch",
                    json!({"upload_id": upload, "folder": folder_param(to)}),
                )
                .await?;
            print(json, &file, file_line);
        }
        FilesCommand::Cloud(CloudCommand::Rm { uploads }) => {
            let result = remote
                .call("files/cloud_delete", json!({"upload_ids": uploads}))
                .await?;
            print(json, &result, |r| {
                tr!(
                    "deleted {} cloud copies; files on this device are unchanged",
                    "已删除 {} 个云端副本；本机文件不变",
                    r["deleted"]
                )
            });
        }
        FilesCommand::Cloud(CloudCommand::Verify { upload }) => {
            let check = remote
                .call("files/cloud_verify", json!({"upload_id": upload}))
                .await?;
            print(json, &check, |c| match c["ok"] == true {
                true => tr!("verified", "已核对"),
                false => tr!(
                    "failed: {}",
                    "失败：{}",
                    c["error"].as_str().unwrap_or_default()
                ),
            });
        }
        FilesCommand::Cloud(CloudCommand::Usage) => {
            let usage = remote.call("files/cloud_usage", Value::Null).await?;
            print(json, &usage, pretty);
        }
        FilesCommand::Evict { id, version } => {
            let file = remote
                .call("files/evict", json!({"id": id, "version": version}))
                .await?;
            print(json, &file, file_line);
        }
        FilesCommand::Link {
            id,
            version,
            days,
            max_downloads,
            password,
        } => {
            if !(1..=30).contains(&days) {
                return Err(Failure::new("INVALID_INPUT", "days must be 1 to 30"));
            }
            let secret = if password {
                let typed = rpassword::prompt_password(tr!("Password: ", "口令："))
                    .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
                if typed.is_empty() {
                    return Err(Failure::new("INVALID_INPUT", "empty password"));
                }
                Some(typed)
            } else {
                None
            };
            let link = remote
                .call(
                    "files/share_link",
                    json!({
                        "id": id,
                        "version": version,
                        "expires_in_ms": days * 86_400_000,
                        "max_downloads": max_downloads,
                        "password": secret,
                    }),
                )
                .await?;
            print(json, &link, link_line);
        }
        FilesCommand::Links { refresh } => {
            let links = remote
                .call("files/share_links", json!({"refresh": refresh}))
                .await?;
            print(json, &links, |list| {
                let lines: Vec<String> = list
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(link_line)
                    .collect();
                if lines.is_empty() {
                    tr!("no links", "没有链接")
                } else {
                    lines.join("\n")
                }
            });
        }
        FilesCommand::LinkRevoke { share_id } => {
            let link = remote
                .call("files/share_revoke", json!({"share_id": share_id}))
                .await?;
            print(json, &link, link_line);
        }
        FilesCommand::Mount { status } => {
            let method = if status {
                "files/mount_status"
            } else {
                "files/mount"
            };
            let params = if status {
                Value::Null
            } else {
                json!({"name": tr!("dMsg Files", "dMsg 文件")})
            };
            let mount = remote.call(method, params).await?;
            print(json, &mount, mount_line);
        }
        FilesCommand::Unmount => {
            let mount = remote.call("files/unmount", Value::Null).await?;
            print(json, &mount, mount_line);
        }
    }
    Ok(())
}

fn link_line(l: &Value) -> String {
    let share = l["share_id"].as_str().unwrap_or_default();
    let name = l["name"].as_str().unwrap_or("(another device's link)");
    let state = l["state"].as_str().unwrap_or_default();
    let downloads = l["downloads"].as_u64().unwrap_or(0);
    let limit = match l["max_downloads"].as_u64().unwrap_or(0) {
        0 => String::new(),
        n => format!("/{n}"),
    };
    let now = dmsg_client::now_ms();
    let left = l["expires_at"].as_u64().unwrap_or(0).saturating_sub(now);
    let expires = match left {
        0 => tr!("expired", "已过期"),
        ms if ms < 86_400_000 => tr!("{} hours left", "剩 {} 小时", ms.div_ceil(3_600_000)),
        ms => tr!("{} days left", "剩 {} 天", ms.div_ceil(86_400_000)),
    };
    let mut line = tr!(
        "{share} {name} [{state}] downloads {downloads}{limit}, {expires}",
        "{share} {name} [{state}] 下载 {downloads}{limit}，{expires}"
    );
    if let Some(url) = l["url"].as_str() {
        line.push('\n');
        line.push_str(url);
        if l["password"] == true {
            line.push_str(&tr!(
                "\n(the link opens only with its password; send it another way)",
                "\n（链接须配合口令打开；请另行告知口令）"
            ));
        } else {
            line.push_str(&tr!(
                "\n(the key is in the link after #; send it over a private channel)",
                "\n（密钥在链接 # 之后；请通过私密渠道发送）"
            ));
        }
    }
    line
}

fn mount_line(m: &Value) -> String {
    let note = m["note"].as_str().unwrap_or_default();
    if m["mounted"] == true {
        let point = m["mount_point"]
            .as_str()
            .or(m["url"].as_str())
            .unwrap_or_default();
        return tr!("Mounted at {point}\n{note}", "已挂载于 {point}\n{note}");
    }
    if m["enabled"] != true {
        return tr!(
            "Not mounted; mounting is off (files_mount = true in config.toml, or the desktop's settings, turns it on).\n{note}",
            "未挂载；挂载功能已关闭（在 config.toml 中设置 files_mount = true，或在桌面端设置中开启）。\n{note}"
        );
    }
    if m["supported"] != true {
        return tr!(
            "Not mounted; not supported on this system.",
            "未挂载；此系统不支持。"
        );
    }
    tr!("Not mounted.\n{note}", "未挂载。\n{note}")
}

fn cloud_line(c: &Value) -> String {
    let upload = c["upload_id"].as_str().unwrap_or_default();
    if c["needs_root"] == true {
        return tr!(
            "{upload} (needs a content root this device lacks; generation {})",
            "{upload}（需要本机没有的内容根；代次 {}）",
            c["generation"]
        );
    }
    let mut path: Vec<&str> = c["path"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    path.push(c["name"].as_str().unwrap_or_default());
    let local = if c["local"] == true {
        tr!(" (here too)", "（本机也有）")
    } else {
        String::new()
    };
    let device = c["device"].as_str().unwrap_or_default();
    tr!(
        "{upload} /{} {} bytes from device {} at {}{local}",
        "{upload} /{} {} 字节，设备 {}，时间 {}{local}",
        path.join("/"),
        c["size"],
        short(device, 8),
        c["committed_at"]
    )
}
