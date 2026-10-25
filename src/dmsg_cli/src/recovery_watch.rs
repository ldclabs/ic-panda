//! Observe recoveries independently of an open window or an unlocked session.

use crate::daemon::Daemon;
use dmsg_client::{hold::Hold, recovery_watch};
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
        let current = h
            .with(|c| Ok(c.session_state().account))
            .await
            .unwrap_or(None);
        if current != account {
            account = current;
            next = Instant::now();
        }
        // Announce security read by any foreground/background operation too.
        if let Ok(warning) = h.with(|c| c.recovery_warning()).await {
            if watch.changed(&account, &warning) {
                d.emit("recovery/changed", Value::Null);
            }
            let fresh = warning["observed_at_ms"].as_u64().is_some_and(|at| {
                dmsg_client::now_ms() < at.saturating_add(recovery_watch::POLL_MS)
            });
            if account.is_some() && !fresh && Instant::now() >= next {
                next = Instant::now() + Duration::from_millis(recovery_watch::POLL_MS);
                let result = recovery_watch::refresh(&h).await;
                d.note_result("recovery/watch", &result);
                if let Ok(warning) = h.with(|c| c.recovery_warning()).await
                    && watch.changed(&account, &warning)
                {
                    d.emit("recovery/changed", Value::Null);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
