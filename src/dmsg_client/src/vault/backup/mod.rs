//! Portable password-encrypted vault backups. Only the foreground owner hosts
//! these operations. Export requires a separate, single-use high-risk approval.
mod content;
mod restore;
#[cfg(test)]
mod tests;
mod wire;

use crate::{
    Client,
    approval::{Action, HIGH_RISK_TTL_MS, State},
    content::VAULT,
    crypto::random_key,
    hold::Hold,
    now_ms,
    upload::random_hex,
    vault::reading::Session,
};
use content::{Body, Entry, FileMeta, Meta};
use dmsg_types::{
    Error, MINUTE, Result, ensure, ensure_valid,
    user::{Capability, VaultWriteState},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{Read, Seek, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

pub use restore::{commit, inspect, receipt};

const TTL: u64 = 10 * MINUTE;

#[derive(Clone)]
struct Guard {
    live: Arc<AtomicBool>,
    progress: Arc<AtomicUsize>,
    items: Arc<AtomicUsize>,
    until: u64,
}

impl Guard {
    fn new(until: u64) -> Self {
        Self {
            live: Arc::new(AtomicBool::new(true)),
            progress: Arc::new(AtomicUsize::new(0)),
            items: Arc::new(AtomicUsize::new(0)),
            until,
        }
    }

    fn check(&self) -> Result<()> {
        ensure(self.live.load(Ordering::Acquire), Error::Expired)?;
        ensure(now_ms() < self.until, Error::Expired)
    }
}

pub(crate) struct Operation {
    id: String,
    account: String,
    session: Session,
    guard: Guard,
    state: Stage,
}

enum Stage {
    Working,
    Inventory(Inventory),
    Requested {
        selection: Selection,
        password: Zeroizing<String>,
        request: String,
    },
    Ready(File, Value, Arc<AtomicBool>),
    Reviewed(restore::Reviewed),
}

impl Drop for Operation {
    fn drop(&mut self) {
        self.guard.live.store(false, Ordering::Release);
    }
}

#[derive(Serialize, serde::Deserialize, PartialEq, Eq)]
pub(crate) struct Permissions {
    read: bool,
    sign: bool,
    ready: bool,
    generation: u64,
}

impl Permissions {
    pub(crate) fn observed(state: &crate::account::Security, device: &dmsg_types::Hash) -> Self {
        let live = state.device(device).filter(|d| d.revoked_at.is_none());
        Self {
            read: live.is_some_and(|d| d.input.capabilities.contains(&Capability::VaultUnlock)),
            sign: live.is_some_and(|d| d.input.capabilities.contains(&Capability::ContentSign)),
            ready: state.snapshot.vault_write_state == VaultWriteState::Ready,
            generation: state.snapshot.content_root_generation,
        }
    }
}

pub(crate) const PERMISSIONS: &str = "vault-backup-permissions";

fn access(c: &Client, write: bool) -> Result<()> {
    let roots = c.roots()?;
    let account = c.account()?;
    let access = if let Some(state) = &c.security {
        ensure(
            state.snapshot.account_id == account.id,
            Error::DeviceNotApproved,
        )?;
        Some(Permissions::observed(state, &c.workspace.meta.device_id))
    } else {
        c.workspace.private(c.keys()?, PERMISSIONS)?
    };
    if let Some(access) = access {
        ensure(access.read, Error::DeviceNotApproved)?;
        if write {
            ensure(access.sign, Error::Forbidden)?;
            ensure(
                access.ready && roots.generation >= access.generation,
                Error::RekeyRequired,
            )?;
        }
    } else if write {
        // Older workspaces may only have an unlocked content root. It proves
        // read access, but not permission to sign newly imported versions.
        return Err(Error::PolicyStale);
    }
    Ok(())
}

/// Start one foreground workflow. Its handle is available before long-running I/O.
pub fn begin(c: &mut Client, write: bool) -> Result<String> {
    access(c, write)?;
    let id = random_hex();
    let guard = Guard::new(now_ms() + TTL);
    c.vault_backup = Some(Operation {
        id: id.clone(),
        account: c.account()?.id.to_string(),
        session: Session::start(c)?,
        guard: guard.clone(),
        state: Stage::Working,
    });
    Ok(id)
}

fn active<'a>(c: &'a mut Client, id: &str, write: bool) -> Result<&'a mut Operation> {
    if c.vault_backup
        .as_ref()
        .is_some_and(|op| op.id == id && now_ms() >= op.guard.until)
    {
        c.vault_backup = None;
        return Err(Error::Expired);
    }
    access(c, write)?;
    let op = c
        .vault_backup
        .as_ref()
        .filter(|op| op.id == id)
        .ok_or(Error::Expired)?;
    op.session.check(c)?;
    ensure(op.account == c.account()?.id.to_string(), Error::Expired)?;
    op.guard.check()?;
    Ok(c.vault_backup.as_mut().unwrap())
}

