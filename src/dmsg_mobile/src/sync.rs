//! One content lane for foreground sync and inbox key recovery.
use super::*;
use dmsg_client::sync::{SyncProgress, initialized, sync_observed};
use serde_json::{Value, json};
use std::time::Instant;

#[derive(Clone, Default, uniffi::Record)]
pub struct ContentSyncStatus {
    pub running: bool,
    pub initialized: bool,
    pub stage: String,
    pub entries: u64,
    pub total: u64,
    pub downloaded_bytes: u64,
    pub elapsed_ms: u64,
    pub last_attempt_at_ms: Option<u64>,
    pub last_success_at_ms: Option<u64>,
    pub retry_at_ms: Option<u64>,
    pub retry_required: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Trigger {
    Automatic,
    Manual,
    Inbox,
}

struct Running {
    generation: u64,
    receive_only: bool,
    active: bool,
    started: Instant,
    elapsed_ms: u64,
    progress: SyncProgress,
}

#[derive(Default)]
pub(crate) struct Tracker(Mutex<Option<Running>>);

struct Pass<'a> {
    tracker: &'a Tracker,
    trigger: Trigger,
    outcome: &'static str,
}

impl Tracker {
    fn start(&self, generation: u64, queued_ms: u64, trigger: Trigger) -> Pass<'_> {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(Running {
            generation,
            receive_only: matches!(trigger, Trigger::Inbox),
            active: true,
            started: Instant::now(),
            elapsed_ms: 0,
            progress: SyncProgress::default(),
        });
        eprintln!("dmsg: sync trigger={trigger:?} queue_ms={queued_ms}");
        Pass {
            tracker: self,
            trigger,
            outcome: "cancelled",
        }
    }

    fn progress(&self, progress: &SyncProgress) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run) = state.as_mut() {
            if run.progress.stage != progress.stage
                || run.progress.export_pages != progress.export_pages
                || run.progress.delta_pages != progress.delta_pages
            {
                eprintln!(
                    "dmsg: sync stage={} entries={} total={} bytes={} pages={} elapsed_ms={}",
                    progress.stage.as_str(),
                    progress.entries,
                    progress.total,
                    progress.downloaded_bytes,
                    progress.export_pages + progress.delta_pages,
                    run.started.elapsed().as_millis()
                );
            }
            run.progress = progress.clone();
        }
    }

    fn received_only(&self, generation: u64) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|s| s.generation == generation && s.receive_only)
    }

    fn status(&self, generation: u64) -> ContentSyncStatus {
        let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let Some(run) = state.as_ref().filter(|s| s.generation == generation) else {
            return ContentSyncStatus::default();
        };
        ContentSyncStatus {
            running: run.active,
            stage: run.progress.stage.as_str().into(),
            entries: run.progress.entries,
            total: run.progress.total,
            downloaded_bytes: run.progress.downloaded_bytes,
            elapsed_ms: if run.active {
                run.started.elapsed().as_millis() as u64
            } else {
                run.elapsed_ms
            },
            ..Default::default()
        }
    }
}

impl Drop for Pass<'_> {
    fn drop(&mut self) {
        if let Some(run) = self
            .tracker
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            run.active = false;
            run.elapsed_ms = run.started.elapsed().as_millis() as u64;
            eprintln!(
                "dmsg: sync trigger={:?} outcome={} stage={} entries={} bytes={} elapsed_ms={}",
                self.trigger,
                self.outcome,
                run.progress.stage.as_str(),
                run.progress.entries,
                run.progress.downloaded_bytes,
                run.elapsed_ms
            );
        }
    }
}

/// Called only while owning the account's sync lane; cancellation releases it.
pub(crate) async fn content_pass<H: Hold>(
    h: &H,
    tracker: &Tracker,
    generation: u64,
    requested: u64,
    trigger: Trigger,
) -> dmsg_types::Result<Value> {
    let automatic = !matches!(trigger, Trigger::Manual);
    if !automatic {
        h.with(dmsg_client::sync::retry_uploads).await?;
    }
    let last = h.with(|c| c.last_sync()).await?;
    let now = dmsg_client::now_ms();
    if let Some(last) = last {
        if (matches!(trigger, Trigger::Inbox) || !tracker.received_only(generation))
            && last.at >= requested
            && last.error.is_none()
            && (automatic || last.upload_error.is_none())
        {
            eprintln!(
                "dmsg: sync trigger={trigger:?} outcome=coalesced queue_ms={}",
                now.saturating_sub(requested)
            );
            return Ok(json!({"coalesced":true}));
        }
        if automatic && last.error.is_some() && last.automatic_at().is_none_or(|at| at > now) {
            eprintln!(
                "dmsg: sync trigger={trigger:?} outcome=paused retry_at_ms={}",
                last.automatic_at().unwrap_or(0)
            );
            return Ok(json!({"paused":true,"last_sync":last}));
        }
    }
    let mut pass = tracker.start(generation, now.saturating_sub(requested), trigger);
    let result = sync_observed(h, matches!(trigger, Trigger::Inbox), &mut |p| {
        tracker.progress(p)
    })
    .await;
    if let Err(error) = &result {
        eprintln!("dmsg: sync error={}", dmsg_client::error_code(error));
    }
    pass.outcome = if result.is_ok() { "complete" } else { "failed" };
    if let Some(last) = h.with(|c| c.last_sync()).await? {
        eprintln!(
            "dmsg: sync failures={} retry_at_ms={}",
            last.failures,
            last.retry_at.unwrap_or(0)
        );
    }
    serde_json::to_value(result?).map_err(|_| Error::IntegrityFailed)
}

#[uniffi::export]
impl MobileEngine {
    pub async fn sync(
        &self,
        automatic: bool,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        let turn = self.read_turns.lane("sync");
        let tracker = self.sync_tracker.clone();
        let generation = self.host.generation();
        let requested = dmsg_client::now_ms();
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    content_pass(
                        h,
                        &tracker,
                        generation,
                        requested,
                        if automatic {
                            Trigger::Automatic
                        } else {
                            Trigger::Manual
                        },
                    )
                    .await
                })
            }))
            .await
    }

    /// Local status only; no network and no new synchronization request.
    pub async fn content_sync_status(&self) -> Result<ContentSyncStatus> {
        let tracker = self.sync_tracker.clone();
        let generation = self.host.generation();
        self.held(move |h| {
            Box::pin(async move {
                h.with(|c| {
                    let mut status = tracker.status(generation);
                    status.initialized = initialized(c)?;
                    if let Some(last) = c.last_sync()? {
                        status.last_attempt_at_ms = Some(last.at);
                        status.last_success_at_ms = last.succeeded_at();
                        status.retry_required = last.error.is_some();
                        status.retry_at_ms = last.error.as_ref().and_then(|_| last.automatic_at());
                        status.error = last.error.or(last.upload_error);
                    }
                    Ok(status)
                })
                .await
            })
        })
        .await
    }
}
