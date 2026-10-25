//! Reconstructible, device-local display records. They never authorize an action.
//! Only an unlocked workspace can open them; no names or URLs occur in record keys.

use crate::hold::Hold;
use crate::{Client, codec::canonical, principal::PrincipalDocument};
use dmsg_types::{AccountId, Error, Result, agent::PrincipalInfo, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;

/// Every local step of a snapshot refresh also checks explicit cache invalidation.
pub struct Scoped<'a, H> {
    /// Existing session-bound client access.
    pub inner: &'a H,
    /// Captured with the original local observation.
    pub session: Session,
}

impl<H: Hold> Hold for Scoped<'_, H> {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        self.inner
            .with(|c| {
                c.check_display_session(self.session)?;
                step(c)
            })
            .await
    }
}

pub(crate) const PROFILE: &str = "profile-display";
pub(crate) const AVATAR: &str = "profile-avatar";
pub(crate) const NAMES: &str = "public-names";
/// Display freshness, never an authorization lifetime.
pub const PROFILE_TTL: u64 = 600_000;
/// A public name that is absent is checked sooner.
pub const ABSENT_TTL: u64 = 60_000;
/// Recheck even same-URL images, since external images can change.
pub const AVATAR_TTL: u64 = 86_400_000;
const MAX_NAMES: usize = 2048;
pub(crate) const MAX_RECORD: usize = 2 * 1024 * 1024;

/// The unlocked account and generation that own a pending display read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Session {
    /// Workspace account, never a process-wide namespace.
    pub account: AccountId,
    /// Invalidated on lock, including lock followed by unlock of the same account.
    pub generation: u64,
    /// In-flight display work is also invalidated by explicit cache clearing.
    pub cache_generation: u64,
}

/// A validated public display projection. `None` version means certified absence.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    /// Published name, without controller or authorization fields.
    pub name: Option<String>,
    /// Published image URL, never a credential-bearing request URL.
    pub avatar_url: Option<String>,
    /// Directory document version, or certified absence.
    pub updated_at: Option<u64>,
    /// Last accepted observation, not last local access.
    pub observed_at_ms: u64,
}

impl Profile {
    /// Project only a document already validated by the principal resolver.
    pub fn from_document(document: Option<&PrincipalDocument>, now: u64) -> Self {
        Self {
            name: document
                .and_then(|d| d.name.clone())
                .filter(|n| !n.is_empty()),
            avatar_url: document
                .and_then(|d| d.avatar_url.clone())
                .filter(|u| !u.is_empty()),
            updated_at: document.map(|d| d.updated_at as u64),
            observed_at_ms: now,
        }
    }

    fn valid(&self) -> bool {
        self.name.as_ref().is_none_or(|s| s.chars().count() <= 80)
            && self.avatar_url.as_ref().is_none_or(|s| s.len() <= 2048)
            && (self.updated_at.is_some() || (self.name.is_none() && self.avatar_url.is_none()))
    }
}

/// The single current avatar, sealed with its URL and bytes together.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Avatar {
    /// Exact published URL; the old image must not stand for a new URL.
    pub url: String,
    /// File-header-validated image type.
    pub mime: String,
    /// Bounded image bytes, encoded as a CBOR byte string.
    #[serde(with = "serde_bytes")]
    pub bytes: Vec<u8>,
    /// Last successful download; failures do not extend it.
    pub fetched_at_ms: u64,
}

/// One name observation; failures are represented by `Result`, never by `None`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Name {
    /// Empty or unpublished names are absent.
    pub name: Option<String>,
    /// Certified document version, absent only for certified missing documents.
    pub updated_at: Option<u64>,
    /// Last accepted observation.
    pub observed_at_ms: u64,
}

impl Name {
    /// Project a validated document or a trusted absence.
    pub fn from_document(document: Option<&PrincipalDocument>, now: u64) -> Self {
        let p = Profile::from_document(document, now);
        Self {
            name: p.name,
            updated_at: p.updated_at,
            observed_at_ms: now,
        }
    }

    /// Whether a display read should ask the daemon to refresh this name.
    pub fn stale(&self, now: u64) -> bool {
        stale(
            self.observed_at_ms,
            if self.name.is_some() {
                PROFILE_TTL
            } else {
                ABSENT_TTL
            },
            now,
        )
    }
}

/// Display-only wall-clock freshness. A large backward clock jump forces refresh.
pub fn stale(observed: u64, ttl: u64, now: u64) -> bool {
    observed > now.saturating_add(300_000) || now.saturating_sub(observed) >= ttl
}

impl Client {
    /// The generation used to distinguish lock/unlock of the same account.
    pub fn session_generation(&self) -> u64 {
        self.session_generation
    }

    /// Capture alongside a local read, while the same client lock is held.
    pub fn display_session(&self) -> Result<Session> {
        ensure(self.session_state().state == "unlocked", Error::Locked)?;
        self.keys()?;
        Ok(Session {
            account: self.account()?.id,
            generation: self.session_generation,
            cache_generation: self.display_generation,
        })
    }

    /// Recheck before every read or commit of work queued in `session`.
    pub fn check_display_session(&self, session: Session) -> Result<()> {
        ensure(self.display_session()? == session, Error::Locked)
    }

