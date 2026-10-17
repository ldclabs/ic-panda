//! `dmsg doctor` (plan §13): checks what this home needs to run and
//! reports each check as `ok`, `warn` or `fail`; it changes nothing.

use crate::home::{Home, Settings, Storage};
use crate::locale::tr;
use crate::remote::Remote;
use serde_json::{Value, json};
use std::time::Duration;

/// Clock skew that is fine, and beyond which signed requests fail: device
/// signatures carry 45-second deadlines and relay evidence is fresh for 60.
const SKEW_OK_MS: i64 = 5_000;
const SKEW_MAX_MS: i64 = 30_000;

#[derive(Default)]
struct Report(Vec<Value>);

impl Report {
    fn add(&mut self, name: &str, status: &str, detail: String) {
        self.0
            .push(json!({"name": name, "status": status, "detail": detail}));
    }
}

/// Run every check; `ok` is false when any failed.
pub async fn run(home: &Home) -> Value {
    let mut report = Report(Vec::new());
    let settings = match home.settings() {
        Ok(settings) => {
            let environment = dmsg_client::config::environment_name(&settings.config.environment);
            let detail = match settings.built_in {
                true => format!("{environment} (built in)"),
                false => environment.to_string(),
            };
            report.add("config", "ok", detail);
            Some(settings)
        }
        Err(error) => {
            report.add("config", "fail", error.to_string());
            None
        }
    };
    permissions(home, settings.as_ref(), &mut report);
    if let Some(settings) = &settings {
        credentials(home, settings, &mut report);
        // The network checks run side by side and report in this order.
        let (mut local, mut remote, mut homes) = Default::default();
        tokio::join!(
            daemon(home, &mut local),
            relay(settings, &mut remote),
            registry(settings, &mut homes)
        );
        let reports: [Report; 3] = [local, remote, homes];
        report.0.extend(reports.into_iter().flat_map(|r| r.0));
    }
    let logs = crate::logs::dir(&home.dir);
    report.add("logs", "ok", logs.display().to_string());
    let ok = report.0.iter().all(|c| c["status"] != "fail");
    json!({"ok": ok, "checks": report.0})
}

/// The home is for its owner only.
fn permissions(home: &Home, settings: Option<&Settings>, report: &mut Report) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &std::path::Path| {
            std::fs::metadata(path)
                .ok()
                .map(|m| m.permissions().mode() & 0o777)
        };
        let dir = home.dir.display();
        match mode(&home.dir) {
            None => report.add(
                "permissions",
                "warn",
                tr!("{dir} does not exist yet", "{dir} 还不存在"),
            ),
            Some(m) if m & 0o077 != 0 => report.add(
                "permissions",
                "warn",
                tr!(
                    "other users can open {dir} (mode {m:o}); run `chmod 700 {dir}`",
                    "其他用户可以访问 {dir}（权限 {m:o}）；运行 `chmod 700 {dir}`"
                ),
            ),
            Some(_) => {
                let token = home.dir.join("owner.token");
                match (settings.map(|s| s.storage), mode(&token)) {
                    (Some(Storage::File), Some(m)) if m & 0o077 != 0 => report.add(
                        "permissions",
                        "fail",
                        tr!(
                            "other users can read the owner credential (mode {m:o})",
                            "其他用户可以读取 owner 凭证（权限 {m:o}）"
                        ),
                    ),
                    _ => report.add("permissions", "ok", tr!("owner only", "仅所有者可访问")),
                }
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (home, settings);
        report.add("permissions", "ok", tr!("not checked here", "此平台不检查"));
    }
}

/// The owner credential can be read where the settings keep it.
fn credentials(home: &Home, settings: &Settings, report: &mut Report) {
    let store = match settings.storage {
        Storage::System => tr!("system credential store", "系统凭据库"),
        Storage::File => "owner.token".to_string(),
    };
    match home.owner_token(settings, false) {
        Ok(_) => report.add("credentials", "ok", store),
        Err(dmsg_types::Error::AuthRequired) => report.add(
            "credentials",
            "warn",
            tr!(
                "no owner credential in the {store} yet; the daemon creates it",
                "{store}中还没有 owner 凭证；daemon 启动时创建"
            ),
        ),
        Err(error) => report.add("credentials", "fail", format!("{store}: {error}")),
    }
}

