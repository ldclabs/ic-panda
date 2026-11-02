//! Complete, resumable baselines and ordinary fixed-range incremental pulls.
use super::*;
use crate::store::Store;

pub(super) const CURSOR: &str = "content-cursor-v1";
pub(super) const UPLOAD_BLOCKED: &str = "sync-upload-blocked";
const TARGET: &str = "content-pull-target";
const FULL: &str = "content-snapshot-job";

#[derive(Clone, Serialize, Deserialize)]
struct FullJob {
    request_id: Hash,
    export_id: String,
    attempted: bool,
    start: Option<Json>,
    cursor: u64,
    counts: BTreeMap<String, u64>,
    complete: bool,
    close_id: Hash,
    #[serde(default)]
    close_retry_at: u64,
}

#[derive(Clone, Serialize, Deserialize)]
enum Entry {
    Object(UploadPlan),
    Revision(Revision, bool),
    Retired(String),
}

struct Page {
    entries: Vec<(u64, Entry)>,
    counts: BTreeMap<String, u64>,
    cursor: u64,
    end: bool,
    verify_ms: u64,
}

/// Commit only after the page's bytes and revisions have been verified and applied.
pub(super) fn committed(store: &Store, through: u64, complete: bool) -> Result<()> {
    let previous = store.meta::<u64>(CURSOR)?.unwrap_or(0);
    ensure(through >= previous, Error::IntegrityFailed)?;
    store.set_meta(CURSOR, &through)?;
    if complete {
        if let Some(mut job) = store.meta::<FullJob>(FULL)? {
            ensure(
                job.start.as_ref().and_then(|s| s["through"].as_u64()) == Some(through),
                Error::IntegrityFailed,
            )?;
            job.complete = true;
            store.set_meta(FULL, &job)?;
            store.sync_clear_staged()?;
        }
        store.delete_meta(TARGET)?;
    } else if store.meta::<u64>(TARGET)? == Some(through) {
        store.delete_meta(TARGET)?;
    }
    Ok(())
}

pub(super) async fn run<H: Hold>(
    h: &H,
    stage: &mut Observer<'_>,
    report: &mut SyncReport,
) -> Result<()> {
    let (baseline, pending_snapshot) = h
        .with(|c| {
            Ok((
                c.workspace.store.meta::<u64>(CURSOR)?,
                c.workspace
                    .store
                    .meta::<FullJob>(FULL)?
                    .is_some_and(|job| !job.complete),
            ))
        })
        .await?;
    if baseline.is_none() || pending_snapshot {
        let complete = full(h, stage).await?;
        report.full_snapshot = true;
        report.export_pages += complete.export_pages;
        report.downloaded_bytes += complete.downloaded_bytes;
        report.verify_ms += complete.verify_ms;
        report.apply_ms += complete.apply_ms;
        report.revisions += complete.revisions;
        report.changes += complete.changes;
        report.through = complete.through;
        return Ok(());
    }
    incremental(h, stage, report).await
}

