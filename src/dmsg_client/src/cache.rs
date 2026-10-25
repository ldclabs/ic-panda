//! Owner display snapshots. The closed kind list also owns inventory and clearing.
//! Protocol checkpoints, journals, outbox and content are never cache entries.

use crate::{
    Client,
    codec::{canonical, sha256},
    display,
};
use dmsg_types::{Error, Result, ensure};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;

/// Explicit display domains; no caller-provided method or record prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// Certified entitlement projected for display, never for authorization.
    Plan,
    /// Product catalog.
    Catalog,
    /// Public payment assets.
    Assets,
    /// PANDA membership overview.
    Panda,
    /// Owned names.
    Handles,
    /// Account/device overview.
    Account,
    /// Account device summary.
    Summary,
    /// Recovery overview.
    Recovery,
    /// Wallet public state projected for display.
    Wallet,
    /// Wallet balances.
    Balances,
    /// Existing paged wallet history.
    History,
    /// Existing asset metadata.
    Asset,
    /// Existing contact policy snapshot.
    Inbox,
    /// Existing cloud inventory snapshot.
    Storage,
    /// Cloud usage and notices.
    Usage,
}

/// All reconstructible domains, used by the owner-only inventory.
pub const KINDS: &[Kind] = &[
    Kind::Plan,
    Kind::Catalog,
    Kind::Assets,
    Kind::Panda,
    Kind::Handles,
    Kind::Account,
    Kind::Summary,
    Kind::Recovery,
    Kind::Wallet,
    Kind::Balances,
    Kind::History,
    Kind::Asset,
    Kind::Inbox,
    Kind::Storage,
    Kind::Usage,
];

impl Kind {
    /// Stable, non-sensitive category used in events and record prefixes.
    pub fn name(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Catalog => "catalog",
            Self::Assets => "assets",
            Self::Panda => "panda",
            Self::Handles => "handles",
            Self::Account => "account",
            Self::Summary => "account-summary",
            Self::Recovery => "recovery",
            Self::Wallet => "wallet",
            Self::Balances => "balances",
            Self::History => "history",
            Self::Asset => "asset",
            Self::Inbox => "inbox-policy",
            Self::Storage => "storage",
            Self::Usage => "usage",
        }
    }

    /// Display freshness only; security commands always query their own authority.
    pub fn ttl(self) -> u64 {
        match self {
            Self::Catalog | Self::Assets => 3_600_000,
            Self::Asset => display::AVATAR_TTL,
            Self::Storage => 300_000,
            _ => 60_000,
        }
    }

    fn prefix(self) -> String {
        format!("display:{}:", self.name())
    }
}

/// A complete observation. Missing pages cause a miss, never an incomplete success.
#[derive(Serialize, Deserialize)]
struct Header {
    observed_at_ms: u64,
    pages: u32,
    digest: dmsg_types::Hash,
    source: dmsg_types::Hash,
}

/// Opened data for a local display response, not a security proof.
pub struct Snapshot {
    /// Display projection.
    pub data: Value,
    /// Last successful complete observation.
    pub observed_at_ms: u64,
}

/// Identity, clear and same-kind invalidation generations and source captured
/// before a network read. A native cache producer must not relabel an old reply
/// with today's source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteScope {
    account: dmsg_types::AccountId,
    session: u64,
    generation: u64,
    invalidation: u64,
    source: dmsg_types::Hash,
}

const PAGE: usize = 512 * 1024;
const MAX_ENTRY: usize = 16 * 1024 * 1024;
const BUDGET: usize = 32 * 1024 * 1024;
const MAX_ROWS: usize = 4096;

impl Client {
    /// Capture before reading a cacheable resource from its source.
    pub fn cache_scope(&self, kind: Kind) -> Result<WriteScope> {
        self.keys()?;
        Ok(WriteScope {
            account: self.account()?.id,
            session: self.session_generation,
            generation: self.display_generation,
            invalidation: self
                .cache_invalidations
                .get(&kind)
                .copied()
                .unwrap_or_default(),
            source: self.cache_source(kind)?,
        })
    }

