//! The daemon methods of team wallets (`wallet/*`). Reading, proposing,
//! executing approved proposals, broadcasting, reporting and returning an
//! unused activation deposit to its payer run at once; creating a wallet,
//! joining one, deciding on a proposal and cosigning become approval
//! requests, which only a person answers.

use crate::daemon::Daemon;
use crate::params::{number, optional, parse, source, text, to_json};
use dmsg_client::wallet::{default_policy, parse_wallet};
use dmsg_types::{
    Error, Result,
    wallet::{ChainOutcome, Decision, Intent, MemberId, MemberInput, Role, WalletSpec},
};
use serde_json::{Value, json};

/// The methods this module serves.
pub(crate) const METHODS: [&str; 29] = [
    "wallet/list",
    "wallet/show",
    "wallet/principal",
    "wallet/quote",
    "wallet/create",
    "wallet/deposits",
    "wallet/refund",
    "wallet/join",
    "wallet/proposals",
    "wallet/proposal",
    "wallet/propose",
    "wallet/send",
    "wallet/approve",
    "wallet/reject",
    "wallet/veto",
    "wallet/cancel",
    "wallet/freeze",
    "wallet/execute",
    "wallet/broadcast",
    "wallet/settle",
    "wallet/report",
    "wallet/reconcile",
    "wallet/balances",
    "wallet/audit",
    "wallet/refuel",
    "wallet/cosigners",
    "wallet/cosigner_new",
    "wallet/policy_default",
    "wallet/intent",
];

/// The events a method's success announces.
pub(crate) fn events_of(method: &str) -> &'static [&'static str] {
    match method {
        "wallet/create" | "wallet/join" | "wallet/approve" | "wallet/reject" | "wallet/veto" => {
            &["approval/changed"]
        }
        "wallet/execute" => &["approval/changed", "wallet/changed"],
        "wallet/propose"
        | "wallet/send"
        | "wallet/cancel"
        | "wallet/freeze"
        | "wallet/broadcast"
        | "wallet/settle"
        | "wallet/report"
        | "wallet/reconcile"
        | "wallet/refuel"
        | "wallet/cosigner_new" => &["wallet/changed"],
        _ => &[],
    }
}

/// An approval threshold, one by default.
fn threshold(params: &Value, name: &str) -> Result<u8> {
    Ok(optional(params, name, number)?.unwrap_or(1))
}

fn role(text: &str) -> Result<Role> {
    Ok(match text.to_ascii_lowercase().as_str() {
        "admin" => Role::Admin,
        "approver" => Role::Approver,
        "proposer" => Role::Proposer,
        "viewer" => Role::Viewer,
        "recovery" => Role::Recovery,
        _ => return Err(Error::InvalidInput("role".into())),
    })
}

