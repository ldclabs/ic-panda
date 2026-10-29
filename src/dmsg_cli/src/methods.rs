//! The daemon methods of channels, the inbox, the payer, plans and names
//! (plan W5). Payments, checkouts and name operations become approval
//! requests: only a person answers them, in the desktop approval window or
//! at an interactive terminal.

use crate::daemon::Daemon;
use crate::params::{absolute, account, number, optional, parse, source, text, to_json};
use dmsg_client::{
    Client,
    channel::{self, Draft, Invitation, MessageRef, OwnerTransfer, Sponsor},
    channel_log::{Action, ChannelType, Role},
    commerce, handle,
    hold::{Hold, Unlocked},
    inbox::{self, PolicyInput},
    payer,
};
use dmsg_types::{Error, Result, integration::SettlementMethod};
use serde_json::{Value, json};

/// The methods this module serves.
pub(crate) const METHODS: [&str; 69] = [
    "channel/list",
    "channel/show",
    "channel/delete_legacy_draft",
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
    "payer/status",
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
    "plan/refund_quote",
    "plan/panda",
    "plan/notice_ack",
    "plan/discard",
    "handle/resolve",
    "handle/price",
    "handle/mine",
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
        let id = || text(params, "id");
        // These wait on the relay and the IC without the client.
        let h = self.hold();
        match method {
            "channel/invite" => {
                let role = match params["role"].as_str().unwrap_or("member") {
                    "member" => Role::Member,
                    "publisher" => Role::Publisher,
                    _ => return Err(Error::InvalidInput("role".into())),
                };
                let channel = id()?;
                h.with(|c| Ok(c.session_state())).await?;
                let _turn = self.turn(&format!("channel-control:{channel}")).await;
                return Ok(to_json(
                    &channel::invite(&h, &channel, &account(params, "account")?, role).await?,
                ));
            }
            "channel/rotate" | "channel/rekey" => {
                let channel = id()?;
                h.with(|c| Ok(c.session_state())).await?;
                let _turn = self.turn(&format!("channel-control:{channel}")).await;
                let view = if method == "channel/rekey" {
                    channel::rekey(&h, &channel).await?
                } else {
                    channel::rotate(&h, &channel).await?
                };
                return Ok(to_json(&view));
            }
            "channel/sync" => {
                let backfill = params["backfill"].as_bool().unwrap_or(false);
                return Ok(to_json(&channel::sync(&h, &id()?, backfill).await?));
            }
            "channel/send" => {
                let message_id = params["message_id"].as_str();
                let _turn = self.message_turn(message_id).await;
                let channel = id()?;
                let result = channel::send(&h, &channel, &draft(params)?, message_id).await;
                self.hint_channel(&channel);
                return result;
            }
            "channel/send_file" => {
                let message_id = params["message_id"].as_str();
                let _turn = self.message_turn(message_id).await;
                let channel = id()?;
                let result = channel::send_file(
                    &h,
                    &channel,
                    &text(params, "file")?,
                    params["version"].as_str(),
                    &draft(params)?,
                    message_id,
                )
                .await;
                self.hint_channel(&channel);
                return result;
            }
            "channel/download" => {
                let seq = number(params, "seq")?;
                let manifest =
                    channel::download(&h, &id()?, seq, &absolute(params, "path")?).await?;
                return Ok(to_json(&manifest.public()));
            }
            "channel/mark" => {
                let channel = id()?;
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
                return channel::preview_image(&h, &id()?, number(params, "seq")?).await;
            }
            "channel/receipts" => {
                let channel = id()?;
                return h.send(|c| c.channel_receipts_request(&channel)).await;
            }
            "inbox/list" => {
                if params["refresh"] == false {
                    return h.with(|c| Ok(to_json(&c.inbox_cached()?))).await;
                }
                h.with(|c| Ok(c.session_state())).await?;
                let _turn = self.turn("inbox/read").await;
                return Ok(to_json(&inbox::list(&h).await?));
            }
            "inbox/policy" => {
                if params["refresh"] == false {
                    return h
                        .with(|c| Ok(c.inbox_cached_policy()?.unwrap_or(Value::Null)))
                        .await;
                }
                h.with(|c| Ok(c.session_state())).await?;
                let _turn = self.turn("inbox/policy").await;
                return Ok(inbox::policy(&h).await?.unwrap_or(Value::Null));
            }
            "inbox/configure" => {
                let input: PolicyInput = serde_json::from_value(params.clone())
                    .map_err(|e| Error::InvalidInput(format!("policy: {e}")))?;
                h.with(|c| Ok(c.session_state())).await?;
                let _turn = self.turn("inbox/policy").await;
                return inbox::configure(&h, &input).await;
            }
            "inbox/send" => {
                let order = params["order"].as_str().map_or_else(
                    || hex::encode(dmsg_client::crypto::random_id()),
                    str::to_string,
                );
                h.with(|c| Ok(c.session_state())).await?;
                let _turn = self.turn(&format!("inbox-send:{order}")).await;
                return inbox::send(
                    &h,
                    &account(params, "recipient")?,
                    &text(params, "text")?,
                    Some(&order),
                    params["invitation"].as_str(),
                    params["pay"].as_bool().unwrap_or(false),
                )
                .await;
            }
            "inbox/order" => {
                return inbox::order(&h, &account(params, "recipient")?, &text(params, "order")?)
                    .await;
            }
            "inbox/pay" => {
                return inbox::request(
                    &h,
                    source(params)?,
                    &account(params, "recipient")?,
                    &text(params, "order")?,
                )
                .await;
            }
            "inbox/refund" => {
                h.with(|c| Ok(c.session_state())).await?;
                let _turn = self.turn("inbox/funds").await;
                return inbox::refund(&h, &text(params, "order")?).await;
            }
            "inbox/escrow" => return inbox::escrow(&h, &text(params, "order")?).await,
            "inbox/mark" => {
                let order = text(params, "order")?;
                h.with(|c| Ok(c.session_state())).await?;
                let _turn = self.turn(&format!("inbox-mark:{order}")).await;
                return inbox::mark(
                    &h,
                    &order,
                    params["read"].as_bool().unwrap_or(true),
                    params["archived"].as_bool().unwrap_or(false),
                )
                .await;
            }
            "plan/quote" => {
                let h = Unlocked::new(&h).await?;
                let method = match params["method"].as_str().unwrap_or("cash") {
                    "cash" | "Cash" => SettlementMethod::Cash,
                    "panda" | "Panda" => SettlementMethod::Panda,
                    _ => return Err(Error::InvalidInput("method".into())),
                };
                return Ok(commerce::plan_quote(
                    &h,
                    &text(params, "sku")?,
                    method,
                    &text(params, "selection")?,
                )
                .await?
                .view());
            }
            "plan/checkout" | "plan/advance" => {
                let h = Unlocked::new(&h).await?;
                let id = id()?;
                let _turn = self.turn(&format!("checkout:{id}")).await;
                return commerce::checkout_request(
                    &h,
                    source(params)?,
                    &id,
                    method == "plan/advance",
                )
                .await;
            }
            "plan/catalog" => return commerce::catalog_display(&h).await,
            "plan/status" => {
                let refresh = params["refresh"].as_bool().unwrap_or(true);
                return commerce::plan_status(&h, refresh).await;
            }
            "plan/assets" => return commerce::plan_assets(&h).await,
            "plan/order" => return commerce::plan_order_view(&h, &id()?).await,
            "plan/cancel" => return commerce::plan_cancel(&h, &id()?).await,
            "plan/reconcile" => return commerce::plan_reconcile(&h, &id()?).await,
            "plan/refund" => {
                let blocks: Vec<u64> = parse(params, "blocks")?;
                return commerce::plan_refund(&h, &id()?, &blocks).await;
            }
            "plan/refund_quote" => return commerce::plan_refund_quote(&h, &id()?).await,
            "plan/panda" => return commerce::plan_panda(&h).await,
            "plan/notice_ack" => return commerce::notice_ack(&h, &id()?).await,
            "payer/balance" => {
                let ledger = candid::Principal::from_text(text(params, "ledger")?)
                    .map_err(|_| Error::InvalidInput("ledger".into()))?;
                return payer::payer_balance(&h, ledger).await;
            }
            "payer/status" => return payer::payer_status(&h).await,
            "handle/resolve" => {
                let record = handle::resolve(&h, &text(params, "name")?).await?;
                return Ok(record.map_or(Value::Null, |r| {
                    json!({"name": r.handle, "display_handle": r.display_handle.as_ref().unwrap_or(&r.handle), "account": r.owner_account.to_string(), "version": r.version})
                }));
            }
            "handle/price" => return handle::price_view(&h, &text(params, "name")?).await,
            "handle/mine" => return Ok(Value::Array(handle::mine(&h).await?)),
            "handle/register" | "handle/claim" => {
                let h = Unlocked::new(&h).await?;
                let name = text(params, "name")?;
                let job = if method == "handle/register" {
                    handle::register_prepare(&h, &name).await?
                } else {
                    handle::claim_prepare(&h, &name).await?
                };
                return handle::request(&h, source(params)?, &job.id).await;
            }
            _ => {}
        }
        // The client alone, without the network.
        let answered = local(&mut *self.client().lock().await, method, params);
        if let Some(answer) = answered {
            return answer;
        }
        // These still wait on the relay and the IC under the client:
        // channel controls and rotations, payer setup, and actions that
        // still serialize their execution journals across the network.
        self.under_lock(async |client| match method {
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
            "channel/resume" => client.channel_resume(&id()?, &text(params, "key")?).await,
            "channel/sponsor_cancel" => Ok(to_json(&client.channel_sponsor_cancel(&id()?).await?)),
            "payer/use" => {
                let pem = optional(params, "pem", absolute)?;
                client.payer_use(pem.as_deref()).await
            }
            _ => Err(Error::NotFound),
        })
        .await
    }

    /// The turn of the message `message_id` names, so a retry waits for the
    /// send under way; a message without one is new and needs none.
    pub(crate) async fn message_turn(
        &self,
        message_id: Option<&str>,
    ) -> Option<tokio::sync::OwnedMutexGuard<()>> {
        match message_id {
            Some(id) => Some(self.turn(&format!("message:{id}")).await),
            None => None,
        }
    }
}

