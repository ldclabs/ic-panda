//! Content-free notification state shared by desktop and mobile hosts.
//! Business adapters own authenticity and ordering; presentation never changes
//! business read receipts, approval decisions or authorization.

mod delivery;
pub mod policy;
pub mod preferences;
pub mod remote;
mod runtime;
#[cfg(test)]
mod tests;

use crate::{codec::digest, store::Store};
pub use delivery::{Batch, next, result};
use dmsg_types::{Error, Result, ensure};
pub use runtime::{Badge, GlobalSettings, QuietHours, Runtime};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Policy/wire version. Changing defaults requires new interoperability vectors.
pub const VERSION: u8 = 1;
/// Recent history retention, independent of domain audit retention.
pub const RETENTION_MS: u64 = 30 * 86_400_000;

/// Human notification categories; callers cannot supply a free-form urgency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Certified incidents requiring attention.
    Security,
    /// An action the owner is actually eligible to take.
    Action,
    /// Verified new messages from another author.
    Message,
    /// Verified service deadlines or a blocked user task.
    Service,
}

/// Closed set of navigation destinations, never a URL or an executable command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Target {
    /// Channel ID.
    Channel(String),
    /// Inbox order ID, or the inbox summary.
    Inbox(Option<String>),
    /// Local approval request ID.
    Approval(String),
    /// Certified pending recovery digest.
    Recovery(String),
    /// Hosted Agent ID.
    AgentMail(String),
    /// A local task ID.
    Task(String),
    /// Service contract ID.
    Plan(String),
    /// A fixed service issue and its persistent episode ID.
    Service(String),
    /// A verified team-wallet proposal, always re-read before any action.
    Wallet {
        /// Wallet canister Principal.
        wallet: String,
        /// Canonical decimal proposal ID.
        proposal: String,
    },
    /// A content-free category awaiting reconciliation.
    Summary(Category),
}

impl Target {
    /// Category fixed by the trusted domain adapter.
    pub fn category(&self) -> Category {
        match self {
            Self::Recovery(_) => Category::Security,
            Self::Approval(_) | Self::Task(_) | Self::Wallet { .. } => Category::Action,
            Self::Plan(_) | Self::Service(_) => Category::Service,
            Self::Summary(category) => *category,
            _ => Category::Message,
        }
    }

    /// Agent mail remains local to the verified connector host.
    pub fn local_only(&self) -> bool {
        matches!(
            self,
            Self::Approval(_) | Self::Task(_) | Self::AgentMail(_) | Self::Service(_)
        )
    }
}

/// Stable, account-bound identity; no display text or observation time enters it.
pub fn item_id(account: &str, target: &Target) -> String {
    hex::encode(*digest("dmsg/notification/item/v1", &(account, target)))
}

/// Unsigned sequence numbers cross JSON as canonical decimal strings.
pub mod decimal {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    /// Encode without losing JavaScript integer precision.
    pub fn serialize<S: Serializer>(value: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&value.to_string())
    }

    /// Accept only the canonical decimal representation.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        let text = String::deserialize(d)?;
        let value = text.parse::<u64>().map_err(D::Error::custom)?;
        if value.to_string() != text {
            return Err(D::Error::custom("noncanonical sequence"));
        }
        Ok(value)
    }
}

