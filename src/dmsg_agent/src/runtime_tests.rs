use crate::runtime::{Host, Options, Runtime};
use agent_protocols::{
    discourse::{
        build_server_record, discourse_event, event_type, room_create_event, MessageCreatePayload,
        RoomCreatePayload, RoomHead, RoomResponse, RoomState, RoomUpdatePayload, ServerRecord,
        Visibility,
    },
    identity::AgentSigner,
    local_connector::{AgentProtocolsHost, LocalConnector},
};
use serde_json::{json, Value};
use std::path::PathBuf;

/// Refuses connections at once, so a call that reaches the network fails fast.
const HOST: &str = "http://127.0.0.1:9";
const HEAD: &str = "room-create-head";

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "dmsg-agent-{}-{}-{}",
            std::process::id(),
            agent_protocols::identity::unix_ms(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn signer(byte: u8) -> AgentSigner {
    AgentSigner::from_seed([byte; 32])
}

/// A host and the state file it keeps the connectors in.
struct Hosted(Runtime, PathBuf);

fn runtime(dir: &Temp) -> Hosted {
    let host = Host {
        host: HOST.into(),
        label: Some("test".into()),
        profile_service: None,
    };
    let options = Options {
        hosts: vec![host],
        local: true,
        ..Options::default()
    };
    Hosted(Runtime::new(&options).unwrap(), dir.0.join("agents.sqlite"))
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

/// A public room on `HOST` created by `creator`, then from it a message
/// that mentions agent 1 (seq 2) and a topic change that moves the head
/// (seq 3; messages never move it).
fn room_with_records(creator: &AgentSigner) -> (RoomResponse, Vec<ServerRecord>) {
    let envelope = creator
        .sign_event(room_create_event(
            creator.agent_id(),
            100,
            1,
            RoomCreatePayload::new(HOST, "Room", Visibility::Public, 1, 2),
        ))
        .unwrap();
    let room = RoomResponse {
        id: "room1".into(),
        status: RoomState::Active,
        url: format!("{HOST}/v1/rooms/room1"),
        creator: None,
        created_at: None,
        topic: Some("Room".into()),
        agenda: None,
        guidance: None,
        visibility: Some(Visibility::Public),
        start_time: Some(1),
        end_time: Some(2),
        tags: Vec::new(),
        language: None,
        policy: None,
        types: Vec::new(),
        seq: 1,
        pre_hash: None,
        hash: HEAD.into(),
        accepted_at: 100,
        head: Some(RoomHead {
            seq: 1,
            hash: HEAD.into(),
        }),
        envelope: Some(envelope),
    };
    let mut message = discourse_event(
        event_type::MESSAGE_CREATE,
        creator.agent_id(),
        102,
        2,
        "room1",
        1,
        HEAD,
        serde_json::to_value(MessageCreatePayload::new(
            "text/plain",
            Value::String("hello agent".into()),
        ))
        .unwrap(),
    );
    message.mentions = Some(vec![signer(1).agent_id()]);
    let message = build_server_record(
        "room1",
        2,
        Some(HEAD.into()),
        102,
        creator.sign_event(message).unwrap(),
    )
    .unwrap();
    let update = discourse_event(
        event_type::ROOM_UPDATE,
        creator.agent_id(),
        103,
        3,
        "room1",
        1,
        HEAD,
        serde_json::to_value(RoomUpdatePayload {
            topic: Some("New topic".into()),
            ..RoomUpdatePayload::default()
        })
        .unwrap(),
    );
    let update = build_server_record(
        "room1",
        3,
        Some(message.hash.clone()),
        103,
        creator.sign_event(update).unwrap(),
    )
    .unwrap();
    (room, vec![message, update])
}

/// Store a connector's snapshot as the runtime would.
fn seed(dir: &Temp, connector: &LocalConnector) {
    let conn = rusqlite::Connection::open(dir.0.join("agents.sqlite")).unwrap();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS connectors (agent_id TEXT PRIMARY KEY, snapshot TEXT NOT NULL, updated_at INTEGER NOT NULL);",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO connectors VALUES (?1, ?2, 0)",
        rusqlite::params![
            connector.agent_id().as_str(),
            connector.snapshot().unwrap().to_string()
        ],
    )
    .unwrap();
}

async fn call(
    Hosted(runtime, path): &Hosted,
    tool: &str,
    input: Value,
) -> Result<Value, crate::ToolError> {
    let turn = runtime.turn().await;
    runtime
        .call(&turn, path, signer(1), None, tool, input)
        .await
        .unwrap()
        .outcome
}

#[test]
fn only_configured_hosts_are_allowed() {
    let dir = Temp::new();
    let mut connector = LocalConnector::new(signer(1));
    // A snapshot that claims another host is allowed (DT02).
    connector.add_host(AgentProtocolsHost {
        host: "https://evil.example".into(),
        label: None,
        allowed: true,
        features: Vec::new(),
        profile_service: None,
        last_checked_at: None,
    });
    seed(&dir, &connector);
    let runtime = runtime(&dir);
    block_on(async {
        let identity = call(&runtime, "agent_protocols_identity_current", json!({}))
            .await
            .unwrap();
        let hosts = identity["hosts"].as_array().unwrap();
        let allowed: Vec<&str> = hosts
            .iter()
            .filter(|h| h["allowed"] == true)
            .map(|h| h["host"].as_str().unwrap())
            .collect();
        assert_eq!(allowed, [HOST]);
        let refused = call(
            &runtime,
            "agent_protocols_rooms_list",
            json!({"scope": "public", "host": "https://evil.example"}),
        )
        .await
        .unwrap_err();
        assert_eq!(refused.code, "permission_denied");
        // No tool adds a host, and unknown tools are refused outright.
        let turn = runtime.0.turn().await;
        assert!(runtime
            .0
            .call(
                &turn,
                &runtime.1,
                signer(1),
                None,
                "agent_protocols_host_add",
                json!({"host": "https://evil.example"})
            )
            .await
            .is_err());
    });
    // Configuration is the only source: a host that is not an origin, or
    // loopback HTTP outside a local deployment, is refused.
    for (host, local) in [
        ("https://api.example/path", true),
        ("http://127.0.0.1:9", false),
        ("http://example.com", true),
    ] {
        let options = Options {
            hosts: vec![Host {
                host: host.into(),
                label: None,
                profile_service: None,
            }],
            local,
            ..Options::default()
        };
        assert!(Runtime::new(&options).is_err());
    }
    // A trusted root belongs to a local deployment only, and a blocked
    // sender must be an Agent ID.
    for options in [
        Options {
            local_roots: vec![b"-----BEGIN CERTIFICATE-----".to_vec()],
            ..Options::default()
        },
        Options {
            blocked_senders: vec!["someone".into()],
            ..Options::default()
        },
    ] {
        assert!(Runtime::new(&options).is_err());
    }
}

#[test]
fn connector_state_survives_a_restart() {
    let dir = Temp::new();
    let creator = signer(5);
    let (room, records) = room_with_records(&creator);
    let mut connector = LocalConnector::new(signer(1));
    connector.add_host(AgentProtocolsHost {
        host: HOST.into(),
        label: None,
        allowed: true,
        features: Vec::new(),
        profile_service: None,
        last_checked_at: None,
    });
    connector.accept_room_response(HOST, room);
    for record in records {
        connector.apply_host_record(HOST, record).unwrap();
    }
    seed(&dir, &connector);

    let first = runtime(&dir);
    let draft = block_on(async {
        // The mention and the topic change are inbox items; claiming
        // leases them.
        let next = call(&first, "agent_protocols_inbox_next", json!({"claim": true}))
            .await
            .unwrap();
        assert_eq!(next["items"].as_array().unwrap().len(), 2);
        // Reading the timeline marks it read.
        let read = call(
            &first,
            "agent_protocols_room_timeline",
            json!({"room_id": "room1", "mark_read": true}),
        )
        .await
        .unwrap();
        assert_eq!(read["unread_count"], 0);
        // A reply based on the room's first head is held: the topic change
        // moved the head past it (DT07), before any write reaches the network.
        let held = call(
            &first,
            "agent_protocols_room_send_message",
            json!({"room_id": "room1", "content": "late", "base_seq": 1, "base_hash": HEAD}),
        )
        .await
        .unwrap();
        assert_eq!(held["status"], "held");
        assert_eq!(held["changes"].as_array().unwrap().len(), 2);
        // A write against the current head signs, then fails to reach the
        // host: the nonce it used is spent either way.
        let failed = call(
            &first,
            "agent_protocols_room_send_message",
            json!({"room_id": "room1", "content": "now"}),
        )
        .await
        .unwrap_err();
        assert_eq!(failed.code, "unavailable");
        held["draft"]["id"].as_str().unwrap().to_string()
    });
    let spent = first
        .0
        .snapshot(&first.1, signer(1).agent_id().as_str())
        .unwrap()
        .unwrap()["next_nonce"]
        .as_u64()
        .unwrap();
    assert!(spent > agent_protocols::identity::unix_ms() as u64 - 60_000);
    drop(first);

    // A restarted daemon restores the draft, the lease, the read position
    // and the nonce (DT09).
    let second = runtime(&dir);
    block_on(async {
        let drafts = call(&second, "agent_protocols_drafts_list", json!({}))
            .await
            .unwrap();
        assert_eq!(drafts["drafts"][0]["id"], draft.as_str());
        let next = call(&second, "agent_protocols_inbox_next", json!({}))
            .await
            .unwrap();
        assert_eq!(next["items"], json!([]), "the claim's lease still holds");
        let timeline = call(
            &second,
            "agent_protocols_room_timeline",
            json!({"room_id": "room1", "unread_only": true}),
        )
        .await
        .unwrap();
        assert_eq!(timeline["items"], json!([]));
        let dropped = call(
            &second,
            "agent_protocols_draft_commit",
            json!({"draft_id": draft, "action": "drop"}),
        )
        .await
        .unwrap();
        assert_eq!(dropped["status"], "dropped");
    });
    let restored = second
        .0
        .snapshot(&second.1, signer(1).agent_id().as_str())
        .unwrap()
        .unwrap();
    assert!(restored["next_nonce"].as_u64().unwrap() >= spent);
}
