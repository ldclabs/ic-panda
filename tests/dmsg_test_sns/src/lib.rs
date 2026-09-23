//! Controllable SNS Candid fixture. Never deploy with production privileges.
use candid::{CandidType, Principal};
use dmsg_protocol::membership::{decision_digest, membership_intent_digest};
use dmsg_runtime::storage::Stored;
use dmsg_types::{membership::*, *};
use ic_stable_structures::{DefaultMemoryImpl, StableCell};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap};
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
#[derive(Default)]
struct Adapter {
    receipts: BTreeMap<Hash, MembershipDecisionReceipt>,
    applies: u64,
    reads: u64,
    lose_reply: bool,
    unavailable: bool,
    reject_close: bool,
}

thread_local! {
    static STATE: RefCell<StableCell<Stored<Option<Neuron>>, DefaultMemoryImpl>> = RefCell::new(StableCell::init(DefaultMemoryImpl::default(), Stored(None)));
    static ADAPTER: RefCell<Adapter> = RefCell::new(Adapter::default());
    static PAUSE_ON_READ: RefCell<Option<Principal>> = const { RefCell::new(None) };
}

#[ic_cdk::update]
fn set_neuron(neuron: Option<Neuron>) {
    STATE.with_borrow_mut(|t| t.set(Stored(neuron)));
}

#[ic_cdk::update]
async fn get_neuron(request: Request) -> Response {
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

#[ic_cdk::update]
fn set_adapter_faults(lose_reply: bool, unavailable: bool, reject_close: bool) {
    ADAPTER.with_borrow_mut(|a| {
        a.lose_reply = lose_reply;
        a.unavailable = unavailable;
        a.reject_close = reject_close;
    });
}

fn authorize(intent: MembershipIntent) -> Result<MembershipAuthorization> {
    let at = nanos_to_millis(ic_cdk::api::time());
    ensure(at < intent.valid_until_ms, Error::Expired)?;
    Ok(MembershipAuthorization {
        intent_digest: membership_intent_digest(&intent),
        security_epoch: 1,
        verified_at_ms: at,
        valid_until_ms: intent.valid_until_ms.min(at + 5 * MINUTE),
    })
}

// Deliberately bypasses product account authorization for adapter fault tests only.
#[ic_cdk::update]
fn authorize_membership_intent(request: ClaimRequest) -> Result<MembershipAuthorization> {
    authorize(request.authorization)
}

#[ic_cdk::update]
fn authorize_membership_close(
    _: Hash,
    intent: MembershipIntent,
) -> Result<MembershipAuthorization> {
    authorize(intent)
}

#[ic_cdk::update]
fn barrier() {}

#[ic_cdk::update]
async fn apply_membership_decision(d: MembershipDecision) -> Result<MembershipDecisionReceipt> {
    let at = nanos_to_millis(ic_cdk::api::time());
    let (receipt, lose) = ADAPTER.with_borrow_mut(|a| {
        a.applies += 1;
        if let Some(r) = a.receipts.get(&d.decision_id) {
            return (r.clone(), false);
        }
        let applied = if d.kind == DecisionKind::Close {
            !a.reject_close
        } else {
            at < d.apply_by_ms && at < d.qualification_until_ms
        };
        let receipt = MembershipDecisionReceipt {
            decision_id: d.decision_id,
            decision_digest: decision_digest(&d),
            outcome: if applied {
                DecisionOutcome::Applied
            } else {
                DecisionOutcome::Rejected
            },
            contract_id: applied.then_some(d.claim_id),
            starts_at_ms: d.starts_at_ms,
            expires_at_ms: d.expires_at_ms,
            business_revision: a.receipts.len() as u64 + 1,
            commitment_until_ms: if !applied {
                0
            } else if d.kind == DecisionKind::Close {
                at
            } else {
                d.expires_at_ms
            },
        };
        a.receipts.insert(d.decision_id, receipt.clone());
        (receipt, std::mem::take(&mut a.lose_reply))
    });
    if lose {
        let _: () = dmsg_runtime::call(ic_cdk::api::canister_self(), "barrier", ())
            .await
            .unwrap();
        ic_cdk::trap("injected lost adapter ACK after commit");
    }
    Ok(receipt)
}

#[ic_cdk::update]
fn get_membership_decision(id: Hash) -> Result<Option<MembershipDecisionReceipt>> {
    ADAPTER.with_borrow_mut(|a| {
        a.reads += 1;
        ensure(
            !a.unavailable,
            Error::Unavailable("injected adapter outage".into()),
        )?;
        Ok(a.receipts.get(&id).cloned())
    })
}

#[ic_cdk::query]
fn adapter_calls() -> (u64, u64) {
    ADAPTER.with_borrow(|a| (a.applies, a.reads))
}

ic_cdk::export_candid!();
