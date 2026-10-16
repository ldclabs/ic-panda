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
use dmsg_agent::resources::{resource_list, resource_templates, Resource};
use dmsg_agent::tools::{is_dmsg, is_read_only, is_standard, standard_capability, tool_list};
use dmsg_agent::Scope;
use dmsg_client::{
    agent::PrincipalChange,
    agents::AgentEntry,
    attest::StatementInput,
    codec::{b64, unb64},
    error_code,
    library::catalog::ROOT,
    vault::{Filter, Item},
    Client,
};
use dmsg_types::{
    agent::{DelegationAuthority, PrincipalType},
    ensure, Error, Result,
};
use serde_json::{json, Value};
use std::time::Duration;

/// Most bytes one `dmsg_files_read` returns.
const MAX_READ: u64 = 1024 * 1024;
/// Largest library file an agent reads from.
const MAX_AGENT_FILE: u64 = 64 * 1024 * 1024;
/// Largest content one `dmsg_files_write` takes.
const MAX_AGENT_WRITE: usize = 8 * 1024 * 1024;
/// How long `mcp/changed` waits at most.
const CHANGED_WAIT_MS: u64 = 25_000;
/// Input fields that name what a call acts on, in order of preference.
const TARGETS: [&str; 8] = [
    "id",
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

fn text(params: &Value, name: &str) -> Result<String> {
    params[name]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| Error::InvalidInput(format!("missing {name}")))
}

fn generation(params: &Value) -> Result<u32> {
    params["generation"]
        .as_u64()
        .and_then(|g| u32::try_from(g).ok())
        .ok_or_else(|| Error::InvalidInput("generation".into()))
}

fn strings(params: &Value, name: &str) -> Result<Vec<String>> {
    serde_json::from_value(params[name].clone())
        .map_err(|_| Error::InvalidInput(format!("{name}: a list of strings")))
}

/// The local source of a request: the CLI or the desktop.
fn source(params: &Value) -> Result<&'static str> {
    match params["source"].as_str() {
        Some("gui") => Ok("gui"),
        Some("cli") | None => Ok("cli"),
        Some(_) => Err(Error::InvalidInput("source".into())),
    }
}

