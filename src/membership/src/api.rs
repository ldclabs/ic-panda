//! Governance pins and SNS interface verification. No legacy claim/close/change exports.
use crate::{sns, store};
use candid::{CandidType, Principal};
use dmsg_types::{membership::MembershipInit, *};
use serde::{Deserialize, Serialize};
#[ic_cdk::init]
fn init(args: MembershipInit) {
    for id in [args.governance, args.sns_root, args.panda_ledger] {
        dmsg_protocol::authenticated(id).expect("service pin");
    }
    if args.environment != Environment::Local {
        dmsg_protocol::nonzero(
            args.expected_governance_module_hash
                .as_ref()
                .expect("reviewed SNS governance module")
                .as_slice(),
        )
        .expect("module pin");
    }
    store::save_config(&store::Config {
        init: args,
        sns_verified: false,
        sns_verified_at_ms: 0,
        paused: false,
    });
    store::persist_limits();
    store::rebuild();
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    store::persist_limits();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    store::rebuild();
}

#[derive(CandidType, Serialize)]
struct SummaryRequest {
    update_canister_list: Option<bool>,
}

#[derive(CandidType, Deserialize)]
struct SummaryStatus {
    module_hash: Option<Vec<u8>>,
}

#[derive(CandidType, Deserialize)]
struct CanisterSummary {
    canister_id: Option<Principal>,
    status: Option<SummaryStatus>,
}

#[derive(CandidType, Deserialize)]
struct Summary {
    governance: Option<CanisterSummary>,
}

/// Projection pinned to the reviewed root.did, including a real module identity check.
pub(crate) async fn verify_sns() -> Result<()> {
    let before = store::config();
    let observed = nanos_to_millis(ic_cdk::api::time());
    let reply: sns::SnsCanisters = dmsg_runtime::call(
        before.init.sns_root,
        "list_sns_canisters",
        (sns::ListRequest {},),
    )
    .await?;
    ensure(
        reply.root == Some(before.init.sns_root)
            && reply.governance == Some(before.init.governance)
            && reply.ledger == Some(before.init.panda_ledger),
        Error::IntegrityFailed,
    )?;
    let decimals: u8 = dmsg_runtime::call(before.init.panda_ledger, "icrc1_decimals", ()).await?;
    ensure(decimals == 8, Error::UnsupportedProtocol)?;
    if let Some(expected) = before.init.expected_governance_module_hash {
        let summary: Summary = dmsg_runtime::call(
            before.init.sns_root,
            "get_sns_canisters_summary",
            (SummaryRequest {
                update_canister_list: Some(false),
            },),
        )
        .await?;
        ensure(
            summary.governance.is_some_and(|g| {
                g.canister_id == Some(before.init.governance)
                    && g.status
                        .is_some_and(|s| s.module_hash.as_deref() == Some(expected.as_slice()))
            }),
            Error::UnsupportedProtocol,
        )?;
    }
    let mut current = store::config();
    ensure(current.init == before.init, Error::PolicyStale)?;
    if observed >= current.sns_verified_at_ms {
        current.sns_verified = true;
        current.sns_verified_at_ms = observed;
        store::save_config(&current);
    }
    Ok(())
}

#[ic_cdk::update]
async fn verify_sns_configuration() -> Result<()> {
    let at = nanos_to_millis(ic_cdk::api::time());
    store::reserve_call(at, store::CallBudget::Refresh)?;
    verify_sns().await
}

pub(crate) async fn fresh_sns(at: u64) -> bool {
    let c = store::config();
    if c.sns_verified && at < c.sns_verified_at_ms.saturating_add(60 * MINUTE) {
        return true;
    }
    let valid = verify_sns().await.is_ok();
    if !valid {
        let mut current = store::config();
        if current.sns_verified_at_ms <= at {
            current.sns_verified = false;
            store::save_config(&current);
        }
    }
    valid
}

#[ic_cdk::update]
fn set_admission_pause(paused: bool) -> Result<()> {
    ensure(
        ic_cdk::api::msg_caller() == store::config().init.governance,
        Error::Forbidden,
    )?;
    let mut c = store::config();
    c.paused = paused;
    store::save_config(&c);
    Ok(())
}

#[ic_cdk::update]
fn set_sns_governance_module_hash(hash: Hash) -> Result<()> {
    ensure(
        ic_cdk::api::msg_caller() == store::config().init.governance,
        Error::Forbidden,
    )?;
    dmsg_protocol::nonzero(hash.as_slice())?;
    let mut c = store::config();
    c.init.expected_governance_module_hash = Some(hash);
    c.sns_verified = false;
    store::save_config(&c);
    Ok(())
}
