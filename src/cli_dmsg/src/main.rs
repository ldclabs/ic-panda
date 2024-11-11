use candid::{Nat, Principal};
use chrono::prelude::*;
use ciborium::{from_reader, into_writer};
use clap::{Parser, Subcommand};
use ic_agent::{
    identity::{AnonymousIdentity, BasicIdentity},
    Identity,
};
use num_traits::cast::ToPrimitive;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

mod agent;
mod dmsg;

use agent::build_agent;
use dmsg::DMsgAgent;

static IC_HOST: &str = "https://icp-api.io";
const TOKEN_1: u64 = 100_000_000;
const MAX_AMOUNT_TOKEN: u64 = 1_000_000; // 1_000_000 PANDA

// "druyg-tyaaa-aaaaq-aactq-cai" PANDA token canister id
static TOKEN_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 0, 0, 167, 1, 1]);
// "nscli-qiaaa-aaaaj-qa4pa-cai" ICPanda Message canister id
static MSG_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 1, 48, 7, 30, 1, 1]);

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(long, short, value_name = "PEM_FILE", default_value = "Anonymous")]
    id_file: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    Send {
        #[arg(long, short)]
        record_file: String,

        #[arg(long, short)]
        amount: u64,

        #[arg(long, short)]
        usernames: Vec<String>,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SendRecords(pub BTreeMap<String, (Principal, u64, u64)>);

// ./target/debug/cli_dmsg  send -u zensh -u panda -a 5000 -r debug/records.cbor
#[tokio::main]
async fn main() -> Result<(), String> {
    let cli = Cli::parse();
    let identity = load_identity(&cli.id_file)?;
    println!("principal: {}", &identity.sender().unwrap().to_text());

    let agent = build_agent(IC_HOST, Box::new(identity)).await?;

    match &cli.command {
        Some(Commands::Send {
            record_file,
            amount,
            usernames,
        }) => {
            if *amount > MAX_AMOUNT_TOKEN {
                return Err(format!(
                    "Amount should be less than or equal to {}",
                    pretty_amount(MAX_AMOUNT_TOKEN)
                ));
            }

            let mut sr = match std::fs::read(record_file) {
                Ok(data) => from_reader(&data[..]).map_err(format_error)?,
                Err(_) => SendRecords(BTreeMap::new()),
            };
            let agent = DMsgAgent {
                agent,
                canister_id: MSG_CANISTER,
            };

            for username in usernames {
                match agent.get_user_by_username(username).await {
                    Err(err) => {
                        println!("invalid username {}, error: {}", username, err);
                    }
                    Ok(user) => {
                        if let Some(info) = sr.0.get(username) {
                            let utc: DateTime<Utc> =
                                Utc.timestamp_millis_opt(info.2 as i64).unwrap();
                            println!(
                                "{} already sent: {}, amount: {}, time: {}",
                                username,
                                info.0.to_text(),
                                pretty_amount(info.1),
                                utc.to_rfc3339()
                            );
                            continue;
                        }

                        let amount_e8s = *amount * TOKEN_1;
                        let start_at = SystemTime::now();
                        let now = start_at.duration_since(UNIX_EPOCH).map_err(format_error)?;
                        let now = now.as_millis() as u64;
                        let _ = agent
                            .send_token_to(&TOKEN_CANISTER, user.id, amount_e8s.into())
                            .await?;
                        sr.0.insert(username.clone(), (user.id, amount_e8s, now));
                        let utc: DateTime<Utc> = Utc.timestamp_millis_opt(now as i64).unwrap();
                        println!(
                            "{} sent: {}, amount: {}, time: {}",
                            username,
                            user.id.to_text(),
                            pretty_amount(amount_e8s),
                            utc.to_rfc3339()
                        );
                        std::fs::write(record_file, to_cbor_bytes(&sr)).map_err(format_error)?;
                    }
                };
            }

            let send_total: u64 = sr.0.iter().map(|(_, (_, amount, _))| *amount).sum();
            println!("Sent count: {}", sr.0.len());
            println!("Total sent: {}", pretty_amount(send_total));
        }

        None => {}
    }
    Ok(())
}

fn load_identity(path: &str) -> Result<Box<dyn Identity>, String> {
    if path == "Anonymous" {
        return Ok(Box::new(AnonymousIdentity));
    }

    let content = std::fs::read_to_string(path).map_err(format_error)?;
    BasicIdentity::from_pem(content.as_bytes())
        .map(|id| Box::new(id) as Box<dyn Identity>)
        .map_err(format_error)
}

pub fn format_error<T>(err: T) -> String
where
    T: std::fmt::Debug,
{
    format!("{:?}", err)
}

fn pretty_amount(amount: u64) -> String {
    let amount = amount as f64 / 100_000_000.0;
    if amount > 1000000.0 {
        format!("{:.2}M", amount / 1_000_000.0)
    } else {
        format!("{:.2}K", amount / 1000.0)
    }
}

pub fn to_cbor_bytes(obj: &impl Serialize) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    into_writer(obj, &mut buf).expect("failed to encode in CBOR format");
    buf
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

pub fn valid_principal(principal: &Principal) -> bool {
    principal.as_slice().len() >= 28
}

pub fn nat_to_u64(nat: &Nat) -> u64 {
    nat.0.to_u64().unwrap_or(0)
}
