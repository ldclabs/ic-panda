//! Starting, restarting and updating the daemon.
//!
//! `dmsg start` runs `dmsg daemon` in the background; its run log is
//! `<home>/logs/dmsg-<day>.log` ([`crate::logs`]) and its stderr, only what
//! fails before that log opens, `<home>/logs/stderr.log`. A running daemon
//! older than the binary that starts it is restarted the way `restart` does
//! without waiting, and left running when busy. `restart` and `update` stop
//! the daemon only when nothing approved, no account update and no root
//! rotation is left unsettled: they wait for it (`--wait`, 60 seconds) and
//! otherwise refuse with `BUSY`, unless `--interrupt`; the running operation
//! always finishes first, and everything unsettled is journaled and resumes
//! after the restart (plan DT10). An update replaces the binary only after its
//! checksum and applicable release signature checked, and only once the daemon
//! stopped; it then starts the new daemon and checks its version.

use crate::home::Home;
use crate::locale::tr;
use crate::release::{self, VERSION};
use crate::remote::{Failure, Remote};
use serde_json::{Value, json};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long `dmsg start` waits for the daemon to listen. Allows time for a
/// macOS Keychain permission dialog after a signing-team change.
const START_TIMEOUT: Duration = Duration::from_secs(120);

fn unavailable(error: impl std::fmt::Display) -> Failure {
    Failure::new("UNAVAILABLE", error.to_string())
}

/// Read the home's owner credential at most once and cache it for this command.
fn cached_owner_token(home: &Home, cache: &mut Option<String>) -> Result<String, Failure> {
    if let Some(token) = cache {
        return Ok(token.clone());
    }
    let settings = home
        .settings()
        .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
    let token = home
        .owner_token(&settings, false)
        .map_err(|e| Failure::new(&dmsg_client::error_code(&e), e.to_string()))?;
    *cache = Some(token.clone());
    Ok(token)
}

/// A stopped daemon needs no credential. Surface errors from a live one.
async fn running_remote(
    home: &Home,
    owner_token: &mut Option<String>,
) -> Result<Option<(Remote, Value)>, Failure> {
    if home.discovery().is_none() {
        return Ok(None);
    }
    match home.lock() {
        Ok(_lock) => {
            // A stopped daemon left this file. Remove it while we hold the lock.
            home.unpublish();
            return Ok(None);
        }
        Err(dmsg_types::Error::Locked) => {}
        Err(error) => return Err(unavailable(error)),
    }
    let token = cached_owner_token(home, owner_token)?;
    // A Keychain prompt can outlive the process that published the port.
    let Some(discovery) = home.discovery() else {
        return Ok(None);
    };
    let remote = Remote::connect_with_token(&discovery, token)?;
    match remote.status().await {
        Ok(status) => Ok(Some((remote, status))),
        Err(error) if error.code == "DAEMON_STOPPED" => Ok(None),
        Err(error) => Err(error),
    }
}

