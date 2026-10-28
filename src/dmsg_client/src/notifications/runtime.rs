//! Small installation-wide state; serialized by the same Client owner as SQLite.
use super::{Category, VERSION};
use crate::storage;
use dmsg_types::{Error, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Write, path::Path};

/// System badge preference, separate from category presentation settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Badge {
    /// Owner actions and ordinary unread messages.
    #[default]
    All,
    /// Owner actions only.
    Actions,
    /// Hide the OS badge.
    Off,
}

/// A daily quiet interval in the installation's explicit IANA time zone.
/// Hosts convert it to UTC boundaries, including DST, before policy evaluation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuietHours {
    /// Minutes after local midnight, 0..1439.
    pub start: u16,
    /// Minutes after local midnight, distinct from start.
    pub end: u16,
    /// IANA time zone, validated by the platform time-zone implementation.
    pub timezone: String,
}

/// Device-wide preferences, shared across all registered accounts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalSettings {
    /// Optimistic concurrency revision.
    pub revision: u64,
    /// Explicit user opt-in. A fresh install is off.
    pub enabled: bool,
    /// UTC pause expiry.
    pub paused_until: Option<u64>,
    /// Daily quiet interval, off by default.
    pub quiet_hours: Option<QuietHours>,
    /// Allow security during app quiet hours; never bypass OS Focus/permissions.
    pub security_exception: bool,
    /// OS badge mode.
    pub badge: Badge,
}

/// Persisted cross-account budgets. No credentials or message data belong here.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    /// File format version.
    pub version: u8,
    /// Random installation ID, not an authentication credential.
    pub installation: String,
    /// Device-wide preferences.
    pub settings: GlobalSettings,
    /// At most three recent ordinary message attempts.
    pub messages: Vec<u64>,
    /// At most three recent action attempts.
    pub actions: Vec<u64>,
    /// Last security summary attempt (first is never delayed).
    pub security_at: Option<u64>,
    /// Last attempt which reserved sound.
    pub sound_at: Option<u64>,
    /// Fail-quiet interval after a corrupt runtime is rebuilt.
    pub quiet_until: u64,
    /// One-time migration marker for the desktop's old reminder file.
    pub reminders_imported: bool,
    /// Old contract phases awaiting verified account ownership.
    pub legacy_plans: BTreeMap<String, u8>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            version: VERSION,
            installation: hex::encode(crate::crypto::random_id()),
            settings: GlobalSettings::default(),
            messages: vec![],
            actions: vec![],
            security_at: None,
            sound_at: None,
            quiet_until: 0,
            reminders_imported: false,
            legacy_plans: BTreeMap::new(),
        }
    }
}

impl Runtime {
    /// Read or initialize installation state. Corruption does not enable alerts.
    /// I/O errors are returned; no secrets are loaded by this operation.
    pub fn load(path: &Path, now: u64) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<Self>(&bytes) {
                Ok(value) if value.version == VERSION => Ok(value),
                _ => {
                    let value = Self {
                        quiet_until: now.saturating_add(60_000),
                        ..Self::default()
                    };
                    // Preserve the damaged state for diagnosis without dumping it.
                    std::fs::rename(path, path.with_extension("corrupt")).map_err(storage)?;
                    value.save(path)?;
                    Ok(value)
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let value = Self::default();
                value.save(path)?;
                Ok(value)
            }
            Err(error) => Err(storage(error)),
        }
    }

    /// Atomically replace the private file before any platform submission.
    /// Returns storage errors; callers must not display after a failed reservation.
    pub fn save(&self, path: &Path) -> Result<()> {
        let temp = path.with_extension("tmp");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(storage)?;
        let bytes = serde_json::to_vec(self).map_err(|_| Error::IntegrityFailed)?;
        file.write_all(&bytes).map_err(storage)?;
        file.sync_all().map_err(storage)?;
        std::fs::rename(&temp, path).map_err(storage)
    }

    /// Apply an explicit revision-checked global settings change.
    pub fn configure(&mut self, expected: u64, mut settings: GlobalSettings) -> Result<()> {
        ensure(self.settings.revision == expected, Error::VersionConflict)?;
        if let Some(hours) = &settings.quiet_hours {
            ensure(
                hours.start < 1440
                    && hours.end < 1440
                    && hours.start != hours.end
                    && !hours.timezone.is_empty()
                    && hours.timezone.len() <= 128,
                Error::InvalidInput("quiet hours".into()),
            )?;
        }
        settings.revision = expected.checked_add(1).ok_or(Error::VersionConflict)?;
        self.settings = settings;
        Ok(())
    }

    /// Reserve a new OS presentation. Retried attempts never call this again.
    pub fn reserve(&mut self, category: Category, sound: bool, now: u64) {
        self.messages.retain(|at| now < at.saturating_add(60_000));
        self.actions.retain(|at| now < at.saturating_add(60_000));
        match category {
            Category::Message => self.messages.push(now),
            Category::Action => self.actions.push(now),
            Category::Security => self.security_at = Some(now),
            Category::Service => {}
        }
        if sound {
            self.sound_at = Some(now);
        }
    }
}