/// Minimal projection, intentionally readable while the content root is locked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    /// Stable account-bound ID.
    pub id: String,
    /// Originating account, never inferred from the current UI account.
    pub account: String,
    /// Restricted navigation target.
    pub target: Target,
    /// Opaque verified source for action grouping; no origin/name in plaintext.
    pub group: String,
    /// Domain adapter's validated version or watermark.
    #[serde(with = "decimal")]
    pub version: u64,
    /// A business terminal state is the only way to resolve an item.
    pub open: bool,
    /// Whether this owner can still take an action.
    pub actionable: bool,
    /// Known ordinary unread count; None means awaiting verification.
    pub unread: Option<u64>,
    /// Last trusted count is retained while newer metadata awaits decryption.
    #[serde(default)]
    pub unverified: bool,
    /// First observation of this incident, in UTC milliseconds.
    pub first_at: u64,
    /// Last domain change in UTC milliseconds.
    pub updated_at: u64,
    /// Business-verified deadline; never guessed from a recovery observation.
    pub deadline: Option<u64>,
    /// Last actually visible version; listing reminders does not advance it.
    #[serde(with = "decimal")]
    pub observed: u64,
    /// Last attempted message version.
    #[serde(with = "decimal")]
    pub notified: u64,
    /// Last attempted reminder phase (0 means none).
    pub stage: u8,
    /// Last presentation attempt, not a delivery or read receipt.
    pub attempted_at: Option<u64>,
    /// Start of the current aggregation window; later messages do not extend it.
    pub pending_since: u64,
    /// User-selected reminder time, bounded by the business deadline.
    pub snoozed_until: Option<u64>,
    /// Resource mute, including hidden/left channels.
    pub muted: bool,
    /// Optional mute expiry. Expiry does not replay old unread messages.
    pub muted_until: Option<u64>,
    /// Initial history and legacy untyped hints can only make quiet summaries.
    pub quiet: bool,
}

impl Item {
    /// Construct a verified domain projection; merge with `upsert` before display.
    pub fn new(account: String, target: Target, version: u64, now: u64) -> Self {
        let id = item_id(&account, &target);
        Self {
            group: id.clone(),
            id,
            account,
            target,
            version,
            open: true,
            actionable: false,
            unread: Some(0),
            unverified: false,
            first_at: now,
            updated_at: now,
            deadline: None,
            observed: 0,
            notified: 0,
            stage: 0,
            attempted_at: None,
            pending_since: now,
            snoozed_until: None,
            muted: false,
            muted_until: None,
            quiet: false,
        }
    }

    /// The category is derived, never accepted separately from a payload.
    pub fn category(&self) -> Category {
        self.target.category()
    }

    /// Whether a resource mute applies at this instant.
    pub fn is_muted(&self, now: u64) -> bool {
        self.muted && self.muted_until.is_none_or(|until| until > now)
    }
}

/// Per-category device preferences.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CategorySettings {
    /// Allow system presentation, independently of badge counts.
    pub enabled: bool,
    /// Request sound, subject to the global sound budget and OS settings.
    pub sound: bool,
}

/// Account/device settings. Resource preferences stay account-scoped.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Optimistic concurrency revision.
    pub revision: u64,
    /// Explicit opt-in for this account on this installation.
    pub enabled: bool,
    /// All four categories must be present.
    pub categories: BTreeMap<Category, CategorySettings>,
    /// Use actual-view receipts from other devices.
    pub merge_devices: bool,
    /// Explicit owner opt-ins, not the connector's Mail watch setting.
    pub agent_mail: BTreeMap<String, bool>,
    /// Last remote-acknowledged local revision (zero until supported/confirmed).
    pub remote_revision: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            revision: 0,
            enabled: false,
            merge_devices: true,
            remote_revision: 0,
            agent_mail: BTreeMap::new(),
            categories: [
                Category::Security,
                Category::Action,
                Category::Message,
                Category::Service,
            ]
            .into_iter()
            .map(|category| {
                (
                    category,
                    CategorySettings {
                        enabled: true,
                        sound: category == Category::Security,
                    },
                )
            })
            .collect(),
        }
    }
}

/// Independent, state-derived badge counts.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Counts {
    /// Unique unresolved owner-actionable items.
    pub actionable: u64,
    /// Ordinary unread messages, excluding muted channels and Agent Mail.
    pub unread: u64,
    /// A generic hint or stale state needs reconciliation.
    pub unverified: bool,
}

/// A presentation attempt covers exact versions, even for a merged summary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Covered {
    /// Stable item ID.
    pub id: String,
    /// Covered domain sequence.
    #[serde(with = "decimal")]
    pub version: u64,
    /// Covered reminder phase.
    pub stage: u8,
}

