//! Local fault gate for legacy freeze tests. Never deploy with real data.
use candid::CandidType;
use ic_oss_types::{
    cose::Token,
    folder::{CreateFolderInput, CreateFolderOutput},
};
use serde::Deserialize;
use serde_bytes::ByteBuf;
use std::cell::Cell;

thread_local! {
    static PAUSED: Cell<bool> = const { Cell::new(false) };
}

#[derive(CandidType, Deserialize)]
enum NamespaceReply {
    Err(String),
}

#[derive(CandidType, Deserialize)]
struct NameOnly {
    name: String,
}

#[ic_cdk::update]
fn admin_create_namespace(_input: NameOnly) -> NamespaceReply {
    NamespaceReply::Err("synthetic existing namespace".into())
}

#[ic_cdk::update]
fn pause() {
    PAUSED.with(|p| p.set(true));
}

#[ic_cdk::update]
fn release() {
    PAUSED.with(|p| p.set(false));
}

#[ic_cdk::update]
fn barrier() {}

#[ic_cdk::update]
async fn admin_weak_access_token(
    _token: Token,
    _issued_at: u64,
    _ttl: u64,
) -> Result<ByteBuf, String> {
    let canister = ic_cdk::api::canister_self();
    // Test-only remote-call gate. CDK protected method futures must be driven
    // by inter-canister callbacks, not a waker from an unrelated update.
    while PAUSED.with(Cell::get) {
        ic_cdk::call::Call::bounded_wait(canister, "barrier")
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(ByteBuf::from(vec![1, 2, 3]))
}

#[ic_cdk::update]
fn create_folder(
    _input: CreateFolderInput,
    _token: Option<ByteBuf>,
) -> Result<CreateFolderOutput, String> {
    Ok(CreateFolderOutput {
        id: 1,
        created_at: ic_cdk::api::time() / 1_000_000,
    })
}

ic_cdk::export_candid!();
