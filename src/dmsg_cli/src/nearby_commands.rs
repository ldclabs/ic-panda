//! `dmsg nearby`: face-to-face encrypted sharing of Files and Vault snapshots.

use crate::locale::tr;
use crate::remote::{Failure, Remote};
use crate::{pretty, print};
use clap::Subcommand;
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Subcommand)]
pub enum NearbyCommand {
    /// Show the active nearby receive and send session status.
    Status,
    /// Discover nearby dMsg devices advertising `_dmsg-share._tcp`.
    Browse {
        /// Discovery window in milliseconds (400..10000).
        #[arg(long, default_value_t = 2500)]
        timeout_ms: u64,
    },
    /// Open a foreground receiving session and print its `dmsg-nearby/1:` offer text.
    Receive {
        /// Display alias shown to the sender.
        #[arg(long)]
        alias: Option<String>,
        /// Also advertise on local mDNS / Apple AWDL so nearby devices can browse this device.
        #[arg(long)]
        discoverable: bool,
    },
    /// Accept one pending incoming peer after verifying its 6-digit code.
    Accept {
        /// Pending peer ID from `dmsg nearby status`.
        peer_id: String,
        /// Destination Files folder ID.
        #[arg(long, default_value = "root")]
        folder: String,
        /// File indices to accept (omit to accept all offered files).
        #[arg(long = "index")]
        selected: Vec<u32>,
        /// Optionally exchange signed dMsg identity after confirmation.
        #[arg(long)]
        share_identity: bool,
    },
    /// Reject one pending incoming peer.
    Reject {
        /// Pending peer ID from `dmsg nearby status`.
        peer_id: String,
    },
    /// Send one or more files from the local Files library to a nearby receiver.
    Send {
        /// Receiver `dmsg-nearby/1:...` offer text (paste from QR or peer device).
        #[arg(long)]
        offer: Option<String>,
        /// Direct host IP from `dmsg nearby browse`.
        #[arg(long)]
        host: Option<String>,
        /// Direct TCP port from `dmsg nearby browse`.
        #[arg(long)]
        port: Option<u16>,
        /// Optional peer display alias when using `--host`/`--port`.
        #[arg(long)]
        peer_alias: Option<String>,
        /// Apple peer-to-peer service instance from `dmsg nearby browse`.
        #[arg(long, conflicts_with_all = ["offer", "host", "port"])]
        advert: Option<String>,
        /// Sender alias shown to the receiver.
        #[arg(long)]
        alias: Option<String>,
        /// Optionally exchange signed dMsg identity after confirmation.
        #[arg(long)]
        share_identity: bool,
        /// Wait until transfer finishes or fails.
        #[arg(long)]
        wait: bool,
        /// Local Files library file IDs to send.
        #[arg(required = true)]
        files: Vec<String>,
    },
    /// Send a Vault snapshot to a nearby receiver (requires high-risk SecretShare approval).
    SendVault {
        /// Receiver `dmsg-nearby/1:...` offer text.
        #[arg(long)]
        offer: Option<String>,
        /// Direct host IP from `dmsg nearby browse`.
        #[arg(long)]
        host: Option<String>,
        /// Direct TCP port from `dmsg nearby browse`.
        #[arg(long)]
        port: Option<u16>,
        /// Apple peer-to-peer service instance from `dmsg nearby browse`.
        #[arg(long, conflicts_with_all = ["offer", "host", "port"])]
        advert: Option<String>,
        /// Vault item ID.
        #[arg(long)]
        item: String,
        /// Exact Vault item revision.
        #[arg(long)]
        revision: String,
        /// Field IDs to include in the snapshot (repeatable).
        #[arg(long = "field")]
        fields: Vec<String>,
        /// Include item notes in the snapshot.
        #[arg(long)]
        notes: bool,
        /// Sender alias shown to the receiver.
        #[arg(long)]
        alias: Option<String>,
    },
    /// Confirm the 6-digit verification code on the sender side (browse mode).
    Confirm,
    /// Cancel active nearby receive and/or send sessions.
    Cancel {
        /// Which side to cancel: `receive`, `send` or `all`.
        #[arg(long, default_value = "all", value_parser = ["receive", "send", "all"])]
        role: String,
    },
}

