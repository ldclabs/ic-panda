//! `dmsg events`: the owner's view of the MCP Events relay endpoints, with
//! the same shapes the `dmsg_events_*` tools return, for scripts and tests.
//! An agent creates and reads its own endpoints through `dmsg mcp`.

use crate::remote::{Failure, Remote};
use crate::{pretty, print};
use clap::Subcommand;
use serde_json::{Value, json};

#[derive(Subcommand)]
pub enum EventsCommand {
    /// Webhook endpoints on the relay.
    #[command(subcommand)]
    Endpoint(EndpointCommand),
    /// Register the upstream subscription behind an endpoint.
    Bind {
        endpoint: String,
        /// The upstream MCP server's URL; its origin may publish v1a keys.
        #[arg(long)]
        server: Option<String>,
        /// The upstream subscription ID.
        #[arg(long)]
        subscription: Option<String>,
        /// The upstream's refreshBefore: Unix milliseconds or RFC 3339.
        #[arg(long)]
        refresh_before: Option<String>,
        /// Replace context.session.
        #[arg(long)]
        session: Option<String>,
    },
    /// Deliveries after a cursor, waiting for one up to `--wait` ms.
    Poll {
        #[arg(long)]
        after: Option<String>,
        /// An endpoint; repeat for more. Every endpoint by default.
        #[arg(long = "endpoint")]
        endpoints: Vec<String>,
        #[arg(long)]
        session: Option<String>,
        #[arg(long, default_value_t = 0)]
        wait: u64,
    },
    /// Print deliveries as they arrive, one JSON line each, until stopped.
    Stream {
        #[arg(long)]
        after: Option<String>,
        #[arg(long = "endpoint")]
        endpoints: Vec<String>,
        #[arg(long)]
        session: Option<String>,
    },
    /// Forget deliveries up to a cursor.
    Ack {
        through: String,
        #[arg(long = "endpoint")]
        endpoints: Vec<String>,
        #[arg(long)]
        session: Option<String>,
    },
    /// Whether deliveries are being fetched, and the last fetch.
    Status,
}

#[derive(Subcommand)]
pub enum EndpointCommand {
    /// A new endpoint; prints its URL and secret once.
    Create {
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        session: Option<String>,
        /// The consuming agent, as it names itself.
        #[arg(long)]
        agent: Option<String>,
    },
    /// Every endpoint of this device.
    List,
    /// Delete an endpoint; the relay answers 410 from now on.
    Delete { endpoint: String },
    /// A new secret; the previous one verifies for 24 more hours.
    Rotate { endpoint: String },
}

fn selection(endpoints: Vec<String>, session: Option<String>) -> Value {
    json!({"endpoint_ids": endpoints, "session": session})
}

pub async fn events(remote: &Remote, json: bool, command: EventsCommand) -> Result<(), Failure> {
    let (method, params) = match command {
        EventsCommand::Endpoint(EndpointCommand::Create {
            label,
            session,
            agent,
        }) => {
            let context: serde_json::Map<String, Value> =
                [("label", label), ("session", session), ("agent", agent)]
                    .into_iter()
                    .filter_map(|(k, v)| Some((k.to_string(), Value::String(v?))))
                    .collect();
            ("mcp-events/create", json!({"context": context}))
        }
        EventsCommand::Endpoint(EndpointCommand::List) => ("mcp-events/list", Value::Null),
        EventsCommand::Endpoint(EndpointCommand::Delete { endpoint }) => {
            ("mcp-events/delete", json!({"endpoint_id": endpoint}))
        }
        EventsCommand::Endpoint(EndpointCommand::Rotate { endpoint }) => {
            ("mcp-events/rotate", json!({"endpoint_id": endpoint}))
        }
        EventsCommand::Bind {
            endpoint,
            server,
            subscription,
            refresh_before,
            session,
        } => {
            let refresh_before = refresh_before.map(|t| match t.parse::<u64>() {
                Ok(ms) => Value::from(ms),
                Err(_) => Value::from(t),
            });
            (
                "mcp-events/bind",
                json!({
                    "endpoint_id": endpoint,
                    "server": server,
                    "subscription_id": subscription,
                    "refresh_before": refresh_before,
                    "session": session,
                }),
            )
        }
        EventsCommand::Poll {
            after,
            endpoints,
            session,
            wait,
        } => {
            let mut params = selection(endpoints, session);
            params["cursor"] = after.into();
            params["wait_ms"] = wait.into();
            ("mcp-events/poll", params)
        }
        EventsCommand::Stream {
            after,
            endpoints,
            session,
        } => {
            let mut params = selection(endpoints, session);
            params["cursor"] = after.into();
            params["wait_ms"] = crate::mcp_events::MAX_WAIT_MS.into();
            loop {
                let page = remote.call("mcp-events/poll", params.clone()).await?;
                if page["truncated"] == true {
                    eprintln!(
                        "MCP event history was truncated by local retention; some deliveries cannot be replayed"
                    );
                }
                for event in page["events"].as_array().into_iter().flatten() {
                    println!("{event}");
                }
                params["cursor"] = page["cursor"].clone();
            }
        }
        EventsCommand::Ack {
            through,
            endpoints,
            session,
        } => {
            let mut params = selection(endpoints, session);
            params["through"] = through.into();
            ("mcp-events/ack", params)
        }
        EventsCommand::Status => ("mcp-events/status", Value::Null),
    };
    let value = remote.call(method, params).await?;
    print(json, &value, pretty);
    Ok(())
}
