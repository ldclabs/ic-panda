//! Content scheduling is independent of mailbox, file and mount maintenance.
use super::*;
use dmsg_client::sync::{Due, Moment, RetryCause, Schedule};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Trigger {
    Automatic,
    Resume,
    Manual,
    Inbox,
}

impl Trigger {
    fn name(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Resume => "unlock",
            Self::Manual => "manual",
            Self::Inbox => "inbox",
        }
    }
}

struct Running<'a> {
    daemon: &'a Daemon,
    /// Whether the pass moved anything a view shows. A pass that was cut off
    /// may have, so it counts until the pass reports otherwise.
    changed: bool,
}

/// `sync/finished` ends every pass and carries status; `sync/changed` also says
/// that views have new content to read.
impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.daemon.syncing.store(false, Ordering::Relaxed);
        self.daemon.emit("sync/finished", Value::Null);
        if self.changed {
            self.daemon.emit("sync/changed", Value::Null);
        }
    }
}

pub(super) async fn run(daemon: Arc<Daemon>) {
    let mut schedule = Schedule::new(now_ms());
    let mut flushed = 0;
    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(1)) => {},
            _ = daemon.sync_wake.notified() => {},
        }
        if daemon.stopping() {
            continue;
        }
        let (now, session, resumed, due, automatic) = {
            let client = daemon.client.lock().await;
            let resumed = daemon.sync_resumed.swap(false, Ordering::Relaxed);
            if resumed {
                let _ = dmsg_client::sync::retry_early(&client, RetryCause::Unlock);
            }
            let now = now_ms();
            let session = client.session_state();
            let last = client.last_sync().ok().flatten();
            let due = schedule.due(&Moment {
                now,
                account: session.account.as_deref(),
                local_change: client.local_change().unwrap_or(0),
                last: last.as_ref(),
                interval: daemon.settings.sync_interval_ms,
                hinted: daemon.hints_connected(),
                requested: daemon.sync_requested.swap(false, Ordering::Relaxed),
                running: daemon.syncing.load(Ordering::Relaxed),
            });
            let automatic = last.as_ref().is_none_or(|s| now >= s.automatic_at());
            (now, session, resumed, due, automatic)
        };
        let unlocked = session.state == "unlocked";
        if match due {
            Due::Now => unlocked,
            Due::Periodic => unlocked || session.resident,
            Due::No => false,
        } {
            schedule.started(now);
            let result = daemon
                .sync_pass(
                    if resumed {
                        Trigger::Resume
                    } else {
                        Trigger::Automatic
                    },
                    None,
                )
                .await;
            let report = result.as_ref().ok().and_then(Option::as_ref);
            schedule.finished(report.map(|r| (r.sent, r.through)));
            daemon.note_result("sync", &result);
        } else if automatic && now >= flushed + FLUSH_MS {
            flushed = now;
            let result = daemon.flush_pass().await;
            daemon.note_result("flush", &result);
            if let Ok(settled @ 1..) = result {
                crate::logs::write("flush", &[("settled", &settled)]);
            }
        }
    }
}

impl Daemon {
    pub(super) fn sync_progress_for(
        &self,
        generation: u64,
    ) -> Option<dmsg_client::sync::SyncProgress> {
        self.sync_progress
            .lock()
            .expect("sync progress")
            .as_ref()
            .filter(|(owner, _)| *owner == generation)
            .map(|(_, progress)| progress.clone())
    }

