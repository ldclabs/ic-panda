//! Deterministic policy: no I/O, clocks, locale, platform APIs or business writes.
use super::{Category, Item, Runtime, Settings, Target};
use serde::{Deserialize, Serialize};

/// Shared aggregation and cross-device coordination window (not two windows).
pub const MERGE_MS: u64 = 3_000;
/// Per-source interval and installation rolling-budget window.
pub const WINDOW_MS: u64 = 60_000;
/// Maximum new message/action OS presentations per installation per window.
pub const BUDGET: usize = 3;

/// Host-verified presentation context. The same timestamp is used for one turn.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    /// UTC milliseconds read once by the caller.
    pub now: u64,
    /// Native permission permits a request, or the owner opted in on a platform
    /// where permission is system-managed. This is never a delivery guarantee.
    pub allowed: bool,
    /// The application is actually visible and focused.
    pub foreground: bool,
    /// This exact account/item/version is in the loaded viewport.
    pub viewing: bool,
    /// Exact loaded viewports reported through the native owner's session fence.
    #[serde(default)]
    pub visible: Vec<super::Covered>,
    /// Another authenticated device viewed this exact version/watermark.
    pub remote_viewed: bool,
    /// Current daily quiet interval's UTC end, resolved by the host time zone.
    pub quiet_until: Option<u64>,
    /// Existing system group can be updated without another alert.
    pub can_update: bool,
    /// Mobile shared events have one remote OS owner, so local sync stays in-app.
    pub remote_owner: bool,
}

/// Observable policy result; reasons are fixed diagnostic values, not content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Decision {
    /// Persist state without presenting.
    Keep {
        /// Fixed suppression reason.
        reason: Reason,
    },
    /// Reevaluate at a bounded deadline or on an earlier state change.
    Wait {
        /// Earliest reevaluation in UTC milliseconds.
        until: u64,
        /// Fixed scheduling reason.
        reason: Reason,
    },
    /// Present fixed text using a platform adapter.
    Present {
        /// Reminder phase covered by this plan.
        stage: u8,
        /// Whether this first attempt may request a sound.
        sound: bool,
        /// Application hint only; no OS API or sound.
        in_app: bool,
        /// Update an existing group silently; unsupported hosts keep the summary.
        update: bool,
    },
    /// Remove the system group best-effort; business audit remains unchanged.
    Remove,
}

/// Closed diagnostic vocabulary shared with the service's policy vectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// Preference or registration is off.
    Disabled,
    /// OS permission has not been granted or cannot be confirmed.
    Permission,
    /// Resource mute applies.
    Muted,
    /// App pause/quiet hours apply.
    Quiet,
    /// The owner actually saw this version.
    Viewed,
    /// This exact version/phase was already attempted.
    Covered,
    /// Await the short collection window.
    Merge,
    /// Await a rolling budget or source interval.
    Budget,
    /// Await a verified deadline phase or snooze.
    Deadline,
    /// Shared mobile notifications belong to the remote path.
    RemoteOwner,
}

/// The current or next necessary phase, with its due time and presentation scope.
fn phase(item: &Item, now: u64) -> Option<(u8, u64, bool)> {
    let hour = 3_600_000;
    match item.category() {
        Category::Message => {
            (item.version > item.notified).then_some((1, item.pending_since + MERGE_MS, false))
        }
        Category::Security => {
            let phase = item.deadline.map_or(1, |deadline| {
                if now >= deadline.saturating_sub(hour) {
                    3
                } else if now >= deadline.saturating_sub(24 * hour) {
                    2
                } else {
                    1
                }
            });
            if item.stage < phase {
                return Some((phase, now, false));
            }
            item.deadline.and_then(|deadline| match item.stage {
                1 => Some((2, deadline.saturating_sub(24 * hour), false)),
                2 => Some((3, deadline.saturating_sub(hour), false)),
                _ => None,
            })
        }
        Category::Action => {
            if item.stage == 0 {
                return Some((1, item.pending_since + MERGE_MS, false));
            }
            if item.stage >= 2 || item.observed >= item.version {
                return None;
            }
            item.deadline.and_then(|deadline| {
                let margin = 300_000.min(deadline.saturating_sub(item.first_at) / 5);
                let at = deadline.saturating_sub(margin);
                (at >= item
                    .attempted_at
                    .unwrap_or(item.first_at)
                    .saturating_add(WINDOW_MS))
                .then_some((2, at, false))
            })
        }
        Category::Service => item
            .deadline
            .and_then(|deadline| {
                for (stage, days) in [(3, 1), (2, 7), (1, 30)] {
                    let at = deadline.saturating_sub(days * 24 * hour);
                    if now >= at && item.stage < stage {
                        return Some((stage, now, stage < 3));
                    }
                }
                match item.stage {
                    0 => Some((1, deadline.saturating_sub(30 * 24 * hour), true)),
                    1 => Some((2, deadline.saturating_sub(7 * 24 * hour), true)),
                    2 => Some((3, deadline.saturating_sub(24 * hour), false)),
                    _ => None,
                }
            })
            .or_else(|| (item.stage == 0).then_some((1, now, false))),
    }
}

