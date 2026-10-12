//! `dmsg autostart`: start the daemon when the current user logs in. macOS
//! uses a LaunchAgent, Linux a systemd user unit (XDG autostart without a
//! user systemd), Windows the user's `Run` key, which runs `dmsg start`.
//! Registering does not start a daemon now, and removing the registration
//! leaves a running daemon alone.

use crate::remote::Failure;
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(any(target_os = "linux", windows))]
use std::process::Command;

#[cfg(any(target_os = "macos", test))]
const LABEL: &str = "net.dmsg.daemon";
#[cfg(target_os = "linux")]
const UNIT: &str = "dmsg.service";
#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const RUN_VALUE: &str = "dMsg";

fn failed(error: impl std::fmt::Display) -> Failure {
    Failure::new("UNAVAILABLE", error.to_string())
}

#[cfg(unix)]
fn user_home() -> Result<PathBuf, Failure> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| failed("HOME is not set"))
}

#[cfg(any(target_os = "linux", windows))]
fn run(command: &mut Command) -> Result<(), Failure> {
    let output = command.output().map_err(failed)?;
    if output.status.success() {
        return Ok(());
    }
    Err(failed(String::from_utf8_lossy(&output.stderr).trim()))
}

/// Register `exe --home <home>` to start at login.
pub fn install(exe: &Path, home: &Path) -> Result<(), Failure> {
    let home = std::path::absolute(home).map_err(failed)?;
    #[cfg(target_os = "macos")]
    {
        let path = launch_agent()?;
        std::fs::create_dir_all(path.parent().expect("LaunchAgents")).map_err(failed)?;
        // launchd opens the log but does not create its directory.
        std::fs::create_dir_all(home.join("logs")).map_err(failed)?;
        std::fs::write(path, plist(exe, &home)).map_err(failed)
    }
    #[cfg(target_os = "linux")]
    {
        let dir = config_dir()?.join("systemd").join("user");
        std::fs::create_dir_all(&dir).map_err(failed)?;
        std::fs::write(dir.join(UNIT), unit(exe, &home)).map_err(failed)?;
        let systemd = run(Command::new("systemctl").args(["--user", "daemon-reload"]))
            .and_then(|_| run(Command::new("systemctl").args(["--user", "enable", UNIT])));
        if systemd.is_ok() {
            return Ok(());
        }
        // Containers and some SSH sessions have no user systemd.
        let _ = std::fs::remove_file(dir.join(UNIT));
        let path = xdg_entry()?;
        std::fs::create_dir_all(path.parent().expect("autostart")).map_err(failed)?;
        std::fs::write(path, desktop_entry(exe, &home)).map_err(failed)
    }
    #[cfg(windows)]
    {
        let line = format!("\"{}\" --home \"{}\" start", exe.display(), home.display());
        run(Command::new("reg").args([
            "add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &line, "/f",
        ]))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = (exe, home);
        Err(Failure::new(
            "UNSUPPORTED",
            "autostart is not supported here",
        ))
    }
}

/// Remove the registration.
pub fn uninstall() -> Result<(), Failure> {
    #[cfg(target_os = "macos")]
    {
        remove(&launch_agent()?)
    }
    #[cfg(target_os = "linux")]
    {
        let unit = config_dir()?.join("systemd").join("user").join(UNIT);
        if unit.exists() {
            // No `--now`: a running daemon keeps running.
            let _ = run(Command::new("systemctl").args(["--user", "disable", UNIT]));
            remove(&unit)?;
            let _ = run(Command::new("systemctl").args(["--user", "daemon-reload"]));
        }
        remove(&xdg_entry()?)
    }
    #[cfg(windows)]
    {
        if status()? {
            run(Command::new("reg").args(["delete", RUN_KEY, "/v", RUN_VALUE, "/f"]))?;
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        Err(Failure::new(
            "UNSUPPORTED",
            "autostart is not supported here",
        ))
    }
}

/// Whether the registration exists.
pub fn status() -> Result<bool, Failure> {
    #[cfg(target_os = "macos")]
    {
        Ok(launch_agent()?.exists())
    }
    #[cfg(target_os = "linux")]
    {
        Ok(config_dir()?
            .join("systemd")
            .join("user")
            .join(UNIT)
            .exists()
            || xdg_entry()?.exists())
    }
    #[cfg(windows)]
    {
        Ok(Command::new("reg")
            .args(["query", RUN_KEY, "/v", RUN_VALUE])
            .output()
            .map_err(failed)?
            .status
            .success())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        Err(Failure::new(
            "UNSUPPORTED",
            "autostart is not supported here",
        ))
    }
}

#[cfg(unix)]
fn remove(path: &Path) -> Result<(), Failure> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(failed(e)),
        _ => Ok(()),
    }
}

#[cfg(target_os = "macos")]
fn launch_agent() -> Result<PathBuf, Failure> {
    Ok(user_home()?
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

#[cfg(target_os = "linux")]
fn config_dir() -> Result<PathBuf, Failure> {
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => Ok(PathBuf::from(dir)),
        _ => Ok(user_home()?.join(".config")),
    }
}

#[cfg(target_os = "linux")]
fn xdg_entry() -> Result<PathBuf, Failure> {
    Ok(config_dir()?.join("autostart").join("dmsg.desktop"))
}

/// The daemon in the foreground under launchd; its diagnostics go to
/// `<home>/logs/daemon.log`, like `dmsg start`.
#[cfg(any(target_os = "macos", test))]
fn plist(exe: &Path, home: &Path) -> String {
    let xml = |p: &Path| {
        p.to_string_lossy()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let log = home.join("logs").join("daemon.log");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>--home</string>
    <string>{}</string>
    <string>daemon</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>StandardErrorPath</key>
  <string>{}</string>
</dict>
</plist>
"#,
        xml(exe),
        xml(home),
        xml(&log)
    )
}

#[cfg(any(target_os = "linux", test))]
fn quoted(path: &Path) -> String {
    let text = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("\"{text}\"")
}

#[cfg(any(target_os = "linux", test))]
fn unit(exe: &Path, home: &Path) -> String {
    format!(
        "[Unit]\nDescription=dMsg daemon\n\n[Service]\nType=simple\nExecStart={} --home {} daemon\nRestart=no\n\n[Install]\nWantedBy=default.target\n",
        quoted(exe).replace('%', "%%"),
        quoted(home).replace('%', "%%")
    )
}

#[cfg(any(target_os = "linux", test))]
fn desktop_entry(exe: &Path, home: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=dMsg\nComment=Start the dMsg daemon\nExec={} --home {} daemon\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        quoted(exe),
        quoted(home)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrations_quote_their_paths() {
        let exe = Path::new("/Users/me/dMsg & co/dmsg");
        let home = Path::new("/Users/me/.dmsg \"prod\"");
        let plist = plist(exe, home);
        assert!(plist.contains("<string>/Users/me/dMsg &amp; co/dmsg</string>"));
        assert!(plist.contains("<string>/Users/me/.dmsg &quot;prod&quot;</string>"));
        assert!(plist.contains("&quot;prod&quot;/logs/daemon.log</string>"));
        assert!(plist.contains("<string>daemon</string>"));
        let unit = unit(Path::new("/opt/d m/dmsg"), Path::new("/home/me/100%"));
        assert!(unit.contains("ExecStart=\"/opt/d m/dmsg\" --home \"/home/me/100%%\" daemon"));
        let entry = desktop_entry(Path::new("/opt/d m/dmsg"), Path::new("/h"));
        assert!(entry.contains("Exec=\"/opt/d m/dmsg\" --home \"/h\" daemon"));
    }
}