/// A member from `{id, role, label?}`: an account ID, or a Principal.
pub(crate) fn member(value: &Value) -> Result<MemberInput> {
    let id = value["id"]
        .as_str()
        .ok_or_else(|| Error::InvalidInput("member id".into()))?;
    let id = match id.parse() {
        Ok(account) => MemberId::Account(account),
        Err(_) => MemberId::Principal(
            candid::Principal::from_text(id)
                .map_err(|_| Error::InvalidInput("member id".into()))?,
        ),
    };
    Ok(MemberInput {
        id,
        role: role(value["role"].as_str().unwrap_or("approver"))?,
        label: value["label"].as_str().unwrap_or_default().to_string(),
        agent_ids: value["agent_ids"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// A wallet spec: `spec` as the full record, or `name`, `members` and the
/// two thresholds over the default policy.
fn spec(params: &Value) -> Result<WalletSpec> {
    if !params["spec"].is_null() {
        return parse(params, "spec");
    }
    let members = params["members"]
        .as_array()
        .map(|m| m.iter().map(member).collect::<Result<Vec<_>>>())
        .transpose()?
        .unwrap_or_default();
    Ok(WalletSpec {
        name: text(params, "name")?,
        members,
        policy: default_policy(
            threshold(params, "approvals")?,
            threshold(params, "admin_approvals")?,
        ),
        channel_hint: params["channel"].as_str().map(str::to_string),
    })
}

fn outcome(text: &str) -> Result<ChainOutcome> {
    Ok(match text {
        "broadcast" => ChainOutcome::Broadcast,
        "settled" => ChainOutcome::Settled,
        "failed" => ChainOutcome::Failed,
        _ => return Err(Error::InvalidInput("outcome".into())),
    })
}

impl Daemon {
    /// Run one of [`METHODS`].
    pub(crate) async fn call_wallet(&self, method: &str, params: &Value) -> Result<Value> {
        let mut client = self.client().lock().await;
        let wallet = || parse_wallet(&text(params, "wallet")?);
        let id = || number::<u64>(params, "proposal");
        match method {
            "wallet/list" => client.wallet_list().await,
            "wallet/show" => client.wallet_show(wallet()?).await,
            "wallet/principal" => client.wallet_principal().await,
            "wallet/quote" => client.wallet_quote().await,
            "wallet/create" => client.wallet_create_request(
                source(params)?,
                &text(params, "quote")?,
                spec(params)?,
            ),
            "wallet/deposits" => client.wallet_deposits().await,
            "wallet/refund" => client.wallet_refund(&text(params, "quote")?).await,
            "wallet/join" => client.wallet_join_request(source(params)?, wallet()?).await,
            "wallet/proposals" => {
                let limit = params["limit"].as_u64().unwrap_or(50).min(100) as u16;
                client
                    .wallet_proposals(wallet()?, params["before"].as_u64(), limit)
                    .await
            }
            "wallet/proposal" => client.wallet_proposal(wallet()?, id()?).await,
            "wallet/propose" => {
                let intent: Intent = parse(params, "intent")?;
                client.wallet_propose(wallet()?, intent, None).await
            }
            "wallet/send" => {
                client
                    .wallet_send(
                        wallet()?,
                        number(params, "from")?,
                        &text(params, "asset")?,
                        &text(params, "to")?,
                        &text(params, "amount")?,
                        &text(params, "max_fee")?,
                        params["memo"].as_str(),
                        None,
                    )
                    .await
            }
            "wallet/approve" | "wallet/reject" | "wallet/veto" => {
                let decision = match method {
                    "wallet/approve" => Decision::Approve,
                    "wallet/reject" => Decision::Reject,
                    _ => Decision::Veto,
                };
                client
                    .wallet_decide_request(source(params)?, wallet()?, id()?, decision)
                    .await
            }
            "wallet/cancel" => client.wallet_cancel(wallet()?, id()?).await,
            "wallet/freeze" => client.wallet_freeze(wallet()?).await,
            "wallet/execute" => {
                client
                    .wallet_execute(source(params)?, wallet()?, id()?)
                    .await
            }
            "wallet/broadcast" => {
                let plan = optional(params, "plan", number)?;
                client.wallet_broadcast(wallet()?, id()?, plan).await
            }
            "wallet/settle" => client.wallet_settle(wallet()?, id()?).await,
            "wallet/report" => {
                client
                    .wallet_report(
                        wallet()?,
                        id()?,
                        number(params, "plan")?,
                        outcome(&text(params, "outcome")?)?,
                        params["note"].as_str().unwrap_or_default(),
                    )
                    .await
            }
            "wallet/reconcile" => client.wallet_reconcile(wallet()?, id()?).await,
            "wallet/balances" => client.wallet_balances(wallet()?).await,
            "wallet/audit" => client.wallet_audit(wallet()?).await,
            "wallet/refuel" => client.wallet_refuel(wallet()?).await,
            "wallet/cosigners" => client.wallet_cosigners_local(),
            "wallet/cosigner_new" => client.wallet_cosigner_new(
                &text(params, "label")?,
                params["hardware"].as_bool().unwrap_or(false),
            ),
            "wallet/policy_default" => Ok(to_json(&default_policy(
                threshold(params, "approvals")?,
                threshold(params, "admin_approvals")?,
            ))),
            // The JSON shape of an intent, for building one by hand.
            "wallet/intent" => {
                let intent: Intent = parse(params, "intent")?;
                Ok(json!({"intent": dmsg_client::wallet::intent_view(&intent)}))
            }
            _ => Err(Error::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_fit_a_byte_and_default_to_one() {
        let spec = |approvals: Value| spec(&json!({"name": "team", "approvals": approvals}));
        assert!(matches!(spec(json!(256)), Err(Error::InvalidInput(_))));
        assert!(spec(json!(2)).is_ok());
        assert!(spec(Value::Null).is_ok());
    }
}