// Replacing an input invalidates the previous preview and its in-flight worker.
fn prepare(c: &mut Client, id: &str, write: bool) -> Result<Guard> {
    let op = active(c, id, write)?;
    op.guard.live.store(false, Ordering::Release);
    op.guard = Guard::new(now_ms() + TTL);
    op.state = Stage::Working;
    Ok(op.guard.clone())
}

/// Cancel exactly this operation, including any in-flight worker.
pub fn cancel(c: &mut Client, id: &str) {
    if c.vault_backup.as_ref().is_some_and(|op| op.id == id) {
        c.vault_backup = None;
    }
}

/// Progress counts plaintext attachment bytes handled by this operation.
pub fn progress(c: &mut Client, id: &str) -> Result<Value> {
    let original = receipt(c, id)?;
    if !original.is_null() {
        return Ok(json!({"state":"committed","receipt":original}));
    }
    if c.vault_backup.as_ref().is_none_or(|op| op.id != id) {
        return Ok(json!({"state":"uncommitted"}));
    }
    let op = active(c, id, false)?;
    Ok(
        json!({"state":"active","bytes":op.guard.progress.load(Ordering::Acquire),"items":op.guard.items.load(Ordering::Acquire)}),
    )
}

/// Generate a portable random password with 128 bits of entropy.
pub fn generate_password() -> String {
    crate::codec::b64(&Zeroizing::new(random_key())[..16])
}

#[derive(Clone, Serialize)]
struct LocalFile {
    version: String,
    meta: FileMeta,
    available: bool,
}

fn local_file(c: &mut Client, id: &str) -> Result<LocalFile> {
    let (f, version_id, version) = c.pick_version(id, None)?;
    ensure(!c.catalog()?.file_trashed(&f), Error::NotFound)?;
    let digest = crate::upload::hex_id(&version.sha256)?;
    Ok(LocalFile {
        version: version_id,
        available: version.local && version.size <= crate::files::MAX_FILE,
        meta: FileMeta {
            name: f.name,
            mime: version.mime.unwrap_or(f.mime),
            size: version.size,
            sha256: (*digest).into(),
        },
    })
}

#[derive(Clone, Serialize)]
struct Row {
    id: String,
    revision: String,
    title: String,
    kind: String,
    files: Vec<String>,
    synced: bool,
}

#[derive(Default, Serialize)]
struct Inventory {
    at: u64,
    items: Vec<Row>,
    files: BTreeMap<String, Option<LocalFile>>,
    managed: usize,
    trash: usize,
    conflicts: usize,
}

struct Selection {
    rows: Vec<Row>,
    files: BTreeMap<String, LocalFile>,
    include: bool,
    at: u64,
}

/// Inspect the unfiltered local inventory. No field values cross the UI bridge.
pub async fn inventory<H: Hold>(h: &H, id: &str) -> Result<Value> {
    let (guard, heads, conflicts, account, at) = h
        .with(|c| {
            let guard = prepare(c, id, false)?;
            Ok((
                guard,
                c.workspace.store.heads(VAULT)?,
                super::reading::conflict_ids(c)?,
                c.account()?.id.to_string(),
                now_ms(),
            ))
        })
        .await?;
    let mut list = Inventory {
        at,
        ..Inventory::default()
    };
    for (item, rev) in heads {
        h.with(|c| {
            guard.check()?;
            active(c, id, false)?;
            let e = c.vault_version(&item, &rev)?;
            if e.item.reserved() {
                list.managed += 1;
            } else if e.deleted {
                list.trash += 1;
            } else {
                list.conflicts += usize::from(conflicts.contains(&item));
                for f in &e.item.files {
                    if !list.files.contains_key(f) {
                        let file = match local_file(c, f) {
                            Ok(f) => Some(f),
                            Err(Error::NotFound) => None,
                            Err(e) => return Err(e),
                        };
                        list.files.insert(f.clone(), file);
                    }
                }
                list.items.push(Row {
                    id: e.id,
                    revision: e.revision,
                    title: e.item.title,
                    kind: e.item.kind,
                    files: e.item.files,
                    synced: e.synced,
                });
            }
            Ok(())
        })
        .await?;
    }
    let files: BTreeMap<_, _> = list
        .files
        .iter()
        .map(|(id, f)| {
            (
                id,
                f.as_ref()
                    .map(|f| json!({"available":f.available,"meta":{"size":f.meta.size}})),
            )
        })
        .collect();
    let view = json!({"id":id,"account":account,"at":at,"inventory":{"items":list.items,"files":files,"managed":list.managed,"trash":list.trash,"conflicts":list.conflicts}});
    h.with(|c| {
        guard.check()?;
        active(c, id, false)?.state = Stage::Inventory(list);
        Ok(view)
    })
    .await
}