/// Stable platform plan, persisted before the host calls an OS API.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    /// Account- and group-bound system ID.
    pub system_id: String,
    /// Identity of this exact coverage; repeated queries return the same ID.
    pub delivery_id: String,
    /// Original account.
    pub account: String,
    /// Fixed text selection, not user-controlled body text.
    pub category: Category,
    /// Original target, or a typed summary for multiple items.
    pub target: Target,
    /// Covered versions/phases.
    pub covered: Vec<Covered>,
    /// Verified count, None for an unverified hint.
    pub count: Option<u64>,
    /// Sound is reserved once; reconsumption always turns it off.
    pub sound: bool,
    /// Foreground plans update the application, never the OS.
    pub in_app: bool,
    /// Replace the existing native group without requesting a new alert.
    pub update: bool,
    /// UTC time of the first reservation.
    pub attempted_at: u64,
    /// Earliest retry, if the platform actually reported a failure.
    pub next_at: Option<u64>,
    /// Number of attempts, bounded by the policy.
    pub attempts: u8,
    /// Independent presentation state.
    pub result: DeliveryResult,
}

/// OS submission is not proof the user saw or acted on anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryResult {
    /// Persisted reservation awaiting host response.
    Pending,
    /// OS/provider accepted the request.
    Submitted,
    /// Platform explicitly reported a failure.
    Failed,
    /// Newer context or preferences suppressed the pending plan.
    Suppressed,
    /// A platform with a reliable callback reported display.
    Shown,
}

/// Merge a domain projection, preserving presentation/view state.
/// Stale domain sequences cannot revive resolved items. Errors are storage errors
/// or invalid projections; adapters must authenticate their input first.
pub fn upsert(store: &Store, mut item: Item) -> Result<bool> {
    ensure(
        item.id == item_id(&item.account, &item.target) && item.version > 0,
        Error::InvalidInput("notification identity".into()),
    )?;
    if item.category() == Category::Message && item.unread.is_none() {
        item.unverified = true;
    }
    if let Some(old) = store.notification_item(&item.id)? {
        if item.version < old.version {
            return Ok(false);
        }
        item.first_at = old.first_at;
        item.observed = old.observed;
        item.notified = old.notified;
        item.stage = old.stage.max(item.stage);
        item.attempted_at = old.attempted_at;
        item.snoozed_until = old.snoozed_until;
        if item.category() == Category::Message && item.unread.is_none() {
            item.unread = old.unread;
        }
        if matches!(item.target, Target::Recovery(_)) && item.deadline.is_none() {
            item.deadline = old.deadline;
        }
        item.quiet |= old.quiet && old.notified < old.version;
        item.pending_since = if old.notified < old.version {
            old.pending_since
        } else {
            item.updated_at
        };
        // A trusted scope refresh may change counts or resolve the same version.
        if !old.open && item.version == old.version && item.category() != Category::Message {
            item.open = false;
        }
        if item.version == old.version
            && item.open == old.open
            && item.actionable == old.actionable
            && item.unread == old.unread
            && item.unverified == old.unverified
            && item.deadline == old.deadline
            && item.muted == old.muted
            && item.muted_until == old.muted_until
            && item.quiet == old.quiet
        {
            item.updated_at = old.updated_at;
        }
        if item == old {
            return Ok(false);
        }
    }
    // Muted arrivals advance the presentation watermark, never the business read
    // cursor. Expiring the mute must not replay these messages.
    if item.category() == Category::Message {
        if item.unread == Some(0) && !item.unverified {
            item.open = false;
        }
        if item.is_muted(item.updated_at) || !item.open {
            item.notified = item.version;
        }
    }
    store.set_notification_item(&item)?;
    Ok(true)
}

/// Compute counts without networking or losing them when the workspace locks.
pub fn counts(items: &[Item], now: u64) -> Counts {
    let mut result = Counts::default();
    for item in items.iter().filter(|item| item.open) {
        result.actionable += u64::from(
            item.actionable
                && !(item.category() == Category::Action
                    && item.deadline.is_some_and(|at| at <= now)),
        );
        if item.category() == Category::Message
            && !item.is_muted(now)
            && !matches!(item.target, Target::AgentMail(_))
        {
            result.unread = result.unread.saturating_add(item.unread.unwrap_or(0));
            result.unverified |= item.unread.is_none() || item.unverified;
        }
    }
    result
}

/// Page of local reminders; unfinished work is never evicted by history limits.
#[derive(Serialize)]
pub struct Page {
    /// Content-free reminders in display order.
    pub items: Vec<Item>,
    /// Offset of the next bounded page.
    pub next: Option<usize>,
}

