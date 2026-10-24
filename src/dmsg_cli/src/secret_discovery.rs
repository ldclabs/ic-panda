//! Explicit-root scanning. Capability directories prevent ancestor/symlink races from escaping the root.
use crate::{
    daemon::Daemon,
    params::{parse, source, text},
};
use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, Metadata, OpenOptions},
};
use dmsg_client::{
    secrets::discovery::{self, ImportWork, Report},
    vault::reading::Session,
};
use dmsg_types::{Error, Result, ensure};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use zeroize::Zeroizing;
const MAX_FILES: u64 = 10000;
const MAX_FILE: u64 = 1024 * 1024;
const MAX_BYTES: u64 = 100 * 1024 * 1024;
const SKIP: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".venv",
    "venv",
    "vendor",
    ".dmsg-secrets",
];

fn identity(metadata: &Metadata) -> String {
    format!("{}:{}", metadata.dev(), metadata.ino())
}

fn fail() -> Error {
    Error::InvalidInput("selected scan directory".into())
}

fn open_root(path: &Path) -> Result<(PathBuf, Dir, String)> {
    ensure(path.is_absolute(), fail())?;
    let path = std::fs::canonicalize(path).map_err(|_| fail())?;
    let root = Dir::open_ambient_dir(&path, ambient_authority()).map_err(|_| fail())?;
    let metadata = root.dir_metadata().map_err(|_| fail())?;
    Ok((path, root, identity(&metadata)))
}

fn open_file(dir: &Dir, name: &Path) -> Result<cap_std::fs::File> {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY);
    }
    dir.open_with(name, &options)
        .map_err(|_| Error::InvalidInput("scan entry changed or inaccessible".into()))
}

fn inspect_file(
    dir: &Dir,
    name: &Path,
    relative: &str,
    remaining: u64,
    consumed: &mut u64,
    cancel: &AtomicBool,
) -> Result<Option<discovery::Candidate>> {
    ensure(!cancel.load(Ordering::Acquire), Error::Expired)?;
    let mut file = open_file(dir, name)?;
    let before = file.metadata().map_err(|_| fail())?;
    ensure(
        before.is_file() && before.len() <= MAX_FILE && before.len() < remaining,
        Error::QuotaExceeded,
    )?;
    let mut bytes = Zeroizing::new(Vec::new());
    let mut prefix = Zeroizing::new([0u8; 64]);
    let count = file.read(&mut prefix[..]).map_err(|_| fail())?;
    *consumed += count as u64;
    bytes.extend_from_slice(&prefix[..count]);
    let base = name.to_string_lossy();
    let candidate = base == ".env.refs"
        || base == ".env"
        || (base.starts_with(".env.") && !base.ends_with(".refs"))
        || bytes.starts_with(b"-----BEGIN OPENSSH PRIVATE KEY-----")
        || bytes.starts_with(b"-----BEGIN RSA PRIVATE KEY-----")
        || bytes.starts_with(b"-----BEGIN EC PRIVATE KEY-----");
    if !candidate {
        return Ok(None);
    }
    let mut buffer = Zeroizing::new(vec![0u8; 65536]);
    while bytes.len() as u64 <= before.len() {
        ensure(!cancel.load(Ordering::Acquire), Error::Expired)?;
        let size = (before.len() + 1 - bytes.len() as u64).min(buffer.len() as u64) as usize;
        let count = file.read(&mut buffer[..size]).map_err(|_| fail())?;
        if count == 0 {
            break;
        }
        *consumed += count as u64;
        bytes.extend_from_slice(&buffer[..count]);
    }
    let after = file.metadata().map_err(|_| fail())?;
    ensure(!cancel.load(Ordering::Acquire), Error::Expired)?;
    ensure(
        identity(&before) == identity(&after)
            && before.len() == after.len()
            && before.modified().ok() == after.modified().ok()
            && bytes.len() as u64 == before.len(),
        Error::VersionConflict,
    )?;
    Ok(discovery::inspect(relative, identity(&before), &bytes))
}

