//! The daemon methods of channels, the inbox, the payer, plans and names
//! (plan W5). Payments, checkouts and name operations become approval
//! requests: only a person answers them, in the desktop approval window or
//! at an interactive terminal.

use crate::daemon::Daemon;
use crate::params::{absolute, account, number, optional, parse, source, text, to_json};
use dmsg_client::{
    Client,
    channel::{Invitation, OwnerTransfer, Sponsor},
    channel_log::{Action, ChannelType, Role},
    cloud::found,
    inbox::PolicyInput,
};
use dmsg_types::{Error, Result, integration::SettlementMethod};
use serde_json::{Value, json};

/// The methods this module serves.
pub(crate) const METHODS: [&str; 51] = [
    "channel/list",
    "channel/show",
    "channel/create",
    "channel/invite",
    "channel/join",
    "channel/control",
    "channel/leave",
    "channel/rotate",
    "channel/rekey",
    "channel/send",
    "channel/send_file",
    "channel/sync",
    "channel/messages",
    "channel/download",
    "channel/history",
    "channel/owner_propose",
    "channel/owner_accept",
    "channel/owner_commit",
    "channel/sponsor_propose",
    "channel/sponsor_accept",
    "channel/sponsor_commit",
    "channel/reactivate",
    "channel/pending",
    "channel/resume",
    "inbox/policy",
    "inbox/configure",
    "inbox/send",
    "inbox/order",
    "inbox/outgoing",
    "inbox/pay",
    "inbox/escrow",
    "inbox/refund",
    "inbox/list",
    "inbox/mark",
    "payer/use",
    "payer/balance",
    "plan/catalog",
    "plan/status",
    "plan/assets",
    "plan/quote",
    "plan/checkout",
    "plan/advance",
    "plan/order",
    "plan/checkouts",
    "plan/cancel",
    "plan/reconcile",
    "plan/refund",
    "handle/resolve",
    "handle/price",
    "handle/claim",
    "handle/register",
];

/// The events a method's success announces.
pub(crate) fn events_of(method: &str) -> &'static [&'static str] {
    match method {
        "channel/list" | "channel/show" | "channel/messages" | "channel/pending" => &[],
        m if m.starts_with("channel/") => &["channel/changed"],
        "inbox/configure" | "inbox/send" | "inbox/mark" | "inbox/refund" => &["inbox/changed"],
        "inbox/pay" | "plan/checkout" | "plan/advance" | "handle/claim" | "handle/register" => {
            &["approval/changed"]
        }
        _ => &[],
    }
}

