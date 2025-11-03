use url::Url;

use crate::{helper::pretty_format, store};

#[ic_cdk::update(guard = "is_controller")]
fn admin_update_domain(domain: String, uri: String) -> Result<(), String> {
    validate_admin_update_domain(domain.clone(), uri.clone())?;
    store::state::with_mut(|s| {
        s.domains.insert(domain, uri);
        Ok(())
    })
}

#[ic_cdk::update]
fn validate_admin_update_domain(domain: String, uri: String) -> Result<String, String> {
    if domain.is_empty() {
        return Err("domain cannot be empty".to_string());
    }
    let parsed_url = Url::parse(&uri).map_err(|e| format!("invalid uri: {}", e))?;
    if parsed_url.scheme() != "https" {
        return Err("uri must use https scheme".to_string());
    }
    pretty_format(&(domain, uri))
}

#[ic_cdk::update(guard = "is_controller")]
fn admin_remove_domain(domain: String) -> Result<(), String> {
    validate_admin_remove_domain(domain.clone())?;
    store::state::with_mut(|s| {
        s.domains.remove(&domain);
        Ok(())
    })
}

#[ic_cdk::update]
fn validate_admin_remove_domain(domain: String) -> Result<String, String> {
    if domain.is_empty() {
        return Err("domain cannot be empty".to_string());
    }
    store::state::with(|s| {
        if !s.domains.contains_key(&domain) {
            return Err("domain does not exist".to_string());
        }
        Ok(())
    })?;
    pretty_format(&(domain,))
}

#[ic_cdk::update(guard = "is_controller")]
fn admin_update_statement(statement: String) -> Result<(), String> {
    if statement.is_empty() {
        return Err("statement cannot be empty".to_string());
    }

    store::state::with_mut(|s| {
        s.statement = statement;
        Ok(())
    })
}

#[ic_cdk::update]
fn validate_admin_update_statement(statement: String) -> Result<String, String> {
    if statement.is_empty() {
        return Err("statement cannot be empty".to_string());
    }

    pretty_format(&(statement,))
}

fn is_controller() -> Result<(), String> {
    let caller = ic_cdk::api::msg_caller();
    if ic_cdk::api::is_controller(&caller)
        || store::state::with(|s| s.governance_canister == Some(caller))
    {
        Ok(())
    } else {
        Err("user is not a controller".to_string())
    }
}