/// Shared desktop/mobile history filtering, independent of business storage.
pub fn list(store: &Store, open_only: bool, offset: usize, limit: usize, now: u64) -> Result<Page> {
    let cleared = store.meta::<u64>("notification:cleared_at")?.unwrap_or(0);
    let mut items = store.notification_items()?;
    let mut recent = 0;
    items.retain(|item| {
        let actionable = item.open
            && item.actionable
            && !(item.category() == Category::Action && item.deadline.is_some_and(|at| at <= now));
        if actionable {
            return true;
        }
        if open_only
            || item.updated_at < now.saturating_sub(RETENTION_MS)
            || (!item.open && item.updated_at <= cleared)
        {
            return false;
        }
        recent += 1;
        recent <= 1000
    });
    if open_only {
        items.sort_by_key(|i| {
            (
                i.category() != Category::Security,
                i.deadline.unwrap_or(u64::MAX),
                std::cmp::Reverse(i.first_at),
            )
        });
    }
    let limit = limit.clamp(1, 100);
    let end = offset.saturating_add(limit);
    let next = (end < items.len()).then_some(end);
    Ok(Page {
        items: items.into_iter().skip(offset).take(limit).collect(),
        next,
    })
}

/// Record actual visibility; this does not call any business read/ack API.
pub fn observe(store: &Store, id: &str, version: u64) -> Result<()> {
    let mut item = store.notification_item(id)?.ok_or(Error::NotFound)?;
    ensure(version == item.version, Error::VersionConflict)?;
    item.observed = version;
    store.set_notification_item(&item)
}

/// Snooze a still-actionable item without extending its business deadline.
pub fn snooze(store: &Store, id: &str, version: u64, until: u64, now: u64) -> Result<()> {
    let mut item = store.notification_item(id)?.ok_or(Error::NotFound)?;
    ensure(item.version == version, Error::VersionConflict)?;
    ensure(
        item.open
            && item.actionable
            && until > now
            && item.deadline.is_none_or(|deadline| until < deadline),
        Error::InvalidInput("notification snooze".into()),
    )?;
    item.snoozed_until = Some(until);
    store.atomic(|store| {
        store.set_notification_item(&item)?;
        if !item.target.local_only() {
            store.set_meta(
                &format!("notification:snooze:{}", item.id),
                &(version, until),
            )?;
        }
        Ok(())
    })
}

/// Read local account/device preferences without decrypting content.
pub fn settings(store: &Store) -> Result<Settings> {
    Ok(store.meta("notification:settings")?.unwrap_or_default())
}

/// Commit a complete allowlisted preference draft at its expected revision.
/// Remote acknowledgment is never accepted from a UI draft.
pub fn configure(store: &Store, expected: u64, mut draft: Settings) -> Result<Settings> {
    let previous = settings(store)?;
    ensure(previous.revision == expected, Error::VersionConflict)?;
    ensure(
        draft.categories.len() == 4 && draft.agent_mail.len() <= 256,
        Error::InvalidInput("notification settings".into()),
    )?;
    draft.revision = expected.checked_add(1).ok_or(Error::VersionConflict)?;
    draft.remote_revision = previous.remote_revision;
    store.atomic(|store| {
        store.set_meta("notification:settings", &draft)?;
        // The restricted receiver can stop remote delivery even for a locked or
        // inactive account, without waiting for device signing authority.
        if !draft.enabled {
            store.set_meta("notification:revoke_pending", &true)?;
        } else if store
            .meta::<serde_json::Value>("notification:receive")?
            .is_none()
        {
            store.delete_meta("notification:revoke_pending")?;
        }
        Ok(())
    })?;
    Ok(draft)
}

impl crate::Client {
    /// Project the connector host's committed receive watermark. The Mail watch
    /// does not opt the owner into system alerts; that is a separate preference.
    pub fn project_agent_mail_notification(
        &self,
        agent: &str,
        watermark: u64,
        unread: u64,
        now: u64,
    ) -> Result<()> {
        self.agent_find(agent)?;
        if watermark == 0 {
            return Ok(());
        }
        let mut item = Item::new(
            self.account()?.id.to_string(),
            Target::AgentMail(agent.into()),
            watermark,
            now,
        );
        item.unread = Some(unread);
        item.open = unread > 0;
        upsert(&self.workspace.store, item)?;
        Ok(())
    }

