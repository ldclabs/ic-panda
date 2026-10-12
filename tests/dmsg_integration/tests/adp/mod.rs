//! A minimal Agent Discourse host for the desktop agent test: public rooms,
//! direct joins, messages and topic changes, served over loopback HTTP with
//! the SDK's own record and validation rules. It checks signatures, the
//! room path, per-actor nonces and the head a write is based on; it does
//! not check request JWTs. `refuse_next_nonce` makes the next write answer
//! `nonce_not_greater` with a chosen `Max-Seen-Nonce`, as if another
//! service sharing the key had seen it.

use agent_protocols::discourse::{
    build_server_record, event_type, event_type_advances_head, validate_discourse_envelope,
    validate_room_base, validate_room_create_host, validate_room_path, RoomCreatePayload, RoomHead,
    RoomResponse, RoomState, ServerRecord,
};
use agent_protocols::identity::{verify_envelope, Envelope};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::{Arc, Mutex};

struct Room {
    create: Envelope<RoomCreatePayload>,
    records: Vec<ServerRecord>,
    head: (u64, String),
}

#[derive(Default)]
struct State {
    rooms: BTreeMap<String, Room>,
    nonces: BTreeMap<String, u64>,
    /// `Max-Seen-Nonce` the next write is refused with, and whether the
    /// host keeps it as the actor's maximum.
    refuse: Option<(u64, bool)>,
}

/// A running host.
pub struct Adp {
    pub origin: String,
    state: Arc<Mutex<State>>,
}

type Reply = (u16, Value, Option<u64>);

fn error(status: u16, code: &str, message: impl Into<String>) -> Reply {
    (
        status,
        json!({"error": {"code": code, "message": message.into()}}),
        None,
    )
}

