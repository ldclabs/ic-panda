//! Administrative access and SNS generic-function validators.
//!
//! Every administrative method accepts its canister's controllers, which
//! operate local deployments, and the fixed SNS governance Principal, which
//! calls it when a proposal executes. Each such method has a same-argument
//! `validate_*` query that SNS governance calls before the vote: it runs the
//! method's checks against current state and renders the payload for voters.
use candid::Principal;
use dmsg_protocol::agent::account_allocator_digest;
use dmsg_types::*;

/// Admit a controller or the configured governance Principal.
pub fn check_admin(caller: Principal, governance: Principal) -> Result<()> {
    ensure(
        caller == governance || ic_cdk::api::is_controller(&caller),
        Error::Forbidden,
    )
}

/// Reply of an SNS generic-function validator: the rendered payload, or why
/// the method would currently reject it.
pub type Validation = std::result::Result<String, String>;

/// Render a validator reply; a rejection carries the error the method returns.
pub fn validation(rendered: Result<String>) -> Validation {
    rendered.map_err(|error| format!("{error:?}"))
}

/// Lowercase hexadecimal, for digests and identifiers shown to voters.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Note for a payload that matches current state: empty when it changes
/// something, otherwise `" {state}; no change."`.
pub fn unchanged(fresh: bool, state: &str) -> String {
    if fresh {
        String::new()
    } else {
        format!(" {state}; no change.")
    }
}

/// Render an `admin_add_user_home` payload with the account-ID allocator
/// fingerprint that routes accounts to the home.
pub fn user_home_payload(
    environment: &Environment,
    namespace: &str,
    home: Principal,
    fresh: bool,
) -> String {
    format!(
        "Add user home {home}, routing accounts with allocator fingerprint {}.{}",
        hex(&account_allocator_digest(environment, namespace, home)[..5]),
        unchanged(fresh, "Already listed"),
    )
}
