//! Pure v2 money/contract rules. Ledger, product and SNS replies must be authenticated separately.
use crate::{digest, integration::*, membership::mul_div, *};
use candid::Principal;
use dmsg_types::{integration::*, integration_billing::*, membership::Eligibility, *};
use icrc_ledger_types::icrc1::account::Account;

/// Maximum accepted age/window of a governance-authenticated price observation.
pub const PRICE_WINDOW_MS: u64 = 30 * MINUTE;
/// Maximum deviation from the stable-asset unit USD price for new quotes.
pub const MAX_DEPEG_USD_MICROS: u128 = 10_000;
/// Known ineligibility accumulates seven days before rights terminate permanently.
pub const REPAIR_WINDOW_MS: u64 = 7 * DAY;

/// Validate immutable asset policy; verification of ledger decimals/fees is a service step.
pub fn validate_asset(asset: &SettlementAsset) -> Result<()> {
    ensure(
        asset.version == 2 && asset.policy_version > 0,
        Error::UnsupportedProtocol,
    )?;
    authenticated(asset.ledger)?;
    ensure_valid(
        asset.decimals == 6
            && asset.price_usd_micros > 0
            && asset.network_fee_atomic > 0
            && asset.network_fee_atomic <= asset.max_network_fee_atomic
            && asset.max_network_fee_atomic <= 10_000_000
            && asset.price_valid_until_ms > asset.price_observed_at_ms
            && asset.price_valid_until_ms - asset.price_observed_at_ms <= PRICE_WINDOW_MS,
        "asset policy",
    )?;
    if asset.environment != Environment::Local {
        let expected = match asset.asset {
            SettlementAssetKind::CkUsdt => CKUSDT_LEDGER,
            SettlementAssetKind::CkUsdc => CKUSDC_LEDGER,
        };
        ensure(asset.ledger.to_text() == expected, Error::Forbidden)?;
    }
    Ok(())
}

/// Quote admission uses fresh explicit reference pricing; no implicit dollar peg.
pub fn asset_available(asset: &SettlementAsset, at: u64) -> Result<()> {
    validate_asset(asset)?;
    ensure(
        asset.enabled
            && asset.price_observed_at_ms <= at
            && at < asset.price_valid_until_ms
            && asset.price_usd_micros.abs_diff(1_000_000) <= MAX_DEPEG_USD_MICROS,
        Error::PolicyStale,
    )
}

/// An accepted quote keeps its own live price observation across later price publications.
/// Both observations must be available and every non-price term must be unchanged.
pub fn check_quoted_asset(
    current: &SettlementAsset,
    quoted: &SettlementAsset,
    at: u64,
) -> Result<()> {
    asset_available(current, at)?;
    asset_available(quoted, at)?;
    ensure(
        SettlementAsset {
            policy_version: quoted.policy_version,
            price_usd_micros: quoted.price_usd_micros,
            price_observed_at_ms: quoted.price_observed_at_ms,
            price_valid_until_ms: quoted.price_valid_until_ms,
            ..current.clone()
        } == *quoted,
        Error::PolicyStale,
    )
}

/// Convert the full bill with one final upward rounding and arbitrary-width intermediates.
/// `asset` must already pass [`validate_asset`].
pub fn cash_amount(amount_usd_micros: u128, asset: &SettlementAsset) -> Result<u128> {
    ensure_valid(amount_usd_micros > 0, "positive amount")?;
    let unit = 10u128
        .checked_pow(u32::from(asset.decimals))
        .ok_or(Error::QuotaExceeded)?;
    mul_div(amount_usd_micros, unit, asset.price_usd_micros, true)
}

/// Per-operation receiving account; a different ledger cannot become a second order.
pub fn checkout_id(home: Principal, offer: &BillingOffer) -> Hash {
    digest(
        "dmsg/checkout/order/v2",
        &(home, &offer.app_id, offer.operation_id),
    )
}

