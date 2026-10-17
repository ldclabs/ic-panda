use candid::Principal;
use ic_auth_types::{Delegation, SignedDelegation};
use ic_canister_sig_creation::delegation_signature_msg;
use serde_bytes::ByteBuf;

use crate::{api_admin::is_controller, helper::MAX_PUBLIC_KEY_DER_BYTES, store};

const MAX_DELEGATION_SEED_BYTES: usize = 32;
// The largest sign-in arguments are a 255-byte domain, a 44-byte address, a
// 128-byte session key and a 65-byte signature: about 530 bytes of Candid.
const MAX_SIGN_IN_ARG_BYTES: usize = 1_024;

/// Accept only the ingress a method would serve, so nobody makes this canister
/// pay for ingress: bounded sign-in calls from anyone, and everything else
/// (admin methods, validators, queries sent as update calls) from a controller
/// or governance. This runs on one replica and is not a security boundary;
/// every method still checks its input and caller.
#[ic_cdk::inspect_message]
fn inspect_message() {
    let allowed = match ic_cdk::api::msg_method_name().as_str() {
        "sign_in_with_ethereum" | "sign_in_with_solana" => {
            ic_cdk::api::msg_arg_data().len() <= MAX_SIGN_IN_ARG_BYTES
        }
        _ => is_controller().is_ok(),
    };
    if allowed {
        ic_cdk::api::accept_message();
    }
}

#[ic_cdk::query]
fn info() -> Result<store::State, String> {
    Ok(store::state::with(Clone::clone))
}

#[ic_cdk::query]
fn whoami() -> Result<Principal, String> {
    Ok(ic_cdk::api::msg_caller())
}

#[ic_cdk::query]
fn get_delegation(
    seed: ByteBuf,
    pubkey: ByteBuf,
    expiration: u64,
) -> Result<SignedDelegation, String> {
    if seed.is_empty() || seed.len() > MAX_DELEGATION_SEED_BYTES {
        return Err("invalid seed length".to_string());
    }
    if pubkey.is_empty() || pubkey.len() > MAX_PUBLIC_KEY_DER_BYTES {
        return Err("invalid public key length".to_string());
    }
    let delegation_hash = delegation_signature_msg(pubkey.as_slice(), expiration, None);
    let signature = store::state::get_signature(seed.as_slice(), delegation_hash.as_slice())?;

    Ok(SignedDelegation {
        delegation: Delegation {
            pubkey: pubkey.into(),
            expiration,
            targets: None,
            permissions: None,
        },
        signature: signature.into(),
    })
}
