//! CLI commands of Agent Delegation, the hosted Agent IDs and their
//! capability grants, and `dmsg ap`, which runs one MCP tool as an Agent ID.
//! Principal changes and grants create approval requests, which an
//! interactive terminal decides at once.

use crate::remote::{Failure, Remote};
use crate::{created, pretty, print};
use clap::Subcommand;
use serde_json::{json, Value};

#[derive(Subcommand)]
pub enum AgentCommand {
    /// The account's Agent Delegation principal.
    #[command(subcommand)]
    Principal(PrincipalCommand),
    /// The principal's controller keys, kept in the vault.
    #[command(subcommand)]
    Controller(ControllerCommand),
    /// Grant an agent a credential, signed with a controller key.
    Grant {
        /// The agent: `did:agent:…` or a hosted Agent ID's label.
        subject: String,
        /// An exact scope; repeat for more.
        #[arg(long = "scope", required = true)]
        scopes: Vec<String>,
        /// A relying party, HTTPS origin or Agent ID; repeat for more.
        #[arg(long = "audience", required = true)]
        audiences: Vec<String>,
        /// Lifetime in days, 1 to 366.
        #[arg(long, default_value_t = 30)]
        days: u64,
        /// Controller generation; the newest current one by default.
        #[arg(long)]
        generation: Option<u32>,
        /// How the agent relates to this account, shown to relying parties.
        #[arg(long)]
        relationship: Option<String>,
    },
    /// Revoke a credential.
    Revoke {
        /// Delegation ID.
        id: String,
        /// Controller generation; the newest current one by default.
        #[arg(long)]
        generation: Option<u32>,
        /// Reason, shown to relying parties.
        #[arg(long)]
        reason: Option<String>,
    },
    /// The principal's credentials, as the delegation service holds them.
    Credentials,
    /// Grants and revocations signed on this device and where they stand.
    Delegations,
    /// The Agent IDs this device hosts.
    #[command(subcommand)]
    Id(IdCommand),
}

#[derive(Subcommand)]
pub enum PrincipalCommand {
    /// The principal, its controllers and publication.
    Show,
    /// Publish this account as a principal (needs approval).
    Enable {
        /// person, organization, team, project or other.
        #[arg(long = "type", default_value = "person")]
        kind: String,
    },
    /// Ask the user home to publish the principal document again.
    Publish,
    /// Let the delegation service read the published document again.
    Refresh,
}