/// Fully reconstruct cash terms before opening; callers authenticate the offer first.
pub fn checkout_quote(
    home: Principal,
    offer: BillingOffer,
    app: &AppRegistration,
    product: &ProductRegistration,
    asset: SettlementAsset,
    payer: Account,
    at: u64,
) -> Result<CheckoutQuote> {
    validate_billing_offer(&offer, app, product, at)?;
    asset_available(&asset, at)?;
    authenticated(payer.owner)?;
    ensure(
        offer
            .allowed_settlement_methods
            .contains(&SettlementMethod::Cash)
            && product.ledgers.contains(&asset.ledger)
            && asset.environment == offer.environment,
        Error::Forbidden,
    )?;
    let id = checkout_id(home, &offer);
    let cash = CashQuote {
        version: 2,
        offer_hash: billing_offer_hash(&offer),
        ledger: asset.ledger,
        amount_atomic: cash_amount(offer.amount_usd_micros, &asset)?,
        conversion_hash: digest("dmsg/asset-policy/v2", &asset),
        payer,
        deposit: Account {
            owner: home,
            subaccount: Some(id.into_array()),
        },
        max_network_fee_atomic: asset.max_network_fee_atomic,
        fee_reserve_atomic: asset
            .max_network_fee_atomic
            .checked_mul(2)
            .ok_or(Error::QuotaExceeded)?,
        funding_deadline_ms: at.saturating_add(CASH_FUNDING_MS).min(offer.expires_at_ms),
        activation_deadline_ms: at
            .saturating_add(CASH_ACTIVATION_MS)
            .min(offer.expires_at_ms),
    };
    validate_cash_quote(&cash, &offer, product, at)?;
    Ok(CheckoutQuote {
        offer,
        cash,
        asset,
        product: product.clone(),
        quoted_at_ms: at,
    })
}

/// Complete accepted cash terms, including the original merchant configuration.
pub fn checkout_quote_hash(quote: &CheckoutQuote) -> Hash {
    digest("dmsg/checkout/quote/v2", quote)
}

/// Domain-separated product-authorization binding, including the exact account approval.
pub fn product_authorization_hash(request: &ProductAuthorizationRequest) -> Hash {
    digest("dmsg/product-authorization/v2", request)
}

/// Match a fresh replicated authority reply. No new contract is implied.
pub fn check_product_authorization(
    request: &ProductAuthorizationRequest,
    value: &ProductAuthorization,
    at: u64,
) -> Result<()> {
    ensure(
        value.request_hash == product_authorization_hash(request)
            && value.verified_at_ms <= at
            && at - value.verified_at_ms <= MINUTE
            && at < value.valid_until_ms
            && value.valid_until_ms <= value.verified_at_ms.saturating_add(MINUTE),
        Error::PolicyStale,
    )?;
    authenticated(value.operator)
}

/// Account approvals are purpose-separated by settlement method.
pub fn approval_purpose(method: &SettlementMethod) -> ApprovalPurpose {
    match method {
        SettlementMethod::Cash => ApprovalPurpose::CashCheckout,
        SettlementMethod::Panda => ApprovalPurpose::PandaSubscription,
    }
}

/// Match an explicit operator approval to a fixed product and service, before role checks.
pub fn check_operator_approval(
    approval: &ProductApproval,
    offer: &BillingOffer,
    method: SettlementMethod,
    at: u64,
) -> Result<()> {
    ensure(
        approval.version == 2
            && approval.offer_hash == billing_offer_hash(offer)
            && approval.method == method
            && approval.approved_at_ms <= at
            && at < approval.expires_at_ms,
        Error::Forbidden,
    )?;
    authenticated(approval.operator)
}

/// Check the two independent approvals against the selected economic service and bill.
pub fn validate_product_request(
    request: &ProductAuthorizationRequest,
    service: Principal,
    method: SettlementMethod,
    at: u64,
) -> Result<()> {
    let a = &request.account_approval;
    ensure(
        a.service == service
            && a.beneficiary == request.offer.beneficiary
            && a.app_id == request.offer.app_id
            && a.environment == request.offer.environment
            && a.operation_id == request.offer.operation_id
            && a.purpose == approval_purpose(&method),
        Error::Forbidden,
    )?;
    if let Some(product) = &request.product_approval {
        check_operator_approval(product, &request.offer, method, at)?;
    }
    authenticated(request.user_home)
}

