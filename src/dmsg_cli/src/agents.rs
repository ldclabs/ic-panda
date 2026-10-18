//! The daemon methods of Agent Delegation and Agent Protocols (plan W6).
//!
//! The owner enables the account's principal, registers controller keys and
//! signs grants through the approval center; hosts Agent IDs and grants them
//! capabilities. An agent's MCP session presents its own credential, which
//! reaches only the `mcp/*` methods: the standard connector tools and
//! resources as its own Agent ID, and the `dmsg_*` tools, each as far as
//! its capabilities allow. Nothing an agent calls can answer an approval,
//! change the host allowlist or the grants, or reach another Agent ID.
//!
//! The audit record (plan R38) gets the owner's grants, credentials and
//! residency changes, and every call that writes or reaches the owner's
//! data, with who called, through which way in and on what.

use crate::daemon::Daemon;
use crate::params::{number, optional, parse, source, text, to_json};
use dmsg_agent::Scope;
use dmsg_agent::resources::{Resource, resource_list, resource_templates};
use dmsg_agent::tools::{is_dmsg, is_read_only, is_standard, standard_capability, tool_list};
use dmsg_client::{
    Client,
    agent::PrincipalChange,
    agents::AgentEntry,
    attest::StatementInput,
    codec::{b64, unb64},
    error_code,
    library::catalog::ROOT,
    vault::{Changes, Filter, Item, otp},
};
use dmsg_types::{
    Error, Result,
    agent::{DelegationAuthority, PrincipalType},
    ensure,
};
use serde_json::{Value, json};
use std::time::Duration;

/// Most bytes one `dmsg_files_read` returns.
const MAX_READ: u64 = 1024 * 1024;
/// Largest content one `dmsg_files_write` takes.
const MAX_AGENT_WRITE: usize = 8 * 1024 * 1024;
/// How long `mcp/changed` waits at most.
const CHANGED_WAIT_MS: u64 = 25_000;
/// Input fields that name what a call acts on, in order of preference.
const TARGETS: [&str; 9] = [
    "id",
    "wallet",
    "channel",
    "order_id",
    "endpoint_id",
    "to",
    "room_id",
    "draft_id",
    "mailbox_id",
];

/// The owner's methods this module serves.
pub(crate) const METHODS: [&str; 22] = [
    "agent/principal",
    "agent/enable",
    "agent/register",
    "agent/retire",
    "agent/compromise",
    "agent/rename",
    "agent/publish",
    "agent/refresh",
    "agent/grant",
    "agent/revoke",
    "agent/credentials",
    "agent/delegations",
    "agent/ids",
    "agent/create",
    "agent/use",
    "agent/resident",
    "agent/credential",
    "grants/list",
    "grants/add",
    "grants/revoke",
    "ap/call",
    "audit/list",
];

/// The only methods an agent's credential reaches.
pub(crate) const AGENT_METHODS: [&str; 5] = [
    "mcp/tools",
    "mcp/call",
    "mcp/resources",
    "mcp/read",
    "mcp/changed",
];

/// Who makes a tool call: an agent's MCP session, or the owner from the
/// CLI or the desktop acting as one of its Agent IDs.
struct Caller<'a> {
    /// `owner`, or the agent's `did:agent:`.
    actor: &'a str,
    /// `mcp`, `cli` or `gui`.
    via: &'a str,
    /// The connector session: the MCP session, or the owner's named one.
    session: &'a str,
    /// The MCP session, which MCP Events endpoints are bound to.
    mcp_session: Option<&'a str>,
}

/// The events a method's success announces.
pub(crate) fn events_of(method: &str) -> &'static [&'static str] {
    match method {
        "agent/enable" | "agent/register" | "agent/retire" | "agent/compromise"
        | "agent/rename" | "agent/grant" | "agent/revoke" => &["approval/changed"],
        "agent/publish" | "agent/create" | "agent/use" | "agent/resident" | "agent/credential"
        | "grants/add" | "grants/revoke" => &["agent/changed"],
        "ap/call" | "mcp/call" => &["agent/changed"],
        _ => &[],
    }
}

/// A tool's refusal in the shape tools return it.
fn refused(error: &Error) -> Value {
    json!({"error": {"code": error_code(error), "message": error.to_string()}})
}