    /// Rebuild the unlocked account's projections from local authoritative data.
    /// This reads no network, executes no actions, and does not reserve display.
    pub fn reconcile_notifications(&self, now: u64) -> Result<()> {
        self.keys()?;
        self.reconcile_channel_notifications(now)?;
        for row in self.workspace.store.approvals(&[])? {
            self.project_approval_notification(&self.approval_record(&row.id)?)?;
        }
        for letter in self.inbox_cached()? {
            self.project_inbox_notification(&letter, letter.created_at.max(1), now, true)?;
        }
        // Unlocked reads now own these states; a later locked hint opens a new
        // placeholder rather than finding this one still open or covered.
        for mut item in self.workspace.store.notification_items()? {
            if matches!(item.target, Target::Summary(_)) && item.open {
                item.open = false;
                item.unread = Some(0);
                item.unverified = false;
                item.notified = item.version;
                item.stage = 0;
                item.updated_at = now;
                self.workspace.store.set_notification_item(&item)?;
            }
        }
        self.workspace
            .store
            .prune_notifications(now.saturating_sub(RETENTION_MS))
    }

    /// Installation counts for the app badge and reminder entry. The selected
    /// account counts whether or not it alerts, as the badge did before system
    /// notifications; another local account counts once its owner opted in.
    pub fn notification_status(&self, runtime: &Runtime, now: u64) -> Result<serde_json::Value> {
        let selected = self.session_state().account;
        let mut total = Counts::default();
        let mut accounts = Vec::new();
        for local in self.accounts()?.into_iter().filter(|a| a.bound) {
            let id = local.account.parse().map_err(|_| Error::IntegrityFailed)?;
            self.with_notification_store(&id, |store| {
                let settings = settings(store)?;
                let counts = counts(&store.notification_items()?, now);
                if settings.enabled || selected.as_deref() == Some(local.account.as_str()) {
                    total.actionable = total.actionable.saturating_add(counts.actionable);
                    total.unread = total.unread.saturating_add(counts.unread);
                    total.unverified |= counts.unverified;
                }
                accounts.push(serde_json::json!({"account": local.account, "enabled": settings.enabled,
                    "counts": counts, "pending_sync": settings.revision > settings.remote_revision}));
                Ok(())
            })?;
        }
        let badge = match runtime.settings.badge {
            Badge::All => total.actionable.saturating_add(total.unread),
            Badge::Actions => total.actionable,
            Badge::Off => 0,
        };
        Ok(serde_json::json!({
            "counts": total,
            "accounts": accounts,
            "badge": badge,
            "enabled": runtime.settings.enabled,
        }))
    }

    /// Project local user work requiring intervention. A transient failed sync
    /// is not a notification; an unfinished task blocked for 15 minutes is.
    /// Returns the next health deadline without scheduling a native presentation.
    pub fn notification_health(&self, now: u64) -> Result<Option<u64>> {
        if self.keys.is_none() || self.root.is_none() {
            return Ok(None);
        }
        let account = self.account()?.id.to_string();
        let store = &self.workspace.store;
        let last_sync = self.last_sync()?;
        let failure = last_sync.as_ref().and_then(|sync| sync.error.as_deref());
        let pending = store.count_in(crate::store::LOCAL)? > 0;
        let conflicts = store.count_conflicts(crate::content::VAULT)?;
        let temporary = pending && matches!(failure, Some("NETWORK_UNAVAILABLE" | "UNAVAILABLE"));
        let since: Option<u64> = store.meta("notification:network_since")?;
        let since = if temporary {
            Some(since.unwrap_or_else(|| last_sync.as_ref().map_or(now, |s| s.at)))
        } else {
            None
        };
        match since {
            Some(at) => store.set_meta("notification:network_since", &at)?,
            None => store.delete_meta("notification:network_since")?,
        }
        let due = since.map(|at| at.saturating_add(900_000));
        self.notification_health_episode(
            &account,
            "network",
            due.is_some_and(|at| at <= now),
            true,
            now,
        )?;
        self.notification_health_episode(&account, "conflicts", conflicts > 0, false, now)?;
        self.notification_health_episode(
            &account,
            "sync_failed",
            pending
                && matches!(
                    failure,
                    Some("INTEGRITY_FAILED" | "FORBIDDEN" | "QUOTA_EXCEEDED")
                ),
            false,
            now,
        )?;
        Ok(due.filter(|at| *at > now))
    }

