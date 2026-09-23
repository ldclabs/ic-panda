//! Exact third-party approvals, separate from document billing and vault access.
use crate::{account, state::AccountState, store};
use candid::Principal;
use dmsg_protocol::{canonical, digest, integration::*};
use dmsg_runtime::{
    storage::{MapExt, Stored},
    Certification,
};
use dmsg_types::{integration::*, user::*, *};
use ic_stable_structures::{memory_manager::VirtualMemory, DefaultMemoryImpl, StableBTreeMap};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap};

type Memory = VirtualMemory<DefaultMemoryImpl>;
const AUTH_DOMAIN: &str = "dmsg/authentication/approve/v1";
const APPLICATION_DOMAIN: &str = "dmsg/application/approve/v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum ExternalBody {
    Authentication(AuthenticationResult),
    Application(ApplicationApproval),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ApprovedExternal {
    fingerprint: Hash,
    device_id: Hash,
    security_epoch: u64,
    expires_at_ms: u64,
    body: ExternalBody,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ExternalState {
    hour: u64,
    used: u32,
    operations: BTreeMap<Hash, ApprovedExternal>,
}

thread_local! {
    static EXTERNAL: RefCell<StableBTreeMap<Vec<u8>, Stored<ExternalState>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(7)));
}

fn load(id: &AccountId) -> ExternalState {
    EXTERNAL
        .with_borrow(|t| t.load(id.as_slice()))
        .unwrap_or_default()
}

fn current_device(account: &AccountState, device_id: Hash, epoch: u64) -> Result<()> {
    ensure(
        account.status == AccountStatus::Active && !account.sensitive_policy.frozen,
        Error::Locked,
    )?;
    ensure(account.security_epoch == epoch, Error::PolicyStale)?;
    let device = account
        .devices
        .get(&device_id)
        .ok_or(Error::DeviceNotApproved)?;
    ensure(
        device.revoked_at.is_none()
            && device
                .input
                .capabilities
                .contains(&Capability::FormalApprove),
        Error::DeviceNotApproved,
    )
}

fn precheck<T: Serialize>(
    account: &AccountState,
    caller: Principal,
    approval: &Approval,
    domain: &str,
    command: &T,
    at: u64,
) -> Result<()> {
    current_device(account, approval.device_id, approval.security_epoch)?;
    account::check_device(
        account,
        caller,
        approval,
        domain,
        command,
        (Some(Capability::FormalApprove), false),
        at,
    )?;
    Ok(())
}

fn existing(
    state: &ExternalState,
    account: &AccountState,
    caller: Principal,
    id: Hash,
    fingerprint: Hash,
    at: u64,
) -> Result<Option<ExternalBody>> {
    ensure(account.auth_bindings.contains(&caller), Error::AuthRequired)?;
    let Some(record) = state.operations.get(&id) else {
        return Ok(None);
    };
    ensure(
        record.fingerprint == fingerprint,
        Error::IdempotencyConflict,
    )?;
    current_device(account, record.device_id, record.security_epoch)?;
    ensure(at < record.expires_at_ms, Error::Expired)?;
    Ok(Some(record.body.clone()))
}

/// All validation precedes mutation; failed approvals consume neither quota nor sequences.
fn commit(
    state: &mut ExternalState,
    account: &mut AccountState,
    approval: &Approval,
    fingerprint: Hash,
    body: ExternalBody,
    at: u64,
) -> Result<()> {
    let count = state
        .operations
        .values()
        .filter(|r| at < r.expires_at_ms)
        .count();
    ensure(count < MAX_ACCOUNT_OPERATIONS, Error::QuotaExceeded)?;
    let hour = at / (60 * MINUTE);
    let used = if state.hour == hour { state.used } else { 0 };
    ensure(used < MAX_HOURLY_APPROVALS, Error::QuotaExceeded)?;
    let expires_at_ms = match &body {
        ExternalBody::Authentication(result) => result.expires_at_ms,
        ExternalBody::Application(application) => {
            application.expires_at_ms.min(approval.expires_at)
        }
    };
    // Guard finish()'s checked increments before modifying the candidate state.
    ensure(account.account_version < u64::MAX, Error::QuotaExceeded)?;
    state.operations.retain(|_, r| at < r.expires_at_ms);
    state.hour = hour;
    state.used = used + 1;
    state.operations.insert(
        approval.request_id,
        ApprovedExternal {
            fingerprint,
            device_id: approval.device_id,
            security_epoch: account.security_epoch,
            expires_at_ms,
            body,
        },
    );
    account::finish(account, approval, fingerprint);
    Ok(())
}

