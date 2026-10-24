use crate::runtime::{Host, Options, Runtime, Scope};
use agent_protocols::{
    discourse::{
        MessageCreatePayload, RoomCreatePayload, RoomHead, RoomResponse, RoomState,
        RoomUpdatePayload, ServerRecord, Visibility, build_server_record, discourse_event,
        event_type, room_create_event,
    },
    identity::AgentSigner,
    local_connector::{AgentProtocolsHost, LocalConnector},
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

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

/// Agent 1's mail keys before it has any.
fn keys() -> Value {
    json!({"version": 1, "agent_id": signer(1).agent_id(), "keys": {}})
}

/// The snapshot the state file keeps for agent 1.
fn stored(path: &Path) -> Value {
    let conn = rusqlite::Connection::open(path).unwrap();
    let text: String = conn
        .query_row(
            "SELECT snapshot FROM connectors WHERE agent_id = ?1",
            [signer(1).agent_id().as_str()],
            |r| r.get(0),
        )
        .unwrap();
    serde_json::from_str(&text).unwrap()
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

async fn call(hosted: &Hosted, tool: &str, input: Value) -> Result<Value, crate::ToolError> {
    call_in(hosted, "test", tool, input).await
}

async fn call_in(
    hosted: &Hosted,
    session: &str,
    tool: &str,
    input: Value,
) -> Result<Value, crate::ToolError> {
    call_within(hosted, session, None, tool, input).await
}

async fn call_within(
    Hosted(runtime, path): &Hosted,
    session: &str,
    reach: Option<&[String]>,
    tool: &str,
    input: Value,
) -> Result<Value, crate::ToolError> {
    let turn = runtime.turn(signer(1).agent_id().as_str()).await;
    let scope = Scope {
        path,
        session,
        reach,
    };
    runtime
        .call(&turn, scope, signer(1), &keys(), tool, input)
        .await
        .unwrap()
        .outcome
}

async fn read(hosted: &Hosted, session: &str, uri: &str) -> dmsg_types::Result<Value> {
    read_within(hosted, session, None, uri).await
}

async fn read_within(
    Hosted(runtime, path): &Hosted,
    session: &str,
    reach: Option<&[String]>,
    uri: &str,
) -> dmsg_types::Result<Value> {
    let scope = Scope {
        path,
        session,
        reach,
    };
    runtime.read(scope, signer(1), uri, None).await
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
        // The remembered host is not even listed.
        let hosts = identity["hosts"].as_array().unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(
            (&hosts[0]["host"], &hosts[0]["allowed"]),
            (&json!(HOST), &json!(true))
        );
        let refused = call(
            &runtime,
            "agent_protocols_rooms_list",
            json!({"scope": "public", "host": "https://evil.example"}),
        )
        .await
        .unwrap_err();
        assert_eq!(refused.code, "permission_denied");
        // No tool adds a host, and unknown tools are refused outright.
        let turn = runtime.0.turn(signer(1).agent_id().as_str()).await;
        assert!(
            runtime
                .0
                .call(
                    &turn,
                    Scope {
                        path: &runtime.1,
                        session: "test",
                        reach: None,
                    },
                    signer(1),
                    &keys(),
                    "agent_protocols_host_add",
                    json!({"host": "https://evil.example"})
                )
                .await
                .is_err()
        );
    });
    // The state file keeps no host: every call applies the configured ones.
    assert_eq!(stored(&runtime.1)["hosts"], json!({}));
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

/// Agent 1's connector after joining `room_with_records`.
fn with_room(dir: &Temp) {
    let (room, records) = room_with_records(&signer(5));
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
    seed(dir, &connector);
}

#[test]
fn each_session_sees_the_room_move_by_itself() {
    let dir = Temp::new();
    with_room(&dir);
    let hosted = runtime(&dir);
    block_on(async {
        // The first state read of a session presents the current head.
        let state = call_in(
            &hosted,
            "a",
            "agent_protocols_room_state",
            json!({"room_id": "room1"}),
        )
        .await
        .unwrap();
        assert_eq!(state["sync"]["presented_seq"], 3);
        // Another session has not read the room, so nothing is presented
        // to it, and a write of its own defaults to the current head.
        let other = call_in(
            &hosted,
            "b",
            "agent_protocols_room_members_list",
            json!({"room_id": "room1"}),
        )
        .await
        .unwrap();
        assert!(other["sync"].get("presented_seq").is_none(), "{other}");
        // Session a keeps its view across calls; the shared snapshot keeps
        // no session's.
        let again = call_in(
            &hosted,
            "a",
            "agent_protocols_room_members_list",
            json!({"room_id": "room1"}),
        )
        .await
        .unwrap();
        assert_eq!(again["sync"]["presented_seq"], 3);
    });
    let saved = stored(&hosted.1);
    assert!(saved["rooms"][0].get("presented_seq").is_none());
}

#[test]
fn resources_read_local_state_without_saving() {
    let dir = Temp::new();
    with_room(&dir);
    let hosted = runtime(&dir);
    block_on(async {
        let identity = read(&hosted, "r", "agent-protocols://identity/current")
            .await
            .unwrap();
        assert_eq!(identity["agent_id"], signer(1).agent_id().as_str());
        let hosts = read(&hosted, "r", "agent-protocols://hosts").await.unwrap();
        assert_eq!(hosts["hosts"][0]["host"], HOST);
        let rooms = read(&hosted, "r", "agent-protocols://rooms").await.unwrap();
        assert_eq!(rooms["rooms"][0]["room_id"], "room1");
        let list = crate::resources::resource_list(&rooms);
        assert_eq!(list.len(), 8);
        assert_eq!(list[7]["uri"], "agent-protocols://rooms/room1");
        let inbox = read(&hosted, "r", "agent-protocols://inbox/pending")
            .await
            .unwrap();
        assert_eq!(inbox["items"].as_array().unwrap().len(), 2);
        let timeline = read(&hosted, "r", "agent-protocols://rooms/room1/timeline")
            .await
            .unwrap();
        assert_eq!(timeline["items"].as_array().unwrap().len(), 2);
        let room = read(&hosted, "r", "agent-protocols://rooms/room1")
            .await
            .unwrap();
        assert_eq!(room["room"]["room_id"], "room1");
        let types = read(&hosted, "r", "agent-protocols://rooms/room1/types")
            .await
            .unwrap();
        assert_eq!(types["types"], json!([]));
        let statuses = read(&hosted, "r", "agent-protocols://rooms/room1/agent-status")
            .await
            .unwrap();
        assert_eq!(statuses["statuses"], json!([]));
        for uri in [
            "agent-protocols://rooms/elsewhere",
            "agent-protocols://rooms/room1/secrets",
            "agent-protocols://nothing",
        ] {
            assert_eq!(
                read(&hosted, "r", uri).await.unwrap_err(),
                dmsg_types::Error::NotFound,
                "{uri}"
            );
        }
        // Reading presented nothing, claimed nothing and marked nothing read.
        let state = call_in(
            &hosted,
            "r",
            "agent_protocols_room_members_list",
            json!({"room_id": "room1"}),
        )
        .await
        .unwrap();
        assert!(state["sync"].get("presented_seq").is_none(), "{state}");
        assert_eq!(state["sync"]["unread_count"], 2);
        let next = call(&hosted, "agent_protocols_inbox_next", json!({}))
            .await
            .unwrap();
        assert_eq!(next["items"].as_array().unwrap().len(), 2);
    });
}

#[test]
fn a_call_reaches_only_the_origins_in_its_scope() {
    const OTHER: &str = "http://localhost:9";
    const PROFILES: &str = "http://localhost:8";
    let dir = Temp::new();
    with_room(&dir);
    let options = Options {
        hosts: vec![
            Host {
                host: HOST.into(),
                label: Some("test".into()),
                profile_service: Some(PROFILES.into()),
            },
            Host {
                host: OTHER.into(),
                label: None,
                profile_service: None,
            },
        ],
        local: true,
        ..Options::default()
    };
    let hosted = Hosted(Runtime::new(&options).unwrap(), dir.0.join("agents.sqlite"));
    assert_eq!(hosted.0.origins("discourse.read"), [HOST, OTHER]);
    assert_eq!(hosted.0.origins("mail.read"), [HOST, OTHER]);
    for capability in ["agent.profile.read", "agent.profile.write"] {
        assert_eq!(hosted.0.origins(capability), [HOST, OTHER, PROFILES]);
    }
    let here = [HOST.to_string()];
    let elsewhere = [OTHER.to_string()];
    let profiles = [PROFILES.to_string(), "https://unknown.example".to_string()];
    block_on(async {
        // Only what the scope names, each without the profile service its
        // host names, and never an origin the configuration does not list.
        for (reach, allowed) in [
            (&elsewhere[..], vec![OTHER]),
            (&profiles[..], vec![PROFILES]),
        ] {
            let identity = call_within(
                &hosted,
                "s",
                Some(reach),
                "agent_protocols_identity_current",
                json!({}),
            )
            .await
            .unwrap();
            let hosts: Vec<&Value> = identity["hosts"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|h| h["allowed"] == true)
                .collect();
            let names: Vec<&str> = hosts.iter().map(|h| h["host"].as_str().unwrap()).collect();
            assert_eq!(names, allowed);
            assert_eq!(identity["hosts"].as_array().unwrap().len(), allowed.len());
            assert!(hosts.iter().all(|h| h["profile_service"].is_null()));
        }
        // A room cached from another host is not read from local state,
        // and the known rooms of other hosts are not listed.
        for tool in [
            "agent_protocols_room_members_list",
            "agent_protocols_room_timeline",
            "agent_protocols_room_state",
            "agent_protocols_room_archive",
            "agent_protocols_room_watch",
        ] {
            let refused = call_within(
                &hosted,
                "s",
                Some(&elsewhere),
                tool,
                json!({"room_id": "room1"}),
            )
            .await
            .unwrap_err();
            assert_eq!(refused.code, "permission_denied", "{tool}");
        }
        let schema = call_within(
            &hosted,
            "s",
            Some(&elsewhere),
            "agent_protocols_schema_get",
            json!({"protocol":"agent-discourse/1.0","room_id":"room1"}),
        )
        .await
        .unwrap_err();
        assert_eq!(schema.code, "permission_denied");
        let refused = call_within(
            &hosted,
            "s",
            Some(&elsewhere),
            "agent_protocols_rooms_list",
            json!({"scope": "public", "host": HOST}),
        )
        .await
        .unwrap_err();
        assert_eq!(refused.code, "permission_denied");
        let rooms = call_within(
            &hosted,
            "s",
            Some(&elsewhere),
            "agent_protocols_rooms_list",
            json!({}),
        )
        .await
        .unwrap();
        assert_eq!(rooms["rooms"], json!([]));
        assert_eq!(
            read_within(
                &hosted,
                "s",
                Some(&elsewhere),
                "agent-protocols://rooms/room1/timeline"
            )
            .await
            .unwrap_err(),
            dmsg_types::Error::Forbidden
        );
        let rooms = read_within(&hosted, "s", Some(&elsewhere), "agent-protocols://rooms")
            .await
            .unwrap();
        assert_eq!(rooms["rooms"], json!([]));
        // The room's own host is in reach.
        let members = call_within(
            &hosted,
            "s",
            Some(&here),
            "agent_protocols_room_members_list",
            json!({"room_id": "room1"}),
        )
        .await
        .unwrap();
        assert!(members["members"].is_array());
        let rooms = call_within(
            &hosted,
            "s",
            Some(&here),
            "agent_protocols_rooms_list",
            json!({}),
        )
        .await
        .unwrap();
        assert_eq!(rooms["rooms"][0]["room_id"], "room1");
        // A held draft is its room's: out of reach, it neither shows the
        // room's changes nor drops.
        let held = call(
            &hosted,
            "agent_protocols_room_send_message",
            json!({"room_id": "room1", "content": "late", "base_seq": 1, "base_hash": HEAD}),
        )
        .await
        .unwrap();
        let draft = held["draft"]["id"].as_str().unwrap();
        // Out of reach, the lists leave it out.
        for input in [json!({}), json!({"host": HOST})] {
            let listed = call_within(
                &hosted,
                "s",
                Some(&elsewhere),
                "agent_protocols_drafts_list",
                input,
            )
            .await
            .unwrap();
            assert_eq!(listed["drafts"], json!([]));
        }
        for (reach, count) in [(&elsewhere, 0), (&here, 1)] {
            let held = read_within(&hosted, "s", Some(reach), "agent-protocols://drafts/held")
                .await
                .unwrap();
            assert_eq!(held["drafts"].as_array().unwrap().len(), count);
        }
        for (tool, input) in [
            ("agent_protocols_drafts_list", json!({"draft_id": draft})),
            (
                "agent_protocols_draft_commit",
                json!({"draft_id": draft, "action": "drop"}),
            ),
        ] {
            let refused = call_within(&hosted, "s", Some(&elsewhere), tool, input)
                .await
                .unwrap_err();
            assert_eq!(refused.code, "permission_denied", "{tool}");
        }
        let shown = call_within(
            &hosted,
            "s",
            Some(&here),
            "agent_protocols_drafts_list",
            json!({"draft_id": draft}),
        )
        .await
        .unwrap();
        assert_eq!(shown["changes"].as_array().unwrap().len(), 2);
    });
}

#[test]
fn connector_state_survives_a_restart() {
    let dir = Temp::new();
    with_room(&dir);

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
    let spent = stored(&first.1)["next_nonce"].as_u64().unwrap();
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
    assert!(stored(&second.1)["next_nonce"].as_u64().unwrap() >= spent);
}

#[test]
fn unchanged_state_is_not_written_and_turns_are_per_agent() {
    let dir = Temp::new();
    with_room(&dir);
    let Hosted(runtime, path) = runtime(&dir);
    let scope = Scope {
        path: &path,
        session: "s",
        reach: None,
    };
    let no_keys = keys();
    block_on(async {
        let turn = runtime.turn(signer(1).agent_id().as_str()).await;
        let run = |tool: &'static str, input: Value| {
            runtime.call(&turn, scope, signer(1), &no_keys, tool, input)
        };
        // The seeded snapshot names a host, which the first save drops.
        let identity = "agent_protocols_identity_current";
        assert!(run(identity, json!({})).await.unwrap().changed);
        assert!(!run(identity, json!({})).await.unwrap().changed);
        let read = json!({"room_id": "room1", "mark_read": true});
        assert!(
            run("agent_protocols_room_timeline", read)
                .await
                .unwrap()
                .changed
        );
        // Another Agent ID's turn does not wait for agent 1's, and is not
        // agent 1's to use.
        let other = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            runtime.turn(signer(2).agent_id().as_str()),
        )
        .await
        .expect("turns of different Agent IDs do not wait");
        assert!(
            runtime
                .call(&other, scope, signer(1), &no_keys, identity, json!({}))
                .await
                .is_err()
        );
    });
}

