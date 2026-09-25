//! Initialization, governance configuration and SNS interface verification.
use crate::{claims, sns, store};
use dmsg_protocol::{authenticated, nonzero};
use dmsg_types::{integration::*, integration_membership::PandaServiceConfig, membership::*, *};
use ic_cdk_management_canister::{canister_info, CanisterInfoArgs};

#[ic_cdk::init]
fn init(args: MembershipInit) {
    for id in [args.governance, args.sns_root, args.panda_ledger] {
        authenticated(id).expect("service pin");
    }
    if args.environment != Environment::Local {
        nonzero(
            args.expected_governance_module_hash
                .as_ref()
                .expect("reviewed SNS governance module")
                .as_slice(),
        )
        .expect("module pin");
    }
    store::save_config(&store::Config {
        init: args,
        service: None,
        sns_verified: false,
        sns_verified_at_ms: 0,
        paused: false,
        application_hour: 0,
        applications: 0,
    });
    store::rebuild();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    store::rebuild();
}

/// Root listing, ledger decimals and, when pinned, the governance module and sole root controller.
async fn verify_sns(at: u64) -> Result<()> {
    let before = store::config();
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
    if let Some(expected) = &before.init.expected_governance_module_hash {
        let info = canister_info(&CanisterInfoArgs {
            canister_id: before.init.governance,
            num_requested_changes: None,
        })
        .await
        .map_err(|_| Error::Unavailable("governance canister_info".into()))?;
        ensure(
            info.module_hash.as_deref() == Some(expected.as_slice())
                && info.controllers == [before.init.sns_root],
            Error::UnsupportedProtocol,
        )?;
    }
    let mut current = store::config();
    ensure(current.init == before.init, Error::PolicyStale)?;
    if at >= current.sns_verified_at_ms {
        current.sns_verified = true;
        current.sns_verified_at_ms = at;
        store::save_config(&current);
    }
    Ok(())
}

/// Reuse a fresh verification; a failure clears only a verification not newer than `at`.
pub(crate) async fn fresh_sns(at: u64) -> Result<()> {
    if store::config().sns_fresh(at) {
        return Ok(());
    }
    let result = verify_sns(at).await;
    if result.is_err() {
        let mut current = store::config();
        if current.sns_verified && current.sns_verified_at_ms <= at {
            current.sns_verified = false;
            store::save_config(&current);
        }
    }
    result
}

#[ic_cdk::update]
async fn verify_sns_configuration() -> Result<()> {
    let at = nanos_to_millis(ic_cdk::api::time());
    if !store::config().sns_fresh(at) {
        store::reserve_call(at, store::CallBudget::Refresh)?;
    }
    fresh_sns(at).await
}

#[ic_cdk::update]
fn set_admission_pause(paused: bool) -> Result<()> {
    let mut c = store::governance(ic_cdk::api::msg_caller())?;
    c.paused = paused;
    store::save_config(&c);
    Ok(())
}

#[ic_cdk::update]
fn set_sns_governance_module_hash(hash: Hash) -> Result<()> {
    let mut c = store::governance(ic_cdk::api::msg_caller())?;
    nonzero(hash.as_slice())?;
    c.init.expected_governance_module_hash = Some(hash);
    c.sns_verified = false;
    store::save_config(&c);
    Ok(())
}

#[ic_cdk::update]
fn configure_panda_service(next: PandaServiceConfig) -> Result<()> {
    let mut c = store::governance(ic_cdk::api::msg_caller())?;
    authenticated(next.commerce_canister)?;
    ensure_valid(
        next.max_claims > 0
            && next.hourly_applications > 0
            && next.hourly_applications <= 10_000
            && next.cooling_ms >= PANDA_COOLING_MS
            && next.cooling_ms < APPLICATION_TTL_MS,
        "PANDA limits",
    )?;
    if let Some(old) = &c.service {
        ensure(
            old.commerce_canister == next.commerce_canister
                && next.max_claims >= claims::live_claims(),
            Error::IntegrityFailed,
        )?;
    }
    c.service = Some(next);
    store::save_config(&c);
    Ok(())
}