fn save(id: &AccountId, old: &ExternalState, next: &ExternalState) {
    EXTERNAL.with_borrow_mut(|t| t.put(id.as_slice(), next));
    store::CERT.with_borrow_mut(|c| {
        for (op, record) in &old.operations {
            if matches!(record.body, ExternalBody::Authentication(_))
                && !next.operations.contains_key(op)
            {
                c.0.delete(&authentication_key(id, op));
            }
        }
        for (op, record) in &next.operations {
            if let ExternalBody::Authentication(result) = &record.body {
                c.0.insert(authentication_key(id, op), canonical(result));
            }
        }
    });
}

async fn configuration(
    app: String,
    product: Option<String>,
) -> Result<(AppRegistration, Option<ProductRegistration>)> {
    let result: Result<(AppRegistration, Option<ProductRegistration>)> = dmsg_runtime::call(
        store::config().init.commerce_canister,
        "read_integration_configuration",
        (app, product),
    )
    .await?;
    result
}

fn home_binding(app: &AppRegistration, account: &AccountState) -> Result<()> {
    ensure(
        app.environment == store::config().init.environment
            && app.user_homes.contains(&account.home_user)
            && app.cose_homes.contains(&account.home_cose),
        Error::Forbidden,
    )
}

#[ic_cdk::update]
async fn approve_authentication(
    account_id: AccountId,
    request: AuthenticationRequest,
    approval: Approval,
) -> Result<AuthenticationResult> {
    let caller = ic_cdk::api::msg_caller();
    let at = nanos_to_millis(ic_cdk::api::time());
    ensure(
        canonical(&(&request, &approval)).len() <= MAX_PAYLOAD,
        Error::QuotaExceeded,
    )?;
    let fingerprint = digest(AUTH_DOMAIN, &(&request, &approval));
    let account = store::load(&account_id)?;
    ensure(
        request.operation_id == approval.request_id,
        Error::IntegrityFailed,
    )?;
    if let Some(body) = existing(
        &load(&account_id),
        &account,
        caller,
        approval.request_id,
        fingerprint,
        at,
    )? {
        return match body {
            ExternalBody::Authentication(result) => Ok(result),
            _ => Err(Error::IdempotencyConflict),
        };
    }
    precheck(&account, caller, &approval, AUTH_DOMAIN, &request, at)?;
    let (app, _) = configuration(request.app_id.clone(), None).await?;
    // Time, epoch, sequence and operation state are reread after the external call.
    let at = nanos_to_millis(ic_cdk::api::time());
    let mut account = store::load(&account_id)?;
    let old = load(&account_id);
    if let Some(body) = existing(&old, &account, caller, approval.request_id, fingerprint, at)? {
        return match body {
            ExternalBody::Authentication(result) => Ok(result),
            _ => Err(Error::IdempotencyConflict),
        };
    }
    precheck(&account, caller, &approval, AUTH_DOMAIN, &request, at)?;
    validate_authentication(&request, &app, at)?;
    home_binding(&app, &account)?;
    let result = AuthenticationResult {
        version: INTEGRATION_VERSION,
        expires_at_ms: request.expires_at_ms.min(approval.expires_at),
        request,
        account_id: account_id.clone(),
        home_user: account.home_user,
        security_epoch: account.security_epoch,
        device_id: approval.device_id,
        approved_at_ms: at,
    };
    let mut next = old.clone();
    commit(
        &mut next,
        &mut account,
        &approval,
        fingerprint,
        ExternalBody::Authentication(result.clone()),
        at,
    )?;
    save(&account_id, &old, &next);
    store::save(&account);
    Ok(result)
}

#[ic_cdk::query]
fn authentication_certificate(account_id: AccountId, operation_id: Hash) -> Result<CertifiedBatch> {
    let caller = ic_cdk::api::msg_caller();
    let at = nanos_to_millis(ic_cdk::api::time());
    let account = store::load(&account_id)?;
    ensure(account.auth_bindings.contains(&caller), Error::AuthRequired)?;
    let state = load(&account_id);
    let record = state.operations.get(&operation_id).ok_or(Error::NotFound)?;
    ensure(
        matches!(record.body, ExternalBody::Authentication(_)),
        Error::Forbidden,
    )?;
    current_device(&account, record.device_id, record.security_epoch)?;
    ensure(at < record.expires_at_ms, Error::Expired)?;
    store::CERT.with_borrow(|c| {
        c.batch(
            ic_cdk::api::canister_self(),
            vec![authentication_key(&account_id, &operation_id)],
        )
    })
}

