//! Preparing quotes and approval requests releases the client during every
//! network wait. Fixed terms, payer checks and journals stay unchanged.

use super::*;

pub(super) async fn registrations<H: Hold>(
    h: &H,
    commerce: Principal,
    product: &str,
) -> Result<(AppRegistration, ProductRegistration)> {
    let ic = h.anonymous().await?;
    let environment = h.with(|c| Ok(c.config.environment.clone())).await?;
    let batch: Result<CertifiedBatch> = ic
        .query(
            commerce,
            "integration_configuration_certificate",
            (DMSG_APP, Some(product)),
        )
        .await?;
    let batch = batch?;
    let root = ic.root_key();
    let now = now_ms();
    let app_key = digest("dmsg/registration/app/v1", &DMSG_APP).to_vec();
    let product_key = digest("dmsg/registration/product/v2", &product).to_vec();
    let app: AppRegistration = decode(
        &certified_leaf(&batch, &app_key, commerce, &root, now)?.ok_or(Error::NotFound)?,
        1 << 20,
    )?;
    let registered: ProductRegistration = decode(
        &certified_leaf(&batch, &product_key, commerce, &root, now)?.ok_or(Error::NotFound)?,
        1 << 20,
    )?;
    validate_app(&app)?;
    validate_product(&registered)?;
    ensure(
        app.app_id == DMSG_APP
            && app.origins.iter().any(|o| o == DMSG_ORIGIN)
            && app.environment == environment,
        Error::IntegrityFailed,
    )?;
    Ok((app, registered))
}

/// Quote this account's subscription without holding the client on the network.
/// # Errors
/// As [`Client::plan_quote`].
pub async fn plan_quote<H: Hold>(
    h: &H,
    sku: &str,
    method: SettlementMethod,
    selection: &str,
) -> Result<Checkout> {
    let h = Unlocked::new(h).await?;
    let (services, _, (actor, ic)) =
        futures_util::try_join!(services(&h), h.warm(crate::client::Warm::Relay), payer(&h))?;
    let (_, (beneficiary, account)) = h
        .signed(|c| {
            let state = c.fresh_state().ok_or(Error::PolicyStale)?;
            ensure(
                state
                    .active(&c.workspace.meta.device_id)?
                    .input
                    .capabilities
                    .contains(&Capability::FormalApprove),
                Error::Forbidden,
            )?;
            c.ensure_payer(actor)?;
            Ok((c.beneficiary()?, c.account()?.id))
        })
        .await?;
    let operation = Hash::new(crate::crypto::random_id());
    let offer: Result<BillingOffer> = ic
        .update(
            services.commerce,
            "prepare_account_subscription",
            (DMSG_APP, &beneficiary, sku, operation),
        )
        .await?;
    let request = CheckoutRequest {
        offer: offer?,
        approving_account: account,
        product_approval: None,
        method,
    };
    quote(&h, request, DMSG_ORIGIN, selection, actor, &ic, &services).await
}

pub(super) async fn quote_request<H: Hold>(
    h: &H,
    request: CheckoutRequest,
    origin: &str,
    selection: &str,
) -> Result<Checkout> {
    let h = Unlocked::new(h).await?;
    let (services, (actor, ic)) = futures_util::try_join!(services(&h), payer(&h))?;
    quote(&h, request, origin, selection, actor, &ic, &services).await
}

