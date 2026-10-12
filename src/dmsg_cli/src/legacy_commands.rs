//! `dmsg legacy`: personal archives from the old site and the official
//! successor of shared old channels.

use crate::print;
use crate::remote::{Failure, Remote};
use clap::Subcommand;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Legacy migration.
#[derive(Subcommand)]
pub enum LegacyCommand {
    /// Make a one-time offer and print the old-site link to open in the
    /// browser where the old data lives.
    Pair {
        /// The old site: https://dmsg.net or https://panda.fans.
        #[arg(long, default_value = "https://dmsg.net")]
        origin: String,
    },
    /// Offers still valid or waiting to continue.
    Pairs,
    /// Forget an offer; a kept archive stays.
    Cancel {
        /// The offer's nonce.
        nonce: String,
    },
    /// Import the encrypted file the old site saved, after comparing the
    /// fingerprint and the old Principal on both sides.
    Import {
        /// The `.dmsg-migration` file.
        file: PathBuf,
        /// The offer's nonce.
        #[arg(long)]
        nonce: String,
        /// The fingerprint both sides show.
        #[arg(long)]
        fingerprint: String,
        /// The old Principal the old site shows.
        #[arg(long)]
        principal: String,
    },
    /// The kept archives.
    List,
    /// Check a kept archive offline again and show it.
    Report {
        /// Archive ID.
        id: String,
        /// First message to show.
        #[arg(long, default_value_t = 0)]
        offset: u64,
        /// Messages to show.
        #[arg(long, default_value_t = 20)]
        limit: u64,
    },
    /// Verify an old attachment or the old avatar and save it.
    File {
        /// Archive ID.
        id: String,
        /// Its source, such as `<bucket>/file/7`.
        source: String,
        /// Where to save it; must not exist.
        out: PathBuf,
    },
    /// Compare a kept archive with the old site's frozen-history file.
    Frozen {
        /// Archive ID.
        id: String,
        /// The `.cbor` file the old site exported after the freeze.
        file: PathBuf,
    },
    /// The official successor of a shared old channel.
    #[command(subcommand)]
    Shared(SharedCommand),
}

/// Shared channel inheritance; documents are JSON files exchanged with the
/// old site and the other participants.
#[derive(Subcommand)]
pub enum SharedCommand {
    /// Verify the frozen source file the old site exported.
    Source {
        /// `legacy-shared-source.json`.
        file: PathBuf,
    },
    /// Propose this account as the new owner; prints the draft.
    Prepare {
        /// `legacy-shared-source.json`.
        file: PathBuf,
        /// Proposal version, 1 for the first.
        #[arg(long, default_value_t = 1)]
        version: u64,
    },
    /// Check a draft another participant handed over.
    Check {
        /// The draft.
        draft: PathBuf,
    },
    /// The request a frozen manager approves on the old site.
    Request {
        /// The draft.
        draft: PathBuf,
        /// The manager's old Principal.
        #[arg(long)]
        manager: String,
    },
    /// Submit the draft with the first manager's approval.
    Propose {
        /// The draft.
        draft: PathBuf,
        /// The manager's old Principal.
        #[arg(long)]
        manager: String,
        /// The approval the old site printed.
        #[arg(long)]
        approval: PathBuf,
    },
    /// Record another manager's approval of the same draft.
    Consent {
        /// The draft.
        draft: PathBuf,
        /// The manager's old Principal.
        #[arg(long)]
        manager: String,
        /// The approval the old site printed.
        #[arg(long)]
        approval: PathBuf,
    },
    /// The directory record: by key from the relay, or a record file.
    View {
        /// Directory key or record file.
        target: String,
    },
    /// As the owner: fix the mapping once every manager approved.
    Commit {
        /// Directory key.
        key: String,
    },
    /// As the owner: create the successor and open its first epoch.
    Activate {
        /// Directory key.
        key: String,
    },
    /// The request an old member approves to claim this account and device.
    ClaimRequest {
        /// Directory key or record file.
        target: String,
        /// The member's old Principal.
        #[arg(long)]
        member: String,
    },
    /// Submit a claim with the old member's approval.
    Claim {
        /// Directory key or record file.
        target: String,
        /// The request printed by `claim-request`.
        #[arg(long)]
        request: PathBuf,
        /// The approval the old site printed.
        #[arg(long)]
        approval: PathBuf,
    },
    /// Record that this account joined the successor.
    Joined {
        /// Directory key.
        key: String,
    },
    /// Hand a joined member the channel's history from a kept archive.
    Grant {
        /// Directory key.
        key: String,
        /// Kept archive ID.
        #[arg(long)]
        archive: String,
        /// The member's old Principal.
        #[arg(long)]
        member: String,
    },
    /// Receive a history grant to this device.
    Receive {
        /// Directory key.
        key: String,
        /// Grant ID.
        #[arg(long)]
        grant: String,
    },
}

fn read(path: &Path) -> Result<Value, Failure> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Failure::new("INVALID_INPUT", format!("{}: {e}", path.display())))?;
    serde_json::from_str(&text)
        .map_err(|e| Failure::new("INVALID_INPUT", format!("{}: {e}", path.display())))
}

