//! `dmsg`: the dMsg command line client. `dmsg daemon` runs the daemon that
//! holds every key and all state; every other command asks it over loopback.

mod account_commands;
mod account_methods;
mod agent_commands;
mod agents;
mod autostart;
mod channels;
mod commands;
mod daemon;
mod doctor;
mod event_commands;
mod hints;
mod home;
mod legacy_commands;
mod legacy_methods;
mod lifecycle;
mod locale;
mod logs;
mod mcp;
mod mcp_events;
mod methods;
mod release;
mod remote;
mod secure;

use clap::{Args, Parser, Subcommand};
use home::Home;
use locale::{row, tr};
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
    /// Start the daemon in the background; see `dmsg logs` for its run log.
    Start,
    /// Stop the daemon after the running operation.
    Stop,
    /// Restart the daemon once no approved request, account update or root
    /// rotation is left unsettled.
    Restart {
        /// Restart even with work in flight; it resumes afterwards.
        #[arg(long)]
        interrupt: bool,
        /// Seconds to wait for work in flight to settle.
        #[arg(long, default_value_t = 60)]
        wait: u64,
    },
    /// Copy this binary to the shared location (`~/.local/bin`, or
    /// `%LOCALAPPDATA%\dMsg\bin`) unless the one there is newer or
    /// Homebrew's; the desktop app runs its bundled copy with this.
    Install {
        /// Install into this directory instead.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Update this binary to the latest release, verified by its SHA-256
    /// (and, on macOS, its signer), and restart a running daemon the way
    /// `restart` does.
    Update {
        /// Only report whether an update is available.
        #[arg(long)]
        check: bool,
        /// Install the latest release even when it is not newer.
        #[arg(long)]
        force: bool,
        /// Restart even with work in flight; it resumes afterwards.
        #[arg(long)]
        interrupt: bool,
        /// Seconds to wait for work in flight to settle.
        #[arg(long, default_value_t = 60)]
        wait: u64,
    },
    /// Start the daemon when this user logs in.
    #[command(subcommand)]
    Autostart(AutostartCommand),
    /// Check `config.toml` without starting anything.
    ValidateConfig,
    /// Check the configuration, credential store, permissions, daemon,
    /// relay, clock, registry and hint channel; exits 1 when one fails.
    Doctor,
    /// The newest lines of the daemon's run log, one file per day kept 30
    /// days; it names methods, outcomes and durations, never content.
    Logs {
        /// How many lines.
        #[arg(long, short = 'n', default_value_t = 100)]
        lines: usize,
    },
    /// Log in with Internet Identity through the dMsg login page; a login
    /// of another account selects that account's workspace.
    Login {
        /// Derivation origin: dmsg.net or, for older logins, panda.fans.
        #[arg(long, default_value = "dmsg.net")]
        origin: String,
        /// Print the login URL instead of opening the browser.
        #[arg(long)]
        no_browser: bool,
        /// Bind the login to the unlocked account instead of switching to
        /// it; goes through the approval center.
        #[arg(long)]
        bind: bool,
    },
    /// Unlock with the current login, or with --fast without one.
    Unlock {
        /// Use fast unlock (up to seven days after a login unlock).
        #[arg(long)]
        fast: bool,
        /// Select this account's workspace first.
        #[arg(long)]
        account: Option<String>,
    },
    /// The account workspaces on this device; each keeps its own data.
    Accounts,
    /// Select another account's workspace, locked.
    Switch {
        /// Account ID.
        account: String,
    },
    /// Lock: forget the login and every key.
    Lock,
    /// Fast unlock on this device, kept in the system credential store.
    #[command(subcommand)]
    FastUnlock(FastUnlockCommand),
    /// The account.
    #[command(subcommand)]
    Account(AccountCommand),
    /// Recovery after every device is lost.
    #[command(subcommand)]
    Recovery(account_commands::RecoveryCommand),
    /// Escrow transfers of paid letters.
    #[command(subcommand)]
    Payment(account_commands::PaymentCommand),
    /// Commerce and membership records the wallet may read and repair.
    #[command(subcommand)]
    Ops(account_commands::OpsCommand),
    /// Legacy migration: personal archives from the old site and shared
    /// old channels.
    #[command(subcommand)]
    Legacy(legacy_commands::LegacyCommand),
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
    /// Who did what on this device: grants, decisions and agents' calls.
    Audit {
        /// Only this actor: `owner`, or an Agent ID's label or `did:agent:…`.
        #[arg(long)]
        agent: Option<String>,
        /// Entries before this sequence, for the next page.
        #[arg(long)]
        before: Option<u64>,
        /// Most entries shown.
        #[arg(long, default_value_t = 50)]
        limit: u64,
    },
    /// Run one MCP tool as a hosted Agent ID; `dmsg ap tools` lists them.
    Ap {
        /// Label or `did:agent:…`; the active Agent ID by default.
        #[arg(long)]
        agent: Option<String>,
        /// The connector session, which keeps what this session was shown
        /// of each room between commands.
        #[arg(long, default_value = "cli")]
        session: String,
        /// Tool name, with or without its `agent_protocols_`/`dmsg_` prefix.
        tool: String,
        /// Tool input as JSON.
        input: Option<String>,
    },
    /// Serve MCP on stdio for the Agent ID whose credential is in
    /// DMSG_AGENT_TOKEN.
    Mcp,
    /// MCP Events: webhook endpoints on the relay and their deliveries.
    #[command(subcommand)]
    Events(event_commands::EventsCommand),
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
    /// Quote a web application's checkout before deciding it: cash on a
    /// ledger, or a PANDA waiver backed by a neuron.
    Quote {
        /// Request ID or a unique prefix.
        id: String,
        /// Ledger Principal (cash) or neuron ID in hex (PANDA).
        selection: String,
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
    /// Sign an application action its product prepared: the action's
    /// canonical CBOR, as a page's `signAction` would send it.
    Action {
        /// The file holding the action, or `-` for standard input.
        path: PathBuf,
    },
    /// Finished and pending attestations.
    History,
    /// This month's formal-execution allowance and the archived months.
    Usage,
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
enum AutostartCommand {
    /// Register this binary to start the daemon at login.
    Install,
    /// Remove the registration; a running daemon keeps running.
    Uninstall,
    /// Whether the registration exists.
    Status,
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
    /// Publish an image (PNG, JPEG or WebP, at most 512 KiB) as the public
    /// avatar, which the relay serves to anyone; the other fields stay.
    Avatar {
        /// The image.
        #[arg(required_unless_present = "remove")]
        file: Option<PathBuf>,
        /// Remove the avatar instead.
        #[arg(long, conflicts_with = "file")]
        remove: bool,
    },
}

#[derive(Subcommand)]
enum AccountCommand {
    /// Create an account for the current login with this device as administrator.
    Create,
    /// Show the account and its devices.
    Show,
    #[command(flatten)]
    Security(account_commands::SecurityCommand),
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
    let this_device = |d: &Value| match d["this_device"] == true {
        true => tr!(" (this device)", "（本设备）"),
        false => String::new(),
    };
    let d = &request["detail"];
    let mut lines = vec![
        row("request", "请求", field(&request["id"])),
        row("from", "来源", field(&request["source"])),
        row("state", "状态", field(&request["state"])),
        row("expires", "期限", field(&request["expires_at"])),
    ];
    match request["type"].as_str() {
        Some("attest") => {
            let s = &d["statement"];
            let (profile, issuer) = (field(&s["profile"]), field(&s["issuer"]));
            lines.push(row(
                "sign",
                "签署",
                tr!(
                    "{profile} statement as {issuer}",
                    "{profile} 声明，签发者 {issuer}"
                ),
            ));
            lines.push(row("origin", "网站", field(&d["origin"])));
            if !s["subject"].is_null() {
                lines.push(row("subject", "主题", field(&s["subject"])));
            }
            for name in [
                "text",
                "sha256",
                "content_type",
                "location",
                "app_id",
                "command",
                "args",
                "operation_id",
            ] {
                if !s["content"][name].is_null() {
                    lines.push(format!("{name:<9} {}", field(&s["content"][name])));
                }
            }
            let file = match (d["file"].as_str(), d["file_checked"].as_bool()) {
                (Some(path), Some(true)) => tr!("{path} (digest matches)", "{path}（摘要一致）"),
                (Some(path), _) => tr!(
                    "{path} (CHANGED or unreadable)",
                    "{path}（已变化或无法读取）"
                ),
                (None, _) if !s["content"]["sha256"].is_null() => tr!(
                    "only the digest is confirmed; the file was not checked",
                    "仅确认摘要，未核对原文件"
                ),
                _ => String::new(),
            };
            if !file.is_empty() {
                lines.push(row("file", "文件", file));
            }
        }
        Some("device_revoke") => {
            let (device, mark) = (field(&d["device_id"]), this_device(d));
            lines.push(row(
                "revoke",
                "撤销",
                tr!(
                    "device {device}{mark}; the content root is rotated without it",
                    "设备 {device}{mark}；随后换根，它不再能读取",
                ),
            ));
        }
        Some("device_capabilities") => {
            let (device, mark) = (field(&d["device_id"]), this_device(d));
            lines.push(row(
                "change",
                "变更",
                tr!(
                    "capabilities of device {device}{mark}",
                    "设备 {device}{mark} 的能力"
                ),
            ));
            lines.push(row("to", "改为", field(&d["capabilities"])));
        }
        Some("principal") => {
            let change = field(&d["change"]);
            lines.push(row(
                "principal",
                "主体",
                tr!(
                    "{change} — an account change, published by the user home",
                    "{change}：账户变更，由 user home 发布",
                ),
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
            let (kind, generation) = (field(&k["type"]), field(&d["generation"]));
            lines.push(row(
                "sign",
                "签署",
                tr!(
                    "delegation.{kind} with controller #{generation}",
                    "用 controller #{generation} 签署 delegation.{kind}",
                ),
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
            lines.push(row(
                "",
                "",
                tr!(
                    "relying parties may act on a grant before its revocation",
                    "撤销生效前，依赖方仍可能依据授权行事",
                ),
            ));
        }
        Some("connect") => {
            let (app, version) = (field(&d["app_id"]), field(&d["app_version"]));
            lines.push(row(
                "web app",
                "网页应用",
                tr!("{app} (version {version})", "{app}（版本 {version}）"),
            ));
            lines.push(row("origin", "网站", field(&d["origin"])));
            lines.push(row(
                "",
                "",
                tr!(
                    "the origin is relayed by your browser through connect.dmsg.net",
                    "来源由浏览器经 connect.dmsg.net 转述",
                ),
            ));
            lines.push(row("browser", "浏览器", field(&d["pairing_label"])));
            lines.push(row("account", "账户", field(&d["account"])));
            lines.push(row(
                "method",
                "方法",
                format!("{} ({})", field(&d["method"]), field(&d["kind"])),
            ));
            lines.push(row("requested", "发起时间", field(&d["requested_at"])));
            lines.push(row(
                "content",
                "内容",
                serde_json::to_string_pretty(&d["content"]).unwrap_or_default(),
            ));
            if d["method"] == "checkout" {
                lines.push(row(
                    "terms",
                    "条款",
                    match d["checkout"].is_null() {
                        true => tr!(
                            "not quoted: run dmsg approvals quote <id> <ledger or neuron>",
                            "尚未报价：运行 dmsg approvals quote <id> <账本或神经元>",
                        ),
                        false => serde_json::to_string_pretty(&d["checkout"]).unwrap_or_default(),
                    },
                ));
            }
        }
        Some("account") => {
            let principal = field(&d["principal"]);
            let change = match d["change"].as_str() {
                Some("bind_login") => tr!(
                    "bind login {principal} to the account; it signs in to every device's data",
                    "把登录 {principal} 绑定到账户；它可以登录并读取所有设备的数据",
                ),
                Some("remove_login") => tr!("remove login {principal}", "移除登录 {principal}"),
                Some("policy") => {
                    let (frozen, daily, purposes) = (
                        field(&d["frozen"]),
                        field(&d["daily_executions"]),
                        field(&d["allowed_purposes"]),
                    );
                    tr!(
                        "signing policy: frozen {frozen}, {daily} executions a day, purposes {purposes}",
                        "签名策略：冻结 {frozen}，每天 {daily} 次执行，用途 {purposes}",
                    )
                }
                Some("recovery_delay") => {
                    let days = d["delay_ms"].as_u64().unwrap_or(0) / 86_400_000;
                    tr!("recovery delay {days} days", "恢复等待期 {days} 天")
                }
                Some("dispute_recovery") => {
                    let op = field(&d["op_id"]);
                    tr!("cancel the pending recovery {op}", "取消待生效的恢复 {op}")
                }
                _ => field(&d["change"]),
            };
            lines.push(row("change", "变更", change));
        }
        _ => lines.push(row(
            "detail",
            "详情",
            serde_json::to_string_pretty(d).unwrap_or_default(),
        )),
    }
    if !request["error"].is_null() {
        lines.push(row("error", "错误", field(&request["error"])));
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
        _ => match ask(&tr!(
            "\nApprove this request? Type yes to approve, no to reject: ",
            "\n批准这个请求？输入 yes 批准，no 拒绝："
        ))
        .as_str()
        {
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
            let id = r["id"].as_str().unwrap_or_default();
            tr!(
                "request {id} waits in the approval center: dmsg approvals approve {id}",
                "请求 {id} 在审批中心等待确认：dmsg approvals approve {id}"
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
            .map(|e| tr!("; execution {e}", "；执行 {e}"))
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
                eprintln!(
                    "{}",
                    tr!(
                        "error: {} ({})",
                        "错误：{}（{}）",
                        failure.message,
                        failure.code
                    )
                );
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

/// A file's checked digest.
fn verified(manifest: &Value) -> String {
    let (sha256, size) = (
        manifest["sha256"].as_str().unwrap_or_default(),
        &manifest["size"],
    );
    tr!(
        "verified {sha256} ({size} bytes)",
        "已核对 {sha256}（{size} 字节）"
    )
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

fn summary(status: &Value) -> String {
    let field = |name: &str| status[name].as_str().unwrap_or("-").to_string();
    let (state, account, device, login) = (
        field("state"),
        field("account"),
        field("device_id"),
        field("principal"),
    );
    let generation = status["root_generation"]
        .as_u64()
        .map_or("-".into(), |g| g.to_string());
    let (pending, conflicts) = (&status["pending_sync"], &status["conflicts"]);
    let resident = match status["resident"] == true {
        true => tr!(
            "\nbackground online: the device key stays while locked",
            "\n后台在线：锁定时设备密钥仍驻留"
        ),
        false => String::new(),
    };
    tr!(
        "state: {state}\naccount: {account}\ndevice: {device}\nlogin: {login}\nroot generation: {generation}\npending sync: {pending}\nconflicts: {conflicts}{resident}",
        "状态：{state}\n账户：{account}\n设备：{device}\n登录：{login}\n内容根代数：{generation}\n待同步：{pending}\n冲突：{conflicts}{resident}"
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

/// Starting, stopping for an update, installing and the login
/// registration: commands that need no running daemon.
async fn lifecycle_command(
    home: &Home,
    json: bool,
    command: &Command,
) -> Option<Result<(), Failure>> {
    let exe = || std::env::current_exe().map_err(|e| Failure::new("UNAVAILABLE", e.to_string()));
    let outcome = match command {
        Command::Start => match exe() {
            Ok(exe) => lifecycle::start(home, &exe)
                .await
                .map(|status| print(json, &status, summary)),
            Err(failure) => Err(failure),
        },
        Command::Restart { interrupt, wait } => {
            lifecycle::restart(home, *interrupt, Duration::from_secs(*wait))
                .await
                .map(|status| print(json, &status, summary))
        }
        Command::Install { dir } => release::install(dir.clone()).map(|report| {
            print(json, &report, |r| {
                let (path, version) = (r["path"].as_str().unwrap_or_default(), &r["version"]);
                let version = version
                    .as_str()
                    .map_or_else(|| tr!("of unknown version", "（版本未知）"), str::to_string);
                let mut text = match r["action"].as_str().unwrap_or_default() {
                    "installed" => tr!("Installed dmsg {version} at {path}.", "已在 {path} 安装 dmsg {version}。"),
                    "upgraded" => tr!(
                        "Upgraded dmsg at {path} to {version}; run `dmsg restart` to use it.",
                        "已把 {path} 的 dmsg 升级到 {version}；运行 `dmsg restart` 后生效。"
                    ),
                    "kept_newer" => tr!("Kept the newer dmsg {version} at {path}.", "保留 {path} 中更新的 dmsg {version}。"),
                    "homebrew" => tr!(
                        "Homebrew's dmsg {version} at {path} stays; update it with `brew upgrade dmsg`.",
                        "{path} 的 dmsg {version} 由 Homebrew 管理，保持不变；用 `brew upgrade dmsg` 更新。"
                    ),
                    _ => tr!("dmsg {version} is already installed at {path}.", "dmsg {version} 已安装在 {path}。"),
                };
                if r["on_path"] != true {
                    text.push_str(&tr!(
                        "\nAdd its directory to PATH to run `dmsg` from any terminal.",
                        "\n把它所在的目录加入 PATH，即可在任意终端运行 `dmsg`。"
                    ));
                }
                text
            })
        }),
        Command::Update { check: true, .. } => lifecycle::check().await.map(|state| {
            print(json, &state, |s| {
                let (current, latest) = (
                    s["current"].as_str().unwrap_or_default(),
                    s["latest"].as_str().unwrap_or_default(),
                );
                match s["available"] == true {
                    true => tr!(
                        "dmsg {current} is installed; {latest} is available. Run `dmsg update`.",
                        "已安装 dmsg {current}，可更新到 {latest}。运行 `dmsg update`。"
                    ),
                    false => tr!("dmsg {current} is up to date.", "dmsg {current} 已是最新版本。"),
                }
            })
        }),
        Command::Update {
            force,
            interrupt,
            wait,
            ..
        } => lifecycle::update(home, *force, *interrupt, Duration::from_secs(*wait))
            .await
            .map(|reply| {
                print(json, &reply, |r| {
                    let text = |name: &str| r[name].as_str().unwrap_or_default().to_string();
                    let (from, to, path, current) = (text("from"), text("to"), text("path"), text("current"));
                    match (r["updated"] == true, r["restarted"] == true) {
                        (true, true) => tr!(
                            "Updated dmsg {from} to {to} at {path}; the daemon restarted, unlock it again.",
                            "已把 {path} 的 dmsg 从 {from} 更新到 {to}；daemon 已重启，请再次解锁。"
                        ),
                        (true, false) => tr!(
                            "Updated dmsg {from} to {to} at {path}.",
                            "已把 {path} 的 dmsg 从 {from} 更新到 {to}。"
                        ),
                        (false, _) => tr!("dmsg {current} is up to date.", "dmsg {current} 已是最新版本。"),
                    }
                })
            }),
        Command::Autostart(command) => {
            let done = match command {
                AutostartCommand::Install => match exe() {
                    Ok(exe) => autostart::install(&exe, &home.dir).map(|_| true),
                    Err(failure) => Err(failure),
                },
                AutostartCommand::Uninstall => autostart::uninstall().map(|_| false),
                AutostartCommand::Status => autostart::status(),
            };
            done.map(|registered| {
                print(json, &json!({"registered": registered}), |_| {
                    match registered {
                        true => tr!("The daemon starts when you log in.", "登录系统时启动 daemon。"),
                        false => tr!("The daemon does not start at login.", "登录系统时不启动 daemon。"),
                    }
                })
            })
        }
        Command::ValidateConfig => home
            .settings()
            .map(|settings| {
                let environment =
                    dmsg_client::config::environment_name(&settings.config.environment);
                let built_in = settings.built_in;
                print(
                    json,
                    &json!({"valid": true, "environment": environment, "built_in": built_in}),
                    |_| match built_in {
                        true => tr!(
                            "No config.toml: the built-in {environment} configuration applies.",
                            "没有 config.toml：使用内置的 {environment} 配置。"
                        ),
                        false => tr!("config.toml is valid ({environment}).", "config.toml 有效（{environment}）。"),
                    },
                )
            })
            .map_err(|e| Failure::new("INVALID_INPUT", e.to_string())),
        Command::Doctor => {
            let report = doctor::run(home).await;
            print(json, &report, doctor::lines);
            if report["ok"] != true {
                std::process::exit(1);
            }
            Ok(())
        }
        Command::Logs { lines } => logs::tail(&home.dir, *lines)
            .map(|lines| print(json, &json!(lines), |_| lines.join("\n")))
            .map_err(|e| Failure::new("NOT_FOUND", e.to_string())),
        _ => return None,
    };
    Some(outcome)
}

async fn run(cli: Cli) -> Result<(), Failure> {
    let home = Home::resolve(cli.home);
    let json = cli.json;
    let Some(command) = cli.command else {
        return match Remote::connect(&home) {
            Ok(remote) => {
                let status = remote.status().await?;
                print(json, &status, |s| {
                    let summary = summary(s);
                    tr!(
                        "{summary}\n\nCommands: dmsg login | unlock | lock | vault | sync | --help",
                        "{summary}\n\n常用命令：dmsg login | unlock | lock | vault | sync | --help"
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
        let failure =
            |e: dmsg_types::Error| Failure::new(&dmsg_client::error_code(&e), e.to_string());
        let config = &settings.config;
        let ic = dmsg_client::ic::Ic::anonymous(config)
            .await
            .map_err(failure)?;
        // Only the deployment's user homes certify receipts for its accounts.
        let homes = dmsg_client::account::user_homes(&ic, config.handle, &config.user_homes)
            .await
            .map_err(failure)?
            .user_homes;
        let report = dmsg_client::attest::verify_online(&ic, &homes, &bundle, file.as_deref())
            .await
            .map_err(failure)?;
        let report = serde_json::to_value(&report).unwrap_or_default();
        print(json, &report, |r| {
            let text = |name: &str| r[name].as_str().unwrap_or_default().to_string();
            let (key, account, device) =
                (text("key_fingerprint"), text("account"), text("device_id"));
            let (at, epoch) = (&r["approved_at"], &r["security_epoch"]);
            [
                row("content", "内容", text("content")),
                row(
                    "signature",
                    "签名",
                    tr!("valid (key {key})", "有效（密钥 {key}）"),
                ),
                row(
                    "approved",
                    "批准",
                    tr!(
                        "by account {account} with device {device} at {at} (epoch {epoch})",
                        "账户 {account} 的设备 {device}，时间 {at}（安全纪元 {epoch}）"
                    ),
                ),
                row("origin", "网站", text("origin")),
                row("certified", "认证时间", &r["certified_at"]),
                row("device", "设备现状", text("current")),
                row(
                    "statement",
                    "声明",
                    serde_json::to_string_pretty(&r["statement"]).unwrap_or_default(),
                ),
            ]
            .join("\n")
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
    if let Some(outcome) = lifecycle_command(&home, json, &command).await {
        return outcome;
    }
    let remote = Remote::connect(&home)?;
    match command {
        Command::Daemon { .. } | Command::Status | Command::AppToken | Command::Mcp => {
            unreachable!()
        }
        Command::Start
        | Command::Restart { .. }
        | Command::Install { .. }
        | Command::Update { .. }
        | Command::Autostart(_)
        | Command::ValidateConfig
        | Command::Doctor
        | Command::Logs { .. } => unreachable!(),
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
        Command::Approvals(ApprovalsCommand::Quote { id, selection }) => {
            let id = request_id(&remote, &id).await?;
            let request = remote
                .call("approval/quote", json!({"id": id, "selection": selection}))
                .await?;
            print(json, &request, render);
        }
        Command::Attest(AttestCommand::Usage) => {
            let usage = remote.call("attest/usage", Value::Null).await?;
            print(json, &usage, pretty);
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
        Command::Attest(AttestCommand::Action { path }) => {
            let bytes = match path.as_os_str() == "-" {
                true => {
                    let mut bytes = Vec::new();
                    std::io::Read::read_to_end(&mut std::io::stdin(), &mut bytes).map(|_| bytes)
                }
                false => std::fs::read(&path),
            }
            .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
            let request = remote
                .call(
                    "attest/action",
                    json!({"action": dmsg_client::codec::b64(&bytes)}),
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
                let path = v["path"].as_str().unwrap_or_default();
                tr!("wrote {path}", "已写入 {path}")
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
                println!(
                    "{}",
                    tr!(
                        "Open this link in the browser to connect (valid ten minutes):\n{url}",
                        "在要连接的浏览器中打开这个链接（十分钟内有效）：\n{url}"
                    )
                );
            } else {
                println!(
                    "{}",
                    tr!(
                        "Opened the pairing page in your browser.",
                        "已在浏览器中打开配对页。"
                    )
                );
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
                    let short = &pairing[..12];
                    let answer = ask(&tr!(
                        "Connect the browser \"{label}\" ({short})? Type yes to connect: ",
                        "连接浏览器“{label}”（{short}）？输入 yes 连接："
                    ));
                    if answer == "yes" {
                        let granted = remote
                            .call(
                                "connect/grant",
                                json!({"invite": invite_id, "pairing": pairing, "human": true}),
                            )
                            .await?;
                        let (label, until) = (
                            granted["label"].as_str().unwrap_or_default(),
                            &granted["expires_at"],
                        );
                        println!(
                            "{}",
                            tr!(
                                "connected {label} until {until}",
                                "已连接 {label}，有效至 {until}"
                            )
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
            print(json, &json!({"revoked": pairing}), |_| {
                tr!("revoked", "已撤销")
            });
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
                let (fetched, queued, refused) = (&r["fetched"], &r["queued"], &r["refused"]);
                let (replied, sealed) = (&r["replied"], &r["sealed"]);
                tr!(
                    "fetched {fetched}, queued {queued}, refused {refused}, replied {replied}, sealed {sealed}",
                    "取回 {fetched}，排队 {queued}，拒绝 {refused}，已答复 {replied}，封存 {sealed}"
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
        Command::Events(command) => event_commands::events(&remote, json, command).await?,
        Command::Audit {
            agent,
            before,
            limit,
        } => agent_commands::audit(&remote, json, agent, before, limit).await?,
        Command::Ap {
            agent,
            session,
            tool,
            input,
        } => agent_commands::ap(&remote, json, agent, session, tool, input).await?,
        Command::Stop => {
            drop(remote);
            lifecycle::drain(&home, true, Duration::ZERO).await?;
            print(json, &json!({"stopped": true}), |_| {
                tr!("stopped", "已停止")
            });
        }
        Command::Login {
            origin,
            no_browser,
            bind,
        } => {
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
                .call(
                    "session/login",
                    json!({"origin": origin, "bind": bind, "source": "cli"}),
                )
                .await?;
            let url = started["url"].as_str().unwrap_or_default();
            if json {
                println!("{}", json!({"url": url}));
            } else {
                println!("{url}");
                eprintln!(
                    "{}",
                    tr!(
                        "Waiting for the login in your browser…",
                        "等待在浏览器中完成登录…"
                    )
                );
            }
            if !no_browser && !open_browser(url) {
                eprintln!(
                    "{}",
                    tr!(
                        "Open the URL above in your browser.",
                        "请在浏览器中打开上面的链接。"
                    )
                );
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
                if bind {
                    return created(&remote, json, result["approval"].clone()).await;
                }
                if result["status"]["principal"].is_null() {
                    return Err(Failure::new("EXPIRED", "the login was not completed"));
                }
                print(json, &result["status"], summary);
                break;
            }
        }
        Command::Unlock { fast, account } => {
            let status = remote
                .call("session/unlock", json!({"fast": fast, "account": account}))
                .await?;
            print(json, &status, summary);
        }
        Command::Accounts => account_commands::accounts(&remote, json).await?,
        Command::Switch { account } => {
            let status = remote
                .call("session/switch", json!({"account": account}))
                .await?;
            print(json, &status, summary);
        }
        Command::Account(AccountCommand::Security(command)) => {
            account_commands::security(&remote, json, command).await?
        }
        Command::Recovery(command) => account_commands::recovery(&remote, json, command).await?,
        Command::Payment(command) => account_commands::payment(&remote, json, command).await?,
        Command::Ops(command) => account_commands::ops(&remote, json, command).await?,
        Command::Legacy(command) => legacy_commands::legacy(&remote, json, command).await?,
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
                let (account, generation) = (
                    v["account"].as_str().unwrap_or_default(),
                    &v["root_generation"],
                );
                tr!(
                    "account: {account}\nroot generation: {generation}",
                    "账户：{account}\n内容根代数：{generation}"
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
                                String::new()
                            } else {
                                tr!(" revoked", " 已撤销")
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
                let (device, token) = (
                    v["device_id"].as_str().unwrap_or_default(),
                    v["token"].as_str().unwrap_or_default(),
                );
                tr!(
                    "device: {device}\n\nOn an administrator device run:\n  dmsg device approve {token} --device {device}",
                    "设备：{device}\n\n在一台管理员设备上运行：\n  dmsg device approve {token} --device {device}"
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
                let (device, generation) = (
                    v["device_id"].as_str().unwrap_or_default(),
                    &v["root_generation"],
                );
                tr!(
                    "approved {device}\nroot generation: {generation}",
                    "已批准 {device}\n内容根代数：{generation}"
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
                    tr!(" (kept as a conflict copy)", "（保存为冲突副本）")
                } else {
                    String::new()
                };
                format!("{}{conflict}", v["revision"].as_str().unwrap_or_default())
            });
        }
        Command::Vault(VaultCommand::Rm { id }) => {
            let entry = remote.call("vault/delete", json!({"id": id})).await?;
            print(json, &entry, |_| tr!("moved to the trash", "已移到回收站"));
        }
        Command::Vault(VaultCommand::Restore { id }) => {
            let entry = remote.call("vault/restore", json!({"id": id})).await?;
            print(json, &entry, |_| tr!("restored", "已恢复"));
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
        Command::Vault(VaultCommand::Resolve { key, keep_current }) => {
            let keep = if keep_current { "current" } else { "theirs" };
            let entry = remote
                .call("vault/resolve", json!({"key": key, "keep": keep}))
                .await?;
            print(json, &entry, |v| {
                let revision = v["revision"].as_str().unwrap_or_default();
                tr!("resolved; head {revision}", "已解决；当前版本 {revision}")
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
                        let (id, name, size) = (
                            e["id"].as_str().unwrap_or_default(),
                            e["item"]["file"]["name"].as_str().unwrap_or_default(),
                            &e["item"]["file"]["size"],
                        );
                        let synced = match e["synced"] == true {
                            true => String::new(),
                            false => tr!(" (not synced)", "（未同步）"),
                        };
                        tr!(
                            "{id} {name} {size} bytes{synced}",
                            "{id} {name} {size} 字节{synced}"
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
            print(json, &manifest, verified);
        }
        Command::File(FileCommand::Verify { id }) => {
            let manifest = remote.call("file/verify", json!({"id": id})).await?;
            print(json, &manifest, verified);
        }
        Command::Sync { command: None } => {
            let report = remote.call("sync/run", Value::Null).await?;
            print(json, &report, |r| {
                let (sent, blocked, revisions) = (&r["sent"], &r["blocked"], &r["revisions"]);
                let (through, pending, conflicts) = (&r["through"], &r["pending"], &r["conflicts"]);
                let collected = &r["collected"];
                tr!(
                    "sent {sent}, refused {blocked}, {revisions} revisions through {through}; {pending} pending, {conflicts} conflicts; {collected} unused chunks deleted",
                    "发送 {sent}，被拒 {blocked}，收到 {revisions} 个版本（至 {through}）；待同步 {pending}，冲突 {conflicts}；删除 {collected} 个不再需要的本地块"
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
        Command::Profile(ProfileCommand::Avatar { file, remove }) => {
            let params = match file {
                Some(file) => json!({"path": absolute(file)?}),
                None => json!({"remove": remove}),
            };
            let profile = remote.call("profile/avatar", params).await?;
            print(json, &profile, |p| {
                let version = &p["profile"]["version"];
                match p["profile"]["avatar_upload"].as_str() {
                    Some(upload) => tr!(
                        "published version {version} with avatar {upload}",
                        "已发布版本 {version}，头像 {upload}"
                    ),
                    None => tr!(
                        "published version {version} without an avatar",
                        "已发布版本 {version}，不带头像"
                    ),
                }
            });
        }
    }
    Ok(())
}