/// Bind a selection and its password to the approval window.
pub fn request(
    c: &mut Client,
    id: &str,
    picks: &[String],
    include: bool,
    password: &str,
    confirmation: &str,
) -> Result<Value> {
    let password = wire::password(password, true)?;
    ensure_valid(
        password == wire::password(confirmation, true)?,
        "backup password confirmation",
    )?;
    let op = active(c, id, false)?;
    let Stage::Inventory(list) = &op.state else {
        return Err(Error::Expired);
    };
    let picks: BTreeSet<_> = picks.iter().collect();
    ensure_valid(
        !picks.is_empty() && picks.len() <= content::ITEMS,
        "backup selection",
    )?;
    let rows: Vec<_> = list
        .items
        .iter()
        .filter(|r| picks.contains(&r.id))
        .cloned()
        .collect();
    ensure(rows.len() == picks.len(), Error::VersionConflict)?;
    let mut files = BTreeMap::new();
    let mut omitted = 0;
    for f in rows.iter().flat_map(|r| &r.files) {
        if include {
            let info = list
                .files
                .get(f)
                .and_then(Option::as_ref)
                .filter(|f| f.available)
                .ok_or(Error::NotFound)?;
            files.insert(f.clone(), info.clone());
        } else {
            omitted += 1;
        }
    }
    ensure(
        files.len() <= content::FILES
            && files.values().map(|f| f.meta.size).sum::<u64>() <= content::FILE_BYTES,
        Error::QuotaExceeded,
    )?;
    let at = list.at;
    let now = now_ms();
    let r = c.approval_create_at(
        "gui",
        Action::VaultBackup {
            plan: id.into(),
            items: rows.len(),
            files: files.len(),
            omitted,
            wallet: rows.iter().any(|r| r.kind == "wallet"),
        },
        now,
        now + HIGH_RISK_TTL_MS,
    )?;
    let view = c.approval_view(&r)?;
    active(c, id, false)?.state = Stage::Requested {
        selection: Selection {
            rows,
            files,
            include,
            at,
        },
        password,
        request: r.id,
    };
    Ok(view)
}

fn same_file(c: &mut Client, id: &str, expected: &LocalFile) -> Result<()> {
    let f = local_file(c, id)?;
    ensure(
        f.available && f.version == expected.version && f.meta == expected.meta,
        Error::VersionConflict,
    )
}

