//! One serial consumer reserves plans; reconnection reuses their identity.
use super::{
    policy::{self, Context, Decision, Reason},
    *,
};

type Candidate = (Item, u8, bool, bool);
type Group = (String, Vec<Candidate>);

/// One bounded scheduling pass, consumed by the sole native presentation owner.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Batch {
    /// New or retried content-free presentation plans.
    pub plans: Vec<Delivery>,
    /// Stable platform groups to close best-effort.
    pub remove: Vec<String>,
    /// Earliest reevaluation; an event may wake the host earlier.
    pub next_due: Option<u64>,
    /// Previous native groups, only populated for an explicit restore query.
    pub history: Vec<Delivery>,
}

impl Batch {
    fn due(&mut self, at: u64, now: u64) {
        let at = at.max(now.saturating_add(1));
        self.next_due = Some(self.next_due.map_or(at, |old| old.min(at)));
    }
}

fn group_id(account: &str, category: Category, group: &str) -> String {
    hex::encode(*digest(
        "dmsg/notification/group/v1",
        &(account, category, group),
    ))
}

/// Prepare local plans. `reserve` durably saves the installation budget before
/// the account transaction; failed storage never permits an unreserved OS call.
/// The caller serializes this through its Client and calls it only for a native
/// consumer, not a daemon background loop.
pub fn next(
    store: &Store,
    runtime: &mut Runtime,
    context: &Context,
    reserve: impl Fn(&Runtime) -> Result<()>,
) -> Result<Batch> {
    let mut batch = Batch::default();
    let settings = settings(store)?;
    let items = store.notification_items()?;
    let prior = store.notification_deliveries()?;
    let now = context.now;
    let mut retrying = std::collections::BTreeSet::new();
    for mut delivery in prior.iter().cloned() {
        let valid = runtime.settings.enabled
            && settings.enabled
            && settings
                .categories
                .get(&delivery.category)
                .is_some_and(|p| p.enabled)
            && delivery.covered.iter().any(|covered| {
                items.iter().any(|item| {
                    item.id == covered.id
                        && (item.version == covered.version
                            || (item.category() == Category::Security
                                && item.version > covered.version))
                        && item.open
                        && !item.is_muted(now)
                        && !(item.category() == Category::Action
                            && item.deadline.is_some_and(|at| at <= now))
                })
            });
        if !valid {
            if delivery.result != DeliveryResult::Suppressed {
                delivery.result = DeliveryResult::Suppressed;
                delivery.next_at = None;
                store.set_notification_delivery(&delivery)?;
                batch.remove.push(delivery.system_id.clone());
            }
            continue;
        }
        if !matches!(
            delivery.result,
            DeliveryResult::Pending | DeliveryResult::Failed
        ) {
            continue;
        }
        let deadline = delivery
            .covered
            .iter()
            .filter_map(|covered| {
                items
                    .iter()
                    .find(|item| item.id == covered.id)
                    .and_then(|item| item.deadline)
            })
            .min();
        let expires = policy::expires_at(delivery.category, delivery.attempted_at, deadline);
        if now >= expires {
            delivery.result = DeliveryResult::Suppressed;
            delivery.next_at = None;
            store.set_notification_delivery(&delivery)?;
            batch.remove.push(delivery.system_id.clone());
            continue;
        }
        retrying.extend(delivery.covered.iter().map(|c| c.id.clone()));
        if !(delivery.category == Category::Security && runtime.settings.security_exception) {
            let quiet = runtime
                .settings
                .paused_until
                .unwrap_or(0)
                .max(context.quiet_until.unwrap_or(0));
            if quiet > now {
                delivery.sound = false;
                delivery.next_at = Some(quiet.min(expires));
                store.set_notification_delivery(&delivery)?;
                batch.due(quiet.min(expires), now);
                continue;
            }
        }
        let Some(at) = delivery.next_at else {
            continue;
        };
        if at > now {
            batch.due(at, now);
            continue;
        }
        // A context change cannot turn a retried background plan into a second
        // foreground or audible notification.
        if context.foreground != delivery.in_app || !context.allowed {
            delivery.result = DeliveryResult::Suppressed;
            delivery.next_at = None;
            store.set_notification_delivery(&delivery)?;
            batch.remove.push(delivery.system_id.clone());
            continue;
        }
        delivery.sound = false;
        delivery.attempts += 1;
        delivery.next_at = policy::retry_at(delivery.attempts, now, expires);
        store.set_notification_delivery(&delivery)?;
        if let Some(at) = delivery.next_at {
            batch.due(at, now);
        }
        batch.plans.push(delivery);
    }

    let mut ready: BTreeMap<(Category, String), Vec<Candidate>> = BTreeMap::new();
    for original in items {
        if retrying.contains(&original.id) {
            continue;
        }
        let mut item = original;
        let category = item.category();
        let group = if category == Category::Security {
            "security".to_string()
        } else {
            item.group.clone()
        };
        let system_id = group_id(&item.account, category, &group);
        if let Some(previous) = prior.iter().find(|p| p.system_id == system_id) {
            item.attempted_at = Some(item.attempted_at.unwrap_or(0).max(previous.attempted_at));
        }
        let mut local_context = context.clone();
        local_context.viewing |= context
            .visible
            .iter()
            .any(|seen| seen.id == item.id && seen.version == item.version);
        match policy::evaluate(&item, &settings, runtime, &local_context) {
            Decision::Present {
                stage,
                sound,
                in_app,
                ..
            } => {
                ready
                    .entry((category, group))
                    .or_default()
                    .push((item, stage, sound, in_app));
            }
            Decision::Wait { until, reason } => {
                if reason == Reason::Quiet && !item.quiet {
                    item.quiet = true;
                    store.set_notification_item(&item)?;
                }
                batch.due(until, now);
            }
            Decision::Remove => {
                if prior.iter().any(|p| p.system_id == system_id) {
                    batch.remove.push(system_id);
                }
            }
            Decision::Keep {
                reason: Reason::Disabled | Reason::Permission,
            } => {
                if !item.quiet {
                    item.quiet = true;
                    store.set_notification_item(&item)?;
                }
            }
            Decision::Keep { .. } => {}
        }
    }
    // A burst across conversations can be represented by one category summary.
    // Every covered item remains in the plan; a count alone is never a watermark.
    let mut categories: BTreeMap<(Category, bool), Vec<Group>> = BTreeMap::new();
    for ((category, group), items) in ready {
        for in_app in [false, true] {
            let matching: Vec<_> = items
                .iter()
                .filter(|(_, _, _, local)| *local == in_app)
                .cloned()
                .collect();
            if !matching.is_empty() {
                categories
                    .entry((category, in_app))
                    .or_default()
                    .push((group.clone(), matching));
            }
        }
    }
    for ((category, _), groups) in categories {
        let group = if groups.len() == 1 {
            groups[0].0.clone()
        } else {
            "summary".into()
        };
        let candidates: Vec<_> = groups.into_iter().flat_map(|(_, items)| items).collect();
        let (first, _, _, in_app) = &candidates[0];
        // Earlier categories/accounts may have used a budget in this pass.
        let (sound, update) = match policy::evaluate(first, &settings, runtime, context) {
            Decision::Present { sound, update, .. } => (sound, update),
            Decision::Wait { until, .. } => {
                batch.due(until, now);
                continue;
            }
            _ => continue,
        };
        let covered: Vec<_> = candidates
            .iter()
            .map(|(item, stage, _, _)| Covered {
                id: item.id.clone(),
                version: item.version,
                stage: *stage,
            })
            .collect();
        let system_id = group_id(&first.account, category, &group);
        let sound = sound && candidates.iter().any(|(_, _, sound, _)| *sound);
        let delivery = Delivery {
            delivery_id: hex::encode(*digest(
                "dmsg/notification/delivery/v1",
                &(
                    &system_id,
                    &covered,
                    candidates
                        .iter()
                        .map(|(i, _, _, _)| i.snoozed_until)
                        .collect::<Vec<_>>(),
                ),
            )),
            system_id,
            account: first.account.clone(),
            category,
            target: if candidates.len() == 1 {
                first.target.clone()
            } else {
                Target::Summary(category)
            },
            covered,
            count: if category == Category::Message {
                candidates.iter().try_fold(0_u64, |count, (item, _, _, _)| {
                    item.unread.map(|n| count.saturating_add(n))
                })
            } else {
                Some(candidates.len() as u64)
            },
            sound,
            in_app: *in_app,
            update,
            attempted_at: now,
            next_at: Some(now.saturating_add(5_000)),
            attempts: 1,
            result: DeliveryResult::Pending,
        };
        if !delivery.in_app {
            runtime.reserve(category, sound, now);
            reserve(runtime)?;
        }
        store.atomic(|store| {
            for (mut item, stage, _, _) in candidates {
                item.notified = item.version;
                item.stage = item.stage.max(stage);
                item.attempted_at = Some(now);
                item.snoozed_until = None;
                store.set_notification_item(&item)?;
            }
            store.set_notification_delivery(&delivery)
        })?;
        batch.due(now.saturating_add(5_000), now);
        batch.plans.push(delivery);
    }
    batch.remove.sort();
    batch.remove.dedup();
    Ok(batch)
}

/// Idempotently settle one platform attempt; never modifies the business state.
pub fn result(
    store: &Store,
    system_id: &str,
    delivery_id: &str,
    result: DeliveryResult,
    now: u64,
) -> Result<()> {
    ensure(
        result != DeliveryResult::Pending,
        Error::InvalidInput("notification result".into()),
    )?;
    let Some(mut delivery) = store.notification_delivery(system_id)? else {
        return Ok(());
    };
    if delivery.delivery_id != delivery_id {
        return Ok(());
    }
    if matches!(
        delivery.result,
        DeliveryResult::Shown | DeliveryResult::Suppressed
    ) {
        return Ok(());
    }
    delivery.result = result;
    delivery.sound = false;
    delivery.next_at = if result == DeliveryResult::Failed {
        policy::retry_at(
            delivery.attempts,
            now,
            policy::expires_at(delivery.category, delivery.attempted_at, None),
        )
    } else {
        None
    };
    store.set_notification_delivery(&delivery)
}
