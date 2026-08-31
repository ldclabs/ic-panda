use candid::{CandidType, Principal};
use cbor2::to_writer;
use ic_auth_types::{Delegation, SignInResponse, SignedDelegation};
use ic_auth_verifier::{user_public_key_from_der, verify_basic_sig, Algorithm};
use ic_canister_sig_creation::{delegation_signature_msg, CanisterSigPublicKey};
use ic_cdk::call::Call;
use serde::Deserialize;
use serde_bytes::ByteBuf;

use crate::types::{Delegator, NameAccount};
use crate::{store, NAMECHAIN_CANISTER};

const NANOSECONDS_PER_MILLISECOND: u64 = 1_000_000;
const MAX_NAME_BYTES: usize = 64;
const MAX_PUBLIC_KEY_DER_BYTES: usize = 128;
const BASIC_SIGNATURE_BYTES: usize = 64;
const ED25519_DER_PREFIX: &[u8] = &[
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

#[derive(CandidType, Deserialize)]
struct NameOwner {
    id: Principal,
}

fn authenticated_caller() -> Result<Principal, String> {
    let caller = ic_cdk::api::msg_caller();
    if caller == Principal::anonymous() {
        return Err("anonymous caller is not allowed".to_string());
    }
    Ok(caller)
}

pub(crate) fn normalize_name(mut name: String) -> Result<String, String> {
    if name.is_empty() {
        return Err("name is empty".to_string());
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(format!("name exceeds {MAX_NAME_BYTES} bytes"));
    }

    name.make_ascii_lowercase();
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err("name contains invalid characters".to_string());
    }
    Ok(name)
}

fn validate_session_key(pubkey: &[u8], sig: &[u8]) -> Result<(), String> {
    if pubkey.is_empty() || pubkey.len() > MAX_PUBLIC_KEY_DER_BYTES {
        return Err("invalid public key length".to_string());
    }
    if sig.len() != BASIC_SIGNATURE_BYTES {
        return Err("invalid signature length".to_string());
    }
    Ok(())
}

fn verify_challenge(pubkey: &[u8], msg: &[u8], sig: &[u8]) -> Result<(), String> {
    if let Some(raw_key) = pubkey.strip_prefix(ED25519_DER_PREFIX) {
        if raw_key.len() == 32 {
            return verify_basic_sig(Algorithm::Ed25519, raw_key, msg, sig)
                .map_err(|err| format!("challenge verification failed: {err}"));
        }
    }

    let (algorithm, raw_key) =
        user_public_key_from_der(pubkey).map_err(|err| format!("invalid public key: {err}"))?;
    verify_basic_sig(algorithm, &raw_key, msg, sig)
        .map_err(|err| format!("challenge verification failed: {err}"))
}

async fn get_name_owner(name: &str) -> Result<Principal, String> {
    let response = Call::bounded_wait(NAMECHAIN_CANISTER, "get_by_username")
        .with_arg(name)
        .await
        .map_err(|err| format!("failed to query name owner: {err}"))?;
    let result: Result<NameOwner, String> = response
        .candid()
        .map_err(|err| format!("failed to decode name owner: {err}"))?;
    result.map(|owner| owner.id)
}

#[ic_cdk::query]
fn get_state() -> Result<store::State, String> {
    Ok(store::state::with(|s| s.clone()))
}

#[ic_cdk::query]
fn whoami() -> Result<Principal, String> {
    Ok(ic_cdk::api::msg_caller())
}

#[ic_cdk::query]
fn get_principal(name: String) -> Result<Principal, String> {
    let name = normalize_name(name)?;
    let user_key = CanisterSigPublicKey::new(ic_cdk::api::canister_self(), name.into_bytes());
    Ok(Principal::self_authenticating(user_key.to_der().as_slice()))
}

#[ic_cdk::query]
fn get_delegators(name: String) -> Result<Vec<Delegator>, String> {
    let name = normalize_name(name)?;
    let res = store::state::get_delegations(&name).ok_or_else(|| "name not found".to_string())?;

    Ok(res.delegators())
}

#[ic_cdk::query]
fn get_my_accounts() -> Result<Vec<NameAccount>, String> {
    let caller = ic_cdk::api::msg_caller();
    let canister = ic_cdk::api::canister_self();
    let names = store::state::get_names(&caller).unwrap_or_default();
    Ok(names
        .into_iter()
        .map(|name| {
            let user_key = CanisterSigPublicKey::new(canister, name.as_bytes().to_vec());
            let account = Principal::self_authenticating(user_key.to_der().as_slice());
            NameAccount { name, account }
        })
        .collect())
}

