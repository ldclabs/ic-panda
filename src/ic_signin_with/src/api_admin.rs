use http::{Uri, uri::Authority};

use crate::{
    helper::{MAX_DOMAIN_BYTES, MAX_STATEMENT_BYTES, MAX_URI_BYTES},
    store::{self, State},
};

const MAX_DOMAINS: usize = 64;

#[ic_cdk::update(guard = "is_controller")]
fn admin_update_domain(domain: String, uri: String) -> Result<(), String> {
    store::state::try_mutate(|state| {
        check_domain_update(state, &domain, &uri)?;
        state.domains.insert(domain, uri);
        Ok(())
    })
}

#[ic_cdk::update]
fn validate_admin_update_domain(domain: String, uri: String) -> Result<String, String> {
    store::state::with(|state| check_domain_update(state, &domain, &uri))?;
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
    store::state::mutate(|state| state.statement = statement);
    Ok(())
}

#[ic_cdk::update]
fn validate_admin_update_statement(statement: String) -> Result<String, String> {
    validate_statement(&statement)?;
    Ok(format!("statement: {statement}"))
}

pub fn is_controller() -> Result<(), String> {
    let caller = ic_cdk::api::msg_caller();
    if ic_cdk::api::is_controller(&caller)
        || store::state::with(|state| state.governance_canister == Some(caller))
    {
        Ok(())
    } else {
        Err("caller is not authorized".to_string())
    }
}

/// Check the statement and domains that Init args install together.
pub fn validate_config(state: &State) -> Result<(), String> {
    validate_statement(&state.statement)?;
    if state.domains.len() > MAX_DOMAINS {
        return Err(format!("domain limit of {MAX_DOMAINS} exceeded"));
    }
    state
        .domains
        .iter()
        .try_for_each(|(domain, uri)| validate_domain_mapping(domain, uri))
}

/// The proposal validator runs this too, so a proposal over the domain limit
/// fails when submitted rather than when executed.
fn check_domain_update(state: &State, domain: &str, uri: &str) -> Result<(), String> {
    validate_domain_mapping(domain, uri)?;
    if !state.domains.contains_key(domain) && state.domains.len() >= MAX_DOMAINS {
        return Err(format!("domain limit of {MAX_DOMAINS} reached"));
    }
    Ok(())
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

    #[test]
    fn domain_updates_stop_at_the_limit_unless_they_replace_one() {
        let mut state = State {
            statement: "Sign in".to_string(),
            ..Default::default()
        };
        for i in 0..MAX_DOMAINS {
            state.domains.insert(
                format!("d{i}.example.com"),
                "https://example.com".to_string(),
            );
        }
        assert!(check_domain_update(&state, "new.example.com", "https://example.com").is_err());
        assert!(check_domain_update(&state, "d0.example.com", "https://d0.example.com").is_ok());
        assert!(validate_config(&state).is_ok());

        state.domains.insert(
            "new.example.com".to_string(),
            "https://example.com".to_string(),
        );
        assert!(validate_config(&state).is_err());
        assert!(validate_config(&State::default()).is_err());
    }
}
