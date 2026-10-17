//! Desktop acceptance (work packages W1–W4a): two `dmsg` daemons with
//! their own homes share one account against the real canisters (PocketIC)
//! and the private relay (workerd); web requests come from the headless
//! bridge page (`src/dmsg_connect/scripts/headless.ts`).
//!
//! The test plays the dmsg.net login page: it reads the daemon's request from
//! the URL fragment, signs a delegation to the session key with a generated
//! Ed25519 login, and posts it through the daemon's callback exactly as the
//! callback page does. No daemon code path is test-only.
//!
//! Approvals are answered through the daemon's `approval/respond` with
//! `human: true`, as the desktop approval window does.
//!
//! Needs the dMsg Wasm (`make build-dmsg`), the `dmsg` binary
//! (`cargo build -p dmsg_cli`, or `DMSG_BIN`), `POCKET_IC_BIN`,
//! `DMSG_CLOUD_DIR` pointing at the private relay checkout, and Node with
//! `pnpm install` done for `src/dmsg_connect`.
mod support;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use candid::Principal;
use dmsg_protocol::{canonical, login::decode_request};
use dmsg_types::login::{LoginCallback, LoginResult};
use ic_agent::Identity;
use ic_agent::identity::{BasicIdentity, Delegation};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Children stopped when the test ends, however it ends: SIGTERM first, so
/// the relay launcher stops its workerd and daemons lock, then SIGKILL.
struct Children(Vec<Child>);