    /// Serialize content submissions, but leave the client available to
    /// local reads, edits and locking throughout every network wait.
    pub(crate) async fn sync_pass(
        &self,
        trigger: Trigger,
        expected_generation: Option<u64>,
    ) -> Result<Option<dmsg_client::sync::SyncReport>> {
        let h = self.hold();
        // Bind the account before waiting for another pass to finish.
        let requested_at = now_ms();
        let observed = self.sync_revision.load(Ordering::Acquire);
        let (before, generation) = h
            .with(|c| {
                ensure(
                    expected_generation.is_none_or(|g| g == c.session_generation()),
                    Error::Locked,
                )?;
                Ok((c.session_state(), c.session_generation()))
            })
            .await?;
        let _turn = self.turn("sync").await;
        h.with(|c| dmsg_types::ensure(c.session_generation() == generation, Error::Locked))
            .await?;
        let pending = h.with(|c| Ok(c.status()?.pending_sync)).await?;
        if self.sync_revision.load(Ordering::Acquire) != observed
            && let Some((finished_generation, received_only, Ok(report))) =
                self.sync_result.lock().expect("sync result").as_ref()
            && *finished_generation == generation
            && (trigger == Trigger::Inbox || !received_only)
            && (trigger == Trigger::Inbox || report.upload_error.is_none() && pending == 0)
        {
            return Ok(Some(report.clone()));
        }
        if trigger == Trigger::Manual {
            // Retry only after the previous pass has finished updating its gate.
            h.with(dmsg_client::sync::retry_uploads).await?;
        } else {
            let last = h.with(|c| c.last_sync()).await?;
            let now = now_ms();
            if let Some(last) = last
                && last.error.is_some()
                && last.automatic_at() > now
            {
                return Ok(None);
            }
        }
        *self.sync_progress.lock().expect("sync progress") = Some((generation, Default::default()));
        self.syncing.store(true, Ordering::Relaxed);
        let mut running = Running {
            daemon: self,
            changed: true,
        };
        let position = h.with(|c| dmsg_client::sync::position(c)).await?;
        self.emit("sync/progress", Value::Null);
        let started = std::time::Instant::now();
        let started_at = now_ms();
        crate::logs::write(
            "sync/start",
            &[
                ("trigger", &trigger.name()),
                ("queue_ms", &started_at.saturating_sub(requested_at)),
            ],
        );
        let mut emitted = started;
        let mut phase = None;
        let result =
            dmsg_client::sync::sync_observed(&h, trigger == Trigger::Inbox, &mut |progress| {
                *self.sync_progress.lock().expect("sync progress") =
                    Some((generation, progress.clone()));
                let changed = phase != Some(progress.stage);
                if changed {
                    phase = Some(progress.stage);
                    crate::logs::write(
                        "sync/progress",
                        &[
                            ("stage", &progress.stage.as_str()),
                            ("entries", &progress.entries),
                            ("total", &progress.total),
                            ("downloaded_bytes", &progress.downloaded_bytes),
                        ],
                    );
                }
                if changed || emitted.elapsed() >= Duration::from_millis(250) {
                    emitted = std::time::Instant::now();
                    self.emit("sync/progress", Value::Null);
                }
            })
            .await;
        let stage = h
            .with(|c| c.last_sync())
            .await
            .ok()
            .flatten()
            .filter(|last| last.at >= started_at)
            .and_then(|last| last.stage)
            .map(|stage| stage.as_str())
            .unwrap_or(if result.is_ok() {
                "complete"
            } else {
                "unrecorded"
            });
        let moved = h.with(|c| dmsg_client::sync::position(c)).await != Ok(position);
        running.changed = moved
            || result
                .as_ref()
                .is_ok_and(|r| r.changes + r.sent + r.blocked > 0);
        *self.sync_result.lock().expect("sync result") =
            Some((generation, trigger == Trigger::Inbox, result.clone()));
        self.sync_revision.fetch_add(1, Ordering::Release);
        if let Ok(report) = &result {
            crate::logs::write(
                "sync/report",
                &[
                    (
                        "mode",
                        &if report.full_snapshot {
                            "snapshot"
                        } else {
                            "incremental"
                        },
                    ),
                    ("pages", &(report.export_pages + report.delta_pages)),
                    ("changes", &report.changes),
                    ("revisions", &report.revisions),
                    ("downloaded_bytes", &report.downloaded_bytes),
                    ("verify_ms", &report.verify_ms),
                    ("apply_ms", &report.apply_ms),
                    ("sent", &report.sent),
                    ("blocked", &report.blocked),
                    ("conflicts", &report.conflicts),
                    ("pending", &report.pending),
                    ("upload", &report.upload_error.as_deref().unwrap_or("ok")),
                ],
            );
        }
        crate::logs::result(
            "sync/pass",
            &[("stage", &stage), ("ms", &started.elapsed().as_millis())],
            &result,
        );
        let after = self.client.lock().await.session_state();
        if before.state != after.state || before.account != after.account {
            self.emit("session/changed", Value::Null);
        }
        result.map(Some)
    }
}
