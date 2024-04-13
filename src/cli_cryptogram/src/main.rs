use base64::{engine::general_purpose, Engine};
use candid::Principal;
use clap::{Parser, Subcommand};
use lib_panda::Cryptogram;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    Sign {
        #[arg(short)]
        principals: Vec<String>,
    },
    Verify {},
}
fn main() {
    let cli = Cli::parse();
    let key = std::env::var("AIRDROP_KEY").expect("AIRDROP_KEY is not set");
    let key = general_purpose::URL_SAFE_NO_PAD
        .decode(key)
        .expect("failed to decode AIRDROP_KEY");
    if key.len() != 32 {
        panic!("AIRDROP_KEY should be 32 bytes");
    }

    match &cli.command {
        Some(Commands::Sign { principals }) => {
            if principals.is_empty() {
                panic!("principals is empty");
            }
            let mut principals = principals.clone();
            if principals.len() == 1 {
                principals = principals[0]
                    .split_terminator(&[' ', ',', ';'][..])
                    .map(|s| s.trim().to_string())
                    .collect();
            }
            let principals: Vec<Principal> = principals
                .into_iter()
                .map(|p| Principal::from_text(p).expect("invalid principal"))
                .collect();
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before Unix epoch");
            let ts = (ts.as_secs() / 60) as u32;
            let prize = Prize(0, ts, 4320, 0, 0);
            for principal in principals {
                let cryptogram = prize.encode(&key, Some(principal));
                println!("{}: {}", principal, cryptogram);
            }
        }
        Some(Commands::Verify {}) => {
            println!("TODO");
        }

        None => {}
    }
}

// Prize format: (Issuer code, Issue time, Expire, Claimable amount, Quantity)
// Issuer code: The lucky code of the issuer, 0 for system
// Issue time: The issue time of the prize, in minutes since UNIX epoch
// Expire: The expire duration in minutes
// Claimable amount: The amount of tokens that can be claimed by users, in PANDA * 1000
// Quantity: How many users can claim the prize
//
// System can only issue prizes for free airdrop with Prize(0, Issue time, expire, 0, 0).
// This prizes will not be stored.
#[derive(Clone, Deserialize, Serialize, Ord, PartialOrd, Eq, PartialEq)]
pub struct Prize(pub u32, pub u32, pub u16, pub u32, pub u16);

impl Prize {
    pub fn is_valid(&self, now_sec: u64) -> bool {
        (self.1 + self.2 as u32) >= (now_sec / 60) as u32
    }

    pub fn is_valid_system(&self, now_sec: u64) -> bool {
        self.0 == 0 && self.3 == 0 && self.4 == 0 && self.is_valid(now_sec)
    }
}