    /// Recoverable display records alone may be discarded on corrupt decoding.
    /// Storage failures propagate; they are not evidence that a record is absent.
    pub(crate) fn display_record<T: Serialize + DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>> {
        let keys = self.keys()?;
        match self.workspace.private(keys, key) {
            Ok(value) => Ok(value),
            Err(
                Error::IntegrityFailed
                | Error::QuotaExceeded
                | Error::InvalidInput(_)
                | Error::UnsupportedProtocol,
            ) => {
                crate::diagnostic::Diagnostic::new("client", "corrupt").emit();
                self.workspace.store.delete_sealed(key)?;
                Ok(None)
            }
            Err(Error::Unavailable(message)) if message == "corrupt record" => {
                crate::diagnostic::Diagnostic::new("client", "corrupt").emit();
                self.workspace.store.delete_sealed(key)?;
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn put_display<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let bytes = canonical(value);
        // Check both encoded size and the decoder's item/depth bounds before writing.
        crate::codec::decode_value(&bytes, MAX_RECORD)?;
        self.workspace.put_private(self.keys()?, key, value)
    }

    /// Current published projection, without any network work.
    pub fn profile_display(&self) -> Result<Option<Profile>> {
        let record = self.display_record::<Profile>(PROFILE)?;
        if record.as_ref().is_some_and(|p| !p.valid()) {
            self.workspace.store.delete_sealed(PROFILE)?;
            return Ok(None);
        }
        Ok(record)
    }

    /// Keep a verified observation unless a newer publication is already present.
    /// Returns whether display fields or the version changed.
    pub fn keep_profile_display(&self, profile: Profile) -> Result<bool> {
        self.keys()?;
        ensure(profile.valid(), Error::IntegrityFailed)?;
        let old = self.profile_display()?;
        if old
            .as_ref()
            .is_some_and(|p| p.updated_at > profile.updated_at)
        {
            return Ok(false);
        }
        let changed = old.as_ref().is_none_or(|p| {
            p.name != profile.name
                || p.avatar_url != profile.avatar_url
                || p.updated_at != profile.updated_at
        });
        self.workspace.store.atomic(|_| {
            self.put_display(PROFILE, &profile)?;
            if old.as_ref().and_then(|p| p.avatar_url.as_ref()) != profile.avatar_url.as_ref() {
                self.workspace.store.delete_sealed(AVATAR)?;
            }
            Ok(changed)
        })
    }

    /// Publication confirmation is newer than a lagging directory response.
    /// A display-write failure must not change a successful publication's outcome.
    pub(crate) fn published_display(&self, info: &PrincipalInfo) {
        if info.state.version != info.published_version {
            return;
        }
        let result = (|| {
            let fields: serde_json::Value =
                serde_json::from_slice(&info.state.metadata).map_err(|_| Error::IntegrityFailed)?;
            self.keep_profile_display(Profile {
                name: fields["name"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
                avatar_url: fields["avatar_url"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
                updated_at: Some(info.state.updated_at),
                observed_at_ms: crate::now_ms(),
            })
        })();
        if let Err(error) = result {
            crate::diagnostic::Diagnostic::error(&error).emit();
        }
    }

    /// Stored bytes only if their URL equals the requested URL.
    pub fn profile_avatar(&self, url: &str) -> Result<Option<Avatar>> {
        let avatar = self.display_record::<Avatar>(AVATAR)?;
        if let Some(ref a) = avatar
            && (a.url.len() > 2048
                || crate::principal::avatar_type(&a.bytes).ok() != Some(a.mime.as_str()))
        {
            self.workspace.store.delete_sealed(AVATAR)?;
            return Ok(None);
        }
        Ok(avatar.filter(|a| a.url == url))
    }

    /// Commit only if this URL still belongs to the current published profile.
    pub fn keep_profile_avatar(&self, avatar: Avatar) -> Result<bool> {
        ensure(
            crate::principal::avatar_type(&avatar.bytes)? == avatar.mime,
            Error::IntegrityFailed,
        )?;
        if self
            .profile_display()?
            .as_ref()
            .and_then(|p| p.avatar_url.as_deref())
            != Some(&avatar.url)
        {
            return Ok(false);
        }
        let changed = self
            .profile_avatar(&avatar.url)?
            .is_none_or(|a| a.bytes != avatar.bytes || a.mime != avatar.mime);
        self.put_display(AVATAR, &avatar)?;
        Ok(changed)
    }

    /// Public names observed by this unlocked workspace; there is no global cache.
    pub fn public_names(&self) -> Result<BTreeMap<AccountId, Name>> {
        let names = self
            .display_record::<BTreeMap<AccountId, Name>>(NAMES)?
            .unwrap_or_default();
        if names.len() > MAX_NAMES
            || names
                .values()
                .any(|n| n.name.as_ref().is_some_and(|s| s.chars().count() > 80))
        {
            self.workspace.store.delete_sealed(NAMES)?;
            return Ok(BTreeMap::new());
        }
        Ok(names)
    }

    /// Merge successful observations once per batch, with bounded oldest-first eviction.
    pub fn keep_public_names(&self, found: Vec<(AccountId, Name)>) -> Result<bool> {
        let mut names = self.public_names()?;
        let mut changed = false;
        for (id, name) in found {
            ensure(
                name.name.as_ref().is_none_or(|s| s.chars().count() <= 80),
                Error::IntegrityFailed,
            )?;
            if names
                .get(&id)
                .is_some_and(|n| n.updated_at > name.updated_at)
            {
                continue;
            }
            changed |= names
                .get(&id)
                .is_none_or(|n| n.name != name.name || n.updated_at != name.updated_at);
            names.insert(id, name);
        }
        while names.len() > MAX_NAMES {
            let oldest = names
                .iter()
                .min_by_key(|(_, n)| n.observed_at_ms)
                .map(|(id, _)| *id)
                .expect("nonempty");
            names.remove(&oldest);
        }
        self.put_display(NAMES, &names)?;
        Ok(changed)
    }
}