pub async fn nearby(remote: &Remote, json_out: bool, cmd: NearbyCommand) -> Result<(), Failure> {
    match cmd {
        NearbyCommand::Status => {
            let view = remote.call("nearby/status", Value::Null).await?;
            print(json_out, &view, format_status);
            Ok(())
        }
        NearbyCommand::Browse { timeout_ms } => {
            let reply = remote
                .call("nearby/browse", json!({"timeout_ms": timeout_ms}))
                .await?;
            print(json_out, &reply, format_peers);
            Ok(())
        }
        NearbyCommand::Receive {
            alias,
            discoverable,
        } => {
            let view = remote
                .call(
                    "nearby/receive_start",
                    json!({"alias": alias, "discoverable": discoverable}),
                )
                .await?;
            print(json_out, &view, |v| {
                let offer = v["receive"]["offer"].as_str().unwrap_or_default();
                let port = v["receive"]["port"].as_u64().unwrap_or(0);
                tr!(
                    "Nearby receive listening on port {port}.\nOffer:\n{offer}\nRun `dmsg nearby status` to see incoming peers and `dmsg nearby accept <peer_id>` after checking the 6-digit code.",
                    "近场接收已在端口 {port} 监听。\n连接码（Offer）：\n{offer}\n运行 `dmsg nearby status` 查看待确认设备，核对 6 位码后执行 `dmsg nearby accept <peer_id>`。"
                )
            });
            Ok(())
        }
        NearbyCommand::Accept {
            peer_id,
            folder,
            selected,
            share_identity,
        } => {
            let selected = if selected.is_empty() {
                let status = remote.call("nearby/status", Value::Null).await?;
                status["receive"]["pending_peers"]
                    .as_array()
                    .and_then(|arr| arr.iter().find(|p| p["peer_id"] == peer_id))
                    .and_then(|p| p["files"].as_array())
                    .map(|files| {
                        files
                            .iter()
                            .filter_map(|f| f["index"].as_u64().map(|n| n as u32))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            } else {
                selected
            };
            let view = remote
                .call(
                    "nearby/receive_accept",
                    json!({
                        "peer_id": peer_id,
                        "folder": folder,
                        "selected": selected,
                        "share_identity": share_identity,
                    }),
                )
                .await?;
            print(json_out, &view, format_status);
            Ok(())
        }
        NearbyCommand::Reject { peer_id } => {
            let view = remote
                .call("nearby/receive_reject", json!({"peer_id": peer_id}))
                .await?;
            print(json_out, &view, format_status);
            Ok(())
        }
        NearbyCommand::Send {
            offer,
            host,
            port,
            peer_alias,
            advert,
            alias,
            share_identity,
            wait,
            files,
        } => {
            let items: Vec<Value> = files
                .into_iter()
                .map(|file_id| json!({"file_id": file_id}))
                .collect();
            let mut view = remote
                .call(
                    "nearby/send_start",
                    json!({
                        "offer": offer,
                        "host": host,
                        "port": port,
                        "peer_alias": peer_alias,
                        "transport": advert.as_ref().map(|_| "awdl"),
                        "advert": advert,
                        "alias": alias,
                        "share_identity": share_identity,
                        "files": items,
                        "source": "cli",
                    }),
                )
                .await?;
            if wait {
                view = wait_send(remote, &view, json_out).await?;
            }
            print(json_out, &view, format_status);
            Ok(())
        }
        NearbyCommand::SendVault {
            offer,
            host,
            port,
            item,
            revision,
            fields,
            advert,
            notes,
            alias,
        } => {
            let view = remote
                .call(
                    "nearby/send_start",
                    json!({
                        "offer": offer,
                        "host": host,
                        "port": port,
                        "alias": alias,
                        "transport": advert.as_ref().map(|_| "awdl"),
                        "advert": advert,
                        "source": "cli",
                        "vault_item": {
                            "item_id": item,
                            "revision": revision,
                            "field_ids": fields,
                            "include_notes": notes,
                            "attachments": [],
                        },
                    }),
                )
                .await?;
            print(json_out, &view, |v| {
                let mut text = format_status(v);
                text.push_str(&tr!(
                    "\nAfter the receiver accepts, complete the high-risk SecretShare approval from `dmsg approvals list`.",
                    "\n对方接受后，请通过 `dmsg approvals list` 或桌面审批窗口完成保险库高风险分享审批。"
                ));
                text
            });
            Ok(())
        }
        NearbyCommand::Confirm => {
            let view = remote.call("nearby/send_confirm_sas", Value::Null).await?;
            print(json_out, &view, format_status);
            Ok(())
        }
        NearbyCommand::Cancel { role } => {
            let mut view = Value::Null;
            if role == "receive" || role == "all" {
                view = remote.call("nearby/receive_cancel", Value::Null).await?;
            }
            if role == "send" || role == "all" {
                view = remote.call("nearby/send_cancel", Value::Null).await?;
            }
            print(json_out, &view, format_status);
            Ok(())
        }
    }
}

async fn wait_send(remote: &Remote, started: &Value, json_out: bool) -> Result<Value, Failure> {
    let id = started["send"]["id"]
        .as_u64()
        .ok_or_else(|| Failure::new("UNAVAILABLE", "missing nearby send"))?;
    let mut showed_sas = false;
    loop {
        let view = remote.call("nearby/status", Value::Null).await?;
        let send = &view["send"];
        if send["id"].as_u64() != Some(id) {
            return Err(Failure::new(
                "CANCELLED",
                "nearby send ended or was replaced",
            ));
        }
        match send["state"].as_str() {
            Some("done") => return Ok(view),
            Some("failed") => {
                return Err(Failure::new(
                    send["error"]["code"].as_str().unwrap_or("UNAVAILABLE"),
                    send["error"]["message"]
                        .as_str()
                        .unwrap_or("nearby send failed"),
                ));
            }
            Some("cancelled") => return Err(Failure::new("CANCELLED", "nearby send cancelled")),
            _ => {}
        }
        if !json_out
            && !showed_sas
            && let Some(sas) = send["sas"].as_str()
        {
            eprintln!(
                "{}",
                tr!("Compare with receiver: {sas}", "与接收方核对：{sas}")
            );
            if send["qr_bound"] == false {
                eprintln!(
                    "{}",
                    tr!(
                        "If the codes match, run `dmsg nearby confirm` in another terminal.",
                        "两端六码一致后，在另一终端运行 `dmsg nearby confirm`。"
                    )
                );
            }
            showed_sas = true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn format_peers(reply: &Value) -> String {
    let Some(peers) = reply["peers"].as_array() else {
        return pretty(reply);
    };
    if peers.is_empty() {
        return tr!(
            "No nearby dMsg devices discovered.",
            "未发现开启附近可见的 dMsg 设备。"
        );
    }
    let mut lines = Vec::new();
    for p in peers {
        let alias = p["alias"].as_str().unwrap_or("dMsg Device");
        let transport = p["transport"].as_str().unwrap_or("lan");
        let host = p["host"].as_str().unwrap_or_default();
        let port = p["port"].as_u64().unwrap_or(0);
        let hint = p["pk_hint"].as_str().unwrap_or_default();
        let endpoint = if transport == "awdl" {
            format!("--advert {}", p["advert"].as_str().unwrap_or_default())
        } else {
            format!("{host}:{port}")
        };
        lines.push(format!("{alias}\t[{transport}]\t{endpoint}\tpk={hint}"));
    }
    lines.join("\n")
}

fn format_status(view: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(r) = view.get("receive").filter(|v| v.is_object()) {
        let state = r["state"].as_str().unwrap_or("idle");
        let relay = if r["relay_ready"] == true {
            "relay_ready"
        } else {
            "local_only"
        };
        parts.push(format!("receive: {state} ({relay})"));
        if let Some(offer) = r["offer"].as_str()
            && state == "waiting"
        {
            parts.push(format!("  offer: {offer}"));
        }
        if let Some(codes) = r["verification_codes"].as_array() {
            for sas in codes.iter().filter_map(Value::as_str) {
                parts.push(tr!("  Compare with sender: {sas}", "  与发送方核对：{sas}"));
            }
        }
        if let Some(peers) = r["pending_peers"].as_array() {
            for p in peers {
                parts.push(format!(
                    "  pending peer={} sas={} kind={} transport={}",
                    p["peer_id"].as_str().unwrap_or_default(),
                    p["sas"].as_str().unwrap_or_default(),
                    p["kind"].as_str().unwrap_or_default(),
                    p["transport"].as_str().unwrap_or_default(),
                ));
            }
        }
    }
    if let Some(s) = view.get("send").filter(|v| v.is_object()) {
        let state = s["state"].as_str().unwrap_or("idle");
        let sas = s["sas"].as_str().unwrap_or("-");
        let transport = s["transport"].as_str().unwrap_or("-");
        parts.push(format!("send: {state} sas={sas} transport={transport}"));
    }
    if parts.is_empty() {
        tr!(
            "No active nearby share session.",
            "当前无进行中的近场分享会话。"
        )
    } else {
        parts.join("\n")
    }
}
