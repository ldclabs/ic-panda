use candid::Principal;
use cbor2::from_slice;
use ic_auth_types::{ByteArrayB64, ByteBufB64, Delegation, SignedDelegation};
use ic_auth_verifier::SignedEnvelope;
use ic_canister_sig_creation::delegation_signature_msg;
use lib_panda::mac_256;
use serde_bytes::ByteBuf;

use crate::{
    helper::{
        authenticated_caller, MAX_PUBLIC_KEY_DER_BYTES, MAX_SIGNED_ENVELOPE_BYTES,
        NANOSECONDS_PER_MILLISECOND,
    },
    store,
};

const MAX_DELEGATION_SEED_BYTES: usize = 32;

#[ic_cdk::query]
fn info() -> Result<store::StateInfo, String> {
    Ok(store::state::with(|s| s.into()))
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

#[ic_cdk::query]
pub fn verify_envelope(
    signed_envelope: ByteBufB64,
    expect_target: Option<Principal>,
    expect_digest: Option<ByteArrayB64<32>>,
) -> Result<Principal, String> {
    if signed_envelope.len() > MAX_SIGNED_ENVELOPE_BYTES {
        return Err("signed envelope is too large".to_string());
    }
    let now_ms = ic_cdk::api::time() / NANOSECONDS_PER_MILLISECOND;
    let signed_envelope: SignedEnvelope = from_slice(signed_envelope.as_slice())
        .map_err(|err| format!("failed to decode signed envelope: {err}"))?;
    signed_envelope.verify(
        now_ms,
        expect_target,
        expect_digest.as_ref().map(|d| d.as_slice()),
    )?;
    Ok(Principal::self_authenticating(&signed_envelope.pubkey))
}

#[ic_cdk::query]
fn my_iv() -> Result<ByteArrayB64<32>, String> {
    let caller = authenticated_caller()?;
    store::state::with(|s| {
        if s.nonce_iv.as_slice().iter().all(|byte| *byte == 0) {
            return Err("canister is still initializing".to_string());
        }
        let pk = mac_256(s.nonce_iv.as_slice(), caller.as_slice());
        Ok(pk.into())
    })
}
