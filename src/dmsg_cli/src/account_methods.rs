//! The daemon methods of account security, recovery, name transfers, payout
//! revisions, the execution allowance and commerce maintenance. Changes
//! that sign with the device become approval requests; only a person
//! answers them, in the desktop approval window or at an interactive
//! terminal.

use crate::daemon::Daemon;
use crate::params::{account, hash, number, optional, source, text, to_json};
use dmsg_client::{
    commerce::{self, Records},
    handle,
    hold::{Hold, Unlocked},
    security::{AccountChange, account_request},
};
use dmsg_types::{DAY, Error, Hash, KeyPurpose, Result};
use serde_json::{Value, json};

/// The methods this module serves.
pub(crate) const METHODS: [&str; 30] = [
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
    "handle/token",
    "handle/forget",
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

/// An amount in base units, as a decimal string or a number.
fn amount(params: &Value, name: &str) -> Result<u128> {
    match &params[name] {
        Value::String(text) => text.parse().ok(),
        Value::Number(n) => n.as_u64().map(u128::from),
        _ => None,
    }
    .ok_or_else(|| Error::InvalidInput(format!("{name}: amount")))
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
    optional(params, "after", hash)
}

impl Daemon {
    /// Run one of [`METHODS`].
    pub(crate) async fn call_account(&self, method: &str, params: &Value) -> Result<Value> {
        match method {
            "handle/transfer" | "handle/accept" => {
                let shared = self.hold();
                let h = Unlocked::new(&shared).await?;
                if method == "handle/transfer" {
                    let (job, token) = handle::transfer_prepare(
                        &h,
                        &text(params, "name")?,
                        &account(params, "target")?,
                    )
                    .await?;
                    return Ok(json!({"operation":job.view(),"token":token}));
                }
                let job = handle::accept_prepare(&h, &text(params, "token")?).await?;
                return handle::request(&h, source(params)?, &job.id).await;
            }

            "payment/revise" => {
                let h = self.hold();
                h.with(|c| Ok(c.session_state())).await?;
                let _turn = self.turn("inbox/funds").await;
                return dmsg_client::inbox::revise(
                    &h,
                    &hash(params, "escrow")?,
                    number(params, "leg")?,
                )
                .await;
            }

            // The user home answers without the client.
            "account/security" => {
                return dmsg_client::security::account_security(&self.hold()).await;
            }
            "handle/transfer_commit" => {
                let id = text(params, "id")?;
                return handle::request(&self.hold(), source(params)?, &id).await;
            }
            // The registry and the user homes answer without the client.
            "handle/operations" => {
                if params["refresh"] == false {
                    return self
                        .hold()
                        .with(|c| Ok(Value::Array(c.handle_operations()?)))
                        .await;
                }
                return Ok(Value::Array(handle::operations(&self.hold()).await?));
            }
            "handle/token" => {
                let token = self
                    .client()
                    .lock()
                    .await
                    .handle_token(&text(params, "id")?)?;
                return Ok(json!({"token": token}));
            }
            "handle/forget" => {
                self.client()
                    .lock()
                    .await
                    .handle_forget(&text(params, "id")?)?;
                return Ok(json!({"forgotten": true}));
            }
            "attest/usage" => return dmsg_client::attest::execution_usage(&self.hold()).await,
            "ops/orders" => {
                return commerce::commerce_records(&self.hold(), Records::Orders, after(params)?)
                    .await;
            }
            "ops/transfers" => {
                return commerce::commerce_records(
                    &self.hold(),
                    Records::Transfers,
                    after(params)?,
                )
                .await;
            }
            "ops/claims" => {
                return commerce::commerce_records(&self.hold(), Records::Claims, after(params)?)
                    .await;
            }
            "ops/reconcile_order" => {
                return commerce::commerce_reconcile_order(&self.hold(), &hash(params, "id")?)
                    .await;
            }
            "ops/process_transfer" => {
                return commerce::commerce_process_transfer(&self.hold(), &hash(params, "id")?)
                    .await;
            }
            "ops/revise_transfer" => {
                let (id, fee) = (hash(params, "id")?, amount(params, "fee")?);
                return commerce::commerce_revise_transfer(&self.hold(), &id, fee).await;
            }
            "ops/claim" => {
                let refresh = params["refresh"].as_bool().unwrap_or(false);
                return commerce::commerce_claim(&self.hold(), &hash(params, "id")?, refresh).await;
            }
            "ops/sweep" => {
                return Ok(json!({"swept": commerce::commerce_sweep(&self.hold()).await?}));
            }
            // Requests for an account change read the account without the
            // client.
            "account/bind_approve" => {
                let request = self
                    .client()
                    .lock()
                    .await
                    .inspect_binding(&text(params, "token")?)?;
                // The person names the login shown on the other device.
                if request.principal.to_text() != text(params, "principal")? {
                    return Err(Error::InvalidInput(
                        "principal does not match the request".into(),
                    ));
                }
                let change = AccountChange::BindLogin {
                    principal: request.principal.to_text(),
                    nonce: request.nonce,
                };
                return account_request(&self.hold(), source(params)?, change).await;
            }
            "account/unbind" => {
                let principal = text(params, "principal")?;
                let change = AccountChange::RemoveLogin { principal };
                return account_request(&self.hold(), source(params)?, change).await;
            }
            "account/policy" => {
                let purposes = params["allowed_purposes"]
                    .as_array()
                    .map(|values| {
                        values
                            .iter()
                            .map(|p| purpose(p.as_str().unwrap_or_default()))
                            .collect::<Result<Vec<_>>>()
                    })
                    .transpose()?;
                return dmsg_client::security::policy_request(
                    &self.hold(),
                    source(params)?,
                    params["frozen"].as_bool(),
                    optional(params, "daily_executions", number)?,
                    purposes,
                )
                .await;
            }
            "account/recovery_delay" => {
                let days = number::<u64>(params, "days")?;
                let change = AccountChange::RecoveryDelay {
                    delay_ms: days.saturating_mul(DAY),
                };
                return account_request(&self.hold(), source(params)?, change).await;
            }
            _ => {}
        }
        // Logins, recovery, handles and payments still wait on the IC under
        // the client.
        self.under_lock(async |client| match method {
            "account/bind_request" => {
                let token = client.bind_request(&account(params, "account")?).await?;
                Ok(json!({"token": token}))
            }
            "account/bind_accept" => Ok(to_json(
                &client.bind_accept(&account(params, "account")?).await?,
            )),
            "recovery/status" => client.recovery_status().await,
            "recovery/request" => client.recovery_request().await,
            "recovery/complete" => Ok(to_json(&client.recovery_complete().await?)),
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
            "handle/reconcile" => {
                client
                    .handle_reconcile(&text(params, "id")?, number(params, "block")?)
                    .await
            }
            "payment/transfers" => client.payment_transfers(&hash(params, "escrow")?).await,
            "approval/quote" => {
                // The CLI names a request by a unique prefix.
                let id = client.approval_id(&text(params, "id")?)?;
                client
                    .approval_quote(&id, &text(params, "selection")?)
                    .await
            }
            _ => Err(Error::NotFound),
        })
        .await
    }
}