    /// Commit only to the account, unlock, clear and same-kind invalidation
    /// generations and source that started the read. An invalidation of another
    /// kind does not reject it. This also covers callers outside the SWR workers.
    pub fn cache_save_checked(
        &self,
        scope: &WriteScope,
        kind: Kind,
        id: &str,
        data: &Value,
        now: u64,
    ) -> Result<()> {
        ensure(self.cache_scope(kind)? == *scope, Error::VersionConflict)?;
        self.cache_save(kind, id, data, now)
    }

    /// Resource identifiers are hidden with a purpose-derived HMAC key.
    /// Source bindings live in the encrypted header, not in readable record keys.
    pub fn cache_key(&self, kind: Kind, id: &str) -> Result<String> {
        self.keys()?;
        if id.is_empty() {
            return Ok(format!("{}current", kind.prefix()));
        }
        let source = canonical(&(kind.name(), id));
        let key = self.keys()?.derive(&"dmsg/display-cache-key/1");
        let mut mac = Hmac::<Sha256>::new_from_slice(key.as_slice()).expect("HMAC key");
        mac.update(&source);
        Ok(format!(
            "{}{}",
            kind.prefix(),
            hex::encode(mac.finalize().into_bytes())
        ))
    }

    fn cache_source(&self, kind: Kind) -> Result<dmsg_types::Hash> {
        let account = self.account()?;
        let caller = matches!(kind, Kind::Account | Kind::Panda | Kind::Recovery)
            .then(|| self.session().ok().map(|s| s.principal));
        let rpc = if matches!(kind, Kind::Wallet | Kind::Balances | Kind::Asset) {
            let settings = crate::wallet::desktop::settings(self)?;
            Some((settings.bitcoin_api, settings.evm_rpc, settings.solana_rpc))
        } else {
            None
        };
        let source = canonical(&(
            kind.name(),
            &self.config.environment,
            account.home,
            self.config.handle,
            &self.config.root_key,
            &self.config.ic_host,
            self.config.relay_origin(),
            caller,
            rpc,
        ));
        Ok(sha256(&source))
    }

    /// Read a complete encrypted snapshot without accessing the network.
    pub fn cache_read(&self, kind: Kind, id: &str) -> Result<Option<Snapshot>> {
        let key = self.cache_key(kind, id)?;
        let Some(header) = self.display_record::<Header>(&key)? else {
            self.cache_delete_key(&key)?;
            return Ok(None);
        };
        if header.pages == 0
            || header.pages as usize > MAX_ENTRY / PAGE
            || header.source != self.cache_source(kind)?
        {
            self.cache_delete_key(&key)?;
            return Ok(None);
        }
        let mut bytes = Vec::new();
        for page in 0..header.pages {
            let Some(part) =
                self.display_record::<serde_bytes::ByteBuf>(&format!("{key}:{page}"))?
            else {
                self.cache_delete_key(&key)?;
                return Ok(None);
            };
            if part.len() > PAGE {
                self.cache_delete_key(&key)?;
                return Ok(None);
            }
            bytes.extend_from_slice(&part);
        }
        if sha256(&bytes) != header.digest {
            self.cache_delete_key(&key)?;
            return Ok(None);
        }
        match serde_json::from_slice(&bytes) {
            Ok(data) => Ok(Some(Snapshot {
                data,
                observed_at_ms: header.observed_at_ms,
            })),
            Err(_) => {
                self.cache_delete_key(&key)?;
                Ok(None)
            }
        }
    }

    /// Persist complete data in bounded sealed pages, atomically replacing the old
    /// observation. Display storage failure never weakens a security decision.
    pub fn cache_save(&self, kind: Kind, id: &str, data: &Value, now: u64) -> Result<()> {
        let key = self.cache_key(kind, id)?;
        let bytes = serde_json::to_vec(data).map_err(|_| Error::IntegrityFailed)?;
        ensure(bytes.len() <= MAX_ENTRY, Error::QuotaExceeded)?;
        self.workspace.store.atomic(|_| {
            self.cache_delete_key(&key)?;
            let mut pages = 0;
            for (page, part) in bytes.chunks(PAGE).enumerate() {
                self.put_display(&format!("{key}:{page}"), &serde_bytes::Bytes::new(part))?;
                pages += 1;
            }
            self.put_display(
                &key,
                &Header {
                    observed_at_ms: now,
                    pages,
                    digest: sha256(&bytes),
                    source: self.cache_source(kind)?,
                },
            )?;
            self.cache_budget(kind, &key)
        })
    }

