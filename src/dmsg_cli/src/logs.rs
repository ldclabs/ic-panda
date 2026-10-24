//! The daemon's run log (plan §13): one file per UTC day,
//! `<home>/logs/dmsg-YYYY-MM-DD.log`, kept for 30 days. A line is a UTC
//! timestamp, an event and `key=value` fields that are names from fixed
//! lists, error codes, counts and durations; never parameters, content,
//! resource file names, paths, tokens or error messages, which may carry any of
//! those. `dmsg logs` prints the newest lines.
//!
//! The daemon's stderr goes to `logs/stderr.log`, which every `dmsg start`
//! rewrites: startup failures keep only their code, and panics after the
//! log opens keep only a source filename and line, never their payload.

use dmsg_client::diagnostic::{Diagnostic, TARGET};
use dmsg_types::{Error, Result};
use std::fmt::Display;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Days a log file is kept.
const KEEP_DAYS: u64 = 30;
const DAY_MS: u64 = 86_400_000;

struct Log {
    dir: PathBuf,
    /// The day the open file is for, and the file.
    file: Mutex<Option<(u64, fs::File)>>,
}

static LOG: OnceLock<Log> = OnceLock::new();
static DIAGNOSTICS: Diagnostics = Diagnostics;

/// Ignore dependency debug logs and validate every diagnostic again before
/// writing it. In particular, never forward arbitrary `record.args()`.
struct Diagnostics;

impl log::Log for Diagnostics {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target() == TARGET && metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata())
            && let Some(diagnostic) = Diagnostic::decode(&record.args().to_string())
        {
            write_diagnostic("failure", &[], diagnostic);
        }
    }

    fn flush(&self) {}
}

fn write_diagnostic(event: &str, fields: &[(&str, &dyn Display)], diagnostic: Diagnostic) {
    let mut fields = fields.to_vec();
    fields.extend([
        ("source", &diagnostic.source as &dyn Display),
        ("reason", &diagnostic.reason as &dyn Display),
    ]);
    if let Some(status) = &diagnostic.http_status {
        fields.push(("http_status", status));
    }
    write(event, &fields);
}

/// Log an application failure using categories, never its Display/Debug text.
pub fn failure(event: &str, fields: &[(&str, &dyn Display)], error: &Error) {
    let code = dmsg_client::error_code(error);
    let mut fields = fields.to_vec();
    fields.push(("outcome", &code));
    write_diagnostic(event, &fields, Diagnostic::error(error));
}

/// An outcome and its safe cause, alongside caller-supplied fixed fields.
pub fn result<T>(event: &str, fields: &[(&str, &dyn Display)], result: &Result<T>) {
    match result {
        Err(error) => failure(event, fields, error),
        Ok(_) => {
            let mut fields = fields.to_vec();
            fields.push(("outcome", &"ok"));
            write(event, &fields);
        }
    }
}

/// The log directory of a home.
pub fn dir(home: &Path) -> PathBuf {
    home.join("logs")
}

/// Start the run log of the daemon serving `home`, and log panics there
/// by their location only.
///
/// # Errors
/// When the directory cannot be created.
pub fn open(home: &Path) -> std::io::Result<()> {
    let dir = dir(home);
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    let _ = LOG.set(Log {
        dir,
        file: Mutex::new(None),
    });
    if log::set_logger(&DIAGNOSTICS).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
    std::panic::set_hook(Box::new(move |info| {
        let at = info
            .location()
            .map(|l| {
                format!(
                    "{}:{}",
                    l.file().rsplit(['/', '\\']).next().unwrap_or("unknown"),
                    l.line()
                )
            })
            .unwrap_or_default();
        write("panic", &[("at", &at)]);
        // The default hook prints the panic payload to stderr.log; it may
        // contain content, paths or credentials from a failed assertion.
    }));
    Ok(())
}

