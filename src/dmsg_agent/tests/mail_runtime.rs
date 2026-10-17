//! Agent Mail through the connector host over real HTTPS: an in-process
//! relay (the SDK's `MailRelayStore`) behind the public localhost test
//! certificate, which the runtime trusts as a local deployment's root.
//! The decryption keys travel between the caller and the connector and
//! never reach `agents.sqlite`; the configuration alone blocks senders.

use agent_protocols::{
    identity::{unix_ms, AgentSigner},
    mail::MailRelayStore,
    SdkError,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use dmsg_agent::{Host, Options, Runtime, Scope};
use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
    ServerConfig, ServerConnection, StreamOwned,
};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const CERT: &[u8] = include_bytes!("../../../tests/fixtures/localhost-tls/cert.der");
const KEY: &[u8] = include_bytes!("../../../tests/fixtures/localhost-tls/key.der");

/// A relay at `https://localhost:<port>`, one request per connection.
fn relay() -> (String, Arc<Mutex<MailRelayStore>>) {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!(
        "https://localhost:{}",
        listener.local_addr().unwrap().port()
    );
    let store = Arc::new(Mutex::new(MailRelayStore::new(&origin).unwrap()));
    let config = Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(CERT.to_vec())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(KEY.to_vec())),
            )
            .unwrap(),
    );
    let served = store.clone();
    std::thread::spawn(move || {
        for socket in listener.incoming() {
            let Ok(socket) = socket else { break };
            let mut stream =
                StreamOwned::new(ServerConnection::new(config.clone()).unwrap(), socket);
            let Some((method, path, jwt, body)) = read(&mut stream) else {
                continue;
            };
            let (status, text, headers) = handle(&served, &method, &path, &jwt, &body);
            let response = format!(
                "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{text}",
                text.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (origin, store)
}

fn read(stream: &mut impl Read) -> Option<(String, String, String, String)> {
    let mut input = Vec::new();
    let mut buf = [0u8; 4096];
    let end = loop {
        if let Some(pos) = input.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        let n = stream.read(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        input.extend_from_slice(&buf[..n]);
    };
    let head = String::from_utf8_lossy(&input[..end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let (method, path) = (first.next()?.to_owned(), first.next()?.to_owned());
    let (mut length, mut jwt) = (0, String::new());
    for line in lines {
        let (name, value) = line.split_once(':')?;
        match name.to_ascii_lowercase().as_str() {
            "content-length" => length = value.trim().parse().ok()?,
            "authorization" => jwt = value.trim().trim_start_matches("Bearer ").to_owned(),
            _ => {}
        }
    }
    while input.len() - end - 4 < length {
        let n = stream.read(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        input.extend_from_slice(&buf[..n]);
    }
    Some((
        method,
        path,
        jwt,
        String::from_utf8_lossy(&input[end + 4..]).into_owned(),
    ))
}

fn handle(
    store: &Mutex<MailRelayStore>,
    method: &str,
    target: &str,
    jwt: &str,
    body: &str,
) -> (u16, String, String) {
    let now = unix_ms();
    let url = url::Url::parse(&format!("https://localhost{target}")).unwrap();
    let segments: Vec<&str> = url.path().trim_start_matches('/').split('/').collect();
    let query = |key: &str| {
        url.query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
    };
    let body = || serde_json::from_str::<Value>(body).unwrap_or(Value::Null);
    let mut relay = store.lock().unwrap();
    let result: Result<(u16, Value), SdkError> = match (method, &segments[..]) {
        ("POST", ["v1", "mailboxes"]) => relay.publish(&body(), now).map(|r| (200, json!(r))),
        ("GET", ["v1", "mailboxes", id, "card"]) => relay.card(id).map(|r| (200, json!(r))),
        ("POST", ["v1", "mailboxes", id, "packets"]) => relay
            .deliver(id, &serde_json::from_value(body()).unwrap(), now)
            .map(|r| (202, json!(r))),
        ("GET", ["v1", "mailboxes", id, "packets"]) => {
            let limit = query("limit").map_or(100, |v| v.parse().unwrap());
            relay
                .list(id, jwt, now, limit, query("cursor").as_deref())
                .map(|r| (200, json!(r)))
        }
        ("DELETE", ["v1", "mailboxes", id, "packets", packet]) => {
            return match relay.delete(id, packet, jwt, now) {
                Ok(()) => (204, String::new(), String::new()),
                Err(e) => failure(e),
            }
        }
        _ => {
            return (
                404,
                r#"{"error":{"code":"not_found","message":"no route"}}"#.into(),
                String::new(),
            )
        }
    };
    match result {
        Ok((status, value)) => (status, value.to_string(), String::new()),
        Err(e) => failure(e),
    }
}

fn failure(error: SdkError) -> (u16, String, String) {
    let code = error.code().unwrap_or("invalid_request").to_owned();
    let status = match code.as_str() {
        "mailbox_unavailable" => 404,
        "stale_card" | "mailbox_conflict" | "nonce_not_greater" => 409,
        "permission_denied" => 403,
        "invalid_token" => 401,
        _ => 400,
    };
    let header = match error {
        SdkError::NonceNotGreater { max_nonce } => format!("Max-Seen-Nonce: {max_nonce}\r\n"),
        _ => String::new(),
    };
    (
        status,
        json!({"error": {"code": code, "message": "refused"}}).to_string(),
        header,
    )
}

fn pem(der: &[u8]) -> Vec<u8> {
    let body = STANDARD.encode(der);
    let lines: Vec<&str> = body
        .as_bytes()
        .chunks(64)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect();
    format!(
        "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
        lines.join("\n")
    )
    .into_bytes()
}

struct Temp(PathBuf);

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp() -> Temp {
    let dir = std::env::temp_dir().join(format!(
        "dmsg-agent-mail-{}-{}",
        std::process::id(),
        unix_ms()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    Temp(dir)
}

/// A host and the state file it keeps the connectors in.
struct Hosted(Runtime, std::path::PathBuf);

fn runtime(dir: &Temp, origin: &str, blocked: &[String]) -> Hosted {
    let options = Options {
        hosts: vec![Host {
            host: origin.into(),
            label: Some("mail".into()),
            profile_service: None,
        }],
        blocked_senders: blocked.to_vec(),
        local_roots: vec![pem(CERT)],
        local: true,
    };
    Hosted(Runtime::new(&options).unwrap(), dir.0.join("agents.sqlite"))
}

/// `seed`'s key store before it holds any key.
fn no_keys(seed: u8) -> Value {
    let agent_id = AgentSigner::from_seed([seed; 32]).agent_id();
    json!({"version": 1, "agent_id": agent_id, "keys": {}})
}

/// A call as `seed`'s Agent ID with a key store of one value.
async fn call(
    Hosted(runtime, path): &Hosted,
    seed: u8,
    keys: &mut Value,
    tool: &str,
    input: Value,
) -> Value {
    let signer = AgentSigner::from_seed([seed; 32]);
    let turn = runtime.turn(signer.agent_id().as_str()).await;
    let called = runtime
        .call(
            &turn,
            Scope {
                path,
                session: "test",
                reach: None,
            },
            signer,
            keys,
            tool,
            input,
        )
        .await
        .unwrap();
    *keys = called.mail_keys;
    called
        .outcome
        .unwrap_or_else(|e| panic!("{tool}: {} {}", e.code, e.message))
}

#[test]
fn mail_keys_stay_with_the_caller_and_the_config_blocks_senders() {
    let (origin, relay) = relay();
    let dir = temp();
    let alice = AgentSigner::from_seed([1; 32]).agent_id().to_string();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let open = runtime(&dir, &origin, &[]);
            let (mut alice_keys, mut bob_keys) = (no_keys(1), no_keys(2));
            let mailbox = call(
                &open,
                2,
                &mut bob_keys,
                "agent_protocols_mail_mailbox_publish",
                json!({"routes": [origin]}),
            )
            .await["mailbox"]
                .clone();
            let secrets: Vec<String> = bob_keys["keys"]
                .as_object()
                .unwrap()
                .values()
                .map(|s| s.as_str().unwrap().to_owned())
                .collect();
            assert_eq!(
                secrets.len(),
                1,
                "the new mailbox's key comes back to the caller"
            );
            let stored = std::fs::read(dir.0.join("agents.sqlite")).unwrap();
            assert!(!String::from_utf8_lossy(&stored).contains(&secrets[0]));

            let address = mailbox["address"].clone();
            let sent = call(
                &open,
                1,
                &mut alice_keys,
                "agent_protocols_mail_send",
                json!({"to": address, "text": "first"}),
            )
            .await;
            assert_eq!(sent["status"], "delivered");
            // Without its keys the recipient cannot open the letter, and
            // leaves it on the relay.
            let mut without = no_keys(2);
            let synced = call(
                &open,
                2,
                &mut without,
                "agent_protocols_mail_sync",
                json!({}),
            )
            .await;
            assert_eq!(synced["received"], json!([]));
            assert_eq!(synced["errors"][0]["code"], "invalid_event");
            let synced = call(
                &open,
                2,
                &mut bob_keys,
                "agent_protocols_mail_sync",
                json!({}),
            )
            .await;
            assert_eq!(synced["received"][0]["parts"][0]["text"], "first");

            // A configured block deletes the sender's packets unopened.
            let blocking = runtime(&dir, &origin, std::slice::from_ref(&alice));
            call(
                &open,
                1,
                &mut alice_keys,
                "agent_protocols_mail_send",
                json!({"to": address, "text": "blocked"}),
            )
            .await;
            let synced = call(
                &blocking,
                2,
                &mut bob_keys,
                "agent_protocols_mail_sync",
                json!({}),
            )
            .await;
            assert_eq!(synced["received"], json!([]));
            let held = relay.lock().unwrap().snapshot().unwrap();
            assert_eq!(held["mailboxes"][0]["packets"], json!([]));
            // Dropping it from the configuration lifts the block the
            // snapshot remembers.
            call(
                &open,
                1,
                &mut alice_keys,
                "agent_protocols_mail_send",
                json!({"to": address, "text": "again"}),
            )
            .await;
            let synced = call(
                &open,
                2,
                &mut bob_keys,
                "agent_protocols_mail_sync",
                json!({}),
            )
            .await;
            assert_eq!(synced["received"][0]["parts"][0]["text"], "again");
        });
}
