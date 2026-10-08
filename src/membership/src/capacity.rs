//! Host-built capacity images: the canister's own claim records, indexes and
//! certification tree, which `membership_capacity_profile` loads into PocketIC.
use crate::{
    claim::Claim,
    fixture::{self, base::*},
    store::{self, RAW},
};
use candid::Principal;
use dmsg_protocol::{commerce_v2::*, digest, integration::product_decision_hash};
use dmsg_types::{
    integration::*, integration_membership::*, membership::Eligibility, membership::MembershipInit,
    *,
};

/// Issue time of the stored records; the profile starts PocketIC's clock here.
pub const AT: u64 = NOW;
/// The first canister PocketIC creates on a fresh application subnet: the
/// profile installs the test SNS there before the membership canister.
const SNS: &str = "xp3jw-ot777-77777-aaaaa-cai";
const SIZES: [u64; 2] = [100_000, 1_000_000];
/// Active claims ending ten days after `AT`, and cancelled records, that the
/// profile's sweep finds due.
const DUE: u64 = 64;

/// The economic actor of claim `i`, listed on its neuron.
fn actor(i: u64) -> Principal {
    Principal::from_slice(&digest("membership capacity actor", &i)[..20])
}

fn request(sns: Principal, i: u64) -> PandaClaimRequest {
    let mut r = fixture::claim();
    r.terms.offer.operation_id = digest("membership capacity operation", &i);
    r.terms.neuron_id = digest("membership capacity neuron", &i);
    r.terms.actor = actor(i);
    r.terms.sns_governance = sns;
    r
}

/// An applied full waiver: decision, Applied receipt and a one-hour lease from `AT`.
fn active(sns: Principal, i: u64) -> Claim {
    let r = request(sns, i);
    let mut c = Claim::new(panda_claim_id(&r.terms), r, PANDA_COOLING_MS);
    c.product_reserved = true;
    c.observe(
        Eligibility::Eligible,
        AT - PANDA_COOLING_MS,
        AT - PANDA_COOLING_MS,
    )
    .expect("first observation");
    c.observe(Eligibility::Eligible, AT, AT)
        .expect("post-cooling observation");
    c.preparing_apply(AT).expect("decision");
    let d = c.decision.clone().expect("decision");
    c.accept(ProductReceipt {
        version: COMMERCE_VERSION,
        decision_id: d.decision_id,
        decision_hash: product_decision_hash(&d),
        adapter: d.offer.adapter,
        outcome: ProductOutcome::Applied {
            business_revision: 1,
            contract_id: digest("membership capacity contract", &i),
            committed_until_ms: d.offer.expires_at_ms,
        },
        applied_at_ms: AT,
    })
    .expect("receipt");
    c.view.committed_until_ms = if i < DUE {
        AT + 10 * DAY
    } else {
        AT + 300 * DAY
    };
    c
}

/// Writes `membership-<claims>.bin` images of growing size to DMSG_MEMBERSHIP_IMAGE_DIR.
#[test]
#[ignore = "host-built stable images for membership_capacity_profile"]
fn capacity_image() {
    let dir =
        std::path::PathBuf::from(std::env::var_os("DMSG_MEMBERSHIP_IMAGE_DIR").expect("image dir"));
    let sns = Principal::from_text(SNS).unwrap();
    store::save_config(&store::Config {
        schema: store::STABLE_SCHEMA,
        init: MembershipInit {
            environment: Environment::Local,
            governance: sns,
            sns_root: sns,
            panda_ledger: sns,
        },
        service: Some(PandaServiceConfig {
            commerce_homes: vec![CommerceHome {
                user_home: principal(5),
                commerce_canister: principal(6),
            }],
            max_claims: store::MAX_FULL_CLAIMS,
            hourly_applications: 10_000,
            cooling_ms: PANDA_COOLING_MS,
            qualifications_per_minute: 200,
            authorizations_per_minute: 200,
            product_calls_per_minute: 200,
        }),
        sns_verified: false,
        sns_verified_at_ms: 0,
        paused: false,
        application_hour: 0,
        applications: 0,
    });
    for i in 0..DUE {
        let r = request(sns, u64::MAX - i);
        let mut c = Claim::new(panda_claim_id(&r.terms), r, PANDA_COOLING_MS);
        c.cancel().expect("cancel");
        c.reservation_released = true;
        store::save(&mut c, AT);
    }
    let mut claims = 0;
    for target in SIZES {
        let started = std::time::Instant::now();
        while claims < target {
            store::save(&mut active(sns, claims), AT);
            claims += 1;
        }
        let path = dir.join(format!("membership-{claims}.bin"));
        let bytes = RAW.with(|m| {
            let image = m.borrow();
            std::fs::write(&path, &*image).expect("image");
            image.len()
        });
        println!(
            "membership_image claims={claims} stable_bytes={bytes} host_build_s={}",
            started.elapsed().as_secs()
        );
    }
}
