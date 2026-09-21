//! Controllable SNS Candid fixture. Never deploy with production privileges.
use candid::{CandidType, Principal};
use dmsg_runtime::storage::Stored;
use ic_stable_structures::{DefaultMemoryImpl, StableCell};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
#[derive(Clone, CandidType, Serialize, Deserialize)]
struct NeuronId {
    id: Vec<u8>,
}
#[derive(Clone, CandidType, Serialize, Deserialize)]
struct Permission {
    principal: Option<Principal>,
    permission_type: Vec<i32>,
}
#[derive(Clone, CandidType, Serialize, Deserialize)]
enum DissolveState {
    DissolveDelaySeconds(u64),
    WhenDissolvedTimestampSeconds(u64),
}
#[derive(Clone, CandidType, Serialize, Deserialize)]
struct Neuron {
    id: Option<NeuronId>,
    permissions: Vec<Permission>,
    cached_neuron_stake_e8s: u64,
    neuron_fees_e8s: u64,
    dissolve_state: Option<DissolveState>,
}
#[derive(CandidType, Deserialize)]
struct Request {
    neuron_id: Option<NeuronId>,
}
#[derive(CandidType, Deserialize)]
struct Empty {}
#[derive(CandidType, Deserialize)]
struct SnsCanisters {
    root: Option<Principal>,
    governance: Option<Principal>,
    ledger: Option<Principal>,
}
#[derive(CandidType, Deserialize)]
struct Response {
    result: Option<NeuronResult>,
}
#[derive(CandidType, Deserialize)]
enum NeuronResult {
    Neuron(Neuron),
}
thread_local! {static STATE:RefCell<StableCell<Stored<Option<Neuron>>,DefaultMemoryImpl>>=RefCell::new(StableCell::init(DefaultMemoryImpl::default(),Stored(None)));}
#[ic_cdk::update]
fn set_neuron(neuron: Option<Neuron>) {
    STATE.with_borrow_mut(|t| t.set(Stored(neuron)));
}
#[ic_cdk::query]
fn get_neuron(request: Request) -> Response {
    Response {
        result: STATE
            .with_borrow(|t| t.get().0.clone())
            .filter(|n| n.id.as_ref().map(|n| &n.id) == request.neuron_id.as_ref().map(|n| &n.id))
            .map(NeuronResult::Neuron),
    }
}
#[ic_cdk::query]
fn list_sns_canisters(_: Empty) -> SnsCanisters {
    let id = Some(ic_cdk::api::canister_self());
    SnsCanisters {
        root: id,
        governance: id,
        ledger: id,
    }
}
#[ic_cdk::query]
fn icrc1_decimals() -> u8 {
    8
}
ic_cdk::export_candid!();
