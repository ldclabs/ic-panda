use candid::{pretty::candid::value::pp_value, CandidType, IDLValue, Nat, Principal};
use ciborium::into_writer;
use clap::{Parser, Subcommand};
use ic_agent::identity::AnonymousIdentity;
use ic_icrc1::Operation;
use icrc_ledger_types::icrc1::account::Account;
use num_traits::{cast::ToPrimitive, Saturating};
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};
use sha2::Digest;
use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

mod agent;
mod block;
mod ledger;
mod neuron;

use agent::build_agent;
use block::BlocksClient;
use ledger::Icrc1Agent;
use neuron::{ListNeurons, Neuron, NeuronAgent, NeuronId};

static IC_HOST: &str = "https://icp-api.io";
const SNAPSHOT_TIME: u64 = 1730419200; // '2024-10-31T24:00:00.000Z'
const SECOND: u64 = 1_000_000_000;
const DEFAULT_FEE: u64 = 10_000; // 0.0001 PANDA
const MIN_AIRDROP_BALANCE: u64 = 1000_000_000_000; // 10000 PANDA

// "druyg-tyaaa-aaaaq-aactq-cai" PANDA token canister id
static TOKEN_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 0, 0, 167, 1, 1]);
// "dwv6s-6aaaa-aaaaq-aacta-cai" ICPanda DAO canister id
static DAO_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 0, 0, 166, 1, 1]);

// owner -> (amount_e8s, opt subaccount, opt neuronid)
#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct Airdrop(pub u64, pub Option<ByteArray<32>>, pub Option<ByteBuf>);

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    Neurons {},
    Ledger {
        /// blocks store directory
        #[arg(long)]
        store: String,
    },
    Sync {
        /// blocks store directory
        #[arg(long)]
        store: String,
    },
}

