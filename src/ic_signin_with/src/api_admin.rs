use http::{uri::Authority, Uri};

use crate::{
    helper::{MAX_DOMAIN_BYTES, MAX_STATEMENT_BYTES, MAX_URI_BYTES},
    store,
};

const MAX_DOMAINS: usize = 64;

#[ic_cdk::update(guard = "is_controller")]
fn admin_update_domain(domain: String, uri: String) -> Result<(), String> {
    validate_domain_mapping(&domain, &uri)?;
    if store::state::with(|state| state.domains.get(&domain) == Some(&uri)) {
        return Ok(());
    }
    store::state::try_mutate(|state| {
        if !state.domains.contains_key(&domain) && state.domains.len() >= MAX_DOMAINS {
            return Err(format!("domain limit of {MAX_DOMAINS} reached"));
        }
        state.domains.insert(domain, uri);
        Ok(())
    })
}

#[ic_cdk::update]
fn validate_admin_update_domain(domain: String, uri: String) -> Result<String, String> {
    validate_domain_mapping(&domain, &uri)?;
    Ok(format!("domain: {domain}\nuri: {uri}"))
}

#[ic_cdk::update(guard = "is_controller")]
fn admin_remove_domain(domain: String) -> Result<(), String> {
    validate_domain(&domain)?;
    store::state::try_mutate(|state| {
        if state.domains.remove(&domain).is_none() {
            return Err("domain does not exist".to_string());
        }
        Ok(())
    })
}

#[ic_cdk::update]
fn validate_admin_remove_domain(domain: String) -> Result<String, String> {
    validate_domain(&domain)?;
    if !store::state::with(|state| state.domains.contains_key(&domain)) {
        return Err("domain does not exist".to_string());
    }
    Ok(format!("domain: {domain}"))
}

#[ic_cdk::update(guard = "is_controller")]
fn admin_update_statement(statement: String) -> Result<(), String> {
    validate_statement(&statement)?;
    if store::state::with(|state| state.statement == statement) {
        return Ok(());
    }
    store::state::mutate(|state| state.statement = statement);
    Ok(())
}

#[ic_cdk::update]
fn validate_admin_update_statement(statement: String) -> Result<String, String> {
    validate_statement(&statement)?;
    Ok(format!("statement: {statement}"))
}

fn validate_domain_mapping(domain: &str, uri: &str) -> Result<(), String> {
    validate_domain(domain)?;
    if uri.is_empty() || uri.len() > MAX_URI_BYTES {
        return Err("invalid uri length".to_string());
    }

    let parsed: Uri = uri.parse().map_err(|err| format!("invalid uri: {err}"))?;
    if parsed.scheme_str() != Some("https") {
        return Err("uri must use https scheme".to_string());
    }
    if parsed.host().is_none_or(str::is_empty) {
        return Err("uri must include a host".to_string());
    }
    Ok(())
}

fn validate_domain(domain: &str) -> Result<(), String> {
    if domain.is_empty() || domain.len() > MAX_DOMAIN_BYTES {
        return Err("invalid domain length".to_string());
    }
    let authority: Authority = domain
        .parse()
        .map_err(|err| format!("invalid domain: {err}"))?;
    if authority.host().is_empty() {
        return Err("domain must include a host".to_string());
    }
    Ok(())
}

fn validate_statement(statement: &str) -> Result<(), String> {
    if statement.is_empty() {
        return Err("statement cannot be empty".to_string());
    }
    if statement.len() > MAX_STATEMENT_BYTES {
        return Err(format!("statement exceeds {MAX_STATEMENT_BYTES} bytes"));
    }
    if statement.contains(['\r', '\n']) {
        return Err("statement cannot contain newlines".to_string());
    }
    Ok(())
}

fn is_controller() -> Result<(), String> {
    let caller = ic_cdk::api::msg_caller();
    if ic_cdk::api::is_controller(&caller)
        || store::state::with(|state| state.governance_canister == Some(caller))
    {
        Ok(())
    } else {
        Err("caller is not authorized".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_domain_mappings_with_lightweight_uri_parsing() {
        assert!(validate_domain_mapping("example.com", "https://example.com/login").is_ok());
        assert!(validate_domain_mapping("example.com:8443", "https://example.com:8443").is_ok());
        assert!(validate_domain_mapping("", "https://example.com").is_err());
        assert!(validate_domain_mapping("example.com", "http://example.com").is_err());
        assert!(validate_domain_mapping("example.com", "https://").is_err());
    }

    #[test]
    fn rejects_multiline_or_unbounded_statements() {
        assert!(validate_statement("Sign in").is_ok());
        assert!(validate_statement("line one\nline two").is_err());
        assert!(validate_statement(&"x".repeat(MAX_STATEMENT_BYTES + 1)).is_err());
    }
}
