use crate::{http, store::*};
use dmsg_protocol::{agent, digest, sha256};
use dmsg_types::{agent::*, *};
use ic_http_certification::{HttpRequest, HttpResponse};

#[ic_cdk::init]
fn init(args: DirectoryInit) {
    agent::validate_directory_init(&args).expect("directory configuration");
    http::rebuild(&args.custom_domains, []);
    save_config(&Config {
        schema: STABLE_SCHEMA,
        init: args,
    });
}

/// Principal IDs and document fields are permanent; homes may only be appended
/// and custom domains replaced.
#[ic_cdk::post_upgrade]
fn post_upgrade(args: Option<DirectoryInit>) {
    let mut c = config();
    assert_eq!(c.schema, STABLE_SCHEMA, "incompatible development state");
    if let Some(args) = args {
        agent::validate_directory_init(&args).expect("directory configuration");
        let mut allowed = c.init.clone();
        allowed.user_homes = args.user_homes.clone();
        allowed.custom_domains = args.custom_domains.clone();
        assert_eq!(args, allowed, "principal documents are permanent");
        assert!(
            args.user_homes.starts_with(&c.init.user_homes),
            "user homes are append-only"
        );
        c.init = args;
        save_config(&c);
    }
    rebuild(&c.init.custom_domains);
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
            agent::allocated_by(
                &account_id,
                &agent::account_allocator_digest(&init.environment, &init.issuer_namespace, caller),
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