impl Daemon {
    /// Run one of the owner's [`METHODS`].
    pub(crate) async fn call_agents(&self, method: &str, params: &Value) -> Result<Value> {
        if method == "ap/call" {
            let agent = {
                let client = self.client().lock().await;
                match params["agent"].as_str() {
                    Some(name) => client.agent_find(name)?,
                    None => client.agent_active()?,
                }
            };
            let via = source(params)?;
            let caller = Caller {
                actor: "owner",
                via,
                session: params["session"].as_str().unwrap_or(via),
                mcp_session: None,
            };
            return self
                .tool(
                    &agent,
                    &caller,
                    &text(params, "tool")?,
                    params["input"].clone(),
                )
                .await;
        }
        if method == "audit/list" {
            let client = self.client().lock().await;
            let actor = match params["agent"].as_str() {
                Some("owner") => Some("owner".to_string()),
                Some(name) => Some(client.agent_find(name)?.agent_id),
                None => None,
            };
            let entries = client.audit_list(
                params["before"].as_u64(),
                actor.as_deref(),
                params["limit"].as_u64().unwrap_or(100) as usize,
            )?;
            return Ok(json!({"entries": entries}));
        }
        // The client alone, without the network.
        let answered = {
            let mut client = self.client().lock().await;
            match method {
                "agent/delegations" => Some(client.delegations().map(Value::Array)),
                "agent/ids" => Some(client.agent_list().map(|mut list| {
                    list["hosts"] = to_json(self.agents().hosts());
                    list
                })),
                "agent/create" => Some((|| {
                    let entry = client.agent_create(&text(params, "label")?)?;
                    let via = source(params)?;
                    client.audit("owner", via, method, Some(&entry.agent_id), None, "ok");
                    Ok(entry.view())
                })()),
                "agent/use" => Some((|| Ok(client.agent_use(&text(params, "agent")?)?.view()))()),
                "agent/resident" => Some((|| {
                    let on = params["on"]
                        .as_bool()
                        .ok_or_else(|| Error::InvalidInput("on".into()))?;
                    let entry = client.agent_resident(&text(params, "agent")?, on)?;
                    let detail = if on { "on" } else { "off" };
                    let via = source(params)?;
                    client.audit(
                        "owner",
                        via,
                        method,
                        Some(&entry.agent_id),
                        Some(detail),
                        "ok",
                    );
                    Ok(entry.view())
                })()),
                "agent/credential" => Some((|| {
                    let agent = client.agent_find(&text(params, "agent")?)?;
                    let credential = client.agent_credential(&agent.agent_id)?;
                    let via = source(params)?;
                    client.audit("owner", via, method, Some(&agent.agent_id), None, "ok");
                    Ok(json!({"agent_id": agent.agent_id, "credential": credential}))
                })()),
                "grants/list" => Some(client.capability_grants().map(|g| to_json(&g))),
                "grants/add" => Some((|| {
                    let grant = client.capability_grant(
                        source(params)?,
                        &text(params, "agent")?,
                        parse(params, "capabilities")?,
                        params["days"].as_u64(),
                    )?;
                    Ok(to_json(&grant))
                })()),
                "grants/revoke" => Some((|| {
                    Ok(to_json(&client.capability_revoke(
                        source(params)?,
                        &text(params, "id")?,
                    )?))
                })()),
                _ => None,
            }
        };
        if let Some(answer) = answered {
            return answer;
        }
        // The Agent Delegation principal and its delegations still wait on
        // the IC under the client.
        self.under_lock(async |client| {
            let principal = match method {
                "agent/enable" => Some(PrincipalChange::Enable {
                    principal_type: match params["type"].as_str().unwrap_or("person") {
                        "person" => PrincipalType::Person,
                        "organization" => PrincipalType::Organization,
                        "team" => PrincipalType::Team,
                        "project" => PrincipalType::Project,
                        "other" => PrincipalType::Other,
                        _ => return Err(Error::InvalidInput("type".into())),
                    },
                }),
                "agent/register" => {
                    let delegation = match (params.get("scopes"), params.get("audiences")) {
                        (None | Some(Value::Null), None | Some(Value::Null)) => {
                            DelegationAuthority::Unrestricted
                        }
                        _ => DelegationAuthority::Restricted {
                            scopes: parse(params, "scopes")?,
                            audiences: parse(params, "audiences")?,
                        },
                    };
                    let info = client.principal().await?.ok_or(Error::NotFound)?;
                    Some(PrincipalChange::Register {
                        generation: dmsg_client::agent::next_generation(&info),
                        name: params["name"].as_str().map(str::to_string),
                        delegation,
                    })
                }
                "agent/retire" => Some(PrincipalChange::Retire {
                    generation: number(params, "generation")?,
                }),
                "agent/compromise" => Some(PrincipalChange::Compromise {
                    generation: number(params, "generation")?,
                    invalid_from: params["invalid_from"]
                        .as_u64()
                        .ok_or_else(|| Error::InvalidInput("invalid_from".into()))?,
                }),
                "agent/rename" => Some(PrincipalChange::Rename {
                    generation: number(params, "generation")?,
                    name: params["name"].as_str().map(str::to_string),
                }),
                _ => None,
            };
            if let Some(change) = principal {
                return client.principal_request(source(params)?, change).await;
            }
            match method {
                "agent/principal" => client.principal_view().await,
                "agent/publish" => {
                    Ok(json!({"published_version": client.principal_publish().await?}))
                }
                "agent/refresh" => client.principal_refresh().await,
                "agent/grant" => {
                    let generation = match params["generation"].is_null() {
                        true => latest_controller(client).await?,
                        false => number(params, "generation")?,
                    };
                    // The subject may name a hosted Agent ID by its label.
                    let subject = text(params, "subject")?;
                    let subject = client.agent_find(&subject).map_or(subject, |a| a.agent_id);
                    client
                        .delegation_grant_request(
                            source(params)?,
                            generation,
                            &subject,
                            parse(params, "scopes")?,
                            parse(params, "audiences")?,
                            params["days"].as_u64().unwrap_or(30),
                            params["relationship"].as_str().map(str::to_string),
                        )
                        .await
                }
                "agent/revoke" => {
                    let generation = match params["generation"].is_null() {
                        true => latest_controller(client).await?,
                        false => number(params, "generation")?,
                    };
                    client
                        .delegation_revoke_request(
                            source(params)?,
                            generation,
                            &text(params, "id")?,
                            params["reason"].as_str().map(str::to_string),
                        )
                        .await
                }
                "agent/credentials" => Ok(Value::Array(client.delegation_credentials().await?)),
                _ => Err(Error::NotFound),
            }
        })
        .await
    }

