//! External integration validation and exact commitments. No network or storage.
use crate::{authenticated, canonical, digest, membership::mul_div, nonzero, validate_origin};
use candid::Principal;
use dmsg_types::{integration::*, membership::Beneficiary, *};
use serde::Serialize;

/// A bounded identifier with one ASCII spelling (no case folding).
pub fn validate_identifier(value: &str) -> Result<()> {
    ensure_valid(
        !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "identifier",
    )
}

fn unique<T: PartialEq>(items: &[T], max: usize, nonempty: bool) -> Result<()> {
    ensure_valid(
        items.len() <= max && (!nonempty || !items.is_empty()),
        "list size",
    )?;
    ensure_valid(
        items
            .iter()
            .enumerate()
            .all(|(i, v)| !items[..i].contains(v)),
        "duplicate",
    )
}

/// Validate an application configuration, without establishing its governance origin.
/// Run it when a registration is accepted; request checks rely on accepted registrations.
pub fn validate_app(app: &AppRegistration) -> Result<()> {
    ensure(
        app.version == INTEGRATION_VERSION,
        Error::UnsupportedProtocol,
    )?;
    validate_identifier(&app.app_id)?;
    ensure_valid(app.config_version > 0, "app version")?;
    unique(&app.origins, MAX_APP_BINDINGS, true)?;
    unique(&app.user_homes, MAX_APP_BINDINGS, true)?;
    unique(&app.cose_homes, MAX_APP_BINDINGS, true)?;
    unique(&app.product_ids, MAX_APP_BINDINGS, false)?;
    unique(&app.capabilities, 4, true)?;
    unique(&app.profiles, 4, false)?;
    ensure_valid(
        app.capabilities.contains(&AppCapability::SignAction)
            == app.profiles.contains(&SigningProfile::AppActionV1)
            && app.capabilities.contains(&AppCapability::SignDocument)
                == app
                    .profiles
                    .iter()
                    .any(|p| *p != SigningProfile::AppActionV1),
        "profile capabilities",
    )?;
    for origin in &app.origins {
        validate_origin(origin, &app.environment)?;
    }
    for home in app.user_homes.iter().chain(&app.cose_homes) {
        authenticated(*home)?;
    }
    for product in &app.product_ids {
        validate_identifier(product)?;
    }
    authenticated(app.action_authority)?;
    authenticated(app.authentication_receiver)
}

/// Validate a product configuration. Local/staging ledgers must be explicitly supplied.
/// Run it when a registration is accepted; request checks rely on accepted registrations.
pub fn validate_product(product: &ProductRegistration) -> Result<()> {
    ensure(
        product.version == COMMERCE_VERSION,
        Error::UnsupportedProtocol,
    )?;
    validate_identifier(&product.product_id)?;
    validate_identifier(&product.subject_schema)?;
    ensure_valid(
        product.config_version > 0 && (1..=64).contains(&product.subject_size),
        "product size/version",
    )?;
    unique(&product.ledgers, 2, true)?;
    for principal in [
        product.quote_authority,
        product.beneficiary_authority,
        product.adapter,
        product.merchant.owner,
    ]
    .iter()
    .chain(&product.ledgers)
    {
        authenticated(*principal)?;
    }
    if product.environment == Environment::Production {
        for ledger in &product.ledgers {
            ensure_valid(
                [CKUSDT_LEDGER, CKUSDC_LEDGER].contains(&ledger.to_text().as_str()),
                "unsupported ledger",
            )?;
        }
    }
    nonzero(&product.terms_hash[..])?;
    nonzero(&product.subsidy_budget_id[..])
}

/// Check subject identity against a trusted product registration, without granting roles.
pub fn validate_subject(subject: &Beneficiary, product: &ProductRegistration) -> Result<()> {
    ensure_valid(
        subject.product_id == product.product_id
            && subject.authority_canister == product.beneficiary_authority
            && subject.subject_schema == product.subject_schema
            && subject.subject_bytes.len() == usize::from(product.subject_size),
        "subject binding",
    )
}

