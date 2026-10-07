//! Host-built capacity images: the canister's own stable records and certification
//! tree, which `commerce_capacity_profile` loads into PocketIC.
use crate::{
    checkout_model::{fixture, Order},
    checkout_store,
    model::{self, Subject},
    store::{self, RAW},
};
use candid::Principal;
use dmsg_protocol::{billing::*, digest};
use dmsg_types::{billing::*, integration_billing::CheckoutStatus, membership::*, *};

/// Issue time of the stored views; the profile starts PocketIC's clock here.
pub const AT: u64 = fixture::base::NOW;
const SIZES: [(u64, u64); 2] = [(100_000, 10_000), (1_000_000, 100_000)];

/// The account of subject `i`, matching the profile's beneficiaries.
fn account(i: u64) -> AccountId {
    AccountId(
        digest("capacity subject", &i)[..12]
            .try_into()
            .expect("account"),
    )
}

/// A Plus subscriber whose cash term started 30 days before `AT`.
fn subscriber(home: Principal, i: u64, catalog: &Catalog) -> Subject {
    let started = AT - 30 * DAY;
    let expires = next_year(started).expect("term");
    let mut s = Subject::new(beneficiary(fixture::base::principal(92), &account(i)));
    s.created_at_ms = Some(AT - 60 * DAY);
    s.business_revision = 1;
    s.contracts.push(MembershipContract {
        term_starts_at_ms: started,
        resource_pauses: vec![],
        contract_id: digest("capacity contract", &i),
        plan: model::plan(catalog, &PlanId::Plus).expect("plan"),
        source: ContractSource::Cash {
            order_id: digest("capacity order", &i),
        },
        starts_at_ms: started,
        expires_at_ms: expires,
        terminated_at_ms: None,
        eligibility: Eligibility::Eligible,
        observed_at_ms: started,
        qualified_until_ms: expires,
        repair_deadline_ms: None,
    });
    model::project(home, &mut s, catalog, AT, None).expect("projection");
    s
}

fn order(i: u64) -> Order {
    let mut input = fixture::open();
    input.quote.offer.operation_id = digest("capacity operation", &i);
    input.authorization.offer = input.quote.offer.clone();
    let mut o = Order::new(fixture::base::principal(8), input);
    o.status = CheckoutStatus::Rejected;
    o.reservation_released = true;
    o
}

/// Writes `commerce-<subjects>.bin` images of growing size to DMSG_COMMERCE_IMAGE_DIR.
/// Stored views name a placeholder commerce home; renewed views name the canister.
#[test]
#[ignore = "host-built stable images for commerce_capacity_profile"]
fn capacity_image() {
    let dir =
        std::path::PathBuf::from(std::env::var_os("DMSG_COMMERCE_IMAGE_DIR").expect("image dir"));
    let home = fixture::base::principal(8);
    let init = CommerceInit {
        limits: CommerceLimits {
            max_subjects: store::MAX_SUBJECTS,
            max_hot_orders: store::MAX_ORDERS,
            max_orders: store::MAX_ORDERS,
            daily_orders: store::MAX_DAILY_ORDERS,
            ..fixture::limits()
        },
        ..fixture::init()
    };
    let catalog = init.catalog.clone();
    store::set_config(store::Config::new(init));
    store::persist_config();
    store::save_catalog(&catalog);
    store::publish_certification(AT);
    let (mut subjects, mut orders) = (0, 0);
    for (target_subjects, target_orders) in SIZES {
        let started = std::time::Instant::now();
        while subjects < target_subjects {
            store::save(&subscriber(home, subjects, &catalog));
            subjects += 1;
        }
        while orders < target_orders {
            checkout_store::save(&mut order(orders), AT);
            orders += 1;
        }
        let bytes = RAW.with(|m| m.borrow().clone());
        std::fs::write(dir.join(format!("commerce-{subjects}.bin")), &bytes).expect("image");
        println!(
            "commerce_image subjects={subjects} orders={orders} stable_bytes={} host_build_s={}",
            bytes.len(),
            started.elapsed().as_secs()
        );
    }
}
