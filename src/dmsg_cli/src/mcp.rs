//! `dmsg mcp`: a stdio MCP server for one hosted Agent ID. Each line on
//! stdin is a JSON-RPC message; each reply is one line on stdout.
//!
//! The process holds no key and no state: it presents the agent's own
//! credential (`DMSG_AGENT_TOKEN`, issued by `dmsg agent id credential`) to
//! the daemon, which runs the Agent ID's connector and checks its
//! capabilities. The owner credential is never read, so no tool reaches an
//! approval, the host allowlist or another Agent ID.

use crate::home::Home;
use crate::remote::{Failure, Remote};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// MCP revisions this server speaks, newest first.
const PROTOCOL_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// Serve MCP on stdin and stdout until stdin closes.
pub async fn run(home: &Home) -> Result<(), Failure> {
    let credential = std::env::var("DMSG_AGENT_TOKEN").map_err(|_| {
        Failure::new(
            "AUTH_REQUIRED",
            "set DMSG_AGENT_TOKEN to the credential from `dmsg agent id credential <agent>`",
        )
    })?;
    let remote = Remote::agent(home, &credential)?;
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| Failure::new("UNAVAILABLE", e.to_string()))?
    {
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            // Notifications (initialized, cancelled) need no answer.
            Ok(message) if message.get("id").is_none() => continue,
            Ok(message) => answer(&remote, &message).await,
            Err(_) => {
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "parse error"}})
            }
        };
        let mut text = reply.to_string();
        text.push('\n');
        stdout
            .write_all(text.as_bytes())
            .await
            .map_err(|e| Failure::new("UNAVAILABLE", e.to_string()))?;
        stdout
            .flush()
            .await
            .map_err(|e| Failure::new("UNAVAILABLE", e.to_string()))?;
    }
    Ok(())
}

async fn answer(remote: &Remote, message: &Value) -> Value {
    let id = message["id"].clone();
    let params = &message["params"];
    let result = match message["method"].as_str().unwrap_or_default() {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or_default();
            let version = PROTOCOL_VERSIONS
                .iter()
                .find(|v| **v == asked)
                .unwrap_or(&PROTOCOL_VERSIONS[1]);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "dmsg", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Agent Protocols tools act as this agent's own Agent ID; dmsg_* tools reach the owner's dMsg account only as far as the owner granted. Sensitive actions only create approval requests for the owner.",
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => remote
            .call("mcp/tools", Value::Null)
            .await
            .map(|v| json!({"tools": v["tools"]})),
        "tools/call" => {
            let input = match &params["arguments"] {
                Value::Null => json!({}),
                arguments => arguments.clone(),
            };
            Ok(
                match remote
                    .call("mcp/call", json!({"tool": params["name"], "input": input}))
                    .await
                {
                    Ok(outcome) => tool_result(outcome),
                    // The daemon refused the call itself, such as while locked.
                    Err(failure) => tool_result(
                        json!({"error": {"code": failure.code, "message": failure.message}}),
                    ),
                },
            )
        }
        _ => {
            return json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "method not found"}})
        }
    };
    match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err(failure) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32603, "message": failure.message, "data": {"code": failure.code}},
        }),
    }
}

/// An MCP tool result from the daemon's `{"result"}` or `{"error"}`.
fn tool_result(outcome: Value) -> Value {
    let error = outcome.get("error").cloned();
    let value = error.clone().unwrap_or_else(|| outcome["result"].clone());
    let mut result = json!({
        "content": [{"type": "text", "text": value.to_string()}],
        "isError": error.is_some(),
    });
    if error.is_some() {
        result["structuredContent"] = json!({"error": value});
    } else if value.is_object() {
        result["structuredContent"] = value;
    }
    result
}