/// Write one line; nothing happens outside the daemon. Callers pass only
/// values from fixed sets, codes and numbers.
pub fn write(event: &str, fields: &[(&str, &dyn Display)]) {
    let Some(log) = LOG.get() else {
        return;
    };
    let now = dmsg_client::now_ms();
    let mut line = format!("{} {}", timestamp(now), clean(event));
    for (key, value) in fields {
        line.push_str(&format!(" {key}={}", clean(&value.to_string())));
    }
    line.push('\n');
    let mut file = log.file.lock().unwrap_or_else(|e| e.into_inner());
    let day = now / DAY_MS;
    if file.as_ref().is_none_or(|(open, _)| *open != day) {
        prune(&log.dir, day);
        let mut options = fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        *file = options.open(log.dir.join(name(day))).ok().map(|f| (day, f));
    }
    if let Some((_, file)) = file.as_mut() {
        let _ = file.write_all(line.as_bytes());
    }
}

/// The newest `count` lines across the kept files, oldest first.
///
/// # Errors
/// When the directory cannot be read.
pub fn tail(home: &Path, count: usize) -> std::io::Result<Vec<String>> {
    let mut files = logs(&dir(home))?;
    files.sort();
    let mut lines: Vec<String> = Vec::new();
    for file in files.iter().rev() {
        if lines.len() >= count {
            break;
        }
        let text = fs::read_to_string(dir(home).join(file)).unwrap_or_default();
        let mut older: Vec<String> = text.lines().map(str::to_string).collect();
        let keep = older.len().saturating_sub(count - lines.len());
        older.drain(..keep);
        older.append(&mut lines);
        lines = older;
    }
    Ok(lines)
}

/// The day's file name.
fn name(day: u64) -> String {
    let (y, m, d) = civil(day);
    format!("dmsg-{y:04}-{m:02}-{d:02}.log")
}

/// The names of the log files in `dir`.
fn logs(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        // `dmsg-YYYY-MM-DD.log`: names sort as their days do.
        if name.len() == 19 && name.starts_with("dmsg-") && name.ends_with(".log") {
            names.push(name);
        }
    }
    Ok(names)
}

/// Remove the files older than the kept days.
fn prune(dir: &Path, today: u64) {
    let oldest = name(today.saturating_sub(KEEP_DAYS - 1));
    for file in logs(dir).unwrap_or_default() {
        if file < oldest {
            let _ = fs::remove_file(dir.join(file));
        }
    }
}

/// One token: whitespace and control characters become `_`.
fn clean(text: &str) -> String {
    text.chars()
        .map(|c| match c.is_whitespace() || c.is_control() {
            true => '_',
            false => c,
        })
        .collect()
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub(crate) fn timestamp(ms: u64) -> String {
    let (y, m, d) = civil(ms / DAY_MS);
    let of_day = ms % DAY_MS;
    let (h, min, s, milli) = (
        of_day / 3_600_000,
        of_day / 60_000 % 60,
        of_day / 1000 % 60,
        of_day % 1000,
    );
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}.{milli:03}Z")
}

/// The proleptic Gregorian date of a day since 1970-01-01.
fn civil(day: u64) -> (u64, u64, u64) {
    let z = day + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + u64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_days_and_keeps_thirty() {
        assert_eq!(name(0), "dmsg-1970-01-01.log");
        assert_eq!(name(20_009), "dmsg-2024-10-13.log");
        assert_eq!(name(11_016), "dmsg-2000-02-29.log");
        assert_eq!(timestamp(1_760_000_000_123), "2025-10-09T08:53:20.123Z");
        let dir = std::env::temp_dir().join(format!("dmsg-logs-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for day in [20_000, 20_009, 20_010, 20_038] {
            fs::write(dir.join(name(day)), format!("line {day}\n")).unwrap();
        }
        fs::write(dir.join("stderr.log"), "kept\n").unwrap();
        prune(&dir, 20_038);
        let mut left = logs(&dir).unwrap();
        left.sort();
        assert_eq!(left, [name(20_009), name(20_010), name(20_038)]);
        assert!(dir.join("stderr.log").exists());
        let home = dir
            .parent()
            .unwrap()
            .join(format!("dmsg-home-{}", std::process::id()));
        fs::create_dir_all(home.join("logs")).unwrap();
        fs::write(home.join("logs").join(name(1)), "a\nb\n").unwrap();
        fs::write(home.join("logs").join(name(2)), "c\n").unwrap();
        assert_eq!(tail(&home, 2).unwrap(), ["b", "c"]);
        assert_eq!(tail(&home, 9).unwrap(), ["a", "b", "c"]);
        assert_eq!(clean("a b\nc"), "a_b_c");
        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(home).unwrap();
    }
}