/// Evaluate one verified projection. Callers group compatible plans and persist
/// coverage/budgets before handing anything to a platform.
pub fn evaluate(item: &Item, settings: &Settings, runtime: &Runtime, ctx: &Context) -> Decision {
    let keep = |reason| Decision::Keep { reason };
    let wait = |until, reason| Decision::Wait { until, reason };
    let now = ctx.now;
    let category = item.category();
    if !item.open
        || (category == Category::Message && item.unread == Some(0) && !item.unverified)
        || (category == Category::Action && item.deadline.is_some_and(|at| at <= now))
    {
        return Decision::Remove;
    }
    let Some(pref) = settings.categories.get(&category) else {
        return keep(Reason::Disabled);
    };
    if !runtime.settings.enabled || !settings.enabled || !pref.enabled {
        return keep(Reason::Disabled);
    }
    if let Target::AgentMail(agent) = &item.target
        && !settings.agent_mail.get(agent).copied().unwrap_or(false)
    {
        return keep(Reason::Disabled);
    }
    if item.is_muted(now) {
        return keep(Reason::Muted);
    }
    if !ctx.allowed && !ctx.foreground {
        return keep(Reason::Permission);
    }
    if !(category == Category::Security && runtime.settings.security_exception) {
        let until = runtime
            .settings
            .paused_until
            .unwrap_or(0)
            .max(ctx.quiet_until.unwrap_or(0));
        if until > now {
            return wait(until, Reason::Quiet);
        }
    }
    let snoozed = item.snoozed_until.is_some_and(|at| at <= now);
    if let Some(until) = item.snoozed_until.filter(|at| *at > now) {
        return wait(until, Reason::Deadline);
    }
    if ctx.viewing {
        return keep(Reason::Viewed);
    }
    if matches!(category, Category::Message | Category::Action)
        && !snoozed
        && (item.observed >= item.version || (settings.merge_devices && ctx.remote_viewed))
    {
        return keep(Reason::Viewed);
    }
    let Some((stage, due, only_in_app)) = (if snoozed {
        Some((item.stage.max(1), now, false))
    } else {
        phase(item, now)
    }) else {
        return keep(Reason::Covered);
    };
    // Actually reading the incident covers the initial alert on this device,
    // while the verified 24-hour/one-hour phases remain independently eligible.
    if category == Category::Security && stage == 1 && item.observed > 0 && !snoozed {
        return item.deadline.map_or_else(
            || keep(Reason::Viewed),
            |deadline| wait(deadline.saturating_sub(86_400_000), Reason::Deadline),
        );
    }
    if due > now {
        return wait(
            due,
            if stage == 1 {
                Reason::Merge
            } else {
                Reason::Deadline
            },
        );
    }
    if ctx.remote_owner && !item.target.local_only() && !ctx.foreground {
        return keep(Reason::RemoteOwner);
    }
    if only_in_app || ctx.foreground {
        return Decision::Present {
            stage,
            sound: false,
            in_app: true,
            update: false,
        };
    }
    if matches!(category, Category::Message | Category::Action) {
        if let Some(last) = item
            .attempted_at
            .filter(|at| now < at.saturating_add(WINDOW_MS))
        {
            // Updates do not consume a new banner or advance the final coverage:
            // the scheduler preserves a pending summary for the interval end.
            return wait(last.saturating_add(WINDOW_MS), Reason::Budget);
        }
        let times = if category == Category::Message {
            &runtime.messages
        } else {
            &runtime.actions
        };
        let active: Vec<_> = times
            .iter()
            .filter(|at| now < at.saturating_add(WINDOW_MS))
            .collect();
        if active.len() >= BUDGET {
            return wait(
                active
                    .into_iter()
                    .min()
                    .copied()
                    .unwrap_or(now)
                    .saturating_add(WINDOW_MS),
                Reason::Budget,
            );
        }
    }
    let update = category == Category::Security
        && runtime
            .security_at
            .is_some_and(|at| now < at.saturating_add(MERGE_MS));
    if update && !ctx.can_update {
        return wait(
            runtime.security_at.unwrap_or(now).saturating_add(MERGE_MS),
            Reason::Merge,
        );
    }
    let sound = pref.sound
        && !item.quiet
        && !update
        && now >= runtime.quiet_until
        && runtime
            .sound_at
            .is_none_or(|at| now >= at.saturating_add(WINDOW_MS));
    Decision::Present {
        stage,
        sound,
        in_app: false,
        update,
    }
}

/// Provider retry delay, bounded by attempts and TTL; no jitter in the pure core.
pub fn retry_at(attempts: u8, now: u64, expires: u64) -> Option<u64> {
    if attempts >= 12 {
        return None;
    }
    let delay = 5_000_u64
        .saturating_mul(1 << attempts.saturating_sub(1).min(8))
        .min(900_000);
    now.checked_add(delay).filter(|at| *at < expires)
}

/// Maximum provider retention, additionally bounded by a business deadline.
pub fn expires_at(category: Category, now: u64, deadline: Option<u64>) -> u64 {
    let ttl = if matches!(category, Category::Message | Category::Action) {
        3_600_000
    } else {
        86_400_000
    };
    // Recovery's execute-after and a service term are reminder anchors, not
    // expiry of the unresolved business item. Only an approval has this TTL cap.
    if category == Category::Action {
        deadline.unwrap_or(u64::MAX).min(now.saturating_add(ttl))
    } else {
        now.saturating_add(ttl)
    }
}
