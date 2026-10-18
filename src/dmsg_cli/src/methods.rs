//! The daemon methods of channels, the inbox, the payer, plans and names
//! (plan W5). Payments, checkouts and name operations become approval
//! requests: only a person answers them, in the desktop approval window or
//! at an interactive terminal.

use crate::daemon::Daemon;
use crate::params::{absolute, account, number, optional, parse, source, text, to_json};
use dmsg_client::{
    Client,
    channel::{Draft, Invitation, MessageRef, OwnerTransfer, Posting, Reading, Sponsor},
    channel_log::{Action, ChannelType, Role},
    cloud::found,
    inbox::PolicyInput,
};
use dmsg_types::{Error, Result, integration::SettlementMethod};
use serde_json::{Value, json};

/// The methods this module serves.
pub(crate) const METHODS: [&str; 62] = [
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
    "channel/outbox",
    "channel/discard",
    "channel/mark",
    "channel/draft",
    "channel/settings",
    "channel/search",
    "channel/hide",
    "channel/preview",
    "channel/sponsor_cancel",
    "channel/names",
    "channel/receipts",
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
        "channel/list" | "channel/show" | "channel/messages" | "channel/pending"
        | "channel/outbox" | "channel/search" | "channel/preview" | "channel/names"
        | "channel/receipts" | "channel/draft" => &[],
        // A read position moves only counts, which the views keep themselves.
        "channel/mark" => &["channel/read"],
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
        // Relay reads signed under the client, and the user homes' public
        // queries, wait for the network without it.
        match method {
            "channel/sync" if params["backfill"] != true => {
                let channel = text(params, "id")?;
                let mut reading = Reading::new(&channel);
                crate::channels::read(self, &mut reading).await?;
                return Ok(json!({
                    "through": self.client().lock().await.channel_view(&channel)?.message_cursor,
                    "received": reading.received,
                }));
            }
            "channel/send" => {
                let channel = text(params, "id")?;
                let draft = draft(params)?;
                crate::channels::read(self, &mut Reading::controls(&channel)).await?;
                let (relay, (message_id, posting)) = {
                    let mut client = self.client().lock().await;
                    let request = client
                        .channel_send_request(&channel, &draft, params["message_id"].as_str())
                        .await?;
                    (client.relay(), request)
                };
                let reply = match posting {
                    Posting::Done(result) => Ok(result),
                    Posting::Send(request) => relay.send(request).await,
                };
                let receipt =
                    self.client()
                        .lock()
                        .await
                        .channel_send_settle(&channel, &message_id, reply)?;
                // The relay stored it: a read that fails now never fails the
                // send, and the next read takes the message.
                let _ = crate::channels::read(self, &mut Reading::new(&channel)).await;
                return Ok(json!({"message_id": message_id, "receipt": receipt}));
            }
            "channel/mark" => {
                let channel = text(params, "id")?;
                let through = number(params, "through")?;
                let view = self.client().lock().await.channel_mark(&channel, through)?;
                // A read receipt only when the account says so (C04). The
                // mark is kept either way, and the next one carries the
                // receipt a lost one left behind.
                if params["receipt"] == true {
                    let _ = crate::channels::report(self, &channel, Some(view.read)).await;
                }
                return Ok(to_json(&view));
            }
            "channel/names" => {
                let accounts: Vec<String> = parse(params, "accounts")?;
                return crate::channels::names(self, &accounts).await;
            }
            "channel/preview" => {
                let (channel, seq) = (text(params, "id")?, number(params, "seq")?);
                return crate::channels::preview(self, &channel, seq).await;
            }
            "channel/receipts" => {
                let channel = text(params, "id")?;
                let (relay, request) = self
                    .signed(|c| c.channel_receipts_request(&channel))
                    .await?;
                return relay.send(request).await;
            }
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
            "channel/send_file" => {
                client
                    .channel_send_file(
                        &id()?,
                        &text(params, "file")?,
                        params["version"].as_str(),
                        &draft(params)?,
                        params["message_id"].as_str(),
                    )
                    .await
            }
            "channel/sync" => {
                let backfill = params["backfill"].as_bool().unwrap_or(false);
                Ok(to_json(&client.channel_sync(&id()?, backfill).await?))
            }
            "channel/messages" => {
                let limit = params["limit"].as_u64().unwrap_or(200).min(1000) as usize;
                Ok(to_json(&match params["before"].as_u64() {
                    Some(before) => client.channel_messages_before(&id()?, before, limit)?,
                    None => client.channel_messages(
                        &id()?,
                        params["after"].as_u64().unwrap_or(0),
                        limit,
                    )?,
                }))
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
            "channel/outbox" => Ok(Value::Array(client.channel_outbox(&id()?)?)),
            "channel/discard" => {
                client.channel_discard(&id()?, &text(params, "message_id")?)?;
                Ok(json!({"discarded": true}))
            }
            "channel/draft" => {
                client.channel_draft(&id()?, params["text"].as_str().unwrap_or_default())?;
                Ok(json!({"saved": true}))
            }
            "channel/settings" => Ok(to_json(&client.channel_settings(
                &id()?,
                params["name"].as_str(),
                params["muted"].as_bool(),
                params["hidden"].as_bool(),
            )?)),
            "channel/search" => {
                let limit = params["limit"].as_u64().unwrap_or(50).min(200) as usize;
                Ok(to_json(&client.channel_search(
                    &id()?,
                    &text(params, "query")?,
                    limit,
                )?))
            }
            "channel/hide" => Ok(to_json(&client.channel_hide(
                &id()?,
                number(params, "seq")?,
                params["hidden"].as_bool().unwrap_or(true),
            )?)),
            "channel/sponsor_cancel" => Ok(to_json(&client.channel_sponsor_cancel(&id()?).await?)),
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

/// The message a call writes: its text, the messages it names and a
/// transfer packet; the Agent ID is never the caller's to claim.
fn draft(params: &Value) -> Result<Draft> {
    let reference = |field: &str| -> Result<Option<MessageRef>> {
        match &params[field] {
            Value::Null => Ok(None),
            value => serde_json::from_value(value.clone())
                .map(Some)
                .map_err(|_| Error::InvalidInput(field.into())),
        }
    };
    Ok(Draft {
        text: params["text"].as_str().unwrap_or_default().to_string(),
        reply_to: reference("reply_to")?,
        edit_of: reference("edit_of")?,
        retract: reference("retract")?,
        agent: None,
        packet: params.get("packet").filter(|p| !p.is_null()).cloned(),
    })
}