#[test]
fn sessions_are_listed_and_a_removed_agent_is_forgotten() {
    let dir = Temp::new();
    with_room(&dir);
    let hosted = runtime(&dir);
    let Hosted(runtime, path) = &hosted;
    let agent = signer(1).agent_id().to_string();
    block_on(async {
        call_in(
            &hosted,
            "older",
            "agent_protocols_identity_current",
            json!({}),
        )
        .await
        .unwrap();
        call_in(
            &hosted,
            "newer",
            "agent_protocols_identity_current",
            json!({}),
        )
        .await
        .unwrap();
        let sessions = runtime.sessions(path).unwrap().remove(&agent).unwrap();
        let names: Vec<&str> = sessions
            .iter()
            .map(|s| s["session"].as_str().unwrap())
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"older") && names.contains(&"newer"));
        assert!(
            sessions
                .iter()
                .all(|s| s["updated_at"].as_i64().unwrap() > 0)
        );

        let turn = runtime.turn(&agent).await;
        runtime.forget(&turn, path).unwrap();
        drop(turn);
        assert!(!runtime.sessions(path).unwrap().contains_key(&agent));
        let conn = rusqlite::Connection::open(path).unwrap();
        let kept: i64 = conn
            .query_row("SELECT COUNT(*) FROM connectors", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kept, 0);
        // The room it knew is gone with it.
        assert_eq!(
            read(&hosted, "older", "agent-protocols://rooms/room1")
                .await
                .unwrap_err(),
            dmsg_types::Error::NotFound
        );
    });
}

