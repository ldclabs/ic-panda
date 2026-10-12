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
        Command::new(&self.bin)
            .arg("--home")
            .arg(&self.home)
            .arg("daemon")
            .stderr(Stdio::null())
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
}

impl Env {
    /// `b_policy` is extra `config.toml` text for device B.
    fn start(name: &str, b_policy: &str) -> Self {
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
        launch["client"] = "desktop".into();
        launch["port"] = free_port().into();
        launch["ttl_ms"] = 1_200_000.into();
        let connect = format!("http://localhost:{}", free_port());
        launch["connect_origin"] = connect.clone().into();
        std::fs::write(dir.join("cloud-launch.json"), launch.to_string()).unwrap();
        children.0.push(
            Command::new("node")
                .arg(cloud.join("dmsg-core/scripts/extension-probe-server.mjs"))
                .current_dir(&cloud)
                .env("DMSG_CLOUD_FIXTURE_DIR", &dir)
                .env("WRANGLER_SEND_METRICS", "false")
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let relay = wait_for(&dir.join("cloud-ready.json"), &mut children)["relay"]
            .as_str()
            .unwrap()
            .to_string();

        // The owner credential stays in the home: tests never touch the
        // system credential store.
        let config = format!(
            "environment = \"local\"\nic_host = \"{}\"\nrelay = \"{relay}\"\nconnect_url = \"{connect}\"\nhandle = \"{}\"\nuser_homes = [\"{}\"]\nroot_key = \"{}\"\nsecure_storage = \"file\"\n",
            deployment.gateway.trim_end_matches('/'),
            deployment.handle,
            deployment.user,
            deployment.root_key_hex,
        );
        let desktop = |name: &str, policy: &str| {
            let home = dir.join(name);
            std::fs::create_dir_all(&home).unwrap();
            std::fs::write(home.join("config.toml"), format!("{config}{policy}")).unwrap();
            Desktop {
                bin: bin.clone(),
                home,
            }
        };
        let (a, b) = (desktop("a", ""), desktop("b", b_policy));
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
        }
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
