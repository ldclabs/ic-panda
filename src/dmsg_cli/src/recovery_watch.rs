//! Observe recoveries independently of an open window or an unlocked session.

use crate::daemon::Daemon;
use dmsg_client::{cache::Kind, hold::Hold, recovery_watch};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::time::Instant;

/// Emit only when the selected account or its pending request changes.
#[derive(Default)]
struct Watch(Option<Value>);

impl Watch {
    fn changed(&mut self, account: &Option<String>, warning: &Value) -> bool {
        let key = json!([account, warning["pending"], warning["digest"]]);
        if self.0.as_ref() == Some(&key) {
            return false;
        }
        self.0 = Some(key);
        true
    }
}

pub(crate) async fn run(d: Arc<Daemon>) {
    let mut watch = Watch::default();
    let mut account = None;
    let mut next = Instant::now();
    let mut ticks = tokio::time::interval(Duration::from_secs(10));
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticks.tick().await;
        let h = d.hold();
        // Announce security read by any foreground/background operation too.
        let Ok((current, warning)) = h
            .with(|c| Ok((c.session_state().account, c.recovery_warning()?)))
            .await
        else {
            continue;
        };
        if current != account {
            account = current;
            next = Instant::now();
        }
        announce(&d, &h, &mut watch, &account, &warning).await;
        let fresh = warning["observed_at_ms"]
            .as_u64()
            .is_some_and(|at| dmsg_client::now_ms() < at.saturating_add(recovery_watch::POLL_MS));
        if account.is_some() && !fresh && Instant::now() >= next {
            next = Instant::now() + Duration::from_millis(recovery_watch::POLL_MS);
            let result = recovery_watch::refresh(&h).await;
            d.note_result("recovery/watch", &result);
            if let Ok(warning) = h.with(|c| c.recovery_warning()).await {
                announce(&d, &h, &mut watch, &account, &warning).await;
            }
        }
    }
}

/// A changed request first expires the account security snapshot, so the
/// page an alert opens refreshes it instead of showing an older read.
async fn announce(
    d: &Daemon,
    h: &impl Hold,
    watch: &mut Watch,
    account: &Option<String>,
    warning: &Value,
) {
    if !watch.changed(account, warning) {
        return;
    }
    let expired = h
        .with(|c| {
            // A locked workspace has no display session to expire.
            if c.display_session().is_ok() {
                c.cache_invalidate(&[Kind::Account])?;
            }
            Ok(())
        })
        .await;
    match expired {
        // An earlier failed refresh must not hold the next one back.
        Ok(()) => d.snapshots.clear(),
        Err(error) => dmsg_client::diagnostic::Diagnostic::error(&error).emit(),
    }
    d.emit("recovery/changed", Value::Null);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home::Home;
    use dmsg_client::{now_ms, testing};

    async fn stale(d: &Daemon) -> Value {
        d.snapshot("account/security", &json!({"mode":"local"}))
            .await
            .unwrap()["stale"]
            .clone()
    }

    #[test]
    fn timestamps_do_not_repeat_events_but_cancellation_and_new_requests_do() {
        let mut watch = Watch::default();
        let account = Some("account-a".into());
        let mut warning = json!({"pending":true,"digest":"one","observed_at_ms":10});
        assert!(watch.changed(&account, &warning));
        warning["observed_at_ms"] = 20.into();
        assert!(!watch.changed(&account, &warning));
        warning["digest"] = "two".into();
        assert!(watch.changed(&account, &warning));
        warning = json!({"pending":false,"digest":null});
        assert!(watch.changed(&account, &warning));
        assert!(!watch.changed(&account, &warning));
        assert!(watch.changed(&Some("account-b".into()), &Value::Null));
    }

    #[tokio::test]
    async fn a_changed_request_expires_the_security_snapshot_once() {
        let dir = testing::TempDir::new();
        let client = testing::unlocked(&dir.path("workspace"), false, None);
        let saved = json!({"pending_recovery": null});
        client
            .cache_save(Kind::Account, "", &saved, now_ms())
            .unwrap();
        let home = Home {
            dir: dir.path("home"),
        };
        let d = Daemon::new(
            "owner".into(),
            0,
            home.settings().unwrap(),
            client,
            dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
            crate::mcp_events::McpEvents::new(&[]).unwrap(),
        );
        assert_eq!(stale(&d).await, false);
        let mut watch = Watch::default();
        let account = Some(testing::account().id.to_string());
        let warning = json!({"pending":true,"digest":"one","observed_at_ms":1});
        announce(&d, &d.hold(), &mut watch, &account, &warning).await;
        assert_eq!(stale(&d).await, true);
        // A later observation of the same request leaves a new read alone.
        d.client()
            .lock()
            .await
            .cache_save(Kind::Account, "", &saved, now_ms())
            .unwrap();
        let warning = json!({"pending":true,"digest":"one","observed_at_ms":2});
        announce(&d, &d.hold(), &mut watch, &account, &warning).await;
        assert_eq!(stale(&d).await, false);
    }
}