async fn quote<H: Hold>(
    h: &H,
    request: CheckoutRequest,
    origin: &str,
    selection: &str,
    actor: Principal,
    ic: &Ic,
    services: &Services,
) -> Result<Checkout> {
    ensure(
        request.approving_account == h.with(|c| Ok(c.account()?.id)).await?,
        Error::InvalidInput("ACCOUNT_MISMATCH".into()),
    )?;
    let (app, product) = registrations(h, services.commerce, &request.offer.product_id).await?;
    let now = now_ms();
    validate_billing_offer(&request.offer, &app, &product, now)?;
    ensure(
        request
            .offer
            .allowed_settlement_methods
            .contains(&request.method)
            && app.origins.iter().any(|o| o == origin),
        Error::Forbidden,
    )?;
    let terms = match request.method {
        SettlementMethod::Cash => {
            let ledger =
                Principal::from_text(selection).map_err(|_| dmsg_types::invalid("ledger"))?;
            let payer = Account {
                owner: actor,
                subaccount: None,
            };
            h.with(|c| c.ensure_payer(actor)).await?;
            let anonymous = h.anonymous().await?;
            let (quote, assets): (Result<CheckoutQuote>, Vec<SettlementAssetView>) = futures_util::try_join!(
                ic.update(
                    services.commerce,
                    "quote_checkout",
                    (&request.offer, ledger, payer)
                ),
                anonymous.query(services.commerce, "settlement_assets", ())
            )?;
            let quote = quote?;
            ensure(
                assets
                    .iter()
                    .any(|a| a.ledger_verified && a.policy == quote.asset),
                Error::PolicyStale,
            )?;
            // The service's terms must be exactly those it would compute.
            let expected = commerce_v2::checkout_quote(
                services.commerce,
                request.offer.clone(),
                &app,
                &product,
                quote.asset.clone(),
                payer,
                quote.quoted_at_ms,
            )?;
            ensure(expected == quote, Error::IntegrityFailed)?;
            Terms::Cash(Box::new(quote))
        }
        SettlementMethod::Panda => {
            let neuron = hex_id(selection).map_err(|_| dmsg_types::invalid("neuron"))?;
            h.with(|c| c.ensure_payer(actor)).await?;
            let terms: Result<PandaApplicationTerms> = ic
                .update(
                    services.membership,
                    "quote_panda_subscription",
                    (
                        &request.offer,
                        services.home,
                        request.approving_account,
                        neuron,
                    ),
                )
                .await?;
            let terms = terms?;
            ensure(
                terms.actor == actor
                    && terms.neuron_id == neuron
                    && terms.user_home == services.home
                    && terms.approving_account == request.approving_account
                    && terms.offer == request.offer,
                Error::IntegrityFailed,
            )?;
            commerce_v2::validate_panda_terms(
                &terms,
                &app,
                &product,
                services.membership,
                terms.sns_governance,
                now_ms(),
                true,
            )?;
            Terms::Panda(Box::new(terms))
        }
    };
    h.with(|c| {
        c.ensure_payer(actor)?;
        ensure(
            c.account()?.id == request.approving_account,
            Error::Forbidden,
        )?;
        let now = now_ms();
        validate_billing_offer(&request.offer, &app, &product, now)?;
        let id = match &terms {
            Terms::Cash(_) => commerce_v2::checkout_id(services.commerce, &request.offer),
            Terms::Panda(terms) => commerce_v2::panda_claim_id(terms),
        };
        let job = Checkout {
            id: hex::encode(*id),
            actor,
            origin: origin.into(),
            request,
            terms,
            approval: None,
            stage: Stage::Review,
            created_at_ms: Some(now),
        };
        if let Ok(old) = c.checkout(&job.id) {
            ensure(
                old.request == job.request && old.actor == job.actor && old.origin == job.origin,
                Error::IdempotencyConflict,
            )?;
            // Approved terms stay fixed; only an unapproved quote is refreshed.
            if old.stage != Stage::Review {
                return Ok(old);
            }
        }
        c.save_checkout(&job)?;
        Ok(job)
    })
    .await
}

pub(super) async fn funds<H: Hold>(h: &H, job: &Checkout) -> Result<()> {
    let (actor, ic) = payer(h).await?;
    ensure(actor == job.actor, Error::Forbidden)?;
    if let Terms::Cash(q) = &job.terms {
        let balance = Ledger {
            ic: &ic,
            canister: q.cash.ledger,
        }
        .balance(actor)
        .await?;
        let total = cash_total(
            q.cash.amount_atomic,
            q.cash.fee_reserve_atomic,
            q.asset.network_fee_atomic,
        )?;
        ensure(balance >= total, crate::insufficient_funds())?;
    }
    h.with(|c| c.ensure_payer(actor)).await
}

/// Prepare an approval after checking funds, leaving other client work free.
/// # Errors
/// As [`Client::checkout_request`]; a changed journal returns `VersionConflict`.
pub async fn checkout_request<H: Hold>(h: &H, source: &str, id: &str, fresh: bool) -> Result<Json> {
    let h = Unlocked::new(h).await?;
    let job = h.with(|c| c.plan_checkout(id)).await?;
    funds(&h, &job).await?;
    h.with(|c| {
        ensure(c.plan_checkout(id)? == job, Error::VersionConflict)?;
        c.ensure_payer(job.actor)?;
        let now = now_ms();
        let request = c.approval_create_at(
            source,
            crate::approval::Action::Checkout {
                id: id.into(),
                digest: job.digest(),
                fresh,
            },
            now,
            now + crate::approval::HIGH_RISK_TTL_MS,
        )?;
        c.approval_view(&request)
    })
    .await
}