/// A value of `schema`'s type for required input `name`: the test host for
/// an origin, the first choice of an enum.
fn placeholder(name: &str, schema: &Value) -> Value {
    if let Some(choices) = schema["enum"].as_array() {
        return choices[0].clone();
    }
    match schema["type"].as_str() {
        Some("integer") => json!(1),
        Some("boolean") => json!(false),
        Some("array") => json!([placeholder(name, &schema["items"])]),
        Some("object") => required_only(schema),
        _ if name.ends_with("service") || matches!(name, "services" | "host" | "url") => {
            json!(HOST)
        }
        _ => json!("x"),
    }
}

/// An input holding only the required fields of `schema`.
fn required_only(schema: &Value) -> Value {
    let mut input = serde_json::Map::new();
    for name in schema["required"].as_array().into_iter().flatten() {
        let name = name.as_str().unwrap();
        input.insert(name.into(), placeholder(name, &schema["properties"][name]));
    }
    Value::Object(input)
}

/// The field a deserialization refusal names, if it is a missing one.
fn missing_field(message: &str) -> Option<&str> {
    let rest = message.split("missing field `").nth(1)?;
    rest.split('`').next()
}

/// Every standard tool declares its inputs (local connector Section 4.3),
/// and those are the ones the connector reads: a guard for SDK upgrades.
#[test]
fn every_declared_input_is_one_the_connector_reads() {
    let dir = Temp::new();
    let hosted = runtime(&dir);
    block_on(async {
        for tool in crate::tools::tool_list() {
            let name = tool["name"].as_str().unwrap();
            if !crate::tools::is_standard(name) {
                continue;
            }
            let schema = &tool["inputSchema"];
            let properties = schema["properties"].as_object();
            assert!(properties.is_some(), "{name} declares its inputs");
            let required: Vec<&str> = schema["required"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| r.as_str().unwrap())
                .collect();
            for field in &required {
                assert!(properties.unwrap().contains_key(*field), "{name}: {field}");
            }
            // The required fields are all the connector needs...
            if let Err(refusal) = call(&hosted, name, required_only(schema)).await {
                assert_eq!(missing_field(&refusal.message), None, "{name}: {refusal:?}");
            }
            // ... and none it can do without.
            if let Err(refusal) = call(&hosted, name, json!({})).await
                && let Some(field) = missing_field(&refusal.message)
            {
                assert!(required.contains(&field), "{name}: {field}");
            }
        }
    });
}

