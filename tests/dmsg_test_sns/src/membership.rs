//! Fault injection for the registered product boundary; never a production authority.
use candid::Principal;
use dmsg_protocol::commerce_v2::*;
use dmsg_types::{integration::*, integration_billing::*, integration_membership::*, *};
use std::cell::RefCell;

#[derive(Clone)]
struct Fixture {
    membership: Principal,
    offer: BillingOffer,
    reserve_error: Option<Error>,
    release_error: Option<Error>,
    callback: Option<(Hash, bool)>,
    reserve_calls: u64,
    release_calls: u64,
    reentry_pending: bool,
}

thread_local! {
    static PRODUCT: RefCell<Option<Fixture>> = const { RefCell::new(None) };
}

#[ic_cdk::update]
fn configure_membership_fixture(membership: Principal, offer: BillingOffer) {
    PRODUCT.with_borrow_mut(|f| {
        *f = Some(Fixture {
            membership,
            offer,
            reserve_error: None,
            release_error: None,
            callback: None,
            reserve_calls: 0,
            release_calls: 0,
            reentry_pending: false,
        })
    });
}

#[ic_cdk::update]
fn membership_fixture_failure(reserve: Option<Error>, release: Option<Error>) {
    PRODUCT.with_borrow_mut(|f| {
        let f = f.as_mut().expect("configured fixture");
        f.reserve_error = reserve;
        f.release_error = release;
    });
}

#[ic_cdk::update]
fn membership_fixture_callback(claim: Hash, cancel: bool) {
    PRODUCT.with_borrow_mut(|f| {
        f.as_mut().expect("configured fixture").callback = Some((claim, cancel))
    });
}

#[ic_cdk::query]
fn membership_fixture_counts() -> (u64, u64, bool, u64, u64) {
    let (verification, neurons) = super::CALL_COUNTS.with_borrow(|c| *c);
    PRODUCT.with_borrow(|f| {
        let (reserve, release, pending) = f.as_ref().map_or((0, 0, false), |f| {
            (f.reserve_calls, f.release_calls, f.reentry_pending)
        });
        (reserve, release, pending, verification, neurons)
    })
}

fn fixture(offer: &BillingOffer) -> Result<Fixture> {
    let f = PRODUCT.with_borrow(Clone::clone).ok_or(Error::NotFound)?;
    ensure(
        ic_cdk::api::msg_caller() == f.membership && *offer == f.offer,
        Error::Forbidden,
    )?;
    Ok(f)
}

#[ic_cdk::update]
fn verify_billing_offer(offer: BillingOffer) -> Result<()> {
    fixture(&offer).map(|_| ())
}

#[ic_cdk::update]
fn authorize_product_billing(request: ProductAuthorizationRequest) -> Result<ProductAuthorization> {
    let f = fixture(&request.offer)?;
    let at = nanos_to_millis(ic_cdk::api::time());
    validate_product_request(&request, f.membership, SettlementMethod::Panda, at)?;
    Ok(ProductAuthorization {
        request_hash: product_authorization_hash(&request),
        operator: request.account_approval.actor,
        verified_at_ms: at,
        valid_until_ms: at + MINUTE,
    })
}

#[ic_cdk::update]
async fn reserve_product_billing(request: ProductAuthorizationRequest, _: u64) -> Result<()> {
    let f = fixture(&request.offer)?;
    PRODUCT.with_borrow_mut(|v| {
        let v = v.as_mut().expect("configured fixture");
        v.reserve_calls += 1;
        v.callback = None;
    });
    if let Some((claim, cancel)) = f.callback {
        let reply: Result<PandaClaimView> = dmsg_runtime::call(
            f.membership,
            if cancel {
                "cancel_panda_application"
            } else {
                "reconcile_panda_claim"
            },
            (claim,),
        )
        .await?;
        PRODUCT.with_borrow_mut(|v| {
            v.as_mut().expect("configured fixture").reentry_pending = reply == Err(Error::Pending)
        });
    }
    f.reserve_error.map_or(Ok(()), Err)
}

#[ic_cdk::update]
fn release_product_billing(request: ProductAuthorizationRequest) -> Result<()> {
    let f = fixture(&request.offer)?;
    PRODUCT.with_borrow_mut(|v| v.as_mut().expect("configured fixture").release_calls += 1);
    f.release_error.map_or(Ok(()), Err)
}