    /// Run one of [`AGENT_METHODS`] for the agent whose credential called.
    pub(crate) async fn call_agent(
        &self,
        agent: &AgentEntry,
        method: &str,
        params: &Value,
    ) -> Result<Value> {
        let mcp_session = params["mcp_session"].as_str();
        let session = mcp_session.unwrap_or("mcp");
        match method {
            "mcp/tools" => Ok(json!({"tools": tool_list(), "agent_id": agent.agent_id})),
            "mcp/call" => {
                let caller = Caller {
                    actor: &agent.agent_id,
                    via: "mcp",
                    session,
                    mcp_session,
                };
                self.tool(
                    agent,
                    &caller,
                    &text(params, "tool")?,
                    params["input"].clone(),
                )
                .await
            }
            "mcp/resources" => {
                // Without discourse.read an agent is shown no rooms.
                let rooms = match self.read(agent, session, "agent-protocols://rooms").await {
                    Err(Error::Forbidden) => Value::Null,
                    rooms => rooms?,
                };
                Ok(json!({"resources": resource_list(&rooms), "templates": resource_templates()}))
            }
            "mcp/read" => {
                let uri = text(params, "uri")?;
                Ok(json!({"uri": uri, "value": self.read(agent, session, &uri).await?}))
            }
            "mcp/changed" => {
                let after = params["after"].as_u64();
                let wait = params["wait_ms"].as_u64().unwrap_or(0).min(CHANGED_WAIT_MS);
                let mut changed = self.connectors().subscribe();
                if after == Some(*changed.borrow_and_update()) && wait > 0 {
                    // A stop wakes waiting readers through the events signal.
                    tokio::select! {
                        _ = changed.changed() => {}
                        _ = self.mcp().arrived.notified() => {}
                        _ = tokio::time::sleep(Duration::from_millis(wait)) => {}
                    }
                }
                let revision = *changed.borrow();
                Ok(json!({"revision": revision}))
            }
            _ => Err(Error::Forbidden),
        }
    }