fn scan(root: Dir, cancel: Arc<AtomicBool>) -> Result<Report> {
    let device = root.dir_metadata().map_err(|_| fail())?.dev();
    let mut queue = vec![(root, String::new(), 0usize)];
    let mut report = Report {
        complete: true,
        ..Default::default()
    };
    let mut candidates_size = 0usize;
    while let Some((dir, path, depth)) = queue.pop() {
        ensure(!cancel.load(Ordering::Acquire), Error::Expired)?;
        if depth > 32 {
            report.complete = false;
            report.warnings.push("directory_depth_limit".into());
            continue;
        }
        let entries = match dir.entries() {
            Ok(entries) => entries,
            Err(_) => {
                report.complete = false;
                report.skipped += 1;
                continue;
            }
        };
        for entry in entries {
            ensure(!cancel.load(Ordering::Acquire), Error::Expired)?;
            report.files += 1;
            if report.files > MAX_FILES
                || report.bytes >= MAX_BYTES
                || report.candidates.len() >= 256
                || candidates_size >= 900 * 1024
            {
                report.complete = false;
                report.warnings.push(format!("scan_limit_at:{path}"));
                return Ok(report);
            }
            let Ok(entry) = entry else {
                report.complete = false;
                report.skipped += 1;
                continue;
            };
            let name = entry.file_name();
            let Some(name_text) = name.to_str() else {
                report.skipped += 1;
                continue;
            };
            let relative = if path.is_empty() {
                name_text.into()
            } else {
                format!("{path}/{name_text}")
            };
            let Ok(meta) = dir.symlink_metadata(&name) else {
                report.complete = false;
                report.skipped += 1;
                continue;
            };
            if meta.file_type().is_symlink() || meta.dev() != device {
                report.skipped += 1;
                continue;
            }
            if meta.is_dir() {
                if SKIP.contains(&name_text) {
                    report.skipped += 1;
                    continue;
                }
                match dir.open_dir_nofollow(&name) {
                    Ok(child) => queue.push((child, relative, depth + 1)),
                    Err(_) => {
                        report.complete = false;
                        report.skipped += 1;
                    }
                }
            } else if meta.is_file() {
                if meta.len() > MAX_FILE {
                    report.skipped += 1;
                    report.complete = false;
                    continue;
                }
                match inspect_file(
                    &dir,
                    Path::new(&name),
                    &relative,
                    MAX_BYTES - report.bytes,
                    &mut report.bytes,
                    &cancel,
                ) {
                    Ok(candidate) => {
                        if let Some(candidate) = candidate {
                            candidates_size += serde_json::to_vec(&candidate)
                                .map_err(|_| Error::IntegrityFailed)?
                                .len();
                            if candidates_size > 900 * 1024 {
                                report.complete = false;
                                report.warnings.push("report_limit".into());
                                return Ok(report);
                            }
                            report.candidates.push(candidate);
                        }
                    }
                    Err(Error::Expired) => return Err(Error::Expired),
                    Err(Error::QuotaExceeded) => {
                        report.complete = false;
                        report.warnings.push("scan_byte_limit".into());
                        return Ok(report);
                    }
                    Err(_) => {
                        report.complete = false;
                        report.skipped += 1;
                    }
                }
            } else {
                report.skipped += 1;
            }
        }
    }
    Ok(report)
}

type SelectedImport = (Vec<dmsg_client::vault::Item>, Vec<Vec<(String, String)>>);

fn read_selected(work: &ImportWork, cancel: &AtomicBool) -> Result<SelectedImport> {
    let (_, root, stamp) = open_root(Path::new(&work.plan.scan.root))?;
    ensure(
        stamp == work.plan.scan.root_identity,
        Error::VersionConflict,
    )?;
    let mut items = Vec::new();
    let mut references = Vec::new();
    for id in &work.plan.candidate_ids {
        ensure(!cancel.load(Ordering::Acquire), Error::Expired)?;
        let candidate = work
            .plan
            .scan
            .report
            .candidates
            .iter()
            .find(|c| &c.id == id)
            .ok_or(Error::NotFound)?;
        let parts: Vec<_> = candidate.relative_path.split('/').collect();
        ensure(
            parts
                .iter()
                .all(|p| !p.is_empty() && *p != "." && *p != ".."),
            Error::Forbidden,
        )?;
        let mut dir = root.try_clone().map_err(|_| fail())?;
        for part in &parts[..parts.len() - 1] {
            dir = dir
                .open_dir_nofollow(part)
                .map_err(|_| Error::VersionConflict)?;
        }
        let mut file = open_file(&dir, Path::new(parts.last().ok_or(Error::NotFound)?))?;
        let before = file.metadata().map_err(|_| fail())?;
        ensure(
            before.is_file()
                && identity(&before) == candidate.file_identity
                && before.len() == candidate.size
                && before.len() <= MAX_FILE,
            Error::VersionConflict,
        )?;
        let mut bytes = Zeroizing::new(Vec::new());
        file.by_ref()
            .take(MAX_FILE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| fail())?;
        let after = file.metadata().map_err(|_| fail())?;
        ensure(
            identity(&before) == identity(&after)
                && before.modified().ok() == after.modified().ok()
                && !cancel.load(Ordering::Acquire),
            Error::VersionConflict,
        )?;
        let (item, refs) = discovery::import_candidate(candidate, &bytes)?;
        items.push(item);
        references.push(refs);
    }
    Ok((items, references))
}