/// Validate request shape, exact app binding and time; does not prove device approval.
/// `app` must be a registration already accepted by [`validate_app`].
pub fn validate_authentication(
    request: &AuthenticationRequest,
    app: &AppRegistration,
    now: u64,
) -> Result<()> {
    ensure(
        request.version == INTEGRATION_VERSION,
        Error::UnsupportedProtocol,
    )?;
    ensure(!app.paused, Error::Locked)?;
    ensure(
        request.environment == app.environment
            && request.app_id == app.app_id
            && request.app_config_version == app.config_version
            && request.receiver == app.authentication_receiver
            && app.origins.contains(&request.origin)
            && app.capabilities.contains(&AppCapability::Authenticate),
        Error::Forbidden,
    )?;
    ensure(
        request.issued_at_ms <= now
            && now < request.expires_at_ms
            && request.expires_at_ms.saturating_sub(request.issued_at_ms) <= AUTH_TTL_MS,
        Error::Expired,
    )?;
    for id in [
        &request.challenge_hash,
        &request.session_key_hash,
        &request.nonce,
        &request.operation_id,
    ] {
        nonzero(&id[..])?;
    }
    Ok(())
}

/// Validate a fresh authoritative offer before acceptance, not during Unknown recovery.
/// `app` and `product` must be registrations already accepted by [`validate_app`] and
/// [`validate_product`].
pub fn validate_billing_offer(
    offer: &BillingOffer,
    app: &AppRegistration,
    product: &ProductRegistration,
    now: u64,
) -> Result<()> {
    ensure(
        offer.version == COMMERCE_VERSION,
        Error::UnsupportedProtocol,
    )?;
    ensure(!app.paused && !product.paused, Error::Locked)?;
    ensure(
        offer.environment == product.environment
            && offer.environment == app.environment
            && offer.app_id == app.app_id
            && offer.product_id == product.product_id
            && app.product_ids.contains(&offer.product_id)
            && app.capabilities.contains(&AppCapability::Checkout)
            && offer.quote_authority == product.quote_authority
            && offer.adapter == product.adapter
            && offer.product_terms_hash == product.terms_hash,
        Error::Forbidden,
    )?;
    validate_subject(&offer.beneficiary, product)?;
    validate_identifier(&offer.sku)?;
    ensure_valid(
        offer.amount_usd_micros > 0 && offer.starts_at_ms < offer.expires_at_ms,
        "paid interval",
    )?;
    ensure(
        offer.issued_at_ms <= now
            && now < offer.accept_by_ms
            && offer.accept_by_ms <= offer.expires_at_ms
            && offer.accept_by_ms.saturating_sub(offer.issued_at_ms) <= OFFER_TTL_MS,
        Error::Expired,
    )?;
    unique(&offer.allowed_settlement_methods, 2, true)?;
    nonzero(&offer.offer_id[..])?;
    nonzero(&offer.operation_id[..])
}

/// Require a published positive rate with at least thirty days' advance notice.
pub fn validate_rate_policy(policy: &PandaRatePolicy) -> Result<()> {
    ensure(
        policy.version == COMMERCE_VERSION,
        Error::UnsupportedProtocol,
    )?;
    ensure_valid(
        policy.policy_version > 0 && policy.r_num > 0 && policy.r_den > 0,
        "PANDA rate",
    )?;
    ensure_valid(
        policy
            .effective_at_ms
            .checked_sub(policy.published_at_ms)
            .is_some_and(|v| v >= POLICY_NOTICE_MS),
        "policy notice",
    )?;
    unique(&policy.product_ids, MAX_APP_BINDINGS, true)?;
    for id in &policy.product_ids {
        validate_identifier(id)?;
    }
    nonzero(&policy.subsidy_budget_id[..])
}

/// Exact ceil(USD micro * PANDA/USD * 10^8 / 10^6); never annualizes the amount.
/// A rate numerator above `u128::MAX / 100` returns QuotaExceeded.
pub fn required_panda_stake(amount_usd_micros: u128, r_num: u128, r_den: u128) -> Result<u128> {
    ensure_valid(
        amount_usd_micros > 0 && r_num > 0 && r_den > 0,
        "amount / rate",
    )?;
    // 10^8 PANDA atomic units per 10^6 USD micro units is exactly 100.
    let r_num = r_num.checked_mul(100).ok_or(Error::QuotaExceeded)?;
    mul_div(amount_usd_micros, r_num, r_den, true)
}