impl Drop for Children {
    fn drop(&mut self) {
        for child in &self.0 {
            let _ = Command::new("kill")
                .args(["-TERM", &child.id().to_string()])
                .status();
        }
        let started = Instant::now();
        for child in &mut self.0 {
            while started.elapsed() < Duration::from_secs(10) {
                if let Ok(Some(_)) = child.try_wait() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn wait_for(path: &Path, children: &mut Children) -> Value {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(120) {
        if let Ok(bytes) = std::fs::read(path)
            && let Ok(value) = serde_json::from_slice(&bytes)
        {
            return value;
        }
        for child in &mut children.0 {
            if let Ok(Some(status)) = child.try_wait() {
                panic!("a helper exited early: {status}");
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timeout waiting for {}", path.display());
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

struct Desktop {
    bin: PathBuf,
    home: PathBuf,
}

impl Desktop {
    /// Start this home's daemon and wait until it listens.
    fn spawn(&self, children: &mut Children) -> usize {
        children.0.push(self.launch());
        wait_for(&self.home.join("daemon.json"), children);
        children.0.len() - 1
    }

    fn launch(&self) -> Child {
        let _ = std::fs::remove_file(self.home.join("daemon.json"));
        // `DMSG_TEST_LOGS=1` shows the daemons' diagnostics.
        let logs = std::env::var_os("DMSG_TEST_LOGS").is_some();
        Command::new(&self.bin)
            .arg("--home")
            .arg(&self.home)
            .arg("daemon")
            .stderr(if logs {
                Stdio::inherit()
            } else {
                Stdio::null()
            })
            .spawn()
            .unwrap()
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(&self.bin);
        command
            .arg("--home")
            .arg(&self.home)
            .arg("--json")
            .args(args);
        command
    }

    /// Run a command that must succeed; its last stdout line is JSON.
    fn run(&self, args: &[&str]) -> Value {
        self.run_with(args, None)
    }

    fn run_with(&self, args: &[&str], stdin: Option<&str>) -> Value {
        let mut child = self
            .command(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.unwrap_or_default().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            output.status.success(),
            "dmsg {args:?} failed: {stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_str(stdout.lines().last().unwrap()).unwrap()
    }

    /// Run a command that must fail; returns its error code.
    fn fail(&self, args: &[&str]) -> String {
        let output = self.command(args).output().unwrap();
        assert!(!output.status.success(), "dmsg {args:?} succeeded");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let value: Value = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
        value["error"]["code"].as_str().unwrap().to_string()
    }

    fn token(&self) -> String {
        if let Ok(token) = std::fs::read_to_string(self.home.join("owner.token")) {
            return token.trim().into();
        }
        // In the system credential store: read it as the desktop app does.
        let output = Command::new(&self.bin)
            .arg("--home")
            .arg(&self.home)
            .arg("app-token")
            .output()
            .unwrap();
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["token"]
            .as_str()
            .unwrap()
            .into()
    }

    fn port(&self) -> u16 {
        let discovery: Value =
            serde_json::from_slice(&std::fs::read(self.home.join("daemon.json")).unwrap()).unwrap();
        discovery["port"].as_u64().unwrap() as u16
    }

    /// The app WebSocket with the owner credential; a read waits at most a
    /// minute.
    fn app_socket(&self) -> AppSocket {
        use tungstenite::client::IntoClientRequest;
        let mut request = format!("ws://127.0.0.1:{}/ws/app/v1", self.port())
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", self.token()).parse().unwrap(),
        );
        let socket = tungstenite::connect(request).unwrap().0;
        if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_ref() {
            stream
                .set_read_timeout(Some(Duration::from_secs(60)))
                .unwrap();
        }
        socket
    }

    /// Wait until the daemon's hint channel is connected.
    fn hints_connected(&self) {
        eventually(60, || {
            (self.run(&["status"])["hints"]["connected"] == true).then_some(())
        });
    }

    /// One JSON-RPC call to the daemon, as the desktop app makes it; the
    /// error is its code.
    fn rpc(
        &self,
        http: &reqwest::blocking::Client,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        let reply: Value = http
            .post(format!("http://127.0.0.1:{}/app/v1", self.port()))
            // A call may move a whole file (a share link re-encrypts and
            // uploads it, about 30 s for 64 MiB in a debug build); the
            // desktop app waits for it, not the client's default 30 s.
            .timeout(Duration::from_secs(300))
            .bearer_auth(self.token())
            .header("content-type", "application/json")
            .body(
                json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string(),
            )
            .send()
            .and_then(|r| r.json())
            // The daemon was stopped or killed mid-request.
            .map_err(|_| "TRANSPORT".to_string())?;
        match reply.get("error") {
            Some(error) => Err(error["data"]["code"]
                .as_str()
                .unwrap_or("UNKNOWN")
                .to_string()),
            None => Ok(reply["result"].clone()),
        }
    }

    /// Decide a request as the desktop approval window does.
    fn decide(
        &self,
        http: &reqwest::blocking::Client,
        request: &Value,
        approve: bool,
    ) -> Result<Value, String> {
        self.rpc(
            http,
            "approval/respond",
            json!({"id": request["id"], "digest": request["digest"], "approve": approve, "human": true}),
        )
    }

    /// The single request of `kind` waiting here, after a mailbox pass.
    fn waiting(&self, http: &reqwest::blocking::Client, kind: &str) -> Vec<Value> {
        match self.rpc(http, "connect/sync", Value::Null) {
            Err(error) => eprintln!("connect/sync on {}: {error}", self.home.display()),
            Ok(report)
                if report["refused"] != 0
                    || report["ignored"] != 0
                    || report["deferred"] != json!([]) =>
            {
                eprintln!("connect/sync on {}: {report}", self.home.display())
            }
            Ok(_) => {}
        }
        self.rpc(http, "approval/list", json!({}))
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["type"] == kind && r["state"] == "awaiting_user")
            .cloned()
            .collect()
    }

    /// Start `dmsg login` and return it with the request it printed.
    fn start_login(&self) -> (Child, BufReader<std::process::ChildStdout>, String) {
        self.start_login_with(&[])
    }

    fn start_login_with(
        &self,
        extra: &[&str],
    ) -> (Child, BufReader<std::process::ChildStdout>, String) {
        let mut child = self
            .command(&[&["login", "--no-browser"], extra].concat())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let url = serde_json::from_str::<Value>(&line).unwrap()["url"]
            .as_str()
            .unwrap_or_else(|| panic!("dmsg login: {line}"))
            .to_string();
        (child, reader, url)
    }

    /// Log in as the Ed25519 login `seed`, playing the login page; the
    /// error is the callback's answer.
    fn login(&self, http: &reqwest::blocking::Client, seed: u8) -> Result<Value, String> {
        self.login_with(http, seed, &[])
    }

    /// Log in as `seed` to bind it to the unlocked account; returns the
    /// approval request the binding waits on.
    fn bind_login(&self, http: &reqwest::blocking::Client, seed: u8) -> Value {
        self.login_with(http, seed, &["--bind"]).unwrap()
    }

    fn login_with(
        &self,
        http: &reqwest::blocking::Client,
        seed: u8,
        extra: &[&str],
    ) -> Result<Value, String> {
        let (mut child, mut reader, url) = self.start_login_with(extra);
        let (page, fragment) = url.split_once('#').unwrap();
        assert_eq!(page, "https://dmsg.net/_/login");
        let request = decode_request(&URL_SAFE_NO_PAD.decode(fragment).unwrap()).unwrap();
        let callback = http.get(&request.next_url).send().unwrap();
        assert_eq!(callback.status(), 200);
        assert!(callback.text().unwrap().contains("fetch('/auth/callback'"));

        // Internet Identity signs no targets; the daemon scopes the chain.
        let login = BasicIdentity::from_raw_key(&[seed; 32]);
        let delegation = Delegation {
            pubkey: request.session_pubkey.to_vec(),
            expiration: (now_ms() + 10 * 60_000) * 1_000_000,
            targets: None,
            permissions: None,
        };
        let signature = login
            .sign_delegation(&delegation)
            .unwrap()
            .signature
            .unwrap();
        let chain = json!({
            "delegations": [{
                "delegation": {
                    "expiration": format!("{:x}", delegation.expiration),
                    "pubkey": hex::encode(&delegation.pubkey),
                },
                "signature": hex::encode(signature),
            }],
            "publicKey": hex::encode(login.public_key().unwrap()),
        });
        let body = canonical(&LoginCallback::Result(LoginResult {
            v: 1,
            state: request.state,
            delegation: chain.to_string().into_bytes().into(),
        }));
        let port = self.port();
        let posted = http
            .post(&request.next_url)
            .header("origin", format!("http://127.0.0.1:{port}"))
            .header("content-type", "application/cbor")
            .body(body)
            .send()
            .unwrap();
        let accepted = posted.status() == 200;
        let answer = posted.text().unwrap_or_default();
        // The listener is gone after one POST, whatever its fate.
        assert_eq!(http.get(&request.next_url).send().unwrap().status(), 404);
        let mut rest = String::new();
        reader.read_to_string(&mut rest).unwrap();
        assert_eq!(child.wait().unwrap().success(), accepted, "{rest}");
        if !accepted {
            return Err(answer);
        }
        Ok(serde_json::from_str(rest.lines().last().unwrap()).unwrap())
    }
}

/// The Principal of the Ed25519 login `seed`.
fn login_principal(seed: u8) -> String {
    BasicIdentity::from_raw_key(&[seed; 32])
        .sender()
        .unwrap()
        .to_text()
}

/// The titles in a device's vault.
fn titles(device: &Desktop) -> Vec<String> {
    device
        .run(&["vault", "list"])
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["item"]["title"].as_str().unwrap().to_string())
        .collect()
}

/// Poll `check` every half second until it returns a value.
type AppSocket = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>;

/// Read the app socket until the daemon announces a `kind` event whose
/// data's `name` is `value`.
fn await_event(socket: &mut AppSocket, kind: &str, name: &str, value: &str) -> Value {
    loop {
        let tungstenite::Message::Text(text) = socket.read().expect("an event within a minute")
        else {
            continue;
        };
        let note: Value = serde_json::from_str(&text).unwrap();
        let event = &note["params"];
        if note["method"] == "event" && event["type"] == kind && event["data"][name] == value {
            return event.clone();
        }
    }
}

fn eventually<T>(seconds: u64, mut check: impl FnMut() -> Option<T>) -> T {
    let started = Instant::now();
    loop {
        if let Some(value) = check() {
            return value;
        }
        assert!(
            started.elapsed() < Duration::from_secs(seconds),
            "timed out after {seconds} s"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// A Node driver of `src/dmsg_connect/scripts`, as one browser: JSON lines
/// over its stdio. `headless.ts` runs the bridge in Node; `browser.ts` runs
/// the bridge pages and an application page in a real browser.
struct Headless {
    child: Child,
    input: std::process::ChildStdin,
    output: BufReader<std::process::ChildStdout>,
    next: u64,
}

impl Headless {
    fn start(env: &Env, relay: &str) -> Self {
        let mut bridge = Self::spawn("headless.ts");
        let d = &env.deployment;
        bridge
            .call(json!({"cmd": "init", "config": {
                "environment": "Local",
                "relay": relay,
                "ic_host": d.gateway.trim_end_matches('/'),
                "root_key_hex": d.root_key_hex,
                "handle": d.handle.to_text(),
                "commerce": d.commerce.to_text(),
            }}))
            .unwrap();
        bridge
    }

    /// A real browser (`chromium`, `firefox` or `webkit`) with a fresh profile.
    fn browser(name: &str, bridge: &str) -> Self {
        let mut driver = Self::spawn("browser.ts");
        // Playwright's own Chromium build, when the installed one is older.
        let executable = match name {
            "chromium" => std::env::var("DMSG_TEST_CHROME").unwrap_or_default(),
            _ => String::new(),
        };
        let started = driver
            .call(json!({"cmd": "init", "browser": name, "bridge": bridge, "executable": executable}))
            .unwrap_or_else(|e| panic!("{name}: {e} (install it with `pnpm --dir src/dmsg_connect exec playwright install {name}`)"));
        eprintln!("{name} {}", started["version"]);
        driver
    }

    fn spawn(script: &str) -> Self {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../src/dmsg_connect/scripts")
            .join(script);
        let mut child = Command::new("node")
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            input,
            output,
            next: 0,
        }
    }

    fn call(&mut self, mut command: Value) -> Result<Value, String> {
        self.next += 1;
        command["id"] = self.next.into();
        writeln!(self.input, "{command}").unwrap();
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap_or_else(|_| panic!("bridge: {line}"));
        assert_eq!(reply["id"], self.next);
        match reply["ok"].as_bool() {
            Some(true) => Ok(reply["value"].clone()),
            _ => Err(reply["error"].as_str().unwrap_or("UNKNOWN").to_string()),
        }
    }
}

impl Drop for Headless {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A loopback HTTP/1.1 forwarder in front of the relay or the IC gateway
/// that can lose a reply: it forwards a matching request and then closes
/// the connection without answering, as a network failure after the
/// server committed would. WebSocket upgrades are tunneled as they are.
struct Proxy {
    origin: String,
    /// `(method, path fragment)` of requests whose next reply is lost.
    rules: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
}

impl Proxy {
    fn start(upstream: &str) -> Self {
        let upstream = upstream
            .trim_end_matches('/')
            .trim_start_matches("http://")
            .to_string();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let rules = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let shared = rules.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (upstream, rules) = (upstream.clone(), shared.clone());
                std::thread::spawn(move || {
                    let _ = Self::serve(stream, &upstream, &rules);
                });
            }
        });
        Self { origin, rules }
    }

    /// Lose the reply to the next `method` request whose path contains `path`.
    fn lose_next(&self, method: &str, path: &str) {
        self.rules
            .lock()
            .unwrap()
            .push((method.into(), path.into()));
    }

    /// Rules not used yet.
    fn pending(&self) -> usize {
        self.rules.lock().unwrap().len()
    }

    /// One request head and body, or `None` at the end of the stream.
    fn read_message(
        reader: &mut BufReader<impl Read>,
    ) -> std::io::Result<Option<(String, Vec<u8>)>> {
        let mut head = String::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                return Ok(None);
            }
            head.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }
        let header = |name: &str| {
            head.lines()
                .find(|l| l.to_ascii_lowercase().starts_with(&format!("{name}:")))
                .map(|l| l.split_once(':').unwrap().1.trim().to_ascii_lowercase())
        };
        let mut body = Vec::new();
        if header("transfer-encoding").is_some_and(|v| v.contains("chunked")) {
            loop {
                let mut size = String::new();
                reader.read_line(&mut size)?;
                body.extend_from_slice(size.as_bytes());
                let n =
                    usize::from_str_radix(size.trim().split(';').next().unwrap(), 16).unwrap_or(0);
                let mut chunk = vec![0; n + 2];
                reader.read_exact(&mut chunk)?;
                body.extend_from_slice(&chunk);
                if n == 0 {
                    break;
                }
            }
        } else if let Some(length) = header("content-length") {
            body.resize(length.parse().unwrap_or(0), 0);
            reader.read_exact(&mut body)?;
        }
        Ok(Some((head, body)))
    }

    /// `Connection: close` in place of any connection header.
    fn closing(head: &str) -> String {
        let mut lines: Vec<&str> = head
            .trim_end()
            .lines()
            .filter(|l| !l.to_ascii_lowercase().starts_with("connection:"))
            .collect();
        lines.push("Connection: close");
        format!("{}\r\n\r\n", lines.join("\r\n"))
    }

    fn serve(
        client: std::net::TcpStream,
        upstream: &str,
        rules: &std::sync::Mutex<Vec<(String, String)>>,
    ) -> std::io::Result<()> {
        let mut reader = BufReader::new(client.try_clone()?);
        let Some((head, body)) = Self::read_message(&mut reader)? else {
            return Ok(());
        };
        let mut server = std::net::TcpStream::connect(upstream)?;
        if head.to_ascii_lowercase().contains("upgrade: websocket") {
            server.write_all(head.as_bytes())?;
            let (mut up_read, mut client_write) = (server.try_clone()?, client.try_clone()?);
            std::thread::spawn(move || std::io::copy(&mut up_read, &mut client_write));
            let mut rest = reader.buffer().to_vec();
            server.write_all(&rest)?;
            rest.clear();
            std::io::copy(&mut reader.into_inner(), &mut server)?;
            return Ok(());
        }
        server.write_all(Self::closing(&head).as_bytes())?;
        server.write_all(&body)?;
        let mut response = BufReader::new(server);
        let Some((reply_head, reply_body)) = Self::read_message(&mut response)? else {
            return Ok(());
        };
        let line = head.lines().next().unwrap_or_default();
        let (method, path) = line.split_once(' ').unwrap_or_default();
        let lost = {
            let mut rules = rules.lock().unwrap();
            let found = rules.iter().position(|(m, p)| {
                m == method && path.split(' ').next().unwrap_or("").contains(p.as_str())
            });
            found.map(|i| rules.remove(i)).is_some()
        };
        if lost {
            // The server committed; the client never hears it.
            return client.shutdown(std::net::Shutdown::Both);
        }
        let mut client = client;
        client.write_all(Self::closing(&reply_head).as_bytes())?;
        client.write_all(&reply_body)
    }
}

/// An HTTPS front for a local HTTP service, with the public localhost test
/// certificate the daemons trust through `agent_ca`. It keeps each write
/// the service accepted and can answer the next matching write itself, as
/// another service sharing the writer's key would have.
struct TlsFront {
    origin: String,
    state: std::sync::Arc<std::sync::Mutex<FrontState>>,
}

#[derive(Default)]
struct FrontState {
    /// `(method, path fragment, raw response)` answered without forwarding.
    answers: Vec<(String, String, String)>,
    /// `(path, JSON body)` of each write the service answered with 2xx.
    accepted: Vec<(String, Value)>,
}

impl TlsFront {
    fn start(upstream: &str) -> Self {
        use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/localhost-tls");
        let cert = CertificateDer::from(std::fs::read(fixture.join("cert.der")).unwrap());
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            std::fs::read(fixture.join("key.der")).unwrap(),
        ));
        let config = std::sync::Arc::new(
            rustls::ServerConfig::builder_with_provider(std::sync::Arc::new(
                rustls::crypto::aws_lc_rs::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap(),
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!(
            "https://localhost:{}",
            listener.local_addr().unwrap().port()
        );
        let state = std::sync::Arc::new(std::sync::Mutex::new(FrontState::default()));
        let (shared, upstream) = (state.clone(), upstream.to_string());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (config, state, upstream) = (config.clone(), shared.clone(), upstream.clone());
                std::thread::spawn(move || {
                    let _ = Self::serve(stream, config, &upstream, &state);
                });
            }
        });
        Self { origin, state }
    }

    /// Answer the next `method` request whose path contains `path` with a
    /// JSON error `code`, the status 409 and these headers.
    fn answer_next(&self, method: &str, path: &str, code: &str, headers: &[(&str, String)]) {
        let body = json!({"error": {"code": code, "message": code}}).to_string();
        let headers: String = headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect();
        let response = format!(
            "HTTP/1.1 409 Conflict\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}",
            body.len()
        );
        self.state
            .lock()
            .unwrap()
            .answers
            .push((method.into(), path.into(), response));
    }

    /// The events the service accepted, as their writers signed them.
    fn events(&self) -> Vec<(String, Value)> {
        self.state
            .lock()
            .unwrap()
            .accepted
            .iter()
            .filter(|(_, body)| body["event"].is_object())
            .cloned()
            .collect()
    }

    /// The highest nonce of an actor the service accepted.
    fn nonce(&self, actor: &str) -> Option<u64> {
        self.events()
            .iter()
            .filter(|(_, body)| body["event"]["actor"] == actor)
            .filter_map(|(_, body)| body["event"]["nonce"].as_u64())
            .max()
    }

    /// The text of every message the service accepted in a room, in order.
    fn messages(&self, room: &str) -> Vec<String> {
        self.events()
            .iter()
            .filter(|(path, body)| {
                path.ends_with(&format!("/v1/rooms/{room}"))
                    && body["event"]["type"]
                        == agent_protocols::discourse::event_type::MESSAGE_CREATE
            })
            .map(|(_, body)| {
                body["event"]["payload"]["content"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect()
    }

    fn serve(
        stream: std::net::TcpStream,
        config: std::sync::Arc<rustls::ServerConfig>,
        upstream: &str,
        state: &std::sync::Mutex<FrontState>,
    ) -> std::io::Result<()> {
        let connection = rustls::ServerConnection::new(config).map_err(std::io::Error::other)?;
        let mut reader = BufReader::new(rustls::StreamOwned::new(connection, stream));
        let Some((head, body)) = Proxy::read_message(&mut reader)? else {
            return Ok(());
        };
        let mut tls = reader.into_inner();
        let line = head.lines().next().unwrap_or_default();
        let (method, rest) = line.split_once(' ').unwrap_or_default();
        let path = rest.split(' ').next().unwrap_or_default().to_string();
        let answer = {
            let mut state = state.lock().unwrap();
            let found = state
                .answers
                .iter()
                .position(|(m, p, _)| m == method && path.contains(p.as_str()));
            found.map(|i| state.answers.remove(i).2)
        };
        let reply = match answer {
            Some(response) => response.into_bytes(),
            None => {
                let mut server = std::net::TcpStream::connect(upstream)?;
                server.write_all(Proxy::closing(&head).as_bytes())?;
                server.write_all(&body)?;
                let mut response = BufReader::new(server);
                let Some((reply_head, reply_body)) = Proxy::read_message(&mut response)? else {
                    return Ok(());
                };
                let accepted = reply_head
                    .split(' ')
                    .nth(1)
                    .is_some_and(|status| status.starts_with('2'));
                if method == "POST"
                    && accepted
                    && let Ok(value) = serde_json::from_slice(&body)
                {
                    state.lock().unwrap().accepted.push((path, value));
                }
                [Proxy::closing(&reply_head).into_bytes(), reply_body].concat()
            }
        };
        tls.write_all(&reply)?;
        tls.conn.send_close_notify();
        tls.flush()
    }
}

/// What a test environment adds to the plain one.
#[derive(Default)]
struct Options {
    /// Extra `config.toml` text for device B.
    b_policy: &'static str,
    /// Put fault-injecting proxies in front of the relay and the IC gateway.
    proxies: bool,
    /// Let the relay take paid letters.
    paid_contacts: bool,
    /// Serve Agent Delegation on the relay with principal documents from
    /// the directory, and a Discourse host the daemons may sign for.
    agents: bool,
    /// Serve Agent Mail and Agent Knowledge: the private `dmsg-mail` and
    /// `dmsg-knowledge` Workers on local HTTPS, which the daemons trust
    /// through `agent_ca`.
    services: bool,
    /// Install frozen old channel and name-identity canisters of the
    /// current release, reviewed by the relay and the daemons (`[legacy]`).
    legacy: bool,
    /// Keep device A's owner credential and keys in the system credential
    /// store, as installed desktops do.
    system_storage: bool,
    /// Install the team wallet hub; the daemons' `[wallet]` names it and
    /// points every external chain at a closed loopback port.
    wallet: bool,
}

/// One deployment, one relay and two desktops, `a` and `b`.
struct Env {
    dir: PathBuf,
    children: Children,
    deployment: support::Deployment,
    http: reqwest::blocking::Client,
    relay: String,
    /// The bridge page's origin (relay CORS, daemon `connect_url`); `serve_web` serves it.
    connect: String,
    a: Desktop,
    b: Desktop,
    daemons: [usize; 2],
    /// The daemons' `config.toml`, for more desktops.
    config: String,
    /// The proxy in front of the relay, if any.
    relay_proxy: Option<Proxy>,
    /// The proxy in front of the IC gateway, if any.
    ic_proxy: Option<Proxy>,
    /// The `dmsg-discourse` host the daemons may sign for, behind HTTPS.
    discourse: Option<TlsFront>,
    /// The Agent Mail relay and the Agent Knowledge service, if served.
    services: Option<Services>,
    /// The frozen old service, if installed.
    legacy: Option<Legacy>,
}

/// The local HTTPS origins of `dmsg-mail` and `dmsg-knowledge`.
struct Services {
    mail: String,
    knowledge: String,
}

/// PEM text of DER `bytes`.
fn pem(label: &str, bytes: &[u8]) -> String {
    use base64::engine::general_purpose::STANDARD;
    let body = STANDARD.encode(bytes);
    let lines: Vec<&str> = body
        .as_bytes()
        .chunks(64)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect();
    format!(
        "-----BEGIN {label}-----\n{}\n-----END {label}-----\n",
        lines.join("\n")
    )
}

/// The public localhost test certificate and key as PEM files; the daemons
/// get the certificate as `agent_ca`.
fn localhost_tls(dir: &Path) -> (PathBuf, PathBuf) {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/localhost-tls");
    let cert = dir.join("localhost-cert.pem");
    let key = dir.join("localhost-key.pem");
    std::fs::write(
        &cert,
        pem(
            "CERTIFICATE",
            &std::fs::read(fixture.join("cert.der")).unwrap(),
        ),
    )
    .unwrap();
    std::fs::write(
        &key,
        pem(
            "PRIVATE KEY",
            &std::fs::read(fixture.join("key.der")).unwrap(),
        ),
    )
    .unwrap();
    (cert, key)
}

/// Start `dmsg-mail` and `dmsg-knowledge` on local HTTPS with the localhost
/// test certificate.
fn serve_services(cloud: &Path, dir: &Path, children: &mut Children) -> Services {
    let (cert, key) = localhost_tls(dir);
    let mut origins = vec![];
    for package in ["dmsg-mail", "dmsg-knowledge"] {
        let ready = dir.join(format!("{package}-ready.json"));
        children.0.push(
            Command::new("node")
                .arg("scripts/probe-server.mjs")
                .current_dir(cloud.join(package))
                .env("PROBE_PORT", free_port().to_string())
                .env("PROBE_CERT", &cert)
                .env("PROBE_KEY", &key)
                .env("PROBE_READY", &ready)
                .env("PROBE_TTL_MS", "1200000")
                .env("WRANGLER_SEND_METRICS", "false")
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );
        origins.push(
            wait_for(&ready, children)["origin"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    let knowledge = origins.pop().unwrap();
    let mail = origins.pop().unwrap();
    Services { mail, knowledge }
}

/// Start `dmsg-discourse` on local workerd behind an HTTPS front.
fn serve_discourse(cloud: &Path, dir: &Path, children: &mut Children) -> TlsFront {
    let port = free_port();
    let front = TlsFront::start(&format!("127.0.0.1:{port}"));
    let ready = dir.join("dmsg-discourse-ready.json");
    children.0.push(
        Command::new("node")
            .arg("scripts/probe-server.mjs")
            .current_dir(cloud.join("dmsg-discourse"))
            .env("PROBE_PORT", port.to_string())
            .env("PROBE_ORIGIN", &front.origin)
            .env("PROBE_READY", &ready)
            .env("PROBE_TTL_MS", "1200000")
            .env("WRANGLER_SEND_METRICS", "false")
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for(&ready, children);
    front
}

impl Env {
    /// `b_policy` is extra `config.toml` text for device B.
    fn start(name: &str, b_policy: &'static str) -> Self {
        Self::launch(
            name,
            Options {
                b_policy,
                ..Options::default()
            },
        )
    }

    fn launch(name: &str, options: Options) -> Self {
        let cloud = PathBuf::from(std::env::var_os("DMSG_CLOUD_DIR").expect("DMSG_CLOUD_DIR"));
        let bin = std::env::var_os("DMSG_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/dmsg")
            });
        assert!(
            bin.exists(),
            "build the dmsg binary first: cargo build -p dmsg_cli"
        );
        let dir = std::env::temp_dir().join(format!("dmsg-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut children = Children(vec![]);

        let deployment = support::Deployment::start();
        let legacy = options.legacy.then(|| Legacy::install(&deployment));
        let wallet_hub = options
            .wallet
            .then(|| deployment.install_wallet_hub(WALLET_PRICE));
        let mut launch: Value = serde_json::from_str(&deployment.fixture_json()).unwrap();
        if let Some(legacy) = &legacy {
            launch["legacy"] = json!({
                "channel": legacy.channel.to_text(),
                "identity": legacy.identity.to_text(),
                "root_key_hex": deployment.root_key_hex,
            });
        }
        launch["port"] = free_port().into();
        launch["ttl_ms"] = 1_200_000.into();
        let connect = format!("http://localhost:{}", free_port());
        launch["connect_origin"] = connect.clone().into();
        launch["paid_contacts"] = options.paid_contacts.into();
        // The proxy forwards to the relay's port; requests name the proxy.
        let relay_port = launch["port"].as_u64().unwrap();
        let relay_proxy = options
            .proxies
            .then(|| Proxy::start(&format!("http://127.0.0.1:{relay_port}")));
        if let Some(proxy) = &relay_proxy {
            launch["api_origin"] = proxy.origin.clone().into();
        }
        if options.agents {
            // The directory takes only https origins; the relay compares
            // its documents with this one instead of its loopback origin.
            launch["delegation_service"] = "https://api.dmsg.test".into();
            launch["principal_origin"] = "https://id.dmsg.test".into();
            launch["principal_gateway"] =
                format!("{}?canisterId={}", deployment.gateway, deployment.directory).into();
        }
        std::fs::write(dir.join("cloud-launch.json"), launch.to_string()).unwrap();
        children.0.push(
            Command::new("node")
                .arg(cloud.join("dmsg-core/scripts/desktop-probe-server.mjs"))
                .current_dir(&cloud)
                .env("DMSG_CLOUD_FIXTURE_DIR", &dir)
                .env("WRANGLER_SEND_METRICS", "false")
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let ready = wait_for(&dir.join("cloud-ready.json"), &mut children);
        let relay = ready["api_origin"]
            .as_str()
            .or(ready["relay"].as_str())
            .unwrap()
            .to_string();
        let ic_proxy = options.proxies.then(|| Proxy::start(&deployment.gateway));
        let ic_host = ic_proxy
            .as_ref()
            .map_or(deployment.gateway.trim_end_matches('/').to_string(), |p| {
                p.origin.clone()
            });

        let discourse = options
            .agents
            .then(|| serve_discourse(&cloud, &dir, &mut children));
        let services = options
            .services
            .then(|| serve_services(&cloud, &dir, &mut children));
        // The owner credential stays in the home: tests never touch the
        // system credential store.
        let mut config = format!(
            "environment = \"local\"\nic_host = \"{ic_host}\"\nrelay = \"{relay}\"\nconnect_url = \"{connect}\"\nhandle = \"{}\"\nuser_homes = [\"{}\"]\nroot_key = \"{}\"\nsecure_storage = \"file\"\n",
            deployment.handle, deployment.user, deployment.root_key_hex,
        );
        let mut hosts = vec![];
        if options.agents || options.services {
            let (cert, _) = localhost_tls(&dir);
            config.push_str(&format!("agent_ca = \"{}\"\n", cert.display()));
        }
        if let Some(discourse) = &discourse {
            hosts.push(format!(
                "{{ host = \"{}\", label = \"test\" }}",
                discourse.origin
            ));
        }
        if let Some(services) = &services {
            hosts.push(format!(
                "{{ host = \"{}\", label = \"mail\" }}",
                services.mail
            ));
            hosts.push(format!(
                "{{ host = \"{}\", label = \"knowledge\" }}",
                services.knowledge
            ));
        }
        if !hosts.is_empty() {
            config.push_str(&format!("agent_hosts = [{}]\n", hosts.join(", ")));
        }
        // A table ends the top-level keys: it goes after every policy.
        let mut table = legacy.as_ref().map_or(String::new(), |legacy| {
            format!(
                "[legacy]\nchannels = [\"{}\"]\nidentity = \"{}\"\ncutover = \"{}\"\n",
                legacy.channel,
                legacy.identity,
                "08".repeat(32)
            )
        });
        if let Some(hub) = wallet_hub {
            // Nothing listens on port 9: chain reads fail fast and stay local.
            let closed = "http://127.0.0.1:9";
            table.push_str(&format!(
                "[wallet]\nhub = \"{hub}\"\nbitcoin_api = \"{closed}\"\nsolana_rpc = \"{closed}\"\nevm_rpc = {{ 1 = \"{closed}\", 8453 = \"{closed}\" }}\n"
            ));
        }
        let desktop = |name: &str, policy: &str| {
            let home = dir.join(name);
            std::fs::create_dir_all(&home).unwrap();
            std::fs::write(home.join("config.toml"), format!("{config}{policy}{table}")).unwrap();
            Desktop {
                bin: bin.clone(),
                home,
            }
        };
        let (a, b) = (desktop("a", ""), desktop("b", options.b_policy));
        if options.system_storage {
            let path = a.home.join("config.toml");
            let text = std::fs::read_to_string(&path).unwrap();
            std::fs::write(
                path,
                text.replace("secure_storage = \"file\"", "secure_storage = \"system\""),
            )
            .unwrap();
        }
        let daemons = [a.spawn(&mut children), b.spawn(&mut children)];
        let http = reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .unwrap();
        Self {
            dir,
            children,
            deployment,
            http,
            relay,
            connect,
            a,
            b,
            daemons,
            config: format!("{config}{table}"),
            relay_proxy,
            ic_proxy,
            discourse,
            services,
            legacy,
        }
    }

    /// Another desktop with its own home and running daemon, and the
    /// daemon's index for [`Self::relaunch`].
    fn desktop(&mut self, name: &str) -> (Desktop, usize) {
        let home = self.dir.join(name);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("config.toml"), &self.config).unwrap();
        let desktop = Desktop {
            bin: self.a.bin.clone(),
            home,
        };
        let index = desktop.spawn(&mut self.children);
        (desktop, index)
    }

    /// Kill the daemon at `index` and start `desktop`'s again in its place.
    fn relaunch(&mut self, desktop: &Desktop, index: usize) {
        let old = &mut self.children.0[index];
        let _ = old.kill();
        let _ = old.wait();
        self.children.0[index] = desktop.launch();
        wait_for(&desktop.home.join("daemon.json"), &mut self.children);
    }

    /// Build the bridge pages for this deployment and the test application,
    /// and serve them as the asset canister would: the bridge at
    /// `self.connect`, the application at two origins. Returns those origins.
    fn serve_web(&mut self) -> (String, String) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src/dmsg_connect");
        let d = &self.deployment;
        let config = json!({
            "environment": "Local",
            "icHost": d.gateway.trim_end_matches('/'),
            "apiOrigin": self.relay,
            "canisters": {"handle": d.handle.to_text(), "commerce": d.commerce.to_text()},
            "rootKey": d.root_key_hex,
        });
        std::fs::write(self.dir.join("connect.json"), config.to_string()).unwrap();
        let (bridge, app) = (self.dir.join("connect-dist"), self.dir.join("app-dist"));
        for (args, var, out) in [
            (&["build"][..], "DMSG_CONNECT_OUT", &bridge),
            (
                &["exec", "vite", "build", "-c", "e2e/vite.config.ts"][..],
                "DMSG_CONNECT_APP_OUT",
                &app,
            ),
        ] {
            let built = Command::new("pnpm")
                .args(args)
                .current_dir(&root)
                .env("DMSG_CONNECT_CONFIG", self.dir.join("connect.json"))
                .env(var, out)
                .stdout(Stdio::null())
                .status()
                .unwrap();
            assert!(built.success(), "building {}", out.display());
        }
        let port = self.connect.rsplit(':').next().unwrap().to_string();
        let mut origins = vec![];
        for (dir, port) in [(&bridge, port), (&app, "0".into()), (&app, "0".into())] {
            let mut child = Command::new("node")
                .arg(root.join("scripts/serve.mjs"))
                .args([dir.to_str().unwrap(), &port, "localhost"])
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut line = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut line)
                .unwrap();
            origins.push(
                serde_json::from_str::<Value>(&line).unwrap()["origin"]
                    .as_str()
                    .unwrap()
                    .to_string(),
            );
            self.children.0.push(child);
        }
        assert_eq!(origins[0], self.connect);
        (origins[1].clone(), origins[2].clone())
    }

    /// A creates the account; B joins it with the same login and A's
    /// approval. Returns the account.
    fn pair(&self) -> String {
        let (a, b) = (&self.a, &self.b);
        assert_eq!(a.login(&self.http, 41).unwrap()["state"], "no_account");
        let created = a.run(&["account", "create"]);
        assert_eq!(
            b.login(&self.http, 41).unwrap()["state"],
            "pairing_required"
        );
        let request = b.run(&["device", "pair-request"]);
        let token = request["token"].as_str().unwrap();
        let device = request["device_id"].as_str().unwrap();
        a.run(&["device", "approve", token, "--device", device]);
        assert_eq!(b.run(&["unlock"])["state"], "unlocked");
        created["account"].as_str().unwrap().to_string()
    }

    /// Stop (or reap) daemon `which` (0 is A, 1 is B) and start it again in place.
    fn restart(&mut self, which: usize) {
        let desktop = if which == 0 { &self.a } else { &self.b };
        let index = self.daemons[which];
        let old = &mut self.children.0[index];
        let _ = old.kill();
        let _ = old.wait();
        self.children.0[index] = desktop.launch();
        wait_for(&desktop.home.join("daemon.json"), &mut self.children);
    }

    fn finish(self) {
        for device in [&self.a, &self.b] {
            let _ = device.command(&["stop"]).output();
        }
        let Env {
            dir,
            children,
            mut deployment,
            ..
        } = self;
        drop(children);
        deployment.ic.stop_live();
        drop(deployment);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn two_desktop_devices_share_an_account() {
    let env = Env::start("desktop", "");
    let (a, b, http) = (&env.a, &env.b, &env.http);

    // The gateway answers only its owner, and never a web page.
    let status_url = format!("http://127.0.0.1:{}/daemon/status", a.port());
    assert_eq!(http.get(&status_url).send().unwrap().status(), 401);
    assert_eq!(
        http.get(&status_url)
            .bearer_auth(a.token())
            .header("origin", "https://evil.example")
            .send()
            .unwrap()
            .status(),
        401
    );
    assert_eq!(a.run(&["status"])["state"], "logged_out");

    // A callback with a foreign Origin closes the login.
    let (mut child, _, url) = a.start_login();
    let request = decode_request(
        &URL_SAFE_NO_PAD
            .decode(url.split_once('#').unwrap().1)
            .unwrap(),
    )
    .unwrap();
    let refused = http
        .post(&request.next_url)
        .header("origin", "https://evil.example")
        .header("content-type", "application/cbor")
        .body(vec![0xa0])
        .send()
        .unwrap();
    assert_eq!(refused.status(), 403);
    assert_eq!(http.get(&request.next_url).send().unwrap().status(), 404);
    assert!(!child.wait().unwrap().success());

    // Device A creates the account, writes an item and syncs. The desktop
    // hears of a login, which no method call carries.
    let mut events = a.app_socket();
    let status = a.login(http, 41).unwrap();
    assert_eq!(status["state"], "no_account");
    await_event(&mut events, "session/changed", "state", "no_account");
    drop(events);
    let created = a.run(&["account", "create"]);
    let account = created["account"].as_str().unwrap().to_string();
    assert_eq!(created["root_generation"], 1);
    let entry = a.run_with(
        &[
            "vault",
            "add",
            "--title",
            "alpha",
            "--body",
            "hello from A",
            "--secret-stdin",
        ],
        Some("s3cret\n"),
    );
    let item = entry["id"].as_str().unwrap().to_string();
    assert_eq!(entry["item"]["secret"], "********");
    assert_eq!(a.run(&["status"])["pending_sync"], 1);
    assert_eq!(a.run(&["sync"])["sent"], 1);
    assert_eq!(a.run(&["status"])["pending_sync"], 0);

    // An item written before the next root rotation is resealed for it.
    let pending = a.run(&["vault", "add", "--title", "gamma"]);

    // Device B logs in with the same login and asks to join.
    let status = b.login(http, 41).unwrap();
    assert_eq!(status["state"], "pairing_required");
    let request = b.run(&["device", "pair-request"]);
    let token = request["token"].as_str().unwrap();
    let device = request["device_id"].as_str().unwrap();
    assert_eq!(
        a.fail(&["device", "approve", token, "--device", &"00".repeat(32)]),
        "INVALID_INPUT"
    );
    let approved = a.run(&["device", "approve", token, "--device", device]);
    assert_eq!(approved["root_generation"], 2);
    let sent = a.run(&["sync"]);
    assert_eq!(
        (sent["sent"].as_u64(), sent["pending"].as_u64()),
        (Some(1), Some(0))
    );
    assert_eq!(
        a.run(&["vault", "show", pending["id"].as_str().unwrap()])["item"]["title"],
        "gamma"
    );

    // B binds, opens the rotated root and, syncing on unlock, reads A's items.
    let status = b.run(&["unlock"]);
    assert_eq!(status["state"], "unlocked");
    assert_eq!(status["account"], account.as_str());
    assert_eq!(status["root_generation"], 2);
    assert!(status["last_sync"]["error"].is_null());
    assert_eq!(b.run(&["sync"])["revisions"], 2);
    let shown = b.run(&["vault", "show", &item, "--reveal"]);
    assert_eq!(shown["item"]["title"], "alpha");
    assert_eq!(shown["item"]["body"], "hello from A");
    assert_eq!(shown["item"]["secret"], "s3cret");
    let devices = b.run(&["device", "list"]);
    assert_eq!(devices.as_array().unwrap().len(), 2);

    // B writes under the new root; A reads it after its own sync.
    let reply = b.run(&["vault", "add", "--title", "beta", "--body", "hello from B"]);
    assert_eq!(b.run(&["sync"])["sent"], 1);
    a.run(&["sync"]);
    let listed = titles(a);
    assert_eq!(listed.len(), 3);
    assert!(
        ["alpha", "beta", "gamma"]
            .iter()
            .all(|t| listed.contains(&t.to_string()))
    );
    assert_eq!(
        a.run(&["vault", "show", reply["id"].as_str().unwrap()])["item"]["body"],
        "hello from B"
    );

    // Locking forgets the login and every key; a new login unlocks again.
    assert_eq!(b.run(&["lock"])["state"], "locked");
    assert_eq!(b.fail(&["vault", "list"]), "LOCKED");
    let status = b.login(http, 41).unwrap();
    assert_eq!(status["state"], "unlocked");
    assert_eq!(b.run(&["vault", "list"]).as_array().unwrap().len(), 3);

    // Another login gets its own workspace: a new device, none of this
    // account's data, and an account of its own.
    assert_eq!(b.run(&["lock"])["state"], "locked");
    let status = b.login(http, 42).unwrap();
    assert_eq!(status["state"], "no_account");
    assert!(status["account"].is_null());
    assert_ne!(status["device_id"], device);
    let second = b.run(&["account", "create"])["account"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(titles(b).is_empty());
    b.run(&["vault", "add", "--title", "delta"]);
    assert_eq!(b.run(&["accounts"]).as_array().unwrap().len(), 2);
    // The first login selects its own account again, with its data only.
    let status = b.login(http, 41).unwrap();
    assert_eq!(status["state"], "unlocked");
    assert_eq!(status["account"], account.as_str());
    assert_eq!(status["device_id"], device);
    assert!(!titles(b).contains(&"delta".to_string()));
    assert_eq!(titles(b).len(), 3);
    // Switching selects the other account, locked until its own login.
    let status = b.run(&["switch", &second]);
    assert_eq!(status["state"], "locked");
    assert_eq!(status["account"], second.as_str());
    assert_eq!(b.fail(&["vault", "list"]), "LOCKED");
    let accounts = b.run(&["accounts"]);
    assert_eq!(
        accounts
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["current"] == true)
            .unwrap()["account"],
        second.as_str()
    );

    // A login without an account asks to be bound to the first account
    // from B; an administrator device approves it, then the login accepts
    // and opens the first account's workspace on B.
    let third = login_principal(43);
    assert_eq!(b.login(http, 43).unwrap()["state"], "no_account");
    let request = b.run(&["account", "bind-request", &account]);
    let token = request["token"].as_str().unwrap();
    assert_eq!(
        a.fail(&[
            "account",
            "bind-approve",
            token,
            "--principal",
            &login_principal(44)
        ]),
        "INVALID_INPUT"
    );
    let approval = a.run(&["account", "bind-approve", token, "--principal", &third]);
    assert_eq!(approval["type"], "account");
    assert_eq!(approval["detail"]["principal"], third.as_str());
    assert_eq!(a.decide(http, &approval, true).unwrap()["state"], "done");
    let status = b.run(&["account", "bind-accept", &account]);
    assert_eq!(status["state"], "unlocked");
    assert_eq!(status["account"], account.as_str());
    assert_eq!(status["principal"], third.as_str());
    let logins = |device: &Desktop| -> Vec<String> {
        device.run(&["account", "security"])["logins"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["principal"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(logins(a), vec![login_principal(41), third.clone()]);

    // On one device: a second login completed for the unlocked account
    // waits on an approval, which binds it; removing it is approved too.
    let approval = a.bind_login(http, 44);
    assert_eq!(approval["detail"]["change"], "bind_login");
    assert_eq!(approval["detail"]["principal"], login_principal(44));
    assert_eq!(a.decide(http, &approval, true).unwrap()["state"], "done");
    assert!(logins(a).contains(&login_principal(44)));
    let removal = a.run(&["account", "unbind", &login_principal(44)]);
    assert_eq!(a.decide(http, &removal, true).unwrap()["state"], "done");
    assert_eq!(logins(a).len(), 2);
    // The login in use is not removed from here.
    assert_eq!(
        a.fail(&["account", "unbind", &login_principal(41)]),
        "FORBIDDEN"
    );

    // Recovery: B starts over as a new device of the first account, and the
    // bound login asks to take the account over; A disputes it.
    let status = b.run(&["reset", "--yes"]);
    assert_eq!(status["state"], "logged_out");
    assert_ne!(status["device_id"], device);
    assert_eq!(b.login(http, 43).unwrap()["state"], "pairing_required");
    let recovery = b.run(&["recovery", "request"]);
    assert_eq!(recovery["pending"]["this_device"], true);
    assert_eq!(recovery["pending"]["new_auth"], third.as_str());
    assert_eq!(recovery["ready"], false);
    assert_eq!(b.fail(&["recovery", "complete"]), "PENDING");
    let security = a.run(&["account", "security"]);
    assert_eq!(security["pending_recovery"]["new_auth"], third.as_str());
    let dispute = a.run(&["recovery", "dispute"]);
    assert_eq!(dispute["detail"]["change"], "dispute_recovery");
    assert_eq!(a.decide(http, &dispute, true).unwrap()["state"], "done");
    assert!(a.run(&["account", "security"])["pending_recovery"].is_null());
    assert!(b.run(&["recovery", "status"])["pending"].is_null());
    assert_eq!(b.fail(&["recovery", "complete"]), "NOT_FOUND");
    // The other account's workspace on B is untouched.
    assert!(
        b.run(&["accounts"])
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["account"] == second.as_str() && a["bound"] == true)
    );

    let status = b.run(&["status"]);
    assert_eq!(status["environment"], "local");
    assert_eq!(status["account"], Value::Null);
    assert!(status["version"].is_string() && status["instance"].is_string());
    env.finish();
}

/// Deterministic bytes of a given length.
fn pattern(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed;
    (0..len)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 56) as u8
        })
        .collect()
}

/// Every file under `dir`, read whole.
fn files_under(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = vec![];
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push((path.clone(), std::fs::read(&path).unwrap()));
        }
    }
    found
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(dmsg_protocol::sha256(bytes).as_slice())
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn desktop_content_and_sync() {
    let mut env = Env::start("content", "background_online = true\n");
    let account = env.pair();
    let (a, b) = (&env.a, &env.b);
    let started = Instant::now();

    // AT06: concurrent edits leave a conflict copy on both devices.
    let x = a.run(&["vault", "add", "--title", "shared", "--body", "v1"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(a.run(&["sync"])["sent"], 1);
    b.run(&["sync"]);
    a.run(&["vault", "edit", &x, "--body", "from A"]);
    b.run(&["vault", "edit", &x, "--body", "from B"]);
    assert_eq!(a.run(&["sync"])["sent"], 1);
    let report = b.run(&["sync"]);
    assert_eq!(
        (report["sent"].as_u64(), report["conflicts"].as_u64()),
        (Some(1), Some(1))
    );
    assert_eq!(b.run(&["vault", "show", &x])["item"]["body"], "from A");
    assert_eq!(a.run(&["sync"])["conflicts"], 1);
    let conflicts = a.run(&["vault", "conflicts"]);
    let copy = &conflicts.as_array().unwrap()[0];
    assert_eq!(copy["item"]["body"], "from B");
    let key = format!(
        "{}:{}",
        copy["id"].as_str().unwrap(),
        copy["revision"].as_str().unwrap()
    );
    a.run(&["vault", "resolve", &key]);
    assert_eq!(a.run(&["sync"])["conflicts"], 0);
    assert_eq!(b.run(&["sync"])["conflicts"], 0);
    assert_eq!(b.run(&["vault", "show", &x])["item"]["body"], "from B");

    // AT06: a tombstone is not overwritten by an older edit; only an
    // explicit restore brings the item back.
    a.run(&["vault", "rm", &x]);
    assert_eq!(a.run(&["sync"])["sent"], 1);
    b.run(&["vault", "edit", &x, "--body", "late edit"]);
    let report = b.run(&["sync"]);
    assert_eq!(
        (report["sent"].as_u64(), report["conflicts"].as_u64()),
        (Some(0), Some(1))
    );
    assert!(b.run(&["vault", "list"]).as_array().unwrap().is_empty());
    assert_eq!(
        b.run(&["vault", "list", "--trash"])
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        b.fail(&["vault", "edit", &x, "--body", "again"]),
        "NOT_FOUND"
    );
    let late = &b.run(&["vault", "conflicts"])[0];
    let late = format!(
        "{}:{}",
        late["id"].as_str().unwrap(),
        late["revision"].as_str().unwrap()
    );
    b.run(&["vault", "resolve", &late]);
    assert_eq!(b.run(&["sync"])["sent"], 1);
    a.run(&["sync"]);
    assert_eq!(a.run(&["vault", "show", &x])["item"]["body"], "late edit");
    b.run(&["vault", "rm", &x]);
    b.run(&["vault", "restore", &x]);
    assert_eq!(b.run(&["sync"])["sent"], 2);

    // DT-F1 and R11: a 100 MiB file added to A's library reaches B only as
    // an explicit backup. Nothing of the library syncs with the vault; B
    // reads the account's backup inventory, fetches the backup, and reads
    // the same bytes.
    let big = pattern(100 * 1024 * 1024, 7);
    let big_path = env.dir.join("big.bin");
    std::fs::write(&big_path, &big).unwrap();
    let added = a.run(&["files", "add", big_path.to_str().unwrap()]);
    let file_id = added[0]["file"]["id"].as_str().unwrap().to_string();
    assert_eq!(added[0]["file"]["sha256"], sha256_hex(&big).as_str());
    assert_eq!(added[0]["file"]["status"], "local");
    assert_eq!(a.run(&["sync"])["sent"], 0);
    assert!(
        b.run(&["files", "cloud", "ls", "--refresh"])
            .as_array()
            .unwrap()
            .is_empty()
    );
    let backup = a.run(&["files", "backup", &file_id]);
    assert_eq!(backup["started"].as_array().unwrap().len(), 1, "{backup}");
    let shown = a.run(&["files", "show", &file_id]);
    assert_eq!(shown["file"]["status"], "backed_up", "{shown}");
    let upload = shown["versions"][0]["backup"]["upload_id"]
        .as_str()
        .unwrap()
        .to_string();
    let cloud = b.run(&["files", "cloud", "ls", "--refresh"]);
    assert_eq!(cloud.as_array().unwrap().len(), 1, "{cloud}");
    assert_eq!(cloud[0]["upload_id"], upload.as_str());
    assert_eq!(cloud[0]["name"], "big.bin");
    assert_eq!(cloud[0]["local"], false);
    let fetched = b.run(&["files", "cloud", "get", &upload]);
    assert_eq!(fetched["id"], file_id.as_str());
    assert_eq!(fetched["status"], "backed_up");
    let out = env.dir.join("big.out");
    let got = b.run(&["files", "get", &file_id, "-o", out.to_str().unwrap()]);
    assert_eq!(got["size"], big.len());
    assert_eq!(sha256_hex(&std::fs::read(&out).unwrap()), sha256_hex(&big));
    assert_eq!(
        b.fail(&["files", "get", &file_id, "-o", out.to_str().unwrap()]),
        "INVALID_INPUT"
    );
    // DT-F2: A deleting its copy leaves the backup. DT-F3: B deleting the
    // backup leaves B's file, and A sees the deletion only as an entry gone.
    a.run(&["files", "rm", &file_id]);
    a.run(&["files", "purge", &file_id]);
    assert_eq!(a.fail(&["files", "show", &file_id]), "NOT_FOUND");
    assert_eq!(
        a.run(&["files", "cloud", "ls", "--refresh"])
            .as_array()
            .unwrap()
            .len(),
        1
    );
    b.run(&["files", "cloud", "rm", &upload]);
    assert!(
        b.run(&["files", "cloud", "ls"])
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        b.run(&["files", "show", &file_id])["file"]["status"],
        "local"
    );
    let again = env.dir.join("big.again");
    b.run(&["files", "get", &file_id, "-o", again.to_str().unwrap()]);
    assert_eq!(
        sha256_hex(&std::fs::read(&again).unwrap()),
        sha256_hex(&big)
    );
    assert!(
        a.run(&["files", "cloud", "ls", "--refresh"])
            .as_array()
            .unwrap()
            .is_empty()
    );
    drop(big);
    eprintln!("100 MiB backup round trip done at {:?}", started.elapsed());

    // AT05: A's daemon dies while a backup uploads; after a restart and a
    // new login the job resumes in the background, and nothing is sent
    // twice.
    let medium = pattern(64 * 1024 * 1024, 11);
    let medium_path = env.dir.join("medium.bin");
    std::fs::write(&medium_path, &medium).unwrap();
    let medium_id = a.run(&["files", "add", medium_path.to_str().unwrap()])[0]["file"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut backup = a
        .command(&["files", "backup", &medium_id])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(1000));
    let index = env.daemons[0];
    let mut killed = env.children.0.remove(index);
    killed.kill().unwrap();
    killed.wait().unwrap();
    if env.daemons[1] > index {
        env.daemons[1] -= 1;
    }
    assert!(
        !backup.wait().unwrap().success(),
        "the backup finished first"
    );
    env.daemons[0] = a.spawn(&mut env.children);
    assert_eq!(a.run(&["status"])["state"], "locked");
    assert_eq!(
        a.run(&["files", "backup-status"]).as_array().unwrap().len(),
        1
    );
    // The pending backup leaves the vault's jobs alone: the restarted
    // daemon's first background flush succeeds while the backup waits.
    let flushed = eventually(40, || {
        let lines = a.run(&["logs", "-n", "100000"]);
        let lines: Vec<&str> = lines
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap())
            .collect();
        let start = lines.iter().rposition(|l| l.contains(" daemon/start "))?;
        lines[start..]
            .iter()
            .find(|l| l.contains(" flush outcome="))
            .map(|l| l.to_string())
    });
    assert!(flushed.contains(" flush outcome=ok"), "{flushed}");
    let status = a.login(&env.http, 41).unwrap();
    assert_eq!(status["state"], "unlocked");
    let deadline = Instant::now() + Duration::from_secs(180);
    let medium_upload = loop {
        let shown = a.run(&["files", "show", &medium_id]);
        if shown["file"]["status"] == "backed_up" {
            break shown["versions"][0]["backup"]["upload_id"]
                .as_str()
                .unwrap()
                .to_string();
        }
        assert!(Instant::now() < deadline, "backup did not resume: {shown}");
        std::thread::sleep(Duration::from_secs(2));
    };
    assert!(
        a.run(&["files", "backup-status"])
            .as_array()
            .unwrap()
            .is_empty()
    );
    let a_report = a.run(&["sync"]);
    let b_report = b.run(&["sync"]);
    assert_eq!(a_report["revisions"], b_report["revisions"]);
    // Eight versions of the shared item (the refused late edit is not one);
    // the library adds none.
    assert_eq!(b_report["revisions"], 8);
    b.run(&["files", "cloud", "refresh"]);
    b.run(&["files", "cloud", "get", &medium_upload]);
    let out = env.dir.join("medium.out");
    b.run(&["files", "get", &medium_id, "-o", out.to_str().unwrap()]);
    assert_eq!(
        sha256_hex(&std::fs::read(&out).unwrap()),
        sha256_hex(&medium)
    );
    eprintln!("interrupted backup done at {:?}", started.elapsed());

    // FS3: share links to the medium file, one for the longest time allowed
    // with its key in the fragment, one behind a password (set through the
    // daemon: the CLI asks a terminal for it). The relay publishes them
    // through its outbox; anyone reads the public descriptor, a manifest
    // read counts a download, the share page's decryption (dmsg_frontend
    // `$lib/share`) opens both to the shared bytes, and after the revoke
    // the link answers 410 while A keeps the last count.
    let link = a.run(&[
        "files",
        "link",
        &medium_id,
        "--days",
        "30",
        "--max-downloads",
        "3",
    ]);
    let share = link["share_id"].as_str().unwrap().to_string();
    assert!(
        link["url"]
            .as_str()
            .unwrap()
            .contains(&format!("/s/{share}#")),
        "{link}"
    );
    let guarded = a
        .rpc(
            &env.http,
            "files/share_link",
            json!({"id": medium_id, "password": "correct horse"}),
        )
        .unwrap();
    let guarded_share = guarded["share_id"].as_str().unwrap().to_string();
    assert!(
        guarded["url"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/s/{guarded_share}")),
        "{guarded}"
    );
    let public = format!("{}/public/shares/{share}", env.relay);
    let published = |url: &str| {
        eventually(30, || {
            let response = env.http.get(url).send().unwrap();
            (response.status() == 200).then(|| response.json::<Value>().unwrap()["data"].clone())
        })
    };
    let descriptor = published(&public);
    assert_eq!(descriptor["status"], "available", "{descriptor}");
    assert_eq!(
        (&descriptor["max_downloads"], &descriptor["password"]),
        (&json!(3), &json!(false))
    );
    assert_eq!(descriptor["chunks"].as_array().unwrap().len(), 64);
    let wrapped = published(&format!("{}/public/shares/{guarded_share}", env.relay));
    assert_eq!(wrapped["password"], true, "{wrapped}");
    assert!(wrapped["key_wrap"].is_string());
    let manifest = env.http.get(format!("{public}/manifest")).send().unwrap();
    assert_eq!(manifest.status(), 200);
    assert_eq!(
        sha256_hex(&manifest.bytes().unwrap()),
        descriptor["manifest"]["digest"].as_str().unwrap()
    );
    let chunk = env.http.get(format!("{public}/chunks/63")).send().unwrap();
    assert_eq!(
        sha256_hex(&chunk.bytes().unwrap()),
        descriptor["chunks"][63]["digest"].as_str().unwrap()
    );
    let live = env.dir.join("share-live.json");
    let report = env.dir.join("share-live-report.json");
    let medium_sha = sha256_hex(&medium);
    std::fs::write(
        &live,
        json!([
            {"url": link["url"], "relay": env.relay, "sha256": medium_sha},
            {"url": guarded["url"], "relay": env.relay, "password": "correct horse", "sha256": medium_sha},
        ])
        .to_string(),
    )
    .unwrap();
    let opened = Command::new("pnpm")
        .args(["exec", "vitest", "run", "src/lib/share/live.test.ts"])
        .arg("--reporter=json")
        .arg(format!("--outputFile={}", report.display()))
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src/dmsg_frontend"))
        .env("DMSG_SHARE_LIVE", &live)
        .stdout(Stdio::null())
        .status()
        .unwrap();
    let report: Value = serde_json::from_str(&std::fs::read_to_string(&report).unwrap()).unwrap();
    assert!(
        opened.success() && report["numPassedTests"] == 1,
        "the share page did not open the links: {report}"
    );
    let count = |id: &str, downloads: u64| {
        eventually(30, || {
            let links = a.run(&["files", "links", "--refresh"]);
            links
                .as_array()
                .unwrap()
                .iter()
                .find(|l| l["share_id"] == id && l["downloads"] == downloads)
                .cloned()
        })
    };
    assert_eq!(count(&share, 2)["state"], "active");
    assert_eq!(count(&guarded_share, 1)["password"], true);
    let revoked = a.run(&["files", "link-revoke", &share]);
    assert_eq!(
        (&revoked["state"], &revoked["downloads"]),
        (&json!("revoked"), &json!(2))
    );
    // The directory learns of the revoke through the outbox; reading the
    // descriptor counts nothing.
    eventually(30, || {
        let response = env.http.get(&public).send().unwrap();
        (response.json::<Value>().unwrap()["data"]["status"] == "revoked").then_some(())
    });
    for path in ["manifest", "chunks/0"] {
        let response = env.http.get(format!("{public}/{path}")).send().unwrap();
        assert_eq!(response.status(), 410, "{path}");
    }
    eprintln!("share link done at {:?}", started.elapsed());

    // Profile: only the published fields, readable by anyone.
    let profile = a.run(&[
        "profile",
        "publish",
        "--name",
        "Desk Top",
        "--link",
        "https://example.com/me",
    ]);
    assert_eq!(profile["profile"]["version"], 1);
    assert_eq!(
        b.run(&["profile", "show"])["profile"]["display_name"],
        "Desk Top"
    );
    let other = b.run(&["profile", "resolve", &account]);
    assert_eq!(other["profile"]["links"][0], "https://example.com/me");
    assert_eq!(other["self_asserted"], true);
    assert_eq!(
        a.run(&["profile", "publish", "--bio", "only a bio"])["profile"]["display_name"],
        ""
    );

    // A public avatar (A4.4): the relay serves the image to anyone, and
    // publishing fields again keeps it; removing it keeps the fields.
    let png = [&[137u8, 80, 78, 71, 13, 10, 26, 10][..], &[7u8; 200]].concat();
    let picture = env.dir.join("avatar.png");
    std::fs::write(&picture, &png).unwrap();
    let with = a.run(&["profile", "avatar", picture.to_str().unwrap()]);
    assert_eq!(with["profile"]["version"], 3);
    assert_eq!(with["profile"]["bio"], "only a bio");
    let upload = with["profile"]["avatar_upload"]
        .as_str()
        .unwrap()
        .to_string();
    let image = b
        .rpc(
            &env.http,
            "profile/avatar_image",
            json!({"account": account, "upload": upload}),
        )
        .unwrap();
    assert_eq!(
        image["url"],
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&png)
        )
    );
    let public = format!("{}/public/accounts/{account}/avatar/{upload}", env.relay);
    let served = env.http.get(&public).send().unwrap();
    assert_eq!(served.headers()["content-type"], "image/png");
    assert_eq!(served.bytes().unwrap().as_ref(), &png[..]);
    let kept = a.run(&["profile", "publish", "--name", "Desk Top"]);
    assert_eq!(kept["profile"]["avatar_upload"], upload.as_str());
    let gif = env.dir.join("avatar.gif");
    std::fs::write(&gif, b"GIF89a-not-an-avatar").unwrap();
    assert_eq!(
        a.fail(&["profile", "avatar", gif.to_str().unwrap()]),
        "INVALID_INPUT"
    );
    // The relay's edge cache may keep serving the old bytes at the public
    // address for its max-age (an hour); the profile no longer names them.
    let removed = a.run(&["profile", "avatar", "--remove"]);
    assert!(removed["profile"]["avatar_upload"].is_null());
    assert_eq!(removed["profile"]["display_name"], "Desk Top");
    assert!(b.run(&["profile", "resolve", &account])["profile"]["avatar_upload"].is_null());

    // AT04: after locking, the home holds no plaintext.
    let marker = b"FILE-PLAINTEXT-MARKER-4471";
    let note_path = env.dir.join("marker-name-4471.txt");
    std::fs::write(&note_path, marker).unwrap();
    a.run(&["files", "add", note_path.to_str().unwrap()]);
    a.run_with(
        &[
            "vault",
            "add",
            "--title",
            "title-marker-4471",
            "--body",
            "body-marker-4471",
            "--secret-stdin",
        ],
        Some("secret-marker-4471"),
    );
    a.run(&["sync"]);
    // DT03: locked, nothing reads or signs.
    assert_eq!(a.run(&["lock"])["state"], "locked");
    assert_eq!(a.fail(&["vault", "list"]), "LOCKED");
    assert_eq!(a.fail(&["files", "verify", &medium_id]), "LOCKED");
    assert_eq!(a.fail(&["files", "ls"]), "LOCKED");
    assert_eq!(a.fail(&["sync"]), "LOCKED");
    assert_eq!(a.fail(&["profile", "publish", "--name", "x"]), "LOCKED");
    assert_eq!(a.fail(&["fast-unlock", "enable"]), "NOT_FOUND");
    for (path, bytes) in files_under(&a.home) {
        for needle in [
            &marker[..],
            b"marker-name-4471",
            b"title-marker-4471",
            b"body-marker-4471",
            b"secret-marker-4471",
            b"from B",
            b"late edit",
        ] {
            assert!(
                !contains(&bytes, needle),
                "{} holds {}",
                path.display(),
                String::from_utf8_lossy(needle)
            );
        }
    }

    // DT03: with background online, a locked B still sends what it sealed,
    // and still reads nothing.
    let pending = b.run(&["vault", "add", "--title", "sent while locked"]);
    let status = b.run(&["lock"]);
    assert_eq!(
        (status["state"].as_str(), status["resident"].as_bool()),
        (Some("locked"), Some(true))
    );
    assert_eq!(b.fail(&["vault", "list"]), "LOCKED");
    let report = b.run(&["sync"]);
    assert_eq!(
        (report["sent"].as_u64(), report["revisions"].as_u64()),
        (Some(1), Some(0))
    );
    assert_eq!(b.run(&["sync", "status"])["pending"], 0);
    assert_eq!(a.login(&env.http, 41).unwrap()["state"], "unlocked");
    assert_eq!(
        a.run(&["vault", "show", pending["id"].as_str().unwrap()])["item"]["title"],
        "sent while locked"
    );

    // AT07 and AT21: A revokes B while B's daemon runs unlocked. A moves to
    // a new root at once; B's next sync finds the device revoked and erases
    // its workspace, so what it wrote afterwards never reaches the account.
    assert_eq!(b.login(&env.http, 41).unwrap()["state"], "unlocked");
    let b_device = b.run(&["status"])["device_id"]
        .as_str()
        .unwrap()
        .to_string();
    let generation = a.run(&["status"])["root_generation"].as_u64().unwrap();
    let revoke = a.run(&["device", "revoke", &b_device]);
    assert_eq!(a.decide(&env.http, &revoke, true).unwrap()["state"], "done");
    assert_eq!(
        a.run(&["status"])["root_generation"].as_u64(),
        Some(generation + 1)
    );
    b.run(&["vault", "add", "--title", "written after revocation"]);
    assert_eq!(b.fail(&["sync"]), "DEVICE_NOT_APPROVED");
    let erased = b.run(&["status"]);
    assert_ne!(erased["device_id"], b_device.as_str(), "{erased}");
    assert!(erased["root_generation"].is_null());
    assert_eq!(b.fail(&["vault", "list"]), "LOCKED");
    a.run(&["sync"]);
    assert!(!titles(a).contains(&"written after revocation".to_string()));
    eprintln!("content acceptance done at {:?}", started.elapsed());
    env.finish();
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn desktop_idle_lock_and_scheduled_sync() {
    let env = Env::start("idle", "idle_lock_minutes = 1\nsync_interval_minutes = 1\n");
    env.pair();
    let (a, b) = (&env.a, &env.b);
    // B's last activity is its unlock while pairing; `status` is not one.
    let since = now_ms();
    a.run(&["vault", "add", "--title", "for the scheduled sync"]);
    assert_eq!(a.run(&["sync"])["sent"], 1);
    // About a minute after its daemon started, B syncs on its own; a
    // minute after its last activity, it locks.
    let status = eventually(150, || {
        let status = b.run(&["status"]);
        (status["state"] == "locked").then_some(status)
    });
    assert!(
        status["last_sync"]["at"].as_u64().unwrap() > since,
        "{status}"
    );
    assert!(status["last_sync"]["error"].is_null(), "{status}");
    assert_eq!(b.fail(&["vault", "list"]), "LOCKED");
    assert_eq!(b.login(&env.http, 41).unwrap()["state"], "unlocked");
    assert!(titles(b).contains(&"for the scheduled sync".to_string()));
    env.finish();
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn desktop_approvals_and_attestation() {
    let env = Env::start("attest", "");
    env.pair();
    let (a, b, http) = (&env.a, &env.b, &env.http);
    let port = a.port();

    // DT04: no credential, no state, on every entry point.
    for (method, path) in [
        ("POST", "/app/v1"),
        ("GET", "/ws/app/v1"),
        ("GET", "/daemon/status"),
    ] {
        let request = http
            .request(
                method.parse().unwrap(),
                format!("http://127.0.0.1:{port}{path}"),
            )
            .header("connection", "upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==");
        assert_eq!(request.send().unwrap().status(), 401, "{path}");
    }

    // The app WebSocket: methods and events with the owner credential.
    let mut socket = {
        use tungstenite::client::IntoClientRequest;
        let mut request = format!("ws://127.0.0.1:{port}/ws/app/v1")
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", a.token()).parse().unwrap(),
        );
        tungstenite::connect(request).unwrap().0
    };
    socket
        .send(tungstenite::Message::Text(
            json!({"jsonrpc": "2.0", "id": 7, "method": "app/initialize"})
                .to_string()
                .into(),
        ))
        .unwrap();
    let mut seen = vec![];
    let next = |socket: &mut tungstenite::WebSocket<_>| -> Value {
        loop {
            if let tungstenite::Message::Text(text) = socket.read().unwrap() {
                return serde_json::from_str(&text).unwrap();
            }
        }
    };
    let init = next(&mut socket);
    assert_eq!(init["id"], 7);
    assert_eq!(init["result"]["protocol"], "dmsg-app/1");
    assert_eq!(init["result"]["status"]["state"], "unlocked");

    // AT13 / DT01: a request from a script waits; only a person decides it.
    let request = a.run(&["attest", "text", "dMsg desktop W3 statement"]);
    assert_eq!(request["state"], "awaiting_user");
    assert_eq!(request["source"], "cli");
    let id = request["id"].as_str().unwrap();
    assert_eq!(a.fail(&["approvals", "approve", id]), "FORBIDDEN");
    assert_eq!(
        a.rpc(
            http,
            "approval/respond",
            json!({"id": id, "digest": request["digest"], "approve": true})
        ),
        Err("FORBIDDEN".into())
    );
    let event = loop {
        let message = next(&mut socket);
        if message["method"] == "event" {
            seen.push(message["params"]["type"].clone());
            if message["params"]["type"] == "approval/changed" {
                break message;
            }
        }
    };
    assert!(event["params"]["seq"].as_u64().unwrap() > 0);
    let gui = a
        .rpc(
            http,
            "attest/request",
            json!({"source": "gui", "statement": {"profile": "text", "text": "From the desktop"}}),
        )
        .unwrap();
    assert_eq!(gui["source"], "gui");
    // A changed request is refused: the digest binds what the person saw.
    let mut stale = request.clone();
    stale["digest"] = "00".repeat(32).into();
    assert_eq!(a.decide(http, &stale, true), Err("INTEGRITY_FAILED".into()));

    // Without FormalApprove the home refuses; nothing was signed.
    assert_eq!(a.decide(http, &request, true), Err("FORBIDDEN".into()));
    assert_eq!(a.run(&["approvals", "show", id])["state"], "failed");

    // FormalApprove for this device: a single administrator approves itself.
    let enable = a.run(&["device", "formal-approve", "on"]);
    assert_eq!(enable["type"], "device_capabilities");
    assert_eq!(enable["detail"]["this_device"], true);
    assert_eq!(a.decide(http, &enable, true).unwrap()["state"], "done");
    let devices = a.run(&["device", "list"]);
    let own = devices
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["current"] == true)
        .unwrap();
    assert!(
        own["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("FormalApprove"))
    );

    // A file statement, approved here and verified elsewhere.
    let file = env.dir.join("release.txt");
    std::fs::write(&file, b"release 1.0 artifact bytes").unwrap();
    let file_text = file.to_str().unwrap();
    let request = a.run(&[
        "attest",
        "file",
        file_text,
        "--text",
        "I approve release 1.0",
        "--content-type",
        "text/plain",
    ]);
    assert_eq!(request["detail"]["file_checked"], true);
    let done = a.decide(http, &request, true).unwrap();
    assert_eq!(done["state"], "done");
    let execution = done["execution"].as_str().unwrap().to_string();
    let history = a.run(&["attest", "history"]);
    let entry = history
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["execution"] == execution.as_str())
        .unwrap();
    assert_eq!(entry["stage"], "complete");
    assert_eq!(entry["origin"], "https://dmsg.net");
    let bundle = env.dir.join("release.attestation.json");
    a.run(&[
        "attest",
        "export",
        &execution,
        "-o",
        bundle.to_str().unwrap(),
    ]);
    // Independent verification: another home, no daemon involved.
    b.run(&["stop"]);
    let report = b.run(&[
        "attest",
        "verify",
        bundle.to_str().unwrap(),
        "--file",
        file_text,
    ]);
    assert_eq!(report["content"], "matches_file");
    assert_eq!(report["signature"], true);
    assert_eq!(report["origin"], "https://dmsg.net");
    assert_eq!(report["current"], "active");
    assert_eq!(report["statement"]["profile"], "file_statement");
    std::fs::write(&file, b"release 1.0 artifact bytes, changed").unwrap();
    assert_eq!(
        b.fail(&[
            "attest",
            "verify",
            bundle.to_str().unwrap(),
            "--file",
            file_text
        ]),
        "INTEGRITY_FAILED"
    );

    // The file changes between request and approval: refused.
    let request = a.run(&["attest", "digest", "--file", file_text]);
    std::fs::write(&file, b"yet another version").unwrap();
    assert_eq!(
        a.decide(http, &request, true),
        Err("INTEGRITY_FAILED".into())
    );
    // A digest without the file: verifiable as "digest only".
    let digest = sha256_hex(b"document bytes");
    let request = a.run(&["attest", "digest", "--sha256", &digest]);
    let done = a.decide(http, &request, true).unwrap();
    let bundle = env.dir.join("digest.attestation.json");
    a.run(&[
        "attest",
        "export",
        done["execution"].as_str().unwrap(),
        "-o",
        bundle.to_str().unwrap(),
    ]);
    assert_eq!(
        a.run(&["attest", "verify", bundle.to_str().unwrap()])["content"],
        "digest_only"
    );
    // Rejected and cancelled requests execute nothing.
    let request = a.run(&["attest", "text", "never signed"]);
    assert_eq!(
        a.decide(http, &request, false).unwrap()["state"],
        "rejected"
    );
    let request = a.run(&["attest", "text", "withdrawn"]);
    assert_eq!(
        a.run(&["approvals", "cancel", request["id"].as_str().unwrap()])["state"],
        "cancelled"
    );

    // Locked: the queue's content is sealed; its size is not.
    let waiting = a.run(&["attest", "text", "waits while locked"]);
    a.run(&["lock"]);
    assert_eq!(a.fail(&["approvals", "list"]), "LOCKED");
    let status = a.run(&["status"]);
    assert!(status["approvals_waiting"].as_u64().unwrap() >= 2);
    assert!(a.decide(http, &waiting, true).is_err());
    assert!(seen.contains(&json!("approval/changed")));
    drop(socket);
    env.finish();
}

/// The test application's action schema: one command with one argument.
fn action_schema() -> dmsg_types::app_action::ActionSchema {
    use dmsg_types::app_action::*;
    let label = |text: &str| {
        vec![ActionLabel {
            locale: "en".into(),
            text: text.into(),
        }]
    };
    ActionSchema {
        version: 1,
        commands: vec![CommandSchema {
            name: "CertifyDisclosure".into(),
            title: label("Certify disclosure draft"),
            fields: SchemaFields(vec![FieldSchema {
                name: "revision".into(),
                label: label("Draft revision"),
                ty: FieldType::Nat {
                    min: 1,
                    max: u64::MAX,
                },
            }]),
        }],
    }
}

/// The single request of `operation` waiting on a device, if any.
fn web_request(
    device: &Desktop,
    http: &reqwest::blocking::Client,
    operation: &str,
) -> Option<Value> {
    device
        .waiting(http, "connect")
        .into_iter()
        .find(|r| r["detail"]["operation_id"] == operation)
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC, the private relay (DMSG_CLOUD_DIR) and Node"]
fn desktop_web_connect() {
    use dmsg_types::integration::{AppCapability, AppRegistration, SigningProfile};
    let mut env = Env::start("connect", "background_online = true\n");
    let account = env.pair();
    let client = env.http.clone();
    let http = &client;
    // The SNS stand-in is the application's action authority.
    env.deployment.register_app(&AppRegistration {
        version: 1,
        environment: dmsg_types::Environment::Local,
        app_id: "token-list".into(),
        config_version: 1,
        origins: vec!["http://localhost:5173".into()],
        product_ids: vec![],
        capabilities: vec![
            AppCapability::Authenticate,
            AppCapability::SignDocument,
            AppCapability::SignAction,
        ],
        profiles: vec![
            SigningProfile::TextStatementV1,
            SigningProfile::DigestStatementV1,
            SigningProfile::FileStatementV1,
            SigningProfile::AppActionV1,
        ],
        authentication_receiver: env.deployment.user,
        action_authority: env.deployment.sns,
        action_schema: Some(action_schema()),
        paused: false,
    });
    for device in [&env.a, &env.b] {
        let request = device.run(&["device", "formal-approve", "on"]);
        assert_eq!(
            device.decide(http, &request, true).unwrap()["state"],
            "done"
        );
    }
    let issuer = format!("https://dmsg.test/u/{account}");
    let origin = "http://localhost:5173";

    // Pairing (§5): B invites, the bridge answers with its key and label.
    let invite = env.b.rpc(http, "connect/invite", Value::Null).unwrap();
    let invite_id = invite["invite_id"].as_str().unwrap().to_string();
    let url = invite["url"].as_str().unwrap().to_string();
    assert!(url.starts_with(&format!("{}/v1/pair#", env.connect)));
    let mut bridge = Headless::start(&env, &env.relay);
    let paired = bridge
        .call(json!({"cmd": "pair", "url": url, "label": "Chrome · Test"}))
        .unwrap();
    assert_eq!(paired["account"], account.as_str());
    // DT18: a phishing page without the link's secret submits its own key.
    let (page, fragment) = url.split_once('#').unwrap();
    let mut forged: dmsg_types::connect::PairingInvite =
        cbor2::from_slice(&URL_SAFE_NO_PAD.decode(fragment).unwrap()).unwrap();
    forged.secret = dmsg_types::Hash::new([7; 32]);
    let forged_url = format!("{page}#{}", URL_SAFE_NO_PAD.encode(canonical(&forged)));
    let mut phishing = Headless::start(&env, &env.relay);
    let evil = phishing
        .call(json!({"cmd": "pair", "url": forged_url, "label": "Chrome · Test"}))
        .unwrap();
    let candidates = env
        .b
        .rpc(http, "connect/candidates", json!({"invite": invite_id}))
        .unwrap();
    assert_eq!(candidates["candidates"].as_array().unwrap().len(), 1);
    assert_eq!(
        candidates["candidates"][0]["pairing_id"],
        paired["pairingId"]
    );
    assert_eq!(candidates["rejected"], 1);
    let grant = |pairing: &Value, human: bool| {
        env.b.rpc(
            http,
            "connect/grant",
            json!({"invite": invite_id, "pairing": pairing, "human": human}),
        )
    };
    assert_eq!(grant(&paired["pairingId"], false), Err("FORBIDDEN".into()));
    assert_eq!(grant(&evil["pairingId"], true), Err("NOT_FOUND".into()));
    grant(&paired["pairingId"], true).unwrap();
    bridge
        .call(json!({"cmd": "await_grant", "account": account, "timeout_ms": 30000}))
        .unwrap();
    assert_eq!(
        phishing.call(json!({"cmd": "await_grant", "account": account, "timeout_ms": 3000})),
        Err("NOT_PAIRED".into())
    );
    let connection = bridge
        .call(json!({"cmd": "connect", "origin": origin, "app_id": "token-list"}))
        .unwrap()["connection"]
        .clone();
    assert_eq!(
        bridge.call(
            json!({"cmd": "connect", "origin": "http://localhost:6666", "app_id": "token-list"})
        ),
        Err("APP_NOT_REGISTERED".into())
    );
    let sign = |bridge: &mut Headless, text: &str, extra: Value| {
        let mut command = json!({
            "cmd": "sign_document", "connection": connection, "account": account,
            "issuer": issuer, "text": text,
        });
        for (k, v) in extra.as_object().unwrap() {
            command[k] = v.clone();
        }
        bridge.call(command)
    };

    // DT26: locked devices learn of a request without its content.
    for device in [&env.a, &env.b] {
        eventually(90, || {
            (device.run(&["status"])["hints"]["connected"] == true).then_some(())
        });
    }
    env.a.run(&["lock"]);
    env.b.run(&["lock"]);
    let hints = env.a.run(&["status"])["hints"]["received"]
        .as_u64()
        .unwrap();
    let first = sign(&mut bridge, "Request while locked", json!({})).unwrap();
    assert_eq!(first["state"], "awaiting_user");
    let op1 = first["operationId"].as_str().unwrap().to_string();
    eventually(30, || {
        (env.a.run(&["status"])["hints"]["received"]
            .as_u64()
            .unwrap()
            > hints)
            .then_some(())
    });
    // B keeps its device key (background online): it fetched the sealed request.
    eventually(30, || {
        (env.b.run(&["status"])["sealed_requests"].as_u64().unwrap() >= 1).then_some(())
    });
    assert_eq!(env.b.fail(&["approvals", "list"]), "LOCKED");
    assert_eq!(env.a.run(&["status"])["sealed_requests"], 0);
    env.a.login(http, 41).unwrap();
    env.b.login(http, 41).unwrap();
    let shown = eventually(30, || web_request(&env.a, http, &op1));
    assert_eq!(shown["detail"]["origin"], origin);
    assert_eq!(shown["detail"]["pairing_label"], "Chrome · Test");
    assert_eq!(shown["detail"]["relayed_by"], "connect.dmsg.net");
    assert_eq!(shown["source"], format!("connect:{origin}"));

    // DT23: a cancellation before authorization wins; nothing executes.
    bridge
        .call(json!({"cmd": "cancel", "connection": connection, "operation_id": op1}))
        .unwrap();
    eventually(30, || {
        let _ = env.a.rpc(http, "connect/sync", Value::Null);
        let state = env
            .a
            .rpc(http, "approval/show", json!({"id": shown["id"]}))
            .unwrap()["state"]
            .clone();
        (state == "cancelled").then_some(())
    });
    assert!(env.a.decide(http, &shown, true).is_err());

    // DT22: created while A is offline, approved on B; A then stands aside.
    env.a.run(&["stop"]);
    let second = sign(&mut bridge, "Approve on the other device", json!({})).unwrap();
    let op2 = second["operationId"].as_str().unwrap().to_string();
    let on_b = eventually(30, || web_request(&env.b, http, &op2));
    assert_eq!(env.b.decide(http, &on_b, true).unwrap()["state"], "done");
    let signed = bridge
        .call(json!({"cmd": "get", "connection": connection, "operation_id": op2, "wait_ms": 20000, "until": ["signed"]}))
        .unwrap();
    assert_eq!(signed["state"], "signed");
    assert!(signed["result"]["artifact"]["cose_sign1"].is_string());
    assert!(signed["result"]["executionCertificateCbor"].is_string());
    env.restart(0);
    env.a.login(http, 41).unwrap();
    let _ = env.a.rpc(http, "connect/sync", Value::Null);
    let all = env
        .a
        .rpc(http, "approval/list", json!({"all": true}))
        .unwrap();
    // A sees the request only as handled elsewhere: nothing to decide.
    assert!(
        all.as_array()
            .unwrap()
            .iter()
            .filter(|r| r["detail"]["operation_id"] == op2.as_str())
            .all(|r| r["state"] == "elsewhere")
    );
    // The page confirms the result; the device records it.
    let returned = bridge
        .call(json!({"cmd": "ack", "connection": connection, "operation_id": op2}))
        .unwrap();
    assert_eq!(returned["state"], "returned");

    // DT23: two devices approve at once; exactly one executes.
    let third = sign(&mut bridge, "Race", json!({})).unwrap();
    let op3 = third["operationId"].as_str().unwrap().to_string();
    let on_a = eventually(30, || web_request(&env.a, http, &op3));
    let on_b = eventually(30, || web_request(&env.b, http, &op3));
    let (ra, rb) = std::thread::scope(|s| {
        let ta = s.spawn(|| env.a.decide(http, &on_a, true));
        let tb = s.spawn(|| env.b.decide(http, &on_b, true));
        (ta.join().unwrap(), tb.join().unwrap())
    });
    let mut states = vec![
        ra.unwrap()["state"].as_str().unwrap().to_string(),
        rb.unwrap()["state"].as_str().unwrap().to_string(),
    ];
    states.sort();
    assert_eq!(states, ["done", "elsewhere"]);
    let race = bridge
        .call(json!({"cmd": "get", "connection": connection, "operation_id": op3, "wait_ms": 20000, "until": ["signed"]}))
        .unwrap();
    assert_eq!(race["state"], "signed");

    // authenticate: the page's request commits to its session key; the
    // result is a certified record the application checks on its own.
    let d = &env.deployment;
    let account_id: dmsg_types::AccountId = account.parse().unwrap();
    let b64 = |bytes: Vec<u8>| base64::engine::general_purpose::STANDARD.encode(bytes);
    let create = |bridge: &mut Headless, method: &str, operation: &str, payload: Vec<u8>| {
        bridge.call(json!({
            "cmd": "create", "connection": connection, "method": method,
            "operation_id": operation, "payload": b64(payload),
        }))
    };
    let auth_op = dmsg_types::Hash::new([0xa5; 32]);
    let auth_id = hex::encode(*auth_op);
    let session = bridge
        .call(json!({"cmd": "session", "operation_id": auth_id}))
        .unwrap();
    let spki = base64::engine::general_purpose::STANDARD
        .decode(session["publicKey"].as_str().unwrap())
        .unwrap();
    let issued = now_ms() - 1_000;
    let request = dmsg_types::integration::AuthenticationRequest {
        version: 1,
        environment: dmsg_types::Environment::Local,
        app_id: "token-list".into(),
        app_config_version: 1,
        origin: origin.into(),
        receiver: d.user,
        challenge_hash: dmsg_types::Hash::new([21; 32]),
        session_key_hash: dmsg_protocol::sha256(&spki),
        purpose: dmsg_types::integration::AuthenticationPurpose::Login,
        nonce: dmsg_types::Hash::new([23; 32]),
        operation_id: auth_op,
        issued_at_ms: issued,
        expires_at_ms: issued + dmsg_types::integration::AUTH_TTL_MS,
    };
    let created = create(&mut bridge, "authenticate", &auth_id, canonical(&request)).unwrap();
    assert_eq!(created["state"], "awaiting_user");
    let shown = eventually(30, || web_request(&env.b, http, &auth_id));
    assert_eq!(shown["detail"]["method"], "authenticate");
    assert_eq!(env.b.decide(http, &shown, true).unwrap()["state"], "done");
    let authenticated = bridge
        .call(json!({"cmd": "get", "connection": connection, "operation_id": auth_id, "wait_ms": 20000, "until": ["signed"]}))
        .unwrap();
    assert_eq!(authenticated["state"], "signed", "{authenticated}");
    let proof: dmsg_types::CertifiedBatch = cbor2::from_slice(
        &base64::engine::general_purpose::STANDARD
            .decode(authenticated["result"]["proof"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    let root = hex::decode(&d.root_key_hex).unwrap();
    let result = dmsg_protocol::authentication::verify_authentication(
        &proof,
        &request,
        d.user,
        &root,
        now_ms(),
    )
    .unwrap();
    assert_eq!(result.account_id, account_id);

    // signAction: the device asks the product authority before it takes
    // the request (inspect_app_action); a refused action is rejected and
    // nothing is signed, an accepted one is attested.
    let action = |op: u8| {
        let issued = now_ms() - 1_000;
        dmsg_types::app_action::AppAction {
            version: 1,
            environment: dmsg_types::Environment::Local,
            app_id: "token-list".into(),
            app_config_version: 1,
            origin: origin.into(),
            receiver: d.sns,
            actor: vec![8; 12].into(),
            signing_account: account_id,
            operation_id: dmsg_types::Hash::new([op; 32]),
            intent_hash: dmsg_types::Hash::new([op; 32]),
            schema_hash: dmsg_protocol::app_action::action_schema_hash(&action_schema()),
            issued_at_ms: issued,
            expires_at_ms: issued + 4 * dmsg_types::MINUTE,
            command: dmsg_types::app_action::ActionCommand {
                name: "CertifyDisclosure".into(),
                args: dmsg_types::app_action::ActionArgs(vec![dmsg_types::app_action::ActionArg {
                    name: "revision".into(),
                    value: dmsg_types::app_action::ActionValue::Nat(u64::from(op)),
                }]),
            },
            files: vec![],
        }
    };
    let (accepted, refused) = (action(0xa6), action(0xa7));
    d.ic.update_call(
        d.sns,
        d.peer,
        "set_action_approval",
        candid::encode_args((
            d.user,
            account_id,
            accepted.clone(),
            None::<(Principal, AppRegistration)>,
        ))
        .unwrap(),
    )
    .unwrap();
    let refused_id = "a7".repeat(32);
    create(&mut bridge, "signAction", &refused_id, canonical(&refused)).unwrap();
    let shown = eventually(30, || web_request(&env.b, http, &refused_id));
    assert_eq!(env.b.decide(http, &shown, true), Err("FORBIDDEN".into()));
    let page = bridge
        .call(json!({"cmd": "get", "connection": connection, "operation_id": refused_id, "wait_ms": 20000, "until": ["rejected"]}))
        .unwrap();
    assert_eq!(
        (page["state"].as_str(), page["error"].as_str()),
        (Some("rejected"), Some("FORBIDDEN")),
        "{page}"
    );
    let accepted_id = "a6".repeat(32);
    create(
        &mut bridge,
        "signAction",
        &accepted_id,
        canonical(&accepted),
    )
    .unwrap();
    let shown = eventually(30, || web_request(&env.b, http, &accepted_id));
    assert_eq!(shown["detail"]["method"], "signAction");
    assert_eq!(env.b.decide(http, &shown, true).unwrap()["state"], "done");
    let signed = bridge
        .call(json!({"cmd": "get", "connection": connection, "operation_id": accepted_id, "wait_ms": 20000, "until": ["signed"]}))
        .unwrap();
    assert_eq!(signed["state"], "signed", "{signed}");
    assert!(signed["result"]["artifact"]["cose_sign1"].is_string());
    assert!(signed["result"]["executionCertificateCbor"].is_string());

    // The same action from this device (`dmsg attest action`): the current
    // registration admits it when it is asked for, the product authority
    // when it is approved; a refused one fails without a signature.
    let local = |name: &str, action: &dmsg_types::app_action::AppAction| {
        let path = env.dir.join(name);
        std::fs::write(&path, canonical(action)).unwrap();
        path.to_str().unwrap().to_string()
    };
    let stale = dmsg_types::app_action::AppAction {
        app_config_version: 2,
        ..action(0xa8)
    };
    assert_eq!(
        env.b
            .fail(&["attest", "action", &local("stale.cbor", &stale)]),
        "FORBIDDEN"
    );
    let request = env
        .b
        .run(&["attest", "action", &local("refused.cbor", &action(0xa9))]);
    assert_eq!(request["source"], "cli");
    assert_eq!(
        request["detail"]["statement"]["content"]["command"],
        "CertifyDisclosure"
    );
    assert_eq!(env.b.decide(http, &request, true), Err("FORBIDDEN".into()));
    let failed = env
        .b
        .rpc(http, "approval/show", json!({"id": request["id"]}))
        .unwrap();
    assert_eq!(
        (failed["state"].as_str(), failed["execution"].as_str()),
        (Some("failed"), None)
    );
    let approved = action(0xaa);
    d.ic.update_call(
        d.sns,
        d.peer,
        "set_action_approval",
        candid::encode_args((
            d.user,
            account_id,
            approved.clone(),
            None::<(Principal, AppRegistration)>,
        ))
        .unwrap(),
    )
    .unwrap();
    let request = env
        .b
        .run(&["attest", "action", &local("approved.cbor", &approved)]);
    let done = env.b.decide(http, &request, true).unwrap();
    assert_eq!(done["state"], "done", "{done}");
    let bundle = env.dir.join("action.attestation.json");
    env.b.run(&[
        "attest",
        "export",
        done["execution"].as_str().unwrap(),
        "-o",
        bundle.to_str().unwrap(),
    ]);
    let verified = env.b.run(&["attest", "verify", bundle.to_str().unwrap()]);
    assert_eq!(
        (verified["content"].as_str(), verified["origin"].as_str()),
        (Some("action"), Some(origin))
    );

    // DT31: the same operation from a second browser: one approval, both
    // get the result. A pairs this browser, so it outlives B's revocation.
    let invite = env.a.rpc(http, "connect/invite", Value::Null).unwrap();
    let mut second_browser = Headless::start(&env, &env.relay);
    let pairing = second_browser
        .call(json!({"cmd": "pair", "url": invite["url"], "label": "Firefox · Test"}))
        .unwrap();
    env.a
        .rpc(
            http,
            "connect/grant",
            json!({"invite": invite["invite_id"], "pairing": pairing["pairingId"], "human": true}),
        )
        .unwrap();
    second_browser
        .call(json!({"cmd": "await_grant", "account": account, "timeout_ms": 30000}))
        .unwrap();
    let connection2 = second_browser
        .call(json!({"cmd": "connect", "origin": origin, "app_id": "token-list"}))
        .unwrap()["connection"]
        .clone();
    let op4 = "c4".repeat(32);
    let same = json!({"operation_id": op4, "page_seed": "5e".repeat(32), "nonce": "6f".repeat(32), "expires_at": now_ms() + 600_000});
    sign(&mut bridge, "One operation, two browsers", same.clone()).unwrap();
    let mut duplicate = json!({
        "cmd": "sign_document", "connection": connection2, "account": account,
        "issuer": issuer, "text": "One operation, two browsers",
    });
    for (k, v) in same.as_object().unwrap() {
        duplicate[k] = v.clone();
    }
    second_browser.call(duplicate).unwrap();
    let on_b = eventually(30, || web_request(&env.b, http, &op4));
    assert_eq!(on_b["detail"]["channels"], 2);
    assert_eq!(
        env.b
            .waiting(http, "connect")
            .iter()
            .filter(|r| r["detail"]["operation_id"] == op4.as_str())
            .count(),
        1
    );
    env.b.decide(http, &on_b, true).unwrap();
    let mut digests = vec![];
    for (browser, connection) in [
        (&mut bridge, &connection),
        (&mut second_browser, &connection2),
    ] {
        let value = browser
            .call(json!({"cmd": "get", "connection": connection, "operation_id": op4, "wait_ms": 20000, "until": ["signed"]}))
            .unwrap();
        assert_eq!(value["state"], "signed");
        digests.push(value["resultDigest"].clone());
    }
    assert_eq!(digests[0], digests[1]);

    // DT29: a bridge that lost its record reads the operation from the devices.
    bridge
        .call(json!({"cmd": "forget", "connection": connection, "operation_id": op2}))
        .unwrap();
    let recovered = bridge
        .call(json!({"cmd": "get", "connection": connection, "operation_id": op2}))
        .unwrap();
    assert!(["signed", "returned"].contains(&recovered["state"].as_str().unwrap()));
    assert_eq!(
        bridge
            .call(json!({"cmd": "get", "connection": connection, "operation_id": "d5".repeat(32)})),
        Err("NOT_FOUND".into())
    );

    // DT24 and DT30: B crashes after it took an operation. A restart
    // reconciles without signing twice; revoking B ends what it held.
    let mut held = None;
    for delay in [600u64, 300, 120, 30] {
        let value = sign(
            &mut second_browser,
            &format!("Crash {delay}"),
            json!({"connection": connection2}),
        )
        .unwrap();
        let op = value["operationId"].as_str().unwrap().to_string();
        let on_b = eventually(30, || web_request(&env.b, http, &op));
        std::thread::scope(|s| {
            s.spawn(|| {
                let _ = env.b.decide(http, &on_b, true);
            });
            std::thread::sleep(Duration::from_millis(delay));
            let child = &mut env.children.0[env.daemons[1]];
            let _ = child.kill();
            let _ = child.wait();
        });
        let _ = env.a.rpc(http, "connect/sync", Value::Null);
        let all = env
            .a
            .rpc(http, "approval/list", json!({"all": true}))
            .unwrap();
        let taken = all
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["detail"]["operation_id"] == op.as_str() && r["state"] == "elsewhere");
        let page = second_browser
            .call(json!({"cmd": "get", "connection": connection2, "operation_id": op}))
            .unwrap();
        let open = !["signed", "failed", "returned"].contains(&page["state"].as_str().unwrap());
        if taken && open && held.is_none() {
            held = Some(op);
            break;
        }
        // DT24: restart, log in, reconcile; at most one attestation.
        env.restart(1);
        env.b.login(http, 41).unwrap();
        let _ = env.b.rpc(http, "connect/sync", Value::Null);
        let page = second_browser
            .call(json!({"cmd": "get", "connection": connection2, "operation_id": op, "wait_ms": 20000, "until": ["signed", "failed"]}))
            .unwrap();
        assert!(
            ["signed", "failed"].contains(&page["state"].as_str().unwrap()),
            "{page}"
        );
        let history = env.b.run(&["attest", "history"]);
        let approval = env
            .b
            .rpc(http, "approval/list", json!({"all": true}))
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["detail"]["operation_id"] == op.as_str())
            .unwrap()["id"]
            .clone();
        assert!(
            history
                .as_array()
                .unwrap()
                .iter()
                .filter(|h| h["approval"] == approval)
                .count()
                <= 1
        );
    }
    let held = held.expect("B was stopped after it took an operation");
    let devices = env.a.run(&["device", "list"]);
    let b_id = devices
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["current"] == false)
        .unwrap()["device_id"]
        .as_str()
        .unwrap()
        .to_string();
    let revoke = env.a.run(&["device", "revoke", &b_id]);
    assert_eq!(revoke["type"], "device_revoke");
    assert_eq!(env.a.decide(http, &revoke, true).unwrap()["state"], "done");
    // DT30: the request B held ends with DEVICE_REVOKED.
    let page = second_browser
        .call(json!({"cmd": "get", "connection": connection2, "operation_id": held}))
        .unwrap();
    assert_eq!(page["state"], "failed");
    assert_eq!(page["error"], "DEVICE_REVOKED");
    // DT25: the pairing B granted is gone; the one A granted stays.
    assert_eq!(
        sign(&mut bridge, "after revocation", json!({})),
        Err("NOT_PAIRED".into())
    );
    sign(
        &mut second_browser,
        "after revocation",
        json!({"connection": connection2}),
    )
    .unwrap();
    env.finish();
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC, the private relay (DMSG_CLOUD_DIR), Node and Playwright browsers"]
fn desktop_browser_connect() {
    use dmsg_types::integration::{AppCapability, AppRegistration, SigningProfile};
    let mut env = Env::start("browser", "");
    let account = env.pair();
    let client = env.http.clone();
    let http = &client;
    let (origin, stranger) = env.serve_web();
    env.deployment.register_app(&AppRegistration {
        version: 1,
        environment: dmsg_types::Environment::Local,
        app_id: "token-list".into(),
        config_version: 1,
        origins: vec![origin.clone()],
        product_ids: vec![],
        capabilities: vec![AppCapability::SignDocument],
        profiles: vec![SigningProfile::TextStatementV1],
        authentication_receiver: env.deployment.user,
        action_authority: env.deployment.user,
        action_schema: None,
        paused: false,
    });
    let request = env.b.run(&["device", "formal-approve", "on"]);
    assert_eq!(env.b.decide(http, &request, true).unwrap()["state"], "done");
    let issuer = format!("https://dmsg.test/u/{account}");

    let browsers = std::env::var("DMSG_TEST_BROWSERS").unwrap_or_else(|_| "chromium".into());
    for name in browsers.split(',').map(str::trim) {
        let mut browser = Headless::browser(name, &env.connect);
        // connect-v1 §2.2: no COOP same-origin, no COEP, no framing.
        let headers = browser.call(json!({"cmd": "headers"})).unwrap();
        let csp = headers["content-security-policy"].as_str().unwrap();
        assert!(csp.contains("frame-ancestors 'none'") && csp.contains("script-src 'self'"));
        assert_eq!(headers["cross-origin-opener-policy"], "unsafe-none");
        assert_eq!(headers["referrer-policy"], "no-referrer");
        assert!(headers.get("cross-origin-embedder-policy").is_none());

        // DT27, pairing: the device opens the link; the page shows its label.
        let invite = env.b.rpc(http, "connect/invite", Value::Null).unwrap();
        let page = browser
            .call(json!({"cmd": "pair", "url": invite["url"]}))
            .unwrap();
        assert_eq!(page["state"], "waiting", "{name}: {page}");
        assert_eq!(page["account"], account.as_str());
        let candidates = eventually(30, || {
            let found = env
                .b
                .rpc(
                    http,
                    "connect/candidates",
                    json!({"invite": invite["invite_id"]}),
                )
                .unwrap();
            (!found["candidates"].as_array().unwrap().is_empty()).then_some(found)
        });
        let candidate = &candidates["candidates"][0];
        assert_eq!(candidate["pairing_id"], page["pairing"]);
        let label = candidate["label"].as_str().unwrap();
        let expected = match name {
            "firefox" => "Firefox",
            "webkit" => "Safari",
            _ => "Chrome",
        };
        assert!(label.starts_with(expected), "{name}: label {label}");
        env.b
            .rpc(
                http,
                "connect/grant",
                json!({"invite": invite["invite_id"], "pairing": candidate["pairing_id"], "human": true}),
            )
            .unwrap();
        let paired = browser.call(json!({"cmd": "await_paired"})).unwrap();
        assert_eq!(paired["state"], "active", "{name}: {paired}");

        // DT20: an origin the app did not register gets no connection.
        let other = browser
            .call(json!({"cmd": "open", "origin": stranger}))
            .unwrap();
        assert_eq!(
            browser.call(json!({"cmd": "connect", "app": other["app"]})),
            Err("APP_NOT_REGISTERED".into())
        );

        // DT27, handshake, signature and acknowledgement.
        let app = browser
            .call(json!({"cmd": "open", "origin": origin}))
            .unwrap()["app"]
            .clone();
        let methods = browser.call(json!({"cmd": "connect", "app": app})).unwrap();
        assert!(
            methods["methods"]
                .as_array()
                .unwrap()
                .contains(&json!("signDocument"))
        );
        let created = browser
            .call(json!({"cmd": "sign_document", "app": app, "text": format!("Signed in {name}"), "account": account, "issuer": issuer}))
            .unwrap();
        assert_eq!(created["state"], "awaiting_user", "{name}: {created}");
        // Only Safari asks for a click in the window before creating (§4.1).
        assert_eq!(created["prompted"], name == "webkit", "{name}");
        let op = created["operationId"].as_str().unwrap().to_string();
        let window = browser.call(json!({"cmd": "popup", "app": app})).unwrap();
        let link = format!("dmsg://connect/request?account={account}&request=");
        assert!(
            window["links"]
                .as_array()
                .unwrap()
                .iter()
                .any(|l| l.as_str().unwrap_or("").starts_with(&link)),
            "{name}: {window}"
        );
        let shown = eventually(30, || web_request(&env.b, http, &op));
        assert_eq!(shown["detail"]["origin"], origin.as_str());
        assert_eq!(shown["detail"]["pairing_label"], label);
        assert_eq!(env.b.decide(http, &shown, true).unwrap()["state"], "done");
        let signed = browser
            .call(json!({"cmd": "get", "app": app, "operation_id": op, "wait_ms": 30000, "until": ["signed"]}))
            .unwrap();
        assert_eq!(signed["state"], "signed", "{name}: {signed}");
        let digest = signed["resultDigest"].clone();
        let acked = browser
            .call(json!({"cmd": "ack", "app": app, "operation_id": op, "result_digest": digest}))
            .unwrap();
        assert_eq!(acked["state"], "returned");
        let again = browser
            .call(json!({"cmd": "get", "app": app, "operation_id": op}))
            .unwrap();
        assert_eq!(again["state"], "returned");

        // Closing the bridge window disconnects the page; the operation stays.
        browser
            .call(json!({"cmd": "close_popup", "app": app}))
            .unwrap();
        assert_eq!(
            browser.call(json!({"cmd": "get", "app": app, "operation_id": op})),
            Err("DISCONNECTED".into())
        );
        let _ = browser.call(json!({"cmd": "quit"}));
    }
    env.finish();
}

/// The texts of a channel's messages this device read.
fn texts(messages: &Value) -> Vec<String> {
    messages
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["text"].as_str().unwrap().to_string())
        .collect()
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn desktop_channels() {
    let mut env = Env::launch(
        "channels",
        Options {
            proxies: true,
            ..Options::default()
        },
    );
    let http = env.http.clone();
    let (a, b) = (&env.a, &env.b);
    // Two accounts: A owns the channel, B joins it.
    assert_eq!(a.login(&http, 41).unwrap()["state"], "no_account");
    let owner = a.run(&["account", "create"])["account"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(b.login(&http, 60).unwrap()["state"], "no_account");
    let member = b.run(&["account", "create"])["account"]
        .as_str()
        .unwrap()
        .to_string();

    // A channel, its first epoch, a message and a file before the invitation.
    let created = a.run(&["channel", "create", "Real collaborative channel"]);
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["ledger"]["epoch"], 1);
    assert_eq!(created["ledger"]["status"], "active");
    a.run(&["channel", "send", &id, "owner history before invitation"]);
    let file = env.dir.join("channel-history.bin");
    std::fs::write(&file, vec![73u8; 1024 * 1024 + 17]).unwrap();
    let item = a.run(&["files", "add", file.to_str().unwrap()])[0]["file"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let shared = a.run(&[
        "channel",
        "send",
        &id,
        "historical attachment",
        "--file",
        &item,
    ]);
    let attachment = shared["receipt"]["seq"].as_u64().unwrap();
    let invitation = a.run(&["channel", "invite", &id, &member]);
    assert_eq!(invitation["account"], member.as_str());
    let joined = b.run(&["channel", "accept", &invitation.to_string()]);
    assert_eq!(joined["status"], "rotation_required");
    assert_eq!(joined["members"][&member]["joined_epoch"], 2);

    // The epoch activation commits but its reply is lost: the retry
    // settles it by request ID, and the second rotation opens epoch 2.
    let relay_proxy = env.relay_proxy.as_ref().unwrap();
    relay_proxy.lose_next("POST", "/rotation/activate");
    assert_eq!(a.fail(&["channel", "rotate", &id]), "EXECUTION_UNKNOWN");
    assert_eq!(relay_proxy.pending(), 0);
    let rotated = a.run(&["channel", "rotate", &id]);
    assert_eq!(rotated["ledger"]["epoch"], 2);
    assert_eq!(rotated["readable_epochs"], json!([1, 2]));

    // B reads from epoch 2 only: epoch-1 messages are skipped by the relay.
    assert_eq!(
        texts(&b.run(&["channel", "read", &id])),
        Vec::<String>::new()
    );
    assert_eq!(b.run(&["channel", "show", &id])["message_cursor"], 2);
    let out = env.dir.join("attachment-early.bin");
    assert_eq!(
        b.fail(&[
            "channel",
            "download",
            &id,
            &attachment.to_string(),
            "-o",
            out.to_str().unwrap()
        ]),
        "NOT_FOUND"
    );
    a.run(&["channel", "send", &id, "new shared epoch message"]);
    // The hint socket lets B's daemon read it without being asked.
    let seen = eventually(60, || {
        let messages = b.rpc(&http, "channel/messages", json!({"id": id})).unwrap();
        (!texts(&messages).is_empty()).then_some(messages)
    });
    assert_eq!(texts(&seen), ["new shared epoch message"]);
    b.run(&["channel", "send", &id, "member reply"]);
    assert!(texts(&a.run(&["channel", "read", &id])).contains(&"member reply".to_string()));
    // Locked, B learns only that messages arrived, through its account's
    // hint channel (A4.5). The relay hints a channel's members at most once
    // every five seconds.
    b.hints_connected();
    let mut events = b.app_socket();
    b.run(&["lock"]);
    std::thread::sleep(Duration::from_secs(6));
    a.run(&["channel", "send", &id, "while B is locked"]);
    let hint = await_event(&mut events, "hint", "kind", "channel");
    assert_eq!(hint["data"], json!({"kind": "channel"}));
    assert_eq!(b.login(&http, 60).unwrap()["state"], "unlocked");

    // History: A seals epoch 1 to B's devices; B reads it again.
    let granted = a.run(&["channel", "history", &id, &member, "1", "1"]);
    assert_eq!(granted["members"][&member]["history"], json!([[1, 1]]));
    let backfilled = b.run(&["channel", "read", &id, "--backfill"]);
    assert!(texts(&backfilled).contains(&"owner history before invitation".to_string()));
    let out = env.dir.join("attachment.bin");
    let downloaded = b.run(&[
        "channel",
        "download",
        &id,
        &attachment.to_string(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(downloaded["size"], 1024 * 1024 + 17);
    assert_eq!(std::fs::read(&out).unwrap(), vec![73u8; 1024 * 1024 + 17]);
    // The download cached the attachment's two chunks; nothing needs them,
    // so the next sync collects them, and a download fetches them again.
    let collected = b.run(&["sync"])["collected"].as_u64().unwrap();
    assert!(collected >= 2, "{collected}");
    assert_eq!(b.run(&["sync"])["collected"], 0);
    std::fs::remove_file(&out).unwrap();
    b.run(&[
        "channel",
        "download",
        &id,
        &attachment.to_string(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(std::fs::read(&out).unwrap(), vec![73u8; 1024 * 1024 + 17]);

    // Ownership moves to B and back: proposed, accepted by the target
    // device, committed by the owner.
    let proposal = a.run(&["channel", "transfer", "propose", &id, &member]);
    let accepted = b.run(&["channel", "transfer", "accept", &proposal.to_string()]);
    let moved = a.run(&["channel", "transfer", "commit", &accepted.to_string()]);
    assert_eq!(moved["owner"], member.as_str());
    let back = b.run(&["channel", "transfer", "propose", &id, &owner]);
    let accepted = a.run(&["channel", "transfer", "accept", &back.to_string()]);
    let returned = b.run(&["channel", "transfer", "commit", &accepted.to_string()]);
    assert_eq!(returned["owner"], owner.as_str());

    // B pays for the channel from now on.
    let proposal = a.run(&["channel", "sponsor", "propose", &id, &member]);
    let accepted = b.run(&["channel", "sponsor", "accept", &proposal.to_string()]);
    let billing = a.run(&["channel", "sponsor", "commit", &accepted.to_string()]);
    assert_eq!(billing["billing_account"], member.as_str());

    // B's new device C joins B's account; the synced channel record and the
    // next epoch's envelope let it read and write.
    let (c, c_daemon) = env.desktop("c");
    let (a, b) = (&env.a, &env.b);
    assert_eq!(c.login(&http, 60).unwrap()["state"], "pairing_required");
    let request = c.run(&["device", "pair-request"]);
    b.run(&[
        "device",
        "approve",
        request["token"].as_str().unwrap(),
        "--device",
        request["device_id"].as_str().unwrap(),
    ]);
    assert_eq!(c.run(&["unlock"])["state"], "unlocked");
    // No backups: B's channel record, epoch keys included, reaches C
    // through the account's synced content.
    b.run(&["sync"]);
    c.run(&["sync"]);
    let listed = c.run(&["channel", "list"]);
    assert_eq!(listed[0]["id"], id.as_str());
    let added = a.run(&["channel", "rotate", &id]);
    assert_eq!(added["ledger"]["epoch"], 3);
    // The relay finishes moving the billing in the background; until then
    // it refuses new messages with VERSION_CONFLICT.
    eventually(60, || {
        let output = c
            .command(&[
                "channel",
                "send",
                &id,
                "new approved device",
                "--message-id",
                &"5".repeat(64),
            ])
            .output()
            .unwrap();
        output.status.success().then_some(())
    });

    // C revokes B's first device; the next epoch excludes it.
    let b_device = b.run(&["status"])["device_id"]
        .as_str()
        .unwrap()
        .to_string();
    let revoke = c.run(&["device", "revoke", &b_device]);
    assert_eq!(c.decide(&http, &revoke, true).unwrap()["state"], "done");
    let excluded = a.run(&["channel", "rotate", &id]);
    assert_eq!(excluded["ledger"]["epoch"], 4);
    assert_ne!(
        b.fail(&["channel", "send", &id, "revoked device cannot send"]),
        ""
    );
    c.run(&["channel", "read", &id]);

    // Removing B: the relay refuses it at once, a new epoch isolates it.
    a.run(&["channel", "remove", &id, &member]);
    let removed = a.run(&["channel", "rotate", &id]);
    assert_eq!(removed["ledger"]["epoch"], 5);
    assert!(removed["ledger"]["members"].get(&member).is_none());
    assert_ne!(
        c.fail(&["channel", "send", &id, "removed member cannot send"]),
        ""
    );

    // C's channel state stays on the device across a restart.
    env.relaunch(&c, c_daemon);
    let a = &env.a;
    assert_eq!(c.login(&http, 60).unwrap()["state"], "unlocked");
    let offline = c.rpc(&http, "channel/messages", json!({"id": id})).unwrap();
    assert!(texts(&offline).contains(&"new approved device".to_string()));

    // Roles, leaving, archiving and reopening, on a second channel.
    let second = a.run(&["channel", "create", "Second channel"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let invitation = a.run(&["channel", "invite", &second, &member]);
    c.run(&["channel", "accept", &invitation.to_string()]);
    a.run(&["channel", "rotate", &second]);
    c.run(&["channel", "send", &second, "member message"]);
    let promoted = a.run(&["channel", "role", &second, &member, "admin"]);
    assert_eq!(promoted["members"][&member]["role"], "admin", "{promoted}");
    // Only the owner changes roles, an admin included.
    c.fail(&["channel", "role", &second, &member, "member"]);
    let demoted = a.run(&["channel", "role", &second, &member, "member"]);
    assert_eq!(demoted["members"][&member]["role"], "member");
    c.run(&["channel", "leave", &second]);
    let left = a.run(&["channel", "rotate", &second]);
    assert!(left["ledger"]["members"].get(&member).is_none(), "{left}");
    c.fail(&["channel", "send", &second, "after leaving"]);
    let archived = a.run(&["channel", "archive", &second]);
    assert_eq!(archived["status"], "archived", "{archived}");
    a.fail(&["channel", "send", &second, "while archived"]);
    let reopened = a.run(&["channel", "reactivate", &second]);
    assert_eq!(reopened["ledger"]["status"], "active", "{reopened}");
    a.run(&["channel", "send", &second, "reopened"]);
    assert!(texts(&a.run(&["channel", "read", &second])).contains(&"reopened".to_string()));
    env.finish();
}

/// A PKCS#8 PEM of the Ed25519 seed `[seed; 32]`, as identity files hold it.
fn ed25519_pem(seed: u8) -> String {
    let mut der = hex::decode("302e020100300506032b657004220420").unwrap();
    der.extend([seed; 32]);
    use base64::engine::general_purpose::STANDARD;
    format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
        STANDARD.encode(der)
    )
}

/// A balance as `payer balance` reports it.
fn balance(desktop: &Desktop, ledger: &Principal) -> u128 {
    desktop.run(&["payer", "balance", &ledger.to_text()])["balance"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

/// Send `amount` from the login `seed`'s own account to `to` on a test
/// ledger, as a wallet outside dMsg would; returns the block.
fn ledger_transfer(
    d: &support::Deployment,
    ledger: Principal,
    seed: u8,
    to: icrc_ledger_types::icrc1::account::Account,
    amount: u128,
) -> u64 {
    use icrc_ledger_types::icrc1::transfer::{TransferArg, TransferError};
    let reply =
        d.ic.update_call(
            ledger,
            Principal::from_text(login_principal(seed)).unwrap(),
            "icrc1_transfer",
            candid::encode_one(TransferArg {
                from_subaccount: None,
                to,
                fee: None,
                created_at_time: None,
                memo: None,
                amount: amount.into(),
            })
            .unwrap(),
        )
        .unwrap();
    let block: Result<candid::Nat, TransferError> = candid::decode_one(&reply).unwrap();
    block.unwrap().0.try_into().unwrap()
}

/// Set a test ledger's network fee.
fn set_ledger_fee(d: &support::Deployment, ledger: Principal, fee: u128) {
    d.ic.update_call(ledger, d.peer, "set_fee", candid::encode_one(fee).unwrap())
        .unwrap();
}

/// An approval request's state.
fn approval_state(desktop: &Desktop, request: &Value) -> String {
    desktop.run(&["approvals", "show", request["id"].as_str().unwrap()])["state"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn desktop_commerce() {
    use dmsg_types::integration::{AppCapability, AppRegistration};
    let mut env = Env::launch(
        "commerce",
        Options {
            proxies: true,
            ..Options::default()
        },
    );
    let http = env.http.clone();
    let d = &env.deployment;
    // dMsg's own checkouts carry the origin https://dmsg.net (plan D-19).
    d.register_app(&AppRegistration {
        version: 1,
        environment: dmsg_types::Environment::Local,
        app_id: "dmsg".into(),
        config_version: 1,
        origins: vec!["https://dmsg.net".into()],
        product_ids: vec!["dmsg".into()],
        capabilities: vec![AppCapability::Checkout],
        profiles: vec![],
        authentication_receiver: d.user,
        action_authority: d.user,
        action_schema: None,
        paused: false,
    });
    let (commerce, handle, ledger, ledger_usdt) = (d.commerce, d.handle, d.ledger, d.ledger_usdt);
    // The prices registered at deployment lapsed when PocketIC went live.
    for asset in [ledger, ledger_usdt] {
        let reply =
            d.ic.update_call(
                commerce,
                d.peer,
                "publish_settlement_price",
                candid::encode_args((asset, 1_000_000u128, 30 * dmsg_types::MINUTE)).unwrap(),
            )
            .unwrap();
        candid::decode_one::<dmsg_types::Result<dmsg_types::integration_billing::SettlementAsset>>(
            &reply,
        )
        .unwrap()
        .unwrap();
    }
    let lose_ledger_reply = |ledger: Principal| {
        d.ic.update_call(
            ledger,
            d.peer,
            "lose_next_response",
            candid::encode_args(()).unwrap(),
        )
        .unwrap();
    };
    let ic_proxy = env.ic_proxy.as_ref().unwrap();
    let a = &env.a;
    // The login 42 is the funded wallet, the neuron's hotkey and the
    // legacy owner of `legacy_probe`.
    assert_eq!(a.login(&http, 42).unwrap()["state"], "no_account");
    let account = a.run(&["account", "create"])["account"]
        .as_str()
        .unwrap()
        .to_string();
    let enable = a.run(&["device", "formal-approve", "on"]);
    assert_eq!(a.decide(&http, &enable, true).unwrap()["state"], "done");
    assert_eq!(a.run(&["plan", "status"])["plan"], "Free");

    // Cash on ckUSDC. The merchant's reply to opening the order is lost,
    // then the ledger's reply to the funding transfer: each retry reads the
    // service and resends the same bytes, and the wallet pays once.
    let quote = a.run(&["plan", "quote", "plus", &ledger.to_text()]);
    let id = quote["id"].as_str().unwrap().to_string();
    assert_eq!(quote["terms"]["method"], "Cash");
    assert_eq!(quote["terms"]["asset"], "CkUsdc");
    let before = balance(a, &ledger);
    ic_proxy.lose_next("POST", &format!("/canister/{commerce}/call"));
    let request = a.run(&["plan", "checkout", &id]);
    assert_eq!(request["type"], "checkout");
    assert_eq!(
        request["detail"]["terms"]["amount"],
        quote["terms"]["amount"]
    );
    assert_eq!(
        a.decide(&http, &request, true),
        Err("EXECUTION_UNKNOWN".into())
    );
    assert_eq!(ic_proxy.pending(), 0);
    assert_eq!(a.run(&["plan", "order", &id])["status"], "AwaitingFunding");
    lose_ledger_reply(ledger);
    a.rpc(&http, "approval/reconcile", Value::Null).unwrap();
    assert_eq!(approval_state(a, &request), "unknown");
    a.rpc(&http, "approval/reconcile", Value::Null).unwrap();
    assert_eq!(approval_state(a, &request), "done");
    assert_eq!(a.run(&["plan", "order", &id])["status"], "Applied");
    let charged = before - balance(a, &ledger);
    let terms = &quote["terms"];
    let number = |v: &Value| v.as_str().unwrap().parse::<u128>().unwrap();
    assert_eq!(
        charged,
        number(&terms["amount"]) + number(&terms["fee_reserve"]) + number(&terms["network_fee"])
    );
    assert_eq!(a.run(&["plan", "status"])["plan"], "Plus");
    // An applied order cannot be cancelled into a refund.
    assert_eq!(a.fail(&["plan", "cancel", &id]), "FORBIDDEN");
    assert_eq!(a.run(&["plan", "order", &id])["status"], "Applied");

    // A deposit beyond the order returns to its source. The ledger's fee
    // has risen, so the refund is refused once (BadFee); its recipient, the
    // wallet, revises it to the fee the ledger asks for and sends it.
    let order = dmsg_types::Hash::new(hex::decode(&id).unwrap().try_into().unwrap());
    let payer = Principal::from_text(login_principal(42)).unwrap();
    let view: dmsg_types::Result<dmsg_types::integration_billing::CheckoutView> =
        candid::decode_one(
            &d.ic
                .query_call(
                    commerce,
                    payer,
                    "get_checkout",
                    candid::encode_one(order).unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
    let deposit = view.unwrap().quote.cash.deposit;
    let before = balance(a, &ledger);
    let extra = ledger_transfer(d, ledger, 42, deposit, 5_000);
    let checked: dmsg_types::Result<dmsg_types::integration_billing::CheckoutProgress> =
        candid::decode_one(
            &d.ic
                .update_call(
                    commerce,
                    payer,
                    "check_checkout_funding",
                    candid::encode_args((
                        order,
                        dmsg_types::integration_billing::CashBlock {
                            ledger,
                            block_index: u128::from(extra),
                        },
                    ))
                    .unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
    checked.unwrap();
    let refund = a.run(&["plan", "refund", &id, &extra.to_string()]);
    assert_eq!(refund["status"], "Pending");
    let refund_id = refund["transfer_id"].as_str().unwrap().to_string();
    set_ledger_fee(d, ledger, 12);
    a.run(&["ops", "process-transfer", &refund_id]);
    let transfers = a.run(&["ops", "transfers"]);
    let refused = transfers["transfers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["transfer_id"] == refund_id.as_str())
        .unwrap();
    assert_eq!(refused["status"], "Rejected", "{refused}");
    assert_eq!(refused["expected_fee"], "12");
    assert_eq!(refused["revisable"], true);
    let revised = a.run(&["ops", "revise-transfer", &refund_id, "--fee", "12"]);
    assert_eq!(revised["replaces"], refund_id.as_str());
    let sent = a.run(&[
        "ops",
        "process-transfer",
        revised["transfer_id"].as_str().unwrap(),
    ]);
    assert_eq!(sent["status"], "Succeeded", "{sent}");
    set_ledger_fee(d, ledger, 10);
    let returned = a.run(&["ops", "transfers"])["transfers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["transfer_id"] == revised["transfer_id"])
        .unwrap()["amount"]
        .as_str()
        .unwrap()
        .parse::<u128>()
        .unwrap();
    assert!(returned > 0 && returned <= 5_000 - 12);
    // The wallet paid the deposit and its fee and got the refund back.
    assert_eq!(before - balance(a, &ledger), 5_000 + 10 - returned);

    // The legacy owner claims its name; the registry's reply is lost and
    // the certified owner settles it.
    ic_proxy.lose_next("POST", &format!("/canister/{handle}/call"));
    let request = a.run(&["handle", "claim", "legacy_probe"]);
    assert_eq!(request["type"], "handle");
    assert_eq!(
        a.decide(&http, &request, true),
        Err("EXECUTION_UNKNOWN".into())
    );
    a.rpc(&http, "approval/reconcile", Value::Null).unwrap();
    assert_eq!(approval_state(a, &request), "done");
    assert_eq!(
        a.run(&["handle", "resolve", "legacy_probe"])["account"],
        account.as_str()
    );

    // A new name, paid from the wallet: an allowance the registry charges
    // once.
    let price = a.run(&["handle", "price", "desktop_paid"]);
    let price: u128 = price["price"].as_str().unwrap().parse().unwrap();
    let before = balance(a, &d.name_ledger);
    let request = a.run(&["handle", "register", "desktop_paid"]);
    assert_eq!(request["type"], "handle");
    assert_eq!(a.decide(&http, &request, true).unwrap()["state"], "done");
    assert_eq!(
        a.run(&["handle", "resolve", "desktop_paid"])["account"],
        account.as_str()
    );
    // The price, which pays the registry's transfer fee, and the approval's fee.
    assert_eq!(before - balance(a, &d.name_ledger), price + 10);

    // Another account pays for its plan on ckUSDT with the same funded
    // identity, as a wallet file.
    let pem = env.dir.join("wallet.pem");
    std::fs::write(&pem, ed25519_pem(42)).unwrap();
    let pem = pem.to_str().unwrap().to_string();
    let b = &env.b;
    assert_eq!(b.login(&http, 62).unwrap()["state"], "no_account");
    let second = b.run(&["account", "create"])["account"]
        .as_str()
        .unwrap()
        .to_string();
    let enable = b.run(&["device", "formal-approve", "on"]);
    assert_eq!(b.decide(&http, &enable, true).unwrap()["state"], "done");
    let wallet = b.run(&["payer", "use", &pem]);
    assert_eq!(wallet["source"], "pem");

    // The name moves to the other account: the owner offers it, the target
    // accepts, and the owner commits within the minute the acceptance lasts.
    let offered = a.run(&["handle", "transfer", "legacy_probe", &second]);
    let accept = b.run(&["handle", "accept", offered["token"].as_str().unwrap()]);
    assert_eq!(accept["detail"]["action"], "AcceptTransfer");
    assert_eq!(b.decide(&http, &accept, true).unwrap()["state"], "done");
    let commit = a.run(&[
        "handle",
        "commit",
        offered["operation"]["id"].as_str().unwrap(),
    ]);
    assert_eq!(commit["detail"]["to"], second.as_str());
    assert_eq!(a.decide(&http, &commit, true).unwrap()["state"], "done");
    assert_eq!(
        a.run(&["handle", "resolve", "legacy_probe"])["account"],
        second.as_str()
    );

    // The payer reads its orders and replays the original decision.
    let orders = a.run(&["ops", "orders"]);
    assert!(
        orders["orders"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["order_id"] == id.as_str() && o["status"] == "Applied")
    );
    assert_eq!(a.run(&["ops", "reconcile-order", &id])["status"], "Applied");

    // A web application's checkout through the bridge: quoted on the
    // device before it can be approved, then paid by the wallet once.
    let invite = b.rpc(&http, "connect/invite", Value::Null).unwrap();
    let mut bridge = Headless::start(&env, &env.relay);
    let paired = bridge
        .call(json!({"cmd": "pair", "url": invite["url"], "label": "Chrome · Test"}))
        .unwrap();
    b.rpc(
        &http,
        "connect/grant",
        json!({"invite": invite["invite_id"], "pairing": paired["pairingId"], "human": true}),
    )
    .unwrap();
    bridge
        .call(json!({"cmd": "await_grant", "account": second, "timeout_ms": 30000}))
        .unwrap();
    let connection = bridge
        .call(json!({"cmd": "connect", "origin": "https://dmsg.net", "app_id": "dmsg"}))
        .unwrap()["connection"]
        .clone();
    let account_id: dmsg_types::AccountId = second.parse().unwrap();
    let operation = dmsg_types::Hash::new([0x51; 32]);
    let offer: dmsg_types::Result<dmsg_types::integration::BillingOffer> = candid::decode_one(
        &d.ic
            .update_call(
                commerce,
                d.peer,
                "prepare_account_subscription",
                candid::encode_args((
                    "dmsg",
                    dmsg_protocol::billing::beneficiary(d.user, &account_id),
                    "plus",
                    operation,
                ))
                .unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    let checkout = dmsg_types::integration_billing::CheckoutRequest {
        offer: offer.unwrap(),
        approving_account: account_id,
        product_approval: None,
        method: dmsg_types::integration::SettlementMethod::Cash,
    };
    let operation_id = hex::encode(*operation);
    let created = bridge
        .call(json!({
            "cmd": "create", "connection": connection, "method": "checkout", "operation_id": operation_id,
            "payload": base64::engine::general_purpose::STANDARD.encode(canonical(&checkout)),
        }))
        .unwrap();
    assert_eq!(created["state"], "awaiting_user");
    let shown = eventually(30, || web_request(b, &http, &operation_id));
    assert_eq!(shown["detail"]["method"], "checkout");
    assert_eq!(shown["detail"]["content"]["sku"], "plus");
    assert_eq!(b.decide(&http, &shown, true), Err("INVALID_INPUT".into()));
    let quoted = b
        .rpc(
            &http,
            "approval/quote",
            json!({"id": shown["id"], "selection": ledger_usdt.to_text()}),
        )
        .unwrap();
    assert_ne!(quoted["digest"], shown["digest"]);
    assert_eq!(quoted["detail"]["checkout"]["terms"]["asset"], "CkUsdt");
    assert_eq!(
        b.decide(&http, &shown, true),
        Err("INTEGRITY_FAILED".into())
    );
    assert_eq!(b.decide(&http, &quoted, true).unwrap()["state"], "done");
    let signed = bridge
        .call(json!({"cmd": "get", "connection": connection, "operation_id": operation_id, "wait_ms": 20000, "until": ["signed"]}))
        .unwrap();
    assert_eq!(signed["state"], "signed");
    let order = signed["result"]["orderId"].as_str().unwrap();
    assert!(signed["result"]["checkoutCbor"].is_string());
    assert_eq!(b.run(&["plan", "order", order])["status"], "Applied");
    assert_eq!(b.run(&["plan", "status"])["plan"], "Plus");

    // A PANDA membership backed by the wallet's neuron, asked for by a web
    // application: the device quotes it on the neuron the person picks; the
    // application is accepted and cools down (activation follows the
    // 65-minute cooling period).
    let (c, _) = env.desktop("c");
    let d = &env.deployment;
    assert_eq!(c.login(&http, 61).unwrap()["state"], "no_account");
    let third = c.run(&["account", "create"])["account"]
        .as_str()
        .unwrap()
        .to_string();
    let enable = c.run(&["device", "formal-approve", "on"]);
    assert_eq!(c.decide(&http, &enable, true).unwrap()["state"], "done");
    c.run(&["payer", "use", &pem]);
    let quote = c.run(&[
        "plan",
        "quote",
        "plus",
        "--method",
        "panda",
        &"4d".repeat(32),
    ]);
    assert_eq!(quote["terms"]["method"], "Panda");
    let invite = c.rpc(&http, "connect/invite", Value::Null).unwrap();
    let paired = bridge
        .call(json!({"cmd": "pair", "url": invite["url"], "label": "Chrome · Test"}))
        .unwrap();
    c.rpc(
        &http,
        "connect/grant",
        json!({"invite": invite["invite_id"], "pairing": paired["pairingId"], "human": true}),
    )
    .unwrap();
    bridge
        .call(json!({"cmd": "await_grant", "account": third, "timeout_ms": 30000}))
        .unwrap();
    let account_id: dmsg_types::AccountId = third.parse().unwrap();
    let operation = dmsg_types::Hash::new([0x52; 32]);
    let offer: dmsg_types::Result<dmsg_types::integration::BillingOffer> = candid::decode_one(
        &d.ic
            .update_call(
                commerce,
                d.peer,
                "prepare_account_subscription",
                candid::encode_args((
                    "dmsg",
                    dmsg_protocol::billing::beneficiary(d.user, &account_id),
                    "plus",
                    operation,
                ))
                .unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    let checkout = dmsg_types::integration_billing::CheckoutRequest {
        offer: offer.unwrap(),
        approving_account: account_id,
        product_approval: None,
        method: dmsg_types::integration::SettlementMethod::Panda,
    };
    let operation_id = hex::encode(*operation);
    bridge
        .call(json!({
            "cmd": "create", "connection": connection, "method": "checkout", "operation_id": operation_id,
            "payload": base64::engine::general_purpose::STANDARD.encode(canonical(&checkout)),
        }))
        .unwrap();
    let shown = eventually(30, || web_request(&c, &http, &operation_id));
    let quoted = c
        .rpc(
            &http,
            "approval/quote",
            json!({"id": shown["id"], "selection": "4d".repeat(32)}),
        )
        .unwrap();
    assert_eq!(quoted["detail"]["checkout"]["terms"]["method"], "Panda");
    assert_eq!(c.decide(&http, &quoted, true).unwrap()["state"], "done");
    let signed = bridge
        .call(json!({"cmd": "get", "connection": connection, "operation_id": operation_id, "wait_ms": 20000, "until": ["signed"]}))
        .unwrap();
    assert_eq!(signed["state"], "signed", "{signed}");
    let claim = signed["result"]["orderId"].as_str().unwrap().to_string();
    assert!(signed["result"]["checkoutCbor"].is_string());
    let order = c.run(&["plan", "order", &claim]);
    assert_eq!(order["status"], "CoolingDown");
    assert_eq!(order["eligibility"], "Eligible");
    assert_eq!(order["neuron"], "4d".repeat(32));
    // The neuron's controller reads the commitment it backs.
    let claims = c.run(&["ops", "claims"]);
    assert!(
        claims["claims"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["claim_id"] == claim.as_str() && c["status"] == "CoolingDown")
    );
    assert!(c.run(&["ops", "sweep"])["swept"].is_u64());
    env.finish();
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn desktop_paid_letters() {
    let env = Env::launch(
        "letters",
        Options {
            proxies: true,
            paid_contacts: true,
            ..Options::default()
        },
    );
    let http = env.http.clone();
    let d = &env.deployment;
    let relay_proxy = env.relay_proxy.as_ref().unwrap();
    let (a, b) = (&env.a, &env.b);

    // The recipient charges strangers for a letter. Signing the payment
    // offer needs PaymentOffer on this device, an account change.
    assert_eq!(b.login(&http, 62).unwrap()["state"], "no_account");
    let recipient = b.run(&["account", "create"])["account"]
        .as_str()
        .unwrap()
        .to_string();
    let device = b.run(&["status"])["device_id"]
        .as_str()
        .unwrap()
        .to_string();
    let request = b.run(&[
        "device",
        "capabilities",
        &device,
        "--set",
        "content_sign,vault_unlock,root_manage,payment_offer",
    ]);
    assert_eq!(b.decide(&http, &request, true).unwrap()["state"], "done");
    // The policy commits but its reply is lost: the retry finds the policy
    // in force and does not publish a second version.
    relay_proxy.lose_next("POST", "/policy");
    assert_eq!(
        b.fail(&["inbox", "configure", "paid", "--price", "1000000"]),
        "EXECUTION_UNKNOWN"
    );
    let policy = b.run(&["inbox", "configure", "paid", "--price", "1000000"]);
    assert_eq!(policy["version"], 1);
    assert_eq!(policy["mode"], "paid");

    // The sender writes; the relay quotes. The same order again returns
    // where it stands.
    assert_eq!(a.login(&http, 42).unwrap()["state"], "no_account");
    let sender = a.run(&["account", "create"])["account"]
        .as_str()
        .unwrap()
        .to_string();
    let order = a.run(&["inbox", "send", &recipient, "Private paid contact", "--pay"]);
    assert_eq!(order["state"], "quoted");
    let order_id = order["order_id"].as_str().unwrap().to_string();
    let resumed = a.run(&[
        "inbox",
        "send",
        &recipient,
        "Private paid contact",
        "--pay",
        "--order",
        &order_id,
    ]);
    assert_eq!(resumed["order_id"], order["order_id"]);
    assert_eq!(resumed["ciphertext"], order["ciphertext"]);

    // The ledger's fee is not the one the escrow terms pay: nothing is
    // paid and the request fails (FEE_BLOCKED).
    let before = balance(a, &d.ledger);
    set_ledger_fee(d, d.ledger, 12);
    let request = a.run(&["inbox", "pay", &recipient, &order_id]);
    assert_eq!(a.decide(&http, &request, true), Err("FEE_BLOCKED".into()));
    assert_eq!(approval_state(a, &request), "failed");
    assert_eq!(balance(a, &d.ledger), before);
    set_ledger_fee(d, d.ledger, 10);

    // Paying: the ledger's reply to the escrow funding is lost, then the
    // relay's reply to the admission. Each retry reads back what committed;
    // the wallet pays once and the escrow settles.
    d.ic.update_call(
        d.ledger,
        d.peer,
        "lose_next_response",
        candid::encode_args(()).unwrap(),
    )
    .unwrap();
    relay_proxy.lose_next("POST", "/admit");
    let request = a.run(&["inbox", "pay", &recipient, &order_id]);
    assert_eq!(request["type"], "letter");
    assert_eq!(request["detail"]["amount"], order["quote"]["amount"]);
    assert_eq!(
        a.decide(&http, &request, true),
        Err("EXECUTION_UNKNOWN".into())
    );
    a.rpc(&http, "approval/reconcile", Value::Null).unwrap();
    assert_eq!(approval_state(a, &request), "unknown");
    assert_eq!(relay_proxy.pending(), 0);
    // The ledger's fee rises before the payouts: both are refused (BadFee).
    set_ledger_fee(d, d.ledger, 12);
    a.rpc(&http, "approval/reconcile", Value::Null).unwrap();
    assert_eq!(approval_state(a, &request), "done");
    let status = a.run(&["inbox", "order", &recipient, &order_id]);
    assert_eq!(status["state"], "visible");
    let escrow = a.run(&["inbox", "escrow", &order_id]);
    assert_eq!(escrow["escrow"]["decision"], "SettlementCommitted");
    let amount: u128 = order["quote"]["amount"].as_str().unwrap().parse().unwrap();
    assert_eq!(before - balance(a, &d.ledger), amount + 10);

    // The payer revises each refused payout to the fee the ledger asks for.
    let escrow_id = escrow["escrow"]["escrow_id"].as_str().unwrap().to_string();
    let legs = a.run(&["payment", "transfers", &escrow_id]);
    let refused: Vec<&Value> = legs.as_array().unwrap().iter().collect();
    assert!(!refused.is_empty());
    for leg in refused {
        assert_eq!(leg["status"], "FeeBlocked", "{leg}");
        assert_eq!(leg["expected_fee"], "12");
        let revised = a.run(&["payment", "revise", &escrow_id, &leg["leg_id"].to_string()]);
        assert_eq!(
            (revised["status"].as_str(), revised["fee"].as_str()),
            (Some("Succeeded"), Some("12")),
            "{revised}"
        );
    }
    set_ledger_fee(d, d.ledger, 10);

    // Two more deposits after the decision, the full amount again and a
    // smaller one: the escrow takes neither, and one refund returns them to
    // the wallet with the unused fee reserve.
    let hash = dmsg_types::Hash::new(hex::decode(&escrow_id).unwrap().try_into().unwrap());
    let payer = Principal::from_text(login_principal(42)).unwrap();
    let info: dmsg_types::Result<dmsg_types::payment::EscrowInfo> = candid::decode_one(
        &d.ic
            .query_call(
                d.payment,
                payer,
                "get_escrow",
                candid::encode_one(hash).unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    let to = icrc_ledger_types::icrc1::account::Account {
        owner: d.payment,
        subaccount: Some(*info.unwrap().subaccount),
    };
    for deposit in [amount, 1_000] {
        let block = ledger_transfer(d, d.ledger, 42, to, deposit);
        let checked: dmsg_types::Result<dmsg_types::payment::EscrowInfo> = candid::decode_one(
            &d.ic
                .update_call(
                    d.payment,
                    payer,
                    "check_funding",
                    candid::encode_args((hash, block)).unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            checked.unwrap().decision,
            dmsg_types::payment::FundsDecision::SettlementCommitted
        );
    }
    let refund = a.run(&["inbox", "refund", &order_id]);
    assert!(refund["refund_leg"].is_u64(), "{refund}");
    let settled = &refund["escrow"];
    assert_eq!(settled["liabilities"], "0", "{settled}");
    // Everything the wallet sent, less what came back, is the letter's price
    // and the network fees: the escrow's own and the wallet's three transfers.
    let number = |v: &Value| v.as_str().unwrap().parse::<u128>().unwrap();
    assert_eq!(
        before - balance(a, &d.ledger),
        number(&settled["recipient_net"])
            + number(&settled["service_fee"])
            + number(&settled["network_fees"])
            + 3 * 10
    );
    assert!(a.run(&["inbox", "refund", &order_id])["refund_leg"].is_null());

    // The recipient opens it with the synced inbox key.
    let letters = b.run(&["inbox", "list"]);
    assert_eq!(letters[0]["text"], "Private paid contact");
    assert_eq!(letters[0]["state"], "visible");
    assert_eq!(letters[0]["sender"], sender.as_str());
    // Until the recipient archives it, the sender cannot write again.
    assert_eq!(
        a.fail(&["inbox", "send", &recipient, "Second letter", "--pay"]),
        "VERSION_CONFLICT"
    );
    b.run(&["inbox", "mark", &order_id, "--archive"]);
    let letters = b.run(&["inbox", "list"]);
    assert_eq!(
        (
            letters[0]["read"].as_bool(),
            letters[0]["archived"].as_bool()
        ),
        (Some(true), Some(true))
    );
    let second = a.run(&["inbox", "send", &recipient, "Second letter", "--pay"]);
    assert_eq!(second["state"], "quoted");
    env.finish();
}

/// An MCP client on `dmsg mcp`'s stdio, as Claude Code or another agent
/// runtime drives it.
/// Activation price of the team wallet hub, in e8s of the test ledger.
const WALLET_PRICE: u64 = 1_000_000_000;

/// Mint `amount` to `to` on a test ledger.
fn mint(
    d: &support::Deployment,
    ledger: Principal,
    to: icrc_ledger_types::icrc1::account::Account,
    amount: u128,
) {
    d.ic.update_call(
        ledger,
        d.peer,
        "mint_test",
        candid::encode_args((to, amount)).unwrap(),
    )
    .unwrap();
}

fn ledger_balance(
    d: &support::Deployment,
    ledger: Principal,
    of: icrc_ledger_types::icrc1::account::Account,
) -> u128 {
    let reply =
        d.ic.query_call(
            ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            candid::encode_one(of).unwrap(),
        )
        .unwrap();
    candid::decode_one::<candid::Nat>(&reply)
        .unwrap()
        .to_string()
        .replace('_', "")
        .parse()
        .unwrap()
}

/// This device's capabilities.
fn own_capabilities(desktop: &Desktop) -> (String, Vec<String>) {
    let devices = desktop.run(&["device", "list"]);
    let own = devices
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["current"] == true)
        .unwrap();
    (
        own["device_id"].as_str().unwrap().to_string(),
        own["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_string())
            .collect(),
    )
}

/// Add `added` to this device's capabilities, approved here; returns them.
fn add_capabilities(
    desktop: &Desktop,
    http: &reqwest::blocking::Client,
    added: &[&str],
) -> Vec<String> {
    let (device, mut set) = own_capabilities(desktop);
    for name in added {
        if !set.iter().any(|c| c == name) {
            set.push(name.to_string());
        }
    }
    let request = desktop.run(&["device", "capabilities", &device, "--set", &set.join(",")]);
    assert_eq!(request["type"], "device_capabilities");
    assert_eq!(
        desktop.decide(http, &request, true).unwrap()["state"],
        "done"
    );
    own_capabilities(desktop).1
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn desktop_wallet() {
    use dmsg_types::wallet::HubInit;
    use icrc_ledger_types::icrc1::account::Account;
    let env = Env::launch(
        "wallet",
        Options {
            wallet: true,
            ..Options::default()
        },
    );
    let http = env.http.clone();
    let (a, b, d) = (&env.a, &env.b, &env.deployment);
    let ledger = d.ledger;
    // Two accounts: A (login 42, the funded payer) creates and pays, B
    // (login 43) approves spending.
    assert_eq!(a.login(&http, 42).unwrap()["state"], "no_account");
    let account_a = a.run(&["account", "create"])["account"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(b.login(&http, 43).unwrap()["state"], "no_account");
    let account_b = b.run(&["account", "create"])["account"]
        .as_str()
        .unwrap()
        .to_string();
    // A holds every capability, six in all; B adds only WalletApprove. The
    // relay keeps syncing both accounts on their new security snapshots.
    let every = ["FormalApprove", "PaymentOffer", "WalletApprove"];
    assert_eq!(add_capabilities(a, &http, &every).len(), 6);
    assert!(add_capabilities(b, &http, &["WalletApprove"]).contains(&"WalletApprove".to_string()));
    for (desktop, title) in [(a, "a"), (b, "b")] {
        desktop.run(&["vault", "add", "--title", title]);
        assert_eq!(desktop.run(&["sync"])["sent"], 1);
    }

    // Activation: a quote from the hub, then one approval pays it from the
    // payer, signs the device approval and creates the wallet.
    let quote = a.run(&["wallet", "quote"]);
    assert_eq!(quote["remaining"], 2);
    assert_eq!(quote["plan"], "Free");
    assert_eq!(quote["price_e8s"], WALLET_PRICE.to_string());
    let fee: u128 = quote["ledger_fee_e8s"].as_str().unwrap().parse().unwrap();
    let before = balance(a, &ledger);
    let create = a.run(&[
        "wallet",
        "create",
        quote["quote_id"].as_str().unwrap(),
        "--name",
        "Desk team",
        "--member",
        &format!("{account_b}:approver:b"),
    ]);
    assert_eq!(create["type"], "wallet");
    assert_eq!(create["detail"]["action"], "create");
    assert_eq!(create["detail"]["price_e8s"], WALLET_PRICE.to_string());
    assert_eq!(a.decide(&http, &create, true).unwrap()["state"], "done");
    let done = a.run(&["approvals", "show", create["id"].as_str().unwrap()]);
    assert_eq!(done["detail"]["result"]["status"], "Ready");
    let wallet = done["detail"]["result"]["wallet"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(balance(a, &ledger), before - u128::from(WALLET_PRICE) - fee);
    // The hub indexes the wallet under both accounts; A has one left.
    for desktop in [a, b] {
        let listed = desktop.run(&["wallet", "list"]);
        assert_eq!(listed[0]["wallet"], wallet.as_str());
        assert_eq!(listed[0]["name"], "Desk team");
    }
    assert_eq!(a.run(&["wallet", "quote"])["remaining"], 1);

    // B joins, binding its device's wallet key with an account approval.
    let join = b.run(&["wallet", "join", &wallet]);
    assert_eq!(join["detail"]["action"], "join");
    assert_eq!(b.decide(&http, &join, true).unwrap()["state"], "done");
    let principal_b = b.run(&["wallet", "principal"])["principal"]
        .as_str()
        .unwrap()
        .to_string();

    // The desktop app reads the wallet once the chain-key addresses exist,
    // each checked against the wallet's key roots.
    let shown = eventually(120, || {
        let w = a
            .rpc(&http, "wallet/show", json!({"wallet": wallet}))
            .unwrap();
        (w["addresses"].as_array().unwrap().len() == 4).then_some(w)
    });
    let chains: Vec<&str> = shown["addresses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| {
            assert_eq!(x["verified"], true, "{x}");
            x["chain"].as_str().unwrap()
        })
        .collect();
    assert_eq!(chains, ["Icp", "Bitcoin", "Evm", "Solana"]);
    assert!(
        shown["addresses"][1]["text"]
            .as_str()
            .unwrap()
            .starts_with("bcrt1p")
    );
    assert_eq!(shown["me"]["role"], "Admin");
    let member = |id: &str| {
        shown["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(member(&account_a)["status"], "Active");
    assert_eq!(member(&account_b)["role"], "Approver");
    assert_eq!(member(&account_b)["status"], "Active");
    assert_eq!(member(&account_b)["principals"], json!([principal_b]));

    // An ICP transfer: A proposes, B approves, A executes.
    let owner = Principal::from_text(&wallet).unwrap();
    mint(
        d,
        ledger,
        Account {
            owner,
            subaccount: None,
        },
        500_000_000,
    );
    let to = Account {
        owner: Principal::from_text(login_principal(77)).unwrap(),
        subaccount: None,
    };
    let proposed = a.run(&[
        "wallet",
        "send",
        &wallet,
        "--from",
        "0",
        "--asset",
        "icp",
        "--to",
        &to.to_string(),
        "--amount",
        "100000000",
        "--max-fee",
        "10000",
    ]);
    assert_eq!(proposed["status"], "Open");
    assert_eq!(proposed["requirement"]["approvals"], 1);
    let id = proposed["id"].as_u64().unwrap().to_string();
    let approve = b.run(&["wallet", "approve", &wallet, &id]);
    assert_eq!(approve["detail"]["action"], "decide");
    assert_eq!(approve["detail"]["decision"], "approve");
    assert_eq!(b.decide(&http, &approve, true).unwrap()["state"], "done");
    let ready = a.run(&["wallet", "proposal", &wallet, &id]);
    assert_eq!(ready["status"], "Ready");
    assert_eq!(ready["votes"][0]["member"], account_b.as_str());
    a.run(&["wallet", "execute", &wallet, &id]);
    eventually(60, || {
        (a.run(&["wallet", "proposal", &wallet, &id])["status"] == "Settled").then_some(())
    });
    assert_eq!(ledger_balance(d, ledger, to), 100_000_000);
    let balances = a
        .rpc(&http, "wallet/balances", json!({"wallet": wallet}))
        .unwrap();
    let icp = &balances["addresses"][0];
    assert_eq!(icp["balances"][0]["asset"], "ICP");
    assert_eq!(
        icp["balances"][0]["balance"]["Ok"],
        (400_000_000 - fee).to_string()
    );
    // Chain reads fail on the closed port without failing the whole call.
    assert!(balances["addresses"][1]["balances"][0]["balance"]["Err"].is_string());

    // Both devices verify the whole audit chain against the wallet's head.
    for desktop in [a, b] {
        let audit = desktop.run(&["wallet", "audit", &wallet]);
        assert_eq!(audit["verified"], true);
        let kinds: Vec<&str> = audit["recent"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .map(|e| e["kind"].as_str().unwrap())
            .collect();
        for kind in [
            "Created",
            "Joined",
            "Proposed",
            "Decided(Approve)",
            "Approved",
            "Settled",
        ] {
            assert!(kinds.contains(&kind), "{kind} in {kinds:?}");
        }
    }

    // A creation that fails after payment, here for want of the hub's
    // cycles: `wallet refund` claims the deposit with this device's wallet
    // key and returns it to the payer that paid it. The deposit account is
    // the hub's.
    let hub = quote["deposit"]
        .as_str()
        .unwrap()
        .parse::<Account>()
        .unwrap()
        .owner;
    let config =
        d.ic.query_call(
            hub,
            Principal::anonymous(),
            "get_hub_config",
            candid::encode_args(()).unwrap(),
        )
        .unwrap();
    let mut pricing = candid::decode_one::<HubInit>(&config).unwrap().pricing;
    pricing.cycles_to_wallet = 1_000_000_000_000_000_000;
    let priced =
        d.ic.update_call(
            hub,
            d.peer,
            "admin_set_pricing",
            candid::encode_one(pricing).unwrap(),
        )
        .unwrap();
    candid::decode_one::<dmsg_types::Result<()>>(&priced)
        .unwrap()
        .unwrap();
    let unused = a.run(&["wallet", "quote"]);
    let quote_id = unused["quote_id"].as_str().unwrap();
    let before = balance(a, &ledger);
    let create = a.run(&["wallet", "create", quote_id, "--name", "Unfunded"]);
    assert_eq!(create["detail"]["quote_id"], quote_id);
    assert_eq!(a.decide(&http, &create, true).unwrap()["state"], "done");
    let done = a.run(&["approvals", "show", create["id"].as_str().unwrap()]);
    let status = done["detail"]["result"]["status"].as_str().unwrap();
    assert!(status.starts_with("Failed"), "{status}");
    assert_eq!(balance(a, &ledger), before - u128::from(WALLET_PRICE) - fee);
    // The desktop app lists it; the wallet's own quote, spent, and the
    // unpaid one are not there.
    let deposits = a.rpc(&http, "wallet/deposits", json!({})).unwrap();
    assert_eq!(deposits.as_array().unwrap().len(), 1, "{deposits}");
    assert_eq!(deposits[0]["quote_id"], quote_id);
    assert_eq!(deposits[0]["state"], "failed");
    assert_eq!(deposits[0]["refundable"], true);
    assert_eq!(
        deposits[0]["refund_e8s"],
        (u128::from(WALLET_PRICE) - fee).to_string()
    );
    let refund = a.run(&["wallet", "refund", quote_id]);
    assert_eq!(refund["to"], login_principal(42));
    assert_eq!(balance(a, &ledger), before - 2 * fee);
    let key = a.run(&["wallet", "principal"])["principal"]
        .as_str()
        .unwrap()
        .to_string();
    let key = Account {
        owner: Principal::from_text(key).unwrap(),
        subaccount: None,
    };
    assert_eq!(ledger_balance(d, ledger, key), 0);
    // The refunded quote is spent: the device forgets it.
    let deposits = a.rpc(&http, "wallet/deposits", json!({})).unwrap();
    assert_eq!(deposits, json!([]));
    assert_eq!(a.fail(&["wallet", "refund", quote_id]), "NOT_FOUND");
    env.finish();
}

struct McpClient {
    child: Child,
    stdin: std::process::ChildStdin,
    /// Messages from stdout, read by a thread.
    lines: std::sync::mpsc::Receiver<Value>,
    /// Notifications and replies read while waiting for another reply.
    pending: std::collections::VecDeque<Value>,
    id: u64,
}

impl McpClient {
    /// A session of the `initialize` lifecycle.
    fn start(desktop: &Desktop, credential: &str) -> Self {
        let mut client = Self::spawn(desktop, credential);
        let init = client.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}),
        );
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        writeln!(
            client.stdin,
            "{}",
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
        )
        .unwrap();
        client
    }

    /// `dmsg mcp`, before any request.
    fn spawn(desktop: &Desktop, credential: &str) -> Self {
        let mut child = Command::new(&desktop.bin)
            .arg("--home")
            .arg(&desktop.home)
            .arg("mcp")
            .env("DMSG_AGENT_TOKEN", credential)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let (sender, lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in stdout.lines().map_while(Result::ok) {
                let message = serde_json::from_str(&line).unwrap_or_else(|_| panic!("mcp: {line}"));
                if sender.send(message).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin,
            lines,
            pending: Default::default(),
            id: 0,
        }
    }

    /// Send a request without waiting; returns its ID.
    fn send(&mut self, method: &str, params: Value) -> u64 {
        self.id += 1;
        writeln!(
            self.stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": self.id, "method": method, "params": params})
        )
        .unwrap();
        self.id
    }

    fn notify(&mut self, method: &str, params: Value) {
        writeln!(
            self.stdin,
            "{}",
            json!({"jsonrpc": "2.0", "method": method, "params": params})
        )
        .unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        loop {
            let message = self
                .lines
                .recv_timeout(Duration::from_secs(90))
                .expect("an MCP reply");
            if message["id"] == id && message.get("method").is_none() {
                return message;
            }
            self.pending.push_back(message);
        }
    }

    /// The next notification of the event stream `stream`, skipping
    /// anything else, such as an earlier stream's heartbeats.
    fn next_of(&mut self, stream: u64) -> Value {
        loop {
            let message = self.pending.pop_front().unwrap_or_else(|| {
                self.lines
                    .recv_timeout(Duration::from_secs(60))
                    .expect("an MCP message")
            });
            if message["params"]["_meta"]["io.modelcontextprotocol/subscriptionId"] == stream {
                return message;
            }
        }
    }

    /// The next notification `method` whose params `pick` accepts, skipping
    /// anything else.
    fn notification(&mut self, method: &str, pick: impl Fn(&Value) -> bool) -> Value {
        if let Some(i) = self
            .pending
            .iter()
            .position(|m| m["method"] == method && pick(&m["params"]))
        {
            return self.pending.remove(i).unwrap()["params"].clone();
        }
        loop {
            let message = self
                .lines
                .recv_timeout(Duration::from_secs(60))
                .unwrap_or_else(|_| panic!("a {method} notification"));
            if message["method"] == method && pick(&message["params"]) {
                return message["params"].clone();
            }
            self.pending.push_back(message);
        }
    }

    /// The next event occurrence of `stream`, skipping heartbeats.
    fn next_event(&mut self, stream: u64) -> Value {
        loop {
            let message = self.next_of(stream);
            match message["method"].as_str() {
                Some("notifications/events/event") => return message["params"].clone(),
                Some("notifications/events/heartbeat") => continue,
                _ => panic!("unexpected {message}"),
            }
        }
    }

    /// A tool's structured result, or its error.
    fn tool(&mut self, name: &str, arguments: Value) -> Result<Value, Value> {
        let reply = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        let result = &reply["result"];
        let content: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        match result["isError"] == true {
            true => Err(content),
            false => Ok(content),
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The verdict of Agent Delegation §9 on credential `id` for `audience`,
/// as a relying party reaches it: the principal document from the
/// directory (through the gateway, which checks its certification), the
/// credential and its accepted records from the delegation service.
fn delegation_verdict(env: &Env, account: &str, id: &str, audience: &str) -> Value {
    use agent_protocols::delegation::{
        DelegationCredential, DelegationRecord, PrincipalDocument, verify_delegation_credential,
    };
    let d = &env.deployment;
    let http = &env.http;
    let document: PrincipalDocument = http
        .get(format!("{}{account}?canisterId={}", d.gateway, d.directory))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(document.id, format!("https://id.dmsg.test/{account}"));
    let service = &env.relay;
    let credential: DelegationCredential = http
        .get(format!("{service}/v1/delegations/{id}"))
        .send()
        .unwrap()
        .json()
        .unwrap();
    let events: Value = http
        .get(format!("{service}/v1/delegations/{id}/events"))
        .send()
        .unwrap()
        .json()
        .unwrap();
    let records: Vec<DelegationRecord> = serde_json::from_value(events["result"].clone()).unwrap();
    let verdict = verify_delegation_credential(
        &credential,
        &records,
        &document,
        &document.id,
        audience,
        now_ms() as i64,
    );
    serde_json::to_value(verdict).unwrap()
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn desktop_agents() {
    let mut env = Env::launch(
        "agents",
        Options {
            agents: true,
            proxies: true,
            ..Options::default()
        },
    );
    let http = env.http.clone();
    let host = env.discourse.as_ref().unwrap().origin.clone();
    let a = &env.a;
    assert_eq!(a.login(&http, 61).unwrap()["state"], "no_account");
    let account = a.run(&["account", "create"])["account"]
        .as_str()
        .unwrap()
        .to_string();
    let enable = a.run(&["device", "formal-approve", "on"]);
    assert_eq!(a.decide(&http, &enable, true).unwrap()["state"], "done");

    // The account becomes a principal with one controller key, kept in the
    // vault; the user home publishes both to the directory.
    let request = a.run(&["agent", "principal", "enable"]);
    assert_eq!(request["type"], "principal");
    assert_eq!(a.decide(&http, &request, true).unwrap()["state"], "done");
    let request = a.run(&["agent", "controller", "register", "--name", "desktop"]);
    assert_eq!(request["detail"]["generation"], 1);
    assert_eq!(a.decide(&http, &request, true).unwrap()["state"], "done");
    let principal = a.run(&["agent", "principal", "show"]);
    assert_eq!(principal["published"], true, "{principal}");
    assert_eq!(principal["controllers"][0]["delegation"], "*");
    let vault = a.run(&["vault", "list"]);
    assert!(
        vault
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["item"]["tags"] == json!(["dmsg-controller"]))
    );

    // Two hosted Agent IDs: one the CLI uses, one an MCP client.
    let host_agent = a.run(&["agent", "id", "create", "host"])["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    let claude = a.run(&["agent", "id", "create", "claude"])["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    a.run(&["agent", "id", "use", "host"]);

    // A grant to Claude's Agent ID, signed here; the first reply from the
    // delegation service is lost and the same envelope settles it.
    let proxy = env.relay_proxy.as_ref().unwrap();
    proxy.lose_next("POST", "/v1/delegations");
    let request = a.run(&[
        "agent",
        "grant",
        "claude",
        "--scope",
        "message.draft",
        "--audience",
        "https://dmsg.net",
        "--days",
        "7",
    ]);
    assert_eq!(request["detail"]["kind"]["subject"], claude.as_str());
    assert_eq!(
        a.decide(&http, &request, true),
        Err("EXECUTION_UNKNOWN".into())
    );
    assert_eq!(proxy.pending(), 0);
    a.rpc(&http, "approval/reconcile", Value::Null).unwrap();
    assert_eq!(approval_state(a, &request), "done");
    let job = &a.run(&["agent", "delegations"])[0];
    assert_eq!(job["stage"], "accepted");
    let id = job["kind"]["id"].as_str().unwrap().to_string();
    assert!(id.starts_with(&format!("{account}.")));
    let verdict = delegation_verdict(&env, &account, &id, "https://dmsg.net");
    assert_eq!(verdict["usable"], true, "{verdict}");
    assert_eq!(
        delegation_verdict(&env, &account, &id, "https://other.test")["usable"],
        false
    );
    let credentials = a.run(&["agent", "credentials"]);
    assert_eq!(credentials.as_array().unwrap().len(), 1);
    assert_eq!(credentials[0]["status"], "active");

    // Claude Code's side: `dmsg mcp` with the agent's own credential.
    let credential = a.run(&["agent", "id", "credential", "claude"])["credential"]
        .as_str()
        .unwrap()
        .to_string();
    let mut mcp = McpClient::start(a, &credential);
    let tools = mcp.request("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 65);
    assert!(names.contains(&"agent_protocols_room_join") && names.contains(&"dmsg_vault_list"));
    assert!(names.contains(&"agent_protocols_mail_send"));
    for name in [
        "dmsg_vault_write",
        "dmsg_files_read",
        "dmsg_wallet_propose_transfer",
        "dmsg_inbox_read",
        "dmsg_inbox_contact",
        "dmsg_action_request",
        "dmsg_delegations_list",
    ] {
        assert!(names.contains(&name), "{name}");
    }
    let identity = mcp
        .tool("agent_protocols_identity_current", json!({}))
        .unwrap();
    assert_eq!(identity["agent_id"], claude.as_str());

    // The agent's credential reaches only the MCP methods (DT01, DT02): no
    // approvals, no status, no other Agent ID, no host outside the config.
    let agent_rpc = |method: &str, params: Value| -> Value {
        http.post(format!("http://127.0.0.1:{}/app/v1", a.port()))
            .bearer_auth(&credential)
            .header("content-type", "application/json")
            .body(
                json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string(),
            )
            .send()
            .unwrap()
            .json()
            .unwrap()
    };
    for (method, params) in [
        (
            "approval/respond",
            json!({"id": "x", "digest": "x", "approve": true, "human": true}),
        ),
        (
            "ap/call",
            json!({"agent": "host", "tool": "agent_protocols_identity_current", "input": {}}),
        ),
        (
            "grants/add",
            json!({"agent": "claude", "capabilities": ["vault.list"]}),
        ),
        ("agent/ids", Value::Null),
    ] {
        assert_eq!(
            agent_rpc(method, params)["error"]["data"]["code"],
            "FORBIDDEN",
            "{method}"
        );
    }
    let status = http
        .get(format!("http://127.0.0.1:{}/daemon/status", a.port()))
        .bearer_auth(&credential)
        .send()
        .unwrap();
    assert_eq!(status.status(), 401);
    // The hosts say where Claude may sign, its grants what it may do there:
    // nothing until the owner grants it, and the owner needs no grant.
    assert_eq!(
        mcp.tool("agent_protocols_rooms_list", json!({}))
            .unwrap_err()["code"],
        "FORBIDDEN"
    );
    let unread = mcp.request(
        "resources/read",
        json!({"uri": "agent-protocols://drafts/held"}),
    );
    assert!(unread["error"].is_object(), "{unread}");
    // A grant reaches only configured origins it names: Claude reads every
    // host and writes only at the test's.
    a.run(&[
        "grants",
        "add",
        "claude",
        "discourse.read:https://other.test",
    ]);
    assert_eq!(
        mcp.tool("agent_protocols_rooms_list", json!({}))
            .unwrap_err()["code"],
        "FORBIDDEN"
    );
    a.run(&[
        "grants",
        "add",
        "claude",
        "discourse.read:*",
        &format!("discourse.write:{host}"),
    ]);
    let foreign = mcp
        .tool(
            "agent_protocols_room_create",
            json!({"host": "https://evil.test", "topic": "x", "visibility": "public", "start_time": 1, "end_time": 2}),
        )
        .unwrap_err();
    assert_eq!(foreign["code"], "permission_denied");
    assert!(
        mcp.tool(
            "agent_protocols_host_add",
            json!({"host": "https://evil.test"})
        )
        .is_err()
    );

    // The CLI's agent opens a room; Claude joins it and speaks.
    let start = now_ms();
    let created = a.run(&[
        "ap",
        "room_create",
        &json!({
            "host": host, "topic": "W6", "visibility": "public", "policy": {"open_roles": ["speaker"]},
            "start_time": start, "end_time": start + 86_400_000,
        })
        .to_string(),
    ]);
    let room = created["room"]["room_id"].as_str().unwrap().to_string();
    let joined = mcp
        .tool(
            "agent_protocols_room_join",
            json!({"room_id": room, "host": host, "role": "speaker"}),
        )
        .unwrap();
    assert_eq!(joined["status"], "joined");
    let state = mcp
        .tool("agent_protocols_room_state", json!({"room_id": room}))
        .unwrap();
    assert_eq!(state["room"]["topic"], "W6");

    // Resources show the agent's own connector state; a subscriber hears
    // when one changes (local connector §4.4).
    let room_uri = format!("agent-protocols://rooms/{room}");
    let listed = mcp.request("resources/list", json!({}));
    let uris: Vec<&str> = listed["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    assert!(
        uris.contains(&"agent-protocols://inbox/pending"),
        "{listed}"
    );
    assert!(uris.contains(&room_uri.as_str()), "{listed}");
    let templates = mcp.request("resources/templates/list", json!({}));
    assert_eq!(
        templates["result"]["resourceTemplates"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    let read = mcp.request("resources/read", json!({"uri": room_uri}));
    let shown: Value =
        serde_json::from_str(read["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(shown["room"]["topic"], "W6");
    let missing = mcp.request(
        "resources/read",
        json!({"uri": "agent-protocols://rooms/elsewhere"}),
    );
    assert_eq!(missing["error"]["code"], -32002);
    let timeline_uri = format!("{room_uri}/timeline");
    let subscribed = mcp.request("resources/subscribe", json!({"uri": timeline_uri}));
    assert!(subscribed["result"].is_object(), "{subscribed}");
    let sent = mcp
        .tool(
            "agent_protocols_room_send_message",
            json!({"room_id": room, "content": "hello from claude"}),
        )
        .unwrap();
    assert_eq!(sent["status"], "sent");
    let discourse = env.discourse.as_ref().unwrap();
    assert_eq!(discourse.messages(&room), ["hello from claude"]);
    mcp.notification("notifications/resources/updated", |p| {
        p["uri"] == timeline_uri.as_str()
    });

    // The host's own agent changes the topic, which moves the room head.
    // Claude's next message, based on the head it saw, is held with the
    // change (DT07); sending the draft re-signs it against the new head.
    a.run(&[
        "ap",
        "room_submit_event",
        &json!({"room_id": room, "type": "room.update", "payload": {"topic": "W6 review"}})
            .to_string(),
    ]);
    // Another session of Claude's Agent ID reads the room through the
    // change. This MCP session has not, so what it has seen is still the
    // old head (local connector §4.2).
    a.run(&[
        "ap",
        "--agent",
        "claude",
        "--session",
        "review",
        "room_timeline",
        &json!({"room_id": room, "refresh": true}).to_string(),
    ]);
    let held = mcp
        .tool(
            "agent_protocols_room_send_message",
            json!({"room_id": room, "content": "late reply"}),
        )
        .unwrap();
    assert_eq!(held["status"], "held", "{held}");
    assert!(
        held["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["type"] == "room.update")
    );
    let draft = held["draft"]["id"].as_str().unwrap().to_string();

    // A nonce another service saw, answered in the host's place: within
    // the bound the connector resyncs and signs once more (DT06).
    let room_path = format!("/v1/rooms/{room}");
    let seen = now_ms() + 5_000;
    discourse.answer_next(
        "POST",
        &room_path,
        "nonce_not_greater",
        &[("Max-Seen-Nonce", seen.to_string())],
    );
    let resent = mcp
        .tool(
            "agent_protocols_room_send_message",
            json!({"room_id": room, "content": "after resync", "on_head_mismatch": "send_anyway"}),
        )
        .unwrap();
    assert_eq!(resent["status"], "sent", "{resent}");
    assert_eq!(discourse.nonce(&claude), Some(seen + 1));
    // Beyond the bound the value is refused, not applied.
    discourse.answer_next(
        "POST",
        &room_path,
        "nonce_not_greater",
        &[("Max-Seen-Nonce", (now_ms() + (1 << 33)).to_string())],
    );
    let refused = mcp
        .tool(
            "agent_protocols_room_send_message",
            json!({"room_id": room, "content": "never"}),
        )
        .unwrap_err();
    assert_eq!(refused["code"], "nonce_not_greater");

    // Capabilities over the account: none until the owner grants one, and
    // an agent's request only waits for a person.
    assert_eq!(
        mcp.tool("dmsg_vault_list", json!({})).unwrap_err()["code"],
        "FORBIDDEN"
    );
    a.run(&[
        "grants",
        "add",
        "claude",
        "vault.list",
        "attest.request",
        "vault.read:work",
        "vault.write:work",
        "inbox.read",
        "inbox.contact",
        "agent.delegation.read",
    ]);
    let items = mcp.tool("dmsg_vault_list", json!({})).unwrap();
    assert!(
        items
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i.get("secret").is_none())
    );
    let attest = mcp
        .tool(
            "dmsg_attest_request",
            json!({"text": "Claude drafted this review."}),
        )
        .unwrap();
    assert_eq!(attest["source"], format!("agent:{claude}"));
    assert_eq!(attest["state"], "awaiting_user");
    let status = mcp
        .tool("dmsg_approval_status", json!({"id": attest["id"]}))
        .unwrap();
    assert_eq!(status["state"], "awaiting_user");

    // Writing: only items that keep a granted tag. The secret the agent
    // wrote comes back only with a grant naming the item.
    let written = mcp
        .tool(
            "dmsg_vault_write",
            json!({"type": "login", "title": "CI token", "tags": ["work"], "username": "bot", "secret": "s3cret"}),
        )
        .unwrap();
    let item = written["id"].as_str().unwrap().to_string();
    assert_eq!(written["item"]["secret"], "********");
    assert_eq!(
        mcp.tool(
            "dmsg_vault_write",
            json!({"title": "Home", "tags": ["home"]})
        )
        .unwrap_err()["code"],
        "FORBIDDEN"
    );
    let edited = mcp
        .tool(
            "dmsg_vault_write",
            json!({"id": item, "body": "rotates monthly"}),
        )
        .unwrap();
    assert_eq!(edited["item"]["body"], "rotates monthly");
    assert_eq!(edited["item"]["username"], "bot");
    let owned = a.run(&["vault", "show", &item, "--reveal"]);
    assert_eq!(owned["item"]["secret"], "s3cret");
    assert_eq!(
        mcp.tool("dmsg_vault_read", json!({"id": item, "secret": true}))
            .unwrap_err()["code"],
        "FORBIDDEN"
    );
    a.run(&["grants", "add", "claude", &format!("vault.secret:{item}")]);
    let revealed = mcp
        .tool("dmsg_vault_read", json!({"id": item, "secret": true}))
        .unwrap();
    assert_eq!(revealed["item"]["secret"], "s3cret");
    // dMsg's own keys stay out of reach, even with `*`.
    let controller = vault
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["item"]["tags"] == json!(["dmsg-controller"]))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        a.fail(&["grants", "add", "claude", "vault.read:dmsg-controller"]),
        "INVALID_INPUT"
    );
    a.run(&[
        "grants",
        "add",
        "claude",
        "vault.read:*",
        "vault.secret:*",
        "vault.write:*",
    ]);
    assert_eq!(
        mcp.tool("dmsg_vault_read", json!({"id": controller, "secret": true}))
            .unwrap_err()["code"],
        "FORBIDDEN"
    );
    assert_eq!(
        mcp.tool("dmsg_vault_read", json!({"id": controller}))
            .unwrap_err()["code"],
        "FORBIDDEN"
    );
    let listed = mcp.tool("dmsg_vault_list", json!({})).unwrap();
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["id"] != controller.as_str())
    );
    assert_eq!(
        mcp.tool(
            "dmsg_vault_write",
            json!({"id": controller, "tags": ["work"]})
        )
        .unwrap_err()["code"],
        "FORBIDDEN"
    );

    // Files: a slice of a library file in a folder the agent was granted.
    let path = env.dir.join("for-claude.txt");
    std::fs::write(&path, b"notes for claude").unwrap();
    let folder = a.run(&["files", "mkdir", "claude"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let file = a.run(&["files", "add", path.to_str().unwrap(), "--to", &folder])[0]["file"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        mcp.tool("dmsg_files_read", json!({"id": file}))
            .unwrap_err()["code"],
        "FORBIDDEN"
    );
    a.run(&["grants", "add", "claude", &format!("files.read:{folder}")]);
    let listed = mcp
        .tool("dmsg_files_list", json!({"folder": folder}))
        .unwrap();
    assert_eq!(listed["files"][0]["id"], file.as_str());
    assert_eq!(
        mcp.tool("dmsg_files_list", json!({})).unwrap_err()["code"],
        "FORBIDDEN"
    );
    let slice = mcp
        .tool("dmsg_files_read", json!({"id": file, "offset": 10}))
        .unwrap();
    assert_eq!(slice["size"], 16);
    assert_eq!(slice["eof"], true);
    assert_eq!(
        URL_SAFE_NO_PAD
            .decode(slice["content_base64url"].as_str().unwrap())
            .unwrap(),
        b"claude"
    );

    // Letters: one from another account, read singly and marked read, and
    // a free one written back.
    let b = &env.b;
    assert_eq!(b.login(&http, 62).unwrap()["state"], "no_account");
    let friend = b.run(&["account", "create"])["account"]
        .as_str()
        .unwrap()
        .to_string();
    a.run(&["inbox", "configure", "free"]);
    b.run(&["inbox", "configure", "free"]);
    b.run(&["inbox", "send", &account, "Hello Claude"]);
    let letters = mcp.tool("dmsg_inbox_list", json!({})).unwrap();
    assert!(letters[0].get("text").is_none(), "{letters}");
    let order = letters[0]["order_id"].as_str().unwrap().to_string();
    let letter = mcp
        .tool(
            "dmsg_inbox_read",
            json!({"order_id": order, "mark_read": true}),
        )
        .unwrap();
    assert_eq!(letter["text"], "Hello Claude");
    assert_eq!(letter["read"], true);
    // Locked, B learns only that a letter arrived, through its account's
    // hint channel (A4.5); the desktop notifies without saying from whom.
    b.hints_connected();
    let mut events = b.app_socket();
    b.run(&["lock"]);
    mcp.tool(
        "dmsg_inbox_contact",
        json!({"to": friend, "text": "Hello from Claude"}),
    )
    .unwrap();
    let hint = await_event(&mut events, "hint", "kind", "inbox");
    assert_eq!(hint["data"], json!({"kind": "inbox"}));
    assert_eq!(b.login(&http, 62).unwrap()["state"], "unlocked");
    let received = b.run(&["inbox", "list"]);
    assert!(
        received
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["text"] == "Hello from Claude")
    );

    // This principal's credentials, and a request to sign an application
    // action, which only an action.request grant allows.
    let delegations = mcp.tool("dmsg_delegations_list", json!({})).unwrap();
    assert_eq!(delegations[0]["status"], "active", "{delegations}");
    assert_eq!(
        mcp.tool("dmsg_action_request", json!({"action": "oA"}))
            .unwrap_err()["code"],
        "FORBIDDEN"
    );
    a.run(&["grants", "add", "claude", "action.request"]);
    assert_eq!(
        mcp.tool("dmsg_action_request", json!({"action": "oA"}))
            .unwrap_err()["code"],
        "INVALID_INPUT"
    );

    let grants = a.run(&["grants", "list"]);
    let account_grant = grants
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["capabilities"][0] == "vault.list")
        .unwrap();
    a.run(&["grants", "revoke", account_grant["id"].as_str().unwrap()]);
    assert_eq!(
        mcp.tool("dmsg_vault_list", json!({})).unwrap_err()["code"],
        "FORBIDDEN"
    );

    // The audit record: what Claude did through MCP, and the owner's
    // grants, credential and decisions through the CLI. Reads of the
    // agent's own connector are not entered.
    let entries = |actor: &str| -> Vec<Value> {
        a.run(&["audit", "--agent", actor, "--limit", "500"])["entries"]
            .as_array()
            .unwrap()
            .clone()
    };
    let by_claude = entries("claude");
    let done = |entries: &[Value], action: &str, outcome: &str| {
        entries
            .iter()
            .any(|e| e["action"] == action && e["outcome"] == outcome)
    };
    assert!(
        by_claude
            .iter()
            .all(|e| e["actor"] == claude.as_str() && e["via"] == "mcp")
    );
    assert!(done(&by_claude, "dmsg_vault_list", "FORBIDDEN"));
    assert!(done(&by_claude, "dmsg_vault_write", "ok"));
    assert!(done(&by_claude, "dmsg_files_read", "ok"));
    assert!(done(&by_claude, "agent_protocols_room_send_message", "ok"));
    assert!(
        !by_claude
            .iter()
            .any(|e| e["action"] == "agent_protocols_identity_current")
    );
    let read_item = by_claude
        .iter()
        .find(|e| e["action"] == "dmsg_vault_read" && e["outcome"] == "ok")
        .unwrap();
    assert_eq!(read_item["target"], item.as_str());
    let by_owner = entries("owner");
    for action in [
        "grants/add",
        "grants/revoke",
        "agent/create",
        "agent/credential",
        "approval/approve",
    ] {
        assert!(done(&by_owner, action, "ok"), "{action}");
    }
    let topic = by_owner
        .iter()
        .find(|e| e["action"] == "agent_protocols_room_submit_event")
        .unwrap();
    assert_eq!(
        (topic["via"].as_str(), topic["detail"].as_str()),
        (Some("cli"), Some(host_agent.as_str()))
    );

    // The 2026-07-28 lifecycle: no initialize, and every request names its
    // version; a listen stream hears the resource list change.
    let mut modern = McpClient::spawn(a, &credential);
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "1"},
    });
    let discovered = modern.request("server/discover", json!({"_meta": meta}));
    assert_eq!(discovered["result"]["resultType"], "complete");
    assert!(
        discovered["result"]["supportedVersions"]
            .as_array()
            .unwrap()
            .contains(&json!("2026-07-28"))
    );
    assert_eq!(
        discovered["result"]["capabilities"]["resources"]["subscribe"],
        true
    );
    let listed = modern.request("tools/list", json!({"_meta": meta}));
    assert_eq!(listed["result"]["resultType"], "complete");
    assert_eq!(listed["result"]["cacheScope"], "private");
    assert_eq!(listed["result"]["tools"].as_array().unwrap().len(), 65);
    let unknown = modern.request(
        "tools/list",
        json!({"_meta": {"io.modelcontextprotocol/protocolVersion": "2099-01-01", "io.modelcontextprotocol/clientCapabilities": {}}}),
    );
    assert_eq!(unknown["error"]["code"], -32022);
    let listen = modern.send(
        "subscriptions/listen",
        json!({"_meta": meta, "notifications": {
            "resourcesListChanged": true,
            "toolsListChanged": true,
            "resourceSubscriptions": ["agent-protocols://rooms", "agent-protocols://nowhere"],
        }}),
    );
    let ack = modern.next_of(listen);
    assert_eq!(ack["method"], "notifications/subscriptions/acknowledged");
    assert_eq!(
        ack["params"]["notifications"],
        json!({"resourcesListChanged": true, "resourceSubscriptions": ["agent-protocols://rooms"]})
    );
    let start = now_ms();
    a.run(&[
        "ap",
        "--agent",
        "claude",
        "room_create",
        &json!({
            "host": host, "topic": "Second", "visibility": "public",
            "start_time": start, "end_time": start + 86_400_000,
        })
        .to_string(),
    ]);
    let mut heard = std::collections::BTreeSet::new();
    while heard.len() < 2 {
        let message = modern.next_of(listen);
        heard.insert(message["method"].as_str().unwrap().to_string());
    }
    assert_eq!(
        heard.into_iter().collect::<Vec<_>>(),
        [
            "notifications/resources/list_changed",
            "notifications/resources/updated"
        ]
    );
    drop(modern);
    drop(mcp);

    // A restarted daemon restores the connector: the held draft, the read
    // position and the nonce (DT09). Its keys come back with the login.
    let nonce = discourse.nonce(&claude).unwrap();
    env.restart(0);
    let (a, discourse) = (&env.a, env.discourse.as_ref().unwrap());
    assert_eq!(a.login(&http, 61).unwrap()["state"], "unlocked");
    let mut mcp = McpClient::start(a, &credential);
    let drafts = mcp.tool("agent_protocols_drafts_list", json!({})).unwrap();
    assert_eq!(drafts["drafts"][0]["id"], draft.as_str());
    let committed = mcp
        .tool(
            "agent_protocols_draft_commit",
            json!({"draft_id": draft, "action": "send"}),
        )
        .unwrap();
    assert_eq!(committed["status"], "sent", "{committed}");
    assert!(discourse.nonce(&claude).unwrap() > nonce);
    assert_eq!(
        discourse.messages(&room),
        ["hello from claude", "after resync", "late reply"]
    );
    let timeline = mcp
        .tool(
            "agent_protocols_room_timeline",
            json!({"room_id": room, "refresh": true}),
        )
        .unwrap();
    assert!(timeline["items"].as_array().unwrap().len() >= 4);
    drop(mcp);

    // Revoking the grant: the credential is no longer usable.
    let request = a.run(&["agent", "revoke", &id]);
    assert_eq!(a.decide(&http, &request, true).unwrap()["state"], "done");
    let verdict = delegation_verdict(&env, &account, &id, "https://dmsg.net");
    assert_eq!(verdict["usable"], false, "{verdict}");
    assert_eq!(a.run(&["agent", "credentials"])[0]["status"], "revoked");
    env.finish();
}

/// Entries of the `dmsg` service in the login keychain, deleted when
/// dropped: what a run against the system credential store leaves.
struct Keychain(Vec<String>);

impl Keychain {
    fn has(name: &str) -> bool {
        Command::new("security")
            .args(["find-generic-password", "-s", "dmsg", "-a", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    }
}

impl Drop for Keychain {
    fn drop(&mut self) {
        for name in &self.0 {
            let _ = Command::new("security")
                .args(["delete-generic-password", "-s", "dmsg", "-a", name])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

/// The system credential store (macOS login keychain): the owner
/// credential, fast unlock and a resident Agent ID. Writes and then deletes
/// entries of the service `dmsg`, so it runs only when asked.
#[test]
#[ignore = "needs macOS, DMSG_TEST_SYSTEM_STORAGE=1, the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn desktop_system_credentials() {
    if !cfg!(target_os = "macos") || std::env::var_os("DMSG_TEST_SYSTEM_STORAGE").is_none() {
        eprintln!(
            "not run: set DMSG_TEST_SYSTEM_STORAGE=1 on macOS (writes to the login keychain)"
        );
        return;
    }
    let env = Env::launch(
        "system",
        Options {
            system_storage: true,
            agents: true,
            ..Options::default()
        },
    );
    let (a, http) = (&env.a, &env.http);
    let owner = format!(
        "owner:{}",
        std::fs::canonicalize(&a.home).unwrap().display()
    );
    let mut keychain = Keychain(vec![owner.clone()]);
    // The owner credential is in the keychain, not in the home.
    assert!(!a.home.join("owner.token").exists());
    assert!(Keychain::has(&owner));
    assert_eq!(a.login(http, 61).unwrap()["state"], "no_account");
    let account = a.run(&["account", "create"])["account"]
        .as_str()
        .unwrap()
        .to_string();
    let device = a.run(&["status"])["device_id"]
        .as_str()
        .unwrap()
        .to_string();
    let workspace = format!("dmsg:local:{account}:{device}");
    let fast = format!("{workspace}:fast-unlock");
    keychain.0.push(fast.clone());

    // Fast unlock: a key in the keychain seals a second copy of the data key.
    a.run(&["fast-unlock", "enable"]);
    assert!(Keychain::has(&fast));
    a.run(&["lock"]);
    assert_eq!(a.run(&["unlock", "--fast"])["state"], "unlocked");

    // A resident Agent ID signs from its keychain copy while locked.
    let agent = a.run(&["agent", "id", "create", "bot"])["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    let resident = [
        format!("{workspace}:agent:{agent}"),
        format!("{workspace}:agent-mail:{agent}"),
        format!("{workspace}:agent-grants:{agent}"),
    ];
    keychain.0.extend(resident.iter().cloned());
    a.run(&["agent", "id", "resident", "bot", "on"]);
    assert!(resident.iter().all(|name| Keychain::has(name)));
    a.run(&["grants", "add", "bot", "discourse.read:*"]);
    let credential = a.run(&["agent", "id", "credential", "bot"])["credential"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(a.run(&["lock"])["state"], "locked");
    let host = env.discourse.as_ref().unwrap().origin.clone();
    let start = now_ms();
    let created = ap(
        a,
        "bot",
        "room_create",
        json!({"host": host, "topic": "resident", "visibility": "public", "start_time": start, "end_time": start + 3_600_000}),
    );
    let room = created["room"]["room_id"].as_str().unwrap();
    // The agent itself is checked against the copy of its grants.
    let mut mcp = McpClient::start(a, &credential);
    let state = mcp
        .tool("agent_protocols_room_state", json!({"room_id": room}))
        .unwrap();
    assert_eq!(state["room"]["topic"], "resident");
    assert_eq!(
        mcp.tool(
            "agent_protocols_room_send_message",
            json!({"room_id": room, "content": "no"})
        )
        .unwrap_err()["code"],
        "FORBIDDEN"
    );
    drop(mcp);

    // Turning both off deletes their keys.
    assert_eq!(a.login(http, 61).unwrap()["state"], "unlocked");
    a.run(&["agent", "id", "resident", "bot", "off"]);
    assert!(resident.iter().all(|name| !Keychain::has(name)));
    a.run(&["fast-unlock", "disable"]);
    assert!(!Keychain::has(&fast));
    env.finish();
    // The owner credential stays with the home; the guard deletes it.
    drop(keychain);
    assert!(!Keychain::has(&owner));
}

/// Real Claude Code, with its own Agent ID, through `dmsg mcp`: it joins a
/// room and speaks. Uses the local `claude` login, so it runs only when asked.
#[test]
#[ignore = "needs DMSG_TEST_CLAUDE=1, a logged-in `claude`, the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn desktop_claude_code() {
    if std::env::var_os("DMSG_TEST_CLAUDE").is_none() {
        eprintln!("not run: set DMSG_TEST_CLAUDE=1 (runs the local Claude Code)");
        return;
    }
    let env = Env::launch(
        "claude",
        Options {
            agents: true,
            ..Options::default()
        },
    );
    let (a, http) = (&env.a, &env.http);
    assert_eq!(a.login(http, 61).unwrap()["state"], "no_account");
    a.run(&["account", "create"]);
    a.run(&["agent", "id", "create", "host"]);
    a.run(&["agent", "id", "create", "claude"]);
    let credential = a.run(&["agent", "id", "credential", "claude"])["credential"]
        .as_str()
        .unwrap()
        .to_string();
    let discourse = env.discourse.as_ref().unwrap();
    a.run(&[
        "grants",
        "add",
        "claude",
        &format!("discourse.read:{}", discourse.origin),
        &format!("discourse.write:{}", discourse.origin),
    ]);
    let start = now_ms();
    let created = ap(
        a,
        "host",
        "room_create",
        json!({
            "host": discourse.origin, "topic": "Claude Code", "visibility": "public",
            "policy": {"open_roles": ["speaker"]}, "start_time": start, "end_time": start + 3_600_000,
        }),
    );
    let room = created["room"]["room_id"].as_str().unwrap().to_string();
    let config = json!({"mcpServers": {"dmsg": {
        "command": a.bin, "args": ["--home", a.home, "mcp"],
        "env": {"DMSG_AGENT_TOKEN": credential},
    }}});
    let prompt = format!(
        "Use only the dmsg MCP tools. Join the Agent Discourse room {room} on the host {} as a speaker, then send the message \"hello from Claude Code\" to it. Answer DONE when it is sent.",
        discourse.origin
    );
    let mut claude = Command::new("claude");
    // As from a plain terminal, not as a session nested in another one.
    for (name, _) in std::env::vars().filter(|(name, _)| name.starts_with("CLAUDE")) {
        claude.env_remove(name);
    }
    let output = claude
        .args([
            "-p",
            &prompt,
            "--output-format",
            "json",
            "--strict-mcp-config",
        ])
        .args(["--mcp-config", &config.to_string()])
        // Every tool of `dmsg mcp`; the Agent ID's credential bounds them.
        .args(["--allowedTools", "mcp__dmsg"])
        .current_dir(&env.dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        discourse.messages(&room),
        ["hello from Claude Code"],
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    env.finish();
}

/// `dmsg ap --agent <agent> <tool> <input>` on `desktop`.
fn ap(desktop: &Desktop, agent: &str, tool: &str, input: Value) -> Value {
    desktop.run(&["ap", "--agent", agent, tool, &input.to_string()])
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC, the private relay and the Mail and Knowledge Workers (DMSG_CLOUD_DIR)"]
fn desktop_mail_knowledge() {
    let mut env = Env::launch(
        "mail",
        Options {
            services: true,
            ..Options::default()
        },
    );
    let http = env.http.clone();
    let services = env.services.as_ref().unwrap();
    let (mail, knowledge) = (services.mail.clone(), services.knowledge.clone());
    let a = &env.a;
    assert_eq!(a.login(&http, 61).unwrap()["state"], "no_account");
    a.run(&["account", "create"]);
    let created = |label: &str| {
        a.run(&["agent", "id", "create", label])["agent_id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let (alice, bob) = (created("alice"), created("bob"));

    // Bob opens a mailbox on the relay; Alice, an MCP client, opens one for
    // replies and writes to Bob with its card attached. The relay (the
    // TypeScript SDK) accepts the cards and packets the daemon's Rust SDK
    // signs and seals (DT08).
    let bob_box =
        ap(a, "bob", "mail_mailbox_publish", json!({"routes": [mail]}))["mailbox"].clone();
    assert_eq!(bob_box["status"], "open");
    let credential = a.run(&["agent", "id", "credential", "alice"])["credential"]
        .as_str()
        .unwrap()
        .to_string();
    // Alice may use Agent Mail, not Agent Knowledge.
    a.run(&[
        "grants",
        "add",
        "alice",
        &format!("mail.read:{mail}"),
        &format!("mail.write:{mail}"),
    ]);
    let mut mcp = McpClient::start(a, &credential);
    assert_eq!(
        mcp.tool(
            "agent_protocols_knowledge_search",
            json!({"service": knowledge, "query": "plan"})
        )
        .unwrap_err()["code"],
        "FORBIDDEN"
    );
    let alice_box = mcp
        .tool(
            "agent_protocols_mail_mailbox_publish",
            json!({"routes": [mail]}),
        )
        .unwrap()["mailbox"]
        .clone();
    let sent = mcp
        .tool(
            "agent_protocols_mail_send",
            json!({
                "to": bob_box["address"], "subject": "W7", "text": "Can you review the plan?",
                "reply_mailbox_id": alice_box["mailbox_id"],
            }),
        )
        .unwrap();
    assert_eq!(sent["status"], "delivered", "{sent}");
    let message_id = sent["message"]["message_id"].clone();

    // Bob's sync opens the letter and wakes his inbox; he reads it and
    // replies without an address, through Alice's reply card.
    let synced = ap(a, "bob", "mail_sync", json!({}));
    assert_eq!(synced["errors"], json!([]), "{synced}");
    assert_eq!(
        synced["received"][0]["parts"][0]["text"],
        "Can you review the plan?"
    );
    let inbox = ap(a, "bob", "inbox_next", json!({"kinds": ["mail.received"]}));
    assert_eq!(inbox["items"][0]["actor"], alice.as_str());
    assert_eq!(inbox["items"][0]["message"]["summary"], "W7");
    let read = ap(
        a,
        "bob",
        "mail_list",
        json!({"unread_only": true, "mark_read": true}),
    );
    assert_eq!(read["messages"][0]["reply_address"], alice_box["address"]);
    assert_eq!(read["unread_count"], 0);
    let reply = ap(
        a,
        "bob",
        "mail_send",
        json!({"in_reply_to": message_id, "text": "Looks good."}),
    );
    assert_eq!(reply["status"], "delivered");
    let answer = mcp.tool("agent_protocols_mail_sync", json!({})).unwrap();
    assert_eq!(answer["received"][0]["from"], bob.as_str());
    assert_eq!(answer["received"][0]["in_reply_to"], message_id);
    assert_eq!(
        answer["received"][0]["thread_id"],
        sent["message"]["thread_id"]
    );

    // The other direction of DT08: a letter the TypeScript SDK seals and
    // signs for Bob, which his Rust connector opens.
    let cloud = PathBuf::from(std::env::var_os("DMSG_CLOUD_DIR").unwrap());
    let output = Command::new("node")
        .arg("scripts/send-probe.mjs")
        .current_dir(cloud.join("dmsg-mail"))
        .env("PROBE_MAIL", &mail)
        .env("PROBE_TO", bob_box["address"].as_str().unwrap())
        .env("PROBE_TEXT", "from TypeScript")
        .env("NODE_EXTRA_CA_CERTS", env.dir.join("localhost-cert.pem"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let typescript: Value = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
    assert_eq!(typescript["status"], "delivered", "{typescript}");
    let synced = ap(a, "bob", "mail_sync", json!({}));
    assert_eq!(synced["errors"], json!([]), "{synced}");
    assert_eq!(synced["received"][0]["from"], typescript["agent_id"]);
    assert_eq!(synced["received"][0]["parts"][0]["text"], "from TypeScript");

    // A letter waits on the relay across a restart. The connector state
    // holds no key; Bob's comes back from the sealed store with the login.
    mcp.tool(
        "agent_protocols_mail_send",
        json!({"to": bob_box["address"], "text": "after the restart"}),
    )
    .unwrap();
    drop(mcp);
    env.restart(0);
    let a = &env.a;
    assert_eq!(a.login(&http, 61).unwrap()["state"], "unlocked");
    let synced = ap(a, "bob", "mail_sync", json!({}));
    let texts: Vec<&str> = synced["received"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["parts"][0]["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts, ["after the restart"], "{synced}");

    // Knowledge: Alice publishes; Bob finds the capsule, reads it by ID and
    // checks its status; the service offers no ranked search; Alice's
    // retraction withdraws it.
    let published = ap(
        a,
        "alice",
        "knowledge_submit",
        json!({
            "services": [knowledge], "type": "knowledge.publish",
            "payload": {
                "license": "https://creativecommons.org/licenses/by/4.0/",
                "kind": "observation",
                "title": "Mail keys stay on the device",
                "statement": "A hosted Agent ID's mail keys are sealed on its device, never in the connector state.",
                "language": "en",
                "context": {"scope": "dMsg desktop W7.", "conditions": [], "limitations": []},
                "basis": "The desktop acceptance test.",
                "tags": ["dmsg"]
            }
        }),
    );
    let id = published["id"].as_str().unwrap().to_string();
    assert_eq!(published["results"][0]["service"], knowledge.as_str());
    let page = ap(
        a,
        "bob",
        "knowledge_query",
        json!({"service": knowledge, "actor": alice}),
    );
    assert_eq!(page["items"][0]["id"], id.as_str());
    let got = ap(
        a,
        "bob",
        "knowledge_get",
        json!({"service": knowledge, "ids": [id]}),
    );
    assert_eq!(
        got["items"][0]["payload"]["title"],
        "Mail keys stay on the device"
    );
    assert_eq!(
        a.fail(&[
            "ap",
            "--agent",
            "bob",
            "knowledge_search",
            &json!({"service": knowledge, "text": "mail keys", "mode": "lexical"}).to_string(),
        ]),
        "unsupported_search_mode"
    );
    let status = ap(
        a,
        "bob",
        "knowledge_status",
        json!({"service": knowledge, "id": id}),
    );
    assert_eq!(status["status"], "active", "{status}");
    let retraction = ap(
        a,
        "alice",
        "knowledge_submit",
        json!({"services": [knowledge], "type": "knowledge.retract", "payload": {"target": id, "reason": "Superseded by the W7 record."}}),
    );
    let status = ap(
        a,
        "bob",
        "knowledge_status",
        json!({"service": knowledge, "id": id}),
    );
    assert_eq!(status["status"], "retracted");
    assert_eq!(status["retractions"], json!([retraction["id"]]));
    env.finish();
}

/// An upstream MCP server sending webhooks as the MCP Events draft says:
/// Standard Webhooks `v1` with the subscriber's secret and `v1a` with its
/// own Ed25519 key, which it publishes as a JWKS on its origin.
struct Upstream {
    http: reqwest::blocking::Client,
    /// The relay origin, which receives the webhooks.
    relay: String,
    key: ed25519_dalek::SigningKey,
    origin: String,
}

impl Upstream {
    fn start(relay: &str) -> Self {
        let key = ed25519_dalek::SigningKey::from_bytes(&[7; 32]);
        let jwks = json!({"keys": [{
            "kty": "OKP", "crv": "Ed25519", "kid": "k1",
            "x": URL_SAFE_NO_PAD.encode(key.verifying_key().as_bytes()),
        }]})
        .to_string();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                let _ = reader.read_line(&mut first);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                }
                let (status, body) = match first.contains(" /.well-known/mcp-webhook-jwks.json ") {
                    true => ("200 OK", jwks.clone()),
                    false => ("404 Not Found", "{}".to_string()),
                };
                let mut stream = stream;
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self {
            http: reqwest::blocking::Client::builder()
                .no_proxy()
                .build()
                .unwrap(),
            relay: relay.to_string(),
            key,
            origin,
        }
    }

    /// POST `body` to an endpoint as delivery `id`, signed at `at` (Unix
    /// seconds) with `secret` and, with `v1a`, that key; the status and body.
    fn deliver(
        &self,
        url: &str,
        secret: &str,
        id: &str,
        body: &str,
        at: u64,
        v1a: Option<&ed25519_dalek::SigningKey>,
    ) -> (u16, Value) {
        use base64::engine::general_purpose::STANDARD;
        use ed25519_dalek::Signer;
        use hmac::{KeyInit, Mac};
        let content = format!("{id}.{at}.{body}");
        let key = STANDARD
            .decode(secret.strip_prefix("whsec_").unwrap())
            .unwrap();
        let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(&key).unwrap();
        mac.update(content.as_bytes());
        let mut signature = format!("v1,{}", STANDARD.encode(mac.finalize().into_bytes()));
        if let Some(key) = v1a {
            signature.push_str(&format!(
                " v1a,{}",
                STANDARD.encode(key.sign(content.as_bytes()).to_bytes())
            ));
        }
        let path = url.split_once("/mcp-events/").unwrap().1;
        let response = self
            .http
            .post(format!("{}/mcp-events/{path}", self.relay))
            .header("content-type", "application/json")
            .header("webhook-id", id)
            .header("webhook-timestamp", at.to_string())
            .header("webhook-signature", signature)
            .header("x-mcp-subscription-id", "sub_1")
            .body(body.to_string())
            .send()
            .unwrap();
        let status = response.status().as_u16();
        let body = response.bytes().unwrap();
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    /// An upstream event occurrence `id`, signed with `v1` and `v1a`.
    fn event(&self, url: &str, secret: &str, id: &str, n: u64) -> (u16, Value) {
        let body = json!({
            "eventId": id, "name": "issue.opened", "timestamp": "2026-10-12T00:00:00Z",
            "data": {"number": n}, "cursor": format!("up_{n}"),
        });
        self.deliver(
            url,
            secret,
            id,
            &body.to_string(),
            now_ms() / 1000,
            Some(&self.key),
        )
    }
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn desktop_mcp_events() {
    let mut env = Env::launch(
        "events",
        Options {
            proxies: true,
            ..Options::default()
        },
    );
    let http = env.http.clone();
    let upstream = Upstream::start(&env.relay);
    let a = &env.a;
    assert_eq!(a.login(&http, 71).unwrap()["state"], "no_account");
    a.run(&["account", "create"]);
    for label in ["anda", "other"] {
        a.run(&["agent", "id", "create", label]);
    }
    let credential = |label: &str| {
        a.run(&["agent", "id", "credential", label])["credential"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let (anda, other) = (credential("anda"), credential("other"));

    // Endpoints need events.endpoint, which no agent holds by default (A8).
    let mut stranger = McpClient::start(a, &other);
    let refused = stranger
        .tool("dmsg_events_endpoint_create", json!({}))
        .unwrap_err();
    assert_eq!(refused["code"], "FORBIDDEN", "{refused}");
    drop(stranger);
    a.run(&["grants", "add", "anda", "events.endpoint"]);

    // dmsg mcp serves one event type by poll and push (A1); an endpoint
    // comes with its URL and secret and keeps the agent's context (A2).
    let mut mcp = McpClient::start(a, &anda);
    let types = mcp.request("events/list", json!({}));
    assert_eq!(types["result"]["events"][0]["name"], "dmsg.relay.delivery");
    assert_eq!(
        types["result"]["events"][0]["delivery"],
        json!(["poll", "push"])
    );
    let endpoint = mcp
        .tool(
            "dmsg_events_endpoint_create",
            json!({"context": {"agent": "anda", "session": "trigger-1", "label": "issues"}}),
        )
        .unwrap();
    let id = endpoint["endpoint_id"].as_str().unwrap().to_string();
    let url = endpoint["url"].as_str().unwrap().to_string();
    let secret = endpoint["secret"].as_str().unwrap().to_string();
    assert_eq!(url, format!("{}/mcp-events/{id}", env.relay));
    assert!(secret.starts_with("whsec_"));

    // A creation whose reply is lost is sent again as the same request:
    // one endpoint results, and the agent gets its secret. (Another MCP
    // session, so this one's stream below reads only its own endpoint.)
    let mut side = McpClient::start(a, &anda);
    let relay_proxy = env.relay_proxy.as_ref().unwrap();
    relay_proxy.lose_next("POST", "/mcp-events/endpoints");
    let retried = side
        .tool(
            "dmsg_events_endpoint_create",
            json!({"context": {"label": "retried"}}),
        )
        .unwrap();
    assert_eq!(relay_proxy.pending(), 0);
    assert!(retried["secret"].as_str().unwrap().starts_with("whsec_"));
    let listed = a.run(&["events", "endpoint", "list"]);
    assert_eq!(listed["endpoints"].as_array().unwrap().len(), 2);
    assert_eq!(listed["unknown"], json!([]));
    // When every try is lost the agent hears EXECUTION_UNKNOWN and never
    // the secret; the next fetch pass deletes what the creation made.
    for _ in 0..3 {
        relay_proxy.lose_next("POST", "/mcp-events/endpoints");
    }
    let lost = side
        .tool("dmsg_events_endpoint_create", json!({}))
        .unwrap_err();
    assert_eq!(lost["code"], "EXECUTION_UNKNOWN");
    let listed = a.run(&["events", "endpoint", "list"]);
    assert_eq!(
        (
            listed["settling"].as_u64(),
            listed["unknown"].as_array().map(Vec::len)
        ),
        (Some(1), Some(1)),
        "{listed}"
    );
    eventually(120, || {
        // A waiting reader wakes the fetch pass.
        a.run(&["events", "poll", "--wait", "1000"]);
        let listed = a.run(&["events", "endpoint", "list"]);
        (listed["settling"] == 0 && listed["unknown"] == json!([])).then_some(())
    });
    side.tool(
        "dmsg_events_endpoint_delete",
        json!({"endpoint_id": retried["endpoint_id"]}),
    )
    .unwrap();
    drop(side);

    // The relay answers the upstream's verification itself (DT14).
    let verification = json!({"type": "verification", "challenge": "c-1"}).to_string();
    let answer = upstream.deliver(
        &url,
        &secret,
        "msg_verification_1",
        &verification,
        now_ms() / 1000,
        None,
    );
    assert_eq!(answer, (200, json!({"challenge": "c-1"})));
    mcp.tool(
        "dmsg_events_bind",
        json!({
            "endpoint_id": id,
            "server": format!("{}/mcp", upstream.origin),
            "subscription_id": "sub_1",
            "refresh_before": now_ms() + 1_000,
        }),
    )
    .unwrap();

    // Push: the stream of this MCP session's endpoints starts with
    // `active`, then the verification, the first event, end-to-end signed,
    // and the reminder that refreshBefore is near.
    let stream = mcp.send(
        "events/stream",
        json!({"name": "dmsg.relay.delivery", "arguments": {}, "cursor": null}),
    );
    let active = mcp.next_of(stream);
    assert_eq!(active["method"], "notifications/events/active");
    assert_eq!(upstream.event(&url, &secret, "evt_1", 1).0, 200);
    let mut seen = std::collections::BTreeMap::new();
    while seen.len() < 3 {
        let event = mcp.next_event(stream);
        assert_eq!(event["name"], "dmsg.relay.delivery");
        assert_eq!(event["data"]["endpoint_id"], id.as_str());
        assert_eq!(event["data"]["context"]["session"], "trigger-1");
        seen.insert(event["data"]["kind"].as_str().unwrap().to_string(), event);
    }
    assert_eq!(seen["verification"]["data"]["upstream"]["challenge"], "c-1");
    let first = &seen["event"];
    assert_eq!(first["eventId"], format!("{id}:evt_1"));
    assert_eq!(first["data"]["upstream"]["data"]["number"], 1);
    assert_eq!(first["data"]["upstream"]["cursor"], "up_1");
    assert_eq!(first["data"]["subscription_id"], "sub_1");
    assert_eq!(first["data"]["verified"], json!({"v1": true, "v1a": true}));
    assert_eq!(seen["refresh_due"]["data"]["subscription_id"], "sub_1");

    // Forged, stale and oversized deliveries are refused at the relay; a
    // repeated one is acknowledged again but delivered once (DT12). A
    // delivery whose v1 holds and v1a does not, as only the relay could
    // make, is dropped by the daemon (DT16).
    let event = |id: &str| {
        json!({"eventId": id, "name": "issue.opened", "timestamp": "t", "data": {}}).to_string()
    };
    let now = now_ms() / 1000;
    let forged = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
    assert_eq!(
        upstream
            .deliver(&url, forged, "evt_x", &event("evt_x"), now, None)
            .0,
        401
    );
    assert_eq!(
        upstream
            .deliver(&url, &secret, "evt_x", &event("evt_x"), now - 400, None)
            .0,
        401
    );
    let huge = json!({"eventId": "evt_x", "name": "x", "data": "x".repeat(262_144)}).to_string();
    assert_eq!(
        upstream.deliver(&url, &secret, "evt_x", &huge, now, None).0,
        413
    );
    assert_eq!(
        upstream.event(&url, &secret, "evt_1", 1),
        (200, json!({"ok": true}))
    );
    let impostor = ed25519_dalek::SigningKey::from_bytes(&[8; 32]);
    assert_eq!(
        upstream
            .deliver(
                &url,
                &secret,
                "evt_bad",
                &event("evt_bad"),
                now,
                Some(&impostor)
            )
            .0,
        200
    );
    assert_eq!(upstream.event(&url, &secret, "evt_2", 2).0, 200);
    let second = mcp.next_event(stream);
    assert_eq!(second["eventId"], format!("{id}:evt_2"));
    assert!(a.run(&["events", "status"])["rejected"].as_u64().unwrap() >= 1);

    // A cancelled stream ends; a new one from the last cursor goes on
    // without loss or repetition, with a heartbeat when quiet (DT17).
    mcp.notify("notifications/cancelled", json!({"requestId": stream}));
    assert_eq!(upstream.event(&url, &secret, "evt_3", 3).0, 200);
    let cursor = second["cursor"].clone();
    let stream = mcp.send(
        "events/stream",
        json!({"name": "dmsg.relay.delivery", "arguments": {"endpoint_id": id}, "cursor": cursor}),
    );
    assert_eq!(mcp.next_of(stream)["method"], "notifications/events/active");
    let third = mcp.next_event(stream);
    assert_eq!(third["eventId"], format!("{id}:evt_3"));
    let heartbeat = mcp.next_of(stream);
    assert_eq!(heartbeat["method"], "notifications/events/heartbeat");
    assert_eq!(heartbeat["params"]["cursor"], third["cursor"]);
    mcp.notify("notifications/cancelled", json!({"requestId": stream}));

    // Poll reads the same records; an acknowledgement lets the daemon
    // forget them, and a cursor past them stays valid (A5).
    let polled = mcp.request(
        "events/poll",
        json!({"name": "dmsg.relay.delivery", "arguments": {}, "cursor": null}),
    );
    let events = polled["result"]["events"].as_array().unwrap().clone();
    assert_eq!(events.len(), 5, "{polled}");
    assert_eq!(polled["result"]["hasMore"], false);
    let acked = mcp
        .tool("dmsg_events_ack", json!({"through": second["cursor"]}))
        .unwrap();
    assert_eq!(acked["acked"], 4);
    let rest = mcp.request(
        "events/poll",
        json!({"name": "dmsg.relay.delivery", "arguments": {}, "cursor": null}),
    );
    assert_eq!(
        rest["result"]["events"][0]["eventId"],
        format!("{id}:evt_3")
    );
    drop(mcp);

    // What arrives while the daemon is down waits on the relay. After the
    // restart, a new MCP session names the endpoint, or its context
    // session, explicitly and reads on from its cursor (A3).
    env.restart(0);
    assert_eq!(upstream.event(&url, &secret, "evt_4", 4).0, 200);
    let a = &env.a;
    assert_eq!(a.login(&http, 71).unwrap()["state"], "unlocked");
    let mut mcp = McpClient::start(a, &anda);
    let own = mcp
        .tool("dmsg_events_next", json!({"cursor": third["cursor"]}))
        .unwrap();
    assert_eq!(
        own["events"],
        json!([]),
        "a new session has no endpoint of its own"
    );
    let named = mcp
        .tool(
            "dmsg_events_next",
            json!({"session": "trigger-1", "cursor": third["cursor"], "wait_ms": 20_000}),
        )
        .unwrap();
    assert_eq!(
        named["events"][0]["eventId"],
        format!("{id}:evt_4"),
        "{named}"
    );

    // A rotated secret and the previous one both verify for a day (A6);
    // a deleted endpoint is gone for the upstream and its readers get a
    // final record (DT13).
    let rotated = mcp
        .tool("dmsg_events_endpoint_rotate", json!({"endpoint_id": id}))
        .unwrap();
    let fresh = rotated["secret"].as_str().unwrap();
    assert_ne!(fresh, secret);
    assert_eq!(upstream.event(&url, &secret, "evt_5", 5).0, 200);
    assert_eq!(upstream.event(&url, fresh, "evt_6", 6).0, 200);
    let listed = mcp.tool("dmsg_events_endpoint_list", json!({})).unwrap();
    assert_eq!(listed["endpoints"][0]["state"], "active");
    assert_eq!(listed["endpoints"][0]["relay"]["subscription_id"], "sub_1");
    mcp.tool("dmsg_events_endpoint_delete", json!({"endpoint_id": id}))
        .unwrap();
    assert_eq!(upstream.event(&url, fresh, "evt_7", 7).0, 410);
    let last = mcp
        .tool(
            "dmsg_events_next",
            json!({"endpoint_id": id, "cursor": named["cursor"], "max": 100}),
        )
        .unwrap();
    let kinds: Vec<&str> = last["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["data"]["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds.last(), Some(&"terminated"), "{last}");
    assert_eq!(
        a.run(&["events", "endpoint", "list"])["endpoints"][0]["state"],
        "deleted"
    );
    drop(mcp);
    env.finish();
}

/// A release host on loopback HTTP, laid out as GitHub's: `/latest` names
/// the tag, `/download/<tag>/<asset>` serves the files.
struct Releases {
    origin: String,
    files: std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<String, Vec<u8>>>>,
}

impl Releases {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let files = std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::<
            String,
            Vec<u8>,
        >::new()));
        let served = files.clone();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut line = String::new();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
                        break;
                    }
                }
                let path = line.split(' ').nth(1).unwrap_or_default().to_string();
                let body = served.lock().unwrap().get(&path).cloned();
                let head = match &body {
                    Some(body) => format!(
                        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                        body.len()
                    ),
                    None => {
                        "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                            .into()
                    }
                };
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body.unwrap_or_default());
            }
        });
        Self { origin, files }
    }

    /// Publish `bytes` as this platform's binary of `version`, with
    /// `listed` as its published SHA-256.
    fn publish(&self, version: &str, bytes: &[u8], listed: &str) {
        let asset = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "dmsg-macos-arm64",
            ("linux", "x86_64") => "dmsg-linux-x86_64",
            ("linux", "aarch64") => "dmsg-linux-arm64",
            other => panic!("no release asset for {other:?}"),
        };
        let tag = format!("dmsg-v{version}");
        let mut files = self.files.lock().unwrap();
        files.insert(
            "/latest".into(),
            json!({"tag_name": tag}).to_string().into_bytes(),
        );
        files.insert(format!("/download/{tag}/{asset}"), bytes.to_vec());
        files.insert(
            format!("/download/{tag}/{asset}.sha256"),
            format!("{listed}  {asset}\n").into_bytes(),
        );
    }
}

/// Stops a daemon that `dmsg start` detached, however the test ends.
struct Detached(PathBuf);

impl Drop for Detached {
    fn drop(&mut self) {
        if let Ok(text) = std::fs::read(self.0.join("daemon.json")) {
            let discovery: Value = serde_json::from_slice(&text).unwrap_or_default();
            if let Some(pid) = discovery["pid"].as_u64() {
                let _ = Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .status();
            }
        }
    }
}

#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn desktop_release() {
    let env = Env::launch(
        "release",
        Options {
            proxies: true,
            ..Options::default()
        },
    );
    let (a, http) = (&env.a, &env.http);
    let _detached = Detached(a.home.clone());
    let ic_proxy = env.ic_proxy.as_ref().unwrap();
    let user = env.deployment.user;
    assert_eq!(a.login(http, 43).unwrap()["state"], "no_account");
    a.run(&["account", "create"]);
    let enable = a.run(&["device", "formal-approve", "on"]);
    assert_eq!(a.decide(http, &enable, true).unwrap()["state"], "done");

    // `dmsg install` puts this binary at the shared location once.
    let bin = env.dir.join("bin");
    let bin_text = bin.to_str().unwrap();
    assert_eq!(
        a.run(&["install", "--dir", bin_text])["action"],
        "installed"
    );
    assert_eq!(a.run(&["install", "--dir", bin_text])["action"], "current");
    let installed = Desktop {
        bin: bin.join("dmsg"),
        home: a.home.clone(),
    };
    let version = installed.run(&["status"])["version"]
        .as_str()
        .unwrap()
        .to_string();
    let bytes = std::fs::read(&a.bin).unwrap();
    let releases = Releases::start();
    let with_releases = |args: &[&str]| {
        let mut command = installed.command(args);
        command.env("DMSG_RELEASE_URL", &releases.origin);
        command
    };
    let run = |args: &[&str]| -> Value {
        let output = with_releases(args).output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            output.status.success(),
            "dmsg {args:?}: {stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_str(stdout.lines().last().unwrap()).unwrap()
    };
    let fail = |args: &[&str]| -> String {
        let output = with_releases(args).output().unwrap();
        assert!(!output.status.success(), "dmsg {args:?} succeeded");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let reply: Value = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
        reply["error"]["code"].as_str().unwrap().to_string()
    };
    let instance = |desktop: &Desktop| desktop.run(&["status"])["instance"].clone();

    // A canister release is not a client release; the same version is no update.
    releases.files.lock().unwrap().insert(
        "/latest".into(),
        json!({"tag_name": "v2.15.1"}).to_string().into_bytes(),
    );
    assert_eq!(fail(&["update", "--check"]), "NOT_FOUND");
    releases.publish(&version, &bytes, &"00".repeat(32));
    let check = run(&["update", "--check"]);
    assert_eq!(check["available"], false);
    assert_eq!(check["latest"], version.as_str());
    assert_eq!(run(&["update"])["updated"], false);

    // A binary that does not match its published SHA-256 replaces nothing.
    let first = instance(a);
    assert_eq!(fail(&["update", "--force"]), "INTEGRITY_FAILED");
    assert_eq!(std::fs::read(&installed.bin).unwrap(), bytes);
    assert_eq!(instance(a), first);
    releases.publish(&version, &bytes, &sha256_hex(&bytes));

    // DT10: an approved request whose outcome is unknown holds the update;
    // a request waiting for a person and a new item are kept. Locked, the
    // daemon cannot reconcile it in the background.
    let todo = a.run(&["attest", "text", "dMsg W8: waits for a person"]);
    let item = a.run(&["vault", "add", "--title", "kept"])["id"].clone();
    ic_proxy.lose_next("POST", &format!("/canister/{user}/call"));
    let held = a.run(&["attest", "text", "dMsg W8: held across the update"]);
    assert_eq!(a.decide(http, &held, true), Err("EXECUTION_UNKNOWN".into()));
    a.run(&["lock"]);
    let started = Instant::now();
    assert_eq!(fail(&["update", "--force", "--wait", "3"]), "BUSY");
    assert!(started.elapsed() >= Duration::from_secs(3));
    assert_eq!(instance(a), first);
    assert_eq!(std::fs::read(&installed.bin).unwrap(), bytes);

    // Once it settles, the update stops the daemon, replaces the binary
    // and starts the new one.
    assert_eq!(a.login(http, 43).unwrap()["state"], "unlocked");
    a.rpc(http, "approval/reconcile", Value::Null).unwrap();
    assert_eq!(approval_state(a, &held), "done");
    let updated = run(&["update", "--force"]);
    assert_eq!(updated["updated"], true);
    assert_eq!(updated["restarted"], true);
    assert_eq!(updated["status"]["version"], version.as_str());
    let second = updated["status"]["instance"].clone();
    assert_ne!(second, first);
    let status = a.run(&["status"]);
    assert_eq!(status["state"], "locked");
    assert_eq!(status["approvals_waiting"], 1);
    assert_eq!(a.login(http, 43).unwrap()["state"], "unlocked");
    assert_eq!(approval_state(a, &todo), "awaiting_user");
    assert_eq!(
        a.run(&["vault", "show", item.as_str().unwrap()])["item"]["title"],
        "kept"
    );
    // `dmsg start` rewrote the new daemon's stderr; the run log (plan §13)
    // holds one line per call and per background change, and nothing but
    // event names, method and tool names, codes and numbers.
    assert!(
        std::fs::read_to_string(a.home.join("logs/stderr.log"))
            .unwrap()
            .contains("dmsg daemon listening")
    );
    let lines = a.run(&["logs", "-n", "100000"]);
    let lines: Vec<&str> = lines
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect();
    assert!(
        lines
            .iter()
            .filter(|l| l.contains(" daemon/start "))
            .count()
            >= 2
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains(" call method=account/create by=owner outcome=ok ms="))
    );
    assert!(lines.iter().any(|l| l.contains(" call method=vault/show ")));
    for line in &lines {
        let mut tokens = line.split(' ');
        let at = tokens.next().unwrap();
        assert!(at.len() == 24 && at.ends_with('Z'), "{line}");
        let event = tokens.next().unwrap();
        assert!(
            event
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b"/-".contains(&b)),
            "{line}"
        );
        for field in tokens {
            let (key, value) = field.split_once('=').unwrap();
            assert!(key.bytes().all(|b| b.is_ascii_lowercase()), "{line}");
            assert!(
                !value.is_empty()
                    && value.len() <= 40
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_./:-".contains(&b)),
                "{line}"
            );
        }
        assert!(!line.contains("kept"), "{line}");
    }
    let doctor = a.run(&["doctor"]);
    assert_eq!(doctor["ok"], true, "{doctor}");
    let check = |name: &str| {
        doctor["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap_or_else(|| panic!("{name}: {doctor}"))["status"]
            .clone()
    };
    for name in [
        "config",
        "permissions",
        "credentials",
        "daemon",
        "relay",
        "clock",
        "registry",
    ] {
        assert_eq!(check(name), "ok", "{name}: {doctor}");
    }

    // `restart --interrupt` does not wait; the unsettled request resumes.
    ic_proxy.lose_next("POST", &format!("/canister/{user}/call"));
    let cut = a.run(&["attest", "text", "dMsg W8: resumed after a restart"]);
    assert_eq!(a.decide(http, &cut, true), Err("EXECUTION_UNKNOWN".into()));
    a.run(&["lock"]);
    assert_eq!(installed.fail(&["restart", "--wait", "0"]), "BUSY");
    let third = installed.run(&["restart", "--interrupt"])["instance"].clone();
    assert_ne!(third, second);
    assert_eq!(a.login(http, 43).unwrap()["state"], "unlocked");
    a.rpc(http, "approval/reconcile", Value::Null).unwrap();
    assert_eq!(approval_state(a, &cut), "done");

    // `stop` returns once the daemon is gone.
    assert_eq!(installed.run(&["stop"])["stopped"], true);
    let output = installed.command(&["status"]).output().unwrap();
    assert_eq!(output.status.code(), Some(3));
    env.finish();
}

/// Old channel and name-identity canisters of the current release in the
/// deployment's PocketIC, with one channel (old manager `52`, old member
/// `53`, three encrypted messages) frozen under cutover `08…08`.
struct Legacy {
    channel: Principal,
    identity: Principal,
    channel_id: u32,
    /// A channel the owner manages with a second old identity.
    shared_id: u32,
    /// The historical channel KEK and DEK as COSE_Keys, and the frozen
    /// DEK wrapper.
    kek: Vec<u8>,
    dek: Vec<u8>,
    wrapped: Vec<u8>,
}

/// The self-authenticating Principal of the Ed25519 seed `[seed; 32]`, as
/// the old site's identities in the driver.
fn old_identity(seed: u8) -> Principal {
    let mut der = vec![
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    der.extend(
        ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
            .verifying_key()
            .to_bytes(),
    );
    Principal::self_authenticating(der)
}

impl Legacy {
    fn install(deployment: &support::Deployment) -> Self {
        use ic_message_types::channel::{
            AddMessageInput, AddMessageOutput, ChannelECDHInput, ChannelInfo, CreateChannelInput,
            Message, UpdateChannelMemberInput,
        };
        use ic_message_types::migration::FreezeStatus;
        #[derive(candid::CandidType)]
        enum ChannelArgs {
            Init {
                managers: Vec<Principal>,
                name: String,
            },
        }
        #[derive(candid::CandidType)]
        enum IdentityArgs {
            Init {
                session_expires_in_ms: u64,
                name: String,
            },
        }
        let ic = &deployment.ic;
        let call = |canister: Principal, caller: Principal, method: &str, args: Vec<u8>| {
            ic.update_call(canister, caller, method, args).unwrap()
        };
        let wasm = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/wasm32-unknown-unknown/release");
        let gov = Principal::from_slice(&[90]);
        let subnet = ic.topology().get_app_subnets()[0];
        let channel = ic.create_canister_on_subnet(Some(gov), None, subnet);
        let identity = ic.create_canister_on_subnet(Some(gov), None, subnet);
        for (id, name, args) in [
            (
                channel,
                "ic_message_channel",
                candid::encode_args((Some(ChannelArgs::Init {
                    managers: vec![gov],
                    name: "legacy channels".into(),
                }),))
                .unwrap(),
            ),
            (
                identity,
                "ic_name_identity",
                candid::encode_args((Some(IdentityArgs::Init {
                    session_expires_in_ms: 3_600_000,
                    name: "legacy names".into(),
                }),))
                .unwrap(),
            ),
        ] {
            ic.add_cycles(id, 10_000_000_000_000_000);
            let module = std::fs::read(wasm.join(format!("{name}.wasm"))).unwrap_or_else(|_| {
                panic!(
                    "build {name}: cargo build --release --target wasm32-unknown-unknown -p {name}"
                )
            });
            ic.install_canister(id, module, args, Some(gov));
        }
        let (owner, member) = (old_identity(52), old_identity(53));
        let kek = ic_cose_types::cose::cose_aes256_key([8; 32], vec![])
            .to_vec()
            .unwrap();
        let dek = ic_cose_types::cose::cose_aes256_key([9; 32], vec![])
            .to_vec()
            .unwrap();
        let wrapped =
            ic_cose_types::cose::encrypt0::cose_encrypt0(&dek, &[8; 32], &[], &[1; 12], None)
                .unwrap();
        let none = || ChannelECDHInput {
            ecdh_pub: None,
            ecdh_remote: None,
        };
        let created: Result<ChannelInfo, String> = candid::decode_one(&call(
            channel,
            gov,
            "admin_create_channel",
            candid::encode_args((CreateChannelInput {
                name: "Frozen channel".into(),
                image: String::new(),
                description: String::new(),
                managers: [(owner, none())].into(),
                dek: wrapped.clone().into(),
                created_by: owner,
                paid: 0,
            },))
            .unwrap(),
        ))
        .unwrap();
        let channel_id = created.unwrap().id;
        let created: Result<ChannelInfo, String> = candid::decode_one(&call(
            channel,
            gov,
            "admin_create_channel",
            candid::encode_args((CreateChannelInput {
                name: "Frozen channel with two managers".into(),
                image: String::new(),
                description: String::new(),
                managers: [(owner, none()), (old_identity(54), none())].into(),
                dek: wrapped.clone().into(),
                created_by: owner,
                paid: 0,
            },))
            .unwrap(),
        ))
        .unwrap();
        let shared_id = created.unwrap().id;
        let added: Result<(u64, Option<Message>), String> = candid::decode_one(&call(
            channel,
            owner,
            "update_member",
            candid::encode_args((UpdateChannelMemberInput {
                id: channel_id,
                member,
                ecdh: none(),
            },))
            .unwrap(),
        ))
        .unwrap();
        added.unwrap();
        for n in 0..3u32 {
            let mut nonce = [2; 12];
            nonce[8..].copy_from_slice(&n.to_be_bytes());
            let payload = ic_cose_types::cose::encrypt0::cose_encrypt0(
                &cbor2::to_vec(&format!("Legacy shared history {n}")).unwrap(),
                &[9; 32],
                &[],
                &nonce,
                None,
            )
            .unwrap();
            let sent: Result<AddMessageOutput, String> = candid::decode_one(&call(
                channel,
                owner,
                "add_message",
                candid::encode_args((AddMessageInput {
                    channel: channel_id,
                    payload: payload.into(),
                    reply_to: None,
                },))
                .unwrap(),
            ))
            .unwrap();
            sent.unwrap();
        }
        let cutover = serde_bytes::ByteArray::new([8u8; 32]);
        for id in [channel, identity] {
            let status: FreezeStatus = candid::decode_one(
                &ic.query_call(id, gov, "legacy_status", candid::encode_args(()).unwrap())
                    .unwrap(),
            )
            .unwrap();
            if status.baseline_needed {
                let done: Result<(), String> = candid::decode_one(&call(
                    id,
                    gov,
                    "admin_legacy_acknowledge_baseline",
                    candid::encode_args((serde_bytes::ByteArray::new([9u8; 32]),)).unwrap(),
                ))
                .unwrap();
                done.unwrap();
            }
            for method in ["admin_legacy_drain", "admin_legacy_seal"] {
                let frozen: Result<FreezeStatus, String> = candid::decode_one(&call(
                    id,
                    gov,
                    method,
                    candid::encode_args((cutover,)).unwrap(),
                ))
                .unwrap();
                frozen.unwrap_or_else(|e| panic!("{method}: {e}"));
            }
        }
        Self {
            channel,
            identity,
            channel_id,
            shared_id,
            kek,
            dek,
            wrapped,
        }
    }

    /// One old-site action of `src/dmsg_legacy/tests/e2e-driver.test.ts`,
    /// as the old identity `seed`.
    fn old_site(&self, env: &Env, seed: u8, action: &str, extra: Value) -> Value {
        let n = std::fs::read_dir(&env.dir).unwrap().count();
        let (input, out) = (
            env.dir.join(format!("old-site-{n}.json")),
            env.dir.join(format!("old-site-{n}-out.json")),
        );
        let mut value = json!({
            "gateway": env.deployment.gateway,
            "root_key_hex": env.deployment.root_key_hex,
            "channel": self.channel.to_text(),
            "identity": self.identity.to_text(),
            "channel_id": self.channel_id,
            "cutover_hex": "08".repeat(32),
            "wrapped_hex": hex::encode(&self.wrapped),
            "kek_hex": hex::encode(&self.kek),
            "dek_hex": hex::encode(&self.dek),
            "seed": seed,
            "action": action,
            "out": out,
        });
        for (key, item) in extra.as_object().unwrap() {
            value[key] = item.clone();
        }
        std::fs::write(&input, value.to_string()).unwrap();
        let status = Command::new("pnpm")
            .args(["exec", "vitest", "run", "tests/e2e-driver.test.ts"])
            .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src/dmsg_legacy"))
            .env("DMSG_LEGACY_DRIVER", &input)
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "old site {action}");
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap()
    }
}

fn strs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

/// A JSON document written for a command.
fn document(env: &Env, name: &str, value: &Value) -> String {
    let path = env.dir.join(name);
    std::fs::write(&path, value.to_string()).unwrap();
    path.to_str().unwrap().to_string()
}

#[test]
#[ignore = "needs the dmsg binary, the legacy canister Wasm, PocketIC, the private relay (DMSG_CLOUD_DIR) and Node"]
fn desktop_legacy() {
    let mut env = Env::launch(
        "legacy",
        Options {
            legacy: true,
            ..Options::default()
        },
    );
    let http = env.http.clone();
    let legacy = env.legacy.take().unwrap();
    // C becomes a second device of X at the end.
    let (c, _) = env.desktop("c");
    let (a, b) = (&env.a, &env.b);
    let (owner, member) = (old_identity(52).to_text(), old_identity(53).to_text());
    assert_eq!(a.login(&http, 41).unwrap()["state"], "no_account");
    let x = a.run(&["account", "create"])["account"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(b.login(&http, 60).unwrap()["state"], "no_account");
    let y = b.run(&["account", "create"])["account"]
        .as_str()
        .unwrap()
        .to_string();

    // Personal archive: the offer opens the old site with the device as
    // target; the old site seals the owner's archive to it.
    let offer = a.run(&["legacy", "pair", "--origin", "https://panda.fans"]);
    let link = offer["link"].as_str().unwrap();
    assert!(link.starts_with("https://panda.fans/legacy#pair="));
    assert_eq!(offer["target"], a.run(&["status"])["device_id"]);
    let file = env.dir.join("owner.dmsg-migration");
    let sealed = legacy.old_site(&env, 52, "archive", json!({"link": link, "file": file}));
    assert_eq!(sealed["principal"], owner.as_str());
    let (nonce, fingerprint) = (
        offer["nonce"].as_str().unwrap(),
        offer["fingerprint"].as_str().unwrap(),
    );
    let file = file.to_str().unwrap();
    let import = |principal: &str| {
        vec![
            "legacy",
            "import",
            file,
            "--nonce",
            nonce,
            "--fingerprint",
            fingerprint,
            "--principal",
            principal,
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>()
    };
    assert_eq!(a.fail(&strs(&import(&member))), "FORBIDDEN");
    let report = a.run(&strs(&import(&owner)));
    let archive = report["id"].as_str().unwrap().to_string();
    let texts = |report: &Value| {
        report["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["text"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    let expected: Vec<String> = (0..3)
        .map(|n| format!("Legacy shared history {n}"))
        .collect();
    assert!(
        expected.iter().all(|t| texts(&report).contains(t)),
        "{report}"
    );
    assert_eq!(a.run(&strs(&import(&owner)))["id"], archive.as_str());
    assert_eq!(a.run(&["legacy", "list"]).as_array().unwrap().len(), 1);

    // After the freeze the old site exports the final history: it matches.
    let frozen = env.dir.join("frozen.cbor");
    legacy.old_site(&env, 52, "frozen", json!({"file": frozen}));
    let compared = a.run(&["legacy", "frozen", &archive, frozen.to_str().unwrap()]);
    assert_eq!(compared["matched"], true, "{compared}");

    // Shared successor: the frozen source, a proposal by X, the old
    // manager's approval on the old site, then commit and activation.
    let source = legacy.old_site(&env, 52, "source", json!({}));
    let source_file = document(&env, "source.json", &source);
    let checked = a.run(&["legacy", "shared", "source", &source_file]);
    assert_eq!(checked["managers"], json!([owner]));
    assert!(
        checked["members"]
            .as_array()
            .unwrap()
            .contains(&json!(member))
    );
    let draft = a.run(&["legacy", "shared", "prepare", &source_file]);
    let draft_file = document(&env, "draft.json", &draft);
    let request = a.run(&[
        "legacy",
        "shared",
        "request",
        &draft_file,
        "--manager",
        &owner,
    ]);
    assert!(
        request["links"][0]
            .as_str()
            .unwrap()
            .starts_with("https://dmsg.net/legacy#approve=")
    );
    let approval = legacy.old_site(&env, 52, "approve", json!({"request": request["request"]}));
    let approval_file = document(&env, "approval.json", &approval);
    let view = a.run(&[
        "legacy",
        "shared",
        "propose",
        &draft_file,
        "--manager",
        &owner,
        "--approval",
        &approval_file,
    ]);
    assert_eq!(view["stage"], "pending");
    let key = view["key"].as_str().unwrap().to_string();
    // Another identity's approval of the same intent is refused.
    assert_eq!(
        a.fail(&[
            "legacy",
            "shared",
            "consent",
            &draft_file,
            "--manager",
            &member,
            "--approval",
            &approval_file,
        ]),
        "FORBIDDEN"
    );
    assert_eq!(
        a.run(&["legacy", "shared", "commit", &key])["stage"],
        "committed"
    );
    let active = a.run(&["legacy", "shared", "activate", &key]);
    assert_eq!(active["stage"], "active");
    let channel = active["proposal"]["channel_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(a.run(&["channel", "show", &channel])["ledger"]["epoch"], 1);

    // The old member claims account Y and B's device on the old site, with
    // the record X handed over; B is not a reader of the directory yet.
    assert_eq!(b.fail(&["legacy", "shared", "view", &key]), "FORBIDDEN");
    let view_file = document(&env, "view.json", &active);
    let claim_request = b.run(&[
        "legacy",
        "shared",
        "claim-request",
        &view_file,
        "--member",
        &member,
    ]);
    let claim_file = document(&env, "claim.json", &claim_request);
    let approval = legacy.old_site(
        &env,
        53,
        "approve",
        json!({"request": claim_request["request"]}),
    );
    let approval_file = document(&env, "claim-approval.json", &approval);
    let claimed = b.run(&[
        "legacy",
        "shared",
        "claim",
        &view_file,
        "--request",
        &claim_file,
        "--approval",
        &approval_file,
    ]);
    assert_eq!(claimed["claims"][&member]["claim"]["account"], y.as_str());

    // X invites Y; Y joins the next epoch and says so.
    let invitation = a.run(&["channel", "invite", &channel, &y]);
    b.run(&["channel", "accept", &invitation.to_string()]);
    assert_eq!(
        a.run(&["channel", "rotate", &channel])["ledger"]["epoch"],
        2
    );
    let joined = b.run(&["legacy", "shared", "joined", &key]);
    assert_eq!(joined["claims"][&member]["joined"], true);

    // X delivers the channel's history from its archive to B's device.
    let granted = a.run(&[
        "legacy",
        "shared",
        "grant",
        &key,
        "--archive",
        &archive,
        "--member",
        &member,
    ]);
    let grant = granted["grants"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    let received = b.run(&["legacy", "shared", "receive", &key, "--grant", &grant]);
    assert!(
        expected.iter().all(|t| texts(&received).contains(t)),
        "{received}"
    );
    assert_eq!(received["principal"], owner.as_str());
    assert_eq!(b.run(&["legacy", "list"]).as_array().unwrap().len(), 1);

    // A second device of X gets the archive through sync and reads it offline.
    assert_eq!(c.login(&http, 41).unwrap()["state"], "pairing_required");
    let pairing = c.run(&["device", "pair-request"]);
    a.run(&[
        "device",
        "approve",
        pairing["token"].as_str().unwrap(),
        "--device",
        pairing["device_id"].as_str().unwrap(),
    ]);
    assert_eq!(c.run(&["unlock"])["state"], "unlocked");
    a.run(&["sync"]);
    c.run(&["sync"]);
    assert_eq!(c.run(&["legacy", "list"])[0]["id"], archive.as_str());
    let synced = c.run(&["legacy", "report", &archive]);
    assert!(
        expected.iter().all(|t| texts(&synced).contains(t)),
        "{synced}"
    );
    assert_eq!(synced["frozen"]["matched"], true);

    // A channel with two old managers: its successor commits only once
    // both approved the same proposal.
    let (a, second) = (&env.a, old_identity(54).to_text());
    let source = legacy.old_site(&env, 52, "source", json!({"channel_id": legacy.shared_id}));
    let source_file = document(&env, "source-two.json", &source);
    let checked = a.run(&["legacy", "shared", "source", &source_file]);
    let mut managers: Vec<String> = serde_json::from_value(checked["managers"].clone()).unwrap();
    managers.sort();
    let mut both = vec![owner.clone(), second.clone()];
    both.sort();
    assert_eq!(managers, both);
    let draft = a.run(&["legacy", "shared", "prepare", &source_file]);
    let draft_file = document(&env, "draft-two.json", &draft);
    let mut approvals = vec![];
    for (seed, manager) in [(52, &owner), (54, &second)] {
        let request = a.run(&[
            "legacy",
            "shared",
            "request",
            &draft_file,
            "--manager",
            manager,
        ]);
        let approval = legacy.old_site(
            &env,
            seed,
            "approve",
            json!({"request": request["request"]}),
        );
        approvals.push(document(
            &env,
            &format!("approval-two-{seed}.json"),
            &approval,
        ));
    }
    let view = a.run(&[
        "legacy",
        "shared",
        "propose",
        &draft_file,
        "--manager",
        &owner,
        "--approval",
        &approvals[0],
    ]);
    assert_eq!(view["stage"], "pending");
    let key = view["key"].as_str().unwrap().to_string();
    assert_eq!(
        a.fail(&["legacy", "shared", "commit", &key]),
        "RECOVERY_INCOMPLETE"
    );
    let consented = a.run(&[
        "legacy",
        "shared",
        "consent",
        &draft_file,
        "--manager",
        &second,
        "--approval",
        &approvals[1],
    ]);
    assert_eq!(consented["votes"].as_object().unwrap().len(), 2);
    assert_eq!(
        a.run(&["legacy", "shared", "commit", &key])["stage"],
        "committed"
    );
    let _ = x;
    env.finish();
}
