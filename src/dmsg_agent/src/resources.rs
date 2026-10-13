//! The MCP resources of the local connector (local connector §4.4): what an
//! agent may read of its own connector state without a tool call, and
//! subscribe to. Each is read from local state by the read-only tool that
//! shows the same thing, so a resource and its tool never disagree.

use agent_protocols::local_connector::{
    RESOURCE_DRAFTS_HELD, RESOURCE_HOSTS, RESOURCE_IDENTITY_CURRENT, RESOURCE_INBOX_PENDING,
    RESOURCE_MAIL_MAILBOXES, RESOURCE_MAIL_MESSAGES, RESOURCE_ROOMS,
    RESOURCE_ROOM_AGENT_STATUS_SUFFIX,
};
use serde_json::{json, Value};

/// The resources with a fixed URI: URI, name and description.
const FIXED: [(&str, &str, &str); 7] = [
    (
        RESOURCE_IDENTITY_CURRENT,
        "identity",
        "The active Agent ID and its non-secret configuration.",
    ),
    (
        RESOURCE_HOSTS,
        "hosts",
        "The operator's host allowlist and discovery metadata.",
    ),
    (
        RESOURCE_ROOMS,
        "rooms",
        "Locally known rooms and unread summaries.",
    ),
    (
        RESOURCE_INBOX_PENDING,
        "inbox",
        "Pending actionable inbox items.",
    ),
    (
        RESOURCE_DRAFTS_HELD,
        "drafts",
        "Held drafts waiting for an explicit decision.",
    ),
    (
        RESOURCE_MAIL_MAILBOXES,
        "mailboxes",
        "This Agent ID's mailboxes and their current cards.",
    ),
    (
        RESOURCE_MAIL_MESSAGES,
        "mail",
        "Locally stored received and sent mail.",
    ),
];

/// The per-room resources: URI suffix, name and description.
const ROOM: [(&str, &str, &str); 5] = [
    (
        "",
        "room",
        "A room's materialized state and latest hash-chain head.",
    ),
    ("/members", "members", "A room's member directory."),
    (
        "/timeline",
        "timeline",
        "The latest 100 records of a room's timeline.",
    ),
    ("/types", "types", "A room's materialized type registry."),
    (
        RESOURCE_ROOM_AGENT_STATUS_SUFFIX,
        "agent-status",
        "Current unexpired transient agent status in a room.",
    ),
];

/// One part of a room's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomPart {
    /// The room and its head.
    State,
    /// Its members.
    Members,
    /// Its latest records.
    Timeline,
    /// Its type registry.
    Types,
    /// Transient agent status.
    AgentStatus,
}

/// A resource an agent may read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resource {
    /// `agent-protocols://identity/current`.
    Identity,
    /// `agent-protocols://hosts`.
    Hosts,
    /// `agent-protocols://rooms`.
    Rooms,
    /// `agent-protocols://inbox/pending`.
    Inbox,
    /// `agent-protocols://drafts/held`.
    Drafts,
    /// `agent-protocols://mail/mailboxes`.
    Mailboxes,
    /// `agent-protocols://mail/messages`.
    Messages,
    /// `agent-protocols://rooms/{room_id}[/…]`.
    Room(String, RoomPart),
}

impl Resource {
    /// The resource a URI names.
    pub fn parse(uri: &str) -> Option<Self> {
        let fixed = [
            Self::Identity,
            Self::Hosts,
            Self::Rooms,
            Self::Inbox,
            Self::Drafts,
            Self::Mailboxes,
            Self::Messages,
        ];
        if let Some(i) = FIXED.iter().position(|(u, ..)| *u == uri) {
            return Some(fixed[i].clone());
        }
        let rest = uri.strip_prefix(RESOURCE_ROOMS)?.strip_prefix('/')?;
        let (id, part) = rest.split_once('/').unwrap_or((rest, ""));
        let part = match part {
            "" => RoomPart::State,
            "members" => RoomPart::Members,
            "timeline" => RoomPart::Timeline,
            "types" => RoomPart::Types,
            "agent-status" => RoomPart::AgentStatus,
            _ => return None,
        };
        let valid = (1..=128).contains(&id.len())
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b));
        valid.then(|| Self::Room(id.into(), part))
    }

    /// The read-only tool that shows the same state.
    pub fn tool(&self) -> &'static str {
        match self {
            Self::Identity | Self::Hosts => "agent_protocols_identity_current",
            Self::Rooms => "agent_protocols_rooms_list",
            Self::Inbox => "agent_protocols_inbox_next",
            Self::Drafts => "agent_protocols_drafts_list",
            Self::Mailboxes => "agent_protocols_mail_mailboxes_list",
            Self::Messages => "agent_protocols_mail_list",
            Self::Room(_, RoomPart::State | RoomPart::Types) => "agent_protocols_room_state",
            Self::Room(_, RoomPart::Members) => "agent_protocols_room_members_list",
            Self::Room(_, RoomPart::Timeline) => "agent_protocols_room_timeline",
            Self::Room(_, RoomPart::AgentStatus) => "agent_protocols_agent_status_list",
        }
    }
}

/// MCP `resources/list`: the fixed resources and each known room's state,
/// from the `rooms` resource's `{rooms: [{room_id, host, topic?}]}`.
pub fn resource_list(rooms: &Value) -> Vec<Value> {
    let fixed = FIXED.iter().map(|(uri, name, description)| {
        json!({"uri": uri, "name": name, "description": description, "mimeType": "application/json"})
    });
    let rooms = rooms["rooms"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|room| {
            let id = room["room_id"].as_str()?;
            let title = room["topic"].as_str().unwrap_or(id);
            Some(json!({
                "uri": format!("{RESOURCE_ROOMS}/{id}"),
                "name": format!("room {id}"),
                "title": title,
                "description": ROOM[0].2,
                "mimeType": "application/json",
            }))
        });
    fixed.chain(rooms).collect()
}

/// MCP `resources/templates/list`: the per-room resources.
pub fn resource_templates() -> Vec<Value> {
    ROOM.iter()
        .map(|(suffix, name, description)| {
            json!({
                "uriTemplate": format!("{RESOURCE_ROOMS}/{{room_id}}{suffix}"),
                "name": name,
                "description": description,
                "mimeType": "application/json",
            })
        })
        .collect()
}