    /// One resource of `agent`'s connector as `session` sees it, within
    /// the grant the tool behind it needs.
    async fn read(&self, agent: &AgentEntry, session: &str, uri: &str) -> Result<Value> {
        let tool = Resource::parse(uri).map(|r| r.tool());
        let (path, signer, reach) = {
            let mut client = self.client().lock().await;
            (
                client.agent_runtime()?,
                client.agent_signer(&agent.agent_id)?,
                match tool {
                    Some(tool) => self.reach(&client, agent, tool)?,
                    None => None,
                },
            )
        };
        let scope = Scope {
            path: &path,
            session,
            reach: reach.as_deref(),
        };
        self.agents().read(scope, signer, uri).await
    }

    /// The origins `agent` may reach with standard tool `tool`, those its
    /// grant names; `None` for a tool that needs no grant, which reaches
    /// every configured host.
    fn reach(
        &self,
        client: &Client,
        agent: &AgentEntry,
        tool: &str,
    ) -> Result<Option<Vec<String>>> {
        let Some(name) = standard_capability(tool) else {
            return Ok(None);
        };
        let origins = self.agents().origins(name);
        let granted = client.capability_targets(&agent.agent_id, name, origins)?;
        ensure(!granted.is_empty(), Error::Forbidden)?;
        Ok(Some(granted))
    }

    /// One tool call as `agent` for `caller`: `{"result": …}` or
    /// `{"error": {code, message, data?}}`, entered in the audit record
    /// unless it only polls or reads the agent's own connector.
    async fn tool(
        &self,
        agent: &AgentEntry,
        caller: &Caller<'_>,
        tool: &str,
        input: Value,
    ) -> Result<Value> {
        let target = TARGETS
            .iter()
            .find_map(|k| input[k].as_str())
            .map(str::to_string);
        let reply = self.run_tool(agent, caller, tool, input).await;
        let audited = !matches!(tool, "dmsg_events_next" | "dmsg_approval_status")
            && (is_dmsg(tool) || (is_standard(tool) && !is_read_only(tool)));
        if audited {
            let outcome = match &reply {
                Ok(value) => value["error"]["code"].as_str().unwrap_or("ok").to_string(),
                Err(error) => error_code(error).to_string(),
            };
            let target = target.or_else(|| {
                let reply = reply.as_ref().ok()?;
                reply["result"]["id"].as_str().map(str::to_string)
            });
            let target: Option<String> = target.map(|t| t.chars().take(200).collect());
            // The owner acting as an Agent ID names it.
            let detail = (caller.actor != agent.agent_id).then_some(agent.agent_id.as_str());
            self.client().lock().await.audit(
                caller.actor,
                caller.via,
                tool,
                target.as_deref(),
                detail,
                &outcome,
            );
        }
        reply
    }

    async fn run_tool(
        &self,
        agent: &AgentEntry,
        caller: &Caller<'_>,
        tool: &str,
        input: Value,
    ) -> Result<Value> {
        if is_standard(tool) {
            // The Agent ID's turn spans reading the mail keys to saving what
            // the call left, so no other call of it sees or overwrites them
            // in between; the keys leave the client lock for the call only.
            let runtime = self.agents();
            let turn = runtime.turn(&agent.agent_id).await;
            let (reach, path, signer, mail_keys) = {
                let mut client = self.client().lock().await;
                // An agent reaches what its grant names; the owner acting as
                // one reaches every configured host.
                let reach = match caller.actor {
                    "owner" => None,
                    _ => match self.reach(&client, agent, tool) {
                        Ok(reach) => reach,
                        Err(error) => return Ok(refused(&error)),
                    },
                };
                (
                    reach,
                    client.agent_runtime()?,
                    client.agent_signer(&agent.agent_id)?,
                    client.agent_mail_keys(&agent.agent_id)?,
                )
            };
            let scope = Scope {
                path: &path,
                session: caller.session,
                reach: reach.as_deref(),
            };
            let called = runtime
                .call(&turn, scope, signer, &mail_keys, tool, input)
                .await?;
            if called.changed {
                self.connectors().send_modify(|revision| *revision += 1);
            }
            if called.mail_keys != mail_keys {
                self.client()
                    .lock()
                    .await
                    .agent_mail_keys_save(&agent.agent_id, &called.mail_keys)?;
            }
            return Ok(match called.outcome {
                Ok(result) => json!({"result": result}),
                Err(error) => json!({"error": error}),
            });
        }
        if !is_dmsg(tool) {
            return Err(Error::NotFound);
        }
        let outcome = match tool.starts_with("dmsg_events_") {
            true => {
                self.events_tool(agent, caller.mcp_session, tool, &input)
                    .await
            }
            false => self.dmsg_tool(agent, tool, &input).await,
        };
        Ok(match outcome {
            Ok(result) => json!({"result": result}),
            Err(error) => refused(&error),
        })
    }

