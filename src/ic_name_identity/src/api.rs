use candid::Principal;
use ciborium::into_writer;
use ic_canister_sig_creation::{delegation_signature_msg, CanisterSigPublicKey};
use ic_crypto_standalone_sig_verifier::{
    user_public_key_from_bytes, verify_basic_sig_by_public_key,
};
use ic_message_types::profile::UserInfo;
use serde_bytes::ByteBuf;

use crate::types::{Delegation, Delegator, NameAccount, SignInResponse, SignedDelegation};
use crate::{call, store, NAMECHAIN_CANISTER};

const MILLISECONDS: u64 = 1000000;

#[ic_cdk::query]
fn get_state() -> Result<store::State, String> {
    Ok(store::state::with(|s| s.clone()))
}

#[ic_cdk::query]
fn whoami() -> Result<Principal, String> {
    Ok(ic_cdk::caller())
}

#[ic_cdk::query]
fn get_principal(name: String) -> Result<Principal, String> {
    let user_key =
        CanisterSigPublicKey::new(ic_cdk::id(), name.to_ascii_lowercase().as_bytes().to_vec());
    Ok(Principal::self_authenticating(user_key.to_der().as_slice()))
}

#[ic_cdk::query]
fn get_delegators(name: String) -> Result<Vec<Delegator>, String> {
    let name = name.to_ascii_lowercase();
    let res = store::state::get_delegations(&name).ok_or_else(|| "name not found".to_string())?;

    Ok(res.delegators())
}

#[ic_cdk::query]
fn get_my_accounts() -> Result<Vec<NameAccount>, String> {
    let caller = ic_cdk::caller();
    let canister = ic_cdk::id();
    let names = store::state::get_names(&caller).unwrap_or_default();
    Ok(names
        .into_iter()
        .map(|name| {
            let user_key =
                CanisterSigPublicKey::new(canister, name.to_ascii_lowercase().as_bytes().to_vec());
            let account = Principal::self_authenticating(user_key.to_der().as_slice());
            NameAccount { name, account }
        })
        .collect())
}

#[ic_cdk::update]
async fn activate_name(name: String) -> Result<Vec<Delegator>, String> {
    let caller = ic_cdk::caller();

    let name = name.to_ascii_lowercase();
    let delegations = store::state::get_delegations(&name);
    if delegations.is_some() {
        return Err("name is already activated".to_string());
    }

    let res: Result<UserInfo, String> =
        call(NAMECHAIN_CANISTER, "get_by_username", (name.clone(),), 0).await?;
    let res = res?;
    if res.id != caller {
        return Err("caller is not the owner of the name".to_string());
    }

    store::state::add_delegator(&name, &caller, 1)
}

#[ic_cdk::update]
fn add_delegator(name: String, delegator: Principal, role: i8) -> Result<Vec<Delegator>, String> {
    if !(-1..=1).contains(&role) {
        return Err(format!("invalid status, {}", role));
    }

    let caller = ic_cdk::caller();

    let name = name.to_ascii_lowercase();
    let delegations =
        store::state::get_delegations(&name).ok_or_else(|| "name not found".to_string())?;
    if !delegations.has_permission(&caller, 1) {
        return Err("caller is not a manager".to_string());
    }

    store::state::add_delegator(&name, &delegator, role)
}

#[ic_cdk::update]
fn remove_delegator(name: String, delegator: Principal) -> Result<(), String> {
    let caller = ic_cdk::caller();
    let name = name.to_ascii_lowercase();
    store::state::remove_delegator(&name, &caller, &delegator)
}

#[ic_cdk::update]
fn leave_delegation(name: String) -> Result<(), String> {
    let caller = ic_cdk::caller();
    let name = name.to_ascii_lowercase();
    store::state::remove_delegator(&name, &caller, &caller)
}

#[ic_cdk::update]
fn sign_in(name: String, pubkey: ByteBuf, sig: ByteBuf) -> Result<SignInResponse, String> {
    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    let name = name.to_ascii_lowercase();

    let (pk, _) = user_public_key_from_bytes(pubkey.as_slice())
        .map_err(|err| format!("invalid public key: {:?}", err))?;
    let mut msg = vec![];
    into_writer(&(&name, &caller), &mut msg).expect("failed to encode Delegations data");
    verify_basic_sig_by_public_key(pk.algorithm_id, &msg, sig.as_slice(), &pk.key)
        .map_err(|err| format!("challenge verification failed: {:?}", err))?;

    let user_key = CanisterSigPublicKey::new(ic_cdk::id(), name.as_bytes().to_vec());
    store::state::delegator_sign_in(&name, &caller, now_ms)?;
    let session_expires_in_ms = store::state::with_mut(|state| {
        state.sign_in_count = state.sign_in_count.saturating_add(1);
        state.session_expires_in_ms
    });
    let expiration = (now_ms + session_expires_in_ms) * MILLISECONDS;
    let delegation_hash = delegation_signature_msg(pubkey.as_slice(), expiration, None);
    store::state::add_signature(user_key.seed.as_slice(), delegation_hash.as_slice());

    Ok(SignInResponse {
        expiration,
        user_key: user_key.to_der().into(),
        seed: user_key.seed.into(),
    })
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
            pubkey,
            expiration,
            targets: None,
        },
        signature: ByteBuf::from(signature),
    })
}