/// The daemon answers, runs this version, and keeps its hint channel.
async fn daemon(home: &Home, report: &mut Report) {
    let status = match Remote::connect(home) {
        Ok(remote) => remote.status().await,
        Err(failure) => Err(failure),
    };
    let status = match status {
        Ok(status) => status,
        Err(failure) if failure.code == "DAEMON_STOPPED" => {
            report.add(
                "daemon",
                "warn",
                tr!(
                    "not running; start it with `dmsg start`",
                    "未运行；用 `dmsg start` 启动"
                ),
            );
            return;
        }
        Err(failure) => {
            report.add(
                "daemon",
                "fail",
                format!("{} ({})", failure.message, failure.code),
            );
            return;
        }
    };
    let (version, port) = (status["version"].as_str().unwrap_or("?"), &status["port"]);
    match version == crate::release::VERSION {
        true => report.add(
            "daemon",
            "ok",
            tr!(
                "{version} on 127.0.0.1:{port}",
                "{version}，127.0.0.1:{port}"
            ),
        ),
        false => report.add(
            "daemon",
            "warn",
            tr!(
                "runs {version} but this dmsg is {}; run `dmsg restart`",
                "运行的是 {version}，而本程序是 {}；运行 `dmsg restart`",
                crate::release::VERSION
            ),
        ),
    }
    let hints = &status["hints"];
    if status["account"].is_null() {
        report.add("hints", "ok", tr!("no account yet", "还没有账户"));
    } else if hints["connected"] == true {
        report.add(
            "hints",
            "ok",
            tr!(
                "connected, {} received",
                "已连接，收到 {} 个",
                hints["received"]
            ),
        );
    } else {
        let error = hints["last_error"].as_str().unwrap_or("-");
        report.add(
            "hints",
            "warn",
            tr!("not connected: {error}", "未连接：{error}"),
        );
    }
}

/// The relay's `/ready`, and this clock against the `time` it reports.
async fn relay(settings: &Settings, report: &mut Report) {
    let origin = settings.config.relay_origin();
    let http = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(http) => http,
        Err(error) => return report.add("relay", "fail", error.to_string()),
    };
    let sent = dmsg_client::now_ms();
    let response = match http.get(format!("{origin}/ready")).send().await {
        Ok(response) => response,
        Err(error) => {
            report.add(
                "relay",
                "fail",
                tr!(
                    "{origin}: unreachable ({error})",
                    "{origin}：无法连接（{error}）"
                ),
            );
            return;
        }
    };
    let received = dmsg_client::now_ms();
    let body: Value = response.json().await.unwrap_or_default();
    let ready = &body["data"];
    let expected = dmsg_client::config::environment_name(&settings.config.environment);
    match (ready["ready"] == true, ready["environment"].as_str()) {
        (true, Some(environment)) if environment == expected => {
            report.add("relay", "ok", origin.clone())
        }
        (true, environment) => report.add(
            "relay",
            "fail",
            tr!(
                "{origin} serves {}, not {expected}",
                "{origin} 是 {} 环境，不是 {expected}",
                environment.unwrap_or("?")
            ),
        ),
        (false, _) => {
            let problems = ready["problems"]
                .as_array()
                .map(|p| {
                    p.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            report.add(
                "relay",
                "fail",
                tr!(
                    "{origin} is not ready: {problems}",
                    "{origin} 未就绪：{problems}"
                ),
            );
        }
    }
    let Some(time) = ready["time"].as_u64() else {
        report.add(
            "clock",
            "warn",
            tr!("the relay reports no time", "中继没有返回时间"),
        );
        return;
    };
    // The relay read its clock in the middle of the request.
    let skew = (sent + received) as i64 / 2 - time as i64;
    let seconds = skew as f64 / 1000.0;
    let status = match skew.abs() {
        s if s <= SKEW_OK_MS => "ok",
        s if s <= SKEW_MAX_MS => "warn",
        _ => "fail",
    };
    report.add(
        "clock",
        status,
        tr!(
            "{seconds:+.1} s against the relay",
            "与中继相差 {seconds:+.1} 秒"
        ),
    );
}

/// The registry lists the user homes this configuration pins.
async fn registry(settings: &Settings, report: &mut Report) {
    let config = &settings.config;
    let homes = match dmsg_client::ic::Ic::anonymous(config).await {
        Ok(ic) => dmsg_client::account::user_homes(&ic, config.handle, &config.user_homes).await,
        Err(error) => Err(error),
    };
    match homes {
        Ok(homes) => report.add(
            "registry",
            "ok",
            tr!(
                "{} user homes, {} open for registration",
                "{} 个用户 home，{} 个可注册",
                homes.user_homes.len(),
                homes.registration_homes.len()
            ),
        ),
        Err(error) => report.add(
            "registry",
            "fail",
            format!("{} ({})", error, dmsg_client::error_code(&error)),
        ),
    }
}

/// The report for people: one line per check.
pub fn lines(report: &Value) -> String {
    report["checks"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            let mark = match c["status"].as_str() {
                Some("ok") => "✓",
                Some("warn") => "!",
                _ => "✗",
            };
            format!(
                "{mark} {:<12} {}",
                c["name"].as_str().unwrap_or_default(),
                c["detail"].as_str().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