#[derive(Subcommand)]
pub enum ControllerCommand {
    /// Create the next controller key and register it (needs approval).
    /// Without scopes and audiences its ceiling is "*".
    Register {
        /// Display label.
        #[arg(long)]
        name: Option<String>,
        /// A scope it may grant; repeat for more.
        #[arg(long = "scope")]
        scopes: Vec<String>,
        /// A relying party it may grant for; repeat for more.
        #[arg(long = "audience")]
        audiences: Vec<String>,
    },
    /// Retire a controller; it never signs again.
    Retire {
        /// Generation.
        generation: u32,
    },
    /// Distrust a controller's signatures from a time, retiring it.
    Compromise {
        /// Generation.
        generation: u32,
        /// Unix milliseconds of the earliest untrusted signature.
        #[arg(long)]
        from: u64,
    },
    /// Change a controller's display label.
    Rename {
        /// Generation.
        generation: u32,
        /// New label; none removes it.
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum IdCommand {
    /// Host an Agent ID; the same label on another device of the account
    /// brings the same Agent ID.
    Create {
        /// `[a-z0-9_-]{1,40}`.
        label: String,
    },
    /// Hosted Agent IDs, the active one and the configured hosts.
    List,
    /// The Agent ID `dmsg ap` uses by default.
    Use {
        /// Label or `did:agent:…`.
        agent: String,
    },
    /// Keep the Agent ID's key in the system credential store, so it signs
    /// while the device is locked; `off` deletes the copy.
    Resident {
        /// Label or `did:agent:…`.
        agent: String,
        /// on or off.
        state: String,
    },
    /// Issue the credential an MCP client gives `dmsg mcp` as
    /// DMSG_AGENT_TOKEN; the previous one stops working.
    Credential {
        /// Label or `did:agent:…`.
        agent: String,
    },
}

#[derive(Subcommand)]
pub enum GrantsCommand {
    /// Every capability grant, revoked ones included.
    List,
    /// Grant a hosted Agent ID capabilities over this account.
    Add {
        /// Label or `did:agent:…`.
        agent: String,
        /// account.read, vault.list, vault.read:<tag>, vault.write:<tag>,
        /// vault.secret:<item>, file.read:<tag>, channel.read:<channel>,
        /// channel.publish:<channel>, inbox.read, inbox.contact,
        /// profile.read, attest.request, action.request,
        /// agent.delegation.read, events.endpoint; `<name>:*` covers every
        /// target. Items tagged `dmsg-…` are dMsg's own keys and out of reach.
        #[arg(required = true)]
        capabilities: Vec<String>,
        /// Lifetime in days; until revoked by default.
        #[arg(long)]
        days: Option<u64>,
    },
    /// Revoke a grant.
    Revoke {
        /// Grant ID.
        id: String,
    },
}

pub async fn agent(remote: &Remote, json: bool, command: AgentCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        AgentCommand::Principal(PrincipalCommand::Show) => ("agent/principal", Value::Null),
        AgentCommand::Principal(PrincipalCommand::Enable { kind }) => {
            return request(remote, json, "agent/enable", json!({"type": kind})).await
        }
        AgentCommand::Principal(PrincipalCommand::Publish) => ("agent/publish", Value::Null),
        AgentCommand::Principal(PrincipalCommand::Refresh) => ("agent/refresh", Value::Null),
        AgentCommand::Controller(ControllerCommand::Register {
            name,
            scopes,
            audiences,
        }) => {
            let ceiling = match scopes.is_empty() && audiences.is_empty() {
                true => json!({"name": name}),
                false => json!({"name": name, "scopes": scopes, "audiences": audiences}),
            };
            return request(remote, json, "agent/register", ceiling).await;
        }
        AgentCommand::Controller(ControllerCommand::Retire { generation }) => {
            return request(
                remote,
                json,
                "agent/retire",
                json!({"generation": generation}),
            )
            .await
        }
        AgentCommand::Controller(ControllerCommand::Compromise { generation, from }) => {
            return request(
                remote,
                json,
                "agent/compromise",
                json!({"generation": generation, "invalid_from": from}),
            )
            .await
        }
        AgentCommand::Controller(ControllerCommand::Rename { generation, name }) => {
            return request(
                remote,
                json,
                "agent/rename",
                json!({"generation": generation, "name": name}),
            )
            .await
        }
        AgentCommand::Grant {
            subject,
            scopes,
            audiences,
            days,
            generation,
            relationship,
        } => {
            return request(
                remote,
                json,
                "agent/grant",
                json!({
                    "subject": subject, "scopes": scopes, "audiences": audiences, "days": days,
                    "generation": generation, "relationship": relationship,
                }),
            )
            .await
        }
        AgentCommand::Revoke {
            id,
            generation,
            reason,
        } => {
            return request(
                remote,
                json,
                "agent/revoke",
                json!({"id": id, "generation": generation, "reason": reason}),
            )
            .await
        }
        AgentCommand::Credentials => ("agent/credentials", Value::Null),
        AgentCommand::Delegations => ("agent/delegations", Value::Null),
        AgentCommand::Id(IdCommand::Create { label }) => ("agent/create", json!({"label": label})),
        AgentCommand::Id(IdCommand::List) => ("agent/ids", Value::Null),
        AgentCommand::Id(IdCommand::Use { agent }) => ("agent/use", json!({"agent": agent})),
        AgentCommand::Id(IdCommand::Resident { agent, state }) => {
            let on = match state.as_str() {
                "on" => true,
                "off" => false,
                _ => return Err(Failure::new("INVALID_INPUT", "state: on or off")),
            };
            ("agent/resident", json!({"agent": agent, "on": on}))
        }
        AgentCommand::Id(IdCommand::Credential { agent }) => {
            ("agent/credential", json!({"agent": agent}))
        }
    };
    let value = remote.call(method, params).await?;
    print(json, &value, pretty);
    Ok(())
}

/// A method that creates an approval request.
async fn request(
    remote: &Remote,
    json: bool,
    method: &str,
    mut params: Value,
) -> Result<(), Failure> {
    params["source"] = "cli".into();
    let request = remote.call(method, params).await?;
    created(remote, json, request).await
}

pub async fn grants(remote: &Remote, json: bool, command: GrantsCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        GrantsCommand::List => ("grants/list", Value::Null),
        GrantsCommand::Add {
            agent,
            capabilities,
            days,
        } => (
            "grants/add",
            json!({"agent": agent, "capabilities": capabilities, "days": days}),
        ),
        GrantsCommand::Revoke { id } => ("grants/revoke", json!({"id": id})),
    };
    let value = remote.call(method, params).await?;
    print(json, &value, pretty);
    Ok(())
}

