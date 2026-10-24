//! Owner commands contain references and metadata only; they never fetch values.
use crate::{
    pretty, print,
    remote::{Failure, Remote},
};
use clap::Subcommand;
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum SecretCommand {
    /// Read-only virtual secret files.
    #[command(subcommand)]
    Mount(MountCommand),
    /// Explicit local discovery.
    #[command(subcommand)]
    Discovery(DiscoveryCommand),
    /// Approved immutable snapshots.
    #[command(subcommand)]
    Shares(ShareCommand),
    /// Receive and import a snapshot.
    #[command(subcommand)]
    Packages(PackageCommand),
    /// Explicit local checks; observations never imply upstream revocation.
    Health,
    /// List local consumers; never scans arbitrary projects or returns values.
    References { reference: String },
    /// Ignore one current finding only.
    Dismiss { id: String },
    /// Owner-reported, ordered credential rotation.
    #[command(subcommand)]
    Rotation(RotationCommand),
    /// Per-use SSH authentication and Git SSH signatures.
    #[command(subcommand)]
    Ssh(SshCommand),
    /// Print a current field's canonical reference, without its value.
    Ref { item_id: String, field_id: String },
    /// Device-local policies; saving or deleting requires a separate approval.
    #[command(subcommand)]
    Policies(PolicyCommand),
    /// Device-local uses and their known or unknown outcomes.
    #[command(subcommand)]
    Uses(UseCommand),
}

#[derive(Subcommand)]
pub enum MountCommand {
    /// Stop and remove the device configuration, leaving project files untouched.
    Remove {
        id: String,
    },
    List,
    /// Inspect a reference-only JSON mapping; no files are created yet.
    Prepare {
        path: PathBuf,
        #[arg(long)]
        window_ms: Option<u64>,
    },
    /// Request approval of the exact mapping and paths.
    Save {
        path: PathBuf,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        base_version: Option<u64>,
        #[arg(long)]
        window_ms: Option<u64>,
    },
    Start {
        id: String,
    },
    Status {
        id: String,
    },
    Revoke {
        id: String,
        #[arg(long)]
        entry_id: Option<String>,
    },
    Stop {
        id: String,
    },
}

#[derive(Subcommand)]
pub enum DiscoveryCommand {
    /// Cancel an active scan without changing source files.
    Cancel {
        scan_id: String,
    },
    /// Explicit root only. Never follows links or edits source files.
    Scan {
        path: PathBuf,
    },
    Status {
        scan_id: String,
    },
    Import {
        scan_id: String,
        #[arg(long = "candidate", required = true)]
        candidates: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum ShareCommand {
    /// Prepare a snapshot for a fixed channel; path contains the explicit selection.
    ChannelPrepare {
        channel_id: String,
        path: PathBuf,
    },
    /// JSON selection contains IDs, revisions and limits; never values or a password.
    Prepare {
        path: PathBuf,
        #[arg(long)]
        password_stdin: bool,
    },
    Request {
        id: String,
    },
    List,
    Show {
        id: String,
    },
    /// Print access material only after confirmed publication.
    Link {
        id: String,
    },
    Reconcile {
        id: String,
    },
    Revoke {
        id: String,
    },
}

#[derive(Subcommand)]
pub enum PackageCommand {
    /// Receive a snapshot from a verified channel message, without a plaintext file.
    ChannelPreview { channel_id: String, seq: u64 },
    /// Read the link from stdin; when --password-stdin, use line 1 for the link and line 2 for its password.
    Preview {
        #[arg(long)]
        password_stdin: bool,
    },
    /// JSON contains the package ID/digest and explicit field/attachment selection.
    Import { path: PathBuf },
}

fn read_json(path: &std::path::Path) -> Result<Value, Failure> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| Failure::new("INVALID_INPUT", "selection file"))?
        .take(262145)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::new("INVALID_INPUT", "selection file"))?;
    if bytes.len() > 262144 {
        return Err(Failure::new(
            "INVALID_INPUT",
            "selection file exceeds 256 KiB",
        ));
    }
    serde_json::from_slice(&bytes).map_err(|_| Failure::new("INVALID_INPUT", "selection JSON"))
}