#[ic_cdk::update]
async fn activate_name(name: String) -> Result<Vec<Delegator>, String> {
    let caller = authenticated_caller()?;

    let name = normalize_name(name)?;
    if store::state::name_exists(&name) {
        return Err("name is already activated".to_string());
    }

    if get_name_owner(&name).await? != caller {
        return Err("caller is not the owner of the name".to_string());
    }
    // The store rechecks existence because the outbound call yielded execution.
    store::state::activate_name(&name, &caller)
}

#[ic_cdk::update]
fn add_delegator(name: String, delegator: Principal, role: i8) -> Result<Vec<Delegator>, String> {
    if !(-1..=1).contains(&role) {
        return Err(format!("invalid role: {role}"));
    }

    let caller = authenticated_caller()?;

    if delegator == Principal::anonymous() {
        return Err("anonymous delegator is not allowed".to_string());
    }
    let name = normalize_name(name)?;
    store::state::add_delegator(&name, &caller, &delegator, role)
}

#[ic_cdk::update]
fn remove_delegator(name: String, delegator: Principal) -> Result<(), String> {
    let caller = authenticated_caller()?;
    let name = normalize_name(name)?;
    store::state::remove_delegator(&name, &caller, &delegator)
}

#[ic_cdk::update]
fn leave_delegation(name: String) -> Result<(), String> {
    let caller = authenticated_caller()?;
    let name = normalize_name(name)?;
    store::state::remove_delegator(&name, &caller, &caller)
}

#[ic_cdk::update]
fn sign_in(name: String, pubkey: ByteBuf, sig: ByteBuf) -> Result<SignInResponse, String> {
    let caller = authenticated_caller()?;
    let now_ms = ic_cdk::api::time() / NANOSECONDS_PER_MILLISECOND;
    let name = normalize_name(name)?;
    validate_session_key(pubkey.as_slice(), sig.as_slice())?;

    // Reject unauthorized callers before performing comparatively expensive cryptography.
    let mut delegations = store::state::get_delegations(&name)
        .ok_or_else(|| "caller is not authorized".to_string())?;
    delegations.record_sign_in(&caller, now_ms)?;

    let mut msg = Vec::with_capacity(name.len() + caller.as_slice().len() + 16);
    to_writer(&(&name, &caller), &mut msg)
        .map_err(|err| format!("failed to encode challenge: {err}"))?;
    verify_challenge(pubkey.as_slice(), &msg, sig.as_slice())?;

    let user_key =
        CanisterSigPublicKey::new(ic_cdk::api::canister_self(), name.as_bytes().to_vec());
    let session_expires_in_ms = store::state::with(|state| state.session_expires_in_ms);
    let expiration = now_ms
        .checked_add(session_expires_in_ms)
        .and_then(|time| time.checked_mul(NANOSECONDS_PER_MILLISECOND))
        .ok_or_else(|| "session expiration overflow".to_string())?;
    let delegation_hash = delegation_signature_msg(pubkey.as_slice(), expiration, None);

    store::state::set_delegations(&name, delegations);
    store::state::with_mut(|state| {
        state.sign_in_count = state.sign_in_count.saturating_add(1);
    });
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
    if seed.is_empty() || seed.len() > MAX_NAME_BYTES {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(CandidType)]
    struct FullUserInfo {
        id: Principal,
        name: String,
        image: String,
    }

    #[test]
    fn normalizes_and_validates_names() {
        assert_eq!(normalize_name("Alice_01".to_string()).unwrap(), "alice_01");
        assert!(normalize_name(String::new()).is_err());
        assert!(normalize_name("alice-bob".to_string()).is_err());
        assert!(normalize_name("a".repeat(MAX_NAME_BYTES + 1)).is_err());
    }

    #[test]
    fn name_owner_decodes_from_a_wider_candid_record() {
        let principal = Principal::from_slice(&[1, 2, 3]);
        let bytes = candid::encode_one(Result::<FullUserInfo, String>::Ok(FullUserInfo {
            id: principal,
            name: "Alice".to_string(),
            image: "avatar".to_string(),
        }))
        .unwrap();

        let decoded: Result<NameOwner, String> = candid::decode_one(&bytes).unwrap();
        assert_eq!(decoded.unwrap().id, principal);
    }
}
