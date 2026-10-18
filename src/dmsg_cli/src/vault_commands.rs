//! CLI commands of the vault: items and their fields, history, trash and
//! conflict copies, one-time codes, imports and the security check.

use crate::locale::tr;
use crate::remote::{Failure, Remote};
use crate::{absolute, interactive, pretty, print, short};
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::io::{IsTerminal, Read};

#[derive(Args)]
pub struct ItemFields {
    /// Title.
    #[arg(long)]
    title: Option<String>,
    /// Notes.
    #[arg(long)]
    notes: Option<String>,
    /// The user name field.
    #[arg(long)]
    username: Option<String>,
    /// The address field.
    #[arg(long)]
    url: Option<String>,
    /// Tags; replaces the item's tags.
    #[arg(long = "tag")]
    tags: Vec<String>,
    /// A field, `[TYPE:]LABEL=VALUE`: TYPE is text (the default), url,
    /// email, phone, date (YYYY-MM-DD), month (YYYY-MM) or concealed. An
    /// existing field of that name takes the value: its label, or a type's
    /// own field by ID, such as host or cvv.
    #[arg(long = "field")]
    fields: Vec<String>,
    /// A one-time password field: an `otpauth://totp/…` URI or a base32
    /// secret.
    #[arg(long)]
    otp: Option<String>,
    /// Read the type's main secret (password, credential, key, private
    /// key, recovery phrase, card number…) from standard input; a note
    /// keeps it in a concealed `secret` field.
    #[arg(long)]
    secret_stdin: bool,
}

