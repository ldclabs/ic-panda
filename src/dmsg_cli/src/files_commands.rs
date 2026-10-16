//! `dmsg files`: the encrypted file library on this device.

use crate::locale::tr;
use crate::remote::{Failure, Remote};
use crate::{absolute, pretty, print};
use clap::Subcommand;
use serde_json::{json, Value};
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
    /// Import files and folders. A file of the same name in the target
    /// folder takes the import as a new version, unless --keep-both.
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
    }
    Ok(())
}