// cargo run -p cli_airdrop -- neurons
// cargo run -p cli_airdrop -- sync --store ./debug/panda_blocks
// cargo run -p cli_airdrop -- ledger --store ./debug/panda_blocks
#[tokio::main]
async fn main() -> Result<(), String> {
    let cli = Cli::parse();
    let agent = build_agent(IC_HOST, Box::new(AnonymousIdentity)).await?;
    let start_at = SystemTime::now();
    let now = start_at.duration_since(UNIX_EPOCH).map_err(format_error)?;
    let now = now.as_secs();
    let snapshot = SNAPSHOT_TIME.min(now);

    match &cli.command {
        Some(Commands::Neurons {}) => {
            let cli = NeuronAgent {
                agent,
                canister_id: DAO_CANISTER,
            };

            let mut logs: String = "".to_string();
            let mut neurons: Vec<Neuron> = vec![];
            let mut airdrops: BTreeMap<Principal, Vec<Airdrop>> = BTreeMap::new();
            let mut start_page_at: Option<NeuronId> = None;
            let mut total_e8s = 0u64;
            loop {
                let res = cli
                    .list_neurons(ListNeurons {
                        limit: 0,
                        start_page_at: start_page_at.clone(),
                        of_principal: None,
                    })
                    .await?;
                if res.neurons.is_empty() {
                    break;
                }

                start_page_at = res.neurons.last().unwrap().id.clone();
                for neuron in res.neurons {
                    if let Some((neuron_id, principal)) = neuron.get_id() {
                        let amount = neuron.voting_power(snapshot);
                        if amount > 0 {
                            let log = format!(
                                "{} <- {}, {}\n",
                                principal.to_text(),
                                pretty_amount(amount),
                                hex::encode(&neuron_id),
                            );
                            logs.push_str(&log);
                            print!("{}", log);
                            total_e8s += amount;
                            airdrops
                                .entry(principal)
                                .or_insert_with(|| vec![])
                                .push(Airdrop(amount, None, Some(neuron_id)));
                        }
                    }
                    // pretty_println(&neuron)?;
                    neurons.push(neuron);
                }
            }

            let airdrops_count: u64 = airdrops.values().map(|v| v.len() as u64).sum();
            let airdrops = to_cbor_bytes(&airdrops);
            let h = sha256(&airdrops);
            let h = hex::encode(h);
            std::fs::write(
                format!("neurons_{}.cbor.{}", now, h),
                to_cbor_bytes(&neurons),
            )
            .map_err(format_error)?;

            std::fs::write(format!("neurons_airdrops_{}.cbor.{}", now, h), airdrops)
                .map_err(format_error)?;
            std::fs::write(format!("neurons_logs_{}.txt", now), logs.as_bytes())
                .map_err(format_error)?;

            println!(
                "neurons: {}, airdrop neurons: {}, total: {} ({}), time elapsed: {:?}",
                neurons.len(),
                airdrops_count,
                pretty_amount(total_e8s),
                total_e8s,
                start_at.elapsed()
            );
        }

        Some(Commands::Ledger { store }) => {
            let cli = BlocksClient::new(
                Icrc1Agent {
                    agent,
                    canister_id: TOKEN_CANISTER,
                },
                store,
            );

            let iter = cli.iter()?;
            let mut ledger: BTreeMap<Account, u64> = BTreeMap::new();

            let mut last_block = 0u64;
            for (index, block) in iter {
                last_block = index;
                if block.timestamp / SECOND > SNAPSHOT_TIME {
                    println!(
                        "Stop iter, block: {}, timestamp: {}, time elapsed: {:?}",
                        index,
                        block.timestamp,
                        start_at.elapsed()
                    );
                    break;
                }

                match block.transaction.operation {
                    Operation::Mint { to, amount } => {
                        if valid_principal(&to.owner) {
                            let v = ledger.entry(to).or_insert_with(|| 0);
                            *v = v.saturating_add(amount.to_u64());
                        }
                    }
                    Operation::Burn {
                        from,
                        spender: _,
                        amount,
                    } => {
                        if valid_principal(&from.owner) {
                            let v = ledger.entry(from).or_insert_with(|| 0);
                            *v = v.saturating_sub(amount.to_u64());
                        }
                    }
                    Operation::Transfer {
                        from,
                        to,
                        spender: _,
                        amount,
                        fee,
                    } => {
                        let fee = fee.map(|v| v.to_u64()).unwrap_or(DEFAULT_FEE);
                        if valid_principal(&from.owner) {
                            let v = ledger.entry(from).or_insert_with(|| 0);
                            *v = v.saturating_sub(amount.to_u64() + fee);
                        }
                        if valid_principal(&to.owner) {
                            let v = ledger.entry(to).or_insert_with(|| 0);
                            *v = v.saturating_add(amount.to_u64());
                        }
                    }
                    _ => continue,
                };
            }

            let mut logs: String = "".to_string();
            let mut airdrops: BTreeMap<Principal, Vec<Airdrop>> = BTreeMap::new();
            let mut total_e8s = 0u64;
            for (account, amount) in ledger.iter() {
                if amount >= &MIN_AIRDROP_BALANCE {
                    logs.push_str(&format!(
                        "{} <- {} ({}), {:?}\n",
                        account.owner.to_text(),
                        pretty_amount(*amount),
                        *amount,
                        account.subaccount.map(|v| hex::encode(v.as_slice()))
                    ));
                    total_e8s += *amount;
                    airdrops
                        .entry(account.owner.clone())
                        .or_insert_with(|| vec![])
                        .push(Airdrop(
                            *amount,
                            account.subaccount.map(ByteArray::from),
                            None,
                        ));
                }
            }

            let airdrops_count = airdrops.len();
            let airdrops = to_cbor_bytes(&airdrops);
            let h = sha256(&airdrops);
            let h = hex::encode(h);
            std::fs::write(format!("ledger_airdrops_{}.cbor.{}", now, h), airdrops)
                .map_err(format_error)?;
            std::fs::write(format!("ledger_logs_{}.txt", now), logs.as_bytes())
                .map_err(format_error)?;

            println!(
                "block: {}, airdrop accounts: {}, total: {} ({}), time elapsed: {:?}",
                last_block,
                airdrops_count,
                pretty_amount(total_e8s),
                total_e8s,
                start_at.elapsed()
            );
        }

        Some(Commands::Sync { store }) => {
            let cli = BlocksClient::new(
                Icrc1Agent {
                    agent,
                    canister_id: TOKEN_CANISTER,
                },
                store,
            );
            let tip = cli.start_synching_blocks().await?;
            println!(
                "Synced up to block {}, hash: {}, time elapsed: {:?}",
                tip.index,
                hex::encode(tip.hash.as_slice()),
                start_at.elapsed()
            );
        }

        None => {}
    }
    Ok(())
}

pub fn format_error<T>(err: T) -> String
where
    T: std::fmt::Debug,
{
    format!("{:?}", err)
}

fn pretty_println<T>(data: &T) -> Result<(), String>
where
    T: CandidType,
{
    let val = IDLValue::try_from_candid_type(data).map_err(format_error)?;
    let doc = pp_value(7, &val);
    println!("{}", doc.pretty(120));
    Ok(())
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
