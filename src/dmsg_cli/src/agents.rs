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
//! data, with who called, through which way in and on what. A paused Agent
//! ID's credential reaches nothing; an agent may ask for capabilities, which
//! only the owner's approval grants.

use crate::daemon::Daemon;
use crate::params::{number, optional, parse, source, text, to_json};
use dmsg_agent::resources::{Resource, resource_list, resource_templates};
use dmsg_agent::tools::{is_dmsg, is_read_only, is_standard, standard_capability, tool_list};
use dmsg_agent::{DurableMail, Scope};
use dmsg_client::{
    Client,
    agent::PrincipalChange,
    agents::AgentEntry,
    attest::StatementInput,
    codec::{b64, unb64},
    error_code,
    hold::Hold,
    library::catalog::ROOT,
    vault::{Changes, Filter, Item},
};
use dmsg_types::{
    Error, Result,
    agent::{DelegationAuthority, PrincipalType},
    ensure,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// Receive for hosted agents with a live Mail grant. The connector's own
/// turn serializes this with foreground calls; different agents progress
/// independently, without holding the account's client across the network.
pub(crate) async fn run_mail(daemon: Arc<Daemon>) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = interval.tick() => {},
            _ = daemon.mail_push().receive.notified() => {},
        }
        if daemon.stopping() {
            continue;
        }
        let agents = daemon.client().lock().await.agent_list();
        let Ok(agents) = agents else { continue };
        let mut pending = tokio::task::JoinSet::new();
        for agent in agents["agents"].as_array().into_iter().flatten() {
            let Some(id) = agent["agent_id"].as_str() else {
                continue;
            };
            let id = id.to_owned();
            let daemon = daemon.clone();
            pending.spawn(async move { daemon.receive_agent_mail(&id).await });
        }
        while let Some(result) = pending.join_next().await {
            if let Ok(Err(error)) = result
                && !matches!(error, Error::Locked | Error::Forbidden | Error::NotFound)
            {
                daemon.note_result::<()>("agent-mail", &Err(error));
            }
        }
        // Coalesce bursts of hints without allowing a busy webhook source
        // to turn into an unbounded mailbox poll loop.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

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
pub(crate) const METHODS: [&str; 29] = [
    "agent/mail_push",
    "agent/mail_push_set",
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
    "agent/credential_revoke",
    "agent/pause",
    "agent/remove",
    "agent/profile",
    "grants/list",
    "grants/add",
    "grants/revoke",
    "ap/call",
    "ap/read",
    "audit/list",
];

/// The only methods an agent's credential reaches.
pub(crate) const AGENT_METHODS: [&str; 6] = [
    "mcp/tools",
    "mcp/call",
    "mcp/resources",
    "mcp/read",
    "mcp/changed",
    "mcp/mail_events",
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

/// The events a method's success announces. A tool call announces only
/// what it entered in the audit record (`audit/changed`), so an agent that
/// polls does not have every window read the principal again.
pub(crate) fn events_of(method: &str) -> &'static [&'static str] {
    match method {
        "agent/enable" | "agent/register" | "agent/retire" | "agent/compromise"
        | "agent/rename" | "agent/grant" | "agent/revoke" => &["approval/changed"],
        "agent/publish"
        | "agent/refresh"
        | "agent/create"
        | "agent/use"
        | "agent/resident"
        | "agent/credential"
        | "agent/credential_revoke"
        | "agent/pause"
        | "agent/remove"
        | "grants/add"
        | "grants/revoke" => &["agent/changed"],
        _ => &[],
    }
}

/// A tool's refusal in the shape tools return it.
fn refused(error: &Error) -> Value {
    json!({"error": {"code": error_code(error), "message": error.to_string()}})
}

impl Daemon {
    /// The same scope and pause checks as an agent's own Mail call. A
    /// locked device can receive only for an explicitly resident agent.
    async fn receive_agent_mail(&self, id: &str) -> Result<Value> {
        let agent = self.client().lock().await.agent_find(id)?;
        ensure(!agent.paused, Error::Forbidden)?;
        let caller = Caller {
            actor: &agent.agent_id,
            via: "background",
            session: "mail-background",
            mcp_session: None,
        };
        self.run_tool(&agent, &caller, "agent_protocols_mail_sync", json!({}))
            .await
    }

