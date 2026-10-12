//! `dmsg`: the dMsg command line client. `dmsg daemon` runs the daemon that
//! holds every key and all state; every other command asks it over loopback.

mod agent_commands;
mod agents;
mod channels;
mod commands;
mod daemon;
mod hints;
mod home;
mod mcp;
mod methods;
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
    /// The approval center: requests from this device, agents and web pages.
    #[command(subcommand)]
    Approvals(ApprovalsCommand),
    /// Formal statements signed by this device, and their verification.
    #[command(subcommand)]
    Attest(AttestCommand),
    /// Web applications through dmsg-connect/1.
    #[command(subcommand)]
    Connect(ConnectCommand),
    /// Formal channels: end-to-end encrypted group conversations.
    #[command(subcommand)]
    Channel(commands::ChannelCommand),
    /// The inbox: contact policy and letters, free or paid.
    #[command(subcommand)]
    Inbox(commands::InboxCommand),
    /// The wallet that pays: the login, or an identity PEM file.
    #[command(subcommand)]
    Wallet(commands::WalletCommand),
    /// Plans and membership: catalog, status and checkouts.
    #[command(subcommand)]
    Plan(commands::PlanCommand),
    /// Names: resolve, price, claim and register.
    #[command(subcommand)]
    Handle(commands::HandleCommand),
    /// Agent Delegation with this account as the principal, and the Agent
    /// IDs this device hosts.
    #[command(subcommand)]
    Agent(agent_commands::AgentCommand),
    /// Capabilities hosted Agent IDs hold over this account.
    #[command(subcommand)]
    Grants(agent_commands::GrantsCommand),
    /// Run one MCP tool as a hosted Agent ID; `dmsg ap tools` lists them.
    Ap {
        /// Label or `did:agent:…`; the active Agent ID by default.
        #[arg(long)]
        agent: Option<String>,
        /// Tool name, with or without its `agent_protocols_`/`dmsg_` prefix.
        tool: String,
        /// Tool input as JSON.
        input: Option<String>,
    },
    /// Serve MCP on stdio for the Agent ID whose credential is in
    /// DMSG_AGENT_TOKEN.
    Mcp,
    /// The daemon's address and owner credential, for the desktop app.
    #[command(hide = true)]
    AppToken,
}

#[derive(Subcommand)]
enum ApprovalsCommand {
    /// Requests waiting for a decision, or every request with --all.
    List {
        /// Include decided requests.
        #[arg(long)]
        all: bool,
    },
    /// Everything a decision needs.
    Show {
        /// Request ID or a unique prefix.
        id: String,
    },
    /// Approve after reviewing it; interactive terminals only.
    Approve {
        /// Request ID or a unique prefix.
        id: String,
    },
    /// Reject; interactive terminals only.
    Reject {
        /// Request ID or a unique prefix.
        id: String,
    },
    /// Withdraw a request this device's own clients made.
    Cancel {
        /// Request ID or a unique prefix.
        id: String,
    },
}

#[derive(Args)]
struct StatementFields {
    /// What the statement is about (a URI or text).
    #[arg(long)]
    subject: Option<String>,
    /// Media type of the original.
    #[arg(long)]
    content_type: Option<String>,
    /// URI of the original.
    #[arg(long)]
    location: Option<String>,
}

