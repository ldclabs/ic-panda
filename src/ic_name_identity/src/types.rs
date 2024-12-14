use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

#[derive(CandidType, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Delegation {
    pub pubkey: ByteBuf,
    pub expiration: u64,
    pub targets: Option<Vec<Principal>>,
}

#[derive(CandidType, Clone, Deserialize, Serialize)]
pub struct NameAccount {
    pub name: String,
    pub account: Principal,
}

#[derive(CandidType, Clone, Deserialize, Serialize)]
pub struct Delegator {
    pub owner: Principal,
    pub sign_in_at: u64, // milliseconds since epoch
    pub role: i8,        // -1: suspend; 0: member; 1: owner
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SignedDelegation {
    pub delegation: Delegation,
    pub signature: ByteBuf,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SignInResponse {
    /// The session expiration time in nanoseconds since the UNIX epoch. This is the time at which
    /// the delegation will no longer be valid.
    pub expiration: u64,
    /// The user canister public key. This key is used to derive the user principal.
    pub user_key: ByteBuf,
    /// seed is a part of the user_key
    pub seed: ByteBuf,
}