fn parse_page(
    trust: &Trust,
    account: &AccountId,
    epochs: &mut BTreeMap<u64, Security>,
    value: &Json,
    after: u64,
    through: u64,
    full: bool,
) -> Result<Page> {
    let started = std::time::Instant::now();
    let now = now_ms();
    let rows = value["entries"].as_array().ok_or(Error::IntegrityFailed)?;
    ensure(rows.len() <= 100, Error::IntegrityFailed)?;
    let mut cursor = after;
    let mut entries = Vec::new();
    let mut counts = BTreeMap::new();
    for row in rows {
        let at = row["cursor"].as_u64().ok_or(Error::IntegrityFailed)?;
        ensure(at > cursor && at <= through, Error::IntegrityFailed)?;
        cursor = at;
        let kind = row["kind"].as_str().ok_or(Error::IntegrityFailed)?;
        *counts.entry(kind.into()).or_default() += 1;
        let entry = match kind {
            "object" => {
                if full {
                    ensure(
                        row["missing_chunks"].as_array().is_some_and(Vec::is_empty),
                        Error::RecoveryIncomplete,
                    )?;
                }
                let plan: UploadPlan = serde_json::from_value(row["plan"].clone())
                    .map_err(|_| Error::IntegrityFailed)?;
                let signed: UploadPlan = verify_signed_entry(
                    trust,
                    account,
                    epochs,
                    value,
                    row,
                    row["signature"]["cose_sign1"].as_str().unwrap_or_default(),
                    "dmsg/upload/reserve/v1",
                    now,
                )?;
                ensure(
                    signed == plan
                        && row["value"]["upload_id"] == plan.upload_id
                        && row["value"]["digest"] == plan.manifest_digest,
                    Error::IntegrityFailed,
                )?;
                Entry::Object(plan)
            }
            "revision" => {
                let revision: Revision = verify_signed_entry(
                    trust,
                    account,
                    epochs,
                    value,
                    row,
                    row["value"]["signed"]["cose_sign1"]
                        .as_str()
                        .unwrap_or_default(),
                    "dmsg/vault/revision/v1",
                    now,
                )?;
                let fields = serde_json::to_value(&revision).map_err(|_| Error::IntegrityFailed)?;
                for (name, field) in fields.as_object().into_iter().flatten() {
                    ensure(row["value"][name] == *field, Error::IntegrityFailed)?;
                }
                let conflict = row["value"]["conflict"]
                    .as_bool()
                    .ok_or(Error::IntegrityFailed)?;
                Entry::Revision(revision, conflict)
            }
            "object_tombstone" => {
                #[derive(Deserialize, Serialize)]
                struct Collect {
                    upload_id: String,
                    generation: u64,
                }
                let removed: Collect = verify_signed_entry(
                    trust,
                    account,
                    epochs,
                    value,
                    row,
                    row["value"]["signed"]["cose_sign1"]
                        .as_str()
                        .unwrap_or_default(),
                    "dmsg/object/collect/v1",
                    now,
                )?;
                ensure(
                    row["value"]["upload_id"] == removed.upload_id
                        && row["value"]["generation"].as_u64() == removed.generation.checked_add(1),
                    Error::IntegrityFailed,
                )?;
                Entry::Retired(removed.upload_id)
            }
            _ => return Err(Error::UnsupportedProtocol),
        };
        entries.push((at, entry));
    }
    let end = value[if full { "inventory_end" } else { "end" }]
        .as_bool()
        .ok_or(Error::IntegrityFailed)?;
    let next = value["next_cursor"]
        .as_u64()
        .ok_or(Error::IntegrityFailed)?;
    ensure(
        next == if !full && end { through } else { cursor },
        Error::IntegrityFailed,
    )?;
    ensure(end || !entries.is_empty(), Error::IntegrityFailed)?;
    Ok(Page {
        entries,
        counts,
        cursor: next,
        end,
        verify_ms: started.elapsed().as_millis() as u64,
    })
}

fn snapshot(entries: impl IntoIterator<Item = Entry>, through: u64) -> Result<Snapshot> {
    let mut snapshot = Snapshot {
        through,
        ..Snapshot::default()
    };
    for entry in entries {
        match entry {
            Entry::Object(plan) if matches!(plan.kind.as_str(), "vault" | "file") => {
                ensure(
                    snapshot.plans.len() < MAX_PLANS,
                    Error::InvalidInput("RESPONSE_TOO_LARGE".into()),
                )?;
                snapshot.plans.push(plan);
            }
            Entry::Revision(revision, conflict) => {
                ensure(
                    snapshot.revisions.len() < MAX_REVISIONS,
                    Error::InvalidInput("RESPONSE_TOO_LARGE".into()),
                )?;
                snapshot.revisions.push((revision, conflict));
            }
            _ => {}
        }
    }
    Ok(snapshot)
}

fn forget_full(store: &Store) -> Result<()> {
    store.atomic(|store| {
        store.delete_meta(FULL)?;
        store.sync_clear_staged()
    })
}

pub(super) fn require_full(store: &Store) -> Result<()> {
    store.atomic(|store| {
        store.delete_meta(CURSOR)?;
        store.delete_meta(TARGET)?;
        store.delete_meta(FULL)?;
        store.sync_clear_staged()
    })
}

