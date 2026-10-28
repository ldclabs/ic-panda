//! Content-free notification state shared by desktop and mobile hosts.
//! Business adapters own authenticity and ordering; presentation never changes
//! business read receipts, approval decisions or authorization.

pub mod policy;
mod runtime;
#[cfg(test)]
mod tests;

use crate::{codec::digest, store::Store};
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
    /// A content-free category awaiting reconciliation.
    Summary(Category),
}

impl Target {
    /// Category fixed by the trusted domain adapter.
    pub fn category(&self) -> Category {
        match self {
            Self::Recovery(_) => Category::Security,
            Self::Approval(_) | Self::Task(_) => Category::Action,
            Self::Plan(_) => Category::Service,
            Self::Summary(category) => *category,
            _ => Category::Message,
        }
    }

    /// Agent mail remains local to the verified connector host.
    pub fn local_only(&self) -> bool {
        matches!(self, Self::Approval(_) | Self::Task(_) | Self::AgentMail(_))
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
    if let Some(old) = store.notification_item(&item.id)? {
        if item.version < old.version {
            return Ok(false);
        }
        item.first_at = old.first_at;
        item.observed = old.observed;
        item.notified = old.notified;
        item.stage = old.stage;
        item.attempted_at = old.attempted_at;
        item.snoozed_until = old.snoozed_until;
        item.pending_since = if old.notified < old.version {
            old.pending_since
        } else {
            item.updated_at
        };
        // A trusted scope refresh may change counts or resolve the same version.
        if !old.open && item.version == old.version {
            item.open = false;
        }
        if item.version == old.version {
            item.updated_at = old.updated_at;
        }
        if item == old {
            return Ok(false);
        }
    }
    // Muted arrivals advance the presentation watermark, never the business read
    // cursor. Expiring the mute must not replay these messages.
    if item.category() == Category::Message && item.is_muted(item.updated_at) {
        item.notified = item.version;
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
            result.unverified |= item.unread.is_none();
        }
    }
    result
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
    store.set_notification_item(&item)
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
    store.set_meta("notification:settings", &draft)?;
    Ok(draft)
}
