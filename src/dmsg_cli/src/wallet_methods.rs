//! The daemon methods of team wallets (`wallet/*`). Reading, proposing,
//! executing approved proposals, broadcasting, reporting and returning an
//! unused activation deposit to its payer run at once; creating a wallet,
//! joining one, deciding on a proposal and cosigning become approval
//! requests, which only a person answers.

use crate::daemon::Daemon;
use crate::params::{number, optional, parse, source, text, to_json};
use dmsg_client::hold::Hold;
use dmsg_client::wallet::{self as wallets, default_policy, parse_wallet};
use dmsg_types::{
    Error, Result,
    wallet::{ChainOutcome, Decision, Intent, MemberId, MemberInput, Role, WalletSpec},
};
use serde_json::{Value, json};

/// The methods this module serves.
pub(crate) const METHODS: [&str; 54] = [
    "wallet/activation_continue",
    "wallet/activation",
    "wallet/assets",
    "wallet/book",
    "wallet/readiness",
    "wallet/preview",
    "wallet/transfer",
    "wallet/change",
    "wallet/receive",
    "wallet/versions",
    "wallet/health",
    "wallet/upgrade",
    "wallet/lock_code",
    "wallet/rpc",
    "wallet/rpc_set",
    "wallet/rpc_check",
    "wallet/audit_page",
    "wallet/cosign",
    "wallet/cosign_status",
    "wallet/cosign_import",
    "wallet/cosign_execute",
    "wallet/channel",
    "wallet/notify",
    "wallet/list",
    "wallet/show",
    "wallet/principal",
    "wallet/quote",
    "wallet/quote_preview",
    "wallet/create",
    "wallet/deposits",
    "wallet/refund",
    "wallet/join",
    "wallet/proposals",
    "wallet/proposal",
    "wallet/propose",
    "wallet/member_agents",
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
        "wallet/execute" | "wallet/cosign" | "wallet/activation_continue" => {
            &["approval/changed", "wallet/changed"]
        }
        "wallet/change"
        | "wallet/transfer"
        | "wallet/upgrade"
        | "wallet/lock_code"
        | "wallet/cosign_import"
        | "wallet/cosign_execute"
        | "wallet/rpc_set"
        | "wallet/propose"
        | "wallet/member_agents"
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
        let wallet = || parse_wallet(&text(params, "wallet")?);
        let id = || number::<u64>(params, "proposal");
        // The wallet's reads wait on the IC without the client.
        let h = self.hold();
        // Pin the account before any queue or network wait.
        h.with(|_| Ok(())).await?;
        match method {
            "wallet/activation" => {
                return wallets::desktop::activation(&h, &text(params, "quote")?).await;
            }
            "wallet/assets" => {
                return wallets::desktop::assets(
                    &h,
                    wallet()?,
                    params["refresh"].as_bool().unwrap_or(false),
                )
                .await;
            }
            "wallet/book" => {
                return wallets::desktop::book(&h, wallet()?, params["after"].as_str()).await;
            }
            "wallet/readiness" => {
                return wallets::desktop::readiness(
                    &h,
                    params["funding"].as_bool().unwrap_or(false),
                )
                .await;
            }
            "wallet/preview" => {
                return wallets::desktop::preview(&h, wallet()?, parse(params, "draft")?).await;
            }
            "wallet/receive" => {
                return wallets::desktop::receive(&h, wallet()?, number(params, "address")?).await;
            }
            "wallet/versions" => return wallets::desktop::versions(&h, wallet()?).await,
            "wallet/health" => return wallets::desktop::health(&h, wallet()?).await,
            "wallet/rpc_check" => return wallets::desktop::rpc_check(&h).await,
            "wallet/audit_page" => {
                return wallets::audit::read(&h, wallet()?, params["before"].as_u64()).await;
            }
            "wallet/cosign_status" => {
                return wallets::packets::status(&h, wallet()?, id()?, number(params, "plan")?)
                    .await;
            }
            "wallet/list" => return wallets::list(&h).await,
            "wallet/show" => return wallets::show(&h, wallet()?).await,
            "wallet/principal" => return wallets::principal(&h).await,
            "wallet/quote" => return wallets::quote(&h).await,
            "wallet/quote_preview" => return wallets::quote_preview(&h).await,
            "wallet/deposits" => return wallets::deposits(&h).await,
            "wallet/proposals" => {
                let limit = params["limit"].as_u64().unwrap_or(50).min(100) as u16;
                return wallets::proposals(&h, wallet()?, params["before"].as_u64(), limit).await;
            }
            "wallet/proposal" => return wallets::proposal(&h, wallet()?, id()?).await,
            "wallet/balances" => return wallets::balances(&h, wallet()?).await,
            "wallet/audit" => return wallets::audit(&h, wallet()?).await,
            _ => {}
        }
        // The client alone, without the network.
        let answered = h
            .with(|client| {
                Ok(match method {
                    "wallet/rpc" => Some(wallets::desktop::rpc_settings(client, None)),
                    "wallet/rpc_set" => Some(wallets::desktop::rpc_settings(
                        client,
                        Some(parse(params, "settings")?),
                    )),
                    "wallet/channel" => Some(wallets::desktop::channel_binding(
                        client,
                        wallet()?,
                        params["channel"].as_str(),
                    )),
                    "wallet/create" => Some(client.wallet_create_request(
                        source(params)?,
                        &text(params, "quote")?,
                        spec(params)?,
                    )),
                    "wallet/cosigners" => Some(client.wallet_cosigners_local()),
                    "wallet/cosigner_new" => Some(client.wallet_cosigner_new(
                        &text(params, "label")?,
                        params["hardware"].as_bool().unwrap_or(false),
                    )),
                    "wallet/policy_default" => Some(Ok(to_json(&default_policy(
                        threshold(params, "approvals")?,
                        threshold(params, "admin_approvals")?,
                    )))),
                    // The JSON shape of an intent, for building one by hand.
                    "wallet/intent" => {
                        let intent: Intent = parse(params, "intent")?;
                        Some(Ok(
                            json!({"intent": dmsg_client::wallet::intent_view(&intent)}),
                        ))
                    }
                    _ => None,
                })
            })
            .await?;
        if let Some(answer) = answered {
            return answer;
        }
        // Mutations of one wallet take turns; network waits release the client.
        let resource = format!(
            "wallet:{}",
            params["wallet"].as_str().unwrap_or("activation")
        );
        let _turn = self.turn(&resource).await;
        let mut result = match method {
            "wallet/activation_continue" => {
                wallets::desktop::continue_activation(
                    &h,
                    source(params)?,
                    &text(params, "request")?,
                )
                .await
            }
            "wallet/change" => {
                wallets::desktop::change(&h, wallet()?, parse(params, "change")?).await
            }
            "wallet/transfer" => {
                wallets::desktop::transfer(
                    &h,
                    wallet()?,
                    parse(params, "draft")?,
                    number(params, "policy_version")?,
                )
                .await
            }
            "wallet/upgrade" | "wallet/lock_code" => {
                wallets::desktop::maintain(&h, wallet()?, method == "wallet/lock_code").await
            }
            "wallet/cosign" => {
                wallets::packets::request(
                    &h,
                    source(params)?,
                    wallet()?,
                    id()?,
                    number(params, "plan")?,
                )
                .await
            }
            "wallet/cosign_import" => {
                wallets::packets::import(
                    &h,
                    wallet()?,
                    id()?,
                    number(params, "plan")?,
                    &text(params, "packet")?,
                )
                .await
            }
            "wallet/cosign_execute" => {
                wallets::packets::execute(&h, wallet()?, id()?, number(params, "plan")?).await
            }
            "wallet/notify" => wallets::desktop::notify(&h, wallet()?, params["proposal"].as_u64())
                .await
                .map(|sent| json!({"sent":sent})),
            "wallet/refund" => wallets::ops::refund(&h, &text(params, "quote")?).await,
            "wallet/join" => wallets::ops::join_request(&h, source(params)?, wallet()?).await,
            "wallet/propose" => {
                let intent: Intent = parse(params, "intent")?;
                wallets::ops::propose(&h, wallet()?, intent, None).await
            }
            "wallet/member_agents" => {
                wallets::ops::member_agents(&h, wallet()?, parse(params, "agent_ids")?).await
            }
            "wallet/send" => {
                wallets::ops::send(
                    &h,
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
                wallets::ops::decide_request(&h, source(params)?, wallet()?, id()?, decision).await
            }
            "wallet/cancel" => wallets::ops::cancel(&h, wallet()?, id()?).await,
            "wallet/freeze" => wallets::ops::freeze(&h, wallet()?).await,
            "wallet/execute" => wallets::ops::execute(&h, source(params)?, wallet()?, id()?).await,
            "wallet/broadcast" => {
                let plan = optional(params, "plan", number)?;
                wallets::ops::broadcast(&h, wallet()?, id()?, plan).await
            }
            "wallet/settle" => wallets::ops::settle(&h, wallet()?, id()?).await,
            "wallet/report" => {
                wallets::ops::report(
                    &h,
                    wallet()?,
                    id()?,
                    number(params, "plan")?,
                    outcome(&text(params, "outcome")?)?,
                    params["note"].as_str().unwrap_or_default(),
                )
                .await
            }
            "wallet/reconcile" => wallets::ops::reconcile(&h, wallet()?, id()?).await,
            "wallet/refuel" => wallets::ops::refuel(&h, wallet()?).await,
            _ => Err(Error::NotFound),
        }?;
        if events_of(method).contains(&"wallet/changed") && params["wallet"].is_string() {
            let proposal = result["id"].as_u64().or(params["proposal"].as_u64());
            if let Err(error) = wallets::desktop::notify(&h, wallet()?, proposal).await {
                result["notification_error"] = json!(error.to_string());
            }
        }
        Ok(result)
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
