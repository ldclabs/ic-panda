use candid::Principal;
use ciborium::from_reader;
use ic_auth_types::{ByteArrayB64, ByteBufB64, Delegation, SignedDelegation};
use ic_auth_verifier::SignedEnvelope;
use ic_canister_sig_creation::delegation_signature_msg;
use lib_panda::mac_256;
use serde_bytes::ByteBuf;

use crate::{helper::msg_caller, store};

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
    let delegation_hash = delegation_signature_msg(pubkey.as_slice(), expiration, None);
    let signature = store::state::get_signature(seed.as_slice(), delegation_hash.as_slice())?;

    Ok(SignedDelegation {
        delegation: Delegation {
            pubkey: pubkey.into(),
            expiration,
            targets: None,
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
    let now_ms = ic_cdk::api::time() / 1_000_000;
    let signed_envelope: SignedEnvelope = from_reader(signed_envelope.as_slice())
        .map_err(|e| format!("failed to decode signed envelope: {:?}", e))?;
    signed_envelope.verify(
        now_ms,
        expect_target,
        expect_digest.as_ref().map(|d| d.as_slice()),
    )?;
    Ok(Principal::self_authenticating(&signed_envelope.pubkey))
}

#[ic_cdk::query]
fn my_iv() -> Result<ByteArrayB64<32>, String> {
    let caller = msg_caller()?;
    store::state::with(|s| {
        let pk = mac_256(s.nonce_iv.as_slice(), caller.as_slice());
        Ok(pk.into())
    })
}