#[derive(Subcommand)]
pub enum VaultCommand {
    /// Add an item: its type's fields, filled from the options.
    Add {
        /// login, api, key, ssh, wallet, card, identity, server, database,
        /// license or note.
        #[arg(long = "type", default_value = "note")]
        kind: String,
        /// For an ssh item: a new Ed25519 key.
        #[arg(long)]
        generate: bool,
        #[command(flatten)]
        fields: ItemFields,
    },
    /// Edit an item; only what the options name changes.
    Edit {
        /// Item ID.
        id: String,
        #[command(flatten)]
        fields: ItemFields,
        /// Take every tag off.
        #[arg(long, conflicts_with = "tags")]
        clear_tags: bool,
        /// Star the item.
        #[arg(long)]
        favorite: bool,
        /// Unstar the item.
        #[arg(long, conflicts_with = "favorite")]
        no_favorite: bool,
        /// Remove a field, by ID or label.
        #[arg(long = "rm-field")]
        remove: Vec<String>,
    },
    /// Add a copy of an item, hidden values included.
    Duplicate {
        /// Item ID.
        id: String,
        /// The copy's title; the item's own with " (copy)" by default.
        #[arg(long)]
        title: Option<String>,
    },
    /// Move items to the trash.
    Rm {
        /// Item IDs.
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Restore an item from the trash.
    Restore {
        /// Item ID.
        id: String,
    },
    /// List items; hidden values stay hidden.
    List {
        /// Deleted items instead.
        #[arg(long)]
        trash: bool,
    },
    /// Search titles, tags, notes, field labels and shown field values on
    /// this device.
    Search {
        /// Text to find.
        query: String,
    },
    /// Show one item.
    Show {
        /// Item ID.
        id: String,
        /// Include concealed values and one-time password secrets.
        #[arg(long)]
        reveal: bool,
        /// An earlier version instead of the current one.
        #[arg(long)]
        revision: Option<String>,
    },
    /// Every version of an item, newest first.
    History {
        /// Item ID.
        id: String,
    },
    /// Make an earlier version's content current again.
    Revert {
        /// Item ID.
        id: String,
        /// The version.
        revision: String,
    },
    /// The current one-time code of an item.
    Otp {
        /// Item ID.
        id: String,
        /// The field; the item's first one-time password by default.
        #[arg(long)]
        field: Option<String>,
    },
    /// Conflict copies waiting for a decision.
    Conflicts,
    /// Settle a conflict copy: adopt it, or keep the current version.
    Resolve {
        /// The copy, as `id:revision`.
        key: String,
        /// Keep the current version instead.
        #[arg(long)]
        keep_current: bool,
    },
    /// Import a 1Password (.1pux or CSV), Bitwarden (JSON or CSV), Chrome,
    /// Firefox or Safari export, read on this device. Without --yes it
    /// only shows what would be added.
    Import {
        /// The export file.
        path: std::path::PathBuf,
        /// Add the items that are not here yet.
        #[arg(long)]
        yes: bool,
        /// With --yes, add items that look already here too.
        #[arg(long, requires = "yes")]
        all: bool,
    },
    /// Weak or reused passwords, addresses without TLS and expiry dates
    /// passed or close.
    Health,
}

/// The secret from standard input, without the final newline.
fn read_secret() -> Result<String, Failure> {
    let mut secret = String::new();
    if std::io::stdin().is_terminal() {
        eprintln!(
            "{}",
            tr!("Secret (end with Ctrl-D):", "秘密字段（按 Ctrl-D 结束）：")
        );
    }
    std::io::stdin()
        .read_to_string(&mut secret)
        .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
    Ok(secret.trim_end_matches(['\r', '\n']).to_string())
}

/// `[TYPE:]LABEL=VALUE` as a field change.
fn field(spec: &str) -> Result<Value, Failure> {
    let invalid = || {
        Failure::new(
            "INVALID_INPUT",
            format!("--field {spec}: expected [TYPE:]LABEL=VALUE"),
        )
    };
    let (name, value) = spec.split_once('=').ok_or_else(invalid)?;
    let types = [
        "text",
        "concealed",
        "url",
        "email",
        "phone",
        "date",
        "month",
    ];
    let (kind, label) = match name.split_once(':') {
        Some((kind, label)) if types.contains(&kind) => (Some(kind), label),
        _ => (None, name),
    };
    if label.trim().is_empty() {
        return Err(invalid());
    }
    let mut change = json!({"label": label, "value": value});
    if let Some(kind) = kind {
        change["type"] = kind.into();
    }
    Ok(change)
}

impl ItemFields {
    /// The options as `vault/create` or `vault/edit` changes; `secret` is
    /// the field `--secret-stdin` fills.
    fn changes(self, secret: Option<&str>) -> Result<serde_json::Map<String, Value>, Failure> {
        let mut changes = serde_json::Map::new();
        if let Some(title) = self.title {
            changes.insert("title".into(), title.into());
        }
        if let Some(notes) = self.notes {
            changes.insert("notes".into(), notes.into());
        }
        if !self.tags.is_empty() {
            changes.insert("tags".into(), json!(self.tags));
        }
        let mut set = Vec::new();
        for (id, value) in [("username", self.username), ("url", self.url)] {
            if let Some(value) = value {
                set.push(json!({"id": id, "value": value}));
            }
        }
        for spec in &self.fields {
            set.push(field(spec)?);
        }
        if let Some(otp) = self.otp {
            set.push(json!({"id": "otp", "type": "otp", "value": otp}));
        }
        if self.secret_stdin {
            // A type without a main secret, such as a note, takes a
            // concealed `secret` field.
            let id = secret.unwrap_or("secret");
            set.push(json!({"id": id, "type": "concealed", "value": read_secret()?}));
        }
        if !set.is_empty() {
            changes.insert("set".into(), set.into());
        }
        Ok(changes)
    }
}

/// The main secret field of `kind`, from the daemon's templates.
async fn secret_of(remote: &Remote, kind: &str) -> Result<Option<String>, Failure> {
    let templates = remote.call("vault/templates", Value::Null).await?;
    Ok(templates["templates"][kind]["secret"]
        .as_str()
        .map(str::to_string))
}

fn entry_lines(list: &Value) -> String {
    list.as_array()
        .into_iter()
        .flatten()
        .map(|e| {
            format!(
                "{} [{}] {}{}{}",
                e["id"].as_str().unwrap_or_default(),
                e["item"]["type"].as_str().unwrap_or_default(),
                e["item"]["title"].as_str().unwrap_or_default(),
                if e["synced"] == true {
                    String::new()
                } else {
                    tr!(" (not synced)", "（未同步）")
                },
                if e["conflict"] == true {
                    let revision = e["revision"].as_str().unwrap_or_default();
                    tr!(" (conflict copy {revision})", "（冲突副本 {revision}）")
                } else {
                    String::new()
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `dmsg vault`.
pub async fn vault(remote: &Remote, json: bool, command: VaultCommand) -> Result<(), Failure> {
    match command {
        VaultCommand::Add {
            kind,
            generate,
            fields,
        } => {
            if fields.title.is_none() {
                return Err(Failure::new("INVALID_INPUT", "--title is required"));
            }
            let secret = secret_of(remote, &kind).await?;
            let mut changes = fields.changes(secret.as_deref())?;
            if generate {
                if kind != "ssh" {
                    return Err(Failure::new(
                        "INVALID_INPUT",
                        "--generate makes keys of ssh items",
                    ));
                }
                let key = remote
                    .call("vault/ssh_keygen", json!({"comment": changes["title"]}))
                    .await?;
                let set = changes
                    .entry("set")
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .expect("an array");
                for id in ["private_key", "public_key", "fingerprint"] {
                    set.push(json!({"id": id, "value": key[id]}));
                }
            }
            let entry = remote
                .call("vault/create", json!({"type": kind, "changes": changes}))
                .await?;
            print(json, &entry, |v| {
                v["id"].as_str().unwrap_or_default().to_string()
            });
        }
        VaultCommand::Edit {
            id,
            fields,
            clear_tags,
            favorite,
            no_favorite,
            remove,
        } => {
            let shown = remote.call("vault/show", json!({"id": id})).await?;
            let kind = shown["item"]["type"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let secret = secret_of(remote, &kind).await?;
            let mut changes = fields.changes(secret.as_deref())?;
            if clear_tags {
                changes.insert("tags".into(), json!([]));
            }
            if favorite || no_favorite {
                changes.insert("favorite".into(), favorite.into());
            }
            if !remove.is_empty() {
                changes.insert("remove".into(), json!(remove));
            }
            let entry = remote
                .call(
                    "vault/edit",
                    json!({"id": id, "base": shown["revision"], "changes": changes}),
                )
                .await?;
            print(json, &entry, |v| {
                let conflict = if v["conflict"] == true {
                    tr!(" (kept as a conflict copy)", "（保存为冲突副本）")
                } else {
                    String::new()
                };
                format!("{}{conflict}", v["revision"].as_str().unwrap_or_default())
            });
        }
        VaultCommand::Duplicate { id, title } => {
            let title = match title {
                Some(title) => title,
                None => {
                    let shown = remote.call("vault/show", json!({"id": id})).await?;
                    let title = shown["item"]["title"].as_str().unwrap_or_default();
                    tr!("{title} (copy)", "{title}（副本）")
                }
            };
            let entry = remote
                .call("vault/duplicate", json!({"id": id, "title": title}))
                .await?;
            print(json, &entry, |v| {
                v["id"].as_str().unwrap_or_default().to_string()
            });
        }
        VaultCommand::Rm { ids } => {
            let entries = remote.call("vault/delete", json!({"ids": ids})).await?;
            print(json, &entries, |_| {
                tr!("moved to the trash", "已移到回收站")
            });
        }
        VaultCommand::Restore { id } => {
            let entry = remote.call("vault/restore", json!({"id": id})).await?;
            print(json, &entry, |_| tr!("restored", "已恢复"));
        }
        VaultCommand::List { trash } => {
            let entries = remote.call("vault/list", json!({"trash": trash})).await?;
            print(json, &entries, entry_lines);
        }
        VaultCommand::Search { query } => {
            let entries = remote.call("vault/list", json!({"query": query})).await?;
            print(json, &entries, entry_lines);
        }
        VaultCommand::Show {
            id,
            reveal,
            revision,
        } => {
            let entry = remote
                .call(
                    "vault/show",
                    json!({"id": id, "reveal": reveal, "revision": revision}),
                )
                .await?;
            print(json, &entry, pretty);
        }
        VaultCommand::History { id } => {
            let versions = remote.call("vault/history", json!({"id": id})).await?;
            print(json, &versions, |list| {
                list.as_array()
                    .into_iter()
                    .flatten()
                    .map(|v| {
                        let mut marks = Vec::new();
                        for (key, mark) in [
                            ("current", tr!("current", "当前")),
                            ("deleted", tr!("deleted", "已删除")),
                            ("conflict", tr!("conflict copy", "冲突副本")),
                        ] {
                            if v[key] == true {
                                marks.push(mark);
                            }
                        }
                        if v["synced"] != true {
                            marks.push(tr!("not synced", "未同步"));
                        }
                        format!(
                            "{} {} {} {}",
                            v["revision"].as_str().unwrap_or_default(),
                            crate::logs::timestamp(v["at"].as_u64().unwrap_or_default()),
                            short(v["device"].as_str().unwrap_or_default(), 8),
                            marks.join(", ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        VaultCommand::Revert { id, revision } => {
            let entry = remote
                .call("vault/revert", json!({"id": id, "revision": revision}))
                .await?;
            print(json, &entry, |v| {
                let revision = v["revision"].as_str().unwrap_or_default();
                tr!("reverted; head {revision}", "已恢复；当前版本 {revision}")
            });
        }
        VaultCommand::Otp { id, field } => {
            let code = remote
                .call("vault/otp", json!({"id": id, "field": field}))
                .await?;
            print(json, &code, |v| {
                let seconds = v["remaining_ms"]
                    .as_u64()
                    .unwrap_or_default()
                    .div_ceil(1000);
                format!(
                    "{} ({})",
                    v["code"].as_str().unwrap_or_default(),
                    tr!("{seconds} s left", "剩余 {seconds} 秒")
                )
            });
        }
        VaultCommand::Conflicts => {
            let conflicts = remote.call("vault/conflicts", Value::Null).await?;
            print(json, &conflicts, |list| {
                list.as_array()
                    .into_iter()
                    .flatten()
                    .map(|c| {
                        let e = &c["copy"];
                        format!(
                            "{}:{} {}{}",
                            e["id"].as_str().unwrap_or_default(),
                            e["revision"].as_str().unwrap_or_default(),
                            e["item"]["title"].as_str().unwrap_or_default(),
                            if e["deleted"] == true {
                                tr!(" (deleted)", "（已删除）")
                            } else if c["hidden"].as_array().is_some_and(|h| !h.is_empty()) {
                                tr!(" (hidden values differ)", "（隐藏的值不同）")
                            } else {
                                String::new()
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        VaultCommand::Resolve { key, keep_current } => {
            let keep = if keep_current { "current" } else { "theirs" };
            let entry = remote
                .call("vault/resolve", json!({"key": key, "keep": keep}))
                .await?;
            print(json, &entry, |v| {
                let revision = v["revision"].as_str().unwrap_or_default();
                tr!("resolved; head {revision}", "已解决；当前版本 {revision}")
            });
        }
        VaultCommand::Import { path, yes, all } => {
            let path = absolute(&path)?;
            let preview = remote
                .call("vault/import_preview", json!({"path": path}))
                .await?;
            let items = preview["items"].as_array().cloned().unwrap_or_default();
            let picks: Vec<u64> = items
                .iter()
                .filter(|i| all || i["duplicate"] != true)
                .filter_map(|i| i["index"].as_u64())
                .collect();
            if !yes {
                print(json, &preview, |p| {
                    let mut lines: Vec<String> = items
                        .iter()
                        .map(|i| {
                            format!(
                                "{} [{}] {} {}{}",
                                i["index"],
                                i["type"].as_str().unwrap_or_default(),
                                i["title"].as_str().unwrap_or_default(),
                                i["username"].as_str().unwrap_or_default(),
                                if i["duplicate"] == true {
                                    tr!(" (already here)", "（已存在）")
                                } else {
                                    String::new()
                                }
                            )
                        })
                        .collect();
                    let (format, skipped) =
                        (p["format"].as_str().unwrap_or_default(), &p["skipped"]);
                    let (n, new) = (items.len(), picks.len());
                    lines.push(tr!(
                        "{format}: {n} items, {new} new, {skipped} left out. Add them with --yes.",
                        "{format}：{n} 个条目，{new} 个新条目，{skipped} 个未读入。加上 --yes 导入。"
                    ));
                    lines.join("\n")
                });
                return Ok(());
            }
            if interactive(json) {
                eprintln!(
                    "{}",
                    tr!(
                        "The export file still holds every secret in plain text; delete it once the import is done.",
                        "导出文件仍以明文保存全部秘密；导入完成后请删除它。"
                    )
                );
            }
            let added = remote
                .call(
                    "vault/import",
                    json!({"path": path, "digest": preview["digest"], "picks": picks}),
                )
                .await?;
            print(json, &added, |v| {
                let n = v["ids"].as_array().map_or(0, Vec::len);
                tr!("imported {n} items", "已导入 {n} 个条目")
            });
        }
        VaultCommand::Health => {
            let findings = remote.call("vault/health", Value::Null).await?;
            print(json, &findings, |list| {
                let lines: Vec<String> = list
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|f| {
                        let issue = match f["issue"].as_str().unwrap_or_default() {
                            "weak" => tr!("weak password", "弱密码"),
                            "reused" => tr!("reused password", "重复使用的密码"),
                            "insecure" => tr!("address without TLS", "未加密的地址"),
                            "expired" => tr!("expired", "已过期"),
                            "expiring" => tr!("expires soon", "即将过期"),
                            other => other.to_string(),
                        };
                        format!(
                            "{} {} {issue}",
                            f["id"].as_str().unwrap_or_default(),
                            f["field"].as_str().unwrap_or_default()
                        )
                    })
                    .collect();
                match lines.is_empty() {
                    true => tr!("nothing to fix", "没有需要处理的问题"),
                    false => lines.join("\n"),
                }
            });
        }
    }
    Ok(())
}