/// Consume a completed approval, encrypt and independently read back the archive.
pub async fn export<H: Hold>(h: &H, id: &str, approval: &str) -> Result<Value> {
    let (selection, password, guard, environment, account, temp) = h
        .with(|c| {
            let op = active(c, id, false)?;
            ensure(
                matches!(&op.state,Stage::Requested{request,..} if request==approval),
                Error::Forbidden,
            )?;
            let mut r = c.approval_record(approval)?;
            ensure(
                matches!(&r.action,Action::VaultBackup{plan,..} if plan==id)
                    && r.state == State::Done
                    && r.delivery_report.is_none(),
                Error::Forbidden,
            )?;
            let now = now_ms();
            ensure(now < r.expires_at, Error::Expired)?;
            r.delivery_report = Some(json!({"startedAt":now}));
            c.save_approval(&r)?;
            let dir = c.slot_dir(&c.slot);
            let temp = tempfile::tempfile_in(dir).map_err(wire::file_io)?;
            let op = active(c, id, false)?;
            let Stage::Requested {
                selection,
                password,
                ..
            } = std::mem::replace(&mut op.state, Stage::Working)
            else {
                return Err(Error::Expired);
            };
            Ok((
                selection,
                password,
                op.guard.clone(),
                c.workspace.meta.environment.clone(),
                c.account()?.id.to_string(),
                temp,
            ))
        })
        .await?;
    let mut entries = Vec::new();
    let mapping: BTreeMap<_, _> = selection
        .files
        .keys()
        .enumerate()
        .map(|(n, id)| (id.clone(), n))
        .collect();
    let mut metadata = 0;
    for row in &selection.rows {
        let entry = h
            .with(|c| {
                active(c, id, false)?;
                let e = c.vault_show(&row.id)?;
                ensure(
                    !e.deleted && !e.item.reserved() && e.revision == row.revision,
                    Error::VersionConflict,
                )?;
                let refs = e
                    .item
                    .files
                    .iter()
                    .map(|f| mapping.get(f).copied())
                    .collect();
                Ok(Entry::new(e, refs))
            })
            .await?;
        metadata += Zeroizing::new(crate::codec::canonical(&entry)).len();
        ensure(metadata <= 16 << 20, Error::QuotaExceeded)?;
        entries.push(entry);
        guard.items.fetch_add(1, Ordering::Release);
    }
    for (f, info) in &selection.files {
        h.with(|c| {
            active(c, id, false)?;
            same_file(c, f, info)
        })
        .await?;
    }
    let omitted = entries
        .iter()
        .flat_map(|e| &e.attachment_refs)
        .filter(|f| f.is_none())
        .count();
    let result = json!({"items":entries.len(),"files":selection.files.len(),"omitted":omitted});
    let body = Body {
        meta: Meta {
            archive_id: <[u8; 16]>::try_from(&random_key()[..16]).unwrap().into(),
            created_at: selection.at,
            environment,
            account,
            with_attachments: selection.include,
            item_count: entries.len(),
            file_count: selection.files.len(),
            omitted_refs: omitted,
        },
        entries,
        files: selection.files.values().map(|f| f.meta.clone()).collect(),
    };
    let (tx, mut rx) = mpsc::channel(1);
    let exporting = guard.clone();
    let worker = tokio::task::spawn_blocking(move || {
        let mut file = body.write(
            temp,
            &password,
            |_, _| rx.blocking_recv().ok_or(Error::Expired),
            || guard.check(),
        )?;
        file.rewind().map_err(wire::file_io)?;
        let (verified, _) = content::read(&file, &password, |_| Ok(()), || guard.check())?;
        ensure(verified == body, Error::IntegrityFailed)?;
        file.rewind().map_err(wire::file_io)?;
        Ok::<_, Error>(file)
    });
    for (f, info) in &selection.files {
        for n in 0..info.meta.size.div_ceil(crate::files::CHUNK as u64) {
            let chunk = h
                .with(|c| {
                    let guard = active(c, id, false)?.guard.clone();
                    let part = c.files_chunk(f, &info.version, n)?;
                    guard.progress.fetch_add(part.len(), Ordering::Release);
                    Ok(part)
                })
                .await?;
            if tx.send(chunk).await.is_err() {
                break;
            }
        }
    }
    drop(tx);
    let file = worker.await.map_err(|_| Error::Expired)??;
    h.with(|c| {
        exporting.check()?;
        active(c, id, false)?.state =
            Stage::Ready(file, result.clone(), Arc::new(AtomicBool::new(false)));
        Ok(result)
    })
    .await
}

/// Save only the verified ciphertext to a host-selected destination. Publication
/// is atomic; a failed save leaves the verified file available for retry.
pub async fn save<H: Hold>(h: &H, id: &str, path: &Path) -> Result<Value> {
    struct Saving(Arc<AtomicBool>);
    impl Drop for Saving {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }
    let (mut source, guard, summary, saving) = h
        .with(|c| {
            let op = active(c, id, false)?;
            let Stage::Ready(file, summary, saving) = &op.state else {
                return Err(Error::Expired);
            };
            let source = file.try_clone().map_err(wire::file_io)?;
            ensure(!saving.swap(true, Ordering::AcqRel), Error::Pending)?;
            Ok((
                source,
                op.guard.clone(),
                summary.clone(),
                Saving(saving.clone()),
            ))
        })
        .await?;
    let parent = path
        .parent()
        .ok_or_else(|| dmsg_types::invalid("backup destination"))?
        .to_owned();
    let (dest, _saving, guard) = tokio::task::spawn_blocking(move || {
        let mut dest = tempfile::NamedTempFile::new_in(parent).map_err(wire::file_io)?;
        source.rewind().map_err(wire::file_io)?;
        let mut buf = vec![0; crate::files::CHUNK];
        loop {
            guard.check()?;
            let n = source.read(&mut buf).map_err(wire::file_io)?;
            if n == 0 {
                break;
            }
            dest.write_all(&buf[..n]).map_err(wire::file_io)?;
        }
        dest.as_file().sync_all().map_err(wire::file_io)?;
        Ok::<_, Error>((dest, saving, guard))
    })
    .await
    .map_err(|_| Error::Expired)??;
    h.with(|c| {
        active(c, id, false)?;
        guard.check()?;
        dest.persist(path).map_err(|e| wire::file_io(e.error))?;
        cancel(c, id);
        Ok(summary)
    })
    .await
}
