//! One content lane for foreground sync and inbox key recovery.
use super::*;
use dmsg_client::sync::{
    Due, Moment, RetryCause, Schedule, SyncProgress, initialized, position, sync_observed,
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Notify;

/// The wait between passes while nothing moves, before it grows.
const APP_SYNC_MS: u64 = dmsg_types::MINUTE;
/// How often the driver looks at the local state.
const LOOK: Duration = Duration::from_secs(2);
/// How often a running pass reports its progress.
const PROGRESS: Duration = Duration::from_secs(1);

/// What reaches the driver from outside its loop.
#[derive(Default)]
pub(crate) struct SyncSignal {
    pub(crate) wake: Notify,
    pub(crate) requested: AtomicBool,
    pub(crate) hinted: AtomicBool,
}

/// Told of the status and of passes that ended, from a background thread.
#[uniffi::export(callback_interface)]
pub trait ContentSyncObserver: Send + Sync {
    fn status(&self, status: ContentSyncStatus);
    /// `changed`: the pass moved what views show without moving the position
    /// the status reports. `error`: why it failed.
    fn passed(&self, changed: bool, error: Option<String>);
}

#[derive(Clone, Default, PartialEq, Eq, uniffi::Record)]
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
    pub failures: u32,
    pub through: u64,
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

/// Local status only; no network and no new synchronization request.
async fn status_of<H: Hold>(
    h: &H,
    tracker: &Tracker,
    generation: u64,
) -> dmsg_types::Result<ContentSyncStatus> {
    h.with(|c| {
        let mut status = tracker.status(generation);
        status.initialized = initialized(c)?;
        status.through = position(c)?;
        if let Some(last) = c.last_sync()? {
            status.last_attempt_at_ms = Some(last.at);
            status.last_success_at_ms = last.succeeded_at();
            status.retry_required = last.error.is_some();
            status.failures = last.failures;
            status.retry_at_ms = last.error.as_ref().map(|_| last.automatic_at());
            status.error = last.error.or(last.upload_error);
        }
        Ok(status)
    })
    .await
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
        if automatic && last.error.is_some() && last.automatic_at() > now {
            eprintln!(
                "dmsg: sync trigger={trigger:?} outcome=paused retry_at_ms={}",
                last.automatic_at()
            );
            return Ok(json!({"paused":true,"last_sync":last}));
        }
    }
    let mut pass = tracker.start(generation, now.saturating_sub(requested), trigger);
    let result = sync_observed(h, matches!(trigger, Trigger::Inbox), &mut |p| {
        tracker.progress(p)
    })
    .await;
    match &result {
        Ok(report) => eprintln!(
            "dmsg: sync sent={} blocked={} conflicts={}",
            report.sent, report.blocked, report.conflicts
        ),
        Err(error) => eprintln!("dmsg: sync error={}", dmsg_client::error_code(error)),
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
    /// Finish already signed submissions during the short background window.
    /// This never opens a snapshot or downloads account content. Unprepared
    /// versions wait for the next foreground pass.
    pub async fn flush_content(&self, operation: Arc<OperationToken>) -> Result<u64> {
        let turn = self.read_turns.lane("sync");
        operation
            .run(self.held(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    dmsg_client::sync::flush(h).await.map(|sent| sent as u64)
                })
            }))
            .await
    }

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

    /// Start a waiting automatic retry now, after an unlock or when the
    /// connection returned; a cooldown the relay asked for stays. This does not
    /// redial an already connected hint channel.
    pub async fn retry_sync_early(&self, network: bool) -> Result<bool> {
        let cause = if network {
            RetryCause::Network
        } else {
            RetryCause::Unlock
        };
        self.held(move |h| {
            Box::pin(async move { h.with(|c| dmsg_client::sync::retry_early(c, cause)).await })
        })
        .await
    }

    /// Local status only; no network and no new synchronization request.
    pub async fn content_sync_status(&self) -> Result<ContentSyncStatus> {
        let tracker = self.sync_tracker.clone();
        let generation = self.host.generation();
        self.held(move |h| Box::pin(async move { status_of(h, &tracker, generation).await }))
            .await
    }

    /// A hint or the network says there is something to do.
    pub fn request_content_sync(&self) {
        self.sync_signal.requested.store(true, Ordering::Relaxed);
        self.sync_signal.wake.notify_one();
    }

    /// Run automatic passes when they are due and report the status while the
    /// app is open and unlocked, until the operation ends, the session changes
    /// or the app leaves the foreground. A failed pass is recorded and retried
    /// on its own time; without `automatic` this only reports the status.
    pub async fn run_content_sync(
        &self,
        automatic: bool,
        observer: Box<dyn ContentSyncObserver>,
        operation: Arc<OperationToken>,
    ) -> Result<()> {
        let turn = self.read_turns.lane("sync");
        let tracker = self.sync_tracker.clone();
        let generation = self.host.generation();
        let signal = self.sync_signal.clone();
        operation
            .run(self.held(move |h| {
                Box::pin(async move {
                    let mut schedule = Schedule::new(dmsg_client::now_ms());
                    // Coming back to the foreground or unlocking is a reason by itself.
                    signal.requested.store(true, Ordering::Relaxed);
                    // A retry's time passing is news even with an equal status.
                    let mut shown = None;
                    let mut publish = async |observer: &dyn ContentSyncObserver| {
                        let status = status_of(h, &tracker, generation).await?;
                        let now = dmsg_client::now_ms();
                        let view = (
                            status.clone(),
                            status.retry_at_ms.is_some_and(|at| at <= now),
                        );
                        if shown.as_ref() != Some(&view) {
                            observer.status(status.clone());
                            shown = Some(view);
                        }
                        dmsg_types::Result::Ok(status)
                    };
                    loop {
                        let status = publish(observer.as_ref()).await?;
                        let now = dmsg_client::now_ms();
                        let requested = signal.requested.swap(false, Ordering::Relaxed);
                        let due = h
                            .with(|c| {
                                let last = c.last_sync()?;
                                let session = c.session_state();
                                Ok(schedule.due(&Moment {
                                    now,
                                    account: session.account.as_deref(),
                                    local_change: c.local_change()?,
                                    last: last.as_ref(),
                                    interval: APP_SYNC_MS,
                                    hinted: signal.hinted.load(Ordering::Relaxed),
                                    requested,
                                    running: status.running,
                                }))
                            })
                            .await?;
                        if !automatic || due == Due::No {
                            tokio::select! {
                                _ = tokio::time::sleep(LOOK) => {},
                                _ = signal.wake.notified() => {},
                            }
                            continue;
                        }
                        schedule.started(now);
                        let before = h.with(|c| position(c)).await?;
                        let pass = async {
                            let _turn = turn.lock().await;
                            content_pass(
                                h,
                                &tracker,
                                generation,
                                dmsg_client::now_ms(),
                                Trigger::Automatic,
                            )
                            .await
                        };
                        tokio::pin!(pass);
                        let result = loop {
                            tokio::select! {
                                result = &mut pass => break result,
                                _ = tokio::time::sleep(PROGRESS) => {
                                    publish(observer.as_ref()).await?;
                                },
                            }
                        };
                        let moved = h.with(|c| position(c)).await? != before;
                        schedule.finished(result.as_ref().ok().and_then(|report| {
                            let sent = report["sent"].as_u64().unwrap_or(0);
                            Some((sent as usize, report["through"].as_u64()?))
                        }));
                        let count =
                            |report: &Value, field: &str| report[field].as_u64().unwrap_or(0);
                        let changed = !moved
                            && result.as_ref().is_ok_and(|report| {
                                count(report, "changes")
                                    + count(report, "sent")
                                    + count(report, "blocked")
                                    > 0
                            });
                        publish(observer.as_ref()).await?;
                        observer.passed(changed, result.err().map(|e| dmsg_client::error_code(&e)));
                    }
                })
            }))
            .await
    }
}
