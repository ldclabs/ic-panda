//! The daemon methods of account security, recovery, name transfers, payout
//! revisions, the execution allowance and commerce maintenance. Changes
//! that sign with the device become approval requests; only a person
//! answers them, in the desktop approval window or at an interactive
//! terminal.

use crate::daemon::Daemon;
use dmsg_client::{commerce::Records, security::AccountChange};
use dmsg_types::{user::SensitivePolicy, AccountId, Error, Hash, KeyPurpose, Result, DAY};
use serde_json::{json, Value};

/// The methods this module serves.
pub(crate) const METHODS: [&str; 28] = [
    "account/security",
    "account/bind_request",
    "account/bind_accept",
    "account/bind_approve",
    "account/unbind",
    "account/policy",
    "account/recovery_delay",
    "recovery/status",
    "recovery/request",
    "recovery/complete",
    "recovery/dispute",
    "handle/transfer",
    "handle/transfer_commit",
    "handle/accept",
    "handle/operations",
    "handle/reconcile",
    "payment/transfers",
    "payment/revise",
    "attest/usage",
    "approval/quote",
    "ops/orders",
    "ops/transfers",
    "ops/claims",
    "ops/reconcile_order",
    "ops/process_transfer",
    "ops/revise_transfer",
    "ops/claim",
    "ops/sweep",
];

/// The events a method's success announces.
pub(crate) fn events_of(method: &str) -> &'static [&'static str] {
    match method {
        "account/bind_accept" | "recovery/request" | "recovery/complete" => {
            &["session/changed", "approval/changed"]
        }
        "account/bind_approve"
        | "account/unbind"
        | "account/policy"
        | "account/recovery_delay"
        | "recovery/dispute"
        | "handle/transfer_commit"
        | "handle/accept"
        | "approval/quote" => &["approval/changed"],
        _ => &[],
    }
}

fn text(params: &Value, name: &str) -> Result<String> {
    params[name]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| Error::InvalidInput(format!("missing {name}")))
}

fn account(params: &Value, name: &str) -> Result<AccountId> {
    text(params, name)?
        .parse()
        .map_err(|_| Error::InvalidInput(format!("{name}: account ID")))
}

fn hash(params: &Value, name: &str) -> Result<Hash> {
    hex::decode(text(params, name)?)
        .ok()
        .and_then(|b| b.try_into().ok())
        .map(Hash::new)
        .ok_or_else(|| Error::InvalidInput(format!("{name}: hex identifier")))
}

fn number(params: &Value, name: &str) -> Result<u64> {
    params[name]
        .as_u64()
        .ok_or_else(|| Error::InvalidInput(format!("missing {name}")))
}

/// An amount in base units, as a decimal string or a number.
fn amount(params: &Value, name: &str) -> Result<u128> {
    match &params[name] {
        Value::String(text) => text.parse().ok(),
        Value::Number(n) => n.as_u64().map(u128::from),
        _ => None,
    }
    .ok_or_else(|| Error::InvalidInput(format!("{name}: amount")))
}

/// The local source of a request: the CLI or the desktop.
pub(crate) fn source(params: &Value) -> Result<&'static str> {
    match params["source"].as_str() {
        Some("gui") => Ok("gui"),
        Some("cli") | None => Ok("cli"),
        Some(_) => Err(Error::InvalidInput("source".into())),
    }
}

fn purpose(name: &str) -> Result<KeyPurpose> {
    match name {
        "file_attestation" | "FileAttestation" => Ok(KeyPurpose::FileAttestation),
        "statement" | "Statement" => Ok(KeyPurpose::Statement),
        "app_action" | "AppAction" => Ok(KeyPurpose::AppAction),
        _ => Err(Error::InvalidInput(format!("purpose {name}"))),
    }
}

fn after(params: &Value) -> Result<Option<Hash>> {
    match params["after"].as_str() {
        Some(_) => hash(params, "after").map(Some),
        None => Ok(None),
    }
}