    fn notification_health_episode(
        &self,
        account: &str,
        kind: &str,
        active: bool,
        service: bool,
        now: u64,
    ) -> Result<()> {
        self.with_notification_store(
            &account.parse().map_err(|_| Error::IntegrityFailed)?,
            |store| {
                let key = format!("notification:health:{kind}");
                let (mut episode, before): (u64, bool) = store.meta(&key)?.unwrap_or((0, false));
                if active == before {
                    return Ok(());
                }
                if active {
                    episode = episode.checked_add(1).ok_or(Error::VersionConflict)?;
                }
                let name = format!("{kind}:{episode}");
                let target = if service {
                    Target::Service(name)
                } else {
                    Target::Task(name)
                };
                let mut item = Item::new(account.into(), target, if active { 1 } else { 2 }, now);
                item.open = active;
                item.actionable = active;
                store.atomic(|store| {
                    upsert(store, item)?;
                    store.set_meta(&key, &(episode, active))
                })
            },
        )
    }

    /// Project a successfully authenticated service status. This function only
    /// accepts status built by the commerce adapter, not renderer/provider input.
    pub(crate) fn project_plan_notification(
        &self,
        status: &serde_json::Value,
        now: u64,
    ) -> Result<()> {
        let Some(account) = self
            .workspace
            .meta
            .account
            .as_ref()
            .map(|a| a.id.to_string())
        else {
            return Ok(());
        };
        let current = &status["current"];
        let revision = status["notification_revision"].as_u64().unwrap_or(1).max(1);
        let active = current["contract"]
            .as_str()
            .filter(|_| status["next"].is_null() && current["terminated_at_ms"].is_null());
        for mut item in self.workspace.store.notification_items()? {
            if let Target::Plan(id) = &item.target
                && Some(id.as_str()) != active
                && item.open
            {
                item.open = false;
                item.actionable = false;
                item.version = item.version.max(revision);
                item.updated_at = now;
                upsert(&self.workspace.store, item)?;
            }
        }
        if let Some(id) = active
            && let Some(deadline) = current["ends_at_ms"].as_u64()
        {
            let mut item = Item::new(account, Target::Plan(id.into()), revision, now);
            item.deadline = Some(deadline);
            let path = self.notification_runtime_path();
            let mut runtime = Runtime::load(&path, now)?;
            if let Some(stage) = runtime.legacy_plans.get(id) {
                item.stage = *stage;
            }
            upsert(&self.workspace.store, item)?;
            if runtime.legacy_plans.remove(id).is_some() {
                runtime.save(&path)?;
            }
        }
        Ok(())
    }
}

/// Refresh the small capability declaration without borrowing Client during I/O.
/// Unsupported services keep original message bodies. A failed request keeps
/// the last answer instead of turning a transient error into "unsupported".
pub async fn refresh_capabilities(h: &impl crate::hold::Hold) -> Result<()> {
    use crate::hold::{Hold, Unlocked};
    let h = Unlocked::new(h).await?;
    let cloud = h.with(|c| Ok(c.relay())).await?;
    let capabilities = cloud.public("/public/notifications/capabilities").await?;
    h.with(|c| {
        let enabled = capabilities.as_ref().is_some_and(|value| {
            value["protocol"] == "dmsg-notifications/1" && value["publish_metadata"] == true
        });
        c.workspace
            .store
            .set_meta("notification:publish_v1", &enabled)?;
        let receive = capabilities.as_ref().is_some_and(|v| {
            v["protocol"] == "dmsg-notifications/1" && v["feed"] == true && v["preferences"] == true
        });
        c.workspace
            .store
            .set_meta("notification:remote_v1", &receive)
    })
    .await
}