impl Daemon {
    /// Run one of [`METHODS`].
    pub(crate) async fn call_more(&self, method: &str, params: &Value) -> Result<Value> {
        // Relay reads signed under the client wait for the network without it.
        match method {
            "inbox/policy" => {
                let (relay, request) = self.signed(Client::inbox_policy_request).await?;
                return Ok(found(relay.send(request).await)?.unwrap_or(Value::Null));
            }
            "inbox/list" => {
                let mut letters = std::collections::BTreeMap::new();
                let mut after = 0;
                for _ in 0..dmsg_client::inbox::INBOX_PAGES {
                    let (relay, request) = self.signed(|c| c.inbox_page_request(after)).await?;
                    let page = relay.send(request).await?;
                    let mut client = self.client().lock().await;
                    match client.inbox_page_settle(after, &page, &mut letters).await? {
                        Some(next) => after = next,
                        None => return Ok(to_json(&letters.into_values().collect::<Vec<_>>())),
                    }
                }
                return Err(Error::QuotaExceeded);
            }
            _ => {}
        }
        let mut client = self.client().lock().await;
        let id = || text(params, "id");
        match method {
            "channel/list" => Ok(to_json(&client.channel_list()?)),
            "channel/show" => Ok(to_json(&client.channel_view(&id()?)?)),
            "channel/create" => {
                let kind = match params["type"].as_str().unwrap_or("collaboration") {
                    "direct" => ChannelType::Direct,
                    "collaboration" => ChannelType::Collaboration,
                    "distribution" => ChannelType::Distribution,
                    _ => return Err(Error::InvalidInput("type".into())),
                };
                let channel = client.channel_create(&text(params, "name")?, kind).await?;
                Ok(to_json(&client.channel_view(&channel)?))
            }
            "channel/invite" => {
                let role = match params["role"].as_str().unwrap_or("member") {
                    "member" => Role::Member,
                    "publisher" => Role::Publisher,
                    _ => return Err(Error::InvalidInput("role".into())),
                };
                let invitation = client
                    .channel_invite(&id()?, &account(params, "account")?, role)
                    .await?;
                Ok(to_json(&invitation))
            }
            "channel/join" => {
                let invitation: Invitation = parse(params, "invitation")?;
                Ok(to_json(&client.channel_join(&invitation).await?))
            }
            "channel/control" => {
                let action: Action = parse(params, "action")?;
                let operation = params["operation"].as_str().map_or_else(
                    || hex::encode(dmsg_client::crypto::random_id()),
                    str::to_string,
                );
                Ok(to_json(
                    &client.channel_control(&id()?, action, &operation).await?,
                ))
            }
            "channel/leave" => {
                let operation = params["operation"].as_str().map_or_else(
                    || hex::encode(dmsg_client::crypto::random_id()),
                    str::to_string,
                );
                client.channel_leave(&id()?, &operation).await?;
                Ok(json!({"left": true}))
            }
            "channel/rotate" => Ok(to_json(&client.channel_rotate(&id()?).await?)),
            "channel/rekey" => Ok(to_json(&client.channel_rekey(&id()?).await?)),
            "channel/send" => {
                client
                    .channel_send(
                        &id()?,
                        &text(params, "text")?,
                        params["message_id"].as_str(),
                    )
                    .await
            }
            "channel/send_file" => {
                client
                    .channel_send_file(
                        &id()?,
                        &text(params, "file")?,
                        params["version"].as_str(),
                        params["text"].as_str().unwrap_or_default(),
                        params["message_id"].as_str(),
                    )
                    .await
            }
            "channel/sync" => {
                let backfill = params["backfill"].as_bool().unwrap_or(false);
                Ok(to_json(&client.channel_sync(&id()?, backfill).await?))
            }
            "channel/messages" => {
                let after = params["after"].as_u64().unwrap_or(0);
                let limit = params["limit"].as_u64().unwrap_or(200).min(1000) as usize;
                Ok(to_json(&client.channel_messages(&id()?, after, limit)?))
            }
            "channel/download" => {
                let seq = number(params, "seq")?;
                let manifest = client
                    .channel_download(&id()?, seq, &absolute(params, "path")?)
                    .await?;
                Ok(to_json(&manifest.public()))
            }
            "channel/history" => {
                let ledger = client
                    .channel_grant_history(
                        &id()?,
                        &account(params, "account")?,
                        number(params, "from")?,
                        number(params, "to")?,
                    )
                    .await?;
                Ok(to_json(&ledger))
            }
            "channel/owner_propose" => Ok(to_json(
                &client
                    .channel_owner_propose(&id()?, &account(params, "account")?)
                    .await?,
            )),
            "channel/owner_accept" => {
                let packet: OwnerTransfer = parse(params, "packet")?;
                Ok(to_json(&client.channel_owner_accept(&packet).await?))
            }
            "channel/owner_commit" => {
                let packet: OwnerTransfer = parse(params, "packet")?;
                Ok(to_json(&client.channel_owner_commit(&packet).await?))
            }
            "channel/sponsor_propose" => Ok(to_json(
                &client
                    .channel_sponsor_propose(&id()?, &account(params, "account")?)
                    .await?,
            )),
            "channel/sponsor_accept" => {
                let packet: Sponsor = parse(params, "packet")?;
                Ok(to_json(&client.channel_sponsor_accept(&packet).await?))
            }
            "channel/sponsor_commit" => {
                let packet: Sponsor = parse(params, "packet")?;
                client.channel_sponsor_commit(&packet).await
            }
            "channel/reactivate" => Ok(to_json(&client.channel_reactivate(&id()?).await?)),
            "channel/pending" => Ok(Value::Array(client.channel_pending(&id()?)?)),
            "channel/resume" => client.channel_resume(&id()?, &text(params, "key")?).await,
            "inbox/configure" => {
                let input: PolicyInput = serde_json::from_value(params.clone())
                    .map_err(|e| Error::InvalidInput(format!("policy: {e}")))?;
                client.inbox_configure(&input).await
            }
            "inbox/send" => {
                client
                    .inbox_send(
                        &account(params, "recipient")?,
                        &text(params, "text")?,
                        params["order"].as_str(),
                        params["invitation"].as_str(),
                        params["pay"].as_bool().unwrap_or(false),
                    )
                    .await
            }
            "inbox/order" => {
                client
                    .inbox_order(&account(params, "recipient")?, &text(params, "order")?)
                    .await
            }
            "inbox/outgoing" => Ok(Value::Array(client.inbox_outgoing()?)),
            "inbox/pay" => {
                client
                    .letter_request(
                        source(params)?,
                        &account(params, "recipient")?,
                        &text(params, "order")?,
                    )
                    .await
            }
            "inbox/escrow" => client.inbox_escrow(&text(params, "order")?).await,
            "inbox/refund" => client.inbox_refund(&text(params, "order")?).await,
            "inbox/mark" => {
                client
                    .inbox_mark(
                        &text(params, "order")?,
                        params["read"].as_bool().unwrap_or(true),
                        params["archived"].as_bool().unwrap_or(false),
                    )
                    .await
            }
            "payer/use" => {
                let pem = optional(params, "pem", absolute)?;
                client.payer_use(pem.as_deref()).await
            }
            "payer/balance" => {
                let ledger = candid::Principal::from_text(text(params, "ledger")?)
                    .map_err(|_| Error::InvalidInput("ledger".into()))?;
                client.payer_balance(ledger).await
            }
            "plan/catalog" => {
                let catalog = client.plan_catalog().await?;
                Ok(json!({
                    "version": catalog.version,
                    "plans": catalog.plans.iter().map(|p| json!({
                        "plan": format!("{:?}", p.plan_id),
                        "price_cents": p.price_cents,
                        "storage_bytes": p.limits.storage_bytes,
                    })).collect::<Vec<_>>(),
                }))
            }
            "plan/status" => {
                client
                    .plan_status(params["refresh"].as_bool().unwrap_or(true))
                    .await
            }
            "plan/assets" => client.plan_assets().await,
            "plan/quote" => {
                let method = match params["method"].as_str().unwrap_or("cash") {
                    "cash" | "Cash" => SettlementMethod::Cash,
                    "panda" | "Panda" => SettlementMethod::Panda,
                    _ => return Err(Error::InvalidInput("method".into())),
                };
                let job = client
                    .plan_quote(&text(params, "sku")?, method, &text(params, "selection")?)
                    .await?;
                Ok(job.view())
            }
            "plan/checkout" => client.checkout_request(source(params)?, &id()?, false),
            "plan/advance" => client.checkout_request(source(params)?, &id()?, true),
            "plan/order" => client.plan_order_view(&id()?).await,
            "plan/checkouts" => Ok(Value::Array(
                client.plan_checkouts()?.iter().map(|c| c.view()).collect(),
            )),
            "plan/cancel" => client.plan_cancel(&id()?).await,
            "plan/reconcile" => client.plan_reconcile(&id()?).await,
            "plan/refund" => {
                let blocks: Vec<u64> = parse(params, "blocks")?;
                client.plan_refund(&id()?, &blocks).await
            }
            "handle/resolve" => {
                let record = client.handle_resolve(&text(params, "name")?).await?;
                Ok(record.map_or(Value::Null, |r| {
                    json!({"name": r.handle, "account": r.owner_account.to_string(), "version": r.version})
                }))
            }
            "handle/price" => client.handle_price(&text(params, "name")?).await,
            "handle/claim" => {
                let job = client.handle_claim_prepare(&text(params, "name")?).await?;
                client.handle_request(source(params)?, &job.id)
            }
            "handle/register" => {
                let job = client
                    .handle_register_prepare(&text(params, "name")?)
                    .await?;
                client.handle_request(source(params)?, &job.id)
            }
            _ => Err(Error::NotFound),
        }
    }
}