#[derive(Subcommand)]
enum AttestCommand {
    /// Sign a text statement.
    Text {
        /// The text, 1 to 4,096 bytes.
        text: String,
        /// What the text is about.
        #[arg(long)]
        subject: Option<String>,
    },
    /// Sign the SHA-256 of a file, or of a digest given in hex.
    Digest {
        /// The file.
        #[arg(long, conflicts_with = "sha256")]
        file: Option<PathBuf>,
        /// The digest, when no file is given.
        #[arg(long)]
        sha256: Option<String>,
        #[command(flatten)]
        fields: StatementFields,
    },
    /// Sign a statement about one file.
    File {
        /// The file.
        path: PathBuf,
        /// The statement.
        #[arg(long)]
        text: String,
        #[command(flatten)]
        fields: StatementFields,
    },
    /// Finished and pending attestations.
    History,
    /// Write a finished attestation as a self-contained bundle.
    Export {
        /// Execution ID.
        execution: String,
        /// Where to write it; must not exist.
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Verify a bundle without the daemon: signature, certified receipt,
    /// their binding, the file, and the device's present status.
    Verify {
        /// The bundle.
        bundle: PathBuf,
        /// The original file of a digest or file statement.
        #[arg(long)]
        file: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ConnectCommand {
    /// Browsers paired with the account.
    #[command(subcommand)]
    Browsers(BrowsersCommand),
    /// Web requests in the approval center.
    Requests,
    /// Fetch and check web requests now.
    Sync,
}

#[derive(Subcommand)]
enum BrowsersCommand {
    /// Pair a browser: open the pairing page and confirm its name here.
    Add {
        /// Print the link instead of opening the default browser.
        #[arg(long)]
        copy_link: bool,
    },
    /// Paired browsers.
    List,
    /// Revoke a pairing.
    Revoke {
        /// Pairing ID.
        pairing: String,
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
    /// Replace a device's capabilities; goes through the approval center.
    Capabilities {
        /// Device ID.
        device: String,
        /// The complete set, comma separated: content_sign, vault_unlock,
        /// root_manage, formal_approve, payment_offer.
        #[arg(long, value_delimiter = ',')]
        set: Vec<String>,
    },
    /// Let this device approve formal signatures and web requests, or stop.
    FormalApprove {
        /// on or off.
        state: String,
    },
    /// Revoke another device and rotate the content root without it.
    Revoke {
        /// Device ID.
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

/// Whether a person sits at this terminal: both ends are terminals.
fn interactive(json: bool) -> bool {
    !json && std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// One line typed at the terminal.
fn ask(prompt: &str) -> String {
    use std::io::Write;
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    line.trim().to_string()
}

/// A request rendered for review.
fn render(request: &Value) -> String {
    let field = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Null => "-".into(),
        other => other.to_string(),
    };
    let d = &request["detail"];
    let mut lines = vec![
        format!("request   {}", field(&request["id"])),
        format!("from      {}", field(&request["source"])),
        format!("state     {}", field(&request["state"])),
        format!("expires   {}", field(&request["expires_at"])),
    ];
    match request["type"].as_str() {
        Some("attest") => {
            let s = &d["statement"];
            lines.push(format!(
                "sign      {} statement as {}",
                field(&s["profile"]),
                field(&s["issuer"])
            ));
            lines.push(format!("origin    {}", field(&d["origin"])));
            if !s["subject"].is_null() {
                lines.push(format!("subject   {}", field(&s["subject"])));
            }
            for name in ["text", "sha256", "content_type", "location"] {
                if !s["content"][name].is_null() {
                    lines.push(format!("{name:<9} {}", field(&s["content"][name])));
                }
            }
            match (d["file"].as_str(), d["file_checked"].as_bool()) {
                (Some(path), Some(true)) => {
                    lines.push(format!("file      {path} (digest matches)"))
                }
                (Some(path), _) => lines.push(format!("file      {path} (CHANGED or unreadable)")),
                (None, _) if !s["content"]["sha256"].is_null() => lines.push(
                    "file      only the digest is confirmed; the file was not checked".into(),
                ),
                _ => {}
            }
        }
        Some("device_revoke") => {
            lines.push(format!(
                "revoke    device {}{}; the content root is rotated without it",
                field(&d["device_id"]),
                if d["this_device"] == true {
                    " (this device)"
                } else {
                    ""
                }
            ));
        }
        Some("device_capabilities") => {
            lines.push(format!(
                "change    capabilities of device {}{}",
                field(&d["device_id"]),
                if d["this_device"] == true {
                    " (this device)"
                } else {
                    ""
                }
            ));
            lines.push(format!("to        {}", field(&d["capabilities"])));
        }
        Some("principal") => {
            lines.push(format!(
                "principal {} — an account change, published by the user home",
                field(&d["change"])
            ));
            for name in [
                "principal_type",
                "generation",
                "name",
                "delegation",
                "invalid_from",
            ] {
                if !d[name].is_null() {
                    lines.push(format!(
                        "{:<9} {}",
                        name.replace("principal_", ""),
                        field(&d[name])
                    ));
                }
            }
        }
        Some("delegation") => {
            let k = &d["kind"];
            lines.push(format!(
                "sign      delegation.{} with controller #{}",
                field(&k["type"]),
                field(&d["generation"])
            ));
            for name in [
                "id",
                "subject",
                "scopes",
                "audiences",
                "expires_at",
                "relationship",
                "reason",
            ] {
                if !k[name].is_null() {
                    lines.push(format!("{name:<9} {}", field(&k[name])));
                }
            }
            lines.push("          relying parties may act on a grant before its revocation".into());
        }
        Some("connect") => {
            lines.push(format!(
                "web app   {} (version {})",
                field(&d["app_id"]),
                field(&d["app_version"])
            ));
            lines.push(format!("origin    {}", field(&d["origin"])));
            lines.push(
                "          the origin is relayed by your browser through connect.dmsg.net".into(),
            );
            lines.push(format!("browser   {}", field(&d["pairing_label"])));
            lines.push(format!("account   {}", field(&d["account"])));
            lines.push(format!(
                "method    {} ({})",
                field(&d["method"]),
                field(&d["kind"])
            ));
            lines.push(format!("requested {}", field(&d["requested_at"])));
            lines.push(format!(
                "content   {}",
                serde_json::to_string_pretty(&d["content"]).unwrap_or_default()
            ));
        }
        _ => lines.push(format!(
            "detail    {}",
            serde_json::to_string_pretty(d).unwrap_or_default()
        )),
    }
    if !request["error"].is_null() {
        lines.push(format!("error     {}", field(&request["error"])));
    }
    lines.join("\n")
}

/// The full ID of a request given by a unique prefix.
async fn request_id(remote: &Remote, prefix: &str) -> Result<String, Failure> {
    let list = remote.call("approval/list", json!({"all": true})).await?;
    let found: Vec<&str> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["id"].as_str())
        .filter(|id| id.starts_with(prefix))
        .collect();
    match found.as_slice() {
        [id] if prefix.len() >= 6 => Ok(id.to_string()),
        [] => Err(Failure::new("NOT_FOUND", "no such request")),
        _ => Err(Failure::new("INVALID_INPUT", "give more of the request ID")),
    }
}

/// Show a request and let the person at the terminal decide it.
async fn decide(remote: &Remote, request: &Value, approve: Option<bool>) -> Result<Value, Failure> {
    println!("{}", render(request));
    let approve = match approve {
        Some(false) => false,
        _ => match ask("\nApprove this request? Type yes to approve, no to reject: ").as_str() {
            "yes" => true,
            "no" => false,
            _ => {
                return Err(Failure::new(
                    "CANCELLED",
                    "no decision; the request still waits",
                ))
            }
        },
    };
    remote
        .call(
            "approval/respond",
            json!({"id": request["id"], "digest": request["digest"], "approve": approve, "human": true}),
        )
        .await
}

/// A request just created: decided here at an interactive terminal, else
/// left waiting in the approval center.
async fn created(remote: &Remote, json: bool, request: Value) -> Result<(), Failure> {
    if !interactive(json) {
        print(json, &request, |r| {
            format!(
                "request {} waits in the approval center: dmsg approvals approve {}",
                r["id"].as_str().unwrap_or_default(),
                r["id"].as_str().unwrap_or_default()
            )
        });
        return Ok(());
    }
    let decided = decide(remote, &request, None).await?;
    println!(
        "{}{}",
        decided["state"].as_str().unwrap_or_default(),
        decided["execution"]
            .as_str()
            .map(|e| format!("; execution {e}"))
            .unwrap_or_default()
    );
    Ok(())
}

fn approval_lines(list: &Value) -> String {
    list.as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            format!(
                "{} {:<13} {:<20} {}",
                &r["id"].as_str().unwrap_or_default()[..12],
                r["state"].as_str().unwrap_or_default(),
                r["type"].as_str().unwrap_or_default(),
                r["source"].as_str().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
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
    if let Command::Attest(AttestCommand::Verify { bundle, file }) = &command {
        let settings = home
            .settings()
            .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
        let text = std::fs::read(bundle).map_err(|e| Failure::new("NOT_FOUND", e.to_string()))?;
        let bundle: dmsg_client::attest::Bundle = serde_json::from_slice(&text)
            .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
        let ic = dmsg_client::ic::Ic::anonymous(&settings.config)
            .await
            .map_err(|e| Failure::new(&dmsg_client::error_code(&e), e.to_string()))?;
        let report = dmsg_client::attest::verify_online(&ic, &bundle, file.as_deref())
            .await
            .map_err(|e| Failure::new(&dmsg_client::error_code(&e), e.to_string()))?;
        let report = serde_json::to_value(&report).unwrap_or_default();
        print(json, &report, |r| {
            format!(
                "content     {}\nsignature   valid (key {})\nauthorized  by account {} with device {} at {} (epoch {})\norigin      {}\ncertified   {}\ndevice now  {}\nstatement   {}",
                r["content"].as_str().unwrap_or_default(),
                r["key_fingerprint"].as_str().unwrap_or_default(),
                r["account"].as_str().unwrap_or_default(),
                r["device_id"].as_str().unwrap_or_default(),
                r["approved_at"],
                r["security_epoch"],
                r["origin"].as_str().unwrap_or_default(),
                r["certified_at"],
                r["current"].as_str().unwrap_or_default(),
                serde_json::to_string_pretty(&r["statement"]).unwrap_or_default()
            )
        });
        return Ok(());
    }
    if let Command::Mcp = command {
        return mcp::run(&home).await;
    }
    if let Command::AppToken = command {
        let discovery = home
            .discovery()
            .ok_or_else(|| Failure::new("DAEMON_STOPPED", "the dmsg daemon is not running"))?;
        let settings = home
            .settings()
            .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
        let token = home
            .owner_token(&settings, false)
            .map_err(|e| Failure::new("AUTH_REQUIRED", e.to_string()))?;
        println!(
            "{}",
            json!({"url": format!("http://127.0.0.1:{}", discovery.port), "token": token})
        );
        return Ok(());
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
        Command::Daemon { .. } | Command::Status | Command::AppToken | Command::Mcp => {
            unreachable!()
        }
        Command::Attest(AttestCommand::Verify { .. }) => unreachable!(),
        Command::Approvals(ApprovalsCommand::List { all }) => {
            let list = remote.call("approval/list", json!({"all": all})).await?;
            print(json, &list, approval_lines);
        }
        Command::Approvals(ApprovalsCommand::Show { id }) => {
            let id = request_id(&remote, &id).await?;
            let request = remote.call("approval/show", json!({"id": id})).await?;
            print(json, &request, render);
        }
        Command::Approvals(ApprovalsCommand::Approve { id } | ApprovalsCommand::Reject { id })
            if !interactive(json) =>
        {
            let _ = id;
            return Err(Failure::new(
                "FORBIDDEN",
                "approvals are decided by a person: run this in an interactive terminal without --json, or use the desktop approval window",
            ));
        }
        Command::Approvals(ApprovalsCommand::Approve { id }) => {
            let id = request_id(&remote, &id).await?;
            let request = remote.call("approval/show", json!({"id": id})).await?;
            let decided = decide(&remote, &request, None).await?;
            println!("{}", decided["state"].as_str().unwrap_or_default());
        }
        Command::Approvals(ApprovalsCommand::Reject { id }) => {
            let id = request_id(&remote, &id).await?;
            let request = remote.call("approval/show", json!({"id": id})).await?;
            let decided = decide(&remote, &request, Some(false)).await?;
            println!("{}", decided["state"].as_str().unwrap_or_default());
        }
        Command::Approvals(ApprovalsCommand::Cancel { id }) => {
            let id = request_id(&remote, &id).await?;
            let request = remote.call("approval/cancel", json!({"id": id})).await?;
            print(json, &request, |r| {
                r["state"].as_str().unwrap_or_default().to_string()
            });
        }
        Command::Attest(AttestCommand::Text { text, subject }) => {
            let request = remote
                .call(
                    "attest/request",
                    json!({"statement": {"profile": "text", "text": text, "subject": subject}}),
                )
                .await?;
            created(&remote, json, request).await?;
        }
        Command::Attest(AttestCommand::Digest {
            file,
            sha256,
            fields,
        }) => {
            let request = remote
                .call(
                    "attest/request",
                    json!({
                        "statement": {
                            "profile": "digest", "sha256": sha256,
                            "content_type": fields.content_type, "location": fields.location,
                            "subject": fields.subject,
                        },
                        "file": file.map(absolute).transpose()?,
                    }),
                )
                .await?;
            created(&remote, json, request).await?;
        }
        Command::Attest(AttestCommand::File { path, text, fields }) => {
            let request = remote
                .call(
                    "attest/request",
                    json!({
                        "statement": {
                            "profile": "file_statement", "text": text,
                            "content_type": fields.content_type, "location": fields.location,
                            "subject": fields.subject,
                        },
                        "file": absolute(path)?,
                    }),
                )
                .await?;
            created(&remote, json, request).await?;
        }
        Command::Attest(AttestCommand::History) => {
            let list = remote.call("attest/history", Value::Null).await?;
            print(json, &list, |list| {
                list.as_array()
                    .into_iter()
                    .flatten()
                    .map(|a| {
                        format!(
                            "{} {:<9} {} {}",
                            a["execution"].as_str().unwrap_or_default(),
                            a["stage"].as_str().unwrap_or_default(),
                            a["statement"]["profile"].as_str().unwrap_or_default(),
                            a["origin"].as_str().unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Command::Attest(AttestCommand::Export { execution, out }) => {
            let bundle = remote
                .call("attest/export", json!({"execution": execution}))
                .await?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&out)
                .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
            use std::io::Write;
            file.write_all(
                serde_json::to_string_pretty(&bundle)
                    .unwrap_or_default()
                    .as_bytes(),
            )
            .map_err(|e| Failure::new("UNAVAILABLE", e.to_string()))?;
            print(json, &json!({"path": out.display().to_string()}), |v| {
                format!("wrote {}", v["path"].as_str().unwrap_or_default())
            });
        }
        Command::Device(DeviceCommand::Capabilities { device, set }) => {
            let request = remote
                .call(
                    "device/capabilities",
                    json!({"device": device, "capabilities": set}),
                )
                .await?;
            created(&remote, json, request).await?;
        }
        Command::Device(DeviceCommand::Revoke { device }) => {
            let request = remote
                .call("device/revoke", json!({"device": device}))
                .await?;
            created(&remote, json, request).await?;
        }
        Command::Device(DeviceCommand::FormalApprove { state }) => {
            let enable = match state.as_str() {
                "on" => true,
                "off" => false,
                _ => return Err(Failure::new("INVALID_INPUT", "on or off")),
            };
            let request = remote
                .call("device/formal_approve", json!({"enable": enable}))
                .await?;
            created(&remote, json, request).await?;
        }
        Command::Connect(ConnectCommand::Browsers(BrowsersCommand::Add { copy_link })) => {
            if !interactive(json) {
                return Err(Failure::new(
                    "FORBIDDEN",
                    "pairing a browser needs a person to confirm its name: run this in an interactive terminal",
                ));
            }
            let invite = remote.call("connect/invite", Value::Null).await?;
            let url = invite["url"].as_str().unwrap_or_default();
            if copy_link || !open_browser(url) {
                println!("Open this link in the browser to connect (valid ten minutes):\n{url}");
            } else {
                println!("Opened the pairing page in your browser.");
            }
            let invite_id = invite["invite_id"].as_str().unwrap_or_default();
            let deadline = invite["expires_at"].as_u64().unwrap_or_default();
            let mut seen = std::collections::BTreeSet::new();
            while dmsg_client::now_ms() < deadline {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let found = remote
                    .call("connect/candidates", json!({"invite": invite_id}))
                    .await?;
                for candidate in found["candidates"].as_array().into_iter().flatten() {
                    let pairing = candidate["pairing_id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    if !seen.insert(pairing.clone()) {
                        continue;
                    }
                    let label = candidate["label"].as_str().unwrap_or_default();
                    let answer = ask(&format!(
                        "Connect the browser \"{label}\" ({})? Type yes to connect: ",
                        &pairing[..12]
                    ));
                    if answer == "yes" {
                        let granted = remote
                            .call(
                                "connect/grant",
                                json!({"invite": invite_id, "pairing": pairing, "human": true}),
                            )
                            .await?;
                        println!(
                            "connected {} until {}",
                            granted["label"].as_str().unwrap_or_default(),
                            granted["expires_at"]
                        );
                        return Ok(());
                    }
                }
            }
            return Err(Failure::new("EXPIRED", "the pairing link expired"));
        }
        Command::Connect(ConnectCommand::Browsers(BrowsersCommand::List)) => {
            let list = remote.call("connect/pairings", Value::Null).await?;
            print(json, &list, |list| {
                list.as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| {
                        format!(
                            "{} {:<8} {} (by {})",
                            p["pairing_id"].as_str().unwrap_or_default(),
                            p["state"].as_str().unwrap_or_default(),
                            p["label"].as_str().unwrap_or("-"),
                            &p["issued_by"].as_str().unwrap_or("-")
                                [..p["issued_by"].as_str().map_or(1, |s| s.len().min(12))]
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Command::Connect(ConnectCommand::Browsers(BrowsersCommand::Revoke { pairing })) => {
            remote
                .call("connect/revoke", json!({"pairing": pairing}))
                .await?;
            print(json, &json!({"revoked": pairing}), |_| "revoked".into());
        }
        Command::Connect(ConnectCommand::Requests) => {
            let list = remote.call("approval/list", json!({"all": true})).await?;
            let web: Vec<Value> = list
                .as_array()
                .into_iter()
                .flatten()
                .filter(|r| r["type"] == "connect")
                .cloned()
                .collect();
            print(json, &Value::Array(web), approval_lines);
        }
        Command::Connect(ConnectCommand::Sync) => {
            let report = remote.call("connect/sync", Value::Null).await?;
            print(json, &report, |r| {
                format!(
                    "fetched {}, queued {}, refused {}, replied {}, sealed {}",
                    r["fetched"], r["queued"], r["refused"], r["replied"], r["sealed"]
                )
            });
        }
        Command::Channel(command) => commands::channel(&remote, json, command).await?,
        Command::Inbox(command) => commands::inbox(&remote, json, command).await?,
        Command::Wallet(command) => commands::wallet(&remote, json, command).await?,
        Command::Plan(command) => commands::plan(&remote, json, command).await?,
        Command::Handle(command) => commands::handle(&remote, json, command).await?,
        Command::Agent(command) => agent_commands::agent(&remote, json, command).await?,
        Command::Grants(command) => agent_commands::grants(&remote, json, command).await?,
        Command::Ap { agent, tool, input } => {
            agent_commands::ap(&remote, json, agent, tool, input).await?
        }
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
