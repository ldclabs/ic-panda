//! The daemon methods of Agent Delegation and Agent Protocols (plan W6).
//!
//! The owner enables the account's principal, registers controller keys and
//! signs grants through the approval center; hosts Agent IDs and grants them
//! capabilities. An agent's MCP session presents its own credential, which
//! reaches only `mcp/tools` and `mcp/call`: the standard connector tools as
//! its own Agent ID, and the `dmsg_*` tools its capabilities allow. Nothing
//! an agent calls can answer an approval, change the host allowlist or the
//! grants, or reach another Agent ID.

use crate::daemon::Daemon;
use dmsg_agent::tools::{is_dmsg, is_standard, tool_list};
use dmsg_client::{
    agent::PrincipalChange, agents::AgentEntry, attest::StatementInput, error_code, vault::Filter,
};
use dmsg_types::{
    agent::{DelegationAuthority, PrincipalType},
    Error, Result,
};
use serde_json::{json, Value};

/// The owner's methods this module serves.
pub(crate) const METHODS: [&str; 21] = [
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
];

/// The only methods an agent's credential reaches.
pub(crate) const AGENT_METHODS: [&str; 2] = ["mcp/tools", "mcp/call"];

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
            return self
                .tool(&agent, &text(params, "tool")?, params["input"].clone())
                .await;
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
            "agent/create" => Ok(client.agent_create(&text(params, "label")?)?.view()),
            "agent/use" => Ok(client.agent_use(&text(params, "agent")?)?.view()),
            "agent/resident" => {
                let on = params["on"]
                    .as_bool()
                    .ok_or_else(|| Error::InvalidInput("on".into()))?;
                Ok(client.agent_resident(&text(params, "agent")?, on)?.view())
            }
            "agent/credential" => {
                let agent = client.agent_find(&text(params, "agent")?)?;
                let credential = client.agent_credential(&agent.agent_id)?;
                Ok(json!({"agent_id": agent.agent_id, "credential": credential}))
            }
            "grants/list" => Ok(to_json(&client.capability_grants()?)),
            "grants/add" => {
                let grant = client.capability_grant(
                    &text(params, "agent")?,
                    strings(params, "capabilities")?,
                    params["days"].as_u64(),
                )?;
                Ok(to_json(&grant))
            }
            "grants/revoke" => Ok(to_json(&client.capability_revoke(&text(params, "id")?)?)),
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
        match method {
            "mcp/tools" => Ok(json!({"tools": tool_list(), "agent_id": agent.agent_id})),
            "mcp/call" => {
                self.tool(agent, &text(params, "tool")?, params["input"].clone())
                    .await
            }
            _ => Err(Error::Forbidden),
        }
    }

    /// One tool call as `agent`: `{"result": …}` or `{"error": {code, message, data?}}`.
    async fn tool(&self, agent: &AgentEntry, tool: &str, input: Value) -> Result<Value> {
        if is_standard(tool) {
            // The key leaves the client lock for the call only.
            let signer = self.client().lock().await.agent_signer(&agent.agent_id)?;
            return Ok(match self.agents().call(signer, tool, input).await? {
                Ok(result) => json!({"result": result}),
                Err(error) => json!({"error": error}),
            });
        }
        if !is_dmsg(tool) {
            return Err(Error::NotFound);
        }
        Ok(match self.dmsg_tool(agent, tool, &input).await {
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
                Ok(Value::Array(
                    client
                        .vault_list(&filter)?
                        .into_iter()
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
                let allowed = entry.item.tags.iter().any(|t| {
                    client
                        .capability_check(did, &format!("vault.read:{t}"))
                        .is_ok()
                });
                if !allowed {
                    client.capability_check(did, "vault.read:*")?;
                }
                // The secret field never reaches an agent.
                Ok(to_json(&entry.masked(false)))
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
                Ok(to_json(&client.inbox_list().await?))
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