/// Quote after verifying the offer's authoritative source; no claim is created here.
pub fn quote_panda(
    offer: &BillingOffer,
    app: &AppRegistration,
    product: &ProductRegistration,
    policy: &PandaRatePolicy,
    now: u64,
) -> Result<PandaQuote> {
    validate_billing_offer(offer, app, product, now)?;
    validate_rate_policy(policy)?;
    ensure(
        offer
            .allowed_settlement_methods
            .contains(&SettlementMethod::Panda)
            && policy.environment == offer.environment
            && policy.product_ids.contains(&offer.product_id)
            && policy.subsidy_budget_id == product.subsidy_budget_id,
        Error::Forbidden,
    )?;
    ensure(policy.effective_at_ms <= now, Error::PolicyStale)?;
    let deadline = now
        .saturating_add(APPLICATION_TTL_MS)
        .min(offer.expires_at_ms);
    ensure(
        deadline.saturating_sub(now) > PANDA_COOLING_MS,
        Error::Expired,
    )?;
    Ok(PandaQuote {
        quoted_at_ms: now,
        version: COMMERCE_VERSION,
        offer_hash: billing_offer_hash(offer),
        policy: policy.clone(),
        required_stake_e8s: required_panda_stake(
            offer.amount_usd_micros,
            policy.r_num,
            policy.r_den,
        )?,
        subsidy_usd_micros: offer.amount_usd_micros,
        application_deadline_ms: deadline,
        committed_until_ms: offer.expires_at_ms,
    })
}

/// Validate immutable cash terms against the accepted product policy and quote time.
pub fn validate_cash_quote(
    quote: &CashQuote,
    offer: &BillingOffer,
    product: &ProductRegistration,
    accepted_at_ms: u64,
) -> Result<()> {
    ensure(
        quote.version == COMMERCE_VERSION,
        Error::UnsupportedProtocol,
    )?;
    ensure(
        quote.offer_hash == billing_offer_hash(offer)
            && offer
                .allowed_settlement_methods
                .contains(&SettlementMethod::Cash)
            && product.ledgers.contains(&quote.ledger),
        Error::Forbidden,
    )?;
    authenticated(quote.payer.owner)?;
    authenticated(quote.deposit.owner)?;
    nonzero(&quote.conversion_hash[..])?;
    ensure_valid(
        quote.amount_atomic > 0
            && quote.fee_reserve_atomic >= quote.max_network_fee_atomic
            && quote
                .amount_atomic
                .checked_add(quote.fee_reserve_atomic)
                .is_some(),
        "cash amount / fee",
    )?;
    ensure_valid(
        quote.funding_deadline_ms > accepted_at_ms
            && quote.funding_deadline_ms.saturating_sub(accepted_at_ms) <= CASH_FUNDING_MS
            && quote.activation_deadline_ms >= quote.funding_deadline_ms
            && quote.activation_deadline_ms.saturating_sub(accepted_at_ms) <= CASH_ACTIVATION_MS
            && quote.activation_deadline_ms <= offer.expires_at_ms,
        "cash deadline",
    )
}

/// Application approval shape and binding. The caller must additionally check the device and product permission.
/// `app` and `product` must be registrations already accepted by [`validate_app`] and
/// [`validate_product`].
pub fn validate_application_approval(
    approval: &ApplicationApproval,
    app: &AppRegistration,
    product: &ProductRegistration,
    expected_service: Principal,
    now: u64,
) -> Result<()> {
    ensure(
        approval.version == INTEGRATION_VERSION,
        Error::UnsupportedProtocol,
    )?;
    ensure(!app.paused && !product.paused, Error::Locked)?;
    ensure(
        approval.app_id == app.app_id
            && approval.app_config_version == app.config_version
            && approval.environment == app.environment
            && approval.environment == product.environment
            && app.origins.contains(&approval.origin)
            && app.product_ids.contains(&product.product_id)
            && approval.service == expected_service,
        Error::Forbidden,
    )?;
    ensure(
        app.capabilities.contains(&AppCapability::Checkout),
        Error::Forbidden,
    )?;
    authenticated(approval.actor)?;
    authenticated(approval.service)?;
    validate_subject(&approval.beneficiary, product)?;
    crate::expiry(now, approval.expires_at_ms, AUTH_TTL_MS)?;
    for id in [
        &approval.action_digest,
        &approval.operation_id,
        &approval.nonce,
    ] {
        nonzero(&id[..])?;
    }
    nonzero(approval.approving_account.as_slice())
}

