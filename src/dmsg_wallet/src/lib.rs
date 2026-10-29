//! dMsg team wallet: one canister per team. Members propose intents and
//! approve them under the wallet's policy; the canister moves ICP and ICRC
//! assets itself and signs Bitcoin, EVM and Solana transactions with chain
//! keys for members to broadcast. It makes no HTTPS outcalls.
mod api;
mod chains;
mod execute;
mod keys;
mod ledger;
mod stable_codec;
mod store;

use candid::Principal;
use dmsg_types::{wallet::*, *};

/// Reject ingress that cannot succeed before it costs cycles: decisions and
/// executions from Principals bound to no member.
#[ic_cdk::inspect_message]
fn inspect_message() {
    let caller = ic_cdk::api::msg_caller();
    let method = ic_cdk::api::msg_method_name();
    let open = matches!(
        method.as_str(),
        "join" | "unbind" | "bootstrap" | "refuel" | "prune"
    );
    let member =
        store::with(|w| caller == w.settings.hub || w.members.iter().any(|m| m.binds(&caller)));
    if caller != Principal::anonymous() && (open || member) {
        ic_cdk::api::accept_message();
    }
}

ic_cdk::export_candid!();
