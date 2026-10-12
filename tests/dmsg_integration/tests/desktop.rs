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
mod adp;
mod support;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use candid::Principal;
use dmsg_protocol::{canonical, login::decode_request};
use dmsg_types::login::{LoginCallback, LoginResult};
use ic_agent::identity::{BasicIdentity, Delegation};
use ic_agent::Identity;
use serde_json::{json, Value};
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
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(value) = serde_json::from_slice(&bytes) {
                return value;
            }
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
        std::fs::read_to_string(self.home.join("owner.token"))
            .unwrap()
            .trim()
            .into()
    }

    fn port(&self) -> u16 {
        let discovery: Value =
            serde_json::from_slice(&std::fs::read(self.home.join("daemon.json")).unwrap()).unwrap();
        discovery["port"].as_u64().unwrap() as u16
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
        let mut child = self
            .command(&["login", "--no-browser"])
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
        let (mut child, mut reader, url) = self.start_login();
        let (page, fragment) = url.split_once('#').unwrap();
        assert_eq!(page, "https://dmsg.net/_/login");
        let request = decode_request(&URL_SAFE_NO_PAD.decode(fragment).unwrap()).unwrap();
        let callback = http.get(&request.next_url).send().unwrap();
        assert_eq!(callback.status(), 200);
        assert!(callback.text().unwrap().contains("fetch('/auth/callback'"));

        let login = BasicIdentity::from_raw_key(&[seed; 32]);
        let delegation = Delegation {
            pubkey: request.session_pubkey.to_vec(),
            expiration: (now_ms() + 10 * 60_000) * 1_000_000,
            targets: Some(
                request
                    .targets
                    .iter()
                    .map(|t| Principal::from_text(t).unwrap())
                    .collect(),
            ),
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
                    "targets": delegation.targets.as_ref().unwrap().iter()
                        .map(|t| hex::encode(t.as_slice())).collect::<Vec<_>>(),
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

/// Poll `check` every half second until it returns a value.
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
        Self::start_as(upstream, None)
    }

    /// A forwarder that names `host` in every request, as a virtual host
    /// on the upstream server expects.
    fn start_as(upstream: &str, host: Option<&str>) -> Self {
        let host = host.map(str::to_string);
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
                let (upstream, rules, host) = (upstream.clone(), shared.clone(), host.clone());
                std::thread::spawn(move || {
                    let _ = Self::serve(stream, &upstream, &rules, host.as_deref());
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
        reader: &mut BufReader<std::net::TcpStream>,
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

    /// `head` with its `Host` header naming `host`.
    fn rehost(head: &str, host: &str) -> String {
        head.lines()
            .map(|l| match l.to_ascii_lowercase().starts_with("host:") {
                true => format!("Host: {host}"),
                false => l.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\r\n")
            + "\r\n"
    }

    fn serve(
        client: std::net::TcpStream,
        upstream: &str,
        rules: &std::sync::Mutex<Vec<(String, String)>>,
        host: Option<&str>,
    ) -> std::io::Result<()> {
        let mut reader = BufReader::new(client.try_clone()?);
        let Some((head, body)) = Self::read_message(&mut reader)? else {
            return Ok(());
        };
        let head = match host {
            Some(host) => Self::rehost(&head, host),
            None => head,
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

/// What a test environment adds to the plain one.
#[derive(Default)]
struct Options {
    /// Extra `config.toml` text for device B.
    b_policy: &'static str,
    /// Put fault-injecting proxies in front of the relay and the IC gateway.
    proxies: bool,
    /// Let the relay take paid letters.
    paid_contacts: bool,
    /// Serve Agent Delegation: the relay's agent host behind a proxy that
    /// names it, principal documents from the directory, and a Discourse
    /// host the daemons may sign for.
    agents: bool,
    /// Serve Agent Mail and Agent Knowledge: the private `dmsg-mail` and
    /// `dmsg-knowledge` Workers on local HTTPS, which the daemons trust
    /// through `agent_ca`.
    services: bool,
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
    /// The relay's agent host, through a proxy naming `agents.dmsg.test`.
    agent_proxy: Option<Proxy>,
    /// The Discourse host the daemons may sign for.
    adp: Option<adp::Adp>,
    /// The Agent Mail relay and the Agent Knowledge service, if served.
    services: Option<Services>,
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

/// Start `dmsg-mail` and `dmsg-knowledge` on local HTTPS with the public
/// localhost test certificate, whose PEM file the daemons get as `agent_ca`.
fn serve_services(cloud: &Path, dir: &Path, children: &mut Children) -> (Services, PathBuf) {
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
    (Services { mail, knowledge }, cert)
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
        let mut launch: Value = serde_json::from_str(&deployment.fixture_json()).unwrap();
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
            launch["agent_origin"] = "https://agents.dmsg.test".into();
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

        let agent_proxy = options.agents.then(|| {
            Proxy::start_as(
                &format!("http://127.0.0.1:{relay_port}"),
                Some("agents.dmsg.test"),
            )
        });
        let adp = options.agents.then(adp::Adp::start);
        let services = options
            .services
            .then(|| serve_services(&cloud, &dir, &mut children));
        // The owner credential stays in the home: tests never touch the
        // system credential store.
        let mut config = format!(
            "environment = \"local\"\nic_host = \"{ic_host}\"\nrelay = \"{relay}\"\nconnect_url = \"{connect}\"\nhandle = \"{}\"\nuser_homes = [\"{}\"]\nroot_key = \"{}\"\nsecure_storage = \"file\"\n",
            deployment.handle,
            deployment.user,
            deployment.root_key_hex,
        );
        let mut hosts = vec![];
        if let (Some(proxy), Some(adp)) = (&agent_proxy, &adp) {
            config.push_str(&format!("agent_service = \"{}\"\n", proxy.origin));
            hosts.push(format!("{{ host = \"{}\", label = \"test\" }}", adp.origin));
        }
        if let Some((services, cert)) = &services {
            config.push_str(&format!("agent_ca = \"{}\"\n", cert.display()));
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
        let desktop = |name: &str, policy: &str| {
            let home = dir.join(name);
            std::fs::create_dir_all(&home).unwrap();
            std::fs::write(home.join("config.toml"), format!("{config}{policy}")).unwrap();
            Desktop {
                bin: bin.clone(),
                home,
            }
        };
        let (a, b) = (desktop("a", ""), desktop("b", options.b_policy));
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
            config,
            relay_proxy,
            ic_proxy,
            agent_proxy,
            adp,
            services: services.map(|(services, _)| services),
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

    // Device A creates the account, writes an item and syncs.
    let status = a.login(http, 41).unwrap();
    assert_eq!(status["state"], "no_account");
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
    let titles: Vec<String> = a
        .run(&["vault", "list"])
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["item"]["title"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(titles.len(), 3);
    assert!(["alpha", "beta", "gamma"]
        .iter()
        .all(|t| titles.contains(&t.to_string())));
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

    // Another login cannot unlock this account's device.
    assert_eq!(b.run(&["lock"])["state"], "locked");
    assert!(b.login(http, 42).unwrap_err().contains("AuthRequired"));

    let status = b.run(&["status"]);
    assert_eq!(status["state"], "locked");
    assert_eq!(status["environment"], "local");
    assert_eq!(status["account"], account.as_str());
    assert_eq!(status["device_id"], device);
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

    // DT11 and R11: a 100 MiB file written on A reads back on B.
    let big = pattern(100 * 1024 * 1024, 7);
    let big_path = env.dir.join("big.bin");
    std::fs::write(&big_path, &big).unwrap();
    let file = a.run(&["file", "import", big_path.to_str().unwrap()]);
    let file_id = file["id"].as_str().unwrap().to_string();
    assert_eq!(file["item"]["file"]["key"], "");
    assert_eq!(file["item"]["file"]["sha256"], sha256_hex(&big).as_str());
    assert_eq!(a.run(&["sync"])["sent"], 1);
    b.run(&["sync"]);
    let out = env.dir.join("big.out");
    let got = b.run(&["file", "get", &file_id, "-o", out.to_str().unwrap()]);
    assert_eq!(got["size"], big.len());
    assert_eq!(sha256_hex(&std::fs::read(&out).unwrap()), sha256_hex(&big));
    assert_eq!(
        b.fail(&["file", "get", &file_id, "-o", out.to_str().unwrap()]),
        "INVALID_INPUT"
    );
    drop(big);
    eprintln!("100 MiB round trip done at {:?}", started.elapsed());

    // AT05: A's daemon dies during an upload; after a restart and a new
    // login the sync resumes and nothing is sent twice.
    let medium = pattern(64 * 1024 * 1024, 11);
    let medium_path = env.dir.join("medium.bin");
    std::fs::write(&medium_path, &medium).unwrap();
    let medium_id = a.run(&["file", "import", medium_path.to_str().unwrap()])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut sync = a
        .command(&["sync"])
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
    assert!(!sync.wait().unwrap().success(), "the sync finished first");
    env.daemons[0] = a.spawn(&mut env.children);
    assert_eq!(a.run(&["status"])["state"], "locked");
    let status = a.login(&env.http, 41).unwrap();
    assert_eq!(status["state"], "unlocked");
    assert_eq!(status["pending_sync"], 0, "{status}");
    let a_report = a.run(&["sync"]);
    let b_report = b.run(&["sync"]);
    assert_eq!(a_report["revisions"], b_report["revisions"]);
    // Eight versions of the shared item (the refused late edit is not one)
    // and the two files: each exactly once.
    assert_eq!(b_report["revisions"], 10);
    let out = env.dir.join("medium.out");
    b.run(&["file", "get", &medium_id, "-o", out.to_str().unwrap()]);
    assert_eq!(
        sha256_hex(&std::fs::read(&out).unwrap()),
        sha256_hex(&medium)
    );
    eprintln!("interrupted upload done at {:?}", started.elapsed());

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

    // AT04: after locking, the home holds no plaintext.
    let marker = b"FILE-PLAINTEXT-MARKER-4471";
    let note_path = env.dir.join("marker-name-4471.txt");
    std::fs::write(&note_path, marker).unwrap();
    a.run(&["file", "import", note_path.to_str().unwrap()]);
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
    assert_eq!(a.fail(&["file", "verify", &file_id]), "LOCKED");
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
    eprintln!("content acceptance done at {:?}", started.elapsed());
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
    assert!(own["capabilities"]
        .as_array()
        .unwrap()
        .contains(&json!("FormalApprove")));

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
    env.deployment.register_app(&AppRegistration {
        version: 1,
        environment: dmsg_types::Environment::Local,
        app_id: "token-list".into(),
        config_version: 1,
        origins: vec!["http://localhost:5173".into()],
        product_ids: vec![],
        capabilities: vec![AppCapability::SignDocument],
        profiles: vec![
            SigningProfile::TextStatementV1,
            SigningProfile::DigestStatementV1,
            SigningProfile::FileStatementV1,
        ],
        authentication_receiver: env.deployment.user,
        action_authority: env.deployment.user,
        action_schema: None,
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
    assert!(all
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["detail"]["operation_id"] == op2.as_str())
        .all(|r| r["state"] == "elsewhere"));
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
        assert!(methods["methods"]
            .as_array()
            .unwrap()
            .contains(&json!("signDocument")));
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
    let item = a.run(&["file", "import", file.to_str().unwrap()])["id"]
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
    assert_eq!(c.login(&http, 60).unwrap()["state"], "unlocked");
    let offline = c.rpc(&http, "channel/messages", json!({"id": id})).unwrap();
    assert!(texts(&offline).contains(&"new approved device".to_string()));
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

/// A balance as `wallet balance` reports it.
fn balance(desktop: &Desktop, ledger: &Principal) -> u128 {
    desktop.run(&["wallet", "balance", &ledger.to_text()])["balance"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
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

    // Another account pays for its plan on ckUSDT with the same funded
    // identity, as a wallet file.
    let pem = env.dir.join("wallet.pem");
    std::fs::write(&pem, ed25519_pem(42)).unwrap();
    let pem = pem.to_str().unwrap().to_string();
    let b = &env.b;
    assert_eq!(b.login(&http, 62).unwrap()["state"], "no_account");
    b.run(&["account", "create"]);
    let enable = b.run(&["device", "formal-approve", "on"]);
    assert_eq!(b.decide(&http, &enable, true).unwrap()["state"], "done");
    let wallet = b.run(&["wallet", "use", &pem]);
    assert_eq!(wallet["source"], "pem");
    let quote = b.run(&["plan", "quote", "plus", &ledger_usdt.to_text()]);
    assert_eq!(quote["terms"]["asset"], "CkUsdt");
    let request = b.run(&["plan", "checkout", quote["id"].as_str().unwrap()]);
    assert_eq!(b.decide(&http, &request, true).unwrap()["state"], "done");
    assert_eq!(
        b.run(&["plan", "order", quote["id"].as_str().unwrap()])["status"],
        "Applied"
    );

    // A PANDA membership backed by the wallet's neuron: accepted and
    // cooling down (activation follows the 65-minute cooling period).
    let (c, _) = env.desktop("c");
    assert_eq!(c.login(&http, 61).unwrap()["state"], "no_account");
    c.run(&["account", "create"]);
    let enable = c.run(&["device", "formal-approve", "on"]);
    assert_eq!(c.decide(&http, &enable, true).unwrap()["state"], "done");
    c.run(&["wallet", "use", &pem]);
    let quote = c.run(&[
        "plan",
        "quote",
        "plus",
        "--method",
        "panda",
        &"4d".repeat(32),
    ]);
    assert_eq!(quote["terms"]["method"], "Panda");
    let request = c.run(&["plan", "checkout", quote["id"].as_str().unwrap()]);
    assert_eq!(c.decide(&http, &request, true).unwrap()["state"], "done");
    let order = c.run(&["plan", "order", quote["id"].as_str().unwrap()]);
    assert_eq!(order["status"], "CoolingDown");
    assert_eq!(order["eligibility"], "Eligible");
    assert_eq!(order["neuron"], "4d".repeat(32));
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

    // Paying: the ledger's reply to the escrow funding is lost, then the
    // relay's reply to the admission. Each retry reads back what committed;
    // the wallet pays once and the escrow settles.
    let before = balance(a, &d.ledger);
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
    a.rpc(&http, "approval/reconcile", Value::Null).unwrap();
    assert_eq!(approval_state(a, &request), "done");
    let status = a.run(&["inbox", "order", &recipient, &order_id]);
    assert_eq!(status["state"], "visible");
    let escrow = a.run(&["inbox", "escrow", &order_id]);
    assert_eq!(escrow["escrow"]["decision"], "SettlementCommitted");
    let amount: u128 = order["quote"]["amount"].as_str().unwrap().parse().unwrap();
    assert_eq!(before - balance(a, &d.ledger), amount + 10);

    // The recipient opens it with the synced inbox key.
    let letters = b.run(&["inbox", "list"]);
    assert_eq!(letters[0]["text"], "Private paid contact");
    assert_eq!(letters[0]["state"], "visible");
    assert_eq!(letters[0]["sender"], sender.as_str());
    env.finish();
}

/// An MCP client on `dmsg mcp`'s stdio, as Claude Code or another agent
/// runtime drives it.
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
    fn start(desktop: &Desktop, credential: &str) -> Self {
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
        let mut client = Self {
            child,
            stdin,
            lines,
            pending: Default::default(),
            id: 0,
        };
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
        verify_delegation_credential, DelegationCredential, DelegationRecord, PrincipalDocument,
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
    let service = &env.agent_proxy.as_ref().unwrap().origin;
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
            ..Options::default()
        },
    );
    let http = env.http.clone();
    let host = env.adp.as_ref().unwrap().origin.clone();
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
    assert!(vault
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["item"]["tags"] == json!(["dmsg-controller"])));

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
    let proxy = env.agent_proxy.as_ref().unwrap();
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
    assert_eq!(names.len(), 50);
    assert!(names.contains(&"agent_protocols_room_join") && names.contains(&"dmsg_vault_list"));
    assert!(names.contains(&"agent_protocols_mail_send"));
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
    let foreign = mcp
        .tool(
            "agent_protocols_room_create",
            json!({"host": "https://evil.test", "topic": "x", "visibility": "public", "start_time": 1, "end_time": 2}),
        )
        .unwrap_err();
    assert_eq!(foreign["code"], "permission_denied");
    assert!(mcp
        .tool(
            "agent_protocols_host_add",
            json!({"host": "https://evil.test"})
        )
        .is_err());

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
    let sent = mcp
        .tool(
            "agent_protocols_room_send_message",
            json!({"room_id": room, "content": "hello from claude"}),
        )
        .unwrap();
    assert_eq!(sent["status"], "sent");
    let adp = env.adp.as_ref().unwrap();
    assert_eq!(adp.messages(&room), ["hello from claude"]);

    // The host's own agent changes the topic, which moves the room head.
    // Claude's next message, based on the head it saw, is held with the
    // change (DT07); sending the draft re-signs it against the new head.
    a.run(&[
        "ap",
        "room_submit_event",
        &json!({"room_id": room, "type": "room.update", "payload": {"topic": "W6 review"}})
            .to_string(),
    ]);
    let held = mcp
        .tool(
            "agent_protocols_room_send_message",
            json!({"room_id": room, "content": "late reply"}),
        )
        .unwrap();
    assert_eq!(held["status"], "held", "{held}");
    assert!(held["changes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["type"] == "room.update"));
    let draft = held["draft"]["id"].as_str().unwrap().to_string();

    // A nonce another service saw: within the bound the connector resyncs
    // and signs once more (DT06).
    let before = adp.nonce(&claude).unwrap();
    adp.refuse_next_nonce(before + 1_000, true);
    let resent = mcp
        .tool(
            "agent_protocols_room_send_message",
            json!({"room_id": room, "content": "after resync", "on_head_mismatch": "send_anyway"}),
        )
        .unwrap();
    assert_eq!(resent["status"], "sent", "{resent}");
    assert_eq!(adp.nonce(&claude), Some(before + 1_001));
    // Beyond the bound the value is refused, not applied.
    adp.refuse_next_nonce(now_ms() + (1 << 33), false);
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
    a.run(&["grants", "add", "claude", "vault.list", "attest.request"]);
    let items = mcp.tool("dmsg_vault_list", json!({})).unwrap();
    assert!(items
        .as_array()
        .unwrap()
        .iter()
        .all(|i| i.get("secret").is_none()));
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
    let grants = a.run(&["grants", "list"]);
    a.run(&["grants", "revoke", grants[0]["id"].as_str().unwrap()]);
    assert_eq!(
        mcp.tool("dmsg_vault_list", json!({})).unwrap_err()["code"],
        "FORBIDDEN"
    );
    drop(mcp);

    // A restarted daemon restores the connector: the held draft, the read
    // position and the nonce (DT09). Its keys come back with the login.
    let nonce = adp.nonce(&claude).unwrap();
    env.restart(0);
    let (a, adp) = (&env.a, env.adp.as_ref().unwrap());
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
    assert!(adp.nonce(&claude).unwrap() > nonce);
    assert_eq!(
        adp.messages(&room),
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
    let _ = host_agent;
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
    let mut mcp = McpClient::start(a, &credential);
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
    /// The relay's webhook host, through a proxy that names it.
    hooks: String,
    key: ed25519_dalek::SigningKey,
    origin: String,
}

impl Upstream {
    fn start(hooks: &str) -> Self {
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
            hooks: hooks.to_string(),
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
            .post(format!("{}/mcp-events/{path}", self.hooks))
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
    let mut env = Env::launch("events", Options::default());
    let http = env.http.clone();
    let hooks = Proxy::start_as(&env.relay, Some("hooks.dmsg.local"));
    let upstream = Upstream::start(&hooks.origin);
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
    assert_eq!(url, format!("https://hooks.dmsg.local/mcp-events/{id}"));
    assert!(secret.starts_with("whsec_"));

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
    assert!(std::fs::read_to_string(a.home.join("logs/daemon.log"))
        .unwrap()
        .contains("dmsg daemon listening"));

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
