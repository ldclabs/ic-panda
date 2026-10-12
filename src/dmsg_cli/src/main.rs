//! `dmsg`: the dMsg command line client. From work package W1 the same binary
//! also runs the daemon (`dmsg daemon`) that holds every key and all state.

use clap::Parser;

/// dMsg command line client.
#[derive(Parser)]
#[command(name = "dmsg", version, about)]
struct Cli {}

fn main() {
    Cli::parse();
    println!("dmsg {}", env!("CARGO_PKG_VERSION"));
}