/// The methods of [`METHODS`] the client answers alone, without the
/// network; `None` for the others.
fn local(client: &mut Client, method: &str, params: &Value) -> Option<Result<Value>> {
    let id = || text(params, "id");
    let mut answer = || -> Result<Option<Value>> {
        Ok(Some(match method {
            "channel/list" => to_json(&client.channel_list()?),
            "channel/show" => to_json(&client.channel_view(&id()?)?),
            "channel/delete_legacy_draft" => {
                client.channel_delete_legacy_draft(&id()?)?;
                json!({"deleted": true})
            }
            "channel/messages" => {
                let limit = params["limit"].as_u64().unwrap_or(200).min(1000) as usize;
                to_json(&match params["before"].as_u64() {
                    Some(before) => client.channel_messages_before(&id()?, before, limit)?,
                    None => client.channel_messages(
                        &id()?,
                        params["after"].as_u64().unwrap_or(0),
                        limit,
                    )?,
                })
            }
            "channel/pending" => Value::Array(client.channel_pending(&id()?)?),
            "channel/outbox" => Value::Array(client.channel_outbox(&id()?)?),
            "channel/discard" => {
                client.channel_discard(&id()?, &text(params, "message_id")?)?;
                json!({"discarded": true})
            }
            "channel/draft" => {
                client.channel_draft(&id()?, params["text"].as_str().unwrap_or_default())?;
                json!({"saved": true})
            }
            "channel/settings" if params.get("muted_until").is_some() => {
                let until = match params.get("muted_until") {
                    Some(Value::Null) => None,
                    Some(value) => Some(
                        value
                            .as_u64()
                            .ok_or_else(|| dmsg_types::invalid("mute deadline"))?,
                    ),
                    None => None,
                };
                to_json(&client.channel_notification_mute(&id()?, until)?)
            }
            "channel/settings" => to_json(&client.channel_settings(
                &id()?,
                params["name"].as_str(),
                params["muted"].as_bool(),
                params["hidden"].as_bool(),
            )?),
            "channel/search" => {
                let limit = params["limit"].as_u64().unwrap_or(50).min(200) as usize;
                to_json(&client.channel_search(&id()?, &text(params, "query")?, limit)?)
            }
            "channel/hide" => to_json(&client.channel_hide(
                &id()?,
                number(params, "seq")?,
                params["hidden"].as_bool().unwrap_or(true),
            )?),
            "inbox/outgoing" => Value::Array(client.inbox_outgoing()?),
            "plan/checkouts" => {
                Value::Array(client.plan_checkouts()?.iter().map(|c| c.view()).collect())
            }
            "plan/discard" => {
                client.plan_discard(&id()?)?;
                json!({"discarded": true})
            }
            _ => return Ok(None),
        }))
    };
    answer().transpose()
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
