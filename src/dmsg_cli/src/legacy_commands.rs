//! `dmsg legacy`: personal archives from the old site.

use crate::locale::tr;
use crate::remote::{Failure, Remote};
use crate::{absolute, pretty, print};
use clap::Subcommand;
use serde_json::{Value, json};
use std::path::PathBuf;

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
    /// Delete one channel and its content from a kept personal archive.
    DeleteChannel {
        /// Archive ID.
        id: String,
        /// Source channel, such as <canister>/channel/1.
        channel: String,
    },
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
                let text = |name: &str| o[name].as_str().unwrap_or_default().to_string();
                let (link, device, fingerprint, nonce) = (
                    text("link"),
                    text("target"),
                    text("fingerprint"),
                    text("nonce"),
                );
                tr!(
                    "Open on the old site, in the browser that holds the old data:\n  {link}\n\ndevice:      {device}\nfingerprint: {fingerprint}\nnonce:       {nonce}\nvalid for one hour. Then:\n  dmsg legacy import <file> --nonce {nonce} --fingerprint {fingerprint} --principal <old principal>",
                    "在保存旧数据的浏览器中打开旧网站：\n  {link}\n\n设备：  {device}\n指纹：  {fingerprint}\nnonce： {nonce}\n一小时内有效。然后运行：\n  dmsg legacy import <文件> --nonce {nonce} --fingerprint {fingerprint} --principal <旧 Principal>"
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
        LegacyCommand::DeleteChannel { id, channel } => (
            "legacy/delete_channel",
            json!({"id": id, "channel": channel}),
        ),
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
    };
    let value = remote.call(method, params).await?;
    print(json, &value, pretty);
    Ok(())
}
