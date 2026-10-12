//! Desktop acceptance (work packages W1 and W2): two `dmsg` daemons with
//! their own homes share one account against the real canisters (PocketIC)
//! and the private relay (workerd).
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
        let _ = std::fs::remove_file(self.home.join("daemon.json"));
        children.0.push(
            Command::new(&self.bin)
                .arg("--home")
                .arg(&self.home)
                .arg("daemon")
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        wait_for(&self.home.join("daemon.json"), children);
        children.0.len() - 1
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

/// One deployment, one relay and two desktops, `a` and `b`.
struct Env {
    dir: PathBuf,
    children: Children,
    deployment: support::Deployment,
    http: reqwest::blocking::Client,
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
            "environment = \"local\"\nic_host = \"{}\"\nrelay = \"{relay}\"\nhandle = \"{}\"\nuser_homes = [\"{}\"]\nroot_key = \"{}\"\nsecure_storage = \"file\"\n",
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
            a,
            b,
            daemons,
        }
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
