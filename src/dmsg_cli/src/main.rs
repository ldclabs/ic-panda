//! `dmsg`: the dMsg command line client. `dmsg daemon` runs the daemon that
//! holds every key and all state; every other command asks it over loopback.

mod daemon;
mod home;
mod remote;
mod secure;

use clap::{Args, Parser, Subcommand};
use home::Home;
use remote::{Failure, Remote};
use serde_json::{json, Value};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::time::Duration;

/// dMsg command line client.
#[derive(Parser)]
#[command(name = "dmsg", version, about)]
struct Cli {
    /// Home directory; defaults to $DMSG_HOME or ~/.dmsg.
    #[arg(long, global = true)]
    home: Option<PathBuf>,
    /// Print JSON for scripts and agents.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the daemon in the foreground.
    Daemon {
        /// Loopback port; 0 picks a free one.
        #[arg(long, default_value_t = 0)]
        port: u16,
    },
    /// Show the daemon and session status.
    Status,
    /// Stop the daemon.
    Stop,
    /// Log in with Internet Identity through the dMsg login page.
    Login {
        /// Derivation origin: dmsg.net or, for older logins, panda.fans.
        #[arg(long, default_value = "dmsg.net")]
        origin: String,
        /// Print the login URL instead of opening the browser.
        #[arg(long)]
        no_browser: bool,
    },
    /// Unlock with the current login, or with --fast without one.
    Unlock {
        /// Use fast unlock (up to seven days after a login unlock).
        #[arg(long)]
        fast: bool,
    },
    /// Lock: forget the login and every key.
    Lock,
    /// Fast unlock on this device, kept in the system credential store.
    #[command(subcommand)]
    FastUnlock(FastUnlockCommand),
    /// The account.
    #[command(subcommand)]
    Account(AccountCommand),
    /// Devices of the account.
    #[command(subcommand)]
    Device(DeviceCommand),
    /// The vault.
    #[command(subcommand)]
    Vault(VaultCommand),
    /// Encrypted files in the vault.
    #[command(subcommand)]
    File(FileCommand),
    /// Synchronize with the relay, or show where sync stands.
    Sync {
        #[command(subcommand)]
        command: Option<SyncCommand>,
    },
    /// The public profile.
    #[command(subcommand)]
    Profile(ProfileCommand),
    /// Erase this device's workspace and start over as a new device.
    Reset {
        /// Confirm: unsynced versions are lost.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum FastUnlockCommand {
    /// Enable fast unlock while unlocked.
    Enable,
    /// Disable fast unlock and delete its key.
    Disable,
}

#[derive(Subcommand)]
enum SyncCommand {
    /// Pending versions, conflicts and the latest sync.
    Status,
}

#[derive(Subcommand)]
enum FileCommand {
    /// Encrypt a file (at most 100 MiB) into the vault; an interrupted
    /// import of the same file resumes.
    Import {
        /// The file.
        path: PathBuf,
    },
    /// List files.
    List,
    /// Verify a file and write it to a new path.
    Get {
        /// Item ID.
        id: String,
        /// Where to write it; must not exist.
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Check every chunk of a file and its SHA-256.
    Verify {
        /// Item ID.
        id: String,
    },
}

#[derive(Subcommand)]
enum ProfileCommand {
    /// The published profile of this account.
    Show,
    /// Publish exactly the given fields; anything left out is unpublished.
    Publish {
        /// Display name.
        #[arg(long, default_value = "")]
        name: String,
        /// Bio.
        #[arg(long, default_value = "")]
        bio: String,
        /// HTTPS links.
        #[arg(long = "link")]
        links: Vec<String>,
    },
    /// Another account's published profile, checked against its devices.
    Resolve {
        /// Account ID.
        account: String,
    },
}

#[derive(Subcommand)]
enum AccountCommand {
    /// Create an account for the current login with this device as administrator.
    Create,
    /// Show the account and its devices.
    Show,
}

#[derive(Subcommand)]
enum DeviceCommand {
    /// List the account's devices.
    List,
    /// Ask to join the login's account; prints a request for an administrator device.
    PairRequest {
        /// Join as a member instead of an administrator.
        #[arg(long)]
        member: bool,
    },
    /// Approve a pairing request and rotate the content root to include the device.
    Approve {
        /// The request printed by `dmsg device pair-request`.
        request: String,
        /// The device ID shown on the new device.
        #[arg(long)]
        device: String,
    },
}

#[derive(Args)]
struct ItemFields {
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
enum VaultCommand {
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
        eprintln!("Secret (end with Ctrl-D):");
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
                    ""
                } else {
                    " (not synced)"
                },
                if e["conflict"] == true {
                    format!(
                        " (conflict copy {})",
                        e["revision"].as_str().unwrap_or_default()
                    )
                } else {
                    String::new()
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// An absolute path for the daemon, which does not share this directory.
fn absolute(path: PathBuf) -> Result<String, Failure> {
    std::path::absolute(&path)
        .map(|p| p.display().to_string())
        .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))
}

fn main() {
    let cli = Cli::parse();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let json = cli.json;
    let code = match runtime.block_on(run(cli)) {
        Ok(()) => 0,
        Err(failure) => {
            if json {
                println!(
                    "{}",
                    json!({"error": {"code": failure.code, "message": failure.message}})
                );
            } else {
                eprintln!("error: {} ({})", failure.message, failure.code);
            }
            1
        }
    };
    std::process::exit(code);
}

fn print(json: bool, value: &Value, human: impl FnOnce(&Value) -> String) {
    if json {
        println!("{value}");
    } else {
        println!("{}", human(value));
    }
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

fn summary(status: &Value) -> String {
    let field = |name: &str| status[name].as_str().unwrap_or("-").to_string();
    format!(
        "state: {}\naccount: {}\ndevice: {}\nlogin: {}\nroot generation: {}\npending sync: {}\nconflicts: {}{}",
        field("state"),
        field("account"),
        field("device_id"),
        field("principal"),
        status["root_generation"]
            .as_u64()
            .map_or("-".into(), |g| g.to_string()),
        status["pending_sync"],
        status["conflicts"],
        if status["resident"] == true {
            "\nbackground online: the device key stays while locked"
        } else {
            ""
        }
    )
}

fn open_browser(url: &str) -> bool {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    command.arg(url).status().is_ok_and(|s| s.success())
}

async fn run(cli: Cli) -> Result<(), Failure> {
    let home = Home::resolve(cli.home);
    let json = cli.json;
    let Some(command) = cli.command else {
        return match Remote::connect(&home) {
            Ok(remote) => {
                let status = remote.status().await?;
                print(json, &status, |s| {
                    format!(
                        "{}\n\nCommands: dmsg login | unlock | lock | vault | sync | --help",
                        summary(s)
                    )
                });
                Ok(())
            }
            Err(failure) => Err(failure),
        };
    };
    if let Command::Daemon { port } = command {
        return daemon::run(home, port)
            .await
            .map_err(|e| Failure::new("UNAVAILABLE", e.to_string()));
    }
    if let Command::Status = command {
        return match Remote::connect(&home) {
            Ok(remote) => {
                let status = remote.status().await?;
                print(json, &status, summary);
                Ok(())
            }
            Err(failure) if failure.code == "DAEMON_STOPPED" && json => {
                println!("{}", json!({"running": false}));
                std::process::exit(3);
            }
            Err(failure) => Err(failure),
        };
    }
    let remote = Remote::connect(&home)?;
    match command {
        Command::Daemon { .. } | Command::Status => unreachable!(),
        Command::Stop => {
            remote.call("daemon/stop", Value::Null).await?;
            print(json, &json!({"stopped": true}), |_| "stopped".into());
        }
        Command::Login { origin, no_browser } => {
            let origin = match origin.as_str() {
                "dmsg.net" | "https://dmsg.net" => "https://dmsg.net",
                "panda.fans" | "https://panda.fans" => "https://panda.fans",
                _ => {
                    return Err(Failure::new(
                        "INVALID_INPUT",
                        "origin must be dmsg.net or panda.fans",
                    ))
                }
            };
            let started = remote
                .call("session/login", json!({"origin": origin}))
                .await?;
            let url = started["url"].as_str().unwrap_or_default();
            if json {
                println!("{}", json!({"url": url}));
            } else {
                println!("{url}");
                eprintln!("Waiting for the login in your browser…");
            }
            if !no_browser && !open_browser(url) {
                eprintln!("Open the URL above in your browser.");
            }
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let result = remote.call("session/login_result", Value::Null).await?;
                if result["pending"] == true {
                    continue;
                }
                if let Some(error) = result["error"].as_str() {
                    return Err(Failure::new("AUTH_REQUIRED", error));
                }
                if result["status"]["principal"].is_null() {
                    return Err(Failure::new("EXPIRED", "the login was not completed"));
                }
                print(json, &result["status"], summary);
                break;
            }
        }
        Command::Unlock { fast } => {
            let status = remote.call("session/unlock", json!({"fast": fast})).await?;
            print(json, &status, summary);
        }
        Command::Lock => {
            let status = remote.call("session/lock", Value::Null).await?;
            print(json, &status, summary);
        }
        Command::FastUnlock(command) => {
            let enable = matches!(command, FastUnlockCommand::Enable);
            let status = remote
                .call("session/fast_unlock", json!({"enable": enable}))
                .await?;
            print(json, &status, summary);
        }
        Command::Reset { yes } => {
            if !yes {
                return Err(Failure::new(
                    "INVALID_INPUT",
                    "reset erases this device's workspace; confirm with --yes",
                ));
            }
            let status = remote.status().await?;
            let status = remote
                .call("workspace/reset", json!({"device": status["device_id"]}))
                .await?;
            print(json, &status, summary);
        }
        Command::Account(AccountCommand::Create) => {
            let created = remote.call("account/create", Value::Null).await?;
            print(json, &created, |v| {
                format!(
                    "account: {}\nroot generation: {}",
                    v["account"].as_str().unwrap_or_default(),
                    v["root_generation"]
                )
            });
        }
        Command::Account(AccountCommand::Show) => {
            let view = remote.call("account/show", Value::Null).await?;
            print(json, &view, pretty);
        }
        Command::Device(DeviceCommand::List) => {
            let view = remote.call("account/show", Value::Null).await?;
            print(json, &view["devices"], |devices| {
                devices
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|d| {
                        format!(
                            "{}{} {} {}{}",
                            if d["current"] == true { "* " } else { "  " },
                            d["device_id"].as_str().unwrap_or_default(),
                            d["role"].as_str().unwrap_or_default(),
                            d["capabilities"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(","),
                            if d["revoked_at"].is_null() {
                                ""
                            } else {
                                " revoked"
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Command::Device(DeviceCommand::PairRequest { member }) => {
            let role = if member { "member" } else { "administrator" };
            let request = remote
                .call("device/pair_request", json!({"role": role}))
                .await?;
            print(json, &request, |v| {
                format!(
                    "device: {}\n\nOn an administrator device run:\n  dmsg device approve {} --device {}",
                    v["device_id"].as_str().unwrap_or_default(),
                    v["token"].as_str().unwrap_or_default(),
                    v["device_id"].as_str().unwrap_or_default()
                )
            });
        }
        Command::Device(DeviceCommand::Approve { request, device }) => {
            let approved = remote
                .call(
                    "device/approve",
                    json!({"token": request, "device": device}),
                )
                .await?;
            print(json, &approved, |v| {
                format!(
                    "approved {}\nroot generation: {}",
                    v["device_id"].as_str().unwrap_or_default(),
                    v["root_generation"]
                )
            });
        }
        Command::Vault(VaultCommand::Add { kind, fields }) => {
            let title = fields
                .title
                .clone()
                .ok_or_else(|| Failure::new("INVALID_INPUT", "--title is required"))?;
            let secret = if fields.secret_stdin {
                read_secret()?
            } else {
                String::new()
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
        Command::Vault(VaultCommand::Edit { id, fields }) => {
            let changes = fields.changes()?;
            let entry = remote
                .call("vault/edit", json!({"id": id, "changes": changes}))
                .await?;
            print(json, &entry, |v| {
                let conflict = if v["conflict"] == true {
                    " (kept as a conflict copy)"
                } else {
                    ""
                };
                format!("{}{conflict}", v["revision"].as_str().unwrap_or_default())
            });
        }
        Command::Vault(VaultCommand::Rm { id }) => {
            let entry = remote.call("vault/delete", json!({"id": id})).await?;
            print(json, &entry, |_| "moved to the trash".into());
        }
        Command::Vault(VaultCommand::Restore { id }) => {
            let entry = remote.call("vault/restore", json!({"id": id})).await?;
            print(json, &entry, |_| "restored".into());
        }
        Command::Vault(VaultCommand::List { trash }) => {
            let entries = remote.call("vault/list", json!({"trash": trash})).await?;
            print(json, &entries, entry_lines);
        }
        Command::Vault(VaultCommand::Search { query }) => {
            let entries = remote.call("vault/list", json!({"query": query})).await?;
            print(json, &entries, entry_lines);
        }
        Command::Vault(VaultCommand::Show { id, reveal }) => {
            let entry = remote
                .call("vault/show", json!({"id": id, "reveal": reveal}))
                .await?;
            print(json, &entry, pretty);
        }
        Command::Vault(VaultCommand::Conflicts) => {
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
                                " (deleted)"
                            } else {
                                ""
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Command::Vault(VaultCommand::Resolve { key, keep_current }) => {
            let keep = if keep_current { "current" } else { "theirs" };
            let entry = remote
                .call("vault/resolve", json!({"key": key, "keep": keep}))
                .await?;
            print(json, &entry, |v| {
                format!(
                    "resolved; head {}",
                    v["revision"].as_str().unwrap_or_default()
                )
            });
        }
        Command::File(FileCommand::Import { path }) => {
            let entry = remote
                .call("file/import", json!({"path": absolute(path)?}))
                .await?;
            print(json, &entry, |v| {
                v["id"].as_str().unwrap_or_default().to_string()
            });
        }
        Command::File(FileCommand::List) => {
            let entries = remote.call("vault/list", Value::Null).await?;
            let files: Vec<Value> = entries
                .as_array()
                .into_iter()
                .flatten()
                .filter(|e| e["item"]["type"] == "file")
                .cloned()
                .collect();
            print(json, &Value::Array(files), |list| {
                list.as_array()
                    .into_iter()
                    .flatten()
                    .map(|e| {
                        format!(
                            "{} {} {} bytes{}",
                            e["id"].as_str().unwrap_or_default(),
                            e["item"]["file"]["name"].as_str().unwrap_or_default(),
                            e["item"]["file"]["size"],
                            if e["synced"] == true {
                                ""
                            } else {
                                " (not synced)"
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Command::File(FileCommand::Get { id, out }) => {
            let manifest = remote
                .call("file/export", json!({"id": id, "path": absolute(out)?}))
                .await?;
            print(json, &manifest, |m| {
                format!(
                    "verified {} ({} bytes)",
                    m["sha256"].as_str().unwrap_or_default(),
                    m["size"]
                )
            });
        }
        Command::File(FileCommand::Verify { id }) => {
            let manifest = remote.call("file/verify", json!({"id": id})).await?;
            print(json, &manifest, |m| {
                format!(
                    "verified {} ({} bytes)",
                    m["sha256"].as_str().unwrap_or_default(),
                    m["size"]
                )
            });
        }
        Command::Sync { command: None } => {
            let report = remote.call("sync/run", Value::Null).await?;
            print(json, &report, |r| {
                format!(
                    "sent {}, refused {}, {} revisions through {}; {} pending, {} conflicts",
                    r["sent"],
                    r["blocked"],
                    r["revisions"],
                    r["through"],
                    r["pending"],
                    r["conflicts"]
                )
            });
        }
        Command::Sync {
            command: Some(SyncCommand::Status),
        } => {
            let status = remote.call("sync/status", Value::Null).await?;
            print(json, &status, pretty);
        }
        Command::Profile(ProfileCommand::Show) => {
            let profile = remote.call("profile/show", Value::Null).await?;
            print(json, &profile, pretty);
        }
        Command::Profile(ProfileCommand::Publish { name, bio, links }) => {
            let profile = remote
                .call(
                    "profile/publish",
                    json!({"display_name": name, "bio": bio, "links": links}),
                )
                .await?;
            print(json, &profile, pretty);
        }
        Command::Profile(ProfileCommand::Resolve { account }) => {
            let profile = remote
                .call("profile/resolve", json!({"account": account}))
                .await?;
            print(json, &profile, pretty);
        }
    }
    Ok(())
}