/// A full inventory is mandatory before the first incremental cursor is written.
pub(super) async fn full<H: Hold>(h: &H, stage: &mut Observer<'_>) -> Result<SyncReport> {
    close(h).await?;
    stage.stage(SyncStage::ExportStart);
    let (base, account) = h.with(|c| Ok((c.base()?, c.account()?.id))).await?;
    let mut job = h
        .with(|c| {
            let store = &c.workspace.store;
            if let Some(job) = store.meta::<FullJob>(FULL)? {
                if job
                    .start
                    .as_ref()
                    .is_none_or(|s| s["expires_at"].as_u64().is_some_and(|t| t > now_ms()))
                {
                    return Ok(job);
                }
                forget_full(store)?;
            }
            let job = FullJob {
                request_id: Hash::new(crate::crypto::random_id()),
                export_id: random_hex(),
                attempted: false,
                start: None,
                cursor: 0,
                counts: BTreeMap::new(),
                complete: false,
                close_id: Hash::new(crate::crypto::random_id()),
                close_retry_at: 0,
            };
            store.set_meta(FULL, &job)?;
            Ok(job)
        })
        .await?;
    if job.start.is_none() {
        let path = format!("{base}/operations/{}", hex::encode(*job.request_id));
        let previous = if job.attempted {
            h.find(&path)
                .await?
                .filter(|p| p["found"] == true)
                .map(|p| p["result"].clone())
        } else {
            None
        };
        job.attempted = true;
        h.with(|c| c.workspace.store.set_meta(FULL, &job)).await?;
        let start = match previous {
            Some(start) => start,
            None => h
                .post(
                    &format!("{base}/exports"),
                    "dmsg/export/start/v1",
                    &serde_json::json!({"export_id": job.export_id, "mode": "sync"}),
                    &job.request_id,
                )
                .await
                .map_err(|error| match error {
                    Error::InvalidInput(_) | Error::NotFound => Error::UnsupportedProtocol,
                    error => error,
                })?,
        };
        ensure(start["content_sync"] == 1, Error::UnsupportedProtocol)?;
        ensure(
            start["account"] == account.to_string() && start["scope"] == "cloud_snapshot",
            Error::IntegrityFailed,
        )?;
        let export_id = start["export_id"].as_str().ok_or(Error::IntegrityFailed)?;
        hex_id(export_id)?;
        job.export_id = export_id.into();
        if !start["expires_at"].as_u64().is_some_and(|t| t > now_ms()) {
            h.with(|c| forget_full(&c.workspace.store)).await?;
            return Err(Error::ResultExpired);
        }
        job.start = Some(start);
        h.with(|c| c.workspace.store.set_meta(FULL, &job)).await?;
    }
    let start = job.start.as_ref().ok_or(Error::IntegrityFailed)?;
    let through = start["through"].as_u64().ok_or(Error::IntegrityFailed)?;
    stage.progress.entries = job.counts.values().sum();
    stage.progress.total = start["counts"]
        .as_array()
        .ok_or(Error::IntegrityFailed)?
        .iter()
        .try_fold(0u64, |total, c| {
            total
                .checked_add(c["n"].as_u64().ok_or(Error::IntegrityFailed)?)
                .ok_or(Error::IntegrityFailed)
        })?;
    stage.emit();
    let trust = h.trust().await?;
    let mut epochs = BTreeMap::new();
    let mut report = SyncReport {
        full_snapshot: true,
        through,
        ..SyncReport::default()
    };
    loop {
        stage.stage(SyncStage::ExportPage);
        let reply = h
            .get(&format!(
                "{base}/exports/{}?after={}&limit=100",
                job.export_id, job.cursor
            ))
            .await;
        let value = match reply {
            Err(error @ (Error::ResultExpired | Error::NotFound)) => {
                h.with(|c| forget_full(&c.workspace.store)).await?;
                return Err(error);
            }
            value => value?,
        };
        let page = parse_page(
            &trust,
            &account,
            &mut epochs,
            &value,
            job.cursor,
            through,
            true,
        )?;
        report.export_pages += 1;
        stage.progress.export_pages += 1;
        report.verify_ms += page.verify_ms;
        let part = snapshot(page.entries.iter().map(|(_, e)| e.clone()), through)?;
        stage.stage(SyncStage::Download);
        report.downloaded_bytes += cache(h, &part.plans, stage).await?;
        for (kind, count) in &page.counts {
            *job.counts.entry(kind.clone()).or_default() += count;
        }
        job.cursor = page.cursor;
        h.with(|c| {
            c.workspace.store.atomic(|store| {
                for (cursor, entry) in &page.entries {
                    store.sync_stage(*cursor, entry)?;
                }
                store.set_meta(FULL, &job)
            })
        })
        .await?;
        stage.progress.entries = job.counts.values().sum();
        stage.emit();
        if page.end {
            break;
        }
    }
    let expected: BTreeMap<String, u64> = start["counts"]
        .as_array()
        .ok_or(Error::IntegrityFailed)?
        .iter()
        .map(|v| {
            Ok((
                v["kind"]
                    .as_str()
                    .ok_or(Error::IntegrityFailed)?
                    .to_string(),
                v["n"].as_u64().ok_or(Error::IntegrityFailed)?,
            ))
        })
        .collect::<Result<_>>()?;
    ensure(job.counts == expected, Error::RecoveryIncomplete)?;
    // A cache eviction between attempts may have removed an earlier page's bytes.
    let (complete, retired) = h
        .with(|c| {
            let entries = c.workspace.store.sync_staged::<Entry>()?;
            let retired: Vec<String> = entries
                .iter()
                .filter_map(|entry| match entry {
                    Entry::Retired(id) => Some(id.clone()),
                    _ => None,
                })
                .collect();
            Ok((snapshot(entries, through)?, retired))
        })
        .await?;
    stage.stage(SyncStage::Download);
    report.downloaded_bytes += cache(h, &complete.plans, stage).await?;
    stage.stage(SyncStage::Apply);
    let (revisions, elapsed) = h
        .with(|c| {
            let start = std::time::Instant::now();
            validate_retirements(c, &retired)?;
            let revisions = c.apply_versions(&complete, true, &retired)?;
            Ok((revisions, start.elapsed().as_millis() as u64))
        })
        .await?;
    report.revisions = revisions;
    report.changes = job.counts.values().sum::<u64>() as usize;
    report.apply_ms += elapsed;
    close_completed(h).await;
    Ok(report)
}