impl Daemon {
    /// Run one of [`METHODS`].
    pub(crate) async fn call_account(&self, method: &str, params: &Value) -> Result<Value> {
        let mut client = self.client().lock().await;
        match method {
            "account/security" => client.account_security().await,
            "account/bind_request" => {
                let token = client.bind_request(&account(params, "account")?).await?;
                Ok(json!({"token": token}))
            }
            "account/bind_accept" => {
                let status = client.bind_accept(&account(params, "account")?).await?;
                Ok(serde_json::to_value(status).expect("status serializes"))
            }
            "account/bind_approve" => {
                let request = client.inspect_binding(&text(params, "token")?)?;
                // The person names the login shown on the other device.
                if request.principal.to_text() != text(params, "principal")? {
                    return Err(Error::InvalidInput(
                        "principal does not match the request".into(),
                    ));
                }
                client
                    .account_request(
                        source(params)?,
                        AccountChange::BindLogin {
                            principal: request.principal.to_text(),
                            nonce: request.nonce,
                        },
                    )
                    .await
            }
            "account/unbind" => {
                let principal = text(params, "principal")?;
                client
                    .account_request(source(params)?, AccountChange::RemoveLogin { principal })
                    .await
            }
            "account/policy" => {
                let current = client.account_security().await?;
                let mut policy = SensitivePolicy {
                    frozen: current["policy"]["frozen"] == true,
                    allowed_purposes: current["policy"]["allowed_purposes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|p| purpose(p.as_str().unwrap_or_default()))
                        .collect::<Result<_>>()?,
                    daily_executions: current["policy"]["daily_executions"]
                        .as_u64()
                        .and_then(|n| u32::try_from(n).ok())
                        .ok_or(Error::IntegrityFailed)?,
                };
                if let Some(frozen) = params["frozen"].as_bool() {
                    policy.frozen = frozen;
                }
                if let Some(daily) = params["daily_executions"].as_u64() {
                    policy.daily_executions =
                        u32::try_from(daily).map_err(|_| Error::InvalidInput("daily".into()))?;
                }
                if let Some(purposes) = params["allowed_purposes"].as_array() {
                    policy.allowed_purposes = purposes
                        .iter()
                        .map(|p| purpose(p.as_str().unwrap_or_default()))
                        .collect::<Result<_>>()?;
                }
                client
                    .account_request(source(params)?, AccountChange::Policy { policy })
                    .await
            }
            "account/recovery_delay" => {
                let days = number(params, "days")?;
                client
                    .account_request(
                        source(params)?,
                        AccountChange::RecoveryDelay {
                            delay_ms: days.saturating_mul(DAY),
                        },
                    )
                    .await
            }
            "recovery/status" => client.recovery_status().await,
            "recovery/request" => client.recovery_request().await,
            "recovery/complete" => {
                let status = client.recovery_complete().await?;
                Ok(serde_json::to_value(status).expect("status serializes"))
            }
            "recovery/dispute" => {
                let status = client.recovery_status().await?;
                let op_id = status["pending"]["op_id"]
                    .as_str()
                    .and_then(|h| hex::decode(h).ok())
                    .and_then(|b| b.try_into().ok())
                    .map(Hash::new)
                    .ok_or(Error::NotFound)?;
                client
                    .account_request(source(params)?, AccountChange::DisputeRecovery { op_id })
                    .await
            }
            "handle/transfer" => {
                let (job, token) = client
                    .handle_transfer_prepare(&text(params, "name")?, &account(params, "target")?)
                    .await?;
                Ok(json!({"operation": job.view(), "token": token}))
            }
            "handle/transfer_commit" => {
                client.handle_request(source(params)?, &text(params, "id")?)
            }
            "handle/accept" => {
                let job = client
                    .handle_accept_prepare(&text(params, "token")?)
                    .await?;
                client.handle_request(source(params)?, &job.id)
            }
            "handle/operations" => Ok(Value::Array(client.handle_operations()?)),
            "handle/reconcile" => {
                client
                    .handle_reconcile(&text(params, "id")?, number(params, "block")?)
                    .await
            }
            "payment/transfers" => client.payment_transfers(&hash(params, "escrow")?).await,
            "payment/revise" => {
                client
                    .payment_revise(&hash(params, "escrow")?, number(params, "leg")?)
                    .await
            }
            "attest/usage" => client.execution_usage().await,
            "approval/quote" => {
                client
                    .approval_quote(&text(params, "id")?, &text(params, "selection")?)
                    .await
            }
            "ops/orders" => {
                client
                    .commerce_records(Records::Orders, after(params)?)
                    .await
            }
            "ops/transfers" => {
                client
                    .commerce_records(Records::Transfers, after(params)?)
                    .await
            }
            "ops/claims" => {
                client
                    .commerce_records(Records::Claims, after(params)?)
                    .await
            }
            "ops/reconcile_order" => client.commerce_reconcile_order(&hash(params, "id")?).await,
            "ops/process_transfer" => client.commerce_process_transfer(&hash(params, "id")?).await,
            "ops/revise_transfer" => {
                client
                    .commerce_revise_transfer(&hash(params, "id")?, amount(params, "fee")?)
                    .await
            }
            "ops/claim" => {
                let refresh = params["refresh"].as_bool().unwrap_or(false);
                client.commerce_claim(&hash(params, "id")?, refresh).await
            }
            "ops/sweep" => Ok(json!({"swept": client.commerce_sweep().await?})),
            _ => Err(Error::NotFound),
        }
    }
}