/// Validate delivery timing; retrieval of a past receipt remains possible after this window.
pub fn validate_decision(decision: &ProductDecision, at: u64) -> Result<()> {
    ensure(
        decision.version == 2 && decision.offer.version == 2,
        Error::UnsupportedProtocol,
    )?;
    ensure(
        decision.decided_at_ms <= at
            && at < decision.apply_by_ms
            && decision.apply_by_ms <= decision.offer.expires_at_ms
            && decision.apply_by_ms.saturating_sub(decision.decided_at_ms) <= APPLICATION_TTL_MS,
        Error::Expired,
    )?;
    nonzero(decision.decision_id.as_slice())?;
    match &decision.source {
        SettlementSource::Cash {
            ledger,
            amount_atomic,
            ..
        } => {
            authenticated(*ledger)?;
            ensure(
                *amount_atomic > 0
                    && decision
                        .offer
                        .allowed_settlement_methods
                        .contains(&SettlementMethod::Cash),
                Error::IntegrityFailed,
            )
        }
        SettlementSource::Panda {
            committed_until_ms,
            lease_until_ms,
            ..
        } => ensure(
            *committed_until_ms == decision.offer.expires_at_ms
                && *lease_until_ms > decision.decided_at_ms
                && *lease_until_ms <= *committed_until_ms
                && decision
                    .offer
                    .allowed_settlement_methods
                    .contains(&SettlementMethod::Panda),
            Error::IntegrityFailed,
        ),
    }
}

/// Construct a full immutable product contract after service/source validation.
pub fn contract(
    decision: &ProductDecision,
    source: SubscriptionSource,
    business_revision: u64,
    at: u64,
) -> Result<SubscriptionContract> {
    validate_decision(decision, at)?;
    let lease = match (&decision.source, &source) {
        (
            SettlementSource::Cash {
                order_id: a,
                ledger: b,
                block_index: c,
                amount_atomic: d,
            },
            SubscriptionSource::Cash {
                order_id,
                ledger,
                block_index,
                amount_atomic,
            },
        ) if (a, b, c, d) == (order_id, ledger, block_index, amount_atomic) => {
            decision.offer.expires_at_ms
        }
        (
            SettlementSource::Panda {
                claim_id,
                quote_hash,
                lease_until_ms,
                ..
            },
            SubscriptionSource::PandaClaim {
                claim_id: id,
                quote,
            },
        ) if claim_id == id
            && *quote_hash == panda_quote_hash(quote)
            && quote.offer_hash == billing_offer_hash(&decision.offer)
            && quote.required_stake_e8s
                == required_panda_stake(
                    decision.offer.amount_usd_micros,
                    quote.policy.r_num,
                    quote.policy.r_den,
                )?
            && quote.committed_until_ms == decision.offer.expires_at_ms =>
        {
            *lease_until_ms
        }
        _ => return Err(Error::IntegrityFailed),
    };
    Ok(SubscriptionContract {
        contract_id: digest(
            "dmsg/subscription/contract/v2",
            &(decision.offer.adapter, decision.decision_id),
        ),
        offer: decision.offer.clone(),
        source,
        decision_id: Some(decision.decision_id),
        applied_at_ms: at,
        business_revision,
        lease_revision: 1,
        status: SubscriptionStatus::Active,
        lease_until_ms: lease,
        max_issued_until_ms: 0,
        qualification: Eligibility::Eligible,
        observed_at_ms: at,
        repair_elapsed_ms: 0,
    })
}

/// Issue only rights inside a live contract and source lease. Compliance/base access is separate.
pub fn entitlement_until(contract: &mut SubscriptionContract, at: u64) -> Option<u64> {
    if contract.status != SubscriptionStatus::Active
        || at < contract.offer.starts_at_ms
        || at >= contract.offer.expires_at_ms
    {
        return None;
    }
    let until = if matches!(contract.source, SubscriptionSource::PandaClaim { .. }) {
        contract.lease_until_ms.min(contract.offer.expires_at_ms)
    } else {
        contract.offer.expires_at_ms
    };
    if at >= until {
        return None;
    }
    contract.max_issued_until_ms = contract.max_issued_until_ms.max(until);
    Some(until)
}

