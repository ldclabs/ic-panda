//! `dmsg`: the dMsg command line client. `dmsg daemon` runs the daemon that
//! holds every key and all state; every other command asks it over loopback.

mod daemon;
mod home;
mod remote;

use clap::{Parser, Subcommand};
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
    /// Unlock with the current login.
    Unlock,
    /// Lock: forget the login and every key.
    Lock,
    /// The account.
    #[command(subcommand)]
    Account(AccountCommand),
    /// Devices of the account.
    #[command(subcommand)]
    Device(DeviceCommand),
    /// The vault.
    #[command(subcommand)]
    Vault(VaultCommand),
    /// Synchronize the vault with the relay.
    Sync,
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

#[derive(Subcommand)]
enum VaultCommand {
    /// Add an item; the secret is read from standard input with --secret-stdin.
    Add {
        /// Title.
        #[arg(long)]
        title: String,
        /// note, login, api or key.
        #[arg(long = "type", default_value = "note")]
        kind: String,
        /// Body text.
        #[arg(long, default_value = "")]
        body: String,
        /// Login name.
        #[arg(long, default_value = "")]
        username: String,
        /// Related URL.
        #[arg(long, default_value = "")]
        url: String,
        /// Tags.
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Read the secret field from standard input.
        #[arg(long)]
        secret_stdin: bool,
    },
    /// List items; secrets stay hidden.
    List,
    /// Show one item.
    Show {
        /// Item ID.
        id: String,
        /// Include the secret field.
        #[arg(long)]
        reveal: bool,
    },
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
        "state: {}\naccount: {}\ndevice: {}\nlogin: {}\nroot generation: {}\npending sync: {}",
        field("state"),
        field("account"),
        field("device_id"),
        field("principal"),
        status["root_generation"]
            .as_u64()
            .map_or("-".into(), |g| g.to_string()),
        status["pending_sync"]
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
        Command::Unlock => {
            let status = remote.call("session/unlock", Value::Null).await?;
            print(json, &status, summary);
        }
        Command::Lock => {
            let status = remote.call("session/lock", Value::Null).await?;
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
        Command::Vault(VaultCommand::Add {
            title,
            kind,
            body,
            username,
            url,
            tags,
            secret_stdin,
        }) => {
            let mut secret = String::new();
            if secret_stdin {
                if std::io::stdin().is_terminal() {
                    eprintln!("Secret (end with Ctrl-D):");
                }
                std::io::stdin()
                    .read_to_string(&mut secret)
                    .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
                secret = secret.trim_end_matches(['\r', '\n']).to_string();
            }
            let now = dmsg_client::now_ms();
            let item = json!({
                "type": kind, "title": title, "tags": tags, "body": body,
                "username": username, "secret": secret, "url": url,
                "favorite": false, "createdAt": now, "updatedAt": now,
            });
            let entry = remote.call("vault/put", json!({"item": item})).await?;
            print(json, &entry, |v| {
                v["id"].as_str().unwrap_or_default().to_string()
            });
        }
        Command::Vault(VaultCommand::List) => {
            let entries = remote.call("vault/list", Value::Null).await?;
            print(json, &entries, |list| {
                list.as_array()
                    .into_iter()
                    .flatten()
                    .map(|e| {
                        format!(
                            "{} {}{}",
                            e["id"].as_str().unwrap_or_default(),
                            e["item"]["title"].as_str().unwrap_or_default(),
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
        Command::Vault(VaultCommand::Show { id, reveal }) => {
            let entry = remote
                .call("vault/show", json!({"id": id, "reveal": reveal}))
                .await?;
            print(json, &entry, pretty);
        }
        Command::Sync => {
            let report = remote.call("sync/run", Value::Null).await?;
            print(json, &report, |r| {
                format!(
                    "sent {}, {} revisions through {}",
                    r["sent"], r["revisions"], r["through"]
                )
            });
        }
    }
    Ok(())
}