    fn cache_delete_key(&self, key: &str) -> Result<()> {
        self.workspace.store.delete_sealed(key)?;
        for page in self.workspace.store.sealed_keys(&format!("{key}:"))? {
            self.workspace.store.delete_sealed(&page)?;
        }
        Ok(())
    }

    fn cache_budget(&self, kind: Kind, keep: &str) -> Result<()> {
        let prefix = kind.prefix();
        let fits = |(records, bytes)| records <= MAX_ROWS as u64 && bytes <= BUDGET as u64;
        if fits(self.workspace.store.sealed_usage(&prefix)?) {
            return Ok(());
        }
        let roots: std::collections::BTreeSet<_> = self
            .workspace
            .store
            .sealed_keys(&prefix)?
            .into_iter()
            .map(|key| {
                format!(
                    "{}{}",
                    prefix,
                    key[prefix.len()..].split(':').next().unwrap_or_default()
                )
            })
            .filter(|key| key != keep)
            .collect();
        let mut headers = Vec::new();
        for key in roots {
            let observed = self
                .display_record::<Header>(&key)?
                .map_or(0, |h| h.observed_at_ms);
            headers.push((observed, key));
        }
        headers.sort();
        for (_, key) in headers {
            self.cache_delete_key(&key)?;
            if fits(self.workspace.store.sealed_usage(&prefix)?) {
                return Ok(());
            }
        }
        Err(Error::QuotaExceeded)
    }

    /// Cache-only inventory: logical encrypted bytes, not SQLite file savings.
    pub fn cache_status(&self) -> Result<Value> {
        self.display_session()?;
        let mut domains = Vec::new();
        for kind in KINDS {
            let (records, bytes) = self.workspace.store.sealed_usage(&kind.prefix())?;
            domains.push(json!({"kind":kind.name(),"records":records,"sealed_bytes":bytes,"budget_bytes":BUDGET,"ttl_ms":kind.ttl()}));
        }
        for (kind, key) in [
            ("profile", display::PROFILE),
            ("avatar", display::AVATAR),
            ("names", display::NAMES),
        ] {
            let saved = self.workspace.store.sealed(key)?;
            domains.push(json!({"kind":kind,"records":usize::from(saved.is_some()),"sealed_bytes":saved.map_or(0, |s|s.len())}));
        }
        Ok(json!({"domains":domains}))
    }

    /// Preserve historical display data but force the next visible read to refresh.
    /// The invalidation fences the in-flight snapshot commits of these kinds only.
    pub fn cache_invalidate(&mut self, kinds: &[Kind]) -> Result<()> {
        self.keys()?;
        // Even if disk cleanup fails, an already running reader cannot restore
        // an observation invalidated by a successful remote mutation.
        for kind in kinds {
            let count = self.cache_invalidations.entry(*kind).or_default();
            *count = count.wrapping_add(1);
        }
        self.workspace.store.atomic(|_| {
            for kind in kinds {
                for key in self.workspace.store.sealed_keys(&kind.prefix())? {
                    if key.matches(':').count() == 2
                        && let Some(mut header) = self.display_record::<Header>(&key)?
                    {
                        header.observed_at_ms = 0;
                        self.put_display(&key, &header)?;
                    }
                }
            }
            Ok(())
        })?;
        Ok(())
    }

    /// Delete only reconstructible records; keys, pins, history checkpoints,
    /// business records and unsent work are deliberately outside this list.
    pub fn cache_clear(&mut self) -> Result<()> {
        self.display_session()?;
        self.display_generation = self.display_generation.wrapping_add(1);
        self.workspace.store.atomic(|_| {
            for kind in KINDS {
                loop {
                    let rows = self.workspace.store.sealed_after(&kind.prefix(), "", 256)?;
                    if rows.is_empty() {
                        break;
                    }
                    for (key, _) in rows {
                        self.workspace.store.delete_sealed(&key)?;
                    }
                }
            }
            for key in [display::PROFILE, display::AVATAR, display::NAMES] {
                self.workspace.store.delete_sealed(key)?;
            }
            Ok(())
        })?;
        Ok(())
    }
}
