//! Starting, restarting and updating the daemon.
//!
//! `dmsg start` runs `dmsg daemon` in the background; its run log is
//! `<home>/logs/dmsg-<day>.log` ([`crate::logs`]) and its stderr, only what
//! fails before that log opens, `<home>/logs/stderr.log`. `restart` and `update` stop the daemon only
//! when nothing approved, no account update and no root rotation is left
//! unsettled: they wait for it (`--wait`, 60 seconds) and otherwise refuse
//! with `BUSY`, unless `--interrupt`; the running operation always finishes
//! first, and everything unsettled is journaled and resumes after the
//! restart (plan DT10). An update replaces the binary only after its
//! checksum (and, on macOS, its signer) checked, and only once the daemon
//! stopped; it then starts the new daemon and checks its version.

use crate::home::Home;
use crate::locale::tr;
use crate::release::{self, VERSION};
use crate::remote::{Failure, Remote};
use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn unavailable(error: impl std::fmt::Display) -> Failure {
    Failure::new("UNAVAILABLE", error.to_string())
}

/// The running daemon's status, if one answers.
async fn running(home: &Home) -> Option<Value> {
    Remote::connect(home).ok()?.status().await.ok()
}

/// Start the daemon in the background unless one runs; returns its status.
pub async fn start(home: &Home, exe: &Path) -> Result<Value, Failure> {
    if let Some(status) = running(home).await {
        return Ok(status);
    }
    // Report a bad configuration here, not in the log.
    home.settings()
        .map_err(|e| Failure::new("INVALID_INPUT", e.to_string()))?;
    let dir = std::path::absolute(&home.dir).map_err(unavailable)?;
    // A discovery file left by a daemon that died names nothing now.
    if home.lock().is_ok() {
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
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = running(home).await {
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
    if in_flight["root_rotation"] == true {
        parts.push(tr!("a root rotation", "一次换根"));
    }
    parts.join(&tr!(", ", "、"))
}

/// Stop the daemon once nothing is in flight, waiting up to `wait`; with
/// `interrupt`, after the running operation. Returns when it has exited.
pub async fn drain(home: &Home, interrupt: bool, wait: Duration) -> Result<(), Failure> {
    let remote = Remote::connect(home)?;
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
                format!("not stopped: {busy}; unlock so it settles, or use --interrupt (it resumes after the restart)"),
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
    if running(home).await.is_some() {
        drain(home, interrupt, wait).await?;
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
    let restart = running(home).await.is_some();
    if restart {
        drain(home, interrupt, wait).await?;
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
