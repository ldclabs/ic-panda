//! `dmsg`: the dMsg command line client. `dmsg daemon` runs the daemon that
//! holds every key and all state; every other command asks it over loopback.

// `clippy.toml` names the guards no `.await` may hold. Denied here, not in
// `[lints.clippy]`: editors' Cargo.toml schema checks reject that table.
#![deny(clippy::await_holding_invalid_type)]

mod account_commands;
mod account_methods;
mod agent_commands;
mod agents;
mod approval_commands;
mod autostart;
mod channels;
mod commands;
mod daemon;
mod display;
mod doctor;
#[cfg(target_os = "macos")]
mod enclave;
mod event_commands;
mod files_commands;
mod files_methods;
mod held;
mod hints;
mod home;
mod legacy_commands;
mod legacy_methods;
mod lifecycle;
mod locale;
mod logs;
mod mail_push;
mod mcp;
mod mcp_events;
mod methods;
mod mount;
#[cfg(target_os = "macos")]
mod mount_netfs;
mod notifications;
mod params;
mod process_command;
mod recovery_watch;
mod release;
mod remote;
mod secret_commands;
mod secret_discovery;
mod secret_methods;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod secret_mounts;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
#[path = "secret_mounts_unavailable.rs"]
mod secret_mounts;
mod secure;
mod snapshots;
#[cfg(unix)]
mod ssh_agent;
mod vault_commands;
mod vault_methods;
mod wallet_commands;
mod wallet_methods;

use approval_commands::AttestCommand;
use clap::{Parser, Subcommand};
use home::Home;
use locale::tr;
use remote::{Failure, Remote};
use serde_json::{Value, json};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
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
    /// Internal: mount a loopback WebDAV share through NetFS.
    #[cfg(target_os = "macos")]
    #[command(hide = true)]
    MountNetfs,
    /// Run the daemon in the foreground.
    Daemon {
        /// Loopback port; 0 picks a free one.
        #[arg(long, default_value_t = 0)]
        port: u16,
    },
    /// Show the daemon and session status.
    Status,
    /// Start the daemon in the background, or restart a running one older
    /// than this binary when it is idle; see `dmsg logs` for its run log.
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
        /// Use fast unlock (up to thirty days after a login unlock); with the
        /// Secure Enclave it asks for Touch ID or the login password and
        /// restores the login.
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
    /// Sign out and forget the saved login and fast-unlock key; keep local data.
    Logout,
    /// Fast unlock on this device, kept in the system credential store.
    #[command(subcommand)]
    FastUnlock(FastUnlockCommand),
    /// Bind this workspace to this device's hardware (macOS).
    #[command(subcommand)]
    DeviceBound(DeviceBoundCommand),
    /// The account.
    #[command(subcommand)]
    Account(account_commands::AccountCommand),
    /// Recovery after every device is lost.
    #[command(subcommand)]
    Recovery(account_commands::RecoveryCommand),
    /// Escrow transfers of paid letters.
    #[command(subcommand)]
    Payment(account_commands::PaymentCommand),
    /// Commerce and membership records the payer may read and repair.
    #[command(subcommand)]
    Ops(account_commands::OpsCommand),
    /// Legacy migration: personal archives from the old site and shared
    /// old channels.
    #[command(subcommand)]
    Legacy(legacy_commands::LegacyCommand),
    /// Devices of the account.
    #[command(subcommand)]
    Device(account_commands::DeviceCommand),
    /// The vault.
    #[command(subcommand)]
    Vault(vault_commands::VaultCommand),
    /// Credential references, device-local policies and approved use history.
    #[command(subcommand)]
    Secrets(secret_commands::SecretCommand),
    /// Approve one-time credential delivery to a program and its descendants.
    Run(process_command::RunCommand),
    /// The encrypted file library on this device.
    #[command(subcommand)]
    Files(files_commands::FilesCommand),
    /// Synchronize with the relay, or show where sync stands.
    Sync {
        #[command(subcommand)]
        command: Option<SyncCommand>,
    },
    /// The public profile.
    #[command(subcommand)]
    Profile(account_commands::ProfileCommand),
    /// Erase this device's workspace and start over as a new device.
    Reset {
        /// Confirm: unsynced versions are lost.
        #[arg(long)]
        yes: bool,
    },
    /// The approval center: requests from this device, agents and web pages.
    #[command(subcommand)]
    Approvals(approval_commands::ApprovalsCommand),
    /// Formal statements signed by this device, and their verification.
    #[command(subcommand)]
    Attest(approval_commands::AttestCommand),
    /// Web applications through dmsg-connect/1.
    #[command(subcommand)]
    Connect(approval_commands::ConnectCommand),
    /// Formal channels: end-to-end encrypted group conversations.
    #[command(subcommand)]
    Channel(commands::ChannelCommand),
    /// The inbox: contact policy and letters, free or paid.
    #[command(subcommand)]
    Inbox(commands::InboxCommand),
    /// The payer: the login, or an identity PEM file.
    #[command(subcommand)]
    Payer(commands::PayerCommand),
    /// Team wallets.
    #[command(subcommand)]
    Wallet(wallet_commands::WalletCommand),
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
    /// Enable fast unlock while unlocked; a login unlock enables it too.
    Enable,
    /// Disable fast unlock, delete its key and the kept login; a login
    /// unlock then leaves it off.
    Disable,
}