fn service_for(purpose: &ApprovalPurpose, account: &AccountState) -> Principal {
    let config = store::config().init;
    match purpose {
        ApprovalPurpose::CashCheckout => config.commerce_canister,
        ApprovalPurpose::PandaSubscription => config.membership_canister,
        ApprovalPurpose::AppAction => account.home_cose,
    }
}

#[ic_cdk::update]
async fn approve_application(application: ApplicationApproval, approval: Approval) -> Result<Hash> {
    let caller = ic_cdk::api::msg_caller();
    let at = nanos_to_millis(ic_cdk::api::time());
    let id = application.approving_account.clone();
    ensure(
        canonical(&(&application, &approval)).len() <= MAX_PAYLOAD,
        Error::QuotaExceeded,
    )?;
    let fingerprint = digest(APPLICATION_DOMAIN, &(&application, &approval));
    let account = store::load(&id)?;
    if existing(
        &load(&id),
        &account,
        caller,
        approval.request_id,
        fingerprint,
        at,
    )?
    .is_some()
    {
        return Ok(approval.request_id);
    }
    precheck(
        &account,
        caller,
        &approval,
        APPLICATION_DOMAIN,
        &application,
        at,
    )?;
    let (app, product) = configuration(
        application.app_id.clone(),
        Some(application.beneficiary.product_id.clone()),
    )
    .await?;
    let at = nanos_to_millis(ic_cdk::api::time());
    let mut account = store::load(&id)?;
    let old = load(&id);
    if existing(&old, &account, caller, approval.request_id, fingerprint, at)?.is_some() {
        return Ok(approval.request_id);
    }
    precheck(
        &account,
        caller,
        &approval,
        APPLICATION_DOMAIN,
        &application,
        at,
    )?;
    validate_application_approval(
        &application,
        &app,
        &product.ok_or(Error::NotFound)?,
        service_for(&application.purpose, &account),
        at,
    )?;
    home_binding(&app, &account)?;
    let mut next = old.clone();
    commit(
        &mut next,
        &mut account,
        &approval,
        fingerprint,
        ExternalBody::Application(application),
        at,
    )?;
    save(&id, &old, &next);
    store::save(&account);
    Ok(approval.request_id)
}

/// Read-only revalidation by the fixed consumer. It creates no product order or claim.
#[ic_cdk::update]
async fn verify_application_authorization(
    approval_id: Hash,
    expected: ApplicationApproval,
) -> Result<ApplicationAuthorization> {
    let caller = ic_cdk::api::msg_caller();
    ensure(caller == expected.service, Error::Forbidden)?;
    let (app, product) = configuration(
        expected.app_id.clone(),
        Some(expected.beneficiary.product_id.clone()),
    )
    .await?;
    let at = nanos_to_millis(ic_cdk::api::time());
    let account = store::load(&expected.approving_account)?;
    home_binding(&app, &account)?;
    validate_application_approval(
        &expected,
        &app,
        &product.ok_or(Error::NotFound)?,
        service_for(&expected.purpose, &account),
        at,
    )?;
    let state = load(&account.account_id);
    let record = state.operations.get(&approval_id).ok_or(Error::NotFound)?;
    ensure(
        record.body == ExternalBody::Application(expected.clone()),
        Error::IdempotencyConflict,
    )?;
    current_device(&account, record.device_id, record.security_epoch)?;
    ensure(at < record.expires_at_ms, Error::Expired)?;
    Ok(ApplicationAuthorization {
        approval_id,
        approval_hash: application_approval_hash(&expected),
        security_epoch: account.security_epoch,
        verified_at_ms: at,
        valid_until_ms: record.expires_at_ms,
    })
}

/// Bounded public cleanup removes only expired authentication/approval records.
#[ic_cdk::update]
fn prune_external_approvals(after: serde_bytes::ByteBuf) -> Option<serde_bytes::ByteBuf> {
    let at = nanos_to_millis(ic_cdk::api::time());
    let entries = EXTERNAL.with_borrow(|t| t.page(after.to_vec(), 64));
    let cursor = entries.last().map(|(key, _)| key.clone().into());
    for (key, old) in entries {
        let Ok(raw) = <[u8; 12]>::try_from(key.as_slice()) else {
            continue;
        };
        let mut next = old.clone();
        next.operations.retain(|_, r| at < r.expires_at_ms);
        if next != old {
            save(&AccountId(raw), &old, &next);
        }
    }
    store::publish();
    cursor
}