fn stdin_lines() -> Result<zeroize::Zeroizing<String>, Failure> {
    use std::io::Read;
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(16385)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::new("INVALID_INPUT", "standard input"))?;
    if bytes.len() > 16384 {
        return Err(Failure::new(
            "INVALID_INPUT",
            "standard input exceeds 16 KiB",
        ));
    }
    String::from_utf8(bytes.to_vec())
        .map(zeroize::Zeroizing::new)
        .map_err(|_| Failure::new("INVALID_INPUT", "UTF-8 input"))
}

fn mount_input(path: &std::path::Path) -> Result<Value, Failure> {
    let mut input = read_json(path)?;
    let cwd =
        std::env::current_dir().map_err(|_| Failure::new("UNAVAILABLE", "current directory"))?;
    for field in ["project_directory", "mount_point"] {
        let value = input[field]
            .as_str()
            .ok_or_else(|| Failure::new("INVALID_INPUT", field))?;
        let path = std::path::Path::new(value);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };
        input[field] = json!(
            path.to_str()
                .ok_or_else(|| Failure::new("INVALID_INPUT", "UTF-8 path"))?
        );
    }
    Ok(input)
}

#[derive(Subcommand)]
pub enum RotationCommand {
    List,
    /// Inspect the current dependency inventory before acknowledging it.
    Dependencies {
        id: String,
    },
    Create {
        item_id: String,
        field_id: String,
    },
    Update {
        id: String,
        #[arg(value_parser=["created","tested","references_checked","upstream_revoked"])]
        step: String,
        /// Exact digest returned by rotation dependencies; required for references_checked.
        #[arg(long)]
        dependencies_digest: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum SshCommand {
    Keys,
    Status,
    /// Print the socket for SSH_AUTH_SOCK; never changes global SSH/Git config.
    Socket,
    Enable {
        item_id: String,
        #[arg(long, default_value = "private_key")]
        field_id: String,
        #[arg(long = "purpose", required = true)]
        purposes: Vec<String>,
    },
    Disable {
        id: String,
    },
}

#[derive(Subcommand)]
pub enum PolicyCommand {
    List,
    /// Request a policy change from a JSON file containing policy, optional id/base_version/grant_to. Never include a token.
    Save {
        path: PathBuf,
    },
    /// Stop new uses immediately; cannot recall a sent request.
    Disable {
        id: String,
    },
    /// Request deletion at the exact version shown.
    Delete {
        id: String,
        base_version: u64,
    },
}

#[derive(Subcommand)]
pub enum UseCommand {
    List {
        #[arg(long)]
        before: Option<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: u64,
    },
    Show {
        request_id: String,
    },
    Cancel {
        request_id: String,
    },
    /// Mark a lost result inspected; does not change it to successful or repeat it.
    Reviewed {
        request_id: String,
    },
}

pub async fn secrets(
    remote: &Remote,
    json_output: bool,
    command: SecretCommand,
) -> Result<(), Failure> {
    let (method, params) = match command {
        SecretCommand::Mount(command) => match command {
            MountCommand::Remove { id } => ("secrets/mounts/remove", json!({"id":id})),
            MountCommand::List => ("secrets/mounts/list", json!({})),
            MountCommand::Prepare { path, window_ms } => (
                "secrets/mounts/prepare",
                json!({"input":mount_input(&path)?,"window_ms":window_ms}),
            ),
            MountCommand::Save {
                path,
                id,
                base_version,
                window_ms,
            } => (
                "secrets/mounts/save_request",
                json!({"source":"cli","input":mount_input(&path)?,"id":id,"base_version":base_version,"window_ms":window_ms}),
            ),
            MountCommand::Start { id } => ("secrets/mounts/start", json!({"id":id})),
            MountCommand::Status { id } => ("secrets/mounts/status", json!({"id":id})),
            MountCommand::Revoke { id, entry_id } => (
                "secrets/mounts/revoke_reads",
                json!({"id":id,"entry_id":entry_id}),
            ),
            MountCommand::Stop { id } => ("secrets/mounts/stop", json!({"id":id})),
        },
        SecretCommand::Discovery(command) => match command {
            DiscoveryCommand::Cancel { scan_id } => {
                ("secrets/discovery/cancel", json!({"scan_id":scan_id}))
            }
            DiscoveryCommand::Scan { path } => {
                let path = std::fs::canonicalize(path)
                    .map_err(|_| Failure::new("INVALID_INPUT", "directory"))?;
                ("secrets/discovery/start", json!({"path":path}))
            }
            DiscoveryCommand::Status { scan_id } => {
                ("secrets/discovery/status", json!({"scan_id":scan_id}))
            }
            DiscoveryCommand::Import {
                scan_id,
                candidates,
            } => (
                "secrets/discovery/import_request",
                json!({"source":"cli","scan_id":scan_id,"candidate_ids":candidates}),
            ),
        },
        SecretCommand::Shares(command) => match command {
            ShareCommand::ChannelPrepare { channel_id, path } => (
                "secrets/shares/channel_prepare",
                json!({"channel_id":channel_id,"input":read_json(&path)?}),
            ),
            ShareCommand::Prepare {
                path,
                password_stdin,
            } => {
                let password = if password_stdin {
                    Some(stdin_lines()?)
                } else {
                    None
                };
                (
                    "secrets/shares/prepare",
                    json!({"input":read_json(&path)?,"password":password.as_ref().map(|s|s.trim_end_matches(['\r','\n']))}),
                )
            }
            ShareCommand::Request { id } => {
                ("secrets/shares/request", json!({"source":"cli","id":id}))
            }
            ShareCommand::List => ("secrets/shares/list", json!({})),
            ShareCommand::Show { id } => ("secrets/shares/get", json!({"id":id})),
            ShareCommand::Link { id } => {
                let result = remote
                    .call("secrets/shares/get", json!({"id":id,"include_link":true}))
                    .await?;
                let link = result["link"]["url"]
                    .as_str()
                    .ok_or_else(|| Failure::new("PENDING", "publication is not confirmed"))?;
                if json_output {
                    println!("{}", json!({"url":link}));
                } else {
                    println!("{link}");
                }
                return Ok(());
            }
            ShareCommand::Reconcile { id } => ("secrets/shares/reconcile", json!({"id":id})),
            ShareCommand::Revoke { id } => ("secrets/shares/revoke", json!({"id":id})),
        },
        SecretCommand::Packages(command) => match command {
            PackageCommand::ChannelPreview { channel_id, seq } => (
                "secrets/packages/channel_preview",
                json!({"channel_id":channel_id,"seq":seq}),
            ),
            PackageCommand::Preview { password_stdin } => {
                let input = stdin_lines()?;
                let mut lines = input.lines();
                let link = lines
                    .next()
                    .ok_or_else(|| Failure::new("INVALID_INPUT", "share link on stdin"))?;
                let password =
                    if password_stdin {
                        Some(lines.next().ok_or_else(|| {
                            Failure::new("INVALID_INPUT", "password on second line")
                        })?)
                    } else {
                        None
                    };
                if lines.next().is_some() {
                    return Err(Failure::new("INVALID_INPUT", "unexpected input lines"));
                }
                (
                    "secrets/packages/import_preview",
                    json!({"link":link,"password":password}),
                )
            }
            PackageCommand::Import { path } => (
                "secrets/packages/import_request",
                json!({"source":"cli","selection":read_json(&path)?}),
            ),
        },
        SecretCommand::References { reference } => (
            "secrets/references",
            json!({"reference":dmsg_client::secrets::SecretRef::parse(&reference).map_err(|_|Failure::new("INVALID_INPUT","reference"))?}),
        ),
        SecretCommand::Health => ("secrets/health", json!({})),
        SecretCommand::Dismiss { id } => ("secrets/health/dismiss", json!({"id":id})),
        SecretCommand::Rotation(command) => match command {
            RotationCommand::List => ("secrets/rotation/list", json!({})),
            RotationCommand::Dependencies { id } => {
                ("secrets/rotation/dependencies", json!({"id":id}))
            }
            RotationCommand::Update {
                id,
                step,
                dependencies_digest,
            } => (
                "secrets/rotation/update",
                json!({"id":id,"step":step,"dependencies_digest":dependencies_digest}),
            ),
            RotationCommand::Create { item_id, field_id } => {
                let r = remote
                    .call(
                        "secrets/ref",
                        json!({"item_id":item_id,"field_id":field_id}),
                    )
                    .await?;
                (
                    "secrets/rotation/create",
                    json!({"reference":r["reference"]}),
                )
            }
        },
        SecretCommand::Ssh(command) => match command {
            SshCommand::Keys => ("secrets/ssh/keys", json!({})),
            SshCommand::Status => ("secrets/ssh/status", json!({})),
            SshCommand::Socket => {
                let status = remote.call("secrets/ssh/status", json!({})).await?;
                let socket = status["socket"]
                    .as_str()
                    .ok_or_else(|| Failure::new("UNAVAILABLE", "SSH socket unavailable"))?;
                println!("{socket}");
                return Ok(());
            }
            SshCommand::Disable { id } => ("secrets/ssh/disable", json!({"id":id})),
            SshCommand::Enable {
                item_id,
                field_id,
                purposes,
            } => {
                let r = remote
                    .call(
                        "secrets/ref",
                        json!({"item_id":item_id,"field_id":field_id}),
                    )
                    .await?;
                (
                    "secrets/ssh/binding_request",
                    json!({"source":"cli","reference":r["reference"],"purposes":purposes}),
                )
            }
        },
        SecretCommand::Ref { item_id, field_id } => (
            "secrets/ref",
            json!({"item_id":item_id,"field_id":field_id}),
        ),
        SecretCommand::Policies(command) => match command {
            PolicyCommand::List => ("secrets/policies/list", json!({})),
            PolicyCommand::Save { path } => {
                use std::io::Read;
                let file = std::fs::File::open(path)
                    .map_err(|_| Failure::new("INVALID_INPUT", "policy file"))?;
                let mut bytes = Vec::new();
                file.take(65537)
                    .read_to_end(&mut bytes)
                    .map_err(|_| Failure::new("INVALID_INPUT", "policy file"))?;
                if bytes.len() > 65536 {
                    return Err(Failure::new("INVALID_INPUT", "policy file exceeds 64 KiB"));
                }
                let mut p: Value = serde_json::from_slice(&bytes)
                    .map_err(|_| Failure::new("INVALID_INPUT", "policy JSON"))?;
                let fields = p
                    .as_object_mut()
                    .ok_or_else(|| Failure::new("INVALID_INPUT", "policy JSON"))?;
                if fields
                    .keys()
                    .any(|k| !["policy", "id", "base_version", "grant_to"].contains(&k.as_str()))
                {
                    return Err(Failure::new("INVALID_INPUT", "policy fields"));
                }
                fields.insert("source".into(), json!("cli"));
                ("secrets/policies/save_request", p)
            }
            PolicyCommand::Disable { id } => ("secrets/policies/disable", json!({"id":id})),
            PolicyCommand::Delete { id, base_version } => (
                "secrets/policies/delete_request",
                json!({"id":id,"base_version":base_version,"source":"cli"}),
            ),
        },
        SecretCommand::Uses(command) => match command {
            UseCommand::List {
                before,
                actor,
                limit,
            } => (
                "secrets/uses/list",
                json!({"before":before,"actor":actor,"limit":limit}),
            ),
            UseCommand::Show { request_id } => {
                ("secrets/uses/get", json!({"request_id":request_id}))
            }
            UseCommand::Cancel { request_id } => {
                ("secrets/uses/cancel", json!({"request_id":request_id}))
            }
            UseCommand::Reviewed { request_id } => (
                "secrets/uses/mark_reviewed",
                json!({"request_id":request_id}),
            ),
        },
    };
    let answer = remote.call(method, params).await?;
    print(json_output, &answer, pretty);
    Ok(())
}
