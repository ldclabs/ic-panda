//! Desktop acceptance (work package W1): two `dmsg` daemons with their own
//! homes share one account against the real canisters (PocketIC) and the
//! private relay (workerd).
//!
//! The test plays the dmsg.net login page: it reads the daemon's request from
//! the URL fragment, signs a delegation to the session key with a generated
//! Ed25519 login, and posts it through the daemon's callback exactly as the
//! callback page does. No daemon code path is test-only.
//!
//! Needs the dMsg Wasm (`make build-dmsg`), the `dmsg` binary
//! (`cargo build -p dmsg_cli`, or `DMSG_BIN`), `POCKET_IC_BIN` and
//! `DMSG_CLOUD_DIR` pointing at the private relay checkout.
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

/// Children killed when the test ends, however it ends.
struct Children(Vec<Child>);

impl Drop for Children {
    fn drop(&mut self) {
        for child in &mut self.0 {
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
            .unwrap()
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

#[test]
#[ignore = "needs the dmsg binary, PocketIC and the private relay (DMSG_CLOUD_DIR)"]
fn two_desktop_devices_share_an_account() {
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
    let dir = std::env::temp_dir().join(format!("dmsg-desktop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut children = Children(vec![]);

    let mut deployment = support::Deployment::start();
    let mut launch: Value = serde_json::from_str(&deployment.fixture_json()).unwrap();
    launch["client"] = "desktop".into();
    launch["port"] = free_port().into();
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

    let config = format!(
        "environment = \"local\"\nic_host = \"{}\"\nrelay = \"{relay}\"\nhandle = \"{}\"\nuser_homes = [\"{}\"]\nroot_key = \"{}\"\n",
        deployment.gateway.trim_end_matches('/'),
        deployment.handle,
        deployment.user,
        deployment.root_key_hex,
    );
    let desktop = |name: &str| {
        let home = dir.join(name);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("config.toml"), &config).unwrap();
        Desktop {
            bin: bin.clone(),
            home,
        }
    };
    let (a, b) = (desktop("a"), desktop("b"));
    for device in [&a, &b] {
        children.0.push(
            Command::new(&device.bin)
                .arg("--home")
                .arg(&device.home)
                .arg("daemon")
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        wait_for(&device.home.join("daemon.json"), &mut children);
    }
    let http = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();

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
    let status = a.login(&http, 41).unwrap();
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

    // Device B logs in with the same login and asks to join.
    let status = b.login(&http, 41).unwrap();
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

    // B binds, opens the rotated root and reads A's item.
    let status = b.run(&["unlock"]);
    assert_eq!(status["state"], "unlocked");
    assert_eq!(status["account"], account.as_str());
    assert_eq!(status["root_generation"], 2);
    assert_eq!(b.run(&["sync"])["revisions"], 1);
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
    assert_eq!(titles.len(), 2);
    assert!(titles.contains(&"alpha".into()) && titles.contains(&"beta".into()));
    assert_eq!(
        a.run(&["vault", "show", reply["id"].as_str().unwrap()])["item"]["body"],
        "hello from B"
    );

    // Locking forgets the login and every key; a new login unlocks again.
    assert_eq!(b.run(&["lock"])["state"], "locked");
    assert_eq!(b.fail(&["vault", "list"]), "LOCKED");
    let status = b.login(&http, 41).unwrap();
    assert_eq!(status["state"], "unlocked");
    assert_eq!(b.run(&["vault", "list"]).as_array().unwrap().len(), 2);

    // Another login cannot unlock this account's device.
    assert_eq!(b.run(&["lock"])["state"], "locked");
    assert!(b.login(&http, 42).unwrap_err().contains("AuthRequired"));

    let status = b.run(&["status"]);
    assert_eq!(status["state"], "locked");
    assert_eq!(status["environment"], "local");
    assert_eq!(status["account"], account.as_str());
    assert_eq!(status["device_id"], device);
    assert!(status["version"].is_string() && status["instance"].is_string());

    for device in [&a, &b] {
        device.run(&["stop"]);
    }
    drop(children);
    deployment.ic.stop_live();
    let _ = std::fs::remove_dir_all(&dir);
}
