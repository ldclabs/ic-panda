//! Initialization, governance configuration and SNS interface verification.
use crate::{sns, store};
use dmsg_protocol::{authenticated, nonzero};
use dmsg_runtime::admin::{self, hex, validation, Validation};
use dmsg_types::{integration::*, integration_membership::PandaServiceConfig, membership::*, *};
use ic_cdk_management_canister::{canister_info, CanisterInfoArgs};
use std::cell::Cell;

thread_local! {
    static VERIFYING: Cell<bool> = const { Cell::new(false) };
}

struct Verification;

impl Drop for Verification {
    fn drop(&mut self) {
        VERIFYING.set(false);
    }
}

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
        schema: store::STABLE_SCHEMA,
        init: args,
        service: None,
        sns_verified: false,
        sns_verified_at_ms: 0,
        paused: false,
        application_hour: 0,
        applications: 0,
    });
    store::publish();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    assert_eq!(
        store::config().schema,
        store::STABLE_SCHEMA,
        "explicit stable-state migration required"
    );
    store::publish();
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
    ensure(!VERIFYING.get(), Error::Pending)?;
    store::reserve_call(at, store::CallBudget::Refresh)?;
    VERIFYING.set(true);
    let _guard = Verification;
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
    fresh_sns(at).await
}

/// Pause or resume new PANDA applications; existing claims keep refreshing.
#[ic_cdk::update]
fn set_admission_pause(paused: bool) -> Result<()> {
    let mut c = store::admin(ic_cdk::api::msg_caller())?;
    c.paused = paused;
    store::save_config(&c);
    Ok(())
}

#[ic_cdk::query]
fn validate_set_admission_pause(paused: bool) -> Validation {
    let action = if paused { "Pause" } else { "Resume" };
    Ok(format!(
        "{action} new PANDA applications.{}",
        admin::unchanged(store::config().paused != paused, "Already set"),
    ))
}

/// Pin a reviewed SNS governance module; qualification is re-verified before use.
#[ic_cdk::update]
fn set_sns_governance_module_hash(hash: Hash) -> Result<()> {
    let mut c = store::admin(ic_cdk::api::msg_caller())?;
    nonzero(hash.as_slice())?;
    c.init.expected_governance_module_hash = Some(hash);
    c.sns_verified = false;
    store::save_config(&c);
    Ok(())
}

#[ic_cdk::query]
fn validate_set_sns_governance_module_hash(hash: Hash) -> Validation {
    let c = store::config();
    validation(nonzero(hash.as_slice()).map(|()| {
        format!(
            "Pin SNS governance {} to module hash {} (currently {}) and re-verify the SNS before new applications.",
            c.init.governance,
            hex(hash.as_slice()),
            c.init
                .expected_governance_module_hash
                .map_or("unpinned".into(), |h| hex(h.as_slice())),
        )
    }))
}

fn check_service(c: &store::Config, next: &PandaServiceConfig) -> Result<()> {
    authenticated(next.commerce_canister)?;
    ensure_valid(
        next.max_claims > 0
            && next.max_claims <= store::MAX_FULL_CLAIMS
            && next.hourly_applications > 0
            && next.hourly_applications <= 10_000
            && next.cooling_ms >= PANDA_COOLING_MS
            && next.cooling_ms < APPLICATION_TTL_MS,
        "PANDA limits",
    )?;
    if let Some(old) = &c.service {
        ensure(
            old.commerce_canister == next.commerce_canister
                && next.max_claims >= store::live_claims(),
            Error::IntegrityFailed,
        )?;
    }
    Ok(())
}

/// Set the PANDA service limits. The commerce canister is fixed once set.
#[ic_cdk::update]
fn configure_panda_service(next: PandaServiceConfig) -> Result<()> {
    let mut c = store::admin(ic_cdk::api::msg_caller())?;
    check_service(&c, &next)?;
    c.service = Some(next);
    store::save_config(&c);
    Ok(())
}

#[ic_cdk::query]
fn validate_configure_panda_service(next: PandaServiceConfig) -> Validation {
    let c = store::config();
    validation(check_service(&c, &next).map(|()| {
        format!(
            "Configure the PANDA service for commerce {}: at most {} claims, {} applications per hour, {} ms cooling.{}",
            next.commerce_canister,
            next.max_claims,
            next.hourly_applications,
            next.cooling_ms,
            admin::unchanged(c.service.as_ref() != Some(&next), "Already set"),
        )
    }))
}