impl Adp {
    pub fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State::default()));
        let (shared, host) = (state.clone(), origin.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (state, host) = (shared.clone(), host.clone());
                std::thread::spawn(move || {
                    let _ = serve(stream, &host, &state);
                });
            }
        });
        Self { origin, state }
    }

    /// Refuse the next write with `nonce_not_greater` and this maximum,
    /// which later writes must exceed when `remember`.
    pub fn refuse_next_nonce(&self, max_seen: u64, remember: bool) {
        self.state.lock().unwrap().refuse = Some((max_seen, remember));
    }

    /// The highest nonce accepted from an actor.
    pub fn nonce(&self, actor: &str) -> Option<u64> {
        self.state.lock().unwrap().nonces.get(actor).copied()
    }

    /// The text of every message in a room, in order.
    pub fn messages(&self, room: &str) -> Vec<String> {
        let state = self.state.lock().unwrap();
        state.rooms[room]
            .records
            .iter()
            .filter(|r| r.envelope.event.kind == event_type::MESSAGE_CREATE)
            .map(|r| {
                r.envelope.event.payload["content"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect()
    }
}

fn now() -> i64 {
    agent_protocols::identity::unix_ms()
}

fn response(host: &str, id: &str, room: &Room) -> RoomResponse {
    let last = room.records.last().unwrap();
    let payload = &room.create.event.payload;
    RoomResponse {
        id: id.into(),
        status: RoomState::Active,
        url: format!("{host}/v1/rooms/{id}"),
        creator: Some(room.create.event.actor.clone()),
        created_at: Some(room.records[0].accepted_at),
        topic: Some(payload.topic.clone()),
        agenda: None,
        guidance: None,
        visibility: Some(payload.visibility),
        start_time: Some(payload.start_time),
        end_time: Some(payload.end_time),
        tags: Vec::new(),
        language: None,
        policy: payload.policy.clone(),
        types: Vec::new(),
        seq: last.seq,
        pre_hash: last.pre_hash.clone(),
        hash: last.hash.clone(),
        accepted_at: last.accepted_at,
        head: Some(RoomHead {
            seq: room.head.0,
            hash: room.head.1.clone(),
        }),
        envelope: Some(room.create.clone()),
    }
}

/// Accept one nonce of `actor`, or say which maximum it must exceed.
fn take_nonce(state: &mut State, actor: &str, nonce: u64) -> Result<(), Reply> {
    let max = state.nonces.get(actor).copied().unwrap_or(0);
    if let Some((refused, remember)) = state.refuse.take() {
        if remember {
            state.nonces.insert(actor.into(), refused.max(max));
        }
        return Err((
            409,
            json!({"error": {"code": "nonce_not_greater", "message": "nonce"}}),
            Some(refused),
        ));
    }
    if nonce <= max {
        return Err((
            409,
            json!({"error": {"code": "nonce_not_greater", "message": "nonce"}}),
            Some(max),
        ));
    }
    state.nonces.insert(actor.into(), nonce);
    Ok(())
}

fn route(method: &str, path: &str, body: &[u8], host: &str, state: &Mutex<State>) -> Reply {
    let path = path.split('?').collect::<Vec<_>>();
    let query = path.get(1).copied().unwrap_or_default();
    let segments: Vec<&str> = path[0].trim_matches('/').split('/').collect();
    let mut state = state.lock().unwrap();
    match (method, segments.as_slice()) {
        ("POST", ["v1", "rooms"]) => {
            let Ok(envelope) = serde_json::from_slice::<Envelope<RoomCreatePayload>>(body) else {
                return error(400, "invalid_event", "room.create");
            };
            if verify_envelope(&envelope).is_err()
                || validate_room_create_host(&envelope.event.payload, host).is_err()
            {
                return error(400, "invalid_event", "room.create");
            }
            if let Err(reply) = take_nonce(
                &mut state,
                envelope.event.actor.as_str(),
                envelope.event.nonce,
            ) {
                return reply;
            }
            let id = format!("room{}", state.rooms.len() + 1);
            let record = build_server_record(
                id.clone(),
                1,
                None,
                now(),
                serde_json::from_value::<Envelope<Value>>(serde_json::to_value(&envelope).unwrap())
                    .unwrap(),
            )
            .unwrap();
            let room = Room {
                create: envelope,
                head: (1, record.hash.clone()),
                records: vec![record],
            };
            let reply = serde_json::to_value(response(host, &id, &room)).unwrap();
            state.rooms.insert(id, room);
            (201, reply, None)
        }
        ("GET", ["v1", "rooms", id]) => match state.rooms.get(*id) {
            Some(room) => (
                200,
                serde_json::to_value(response(host, id, room)).unwrap(),
                None,
            ),
            None => error(404, "room_not_found", "room"),
        },
        ("GET", ["v1", "rooms", id, "events"]) => match state.rooms.get(*id) {
            Some(room) => {
                let after: u64 = query
                    .split('&')
                    .find_map(|p| p.strip_prefix("after_seq="))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let records: Vec<&ServerRecord> =
                    room.records.iter().filter(|r| r.seq > after).collect();
                (200, json!({"result": records}), None)
            }
            None => error(404, "room_not_found", "room"),
        },
        ("POST", ["v1", "rooms", id]) => {
            let Ok(envelope) = serde_json::from_slice::<Envelope<Value>>(body) else {
                return error(400, "invalid_event", "event");
            };
            if verify_envelope(&envelope).is_err()
                || validate_discourse_envelope(&envelope).is_err()
                || validate_room_path(&envelope, id).is_err()
            {
                return error(400, "invalid_event", "event");
            }
            let Some(room) = state.rooms.get(*id) else {
                return error(404, "room_not_found", "room");
            };
            let kind = envelope.event.kind.clone();
            if kind == event_type::ROOM_UPDATE && envelope.event.actor != room.create.event.actor {
                return error(
                    403,
                    "permission_denied",
                    "only the creator updates the room",
                );
            }
            if let (Some(seq), Some(hash)) = (envelope.event.base_seq, &envelope.event.base_hash) {
                let anchor = room
                    .records
                    .iter()
                    .find(|r| r.seq == seq)
                    .map(|r| r.hash.as_str());
                if let Err(e) = validate_room_base(&kind, &[], seq, hash, anchor, room.head.0) {
                    let code = e.code().unwrap_or("room_head_mismatch").to_string();
                    return error(409, &code, e.to_string());
                }
            }
            let actor = envelope.event.actor.to_string();
            if let Err(reply) = take_nonce(&mut state, &actor, envelope.event.nonce) {
                return reply;
            }
            let room = state.rooms.get_mut(*id).unwrap();
            let last = room.records.last().unwrap();
            let record =
                build_server_record(*id, last.seq + 1, Some(last.hash.clone()), now(), envelope)
                    .unwrap();
            if event_type_advances_head(&kind, &[]) {
                room.head = (record.seq, record.hash.clone());
            }
            room.records.push(record.clone());
            (201, serde_json::to_value(record).unwrap(), None)
        }
        _ => error(404, "not_found", path[0]),
    }
}

fn serve(stream: std::net::TcpStream, host: &str, state: &Mutex<State>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut head = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        head.push_str(&line);
        if line == "\r\n" {
            break;
        }
    }
    let length = head
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    let mut request = head.lines().next().unwrap_or_default().split(' ');
    let (method, path) = (request.next().unwrap_or(""), request.next().unwrap_or(""));
    let (status, value, max_seen) = route(method, path, &body, host, state);
    let text = value.to_string();
    let nonce = max_seen
        .map(|m| format!("Max-Seen-Nonce: {m}\r\n"))
        .unwrap_or_default();
    let mut stream = stream;
    write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{nonce}Connection: close\r\n\r\n{text}",
        text.len()
    )?;
    stream.flush()
}
