//! The MCP tool catalog `dmsg mcp` serves: the 23 standard Agent Protocols
//! local connector tools and the `dmsg_*` tools over this account's data
//! (plan §6.8). A `dmsg_*` call needs a capability the owner granted the
//! agent (§6.9); a sensitive one only creates an approval request, which
//! no tool can answer.

use agent_protocols::local_connector::{standard_tool_definitions, TOOL_NAMES};
use serde_json::{json, Value};

/// The `dmsg_*` tools: name, description, read-only, input schema.
const DMSG_TOOLS: [(&str, &str, bool, &str); 10] = [
    (
        "dmsg_account_status",
        "This account's lock, sync and approval state; needs account.read.",
        true,
        r#"{"type":"object","properties":{}}"#,
    ),
    (
        "dmsg_vault_list",
        "Titles, types and tags of vault items, optionally matching a query; needs vault.list.",
        true,
        r#"{"type":"object","properties":{"query":{"type":"string"}}}"#,
    ),
    (
        "dmsg_vault_read",
        "One vault item without its secret field; needs vault.read:<tag> for one of its tags.",
        true,
        r#"{"type":"object","required":["id"],"properties":{"id":{"type":"string"}}}"#,
    ),
    (
        "dmsg_channel_list",
        "The channels this agent may read; needs channel.read:<channel>.",
        true,
        r#"{"type":"object","properties":{}}"#,
    ),
    (
        "dmsg_channel_read",
        "Messages of a channel after a sequence; needs channel.read:<channel>.",
        true,
        r#"{"type":"object","required":["channel"],"properties":{"channel":{"type":"string"},"after":{"type":"integer","minimum":0}}}"#,
    ),
    (
        "dmsg_channel_publish",
        "Send a text message to a channel as this device; needs channel.publish:<channel>.",
        false,
        r#"{"type":"object","required":["channel","text"],"properties":{"channel":{"type":"string"},"text":{"type":"string"}}}"#,
    ),
    (
        "dmsg_inbox_list",
        "Letters received in the inbox; needs inbox.read.",
        true,
        r#"{"type":"object","properties":{}}"#,
    ),
    (
        "dmsg_profile_read",
        "This account's public profile, or another account's with its signature checked.",
        true,
        r#"{"type":"object","properties":{"account":{"type":"string"}}}"#,
    ),
    (
        "dmsg_attest_request",
        "Ask the owner to attest a text statement with this device; returns the approval request, never the decision. Needs attest.request.",
        false,
        r#"{"type":"object","required":["text"],"properties":{"text":{"type":"string"},"title":{"type":"string"}}}"#,
    ),
    (
        "dmsg_approval_status",
        "The state of an approval request this agent created.",
        true,
        r#"{"type":"object","required":["id"],"properties":{"id":{"type":"string"}}}"#,
    ),
];

/// Whether `name` is one of the standard connector tools.
pub fn is_standard(name: &str) -> bool {
    TOOL_NAMES.contains(&name)
}

/// Whether `name` is one of the `dmsg_*` tools.
pub fn is_dmsg(name: &str) -> bool {
    DMSG_TOOLS.iter().any(|(n, ..)| *n == name)
}

/// Every tool as MCP `tools/list` declares it.
pub fn tool_list() -> Vec<Value> {
    let standard = standard_tool_definitions().into_iter().map(|t| {
        json!({
            "name": t.name,
            "description": t.description,
            "inputSchema": t.input_schema,
            "annotations": {
                "readOnlyHint": t.annotations.read_only_hint,
                "idempotentHint": t.annotations.idempotent_hint,
                "destructiveHint": t.annotations.destructive_hint,
                "openWorldHint": t.annotations.open_world_hint,
            },
        })
    });
    let own = DMSG_TOOLS
        .iter()
        .map(|(name, description, read_only, schema)| {
            json!({
                "name": name,
                "description": description,
                "inputSchema": serde_json::from_str::<Value>(schema).expect("tool schema is JSON"),
                "annotations": {
                    "readOnlyHint": read_only,
                    "idempotentHint": read_only,
                    "destructiveHint": false,
                    "openWorldHint": false,
                },
            })
        });
    standard.chain(own).collect()
}
