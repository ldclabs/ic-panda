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