fn absolute(path: &Path) -> Result<String, Failure> {
    std::path::absolute(path)
        .map(|p| p.display().to_string())
        .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// A directory key, or the record in a file.
fn target(text: &str) -> Result<Value, Failure> {
    match Path::new(text).is_file() {
        true => Ok(json!({"view": read(Path::new(text))?})),
        false => Ok(json!({"key": text})),
    }
}

fn report(value: &Value) -> String {
    let mut lines = vec![format!(
        "archive {} · {} · {} · {} objects · {} gaps · {} messages",
        value["id"].as_str().unwrap_or_default(),
        value["principal"].as_str().unwrap_or_default(),
        value["mode"].as_str().unwrap_or_default(),
        value["objects"],
        value["gaps"].as_array().map_or(0, Vec::len),
        value["total_messages"],
    )];
    for gap in value["gaps"].as_array().into_iter().flatten().take(20) {
        lines.push(format!(
            "  gap {} {}: {}",
            gap["code"].as_str().unwrap_or_default(),
            gap["source"].as_str().unwrap_or_default(),
            gap["detail"].as_str().unwrap_or_default()
        ));
    }
    for file in value["files"].as_array().into_iter().flatten() {
        lines.push(format!(
            "  file {} {}{}",
            file["source"].as_str().unwrap_or_default(),
            file["name"].as_str().unwrap_or_default(),
            if file["verified"] == true {
                ""
            } else {
                " (not verified)"
            }
        ));
    }
    for message in value["messages"].as_array().into_iter().flatten() {
        lines.push(format!(
            "  [{}] {}: {}",
            message["time"].as_str().unwrap_or_default(),
            message["author"].as_str().unwrap_or_default(),
            match message["deleted"] == true {
                true => "(deleted)",
                false => message["text"].as_str().unwrap_or_default(),
            }
        ));
    }
    lines.join("\n")
}

pub async fn legacy(remote: &Remote, json: bool, command: LegacyCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        LegacyCommand::Pair { origin } => {
            let offer = remote
                .call("legacy/pair", json!({"origin": origin}))
                .await?;
            print(json, &offer, |o| {
                format!(
                    "Open on the old site, in the browser that holds the old data:\n  {}\n\ndevice:      {}\nfingerprint: {}\nnonce:       {}\nvalid for ten minutes. Then:\n  dmsg legacy import <file> --nonce {} --fingerprint {} --principal <old principal>",
                    o["link"].as_str().unwrap_or_default(),
                    o["target"].as_str().unwrap_or_default(),
                    o["fingerprint"].as_str().unwrap_or_default(),
                    o["nonce"].as_str().unwrap_or_default(),
                    o["nonce"].as_str().unwrap_or_default(),
                    o["fingerprint"].as_str().unwrap_or_default(),
                )
            });
            return Ok(());
        }
        LegacyCommand::Pairs => ("legacy/pairs", Value::Null),
        LegacyCommand::Cancel { nonce } => ("legacy/cancel", json!({"nonce": nonce})),
        LegacyCommand::Import {
            file,
            nonce,
            fingerprint,
            principal,
        } => {
            let value = remote
                .call(
                    "legacy/import",
                    json!({
                        "path": absolute(&file)?,
                        "nonce": nonce,
                        "fingerprint": fingerprint,
                        "principal": principal,
                    }),
                )
                .await?;
            print(json, &value, report);
            return Ok(());
        }
        LegacyCommand::List => ("legacy/list", Value::Null),
        LegacyCommand::Report { id, offset, limit } => {
            let value = remote
                .call(
                    "legacy/report",
                    json!({"id": id, "offset": offset, "limit": limit}),
                )
                .await?;
            print(json, &value, report);
            return Ok(());
        }
        LegacyCommand::File { id, source, out } => (
            "legacy/file",
            json!({"id": id, "source": source, "path": absolute(&out)?}),
        ),
        LegacyCommand::Frozen { id, file } => {
            ("legacy/frozen", json!({"id": id, "path": absolute(&file)?}))
        }
        LegacyCommand::Shared(command) => match command {
            SharedCommand::Source { file } => {
                ("legacy/shared_source", json!({"source": read(&file)?}))
            }
            SharedCommand::Prepare { file, version } => (
                "legacy/shared_prepare",
                json!({"source": read(&file)?, "version": version}),
            ),
            SharedCommand::Check { draft } => {
                ("legacy/shared_check", json!({"draft": read(&draft)?}))
            }
            SharedCommand::Request { draft, manager } => (
                "legacy/shared_request",
                json!({"draft": read(&draft)?, "manager": manager}),
            ),
            SharedCommand::Propose {
                draft,
                manager,
                approval,
            } => (
                "legacy/shared_propose",
                json!({"draft": read(&draft)?, "manager": manager, "approval": read(&approval)?}),
            ),
            SharedCommand::Consent {
                draft,
                manager,
                approval,
            } => (
                "legacy/shared_consent",
                json!({"draft": read(&draft)?, "manager": manager, "approval": read(&approval)?}),
            ),
            SharedCommand::View { target: t } => ("legacy/shared_view", target(&t)?),
            SharedCommand::Commit { key } => ("legacy/shared_commit", json!({"key": key})),
            SharedCommand::Activate { key } => ("legacy/shared_activate", json!({"key": key})),
            SharedCommand::ClaimRequest { target: t, member } => {
                let mut params = target(&t)?;
                params["member"] = member.into();
                ("legacy/shared_claim_request", params)
            }
            SharedCommand::Claim {
                target: t,
                request,
                approval,
            } => {
                let mut params = target(&t)?;
                params["claim"] = read(&request)?["request"]["claim"].clone();
                params["approval"] = read(&approval)?;
                ("legacy/shared_claim", params)
            }
            SharedCommand::Joined { key } => ("legacy/shared_joined", json!({"key": key})),
            SharedCommand::Grant {
                key,
                archive,
                member,
            } => (
                "legacy/shared_grant",
                json!({"key": key, "archive": archive, "member": member}),
            ),
            SharedCommand::Receive { key, grant } => {
                ("legacy/shared_receive", json!({"key": key, "grant": grant}))
            }
        },
    };
    let value = remote.call(method, params).await?;
    print(json, &value, pretty);
    Ok(())
}