impl Daemon {
    pub(crate) async fn call_secret_discovery(&self, method: &str, p: &Value) -> Result<Value> {
        match method {
            "secrets/discovery/start" => {
                let path = PathBuf::from(text(p, "path")?);
                let session = {
                    let c = self.client().lock().await;
                    Session::start(&c)?
                };
                let (path, root, identity) = tokio::task::spawn_blocking(move || open_root(&path))
                    .await
                    .map_err(|_| fail())??;
                let (id, cancel) = {
                    let mut c = self.client().lock().await;
                    session.check(&c)?;
                    c.secret_discovery_begin(path.to_str().ok_or_else(fail)?.into(), identity)?
                };
                let client = self.client_handle();
                let task_id = id.clone();
                tokio::spawn(async move {
                    let result = tokio::task::spawn_blocking(move || scan(root, cancel)).await;
                    let mut c = client.lock().await;
                    if session.check(&c).is_ok() {
                        let report = match result {
                            Ok(Ok(report)) => report,
                            _ => Report {
                                complete: false,
                                warnings: vec!["scan_cancelled_or_failed".into()],
                                ..Default::default()
                            },
                        };
                        let _ = c.secret_discovery_finish(&task_id, report);
                    }
                });
                Ok(json!({"scan_id":id,"state":"running"}))
            }
            "secrets/discovery/cancel" => self
                .client()
                .lock()
                .await
                .secret_discovery_cancel(&text(p, "scan_id")?),
            "secrets/discovery/status" => self
                .client()
                .lock()
                .await
                .secret_discovery_status(&text(p, "scan_id")?),
            "secrets/discovery/import_request" => {
                self.client().lock().await.secret_discovery_request(
                    source(p)?,
                    &text(p, "scan_id")?,
                    parse(p, "candidate_ids")?,
                )
            }
            _ => Err(Error::NotFound),
        }
    }

    pub(crate) async fn import_discovered(&self, work: ImportWork) -> Result<Value> {
        let session = {
            let c = self.client().lock().await;
            c.secret_discovery_check_work(&work)?;
            Session::start(&c)?
        };
        let id = work.request.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let shared = self.client_handle();
        let observing = cancel.clone();
        let watcher = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                if session.check(&*shared.lock().await).is_err() {
                    observing.store(true, Ordering::Release);
                    break;
                }
            }
        });
        struct CancelWorker {
            cancel: Arc<AtomicBool>,
            watcher: tokio::task::JoinHandle<()>,
        }
        impl Drop for CancelWorker {
            fn drop(&mut self) {
                self.cancel.store(true, Ordering::Release);
                self.watcher.abort();
            }
        }
        let guard = CancelWorker {
            cancel: cancel.clone(),
            watcher,
        };
        let result = tokio::task::spawn_blocking(move || {
            let read = read_selected(&work, &cancel);
            (work, read)
        })
        .await;
        drop(guard);
        match result {
            Ok((work, Ok((items, refs)))) => self
                .client()
                .lock()
                .await
                .secret_discovery_commit(work, items, refs),
            Ok((_, Err(error))) => self.client().lock().await.secret_discovery_fail(&id, error),
            Err(_) => self
                .client()
                .lock()
                .await
                .secret_discovery_fail(&id, Error::Unavailable("discovery worker".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scan_never_follows_links_or_returns_values_and_import_rechecks_bytes() {
        let dir = dmsg_client::testing::TempDir::new();
        let root = dir.0.join("project");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join(".env"), "TOKEN='DISCOVERY_TEST_VALUE_ONLY'\n").unwrap();
        let outside = dir.0.join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join(".env"), "OUTSIDE=must_not_be_read\n").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
        let (_, opened, _) = open_root(&root).unwrap();
        let report = scan(opened, Arc::new(AtomicBool::new(false))).unwrap();
        assert_eq!(report.candidates.len(), 1);
        let text = serde_json::to_string(&report).unwrap();
        assert!(!text.contains("DISCOVERY_TEST_VALUE_ONLY"));
        assert!(!text.contains("must_not_be_read"));
        assert!(discovery::import_candidate(&report.candidates[0], b"TOKEN='changed'\n").is_err());
        let (_, opened, _) = open_root(&root).unwrap();
        assert!(scan(opened, Arc::new(AtomicBool::new(true))).is_err());
    }
}