async fn close<H: Hold>(h: &H) -> Result<()> {
    let Some((base, job)) = h
        .with(|c| {
            c.workspace
                .store
                .meta::<FullJob>(FULL)?
                .filter(|j| j.complete)
                .map(|j| c.base().map(|b| (b, j)))
                .transpose()
        })
        .await?
    else {
        return Ok(());
    };
    let now = now_ms();
    if job.close_retry_at > now {
        return Err(crate::transport::busy(job.close_retry_at - now));
    }
    h.post(
        &format!("{base}/exports/{}/close", job.export_id),
        "dmsg/export/close/v1",
        &serde_json::json!({"export_id":job.export_id}),
        &job.close_id,
    )
    .await?;
    h.with(|c| forget_full(&c.workspace.store)).await
}

/// Housekeeping has its own small budget and retry time; verified content remains successful.
pub(super) async fn close_completed<H: Hold>(h: &H) {
    let due = h
        .with(|c| {
            Ok(c.workspace
                .store
                .meta::<FullJob>(FULL)?
                .is_some_and(|j| j.complete && now_ms() >= j.close_retry_at))
        })
        .await;
    if due != Ok(true) {
        return;
    }
    let timeout = futures_timer::Delay::new(std::time::Duration::from_secs(2));
    let retry = match futures_util::future::select(Box::pin(close(h)), Box::pin(timeout)).await {
        futures_util::future::Either::Left((Ok(()), _)) => return,
        futures_util::future::Either::Left((Err(error), _)) => {
            crate::diagnostic::Diagnostic::error(&error).emit();
            crate::transport::retry_after(&error).unwrap_or(60_000)
        }
        futures_util::future::Either::Right(_) => {
            crate::diagnostic::Diagnostic::new("relay", "timeout").emit();
            60_000
        }
    };
    let _ = h
        .with(|c| {
            if let Some(mut job) = c.workspace.store.meta::<FullJob>(FULL)?
                && job.complete
            {
                job.close_retry_at = now_ms().saturating_add(retry);
                c.workspace.store.set_meta(FULL, &job)?;
            }
            Ok(())
        })
        .await;
}

fn plan_for_revision(store: &Store, revision: &Revision) -> Result<UploadPlan> {
    let plan = match &revision.upload_id {
        Some(id) => store.sync_plan(id)?,
        None => store
            .sync_version_plans(&revision.item_id, &revision.revision_id)?
            .into_iter()
            .find(|p| p.kind == "vault"),
    }
    .ok_or(Error::RecoveryIncomplete)?;
    ensure(
        plan.kind == "vault"
            && plan.object_id == revision.item_id
            && plan.version_id == revision.revision_id
            && plan.chunks.len() == 1,
        Error::IntegrityFailed,
    )?;
    Ok(plan)
}

async fn incremental<H: Hold>(
    h: &H,
    stage: &mut Observer<'_>,
    report: &mut SyncReport,
) -> Result<()> {
    let (base, account) = h.with(|c| Ok((c.base()?, c.account()?.id))).await?;
    let trust = h.trust().await?;
    let mut epochs = BTreeMap::new();
    loop {
        stage.stage(SyncStage::DeltaPage);
        let (after, target) = h
            .with(|c| {
                Ok((
                    c.workspace
                        .store
                        .meta::<u64>(CURSOR)?
                        .ok_or(Error::RecoveryIncomplete)?,
                    c.workspace.store.meta::<u64>(TARGET)?,
                ))
            })
            .await?;
        let path = format!(
            "{base}/content/changes?after={after}{}&limit=100",
            target.map(|t| format!("&through={t}")).unwrap_or_default()
        );
        let value = h.get(&path).await.map_err(|e| match e {
            Error::NotFound => Error::UnsupportedProtocol,
            e => e,
        })?;
        ensure(value["content_sync"] == 1, Error::UnsupportedProtocol)?;
        let through = value["through"].as_u64().ok_or(Error::IntegrityFailed)?;
        ensure(
            value["account"] == account.to_string()
                && through >= after
                && target.is_none_or(|t| through == t),
            Error::IntegrityFailed,
        )?;
        let page = parse_page(&trust, &account, &mut epochs, &value, after, through, false)?;
        report.delta_pages += 1;
        stage.progress.delta_pages += 1;
        report.verify_ms += page.verify_ms;
        h.with(|c| c.workspace.store.set_meta(TARGET, &through))
            .await?;
        apply_incremental_page(h, &page, stage, report).await?;
        if page.end {
            return Ok(());
        }
    }
}