    /// A `dmsg_*` tool, after the capability it needs.
    async fn dmsg_tool(&self, agent: &AgentEntry, tool: &str, input: &Value) -> Result<Value> {
        let did = agent.agent_id.as_str();
        let channel = || text(input, "channel");
        let granted =
            async |capability: &str| self.client().lock().await.capability_check(did, capability);
        let wallet = || dmsg_client::wallet::parse_wallet(&text(input, "wallet")?);
        let proposal = || {
            input["proposal"]
                .as_u64()
                .ok_or_else(|| Error::InvalidInput("proposal".into()))
        };
        // These wait on the relay and the IC without the client, as the
        // owner's methods do.
        let h = self.hold();
        match tool {
            "dmsg_channel_publish" => {
                let id = channel()?;
                granted(&format!("channel.publish:{id}")).await?;
                // Members see which Agent ID wrote it through this device.
                let draft = dmsg_client::channel::Draft {
                    text: text(input, "text")?,
                    agent: Some(dmsg_client::channel::AgentRef {
                        id: agent.agent_id.clone(),
                        name: agent.label.clone(),
                    }),
                    ..Default::default()
                };
                return dmsg_client::channel::send(&h, &id, &draft, None).await;
            }
            "dmsg_inbox_list" => {
                granted("inbox.read").await?;
                let letters = dmsg_client::inbox::list(&h).await?;
                return Ok(Value::Array(
                    letters
                        .iter()
                        .map(|l| {
                            json!({
                                "order_id": l.order_id,
                                "sender": l.sender,
                                "state": l.state,
                                "created_at": l.created_at,
                                "read": l.read,
                                "archived": l.archived,
                                "abort_reason": l.abort_reason,
                            })
                        })
                        .collect(),
                ));
            }
            "dmsg_inbox_read" => {
                granted("inbox.read").await?;
                let order = text(input, "order_id")?;
                let letter = dmsg_client::inbox::list(&h)
                    .await?
                    .into_iter()
                    .find(|l| l.order_id == order)
                    .ok_or(Error::NotFound)?;
                let mark = input["mark_read"] == true && !letter.read;
                if mark {
                    // A mark still goes to the relay under the client.
                    self.under_lock(async |client| {
                        client.inbox_mark(&order, true, letter.archived).await
                    })
                    .await?;
                }
                return Ok(json!({
                    "order_id": letter.order_id,
                    "sender": letter.sender,
                    "state": letter.state,
                    "text": letter.text,
                    "created_at": letter.created_at,
                    "read": letter.read || mark,
                    "archived": letter.archived,
                    "abort_reason": letter.abort_reason,
                }));
            }
            "dmsg_profile_read" => {
                granted("profile.read").await?;
                return Ok(match input["account"].as_str() {
                    Some(account) => {
                        let account = account
                            .parse()
                            .map_err(|_| Error::InvalidInput("account".into()))?;
                        to_json(&dmsg_client::profile::resolve_account(&h, &account).await?)
                    }
                    None => to_json(&dmsg_client::profile::show(&h).await?),
                });
            }
            "dmsg_wallet_list" => {
                granted("wallet.read").await?;
                let list = dmsg_client::wallet::list(&h).await?;
                let mut wallets = vec![];
                for w in list.as_array().cloned().unwrap_or_default() {
                    let id = dmsg_client::wallet::parse_wallet(
                        w["wallet"].as_str().unwrap_or_default(),
                    )?;
                    wallets.push(dmsg_client::wallet::show(&h, id).await?);
                }
                return Ok(Value::Array(wallets));
            }
            "dmsg_wallet_balances" => {
                granted("wallet.read").await?;
                return dmsg_client::wallet::balances(&h, wallet()?).await;
            }
            "dmsg_wallet_proposal_status" => {
                granted("wallet.read").await?;
                return dmsg_client::wallet::proposal(&h, wallet()?, proposal()?).await;
            }
            "dmsg_action_request" => {
                granted("action.request").await?;
                let action = unb64(&text(input, "action")?)?;
                let via = format!("agent:{did}");
                let request = dmsg_client::attest::action_request(&h, &via, &action).await?;
                self.emit("approval/changed", Value::Null);
                return Ok(request);
            }
            _ => {}
        }
        // These still wait on the relay and the IC under the client:
        // contacts, delegations and the wallet's proposals and executions.
        let networked = matches!(
            tool,
            "dmsg_inbox_contact"
                | "dmsg_delegations_list"
                | "dmsg_wallet_propose_transfer"
                | "dmsg_wallet_execute"
                | "dmsg_wallet_report_broadcast"
        );
        if networked {
            let via = format!("agent:{did}");
            let result = self
                .under_lock(async |client| match tool {
                    "dmsg_inbox_contact" => {
                        client.capability_check(did, "inbox.contact")?;
                        let to = text(input, "to")?
                            .parse()
                            .map_err(|_| Error::InvalidInput("to".into()))?;
                        // Never paid: a quote comes back and nothing is sent.
                        client
                            .inbox_send(&to, &text(input, "text")?, None, None, false)
                            .await
                    }
                    "dmsg_delegations_list" => {
                        client.capability_check(did, "agent.delegation.read")?;
                        Ok(Value::Array(client.delegation_credentials().await?))
                    }
                    "dmsg_wallet_propose_transfer" => {
                        let id = text(input, "wallet")?;
                        client.capability_check(did, &format!("wallet.propose:{id}"))?;
                        let from = input["from"]
                            .as_u64()
                            .and_then(|f| u32::try_from(f).ok())
                            .ok_or_else(|| Error::InvalidInput("from".into()))?;
                        client
                            .wallet_send(
                                wallet()?,
                                from,
                                &text(input, "asset")?,
                                &text(input, "to")?,
                                &text(input, "amount")?,
                                &text(input, "max_fee")?,
                                input["memo"].as_str(),
                                Some(did.to_string()),
                            )
                            .await
                    }
                    "dmsg_wallet_execute" => {
                        let id = text(input, "wallet")?;
                        client.capability_check(did, &format!("wallet.execute:{id}"))?;
                        client.wallet_execute(&via, wallet()?, proposal()?).await
                    }
                    "dmsg_wallet_report_broadcast" => {
                        let id = text(input, "wallet")?;
                        client.capability_check(did, &format!("wallet.execute:{id}"))?;
                        match input["rebroadcast"] == true {
                            true => client.wallet_broadcast(wallet()?, proposal()?, None).await,
                            false => client.wallet_settle(wallet()?, proposal()?).await,
                        }
                    }
                    _ => Err(Error::NotFound),
                })
                .await?;
            if tool == "dmsg_wallet_execute" {
                self.emit("approval/changed", Value::Null);
            }
            if tool.starts_with("dmsg_wallet_") {
                self.emit("wallet/changed", Value::Null);
            }
            return Ok(result);
        }
        // The client alone, without the network.
        let mut client = self.client().lock().await;
        match tool {
            "dmsg_account_status" => {
                client.capability_check(did, "account.read")?;
                Ok(to_json(&client.status()?))
            }
            "dmsg_vault_list" => {
                client.capability_check(did, "vault.list")?;
                let filter = Filter {
                    trash: false,
                    query: input["query"].as_str().map(str::to_string),
                };
                // dMsg's own keys stay out of every agent's reach.
                Ok(Value::Array(
                    client
                        .vault_list(&filter)?
                        .into_iter()
                        .filter(|e| !e.item.reserved())
                        .map(|e| {
                            json!({
                                "id": e.id,
                                "type": e.item.kind,
                                "title": e.item.title,
                                "tags": e.item.tags,
                                "updated_at": e.item.updated_at,
                            })
                        })
                        .collect(),
                ))
            }
            "dmsg_vault_read" => {
                let entry = client.vault_show(&text(input, "id")?)?;
                ensure(!entry.deleted, Error::NotFound)?;
                // Never dMsg's own keys, not even with `vault.read:*`.
                ensure(!entry.item.reserved(), Error::Forbidden)?;
                tagged(&client, did, "vault.read", &entry.item)?;
                // Hidden values only with a grant naming the item.
                let reveal = input["secret"] == true;
                if !reveal {
                    return Ok(to_json(&entry.masked(false)));
                }
                client.capability_check(did, &format!("vault.secret:{}", entry.id))?;
                let now = dmsg_client::now_ms();
                let codes: serde_json::Map<String, Value> = entry
                    .item
                    .fields
                    .iter()
                    .filter(|f| f.kind == "otp" && !f.value.is_empty())
                    .filter_map(|f| {
                        let code = otp::parse(&f.value).ok()?.code(now).code;
                        Some((f.id.clone(), code.into()))
                    })
                    .collect();
                let mut read = to_json(&entry);
                read["codes"] = codes.into();
                Ok(read)
            }
            "dmsg_vault_write" => {
                let id = input["id"].as_str();
                let changes = Changes {
                    title: optional(input, "title", text)?,
                    notes: optional(input, "notes", text)?,
                    tags: optional(input, "tags", parse)?,
                    set: optional(input, "set", parse)?,
                    remove: optional(input, "remove", parse)?,
                    ..Changes::default()
                };
                let kind = input["type"].as_str().unwrap_or("note");
                let mut item = match id {
                    Some(id) => {
                        let entry = client.vault_show(id)?;
                        ensure(!entry.deleted, Error::NotFound)?;
                        // The edit applies to `base`: the grants must reach
                        // the version it writes.
                        let entry = match input["base"].as_str() {
                            Some(base) => client.vault_version(id, base)?,
                            None => entry,
                        };
                        ensure(!entry.item.reserved(), Error::Forbidden)?;
                        tagged(&client, did, "vault.write", &entry.item)?;
                        entry.item
                    }
                    None => Item::new(kind, "", dmsg_client::now_ms()),
                };
                changes.apply(&mut item);
                item.normalize();
                ensure(!item.reserved(), Error::Forbidden)?;
                tagged(&client, did, "vault.write", &item)?;
                let entry = match id {
                    Some(id) => client.vault_edit(id, input["base"].as_str(), &changes)?,
                    None => client.vault_create(kind, &changes)?,
                };
                // Writing is not reading: the item comes back only to an
                // agent that may read it.
                match tagged(&client, did, "vault.read", &entry.item) {
                    Ok(()) => Ok(to_json(&entry.masked(false))),
                    Err(_) => Ok(json!({
                        "id": entry.id,
                        "revision": entry.revision,
                        "synced": entry.synced,
                        "conflict": entry.conflict,
                        "deleted": entry.deleted,
                    })),
                }
            }
            "dmsg_files_list" => {
                let folder = input["folder"].as_str().unwrap_or(ROOT).to_string();
                client.files_grant_check(did, "files.read", &folder)?;
                let listing = client.files_list(&folder, false)?;
                Ok(json!({
                    "folder": folder,
                    "folders": listing.folders.iter().map(|f| json!({"id": f.id, "name": f.name})).collect::<Vec<_>>(),
                    "files": listing.files.iter().map(file_summary).collect::<Vec<_>>(),
                }))
            }
            "dmsg_files_read" => {
                let id = text(input, "id")?;
                let file = client.files_show(&id)?.file;
                ensure(file.trashed_at.is_none(), Error::NotFound)?;
                client.files_grant_check(did, "files.read", &file.folder)?;
                let length = input["length"]
                    .as_u64()
                    .unwrap_or(MAX_READ)
                    .clamp(1, MAX_READ);
                let (manifest, bytes) = client.files_read_range(
                    &id,
                    input["version"].as_str(),
                    input["offset"].as_u64().unwrap_or(0),
                    length,
                )?;
                let offset = input["offset"].as_u64().unwrap_or(0).min(manifest.size);
                let end = offset + bytes.len() as u64;
                Ok(json!({
                    "id": manifest.id,
                    "version": manifest.version,
                    "name": manifest.name,
                    "mime": manifest.mime,
                    "size": manifest.size,
                    "sha256": manifest.sha256,
                    "offset": offset,
                    "length": bytes.len(),
                    "eof": end == manifest.size,
                    "content_base64url": b64(&bytes),
                }))
            }
            "dmsg_files_write" => {
                let content = unb64(&text(input, "content_base64url")?)?;
                ensure(content.len() <= MAX_AGENT_WRITE, Error::QuotaExceeded)?;
                let (folder, name) = match input["id"].as_str() {
                    Some(id) => {
                        let file = client.files_show(id)?.file;
                        ensure(file.trashed_at.is_none(), Error::NotFound)?;
                        (file.folder, file.name)
                    }
                    None => (
                        input["folder"].as_str().unwrap_or(ROOT).to_string(),
                        text(input, "name")?,
                    ),
                };
                client.files_grant_check(did, "files.write", &folder)?;
                let imported = client.files_write(&folder, &name, &content)?;
                Ok(json!({
                    "id": imported.file.id,
                    "version": imported.version,
                    "name": imported.file.name,
                    "size": imported.file.size,
                    "sha256": imported.file.sha256,
                    "versions": imported.file.versions,
                }))
            }
            "dmsg_files_search" => {
                let found = client.files_search(
                    &text(input, "query")?,
                    &dmsg_client::library::SearchFilter::default(),
                )?;
                Ok(Value::Array(
                    found
                        .iter()
                        .filter(|f| {
                            client
                                .files_grant_check(did, "files.read", &f.folder)
                                .is_ok()
                        })
                        .map(file_summary)
                        .collect(),
                ))
            }
            "dmsg_channel_list" => {
                let channels = client.channel_list()?;
                Ok(Value::Array(
                    channels
                        .iter()
                        .map(to_json)
                        .filter(|c| {
                            let id = c["id"].as_str().unwrap_or_default();
                            client
                                .capability_check(did, &format!("channel.read:{id}"))
                                .is_ok()
                        })
                        .collect(),
                ))
            }
            "dmsg_channel_read" => {
                let id = channel()?;
                client.capability_check(did, &format!("channel.read:{id}"))?;
                let after = input["after"].as_u64().unwrap_or(0);
                Ok(to_json(&client.channel_messages(&id, after, 200)?))
            }
            "dmsg_attest_request" => {
                client.capability_check(did, "attest.request")?;
                let statement = StatementInput::Text {
                    text: text(input, "text")?,
                    subject: input["title"].as_str().map(str::to_string),
                };
                let request = client.attest_request(&format!("agent:{did}"), statement, None)?;
                drop(client);
                self.emit("approval/changed", Value::Null);
                Ok(request)
            }
            "dmsg_approval_status" => {
                let request = client.approval(&text(input, "id")?)?;
                // An agent sees only the requests it made.
                if request["source"] != format!("agent:{did}") {
                    return Err(Error::NotFound);
                }
                Ok(
                    json!({"id": request["id"], "state": request["state"], "error": request["error"]}),
                )
            }
            _ => Err(Error::NotFound),
        }
    }
}