/// `dmsg audit`: who did what on this device, newest first.
pub async fn audit(
    remote: &Remote,
    json: bool,
    agent: Option<String>,
    before: Option<u64>,
    limit: u64,
) -> Result<(), Failure> {
    let value = remote
        .call(
            "audit/list",
            json!({"agent": agent, "before": before, "limit": limit}),
        )
        .await?;
    print(json, &value, |value| {
        value["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| {
                let at = e["at"].as_u64().unwrap_or_default();
                let mut line = format!(
                    "{:>6} {} {:<10} {:<4} {:<28} {}",
                    e["seq"],
                    dmsg_client::mcp_events::timestamp(at),
                    short(e["actor"].as_str().unwrap_or_default()),
                    e["via"].as_str().unwrap_or_default(),
                    e["action"].as_str().unwrap_or_default(),
                    e["outcome"].as_str().unwrap_or_default(),
                );
                for field in ["target", "detail"] {
                    if let Some(text) = e[field].as_str() {
                        line.push(' ');
                        line.push_str(text);
                    }
                }
                line
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    Ok(())
}

/// An actor as one column: `owner`, or the end of a `did:agent:`.
fn short(actor: &str) -> String {
    match actor.strip_prefix("did:agent:") {
        Some(key) => format!("…{}", &key[key.len().saturating_sub(8)..]),
        None => actor.into(),
    }
}

/// `dmsg ap [--agent A] [--session S] <tool> [input]`: one MCP tool as an
/// Agent ID, with the same input and result, in a named connector session
/// whose presented heads persist between commands. `tools` lists them;
/// `agent_protocols_` and `dmsg_` may be left out of standard and dMsg
/// names alike.
pub async fn ap(
    remote: &Remote,
    json: bool,
    agent: Option<String>,
    session: String,
    tool: String,
    input: Option<String>,
) -> Result<(), Failure> {
    if tool == "tools" {
        let list = dmsg_agent::tools::tool_list();
        print(json, &Value::Array(list.clone()), |_| {
            list.iter()
                .map(|t| {
                    format!(
                        "{:<38} {}",
                        t["name"].as_str().unwrap_or_default(),
                        t["description"].as_str().unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        });
        return Ok(());
    }
    let name = [
        tool.clone(),
        format!("agent_protocols_{tool}"),
        format!("dmsg_{tool}"),
    ]
    .into_iter()
    .find(|n| dmsg_agent::tools::is_standard(n) || dmsg_agent::tools::is_dmsg(n))
    .ok_or_else(|| Failure::new("NOT_FOUND", format!("no tool {tool}; see `dmsg ap tools`")))?;
    let input: Value = match input {
        Some(text) => serde_json::from_str(&text)
            .map_err(|e| Failure::new("INVALID_INPUT", format!("input: {e}")))?,
        None => json!({}),
    };
    let outcome = remote
        .call(
            "ap/call",
            json!({"agent": agent, "session": session, "tool": name, "input": input}),
        )
        .await?;
    if let Some(error) = outcome.get("error") {
        return Err(Failure::new(
            error["code"].as_str().unwrap_or("invalid_request"),
            error["message"].as_str().unwrap_or_default(),
        ));
    }
    print(json, &outcome["result"], pretty);
    Ok(())
}