/// Start the daemon in the background unless one runs; returns its status.
/// A running daemon older than this binary, such as one left running while
/// its binary was upgraded, gives way once nothing is in flight; a busy one
/// keeps running until a later start.
pub async fn start(home: &Home, exe: &Path) -> Result<Value, Failure> {
    let mut owner_token = None;
    if let Some((remote, status)) = running_remote(home, &mut owner_token).await? {
        let older = status["version"]
            .as_str()
            .is_some_and(|version| release::newer(VERSION, version));
        if !older
            || drain_with_remote(home, remote, false, Duration::ZERO)
                .await
                .is_err()
        {
            return Ok(status);
        }
    }
    // Report a bad configuration before starting the child.
    home.settings()
        .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
    let dir = std::path::absolute(&home.dir).map_err(unavailable)?;
    // Prepare the owner's private home before opening the child's log.
    if let Ok(_lock) = home.lock() {
        home.unpublish();
    }
    let logs = crate::logs::dir(&dir);
    std::fs::create_dir_all(&logs).map_err(unavailable)?;
    let log_path = logs.join("stderr.log");
    let log = std::fs::File::create(&log_path).map_err(unavailable)?;
    let mut command = Command::new(exe);
    command
        .arg("--home")
        .arg(&dir)
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP
        std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000 | 0x200);
    }
    let mut child = command.spawn().map_err(unavailable)?;
    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        if let Some((_, status)) = running_remote(home, &mut owner_token).await? {
            return Ok(status);
        }
        if child.try_wait().map_err(unavailable)?.is_some() {
            let text = std::fs::read_to_string(&log_path).unwrap_or_default();
            let last = text.lines().last().unwrap_or("no diagnostics");
            return Err(unavailable(format!("the daemon exited: {last}")));
        }
        if Instant::now() > deadline {
            return Err(unavailable(format!(
                "the daemon did not start; see {}",
                log_path.display()
            )));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// What is in flight, for people.
fn describe(in_flight: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(n) = in_flight["approvals"].as_u64().filter(|n| *n > 0) {
        parts.push(tr!(
            "{n} approved request(s) whose outcome is not settled",
            "{n} 个已批准、结果尚未确定的请求"
        ));
    }
    if in_flight["account_update"] == true {
        parts.push(tr!(
            "an account update whose outcome is not known",
            "一次结果未知的账户更新"
        ));
    }
    if in_flight["snapshot_work"] == true {
        parts.push(tr!("a Vault snapshot", "保险库快照"));
    }
    if in_flight["root_rotation"] == true {
        parts.push(tr!("a root rotation", "一次换根"));
    }
    parts.join(&tr!(", ", "、"))
}

/// Stop the daemon once nothing is in flight, waiting up to `wait`; with
/// `interrupt`, after the running operation. Returns when it has exited.
pub async fn drain(home: &Home, interrupt: bool, wait: Duration) -> Result<(), Failure> {
    let remote = Remote::connect(home)?;
    drain_with_remote(home, remote, interrupt, wait).await
}

/// Stop the already-connected daemon once nothing is in flight, waiting up to
/// `wait`; with `interrupt`, after the running operation.
async fn drain_with_remote(
    home: &Home,
    remote: Remote,
    interrupt: bool,
    wait: Duration,
) -> Result<(), Failure> {
    let deadline = Instant::now() + wait;
    let mut told = false;
    loop {
        let reply = remote
            .call("daemon/stop", json!({"if_idle": !interrupt}))
            .await?;
        if reply["stopped"] == true {
            break;
        }
        let busy = describe(&reply["in_flight"]);
        if Instant::now() >= deadline {
            return Err(Failure::new(
                "BUSY",
                format!(
                    "not stopped: {busy}; unlock so it settles, or use --interrupt (it resumes after the restart)"
                ),
            ));
        }
        if !told {
            eprintln!("{}", tr!("Waiting for {busy}…", "等待{busy}…"));
            told = true;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    // Gone once it released the home's lock.
    let deadline = Instant::now() + Duration::from_secs(30);
    while home.lock().is_err() {
        if Instant::now() > deadline {
            return Err(unavailable("the daemon did not exit"));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Ok(())
}

/// `dmsg restart`.
pub async fn restart(home: &Home, interrupt: bool, wait: Duration) -> Result<Value, Failure> {
    if let Some((remote, _)) = running_remote(home, &mut None).await? {
        drain_with_remote(home, remote, interrupt, wait).await?;
    }
    let exe = std::env::current_exe().map_err(unavailable)?;
    start(home, &exe).await
}

/// `dmsg update --check`: the machine-readable state the desktop reads.
pub async fn check() -> Result<Value, Failure> {
    let exe = std::env::current_exe().map_err(unavailable)?;
    let latest = release::latest(&release::http()?).await?;
    Ok(json!({
        "current": VERSION,
        "latest": latest,
        "available": release::newer(&latest, VERSION),
        "homebrew": release::homebrew(&exe),
    }))
}

/// `dmsg update`: install the latest release over this binary; with
/// `force`, also when it is not newer.
pub async fn update(
    home: &Home,
    force: bool,
    interrupt: bool,
    wait: Duration,
) -> Result<Value, Failure> {
    let exe = std::env::current_exe().map_err(unavailable)?;
    if release::homebrew(&exe) {
        return Err(Failure::new(
            "UNSUPPORTED",
            "Homebrew manages this dmsg; run `brew upgrade dmsg`",
        ));
    }
    let target = std::fs::canonicalize(&exe).map_err(unavailable)?;
    let http = release::http()?;
    let latest = release::latest(&http).await?;
    if !release::newer(&latest, VERSION) && !force {
        return Ok(json!({"updated": false, "current": VERSION, "latest": latest}));
    }
    let bytes = release::download(&http, &latest).await?;
    let existing = running_remote(home, &mut None).await?;
    let restart = existing.is_some();
    if let Some((remote, _)) = existing {
        drain_with_remote(home, remote, interrupt, wait).await?;
    }
    // A failed replacement leaves the old binary, which starts again.
    let placed = release::place(&bytes, &target);
    if placed.is_err() && restart {
        start(home, &target).await?;
    }
    placed?;
    let mut reply = json!({"updated": true, "from": VERSION, "to": latest, "path": target, "restarted": restart});
    if restart {
        let status = start(home, &target).await?;
        if status["version"] != latest.as_str() {
            return Err(Failure::new(
                "INTEGRITY_FAILED",
                format!("the restarted daemon reports version {}", status["version"]),
            ));
        }
        reply["status"] = status;
    }
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dmsg_client::testing::TempDir;

    fn file_home(dir: &TempDir) -> Home {
        let production = dmsg_client::config::PRODUCTION;
        std::fs::write(
            dir.path("config.toml"),
            format!("secure_storage = \"file\"\n{production}"),
        )
        .unwrap();
        Home { dir: dir.0.clone() }
    }

    #[tokio::test]
    async fn start_reports_owner_errors_without_launching_another_daemon() {
        let dir = TempDir::new();
        let home = file_home(&dir);
        let held = home.lock().unwrap();
        home.publish(&crate::home::Discovery {
            port: 1,
            pid: 0,
            instance: "existing".into(),
        })
        .unwrap();
        let log_dir = crate::logs::dir(&home.dir);
        std::fs::create_dir_all(&log_dir).unwrap();
        let log = log_dir.join("stderr.log");
        std::fs::write(&log, "original daemon log").unwrap();
        let exe = dir.0.join("no-executable");

        assert_eq!(start(&home, &exe).await.unwrap_err().code, "AUTH_REQUIRED");
        // A storage read error is distinct from an absent owner credential.
        std::fs::create_dir(home.dir.join("owner.token")).unwrap();
        assert_eq!(start(&home, &exe).await.unwrap_err().code, "UNAVAILABLE");
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "original daemon log"
        );
        assert!(home.discovery().is_some());

        // After the daemon exits, its stale discovery must not require a credential.
        drop(held);
        let _ = start(&home, &exe).await.unwrap_err();
        assert!(home.discovery().is_none());
    }

    #[test]
    fn cached_owner_token_reads_storage_at_most_once() {
        let dir = TempDir::new();
        let home = file_home(&dir);

        // Missing credential fails immediately with AUTH_REQUIRED.
        let mut cache = None;
        let missing = cached_owner_token(&home, &mut cache).unwrap_err();
        assert_eq!(missing.code, "AUTH_REQUIRED");
        assert!(cache.is_none());

        let token_path = home.dir.join("owner.token");
        std::fs::write(&token_path, "secret-owner-token\n").unwrap();
        assert_eq!(
            cached_owner_token(&home, &mut cache).unwrap(),
            "secret-owner-token"
        );
        // Removing the backing file proves subsequent calls reuse the cached token.
        std::fs::remove_file(&token_path).unwrap();
        assert_eq!(
            cached_owner_token(&home, &mut cache).unwrap(),
            "secret-owner-token"
        );
    }
}