pub(crate) fn rebuild(c: &mut Certification) {
    EXTERNAL.with_borrow(|t| {
        t.for_each(|_, state| {
            for (operation, record) in state.operations {
                if let ExternalBody::Authentication(result) = record.body {
                    c.0.insert(
                        authentication_key(&result.account_id, &operation),
                        canonical(&result),
                    );
                }
            }
        })
    });
}

#[cfg(test)]
#[path = "external_tests.rs"]
mod tests;

/// Confirm an exact product preparation before the local execution commit.
/// Remote permission changes after this check are also enforced by product commit.
pub(crate) async fn authorize_action(
    id: &AccountId,
    action: &dmsg_types::app_action::AppAction,
) -> Result<()> {
    dmsg_protocol::app_action::validate_app_action(action)?;
    ensure(action.signing_account == *id, Error::Forbidden)?;
    let (app, _) = configuration(action.app_id.clone(), None).await?;
    let at = nanos_to_millis(ic_cdk::api::time());
    dmsg_protocol::app_action::validate_action_admission(action, &app, at)?;
    home_binding(&app, &store::load(id)?)?;
    let approved: Result<()> = dmsg_runtime::call(
        app.action_authority,
        "verify_dmsg_action",
        (id.clone(), action.clone()),
    )
    .await?;
    approved?;
    // A governance change during the authority call invalidates the admission.
    let (current, _) = configuration(action.app_id.clone(), None).await?;
    let at = nanos_to_millis(ic_cdk::api::time());
    ensure(current == app, Error::PolicyStale)?;
    dmsg_protocol::app_action::validate_action_admission(action, &current, at)?;
    home_binding(&current, &store::load(id)?)
}

/// Personal-account product consent. Project products use their own registered authority.
#[ic_cdk::update]
async fn authorize_product_billing(
    request: dmsg_types::integration_billing::ProductAuthorizationRequest,
) -> Result<dmsg_types::integration_billing::ProductAuthorization> {
    use dmsg_protocol::commerce_v2::*;
    use dmsg_types::integration_billing::ProductAuthorization;
    let caller = ic_cdk::api::msg_caller();
    let method = match request.account_approval.purpose {
        ApprovalPurpose::CashCheckout => SettlementMethod::Cash,
        ApprovalPurpose::PandaSubscription => SettlementMethod::Panda,
        _ => return Err(Error::Forbidden),
    };
    let checked =
        verify_application_authorization(request.approval_id, request.account_approval.clone())
            .await?;
    let at = nanos_to_millis(ic_cdk::api::time());
    validate_product_request(&request, caller, method, at)?;
    let b = &request.offer.beneficiary;
    ensure(
        request.user_home == ic_cdk::api::canister_self()
            && b.authority_canister == ic_cdk::api::canister_self()
            && b.subject_schema == "dmsg-account-v1"
            && b.subject_bytes.as_ref() == request.account_approval.approving_account.as_slice(),
        Error::Forbidden,
    )?;
    let account = store::load(&request.account_approval.approving_account)?;
    ensure(
        account.status == AccountStatus::Active && !account.sensitive_policy.frozen,
        Error::Locked,
    )?;
    Ok(ProductAuthorization {
        request_hash: product_authorization_hash(&request),
        operator: request.account_approval.actor,
        verified_at_ms: at,
        valid_until_ms: checked.valid_until_ms.min(at.saturating_add(MINUTE)),
    })
}

/// A registered product may check whether its own account beneficiary still exists.
#[ic_cdk::update]
async fn verify_product_account(
    app_id: String,
    beneficiary: dmsg_types::membership::Beneficiary,
) -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    let (app, product) = configuration(app_id, Some(beneficiary.product_id.clone())).await?;
    let product = product.ok_or(Error::NotFound)?;
    validate_subject(&beneficiary, &product)?;
    ensure(
        caller == product.adapter
            && beneficiary.authority_canister == ic_cdk::api::canister_self()
            && beneficiary.subject_schema == "dmsg-account-v1"
            && app.user_homes.contains(&ic_cdk::api::canister_self()),
        Error::Forbidden,
    )?;
    let id = AccountId(
        beneficiary
            .subject_bytes
            .as_ref()
            .try_into()
            .map_err(|_| Error::IntegrityFailed)?,
    );
    let account = store::load(&id)?;
    ensure(
        account.status == AccountStatus::Active && !account.sensitive_policy.frozen,
        Error::Locked,
    )
}