/// Required binding checks after separately verifying the IC certificate and exact witness leaf.
pub fn match_authentication_result(
    result: &AuthenticationResult,
    expected: &AuthenticationRequest,
    trusted_home: Principal,
    certificate_time_ms: u64,
    now: u64,
) -> Result<()> {
    ensure(
        result.version == INTEGRATION_VERSION && expected.version == INTEGRATION_VERSION,
        Error::UnsupportedProtocol,
    )?;
    ensure(
        &result.request == expected && result.home_user == trusted_home,
        Error::IntegrityFailed,
    )?;
    ensure(
        expected.issued_at_ms <= result.approved_at_ms
            && result.approved_at_ms <= certificate_time_ms
            && certificate_time_ms <= now
            && now < result.expires_at_ms
            && result.expires_at_ms <= expected.expires_at_ms
            && expected.expires_at_ms.saturating_sub(expected.issued_at_ms) <= AUTH_TTL_MS
            && now.saturating_sub(certificate_time_ms) <= MINUTE,
        Error::Expired,
    )
}

/// Bind a terminal adapter receipt to the original decision; separately authenticate the adapter.
pub fn match_product_receipt(receipt: &ProductReceipt, decision: &ProductDecision) -> Result<()> {
    ensure(
        receipt.version == COMMERCE_VERSION && decision.version == COMMERCE_VERSION,
        Error::UnsupportedProtocol,
    )?;
    ensure(
        receipt.decision_id == decision.decision_id
            && receipt.decision_hash == product_decision_hash(decision)
            && receipt.adapter == decision.offer.adapter
            && receipt.applied_at_ms >= decision.decided_at_ms,
        Error::IntegrityFailed,
    )?;
    if let ProductOutcome::Applied {
        committed_until_ms, ..
    } = receipt.outcome
    {
        ensure(
            committed_until_ms == decision.offer.expires_at_ms
                && receipt.applied_at_ms < decision.apply_by_ms,
            Error::IntegrityFailed,
        )?;
    }
    Ok(())
}

/// Canonical bytes of a digest preimage. Explicit framing is shared with independent SDKs.
pub fn commitment_bytes<T: Serialize>(domain: &str, value: &T) -> Vec<u8> {
    canonical(&(1u8, domain, value))
}

/// Commit to the complete authoritative business offer.
pub fn billing_offer_hash(value: &BillingOffer) -> Hash {
    digest("dmsg/commerce/offer/v2", value)
}

/// Commit to the exact approved authentication request.
pub fn authentication_request_hash(value: &AuthenticationRequest) -> Hash {
    digest("dmsg/authentication/request/v1", value)
}

/// Commit to the exact application approval, separate from a device signature.
pub fn application_approval_hash(value: &ApplicationApproval) -> Hash {
    digest("dmsg/application/approval/v1", value)
}

/// Commit to the complete PANDA quotation.
pub fn panda_quote_hash(value: &PandaQuote) -> Hash {
    digest("dmsg/commerce/panda-quote/v2", value)
}

/// Commit to the complete cash quotation.
pub fn cash_quote_hash(value: &CashQuote) -> Hash {
    digest("dmsg/commerce/cash-quote/v2", value)
}

/// Commit to the complete immutable product delivery decision.
pub fn product_decision_hash(value: &ProductDecision) -> Hash {
    digest("dmsg/commerce/decision/v2", value)
}

/// Single raw path segment for a dedicated authentication leaf.
pub fn authentication_key(account: &AccountId, operation: &Hash) -> Vec<u8> {
    [
        b"authentication/v1/".as_slice(),
        account.as_slice(),
        operation.as_slice(),
    ]
    .concat()
}