/// A library file as an agent sees it: no key, no chunks.
fn file_summary(file: &dmsg_client::library::catalog::FileView) -> Value {
    json!({
        "id": file.id,
        "folder": file.folder,
        "name": file.name,
        "mime": file.mime,
        "size": file.size,
        "sha256": file.sha256,
        "tags": file.tags,
        "note": file.note,
        "updated_at": file.updated_at,
        "versions": file.versions,
        "path": file.path,
    })
}

/// Whether `did` holds `<name>:<tag>` for one of the item's tags, or
/// `<name>:*`.
fn tagged(client: &Client, did: &str, name: &str, item: &Item) -> Result<()> {
    let granted = item
        .tags
        .iter()
        .any(|t| client.capability_check(did, &format!("{name}:{t}")).is_ok());
    match granted {
        true => Ok(()),
        false => client.capability_check(did, &format!("{name}:*")),
    }
}

/// The newest current controller.
async fn latest_controller(client: &mut dmsg_client::Client) -> Result<u32> {
    let info = client.principal().await?.ok_or(Error::NotFound)?;
    info.state
        .controllers
        .iter()
        .filter(|c| c.retired_at.is_none())
        .map(|c| c.generation)
        .max()
        .ok_or(Error::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_standard_capability_is_granted_per_origin() {
        use dmsg_client::agents::{HOST_CAPABILITIES, validate_capability};
        for tool in tool_list() {
            if let Some(name) = standard_capability(tool["name"].as_str().unwrap()) {
                assert!(HOST_CAPABILITIES.contains(&name), "{name}");
                validate_capability(&format!("{name}:*")).unwrap();
                validate_capability(&format!("{name}:https://discourse.dmsg.net")).unwrap();
            }
        }
    }
}