    /// Run one of the owner's [`METHODS`].
    pub(crate) async fn call_agents(&self, method: &str, params: &Value) -> Result<Value> {
        if method == "agent/credentials" {
            return Ok(Value::Array(
                dmsg_client::agent::delegation_credentials(&self.hold()).await?,
            ));
        }
        if method == "agent/mail_push_set" {
            return self
                .configure_mail_push(
                    &text(params, "agent")?,
                    params["enabled"]
                        .as_bool()
                        .ok_or_else(|| Error::InvalidInput("enabled".into()))?,
                    source(params)?,
                )
                .await;
        }
        if method == "agent/mail_push" {
            return self
                .hold()
                .with(|c| c.mail_push_view(&text(params, "agent")?))
                .await;
        }
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
        if method == "ap/read" {
            let agent = {
                let client = self.client().lock().await;
                match params["agent"].as_str() {
                    Some(name) => client.agent_find(name)?,
                    None => client.agent_active()?,
                }
            };
            // The owner reads what the connector holds, from every host.
            let session = params["session"].as_str().unwrap_or(source(params)?);
            let uri = text(params, "uri")?;
            let value = self.read(&agent, session, &uri, None).await?;
            return Ok(json!({"uri": uri, "value": value}));
        }
        if method == "agent/profile" {
            let h = self.hold();
            let (agent, path) = h
                .with(|client| {
                    Ok((
                        client.agent_find(&text(params, "agent")?)?,
                        client.agent_runtime()?,
                    ))
                })
                .await?;
            let profiles = self.agents().profiles(&path, &agent.agent_id).await?;
            h.with(|_| Ok(())).await?;
            return Ok(json!({"agent_id": agent.agent_id, "profiles": profiles}));
        }
        if method == "agent/remove" {
            let via = source(params)?;
            let agent = self
                .client()
                .lock()
                .await
                .agent_find(&text(params, "agent")?)?;
            // No call of it runs while its state goes.
            let turn = self.agents().turn(&agent.agent_id).await;
            let (removed, path) = {
                let mut client = self.client().lock().await;
                (
                    client.agent_remove(via, &agent.agent_id)?,
                    client.agent_runtime()?,
                )
            };
            self.agents().forget(&turn, &path)?;
            return Ok(removed.view());
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
                "agent/ids" => Some((|| {
                    let mut list = client.agent_list()?;
                    list["hosts"] = to_json(self.agents().hosts());
                    list["blocked_senders"] = to_json(&self.agents().blocked_senders());
                    list["vocabulary"] = to_json(&dmsg_client::agents::vocabulary());
                    // The connector sessions each one was last used in, as far
                    // as the state file tells them.
                    if let Ok(path) = client.agent_runtime()
                        && let Ok(mut sessions) = self.agents().sessions(&path)
                        && let Some(agents) = list["agents"].as_array_mut()
                    {
                        for agent in agents {
                            let id = agent["agent_id"].as_str().unwrap_or_default();
                            agent["sessions"] = to_json(&sessions.remove(id).unwrap_or_default());
                        }
                    }
                    Ok(list)
                })()),
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
                "agent/credential_revoke" => Some((|| {
                    let entry = client.agent_credential_revoke(&text(params, "agent")?)?;
                    let via = source(params)?;
                    client.audit("owner", via, method, Some(&entry.agent_id), None, "ok");
                    Ok(entry.view())
                })()),
                "agent/pause" => Some((|| {
                    let on = params["on"]
                        .as_bool()
                        .ok_or_else(|| Error::InvalidInput("on".into()))?;
                    let entry = client.agent_pause(&text(params, "agent")?, on)?;
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
                "grants/list" => Some(client.capability_grants().map(|g| to_json(&g))),
                "grants/add" => Some((|| {
                    let grant = client.capability_grant(
                        source(params)?,
                        &text(params, "agent")?,
                        parse(params, "capabilities")?,
                        optional(params, "days", number)?,
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
        let h = self.hold();
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
                return dmsg_client::agent::controller_request(
                    &h,
                    source(params)?,
                    params["name"].as_str().map(str::to_string),
                    controller_authority(params)?,
                )
                .await;
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
            return dmsg_client::agent::principal_request(&h, source(params)?, change).await;
        }
        match method {
            "agent/principal" => dmsg_client::agent::principal_view(&h).await,
            "agent/publish" => {
                Ok(json!({"published_version": dmsg_client::agent::principal_publish(&h).await?}))
            }
            "agent/refresh" => dmsg_client::agent::principal_refresh(&h).await,
            "agent/grant" => {
                let h = dmsg_client::hold::Unlocked::new(&h).await?;
                let generation = match params["generation"].is_null() {
                    true => None,
                    false => Some(number(params, "generation")?),
                };
                // The subject may name a hosted Agent ID by its label.
                let subject = text(params, "subject")?;
                let subject = h
                    .with(|c| Ok(c.agent_find(&subject).map_or(subject, |a| a.agent_id)))
                    .await?;
                dmsg_client::agent::delegation_grant_request(
                    &h,
                    source(params)?,
                    generation,
                    &subject,
                    parse(params, "scopes")?,
                    parse(params, "audiences")?,
                    params["days"].as_u64().unwrap_or(30),
                    params["relationship"].as_str().map(str::to_string),
                    params["replaces"].as_str(),
                )
                .await
            }
            "agent/revoke" => {
                let generation = match params["generation"].is_null() {
                    true => None,
                    false => Some(number(params, "generation")?),
                };
                dmsg_client::agent::delegation_revoke_request(
                    &h,
                    source(params)?,
                    generation,
                    &text(params, "id")?,
                    params["reason"].as_str().map(str::to_string),
                )
                .await
            }
            _ => Err(Error::NotFound),
        }
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
        // The entry as it is now: the owner may have paused it since.
        let paused = self
            .client()
            .lock()
            .await
            .agent_find(&agent.agent_id)?
            .paused;
        ensure(!paused, Error::Forbidden)?;
        match method {
            "mcp/mail_events" => {
                self.mail_events_next(&agent.agent_id, &params["input"])
                    .await
            }
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
                let rooms = match self
                    .read(agent, session, "agent-protocols://rooms", Some(agent))
                    .await
                {
                    Err(Error::Forbidden) => Value::Null,
                    rooms => rooms?,
                };
                Ok(json!({"resources": resource_list(&rooms), "templates": resource_templates()}))
            }
            "mcp/read" => {
                let uri = text(params, "uri")?;
                Ok(
                    json!({"uri": uri, "value": self.read(agent, session, &uri, Some(agent)).await?}),
                )
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

    /// Metadata-only Mail events, authorized again after each wait. The
    /// cursor belongs to one Agent and is independent of relay-event cursors.
    async fn mail_events_next(&self, agent: &str, input: &Value) -> Result<Value> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            mailbox_id: Option<String>,
            cursor: Option<String>,
            max: Option<usize>,
            wait_ms: Option<u64>,
            max_age_ms: Option<u64>,
        }
        let args: Input = serde_json::from_value(input.clone())
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let h = self.hold();
        let mut changed = self.connectors().subscribe();
        let deadline = tokio::time::Instant::now()
            + Duration::from_millis(args.wait_ms.unwrap_or(0).min(CHANGED_WAIT_MS));
        loop {
            changed.borrow_and_update();
            let mut page = h
                .with(|client| {
                    let entry = client.agent_find(agent)?;
                    ensure(!entry.paused, Error::Forbidden)?;
                    let reach = self
                        .reach(client, &entry, "agent_protocols_mail_list", None)?
                        .unwrap_or_default();
                    let now = dmsg_client::now_ms().min(i64::MAX as u64) as i64;
                    dmsg_agent::mail_events::page(
                        &client.agent_runtime()?,
                        dmsg_agent::mail_events::Query {
                            agent,
                            reach: &reach,
                            mailbox: args.mailbox_id.as_deref(),
                            cursor: args.cursor.as_deref(),
                            limit: args.max.unwrap_or(100).min(100),
                            since: args
                                .max_age_ms
                                .map(|age| now.saturating_sub(age.min(i64::MAX as u64) as i64)),
                            now,
                        },
                    )
                })
                .await?;
            if let Some(events) = page["events"].as_array_mut() {
                for event in events {
                    let at = event["received_at"]
                        .as_i64()
                        .ok_or(Error::IntegrityFailed)?;
                    event["timestamp"] = chrono::DateTime::from_timestamp_millis(at)
                        .ok_or(Error::IntegrityFailed)?
                        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                        .into();
                    event.as_object_mut().expect("event").remove("received_at");
                }
            }
            if args.cursor.is_none()
                || page["events"].as_array().is_some_and(|v| !v.is_empty())
                || page["truncated"] == true
                || tokio::time::Instant::now() >= deadline
            {
                return Ok(page);
            }
            tokio::select! {
                _ = changed.changed() => {},
                _ = tokio::time::sleep_until(deadline) => {},
            }
        }
    }

    /// Read the latest committed keys, including a pending crash-recovery checkpoint.
    fn mail_keys_for_read(
        &self,
        client: &mut Client,
        path: &std::path::Path,
        agent: &str,
    ) -> Result<Value> {
        match self.agents().mail_checkpoint(path, agent)? {
            Some(sealed) => client.agent_mail_codec(agent)?.open(&sealed),
            None => client.agent_mail_keys(agent),
        }
    }

    /// One resource of `agent`'s connector as `session` sees it: for an
    /// agent `caller`, within the grant the tool behind it needs; for the
    /// owner, from every host.
    async fn read(
        &self,
        agent: &AgentEntry,
        session: &str,
        uri: &str,
        caller: Option<&AgentEntry>,
    ) -> Result<Value> {
        let tool = Resource::parse(uri).map(|r| r.tool());
        let (path, signer, reach, keys) = {
            let mut client = self.client().lock().await;
            let path = client.agent_runtime()?;
            let keys = if matches!(Resource::parse(uri), Some(Resource::Mailboxes)) {
                Some(self.mail_keys_for_read(&mut client, &path, &agent.agent_id)?)
            } else {
                None
            };
            (
                path,
                client.agent_signer(&agent.agent_id)?,
                match (tool, caller) {
                    (Some(tool), Some(caller)) => self.reach(&client, caller, tool, None)?,
                    _ => None,
                },
                keys,
            )
        };
        let scope = Scope {
            path: &path,
            session,
            reach: reach.as_deref(),
        };
        self.agents().read(scope, signer, uri, keys.as_ref()).await
    }

    /// The origins `agent` may reach with standard tool `tool`, those its
    /// grant names; `None` for a tool that needs no grant, which reaches
    /// every configured host.
    fn reach(
        &self,
        client: &Client,
        agent: &AgentEntry,
        tool: &str,
        input: Option<&Value>,
    ) -> Result<Option<Vec<String>>> {
        // Bundled schemas expose no room state. A room-specific lookup uses
        // the same discourse.read origin gate as the room's other readers.
        if tool == "agent_protocols_schema_get" && input.is_some_and(|v| v.get("room_id").is_none())
        {
            return Ok(None);
        }
        let Some(name) = standard_capability(tool) else {
            return Ok(None);
        };
        // Resolving mailbox cards may also read Profile hints. Only this
        // tool considers those extra origins, still under mail.read grants.
        let origins = self.agents().origins(match tool {
            "agent_protocols_mail_resolve" => "agent.profile.read",
            _ => name,
        });
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
            self.emit("audit/changed", Value::Null);
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
        if dmsg_agent::tools::is_mail_inspection(tool, &input) {
            let h = self.hold();
            let (reach, path, signer, keys) = h
                .with(|client| {
                    let reach = match caller.actor {
                        "owner" => None,
                        _ => {
                            ensure(
                                !client.agent_find(&agent.agent_id)?.paused,
                                Error::Forbidden,
                            )?;
                            self.reach(client, agent, tool, Some(&input))?
                        }
                    };
                    let path = client.agent_runtime()?;
                    let keys = if tool == "agent_protocols_mail_mailboxes_list" {
                        Some(self.mail_keys_for_read(client, &path, &agent.agent_id)?)
                    } else {
                        None
                    };
                    Ok((reach, path, client.agent_signer(&agent.agent_id)?, keys))
                })
                .await?;
            let scope = Scope {
                path: &path,
                session: caller.session,
                reach: reach.as_deref(),
            };
            let outcome = self
                .agents()
                .inspect_mail(scope, signer, tool, input, keys.as_ref())
                .await?;
            h.with(|_| Ok(())).await?;
            return Ok(match outcome {
                Ok(result) => json!({"result": result}),
                Err(error) => json!({"error": error}),
            });
        }
        if is_standard(tool) {
            // The Agent ID's turn spans reading the mail keys to saving what
            // the call left, so no other call of it sees or overwrites them
            // in between; the keys leave the client lock for the call only.
            let runtime = self.agents();
            let h = self.hold();
            h.with(|_| Ok(())).await?;
            let turn = runtime.turn(&agent.agent_id).await;
            let (reach, path, signer, mail_keys, codec) = h
                .with(|client| {
                    // An agent reaches what its grant names; the owner acting as
                    // one reaches every configured host.
                    let reach = match caller.actor {
                        "owner" => None,
                        _ => {
                            ensure(
                                !client.agent_find(&agent.agent_id)?.paused,
                                Error::Forbidden,
                            )?;
                            self.reach(client, agent, tool, Some(&input))?
                        }
                    };
                    let path = client.agent_runtime()?;
                    let codec = client.agent_mail_codec(&agent.agent_id)?;
                    let keys = match runtime.mail_checkpoint(&path, &agent.agent_id)? {
                        Some(sealed) => codec.open(&sealed)?,
                        None => client.agent_mail_keys(&agent.agent_id)?,
                    };
                    Ok((
                        reach,
                        path,
                        client.agent_signer(&agent.agent_id)?,
                        keys,
                        codec,
                    ))
                })
                .await?;
            let scope = Scope {
                path: &path,
                session: caller.session,
                reach: reach.as_deref(),
            };
            let marks_mail = tool == "agent_protocols_mail_list" && input["mark_read"] == true;
            let called = runtime
                .call(
                    &turn,
                    scope,
                    signer,
                    DurableMail {
                        keys: &mail_keys,
                        seal: Box::new(move |keys| codec.seal(keys)),
                    },
                    tool,
                    input,
                )
                .await?;
            let checkpoint = runtime.mail_checkpoint(&path, &agent.agent_id)?.is_some();
            let mail_notice = runtime.mail_notification(&path, &agent.agent_id)?;
            h.with(|client| {
                client.agent_find(&agent.agent_id)?;
                if checkpoint || called.mail_keys != mail_keys {
                    client.agent_mail_keys_save(&agent.agent_id, &called.mail_keys)?;
                }
                client.project_agent_mail_notification(
                    &agent.agent_id,
                    mail_notice.0,
                    mail_notice.1,
                    dmsg_client::now_ms(),
                )?;
                Ok(())
            })
            .await?;
            if checkpoint {
                runtime.clear_mail_checkpoint(&turn, &path)?;
            }
            if called.changed {
                self.connectors().send_modify(|revision| *revision += 1);
            }
            if called.changed
                && matches!(
                    tool,
                    "agent_protocols_mail_mailbox_publish" | "agent_protocols_mail_mailboxes_list"
                )
            {
                self.mail_push().maintain.notify_one();
            }
            if let Ok(result) = &called.outcome {
                let received = result["received"].as_array().map_or(0, Vec::len);
                let delivered = result["delivered"].as_array().map_or(0, Vec::len);
                let expired = result["expired"].as_array().map_or(0, Vec::len);
                if received + delivered + expired > 0
                    || (called.changed
                        && (marks_mail
                            || matches!(
                                tool,
                                "agent_protocols_mail_send"
                                    | "agent_protocols_mail_mailbox_publish"
                                    | "agent_protocols_mail_mailboxes_list"
                            )))
                {
                    // Notifications carry counts, never correspondence or keys.
                    self.emit(
                        "agent/mail",
                        json!({"agent":agent.agent_id,"received":received}),
                    );
                }
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
        if tool.starts_with("dmsg_secret_") {
            let mut client = self.client().lock().await;
            let current = client.agent_find(did)?;
            ensure(
                !current.paused
                    && current.credential == agent.credential
                    && current.credential.is_some(),
                Error::Forbidden,
            )?;
            let answer = match tool {
                "dmsg_secret_policies" => {
                    ensure(
                        input.as_object().is_some_and(|p| p.is_empty()),
                        Error::InvalidInput("input".into()),
                    )?;
                    client.secret_agent_policies(did)
                }
                "dmsg_secret_use_request" => {
                    let input = serde_json::from_value(input.clone())
                        .map_err(|_| Error::InvalidInput("secret use input".into()))?;
                    client.secret_use_request(did, input)
                }
                "dmsg_secret_use_status" | "dmsg_secret_use_cancel" => {
                    ensure(
                        input
                            .as_object()
                            .is_some_and(|p| p.len() == 1 && p.contains_key("request_id")),
                        Error::InvalidInput("input".into()),
                    )?;
                    let id = text(input, "request_id")?;
                    if tool.ends_with("_cancel") {
                        client.secret_use_cancel(&id, Some(did))
                    } else {
                        client.secret_use_get(&id, Some(did))
                    }
                }
                _ => Err(Error::NotFound),
            };
            drop(client);
            if matches!(tool, "dmsg_secret_use_request" | "dmsg_secret_use_cancel")
                && answer.is_ok()
            {
                self.emit("approval/changed", Value::Null);
            }
            return answer;
        }
        let h = self.hold();
        h.with(|_| Ok(())).await?;
        let channel = || text(input, "channel");
        let granted = async |capability: &str| {
            h.with(|client| client.capability_check(did, capability))
                .await
        };
        let wallet = || dmsg_client::wallet::parse_wallet(&text(input, "wallet")?);
        let proposal = || {
            input["proposal"]
                .as_u64()
                .ok_or_else(|| Error::InvalidInput("proposal".into()))
        };
        // These wait on the relay and the IC without the client, as the
        // owner's methods do.
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
                        to_json(&dmsg_client::principal::resolve_account(&h, &account).await?)
                    }
                    None => to_json(&dmsg_client::principal::show(&h).await?),
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
        if matches!(
            tool,
            "dmsg_wallet_propose_transfer" | "dmsg_wallet_execute" | "dmsg_wallet_report_broadcast"
        ) {
            let _turn = self.turn(&format!("wallet:{}", wallet()?)).await;
            let via = format!("agent:{did}");
            let result = match tool {
                "dmsg_wallet_propose_transfer" => {
                    let id = text(input, "wallet")?;
                    granted(&format!("wallet.propose:{id}")).await?;
                    let from = input["from"]
                        .as_u64()
                        .and_then(|f| u32::try_from(f).ok())
                        .ok_or_else(|| Error::InvalidInput("from".into()))?;
                    dmsg_client::wallet::ops::send(
                        &h,
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
                    granted(&format!("wallet.execute:{id}")).await?;
                    dmsg_client::wallet::ops::execute(&h, &via, wallet()?, proposal()?).await
                }
                "dmsg_wallet_report_broadcast" => {
                    let id = text(input, "wallet")?;
                    granted(&format!("wallet.execute:{id}")).await?;
                    match input["rebroadcast"] == true {
                        true => {
                            dmsg_client::wallet::ops::broadcast(&h, wallet()?, proposal()?, None)
                                .await
                        }
                        false => dmsg_client::wallet::ops::settle(&h, wallet()?, proposal()?).await,
                    }
                }
                _ => Err(Error::NotFound),
            }?;
            self.emit("wallet/changed", Value::Null);
            self.emit("approval/changed", Value::Null);
            return Ok(result);
        }

        // These still wait on the relay and the IC under the client:
        // contacts, delegations and the wallet's proposals and executions.
        let networked = matches!(tool, "dmsg_inbox_contact" | "dmsg_delegations_list");
        if networked {
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
            "dmsg_capabilities" => {
                let grants: Vec<Value> = client
                    .capability_live(did)?
                    .iter()
                    .map(|g| json!({"capabilities": g.capabilities, "expires_at": g.expires_at}))
                    .collect();
                Ok(json!({
                    "grants": grants,
                    "default": dmsg_client::agents::DEFAULT_CAPABILITIES,
                    "vocabulary": dmsg_client::agents::vocabulary(),
                }))
            }
            "dmsg_capabilities_request" => {
                let request = client.capability_request(
                    did,
                    parse(input, "capabilities")?,
                    optional(input, "days", number)?,
                )?;
                drop(client);
                self.emit("approval/changed", Value::Null);
                Ok(request)
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
                ensure(
                    input.get("secret").is_none(),
                    Error::InvalidInput("secret is not supported".into()),
                )?;
                Ok(to_json(&entry.masked(false)))
            }
            "dmsg_vault_write" => {
                let id = optional(input, "id", text)?;
                let base = optional(input, "base", text)?;
                ensure(
                    id.is_some() || base.is_none(),
                    Error::InvalidInput("base requires an item".into()),
                )?;
                let id = id.as_deref();
                let changes = Changes {
                    title: optional(input, "title", text)?,
                    notes: optional(input, "notes", text)?,
                    tags: optional(input, "tags", parse)?,
                    set: optional(input, "set", parse)?,
                    remove: optional(input, "remove", parse)?,
                    ..Changes::default()
                };
                let kind = input["type"].as_str().unwrap_or("note");
                let old = match id {
                    Some(id) => {
                        let entry = client.vault_show(id)?;
                        ensure(!entry.deleted, Error::NotFound)?;
                        ensure(!entry.item.reserved(), Error::Forbidden)?;
                        tagged(&client, did, "vault.write", &entry.item)?;
                        // Agents may edit the head, never use writes as a
                        // history-reading API after the owner hides or moves data.
                        ensure(
                            base.as_deref().is_none_or(|base| base == entry.revision),
                            Error::VersionConflict,
                        )?;
                        Some(entry.item)
                    }
                    None => None,
                };
                let mut item = old
                    .clone()
                    .unwrap_or_else(|| Item::new(kind, "", dmsg_client::now_ms()));
                changes.apply(&mut item);
                item.normalize();
                ensure(
                    old.as_ref().is_none_or(|old| old.tags == item.tags),
                    Error::Forbidden,
                )?;
                ensure(!item.reserved(), Error::Forbidden)?;
                // An agent may replace or remove a hidden value, never show
                // it by changing the field's type.
                ensure(
                    old.is_none_or(|old| item.keeps_hidden(&old)),
                    Error::Forbidden,
                )?;
                tagged(&client, did, "vault.write", &item)?;
                let entry = match id {
                    Some(id) => client.vault_edit(id, base.as_deref(), &changes)?,
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

/// An unrestricted ceiling is an explicit owner choice, never an empty form.
fn controller_authority(params: &Value) -> Result<DelegationAuthority> {
    match params.get("unrestricted") {
        Some(Value::Bool(true))
            if !params["scopes"].is_null() || !params["audiences"].is_null() =>
        {
            Err(Error::InvalidInput(
                "choose unrestricted or scopes and audiences".into(),
            ))
        }
        Some(Value::Bool(true)) => Ok(DelegationAuthority::Unrestricted),
        None | Some(Value::Bool(false)) => Ok(DelegationAuthority::Restricted {
            scopes: parse(params, "scopes")?,
            audiences: parse(params, "audiences")?,
        }),
        _ => Err(Error::InvalidInput("unrestricted must be a boolean".into())),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_registration_requires_explicit_authority() {
        for params in [
            json!({}),
            json!({"scopes": null, "audiences": null}),
            json!({"unrestricted": "true"}),
            json!({"unrestricted": true, "scopes": []}),
            json!({"scopes": ["read"]}),
        ] {
            assert!(controller_authority(&params).is_err(), "{params}");
        }
        assert_eq!(
            controller_authority(&json!({"unrestricted": true})).unwrap(),
            DelegationAuthority::Unrestricted
        );
        assert_eq!(
            controller_authority(
                &json!({"scopes": ["app.read"], "audiences": ["https://example.com"]})
            )
            .unwrap(),
            DelegationAuthority::Restricted {
                scopes: vec!["app.read".into()],
                audiences: vec!["https://example.com".into()]
            },
        );
    }

    #[tokio::test]
    async fn background_mail_requires_live_grants_and_residency_while_locked() {
        let dir = dmsg_client::testing::TempDir::new();
        let home = crate::home::Home {
            dir: dir.0.join("home"),
        };
        let mut client = dmsg_client::testing::unlocked(
            &dir.0.join("workspace"),
            false,
            Some(Arc::new(dmsg_client::testing::MemoryStorage::default())),
        );
        let agent = client.agent_create("mail-test").unwrap();
        let runtime = dmsg_agent::Runtime::new(&dmsg_agent::Options {
            hosts: vec![dmsg_agent::Host {
                host: "https://mail.example".into(),
                label: None,
                profile_service: None,
            }],
            ..Default::default()
        })
        .unwrap();
        let daemon = Arc::new(Daemon::new(
            "owner-token".into(),
            0,
            home.settings().unwrap(),
            client,
            runtime,
            crate::mcp_events::McpEvents::new(&[]).unwrap(),
        ));
        let id = &agent.agent_id;
        assert_eq!(daemon.receive_agent_mail(id).await, Err(Error::Forbidden));
        let grant = daemon
            .client()
            .lock()
            .await
            .capability_grant(
                "cli",
                id,
                vec!["mail.read:https://mail.example".into()],
                None,
            )
            .unwrap();
        // Inspection of committed mail must not queue behind a network
        // call of this same Agent ID. Marking reads still take that turn.
        let turn = daemon.agents().turn(id).await;
        for tool in [
            "agent_protocols_mail_list",
            "agent_protocols_mail_mailboxes_list",
        ] {
            let params = json!({"agent":id, "source":"gui", "tool":tool, "input":{}});
            let read = tokio::time::timeout(
                Duration::from_secs(1),
                daemon.call_agents("ap/call", &params),
            )
            .await
            .expect("local Mail inspection should not wait for the connector")
            .unwrap();
            assert!(read["error"].is_null(), "{read}");
        }
        let params = json!({"agent":id, "source":"cli", "tool":"agent_protocols_mail_list", "input":{"mark_read":true}});
        let mut mark = Box::pin(daemon.call_agents("ap/call", &params));
        assert!(futures_util::poll!(&mut mark).is_pending());
        drop(turn);
        assert!(mark.await.unwrap()["error"].is_null());
        let received = daemon.receive_agent_mail(id).await.unwrap();
        assert_eq!(received["result"]["errors"], json!([]));
        assert_eq!(received["result"]["pending_inbox_count"], 0);

        // A queued background call must check a pause made while it waited
        // for another connector call, not reuse the entry it saw earlier.
        let turn = daemon.agents().turn(id).await;
        let waiting = tokio::spawn({
            let daemon = daemon.clone();
            let id = id.clone();
            async move { daemon.receive_agent_mail(&id).await }
        });
        tokio::task::yield_now().await;
        daemon.client().lock().await.agent_pause(id, true).unwrap();
        drop(turn);
        assert_eq!(waiting.await.unwrap(), Err(Error::Forbidden));
        daemon.client().lock().await.agent_pause(id, false).unwrap();
        daemon
            .client()
            .lock()
            .await
            .capability_revoke("cli", &grant.id)
            .unwrap();
        assert_eq!(daemon.receive_agent_mail(id).await, Err(Error::Forbidden));
        daemon
            .client()
            .lock()
            .await
            .capability_grant(
                "cli",
                id,
                vec!["mail.read:https://mail.example".into()],
                None,
            )
            .unwrap();
        let ordinary = daemon
            .client()
            .lock()
            .await
            .agent_create("ordinary")
            .unwrap();
        daemon
            .client()
            .lock()
            .await
            .capability_grant(
                "cli",
                &ordinary.agent_id,
                vec!["mail.read:https://mail.example".into()],
                None,
            )
            .unwrap();
        daemon
            .client()
            .lock()
            .await
            .agent_resident(id, true)
            .unwrap();
        daemon.client().lock().await.lock();
        assert_eq!(
            daemon.receive_agent_mail(&ordinary.agent_id).await,
            Err(Error::Locked)
        );
        assert_eq!(
            daemon.receive_agent_mail(id).await.unwrap()["result"]["errors"],
            json!([])
        );
        daemon.client().lock().await.agent_pause(id, true).unwrap();
        assert_eq!(daemon.receive_agent_mail(id).await, Err(Error::Forbidden));

        // An operation queued for this account cannot resume against the
        // next selected workspace, even when it retained a resident key.
        daemon.client().lock().await.agent_pause(id, false).unwrap();
        let turn = daemon.agents().turn(id).await;
        let mut waiting = Box::pin(daemon.receive_agent_mail(id));
        assert!(futures_util::poll!(&mut waiting).is_pending());
        daemon.client().lock().await.logout().unwrap();
        drop(turn);
        assert_eq!(
            waiting.await,
            Err(Error::Unavailable("the account changed".into()))
        );
    }

    #[tokio::test]
    async fn review_historical_base_cannot_unhide_a_current_secret() {
        let dir = dmsg_client::testing::TempDir::new();
        let home = crate::home::Home {
            dir: dir.0.join("home"),
        };
        let mut c = dmsg_client::testing::unlocked(&dir.0.join("workspace"), false, None);
        let old=c.vault_create("note",&serde_json::from_value::<Changes>(json!({"title":"Imported credential","tags":["work"],"set":[{"id":"credential","type":"text","value":"review-hidden-fixture"}]})).unwrap()).unwrap();
        let current = c
            .vault_edit(
                &old.id,
                None,
                &serde_json::from_value::<Changes>(
                    json!({"set":[{"id":"credential","type":"concealed"}]}),
                )
                .unwrap(),
            )
            .unwrap();
        let agent = c.agent_create("history-reader").unwrap();
        c.capability_grant(
            "gui",
            &agent.agent_id,
            vec!["vault.read:work".into(), "vault.write:work".into()],
            None,
        )
        .unwrap();
        let d = Daemon::new(
            "owner".into(),
            0,
            home.settings().unwrap(),
            c,
            dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
            crate::mcp_events::McpEvents::new(&[]).unwrap(),
        );
        let reply=d.call_agent(&agent,"mcp/call",&json!({"tool":"dmsg_vault_write","input":{"id":old.id,"base":old.revision,"notes":"unchanged field"}})).await.unwrap();
        assert!(
            !reply.to_string().contains("review-hidden-fixture"),
            "a historical write must not reveal the current concealed value"
        );
        assert_eq!(reply["error"]["code"], "VERSION_CONFLICT");
        assert_eq!(
            d.client()
                .lock()
                .await
                .vault_show(&old.id)
                .unwrap()
                .revision,
            current.revision
        );
        assert!(
            d.client()
                .lock()
                .await
                .vault_conflicts()
                .unwrap()
                .is_empty()
        );
        for invalid in [
            json!({"id":old.id,"base":1}),
            json!({"id":[],"notes":"bad id"}),
        ] {
            let rejected = d
                .call_agent(
                    &agent,
                    "mcp/call",
                    &json!({"tool":"dmsg_vault_write","input":invalid}),
                )
                .await
                .unwrap();
            assert_eq!(rejected["error"]["code"], "INVALID_INPUT");
        }
        let changed=d.call_agent(&agent,"mcp/call",&json!({"tool":"dmsg_vault_write","input":{"id":old.id,"base":current.revision,"notes":"safe head edit"}})).await.unwrap();
        assert!(changed["error"].is_null());
        assert!(!changed.to_string().contains("review-hidden-fixture"));
        assert_eq!(changed["result"]["item"]["notes"], "safe head edit");
    }

    #[tokio::test]
    async fn agent_vault_write_never_shows_a_hidden_value() {
        let dir = dmsg_client::testing::TempDir::new();
        let home = crate::home::Home {
            dir: dir.0.join("home"),
        };
        let mut client = dmsg_client::testing::unlocked(
            &dir.0.join("workspace"),
            false,
            Some(Arc::new(dmsg_client::testing::MemoryStorage::default())),
        );
        let agent = client.agent_create("vault-test").unwrap();
        client
            .capability_grant(
                "cli",
                &agent.agent_id,
                vec!["vault.read:work".into(), "vault.write:work".into()],
                None,
            )
            .unwrap();
        let changes: Changes = serde_json::from_value(json!({
            "title": "CI token",
            "tags": ["work"],
            "set": [
                {"id": "password", "value": "s3cret"},
                {"label": "Recovery", "type": "otp", "value": "JBSWY3DPEHPK3PXP"}
            ]
        }))
        .unwrap();
        let created = client.vault_create("login", &changes).unwrap();
        let daemon = Arc::new(Daemon::new(
            "owner-token".into(),
            0,
            home.settings().unwrap(),
            client,
            dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
            crate::mcp_events::McpEvents::new(&[]).unwrap(),
        ));
        let write = async |input: Value| {
            let params = json!({"tool": "dmsg_vault_write", "input": input});
            daemon
                .call_agent(&agent, "mcp/call", &params)
                .await
                .unwrap()
        };
        let field = |reply: &Value, id: &str| {
            reply["result"]["item"]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["id"] == id)
                .map(|f| f["value"].clone())
        };

        for tags in [json!(["work", "new-scope"]), json!(["different"])] {
            let reply = write(json!({"id":created.id,"tags":tags})).await;
            assert_eq!(reply["error"]["code"], "FORBIDDEN");
        }
        for secret in [true, false] {
            let reply = daemon
                .call_agent(
                    &agent,
                    "mcp/call",
                    &json!({"tool":"dmsg_vault_read","input":{"id":created.id,"secret":secret}}),
                )
                .await
                .unwrap();
            assert_eq!(reply["error"]["code"], "INVALID_INPUT");
            assert!(!reply.to_string().contains("s3cret"));
        }
        // A type shown in the clear, named by ID or by label, is refused
        // and nothing is written.
        for set in [
            json!([{"id": "password", "type": "text"}]),
            json!([{"label": "Recovery", "type": "url"}]),
        ] {
            let reply = write(json!({"id": created.id, "set": set})).await;
            assert_eq!(reply["error"]["code"], "FORBIDDEN", "{reply}");
        }
        let kept = daemon
            .client()
            .lock()
            .await
            .vault_show(&created.id)
            .unwrap();
        assert_eq!(kept.revision, created.revision);

        // A new value or removing the field stays possible.
        let rotated =
            write(json!({"id": created.id, "set": [{"id": "password", "value": "rotated"}]})).await;
        assert_eq!(field(&rotated, "password"), Some(json!("********")));
        let removed = write(json!({"id": created.id, "remove": ["Recovery"]})).await;
        assert!(removed["error"].is_null(), "{removed}");
        let owned = daemon
            .client()
            .lock()
            .await
            .vault_show(&created.id)
            .unwrap();
        assert_eq!(owned.item.value("password"), "rotated");
        assert!(owned.item.field("password").unwrap().hidden());
        assert!(!owned.item.fields.iter().any(|f| f.kind == "otp"));

        // A new item has no hidden value to show yet.
        let added = write(json!({
            "type": "login",
            "title": "Staging",
            "tags": ["work"],
            "set": [{"id": "password", "type": "text", "value": "plain"}]
        }))
        .await;
        assert_eq!(field(&added, "password"), Some(json!("plain")));
    }

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
