//! Controllable SNS Candid fixture. Never deploy with production privileges.
use candid::{CandidType, Principal};
use dmsg_runtime::storage::Stored;
use dmsg_types::*;
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
thread_local! {
    static STATE: RefCell<StableCell<Stored<Option<Neuron>>, DefaultMemoryImpl>> = RefCell::new(StableCell::init(DefaultMemoryImpl::default(), Stored(None)));
    static PIN_ON_READ: RefCell<Option<(Principal,Hash)>> = const { RefCell::new(None) };
    static PAUSE_ON_READ: RefCell<Option<Principal>> = const { RefCell::new(None) };
}

#[ic_cdk::update]
fn set_neuron(neuron: Option<Neuron>) {
    STATE.with_borrow_mut(|t| t.set(Stored(neuron)));
}

#[ic_cdk::update]
async fn get_neuron(request: Request) -> Response {
    if let Some((membership, hash)) = PIN_ON_READ.with_borrow_mut(Option::take) {
        let result: Result<()> =
            dmsg_runtime::call(membership, "set_sns_governance_module_hash", (hash,))
                .await
                .unwrap();
        result.unwrap();
    }
    let pause = PAUSE_ON_READ.with_borrow_mut(Option::take);
    if let Some(membership) = pause {
        let result: Result<()> = dmsg_runtime::call(membership, "set_admission_pause", (true,))
            .await
            .unwrap();
        result.unwrap();
    }
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

#[ic_cdk::update]
fn pause_on_neuron_read(membership: Principal) {
    PAUSE_ON_READ.with_borrow_mut(|value| *value = Some(membership));
}

ic_cdk::export_candid!();

// Explicit test-only product authority; exact caller, account and prepared digest.
// This is a fault-injection fixture, never a production permission service.
#[derive(Clone)]
struct ActionApprovalFixture {
    home: Principal,
    account: AccountId,
    digest: Hash,
    change: Option<(Principal, dmsg_types::integration::AppRegistration)>,
}
thread_local! { static ACTION_APPROVAL: RefCell<Option<ActionApprovalFixture>> = const { RefCell::new(None) }; }
#[ic_cdk::update]
fn set_action_approval(
    home: Principal,
    account: AccountId,
    digest: Hash,
    change: Option<(Principal, dmsg_types::integration::AppRegistration)>,
) {
    ACTION_APPROVAL.with_borrow_mut(|s| {
        *s = Some(ActionApprovalFixture {
            home,
            account,
            digest,
            change,
        })
    });
}
#[ic_cdk::update]
async fn verify_dmsg_action(
    account: AccountId,
    action: dmsg_types::app_action::AppAction,
) -> Result<()> {
    let approval = ACTION_APPROVAL
        .with_borrow(Clone::clone)
        .ok_or(Error::Forbidden)?;
    ensure(
        ic_cdk::api::msg_caller() == approval.home
            && account == approval.account
            && dmsg_protocol::app_action::app_action_digest(&action) == approval.digest,
        Error::Forbidden,
    )?;
    if let Some((commerce, app)) = approval.change {
        let result: Result<()> =
            dmsg_runtime::call(commerce, "register_integration_app", (app,)).await?;
        result?;
    }
    Ok(())
}

#[ic_cdk::update]
fn change_pin_on_neuron_read(membership: Principal, hash: Hash) {
    PIN_ON_READ.with_borrow_mut(|v| *v = Some((membership, hash)));
}