async fn apply_incremental_page<H: Hold>(
    h: &H,
    page: &Page,
    stage: &mut Observer<'_>,
    report: &mut SyncReport,
) -> Result<()> {
    let mut part = snapshot(page.entries.iter().map(|(_, e)| e.clone()), page.cursor)?;
    // Verified descriptions are a cache, not business state. A revision may refer
    // to an object from the preceding page or an earlier completed pass.
    let plans = h
        .with(|c| {
            c.workspace.store.atomic(|store| {
                for plan in &part.plans {
                    store.sync_put_plan(plan)?;
                }
                Ok(())
            })?;
            let mut needed = Vec::new();
            for (revision, _) in &part.revisions {
                let plan = plan_for_revision(&c.workspace.store, revision)?;
                if let Some(row) = c
                    .workspace
                    .store
                    .object(&format!("{}:{}", revision.item_id, revision.revision_id))?
                {
                    ensure(
                        plan.chunks[0].digest
                            == hex::encode(*sha256(&object_container(&record(&row)?))),
                        Error::IntegrityFailed,
                    )?;
                } else {
                    needed.push(plan);
                }
            }
            Ok(needed)
        })
        .await?;
    part.plans = plans;
    stage.stage(SyncStage::Download);
    report.downloaded_bytes += cache(h, &part.plans, stage).await?;
    let files = h
        .with(|c| {
            let mut files = Vec::new();
            for plan in &part.plans {
                let bytes = c
                    .workspace
                    .store
                    .chunk(&plan.chunks[0].digest)?
                    .ok_or(Error::RecoveryIncomplete)?;
                let record = read_container(&bytes)?;
                let payload = record.open(c.environment(), c.roots()?)?;
                if let Some(file) = record_file(&record.kind, &payload)? {
                    let plan = c
                        .workspace
                        .store
                        .sync_version_plans(&file.id, &file.version)?
                        .into_iter()
                        .find(|p| p.kind == "file")
                        .ok_or(Error::RecoveryIncomplete)?;
                    files.push(plan);
                }
            }
            Ok(files)
        })
        .await?;
    report.downloaded_bytes += cache(h, &files, stage).await?;
    part.plans.extend(files);
    let retired: Vec<String> = page
        .entries
        .iter()
        .filter_map(|(_, e)| match e {
            Entry::Retired(id) => Some(id.clone()),
            _ => None,
        })
        .collect();
    stage.stage(SyncStage::Apply);
    let (revisions, elapsed) = h
        .with(|c| {
            validate_retirements(c, &retired)?;
            let started = std::time::Instant::now();
            Ok((
                c.apply_versions(&part, false, &retired)?,
                started.elapsed().as_millis() as u64,
            ))
        })
        .await?;
    report.revisions += revisions;
    report.changes += page.entries.len();
    stage.progress.entries += page.entries.len() as u64;
    stage.emit();
    report.apply_ms += elapsed;
    report.through = page.cursor;
    Ok(())
}

fn validate_retirements(c: &Client, retired: &[String]) -> Result<()> {
    for upload in retired {
        if let Some(plan) = c.workspace.store.sync_plan(upload)? {
            if plan.kind == "vault" {
                ensure(
                    c.workspace
                        .store
                        .object(&format!("{}:{}", plan.object_id, plan.version_id))?
                        .is_none(),
                    Error::IntegrityFailed,
                )?;
            } else if plan.kind == "file" {
                for row in c.workspace.store.objects_of_kind(MIGRATION_PART)? {
                    if let Some(file) = c.version_file(&record(&row)?)? {
                        ensure(
                            file.id != plan.object_id || file.version != plan.version_id,
                            Error::IntegrityFailed,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "sync_pull_tests.rs"]
mod tests;
