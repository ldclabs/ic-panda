//! CLI commands of the vault: items, their trash and conflict copies.

use crate::locale::tr;
use crate::remote::{Failure, Remote};
use crate::{pretty, print};
use clap::{Args, Subcommand};
use serde_json::{json, Value};
use std::io::{IsTerminal, Read};

#[derive(Args)]
pub struct ItemFields {
    /// Title.
    #[arg(long)]
    title: Option<String>,
    /// Body text.
    #[arg(long)]
    body: Option<String>,
    /// Login name.
    #[arg(long)]
    username: Option<String>,
    /// Related URL.
    #[arg(long)]
    url: Option<String>,
    /// Tags; replaces the item's tags.
    #[arg(long = "tag")]
    tags: Vec<String>,
    /// Read the secret field from standard input.
    #[arg(long)]
    secret_stdin: bool,
}

#[derive(Subcommand)]
pub enum VaultCommand {
    /// Add an item; the secret is read from standard input with --secret-stdin.
    Add {
        /// note, login, api or key.
        #[arg(long = "type", default_value = "note")]
        kind: String,
        #[command(flatten)]
        fields: ItemFields,
    },
    /// Edit an item; only the given fields change.
    Edit {
        /// Item ID.
        id: String,
        #[command(flatten)]
        fields: ItemFields,
    },
    /// Move an item to the trash.
    Rm {
        /// Item ID.
        id: String,
    },
    /// Restore an item from the trash.
    Restore {
        /// Item ID.
        id: String,
    },
    /// List items; secrets stay hidden.
    List {
        /// Deleted items instead.
        #[arg(long)]
        trash: bool,
    },
    /// Search titles, tags, bodies, user names and URLs on this device.
    Search {
        /// Text to find.
        query: String,
    },
    /// Show one item.
    Show {
        /// Item ID.
        id: String,
        /// Include the secret field.
        #[arg(long)]
        reveal: bool,
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

impl ItemFields {
    /// The given fields as `vault/edit` changes.
    fn changes(self) -> Result<Value, Failure> {
        let mut changes = serde_json::Map::new();
        for (name, value) in [
            ("title", self.title),
            ("body", self.body),
            ("username", self.username),
            ("url", self.url),
        ] {
            if let Some(value) = value {
                changes.insert(name.into(), value.into());
            }
        }
        if !self.tags.is_empty() {
            changes.insert("tags".into(), json!(self.tags));
        }
        if self.secret_stdin {
            changes.insert("secret".into(), read_secret()?.into());
        }
        Ok(Value::Object(changes))
    }
}

fn entry_lines(list: &Value) -> String {
    list.as_array()
        .into_iter()
        .flatten()
        .map(|e| {
            format!(
                "{} {}{}{}",
                e["id"].as_str().unwrap_or_default(),
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
        VaultCommand::Add { kind, fields } => {
            let title = fields
                .title
                .ok_or_else(|| Failure::new("INVALID_INPUT", "--title is required"))?;
            let secret = match fields.secret_stdin {
                true => read_secret()?,
                false => String::new(),
            };
            let now = dmsg_client::now_ms();
            let item = json!({
                "type": kind, "title": title, "tags": fields.tags,
                "body": fields.body.unwrap_or_default(),
                "username": fields.username.unwrap_or_default(), "secret": secret,
                "url": fields.url.unwrap_or_default(),
                "favorite": false, "createdAt": now, "updatedAt": now,
            });
            let entry = remote.call("vault/put", json!({"item": item})).await?;
            print(json, &entry, |v| {
                v["id"].as_str().unwrap_or_default().to_string()
            });
        }
        VaultCommand::Edit { id, fields } => {
            let changes = fields.changes()?;
            let entry = remote
                .call("vault/edit", json!({"id": id, "changes": changes}))
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
        VaultCommand::Rm { id } => {
            let entry = remote.call("vault/delete", json!({"id": id})).await?;
            print(json, &entry, |_| tr!("moved to the trash", "已移到回收站"));
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
        VaultCommand::Show { id, reveal } => {
            let entry = remote
                .call("vault/show", json!({"id": id, "reveal": reveal}))
                .await?;
            print(json, &entry, pretty);
        }
        VaultCommand::Conflicts => {
            let entries = remote.call("vault/conflicts", Value::Null).await?;
            print(json, &entries, |list| {
                list.as_array()
                    .into_iter()
                    .flatten()
                    .map(|e| {
                        format!(
                            "{}:{} {}{}",
                            e["id"].as_str().unwrap_or_default(),
                            e["revision"].as_str().unwrap_or_default(),
                            e["item"]["title"].as_str().unwrap_or_default(),
                            if e["deleted"] == true {
                                tr!(" (deleted)", "（已删除）")
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
    }
    Ok(())
}
