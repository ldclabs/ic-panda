use crate::{http, store::*};
use candid::Principal;
use dmsg_protocol::{agent, digest, sha256};
use dmsg_runtime::admin::{self, validation, Validation};
use dmsg_types::{agent::*, *};
use ic_http_certification::{HttpRequest, HttpResponse};

fn check_admin(caller: Principal) -> Result<()> {
    admin::check_admin(caller, config().init.governance)
}

#[ic_cdk::init]
fn init(args: DirectoryInit) {
    agent::validate_directory_init(&args).expect("directory configuration");
    http::rebuild(&args.custom_domains, []);
    save_config(&Config {
        schema: STABLE_SCHEMA,
        init: args,
    });
}

/// Configuration changes go through administrative methods, never upgrades.
#[ic_cdk::post_upgrade]
fn post_upgrade() {
    let c = config();
    assert_eq!(c.schema, STABLE_SCHEMA, "incompatible development state");
    rebuild(&c.init.custom_domains);
}

fn check_home(init: &DirectoryInit, home: Principal) -> Result<bool> {
    agent::check_user_home(
        &init.environment,
        &init.issuer_namespace,
        &init.user_homes,
        home,
    )
}

/// Append a user home. Its `dmsg_user` must name this canister as
/// `directory_canister` and share the environment, namespace and origin.
#[ic_cdk::update]
fn admin_add_user_home(home: Principal) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    let mut c = config();
    if check_home(&c.init, home)? {
        c.init.user_homes.push(home);
        save_config(&c);
    }
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_add_user_home(home: Principal) -> Validation {
    let init = config().init;
    validation(check_home(&init, home).map(|fresh| {
        admin::user_home_payload(&init.environment, &init.issuer_namespace, home, fresh)
    }))
}

/// Replace the domains served at `/.well-known/ic-domains`. Principal IDs
/// keep the permanent principal origin.
#[ic_cdk::update]
fn admin_set_custom_domains(domains: Vec<String>) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    agent::validate_custom_domains(&domains)?;
    let mut c = config();
    if c.init.custom_domains != domains {
        http::certify_domains(&domains);
        c.init.custom_domains = domains;
        save_config(&c);
    }
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_set_custom_domains(domains: Vec<String>) -> Validation {
    let current = config().init.custom_domains;
    validation(agent::validate_custom_domains(&domains).map(|()| {
        format!(
            "Serve custom domains [{}] at /.well-known/ic-domains (currently [{}]).{}",
            domains.join(", "),
            current.join(", "),
            admin::unchanged(domains != current, "Same domains"),
        )
    }))
}

fn publication(id: &AccountId, init: &DirectoryInit, record: &Record) -> Publication {
    Publication {
        principal_id: agent::principal_id(&init.principal_origin, id),
        home_user: record.home_user,
        version: record.version,
        updated_at: record.updated_at,
        document_digest: record.document_digest,
    }
}

/// Publish an account's principal state. Only the account's home may publish:
/// the first time, the home whose allocator fingerprint the account ID carries;
/// afterwards, the recorded home. Versions only increase: the same version with
/// the same state is idempotent, an older version returns the current
/// publication, and the same version with other content is rejected.
#[ic_cdk::update]
fn publish(account_id: AccountId, state: PrincipalState) -> Result<Publication> {
    let caller = ic_cdk::api::msg_caller();
    let init = config().init;
    ensure(init.user_homes.contains(&caller), Error::Forbidden)?;
    let current = load(&account_id);
    match &current {
        Some(record) => ensure(record.home_user == caller, Error::Forbidden)?,
        None => ensure(
            agent::is_account_home(
                &init.environment,
                &init.issuer_namespace,
                &init.user_homes,
                caller,
                &account_id,
            ),
            Error::Forbidden,
        )?,
    }
    if let Some(record) = &current {
        if state.version < record.version {
            return Ok(publication(&account_id, &init, record));
        }
    }
    let state_digest = digest("dmsg/principal-state/v1", &state);
    if let Some(record) = &current {
        if state.version == record.version {
            ensure(
                state_digest == record.state_digest,
                Error::IdempotencyConflict,
            )?;
            return Ok(publication(&account_id, &init, record));
        }
        ensure(state.updated_at > record.updated_at, Error::IntegrityFailed)?;
    }
    let document = agent::render_principal_document(&init, &account_id, &state)?;
    ensure(
        document.len() <= agent::MAX_PRINCIPAL_DOCUMENT_BYTES,
        Error::QuotaExceeded,
    )?;
    let record = Record {
        home_user: caller,
        version: state.version,
        updated_at: state.updated_at,
        state_digest,
        document_digest: sha256(&document),
        document: document.into(),
    };
    save(&account_id, &record);
    Ok(publication(&account_id, &init, &record))
}

#[ic_cdk::query]
fn get_publication(account_id: AccountId) -> Result<Publication> {
    let record = load(&account_id).ok_or(Error::NotFound)?;
    Ok(publication(&account_id, &config().init, &record))
}

#[ic_cdk::query]
fn directory_config() -> DirectoryInit {
    config().init
}

#[ic_cdk::query(hidden = true)]
fn http_request(request: HttpRequest<'static>) -> HttpResponse<'static> {
    http::serve(&request, &config().init.custom_domains, |id| {
        load(id).map(|record| (record.document, record.document_digest))
    })
}
