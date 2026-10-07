//! Print the `masters` field of a `CoseInit` for one COSE canister.
//!
//! ```sh
//! cargo run -p dmsg_protocol --features cose-pins --example cose_pins -- \
//!   <cose-canister-id> Production [mainnet|pocketic] [key_1]
//! ```
use candid::Principal;
use dmsg_protocol::cose_pins::{master_key_pins, KeySource};
use dmsg_types::{cose::Algorithm, Environment};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage =
        "usage: cose_pins <cose-canister-id> <Production|Staging|Local> [mainnet|pocketic] [key name]";
    let (Some(canister), Some(environment)) = (args.first(), args.get(1)) else {
        eprintln!("{usage}");
        std::process::exit(2);
    };
    let canister = Principal::from_text(canister).expect("canister ID");
    let environment = match environment.as_str() {
        "Production" => Environment::Production,
        "Staging" => Environment::Staging,
        "Local" => Environment::Local,
        _ => panic!("{usage}"),
    };
    let source = match args.get(2).map_or("mainnet", String::as_str) {
        "mainnet" => KeySource::Mainnet,
        "pocketic" => KeySource::PocketIc,
        _ => panic!("{usage}"),
    };
    let key_name = args.get(3).map_or("key_1", String::as_str);
    let masters = master_key_pins(
        source,
        canister,
        &environment,
        key_name,
        &[
            Algorithm::Ed25519,
            Algorithm::EcdsaSecp256k1,
            Algorithm::VetKdBls12381,
        ],
    )
    .expect("known master key");
    // Keep only the algorithms the deployment configures.
    println!("masters = vec {{");
    for master in masters {
        let pin: String = master
            .expected_fingerprint
            .as_slice()
            .iter()
            .map(|b| format!("\\{b:02x}"))
            .collect();
        println!(
            "  record {{ algorithm = variant {{ {:?} }}; key_name = \"{}\"; expected_fingerprint = blob \"{pin}\" }};",
            master.algorithm, master.key_name
        );
    }
    println!("}};");
}