/// Apply a trusted observation. A terminated contract never revives and no branch reduces E.
pub fn observe_contract(
    contract: &mut SubscriptionContract,
    eligibility: Eligibility,
    observed_at_ms: u64,
    lease_until_ms: u64,
    at: u64,
) -> Result<()> {
    ensure(
        matches!(contract.source, SubscriptionSource::PandaClaim { .. }),
        Error::UnsupportedProtocol,
    )?;
    ensure(
        observed_at_ms <= at && observed_at_ms >= contract.observed_at_ms,
        Error::PolicyStale,
    )?;
    if matches!(
        contract.status,
        SubscriptionStatus::Terminated | SubscriptionStatus::Expired
    ) {
        return Ok(());
    }
    if at >= contract.offer.expires_at_ms {
        contract.status = SubscriptionStatus::Expired;
        contract.lease_until_ms = contract.offer.expires_at_ms;
        return Ok(());
    }
    if eligibility == Eligibility::Eligible {
        ensure(
            at < lease_until_ms
                && lease_until_ms <= observed_at_ms.saturating_add(PANDA_LEASE_MS)
                && lease_until_ms <= contract.offer.expires_at_ms,
            Error::MembershipStale,
        )?;
    }
    let revision = contract
        .lease_revision
        .checked_add(1)
        .ok_or(Error::QuotaExceeded)?;
    let previous = contract.qualification.clone();
    if eligibility != Eligibility::Unverifiable && previous == Eligibility::Ineligible {
        contract.repair_elapsed_ms = contract
            .repair_elapsed_ms
            .saturating_add(observed_at_ms - contract.observed_at_ms);
    }
    contract.qualification = eligibility.clone();
    contract.observed_at_ms = observed_at_ms;
    contract.lease_revision = revision;
    if contract.repair_elapsed_ms >= REPAIR_WINDOW_MS {
        contract.status = SubscriptionStatus::Terminated;
        contract.lease_until_ms = at;
        return Ok(());
    }
    match eligibility {
        Eligibility::Eligible => {
            contract.status = SubscriptionStatus::Active;
            contract.lease_until_ms = lease_until_ms;
            contract.repair_elapsed_ms = 0;
        }
        Eligibility::Ineligible => {
            contract.lease_until_ms = at;
            contract.status = SubscriptionStatus::Repairing;
        }
        Eligibility::Unverifiable => {
            contract.lease_until_ms = contract.lease_until_ms.min(at);
            contract.status = SubscriptionStatus::Unverifiable;
        }
    }
    Ok(())
}

/// Earned price begins at actual delivery (or future term start), never at quote creation.
pub fn earned_atomic(
    amount: u128,
    offer: &BillingOffer,
    applied_at_ms: u64,
    at: u64,
) -> Result<u128> {
    let start = offer.starts_at_ms.max(applied_at_ms);
    ensure(start < offer.expires_at_ms, Error::IntegrityFailed)?;
    if at <= start {
        return Ok(0);
    }
    if at >= offer.expires_at_ms {
        return Ok(amount);
    }
    mul_div(
        amount,
        u128::from(at - start),
        u128::from(offer.expires_at_ms - start),
        false,
    )
}

/// Full neuron selection, actor, dMsg account and quote are all in the device-approved digest.
pub fn panda_application_hash(
    terms: &dmsg_types::integration_membership::PandaApplicationTerms,
) -> Hash {
    digest("dmsg/panda/application/v2", terms)
}

/// A reused product operation cannot silently select a different neuron or economic actor.
pub fn panda_claim_id(terms: &dmsg_types::integration_membership::PandaApplicationTerms) -> Hash {
    digest(
        "dmsg/panda/claim/v2",
        &(
            terms.home_membership,
            &terms.offer.app_id,
            terms.offer.operation_id,
        ),
    )
}

/// Independent reconstruction of the accepted amount, threshold and immutable deadline.
pub fn validate_panda_terms(
    terms: &dmsg_types::integration_membership::PandaApplicationTerms,
    app: &AppRegistration,
    product: &ProductRegistration,
    home: Principal,
    governance: Principal,
    at: u64,
    initial: bool,
) -> Result<()> {
    ensure(
        terms.home_membership == home
            && terms.sns_governance == governance
            && app.user_homes.contains(&terms.user_home),
        Error::Forbidden,
    )?;
    authenticated(terms.actor)?;
    nonzero(terms.approving_account.as_slice())?;
    nonzero(terms.neuron_id.as_slice())?;
    let quoted = quote_panda(
        &terms.offer,
        app,
        product,
        &terms.quote.policy,
        terms.quote.quoted_at_ms,
    )?;
    ensure(
        quoted == terms.quote
            && terms.quote.quoted_at_ms <= at
            && at < terms.quote.application_deadline_ms,
        Error::IntegrityFailed,
    )?;
    if initial {
        ensure(at < terms.offer.accept_by_ms, Error::Expired)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "commerce_v2_tests.rs"]
mod tests;
