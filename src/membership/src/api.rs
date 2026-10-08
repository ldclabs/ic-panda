//! Initialization, governance configuration and SNS interface verification.
use crate::{sns, store};
use dmsg_protocol::authenticated;
use dmsg_runtime::admin::{self, validation, Validation};
use dmsg_types::{integration::*, integration_membership::PandaServiceConfig, membership::*, *};
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
    ic_cdk::println!(
        "membership_upgrade live_claims={} instructions={}",
        store::live_claims(),
        ic_cdk::api::instruction_counter()
    );
}

/// The pinned root lists the pinned governance and ledger, and the ledger has eight decimals.
/// The governance module is not pinned: the SNS upgrades it to NNS-approved versions on its
/// own, and a reply the neuron projection cannot read is unverifiable.
async fn verify_sns(at: u64) -> Result<()> {
    let init = store::config().init;
    let reply: sns::SnsCanisters =
        dmsg_runtime::call(init.sns_root, "list_sns_canisters", (sns::ListRequest {},)).await?;
    ensure(
        reply.root == Some(init.sns_root)
            && reply.governance == Some(init.governance)
            && reply.ledger == Some(init.panda_ledger),
        Error::IntegrityFailed,
    )?;
    let decimals: u8 = dmsg_runtime::call(init.panda_ledger, "icrc1_decimals", ()).await?;
    ensure(decimals == 8, Error::UnsupportedProtocol)?;
    let mut current = store::config();
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

fn check_service(c: &store::Config, next: &PandaServiceConfig) -> Result<()> {
    let homes = &next.commerce_homes;
    ensure_valid(
        !homes.is_empty()
            && homes.len() <= dmsg_protocol::agent::MAX_USER_HOMES
            && homes
                .iter()
                .enumerate()
                .all(|(i, h)| homes[..i].iter().all(|o| o.user_home != h.user_home)),
        "commerce homes",
    )?;
    for h in homes {
        authenticated(h.user_home)?;
        authenticated(h.commerce_canister)?;
    }
    ensure_valid(
        next.max_claims > 0
            && next.max_claims <= store::MAX_FULL_CLAIMS
            && next.hourly_applications > 0
            && next.hourly_applications <= 10_000
            && next.cooling_ms >= PANDA_COOLING_MS
            && next.cooling_ms < APPLICATION_TTL_MS
            && [
                next.qualifications_per_minute,
                next.authorizations_per_minute,
                next.product_calls_per_minute,
            ]
            .iter()
            .all(|v| (1..=100_000).contains(v)),
        "PANDA limits",
    )?;
    if let Some(old) = &c.service {
        // Claims keep the commerce of their home; homes are only appended.
        ensure(
            homes.starts_with(&old.commerce_homes) && next.max_claims >= store::live_claims(),
            Error::IntegrityFailed,
        )?;
    }
    Ok(())
}

/// Set the PANDA service limits and the commerce canister of each user home.
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
        let homes: Vec<String> = next
            .commerce_homes
            .iter()
            .map(|h| format!("{} -> {}", h.user_home, h.commerce_canister))
            .collect();
        format!(
            "Configure the PANDA service for user homes [{}]: at most {} claims, {} applications per hour, {} ms cooling; per minute {} SNS reads, {} authorizations and as many activations, {} product calls.{}",
            homes.join(", "),
            next.max_claims,
            next.hourly_applications,
            next.cooling_ms,
            next.qualifications_per_minute,
            next.authorizations_per_minute,
            next.product_calls_per_minute,
            admin::unchanged(c.service.as_ref() != Some(&next), "Already set"),
        )
    }))
}