#[derive(Subcommand)]
enum DeviceBoundCommand {
    /// Bind this workspace to this Mac's Secure Enclave: from now on every
    /// unlock needs it, as well as a login or fast unlock. Copies made
    /// before keep opening with a login. Asks for Touch ID once.
    Enable {
        /// Take only the fingerprints enrolled now, never the login password.
        #[arg(long)]
        biometry_only: bool,
    },
    /// Let a login alone open this workspace again.
    Disable,
}

#[derive(Subcommand)]
enum SyncCommand {
    /// Pending versions, conflicts and the latest sync.
    Status,
}

/// An absolute path for the daemon, which does not share this directory.
fn absolute(path: impl AsRef<Path>) -> Result<String, Failure> {
    std::path::absolute(path)
        .map(|p| p.display().to_string())
        .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))
}

/// A JSON file.
fn read_json(path: &Path) -> Result<Value, Failure> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Failure::new("INVALID_INPUT", format!("{}: {e}", path.display())))?;
    serde_json::from_str(&text)
        .map_err(|e| Failure::new("INVALID_INPUT", format!("{}: {e}", path.display())))
}

/// The start of an identifier, for a column; all of it when shorter.
fn short(text: &str, n: usize) -> &str {
    text.get(..n).unwrap_or(text)
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

fn main() {
    let cli = Cli::parse();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let json = cli.json && !matches!(cli.command, Some(Command::Run(_)));
    let daemon = matches!(&cli.command, Some(Command::Daemon { .. }));
    let code = match runtime.block_on(run(cli)) {
        Ok(()) => 0,
        Err(failure) if failure.code == "PROCESS_EXIT" => failure.message.parse().unwrap_or(1),
        Err(mut failure) => {
            // Background daemon stderr is retained; never log private startup context.
            if daemon {
                failure.message = "daemon failed; see dmsg doctor and dmsg logs".into();
            }
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

/// What binding the workspace to the hardware leaves outside it, from
/// `session/device_bound`'s `unprotected`.
fn unprotected(v: &Value) -> String {
    let mut parts = Vec::new();
    if v["background_online"] == true {
        parts.push(tr!(
            "the device key kept while locked (background online)",
            "锁定后保留的设备签名钥匙（后台在线）"
        ));
    }
    if let Some(n) = v["resident_agents"].as_u64().filter(|n| *n > 0) {
        parts.push(tr!(
            "{n} resident Agent IDs in the credential store",
            "凭据库中的 {n} 个常驻 Agent ID"
        ));
    }
    match parts.is_empty() {
        true => tr!("nothing", "无"),
        false => parts.join(tr!(", ", "，").as_str()),
    }
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
    // Without an account there is no recovery to watch.
    let recovery = match status["account"].is_string() {
        true => format!("\n{}", recovery_summary(&status["recovery"])),
        false => String::new(),
    };
    tr!(
        "state: {state}\naccount: {account}\ndevice: {device}\nlogin: {login}\nroot generation: {generation}\npending sync: {pending}\nconflicts: {conflicts}{resident}{recovery}",
        "状态：{state}\n账户：{account}\n设备：{device}\n登录：{login}\n内容根代数：{generation}\n待同步：{pending}\n冲突：{conflicts}{resident}{recovery}"
    )
}

fn recovery_summary(warning: &Value) -> String {
    let mut text = if warning["pending"] == true {
        tr!(
            "WARNING: account recovery pending. If you did not request it, open Settings > Account security or run dmsg recovery dispute after unlocking/signing in.",
            "警告：账户有待处理的恢复申请。如非本人操作，请打开设置 > 账户安全，或在解锁/登录后运行 dmsg recovery dispute 取消。"
        )
    } else if warning["pending"] == false && warning["stale"] == false {
        tr!(
            "recovery: no pending request at last check",
            "账户恢复：最近检查未发现待处理申请"
        )
    } else {
        tr!(
            "recovery: unknown; waiting for a fresh security check",
            "账户恢复：未知，等待刷新安全状态"
        )
    };
    if warning["stale"] == true {
        text.push_str(&tr!(
            "\nRecovery status is stale; reconnect to check again.",
            "\n恢复状态已过期，请联网后重新检查。"
        ));
    }
    text
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
        let status = Remote::connect(&home)?.status().await?;
        print(json, &status, |s| {
            let summary = summary(s);
            tr!(
                "{summary}\n\nCommands: dmsg login | unlock | lock | vault | sync | --help",
                "{summary}\n\n常用命令：dmsg login | unlock | lock | vault | sync | --help"
            )
        });
        return Ok(());
    };
    let exe = || std::env::current_exe().map_err(|e| Failure::new("UNAVAILABLE", e.to_string()));
    // Commands that need no running daemon; the daemon serves the rest.
    match command {
        #[cfg(target_os = "macos")]
        Command::MountNetfs => mount_netfs::from_stdin(),
        Command::Daemon { port } => daemon::run(home, port)
            .await
            .map_err(|e| Failure::new("UNAVAILABLE", e.to_string())),
        Command::Mcp => mcp::run(&home).await,
        Command::AppToken => app_token(&home),
        Command::Status => match Remote::connect(&home) {
            Ok(remote) => {
                print(json, &remote.status().await?, summary);
                Ok(())
            }
            Err(failure) if failure.code == "DAEMON_STOPPED" && json => {
                println!("{}", json!({"running": false}));
                std::process::exit(3);
            }
            Err(failure) => Err(failure),
        },
        Command::Start => {
            print(json, &lifecycle::start(&home, &exe()?).await?, summary);
            Ok(())
        }
        Command::Stop => {
            lifecycle::drain(&home, true, Duration::ZERO).await?;
            print(json, &json!({"stopped": true}), |_| {
                tr!("stopped", "已停止")
            });
            Ok(())
        }
        Command::Restart { interrupt, wait } => {
            let status = lifecycle::restart(&home, interrupt, Duration::from_secs(wait)).await?;
            print(json, &status, summary);
            Ok(())
        }
        Command::Install { dir } => {
            print(json, &release::install(dir)?, installed);
            Ok(())
        }
        Command::Update { check: true, .. } => {
            print(json, &lifecycle::check().await?, update_available);
            Ok(())
        }
        Command::Update {
            force,
            interrupt,
            wait,
            ..
        } => {
            let wait = Duration::from_secs(wait);
            print(
                json,
                &lifecycle::update(&home, force, interrupt, wait).await?,
                updated,
            );
            Ok(())
        }
        Command::Autostart(command) => {
            let registered = match command {
                AutostartCommand::Install => autostart::install(&exe()?, &home.dir).map(|_| true),
                AutostartCommand::Uninstall => autostart::uninstall().map(|_| false),
                AutostartCommand::Status => autostart::status(),
            }?;
            print(
                json,
                &json!({"registered": registered}),
                |_| match registered {
                    true => tr!(
                        "The daemon starts when you log in.",
                        "登录系统时启动 daemon。"
                    ),
                    false => tr!(
                        "The daemon does not start at login.",
                        "登录系统时不启动 daemon。"
                    ),
                },
            );
            Ok(())
        }
        Command::ValidateConfig => validate_config(&home, json),
        Command::Doctor => {
            let report = doctor::run(&home).await;
            print(json, &report, doctor::lines);
            if report["ok"] != true {
                std::process::exit(1);
            }
            Ok(())
        }
        Command::Logs { lines } => {
            let lines = logs::tail(&home.dir, lines)
                .map_err(|e| Failure::new("NOT_FOUND", e.to_string()))?;
            print(json, &json!(lines), |_| lines.join("\n"));
            Ok(())
        }
        Command::Attest(AttestCommand::Verify { bundle, file }) => {
            approval_commands::verify(&home, json, &bundle, file.as_deref()).await
        }
        command => remote_command(&Remote::connect(&home)?, json, command).await,
    }
}

/// The commands the daemon serves.
async fn remote_command(remote: &Remote, json: bool, command: Command) -> Result<(), Failure> {
    match command {
        Command::Login {
            origin,
            no_browser,
            bind,
        } => account_commands::login(remote, json, &origin, no_browser, bind).await,
        Command::Unlock { fast, account } => {
            let params = json!({"fast": fast, "account": account});
            session(remote, json, "session/unlock", params).await
        }
        Command::Accounts => account_commands::accounts(remote, json).await,
        Command::Switch { account } => {
            session(remote, json, "session/switch", json!({"account": account})).await
        }
        Command::Lock => session(remote, json, "session/lock", Value::Null).await,
        Command::Logout => session(remote, json, "session/logout", Value::Null).await,
        Command::FastUnlock(command) => {
            let enable = matches!(command, FastUnlockCommand::Enable);
            session(
                remote,
                json,
                "session/fast_unlock",
                json!({"enable": enable}),
            )
            .await
        }
        Command::DeviceBound(command) => {
            let params = match command {
                DeviceBoundCommand::Enable { biometry_only } => {
                    json!({"enable": true, "biometry_only": biometry_only})
                }
                DeviceBoundCommand::Disable => json!({"enable": false}),
            };
            let answer = remote.call("session/device_bound", params).await?;
            print(json, &answer, |v| match v["unprotected"].is_object() {
                true => tr!(
                    "This workspace now unlocks only with this device's hardware.\nNot covered: {}",
                    "此工作区现在只能借助本设备硬件解锁。\n不受保护：{}",
                    unprotected(&v["unprotected"])
                ),
                false => summary(v),
            });
            Ok(())
        }
        Command::Reset { yes } => {
            if !yes {
                return Err(Failure::new(
                    "INVALID_INPUT",
                    "reset erases this device's workspace; confirm with --yes",
                ));
            }
            // The daemon checks that this is the device it serves.
            let device = remote.status().await?["device_id"].clone();
            session(remote, json, "workspace/reset", json!({"device": device})).await
        }
        Command::Account(command) => account_commands::account(remote, json, command).await,
        Command::Recovery(command) => account_commands::recovery(remote, json, command).await,
        Command::Payment(command) => account_commands::payment(remote, json, command).await,
        Command::Ops(command) => account_commands::ops(remote, json, command).await,
        Command::Legacy(command) => legacy_commands::legacy(remote, json, command).await,
        Command::Device(command) => account_commands::device(remote, json, command).await,
        Command::Vault(command) => vault_commands::vault(remote, json, command).await,
        Command::Run(command) => process_command::run(remote, command).await,
        Command::Secrets(command) => secret_commands::secrets(remote, json, command).await,
        Command::Files(command) => files_commands::files(remote, json, command).await,
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
            Ok(())
        }
        Command::Sync {
            command: Some(SyncCommand::Status),
        } => {
            let status = remote.call("sync/status", Value::Null).await?;
            print(json, &status, pretty);
            Ok(())
        }
        Command::Profile(command) => account_commands::profile(remote, json, command).await,
        Command::Approvals(command) => approval_commands::approvals(remote, json, command).await,
        Command::Attest(command) => approval_commands::attest(remote, json, command).await,
        Command::Connect(command) => approval_commands::connect(remote, json, command).await,
        Command::Channel(command) => commands::channel(remote, json, command).await,
        Command::Inbox(command) => commands::inbox(remote, json, command).await,
        Command::Payer(command) => commands::payer(remote, json, command).await,
        Command::Wallet(command) => wallet_commands::wallet(remote, json, command).await,
        Command::Plan(command) => commands::plan(remote, json, command).await,
        Command::Handle(command) => commands::handle(remote, json, command).await,
        Command::Agent(command) => agent_commands::agent(remote, json, command).await,
        Command::Grants(command) => agent_commands::grants(remote, json, command).await,
        Command::Audit {
            agent,
            before,
            limit,
        } => agent_commands::audit(remote, json, agent, before, limit).await,
        Command::Ap {
            agent,
            session,
            tool,
            input,
        } => agent_commands::ap(remote, json, agent, session, tool, input).await,
        Command::Events(command) => event_commands::events(remote, json, command).await,
        _ => unreachable!("served without the daemon"),
    }
}

/// A session method, whose reply is the status.
async fn session(remote: &Remote, json: bool, method: &str, params: Value) -> Result<(), Failure> {
    let status = remote.call(method, params).await?;
    print(json, &status, summary);
    Ok(())
}

/// `dmsg app-token`: the daemon's address and the owner credential.
fn app_token(home: &Home) -> Result<(), Failure> {
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
    Ok(())
}

/// `dmsg validate-config`.
fn validate_config(home: &Home, json: bool) -> Result<(), Failure> {
    let settings = home
        .settings()
        .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
    let environment = dmsg_client::config::environment_name(&settings.config.environment);
    let built_in = settings.built_in;
    print(
        json,
        &json!({"valid": true, "environment": environment, "built_in": built_in}),
        |_| match built_in {
            true => tr!(
                "No config.toml: the built-in {environment} configuration applies.",
                "没有 config.toml：使用内置的 {environment} 配置。"
            ),
            false => tr!(
                "config.toml is valid ({environment}).",
                "config.toml 有效（{environment}）。"
            ),
        },
    );
    Ok(())
}

/// `dmsg install`, for people.
fn installed(r: &Value) -> String {
    let path = r["path"].as_str().unwrap_or_default();
    let version = r["version"]
        .as_str()
        .map_or_else(|| tr!("of unknown version", "（版本未知）"), str::to_string);
    let mut text = match r["action"].as_str().unwrap_or_default() {
        "installed" => tr!(
            "Installed dmsg {version} at {path}.",
            "已在 {path} 安装 dmsg {version}。"
        ),
        "upgraded" => tr!(
            "Upgraded dmsg at {path} to {version}; run `dmsg restart` to use it.",
            "已把 {path} 的 dmsg 升级到 {version}；运行 `dmsg restart` 后生效。"
        ),
        "replaced" => tr!(
            "Replaced the dmsg at {path}, which failed the signature check, with {version}; run `dmsg restart` to use it.",
            "{path} 的 dmsg 未通过签名校验，已替换为 {version}；运行 `dmsg restart` 后生效。"
        ),
        "kept_newer" => tr!(
            "Kept the newer dmsg {version} at {path}.",
            "保留 {path} 中更新的 dmsg {version}。"
        ),
        "homebrew" => tr!(
            "Homebrew's dmsg {version} at {path} stays; update it with `brew upgrade dmsg`.",
            "{path} 的 dmsg {version} 由 Homebrew 管理，保持不变；用 `brew upgrade dmsg` 更新。"
        ),
        _ => tr!(
            "dmsg {version} is already installed at {path}.",
            "dmsg {version} 已安装在 {path}。"
        ),
    };
    if r["on_path"] != true {
        text.push_str(&tr!(
            "\nAdd its directory to PATH to run `dmsg` from any terminal.",
            "\n把它所在的目录加入 PATH，即可在任意终端运行 `dmsg`。"
        ));
    }
    text
}

/// `dmsg update --check`, for people.
fn update_available(s: &Value) -> String {
    let (current, latest) = (
        s["current"].as_str().unwrap_or_default(),
        s["latest"].as_str().unwrap_or_default(),
    );
    match s["available"] == true {
        true => tr!(
            "dmsg {current} is installed; {latest} is available. Run `dmsg update`.",
            "已安装 dmsg {current}，可更新到 {latest}。运行 `dmsg update`。"
        ),
        false => tr!(
            "dmsg {current} is up to date.",
            "dmsg {current} 已是最新版本。"
        ),
    }
}

/// `dmsg update`, for people.
fn updated(r: &Value) -> String {
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
        (false, _) => tr!(
            "dmsg {current} is up to date.",
            "dmsg {current} 已是最新版本。"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_status_distinguishes_pending_stale_and_unknown() {
        let pending = recovery_summary(&json!({"pending":true,"stale":false}));
        assert!(pending.contains("dmsg recovery dispute"));
        let stale = recovery_summary(&json!({"pending":true,"stale":true}));
        assert!(stale.contains("dmsg recovery dispute"));
        assert!(stale.len() > pending.len());
        assert_eq!(
            recovery_summary(&Value::Null),
            recovery_summary(&json!({"pending":false}))
        );
        assert_ne!(
            recovery_summary(&Value::Null),
            recovery_summary(&json!({"pending":false,"stale":false}))
        );
        // Unknown is reported for an account only.
        let unknown = recovery_summary(&Value::Null);
        assert!(summary(&json!({"state":"locked","account":"a"})).ends_with(&unknown));
        assert!(!summary(&json!({"state":"logged_out","account":null})).contains(&unknown));
    }

    #[test]
    fn short_columns_never_split_text() {
        assert_eq!(short("0123456789abcdef", 12), "0123456789ab");
        assert_eq!(short("abc", 12), "abc");
        assert_eq!(short("", 12), "");
        assert_eq!(short("名称名称", 4), "名称名称");
    }
}

#[cfg(all(test, unix))]
mod secret_integration_tests;
