//! Agent Mail through the connector host over real HTTPS: an in-process
//! relay (the SDK's `MailRelayStore`) behind the public localhost test
//! certificate, which the runtime trusts as a local deployment's root.
//! The decryption keys travel between the caller and the connector and
//! never reach `agents.sqlite`; the configuration alone blocks senders.

use agent_protocols::{
    SdkError,
    identity::{AgentSigner, unix_ms},
    mail::MailRelayStore,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use dmsg_agent::{DurableMail, Host, Options, Runtime, Scope};
use rustls::{
    ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const CERT: &[u8] = include_bytes!("../../../tests/fixtures/localhost-tls/cert.der");
const KEY: &[u8] = include_bytes!("../../../tests/fixtures/localhost-tls/key.der");

/// Requests for the relay's Agent Discourse discovery document, which it
/// does not publish.
static DISCOVERIES: AtomicUsize = AtomicUsize::new(0);

/// A relay at `https://localhost:<port>`, one request per connection.
fn relay() -> (String, Arc<Mutex<MailRelayStore>>) {
    relay_checked(|_, _, _| {})
}

fn relay_checked(
    check: impl Fn(&str, &str, &str) + Send + Sync + 'static,
) -> (String, Arc<Mutex<MailRelayStore>>) {
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
            check(&method, &path, &body);
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
    if target == "/.well-known/agent-discourse" {
        DISCOVERIES.fetch_add(1, Ordering::SeqCst);
    }
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
        ("GET", ["v1", "me", "mailboxes"]) => relay
            .mailboxes(
                jwt,
                now,
                query("limit").map_or(100, |v| v.parse().unwrap()),
                query("cursor").as_deref(),
            )
            .map(|r| (200, json!(r))),
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
            };
        }
        _ => {
            return (
                404,
                r#"{"error":{"code":"not_found","message":"no route"}}"#.into(),
                String::new(),
            );
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

fn temp(name: &str) -> Temp {
    let dir = std::env::temp_dir().join(format!(
        "dmsg-agent-{name}-{}-{}",
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
    let dir = temp("mail");
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

#[test]
fn calls_share_one_discovery_read_per_host() {
    let (origin, _relay) = relay();
    let dir = temp("discovery");
    let Hosted(runtime, path) = runtime(&dir, &origin, &[]);
    let agent_id = AgentSigner::from_seed([1; 32]).agent_id();
    let keys = no_keys(1);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            for _ in 0..2 {
                let turn = runtime.turn(agent_id.as_str()).await;
                let scope = Scope {
                    path: &path,
                    session: "test",
                    reach: None,
                };
                let input = json!({"scope": "public", "host": origin});
                let called = runtime
                    .call(
                        &turn,
                        scope,
                        AgentSigner::from_seed([1; 32]),
                        &keys,
                        "agent_protocols_rooms_list",
                        input,
                    )
                    .await
                    .unwrap();
                // The relay serves no rooms at the default path either.
                assert!(called.outcome.is_err());
            }
        });
    // It answered that it has no document, which holds for later calls.
    assert_eq!(DISCOVERIES.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn durable_mail_recovers_after_acceptance_when_final_checkpoint_fails() {
    let dir = temp("durable-mail");
    let path = dir.0.join("agents.sqlite");
    let checked = path.clone();
    let posts = Arc::new(AtomicUsize::new(0));
    let observed = posts.clone();
    let (origin, _) = relay_checked(move |method, path, body| {
        if method == "POST" && path == "/v1/mailboxes" {
            let posted: Value = serde_json::from_str(body).unwrap();
            let conn = rusqlite::Connection::open(&checked).unwrap();
            let (snapshot,sealed):(String,String)=conn.query_row("SELECT c.snapshot,m.sealed FROM connectors c JOIN mail_checkpoints m USING(agent_id)",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
            let saved: Value = serde_json::from_str(&snapshot).unwrap();
            let mailbox = posted["event"]["payload"]["mailbox_id"].as_str().unwrap();
            assert_eq!(
                saved["mail"]["pending_publications"][mailbox]["card_hash"],
                posted["hash"]
            );
            assert_eq!(sealed, "test-only-encrypted-checkpoint");
            observed.fetch_add(1, Ordering::SeqCst);
        }
    });
    let Hosted(host, _) = runtime(&dir, &origin, &[]);
    let actor = AgentSigner::from_seed([41; 32]).agent_id();
    let turn = host.turn(actor.as_str()).await;
    let scope = Scope {
        path: &path,
        session: "durable",
        reach: None,
    };
    let captured = Arc::new(Mutex::new(Value::Null));
    let out = captured.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let result = host
        .call_durable(
            &turn,
            scope,
            AgentSigner::from_seed([41; 32]),
            DurableMail {
                keys: &no_keys(41),
                seal: Box::new(move |keys| {
                    if count.fetch_add(1, Ordering::SeqCst) > 0 {
                        return Err(dmsg_types::Error::Unavailable(
                            "injected checkpoint failure".into(),
                        ));
                    }
                    *out.lock().unwrap() = keys.clone();
                    Ok("test-only-encrypted-checkpoint".into())
                }),
            },
            "agent_protocols_mail_mailbox_publish",
            json!({"routes":[origin]}),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(posts.load(Ordering::SeqCst), 1);
    assert!(
        host.mail_checkpoint(&path, actor.as_str())
            .unwrap()
            .is_some()
    );
    let keys = captured.lock().unwrap().clone();
    let disk = std::fs::read(&path).unwrap();
    for secret in keys["keys"].as_object().unwrap().values() {
        assert!(
            !disk
                .windows(secret.as_str().unwrap().len())
                .any(|w| w == secret.as_str().unwrap().as_bytes())
        );
    }
    drop(turn);
    drop(host);
    let Hosted(restored, _) = runtime(&dir, &origin, &[]);
    let turn = restored.turn(actor.as_str()).await;
    let recovered = restored
        .call_durable(
            &turn,
            scope,
            AgentSigner::from_seed([41; 32]),
            DurableMail {
                keys: &keys,
                seal: Box::new(|_| Ok("test-only-encrypted-checkpoint".into())),
            },
            "agent_protocols_mail_sync",
            json!({}),
        )
        .await
        .unwrap();
    assert!(recovered.outcome.is_ok());
    let boxes = restored
        .inspect_mail(
            scope,
            AgentSigner::from_seed([41; 32]),
            "agent_protocols_mail_mailboxes_list",
            json!({}),
            Some(&recovered.mail_keys),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(boxes["mailboxes"][0]["key_available"], true);
    assert_eq!(boxes["mailboxes"][0]["pending_routes"], json!([]));
    // The caller commits its real key store before acknowledging the checkpoint.
    restored.clear_mail_checkpoint(&turn, &path).unwrap();
    assert!(
        restored
            .mail_checkpoint(&path, actor.as_str())
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn checkpoint_failure_before_publication_sends_nothing() {
    let dir = temp("durable-refused");
    let posts = Arc::new(AtomicUsize::new(0));
    let count = posts.clone();
    let (origin, _) = relay_checked(move |method, path, _| {
        if method == "POST" && path == "/v1/mailboxes" {
            count.fetch_add(1, Ordering::SeqCst);
        }
    });
    let Hosted(runtime, path) = runtime(&dir, &origin, &[]);
    let actor = AgentSigner::from_seed([42; 32]).agent_id();
    let turn = runtime.turn(actor.as_str()).await;
    assert!(
        runtime
            .call_durable(
                &turn,
                Scope {
                    path: &path,
                    session: "failure",
                    reach: None
                },
                AgentSigner::from_seed([42; 32]),
                DurableMail {
                    keys: &no_keys(42),
                    seal: Box::new(|_| Err(dmsg_types::Error::Unavailable("injected".into())))
                },
                "agent_protocols_mail_mailbox_publish",
                json!({"routes":[origin]})
            )
            .await
            .is_err()
    );
    assert_eq!(posts.load(Ordering::SeqCst), 0);
    assert!(
        runtime
            .mail_checkpoint(&path, actor.as_str())
            .unwrap()
            .is_none()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_removed_workspace_is_not_recreated_by_a_checkpoint() {
    let dir = temp("checkpoint-reset");
    let posts = Arc::new(AtomicUsize::new(0));
    let count = posts.clone();
    let (origin, _) = relay_checked(move |method, path, _| {
        if method == "POST" && path == "/v1/mailboxes" {
            count.fetch_add(1, Ordering::SeqCst);
        }
    });
    let Hosted(runtime, path) = runtime(&dir, &origin, &[]);
    let removed = path.clone();
    let actor = AgentSigner::from_seed([43; 32]).agent_id();
    let turn = runtime.turn(actor.as_str()).await;
    let result = runtime
        .call_durable(
            &turn,
            Scope {
                path: &path,
                session: "reset",
                reach: None,
            },
            AgentSigner::from_seed([43; 32]),
            DurableMail {
                keys: &no_keys(43),
                seal: Box::new(move |_| {
                    std::fs::remove_file(&removed).unwrap();
                    Ok("test-only-encrypted-checkpoint".into())
                }),
            },
            "agent_protocols_mail_mailbox_publish",
            json!({"routes":[origin]}),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(posts.load(Ordering::SeqCst), 0);
    assert!(!path.exists());
    assert!(
        runtime
            .mail_checkpoint(&path, actor.as_str())
            .unwrap()
            .is_none()
    );
    runtime.clear_mail_checkpoint(&turn, &path).unwrap();
    assert!(!path.exists());
}