fn to_json<T: serde::Serialize + ?Sized>(value: &T) -> Value {
    serde_json::to_value(value).expect("serializable")
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
        let mut client = self.client().lock().await;
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
                        scopes: strings(params, "scopes")?,
                        audiences: strings(params, "audiences")?,
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
                generation: generation(params)?,
            }),
            "agent/compromise" => Some(PrincipalChange::Compromise {
                generation: generation(params)?,
                invalid_from: params["invalid_from"]
                    .as_u64()
                    .ok_or_else(|| Error::InvalidInput("invalid_from".into()))?,
            }),
            "agent/rename" => Some(PrincipalChange::Rename {
                generation: generation(params)?,
                name: params["name"].as_str().map(str::to_string),
            }),
            _ => None,
        };
        if let Some(change) = principal {
            return client.principal_request(source(params)?, change).await;
        }
        match method {
            "agent/principal" => client.principal_view().await,
            "agent/publish" => Ok(json!({"published_version": client.principal_publish().await?})),
            "agent/refresh" => client.principal_refresh().await,
            "agent/grant" => {
                let generation = match params["generation"].is_null() {
                    true => latest_controller(&mut client).await?,
                    false => generation(params)?,
                };
                // The subject may name a hosted Agent ID by its label.
                let subject = text(params, "subject")?;
                let subject = client.agent_find(&subject).map_or(subject, |a| a.agent_id);
                client
                    .delegation_grant_request(
                        source(params)?,
                        generation,
                        &subject,
                        strings(params, "scopes")?,
                        strings(params, "audiences")?,
                        params["days"].as_u64().unwrap_or(30),
                        params["relationship"].as_str().map(str::to_string),
                    )
                    .await
            }
            "agent/revoke" => {
                let generation = match params["generation"].is_null() {
                    true => latest_controller(&mut client).await?,
                    false => generation(params)?,
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
            "agent/delegations" => Ok(Value::Array(client.delegations()?)),
            "agent/ids" => {
                let mut list = client.agent_list()?;
                list["hosts"] = to_json(self.agents().hosts());
                Ok(list)
            }
            "agent/create" => {
                let entry = client.agent_create(&text(params, "label")?)?;
                let via = source(params)?;
                client.audit("owner", via, method, Some(&entry.agent_id), None, "ok");
                Ok(entry.view())
            }
            "agent/use" => Ok(client.agent_use(&text(params, "agent")?)?.view()),
            "agent/resident" => {
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
            }
            "agent/credential" => {
                let agent = client.agent_find(&text(params, "agent")?)?;
                let credential = client.agent_credential(&agent.agent_id)?;
                let via = source(params)?;
                client.audit("owner", via, method, Some(&agent.agent_id), None, "ok");
                Ok(json!({"agent_id": agent.agent_id, "credential": credential}))
            }
            "grants/list" => Ok(to_json(&client.capability_grants()?)),
            "grants/add" => {
                let grant = client.capability_grant(
                    source(params)?,
                    &text(params, "agent")?,
                    strings(params, "capabilities")?,
                    params["days"].as_u64(),
                )?;
                Ok(to_json(&grant))
            }
            "grants/revoke" => Ok(to_json(
                &client.capability_revoke(source(params)?, &text(params, "id")?)?,
            )),
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
            // An agent reaches what its grant names; the owner acting as one
            // reaches every configured host.
            let reach = match caller.actor {
                "owner" => None,
                _ => match self.reach(&*self.client().lock().await, agent, tool) {
                    Ok(reach) => reach,
                    Err(error) => return Ok(refused(&error)),
                },
            };
            // The turn spans reading the mail keys to saving what the call
            // left, so no other call sees or overwrites them in between; the
            // keys leave the client lock for the call only.
            let runtime = self.agents();
            let turn = runtime.turn().await;
            let (path, signer, mail_keys) = {
                let mut client = self.client().lock().await;
                (
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
                .call(&turn, scope, signer, Some(&mail_keys), tool, input)
                .await?;
            self.connectors().send_modify(|revision| *revision += 1);
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
        let mut client = self.client().lock().await;
        let channel = || text(input, "channel");
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
                // The secret field only with a grant naming the item.
                let reveal = input["secret"] == true;
                if reveal {
                    client.capability_check(did, &format!("vault.secret:{}", entry.id))?;
                }
                Ok(to_json(&entry.masked(reveal)))
            }
            "dmsg_vault_write" => {
                let id = input["id"].as_str();
                let now = dmsg_client::now_ms();
                let mut item = match id {
                    Some(id) => {
                        let entry = client.vault_show(id)?;
                        ensure(!entry.deleted, Error::NotFound)?;
                        ensure(!entry.item.reserved(), Error::Forbidden)?;
                        tagged(&client, did, "vault.write", &entry.item)?;
                        entry.item
                    }
                    None => Item {
                        kind: "note".into(),
                        title: String::new(),
                        tags: Vec::new(),
                        body: String::new(),
                        username: String::new(),
                        secret: String::new(),
                        url: String::new(),
                        favorite: false,
                        created_at: now,
                        updated_at: now,
                        resolved_conflicts: None,
                    },
                };
                if let Some(kind) = input["type"].as_str() {
                    item.kind = kind.into();
                }
                for (field, slot) in [
                    ("title", &mut item.title),
                    ("body", &mut item.body),
                    ("username", &mut item.username),
                    ("url", &mut item.url),
                    ("secret", &mut item.secret),
                ] {
                    if let Some(value) = input[field].as_str() {
                        *slot = value.into();
                    }
                }
                if !input["tags"].is_null() {
                    item.tags = strings(input, "tags")?;
                }
                item.updated_at = now;
                ensure(!item.reserved(), Error::Forbidden)?;
                tagged(&client, did, "vault.write", &item)?;
                let entry = client.vault_put(id, input["base"].as_str(), &item)?;
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
                let (manifest, bytes) =
                    client.files_read(&id, input["version"].as_str(), MAX_AGENT_FILE)?;
                let offset = input["offset"].as_u64().unwrap_or(0).min(manifest.size);
                let length = input["length"]
                    .as_u64()
                    .unwrap_or(MAX_READ)
                    .clamp(1, MAX_READ);
                let end = (offset + length).min(manifest.size);
                Ok(json!({
                    "id": manifest.id,
                    "version": manifest.version,
                    "name": manifest.name,
                    "mime": manifest.mime,
                    "size": manifest.size,
                    "sha256": manifest.sha256,
                    "offset": offset,
                    "length": end - offset,
                    "eof": end == manifest.size,
                    "content_base64url": b64(&bytes[offset as usize..end as usize]),
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
                let found = client.files_search(&text(input, "query")?)?;
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
            "dmsg_channel_publish" => {
                let id = channel()?;
                client.capability_check(did, &format!("channel.publish:{id}"))?;
                client.channel_send(&id, &text(input, "text")?, None).await
            }
            "dmsg_inbox_list" => {
                client.capability_check(did, "inbox.read")?;
                let letters = client.inbox_list().await?;
                Ok(Value::Array(
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
                ))
            }
            "dmsg_inbox_read" => {
                client.capability_check(did, "inbox.read")?;
                let order = text(input, "order_id")?;
                let letter = client
                    .inbox_list()
                    .await?
                    .into_iter()
                    .find(|l| l.order_id == order)
                    .ok_or(Error::NotFound)?;
                let mark = input["mark_read"] == true && !letter.read;
                if mark {
                    client.inbox_mark(&order, true, letter.archived).await?;
                }
                Ok(json!({
                    "order_id": letter.order_id,
                    "sender": letter.sender,
                    "state": letter.state,
                    "text": letter.text,
                    "created_at": letter.created_at,
                    "read": letter.read || mark,
                    "archived": letter.archived,
                    "abort_reason": letter.abort_reason,
                }))
            }
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
            "dmsg_profile_read" => {
                client.capability_check(did, "profile.read")?;
                match input["account"].as_str() {
                    Some(account) => {
                        let account = account
                            .parse()
                            .map_err(|_| Error::InvalidInput("account".into()))?;
                        Ok(to_json(&client.profile_resolve(&account).await?))
                    }
                    None => Ok(to_json(&client.profile().await?)),
                }
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
            "dmsg_action_request" => {
                client.capability_check(did, "action.request")?;
                let action = unb64(&text(input, "action")?)?;
                let request = client
                    .attest_action_request(&format!("agent:{did}"), &action)
                    .await?;
                drop(client);
                self.emit("approval/changed", Value::Null);
                Ok(request)
            }
            "dmsg_delegations_list" => {
                client.capability_check(did, "agent.delegation.read")?;
                Ok(Value::Array(client.delegation_credentials().await?))
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
        use dmsg_client::agents::{validate_capability, HOST_CAPABILITIES};
        for tool in tool_list() {
            if let Some(name) = standard_capability(tool["name"].as_str().unwrap()) {
                assert!(HOST_CAPABILITIES.contains(&name), "{name}");
                validate_capability(&format!("{name}:*")).unwrap();
                validate_capability(&format!("{name}:https://discourse.dmsg.net")).unwrap();
            }
        }
    }
}