/// The SDK checks a Knowledge payload against its type before signing; an
/// agent learns every member it got wrong from one refusal.
#[test]
fn a_knowledge_payload_names_each_field_it_gets_wrong() {
    let dir = Temp::new();
    // Knowledge services are HTTPS only; this one refuses connections.
    let service = "https://127.0.0.1:9";
    let options = Options {
        hosts: vec![Host {
            host: service.into(),
            label: Some("knowledge".into()),
            profile_service: None,
        }],
        local: true,
        ..Options::default()
    };
    let hosted = Hosted(Runtime::new(&options).unwrap(), dir.0.join("agents.sqlite"));
    let submit = |payload: Value| json!({"services": [service], "type": "knowledge.publish", "payload": payload});
    block_on(async {
        let refused = call(
            &hosted,
            "agent_protocols_knowledge_submit",
            submit(json!({"title": "Probe", "body": "text", "license": "CC-BY-4.0"})),
        )
        .await
        .unwrap_err();
        assert_eq!(refused.code, "invalid_event");
        let message = &refused.message;
        assert!(message.contains("knowledge.publish payload: "), "{message}");
        for field in [
            "kind",
            "statement",
            "language",
            "context",
            "basis",
            "body",
            "/license",
        ] {
            assert!(message.contains(field), "{field}: {message}");
        }
        let data = refused.data.unwrap();
        assert!(
            data["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["path"] == "/license")
        );

        // A nested field, by its path.
        let mut payload = json!({
            "license": "https://creativecommons.org/licenses/by/4.0/",
            "kind": "observation", "title": "Probe", "statement": "A statement.",
            "language": "en", "basis": "A test.",
            "context": {"conditions": [], "limitations": []},
        });
        let refused = call(
            &hosted,
            "agent_protocols_knowledge_submit",
            submit(payload.clone()),
        )
        .await
        .unwrap_err();
        assert!(
            refused.message.contains("/context: \"scope\""),
            "{}",
            refused.message
        );

        // A valid payload passes to the connector, which signs it and finds
        // the service unreachable.
        payload["context"]["scope"] = json!("The test.");
        let reached = call(&hosted, "agent_protocols_knowledge_submit", submit(payload))
            .await
            .unwrap_err();
        assert_ne!(reached.code, "invalid_event", "{reached:?}");
    });
}

#[test]
fn mail_inspection_never_rewrites_the_snapshot_or_marks_messages() {
    let dir = Temp::new();
    with_room(&dir);
    let Hosted(runtime, path) = runtime(&dir);
    let before = stored(&path);
    block_on(async {
        let _turn = runtime.turn(signer(1).agent_id().as_str()).await;
        let scope = Scope {
            path: &path,
            session: "inspection",
            reach: None,
        };
        for tool in [
            "agent_protocols_mail_list",
            "agent_protocols_mail_mailboxes_list",
        ] {
            runtime
                .inspect_mail(scope, signer(1), tool, json!({}), None)
                .await
                .unwrap()
                .unwrap();
        }
        for (tool, input) in [
            ("agent_protocols_mail_list", json!({"mark_read":true})),
            ("agent_protocols_mail_list", json!({"mark_read":"false"})),
            ("agent_protocols_mail_sync", json!({})),
            ("agent_protocols_mail_mailboxes_list", json!({"host":HOST})),
        ] {
            assert_eq!(
                runtime
                    .inspect_mail(scope, signer(1), tool, input, None)
                    .await,
                Err(dmsg_types::Error::Forbidden)
            );
        }
    });
    assert_eq!(stored(&path), before);
    assert!(runtime.sessions(&path).unwrap().is_empty());
}
