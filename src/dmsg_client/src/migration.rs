//! Personal legacy migration (PRD R39, `docs/protocol/legacy_archive_zh.md`).
//!
//! The device makes a `dmsg-legacy-pair/2` offer and opens the original
//! site's `/legacy` page with it in the URL fragment; the page seals the old
//! identity's archive to the offer's one-time key. Importing checks the file
//! against the stored offer, records the file's digest before anything is
//! saved (the offer then only continues that file), opens it, checks the
//! archive offline and keeps it as encrypted account content: parts of at
//! most 32 MiB (`migration_part`, each one file) and one `migration` record.
//! Both sync to the account's devices like any content; the receiving seed
//! stays on this device and is deleted once the archive is kept.
//!
//! Nothing here grants a device, a name or a shared channel; old authorship
//! and times remain claims of the old service.

use crate::client::Client;
use crate::codec::{canonical, decode, sha256};
use crate::content::MAX_OBJECT;
use crate::files::{FileManifest, MAX_FILE, read_file, store_file};
use crate::legacy::{self, Archive, Offer};
use crate::legacy_proof::{self, Change, FrozenChannel, Trust};
use crate::now_ms;
use crate::store::SYNCED;
use crate::upload::random_hex;
use dmsg_types::{Error, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteArray;
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::path::Path;
use zeroize::Zeroizing;

/// Record kind of a kept archive.
pub const MIGRATION: &str = "migration";
/// Record kind of one part of a kept archive.
pub const MIGRATION_PART: &str = "migration_part";
/// Bytes of one part.
pub const PART: usize = 32 * 1024 * 1024;
/// Unfinished offers a device keeps at most.
pub const MAX_PAIRS: usize = 8;
/// Largest frozen-history file.
pub const MAX_FROZEN: u64 = 32 * 1024 * 1024;
const PAIR: &str = "legacy-pair:";
const STORAGE: &str = "dmsg-legacy-storage/1";
const PART_FORMAT: &str = "dmsg-legacy-part/1";
/// Messages an import's report shows.
pub const REPORT_PAGE: usize = 100;
/// Changes kept per frozen channel; the rest are counted.
const MAX_CHANGES: usize = 64;

/// One offer of this device, sealed under its local data key.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PairState {
    offer: Offer,
    /// The receiving seed, until the archive is kept.
    seed: Option<ByteArray<32>>,
    /// SHA-256 of the transfer file this offer accepted, hex.
    claimed: Option<String>,
    /// When it accepted it.
    opened_at: Option<u64>,
    /// The archive record it produced.
    result: Option<String>,
}

/// A kept archive.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ArchiveRecord {
    /// `dmsg-legacy-storage/1`.
    pub format: String,
    /// SHA-256 of the archive's encoding, hex.
    pub archive_digest: String,
    /// [`Archive::content_id`].
    pub content_id: String,
    /// The part records, in order.
    pub parts: Vec<String>,
    /// The old Principal.
    pub principal: String,
    /// `Local`, `ECDH` or `VetKey`.
    pub mode: String,
    /// Source objects.
    pub objects: u64,
    /// Gaps after the offline check.
    pub gaps: u64,
    /// When this device kept it.
    pub imported_at: u64,
    /// Frozen comparison by channel: the changes kept and how many there were.
    pub frozen: BTreeMap<String, (Vec<Change>, u64)>,
}

/// One part of a kept archive; the file carries its bytes.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PartRecord {
    format: String,
    /// The archive's digest.
    archive: String,
    index: u64,
    file: FileManifest,
}

/// The file a content record carries, if its kind has one.
pub(crate) fn part_file(payload: &[u8]) -> Result<FileManifest> {
    let part: PartRecord = decode(payload, MAX_OBJECT)?;
    ensure(part.format == PART_FORMAT, Error::IntegrityFailed)?;
    Ok(part.file)
}

fn pair_view(offer: &Offer, started: bool) -> Json {
    json!({
        "nonce": offer.nonce,
        "origin": offer.origin,
        "target": offer.target,
        "expires_at": offer.expires_at,
        "fingerprint": offer.fingerprint(),
        "link": offer.link(),
        "started": started,
    })
}

impl Client {
    fn archive_synced(&self, id: &str, record: &ArchiveRecord) -> bool {
        std::iter::once(id)
            .chain(record.parts.iter().map(String::as_str))
            .all(|id| self.current(id).is_ok_and(|(row, _)| row.state == SYNCED))
    }

    fn pair_state(&self, nonce: &str) -> Result<PairState> {
        ensure_valid(
            nonce.len() == 64 && nonce.bytes().all(|b| b.is_ascii_hexdigit()),
            "nonce",
        )?;
        self.workspace
            .private(self.keys()?, &format!("{PAIR}{nonce}"))?
            .ok_or(Error::NotFound)
    }

    fn save_pair(&self, state: &PairState) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{PAIR}{}", state.offer.nonce), state)
    }

    fn pair_states(&self) -> Result<Vec<PairState>> {
        let mut states = vec![];
        for key in self.workspace.store.sealed_keys(PAIR)? {
            if let Some(state) = self.workspace.private(self.keys()?, &key)? {
                states.push(state);
            }
        }
        Ok(states)
    }

    /// A one-time offer for the old site at `origin`, to open in the
    /// system browser.
    ///
    /// # Errors
    /// Returns `Error::RecoveryIncomplete` before the device holds the
    /// account's content root, where the archive will be kept, and
    /// `Error::QuotaExceeded` with eight unfinished offers.
    pub fn legacy_pair(&mut self, origin: &str) -> Result<Json> {
        self.roots()?;
        let now = now_ms();
        let pending = self
            .pair_states()?
            .iter()
            .filter(|s| s.result.is_none() && (s.claimed.is_some() || s.offer.expires_at > now))
            .count();
        ensure(pending < MAX_PAIRS, Error::QuotaExceeded)?;
        let device = hex::encode(*self.workspace.meta.device_id);
        let (offer, seed) = legacy::new_offer(origin, &device, now)?;
        self.save_pair(&PairState {
            offer: offer.clone(),
            seed: Some(ByteArray::new(*seed)),
            claimed: None,
            opened_at: None,
            result: None,
        })?;
        Ok(pair_view(&offer, false))
    }

    /// Offers still valid, or that already accepted a file to continue.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn legacy_pairs(&self) -> Result<Vec<Json>> {
        let now = now_ms();
        Ok(self
            .pair_states()?
            .iter()
            .filter(|s| s.result.is_none() && (s.claimed.is_some() || s.offer.expires_at > now))
            .map(|s| pair_view(&s.offer, s.claimed.is_some()))
            .collect())
    }

    /// Forget an offer and its receiving seed; a kept archive stays.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown offer.
    pub fn legacy_cancel(&mut self, nonce: &str) -> Result<()> {
        self.pair_state(nonce)?;
        self.workspace.delete_private(&format!("{PAIR}{nonce}"))
    }

    /// Import the transfer file at `path` for the offer `nonce`, after the
    /// user compared the fingerprint and the old Principal on both sides.
    /// The same file again returns the kept archive; so does another
    /// export of the same content.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown offer, `Error::Forbidden`
    /// for another fingerprint, another old identity or a file sealed to
    /// another offer, `Error::IdempotencyConflict` when the offer already
    /// accepted another file, `Error::Expired` when a new file arrives after
    /// the offer, and `Error::IntegrityFailed` for a changed file.
    pub fn legacy_import(
        &mut self,
        path: &Path,
        nonce: &str,
        fingerprint: &str,
        principal: &str,
    ) -> Result<Json> {
        self.roots()?;
        let mut state = self.pair_state(nonce)?;
        ensure(state.offer.fingerprint() == fingerprint, Error::Forbidden)?;
        let metadata = std::fs::metadata(path).map_err(io)?;
        ensure_valid(metadata.is_file(), "path")?;
        ensure(
            metadata.len() > 0 && metadata.len() <= legacy::MAX_ARCHIVE as u64,
            Error::QuotaExceeded,
        )?;
        let bytes = std::fs::read(path).map_err(io)?;
        let digest = hex::encode(*sha256(&bytes));
        ensure(
            state.claimed.as_ref().is_none_or(|c| *c == digest),
            Error::IdempotencyConflict,
        )?;
        if let Some(id) = &state.result {
            let kept = self.archive_record(id)?;
            ensure(kept.principal == principal, Error::Forbidden)?;
            return self.legacy_report(id, 0, REPORT_PAGE);
        }
        let seed = state.seed.as_ref().ok_or(Error::NotFound)?;
        let opened_at = state.opened_at.unwrap_or_else(now_ms);
        let opened = legacy::open_transfer(&bytes, &state.offer, seed, opened_at)?;
        let archive = Archive::decode(&opened.plain)?;
        ensure(archive.inventory.principal == principal, Error::Forbidden)?;
        state.claimed = Some(digest);
        state.opened_at = Some(opened_at);
        self.save_pair(&state)?;
        let archive_digest = hex::encode(*sha256(&opened.plain));
        let content_id = archive.content_id();
        let id = match self.find_archive(&content_id, &archive_digest)? {
            Some(id) => id,
            None => {
                let checked = legacy::verify(&archive);
                self.keep_archive(&opened.plain, &archive, &archive_digest, checked.gaps.len())?
            }
        };
        state.result = Some(id.clone());
        state.seed = None;
        self.save_pair(&state)?;
        self.legacy_report(&id, 0, REPORT_PAGE)
    }

    /// A kept archive with the same content or encoding.
    fn find_archive(&self, content_id: &str, archive_digest: &str) -> Result<Option<String>> {
        for (id, _) in self.workspace.store.heads(MIGRATION)? {
            let record = self.archive_record(&id)?;
            if record.content_id == content_id || record.archive_digest == archive_digest {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    /// Keep an archive as parts and a record; parts already written by an
    /// interrupted import are reused.
    pub(crate) fn keep_archive(
        &mut self,
        plain: &[u8],
        archive: &Archive,
        archive_digest: &str,
        gaps: usize,
    ) -> Result<String> {
        let parts = self.store_archive_parts(plain, archive_digest)?;
        let record = ArchiveRecord {
            format: STORAGE.into(),
            archive_digest: archive_digest.into(),
            content_id: archive.content_id(),
            parts,
            principal: archive.inventory.principal.clone(),
            mode: archive.inventory.mode.clone(),
            objects: archive.inventory.objects.len() as u64,
            gaps: gaps as u64,
            imported_at: now_ms(),
            frozen: BTreeMap::new(),
        };
        let id = random_hex();
        self.write(
            MIGRATION,
            id.clone(),
            None,
            false,
            &canonical(&record),
            None,
        )?;
        Ok(id)
    }

    fn store_archive_parts(&mut self, plain: &[u8], archive_digest: &str) -> Result<Vec<String>> {
        let mut existing = BTreeMap::new();
        for (id, _) in self.workspace.store.heads(MIGRATION_PART)? {
            let (_, record) = self.current(&id)?;
            if record.tombstone {
                continue;
            }
            let part: PartRecord =
                decode(&record.open(self.environment(), self.roots()?)?, MAX_OBJECT)?;
            if part.archive == archive_digest {
                existing.insert(part.index, id);
            }
        }
        let mut parts = vec![];
        for (index, chunk) in plain.chunks(PART).enumerate() {
            let index = index as u64;
            if let Some(id) = existing.get(&index) {
                parts.push(id.clone());
                continue;
            }
            let file = store_file(
                &self.workspace.store,
                &format!("legacy-{archive_digest}-{index}.cbor"),
                "application/cbor",
                chunk,
            )?;
            let part = PartRecord {
                format: PART_FORMAT.into(),
                archive: archive_digest.into(),
                index,
                file,
            };
            let id = random_hex();
            let payload = Zeroizing::new(canonical(&part));
            self.write(
                MIGRATION_PART,
                id.clone(),
                None,
                false,
                &payload,
                Some(&part.file),
            )?;
            parts.push(id);
        }
        Ok(parts)
    }

    /// A kept archive's record.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for no such archive.
    pub fn archive_record(&self, id: &str) -> Result<ArchiveRecord> {
        let (row, record) = self.current(id)?;
        ensure(row.kind == MIGRATION && !record.tombstone, Error::NotFound)?;
        let value: ArchiveRecord =
            decode(&record.open(self.environment(), self.roots()?)?, MAX_OBJECT)?;
        ensure(value.format == STORAGE, Error::UnsupportedProtocol)?;
        Ok(value)
    }

    /// A kept archive, read back from its parts and checked.
    ///
    /// # Errors
    /// Returns `Error::RecoveryIncomplete` while a part has not synced and
    /// `Error::IntegrityFailed` when the parts do not make the archive.
    pub(crate) fn kept_archive(&self, id: &str) -> Result<(ArchiveRecord, Zeroizing<Vec<u8>>)> {
        let record = self.archive_record(id)?;
        ensure(
            !record.parts.is_empty() && record.parts.len() <= legacy::MAX_ARCHIVE / PART,
            Error::IntegrityFailed,
        )?;
        let mut plain = Zeroizing::new(Vec::new());
        for (index, part_id) in record.parts.iter().enumerate() {
            let (row, stored) = self.current(part_id)?;
            ensure(row.kind == MIGRATION_PART, Error::IntegrityFailed)?;
            let part: PartRecord =
                decode(&stored.open(self.environment(), self.roots()?)?, MAX_OBJECT)?;
            ensure(
                part.archive == record.archive_digest && part.index == index as u64,
                Error::IntegrityFailed,
            )?;
            read_file(
                &self.workspace.store,
                &part.file,
                MAX_FILE,
                Some(&mut *plain),
            )?;
        }
        ensure(
            hex::encode(*sha256(&plain)) == record.archive_digest,
            Error::IntegrityFailed,
        )?;
        Ok((record, plain))
    }

    /// Delete a personal history channel by rewriting the archive without
    /// its content. The new parts are kept before their parent points to
    /// them; old unreferenced parts then receive synced tombstones.
    ///
    /// # Errors
    /// As [`Self::kept_archive`], or `Error::NotFound` for an absent channel.
    pub fn legacy_delete_channel(&mut self, id: &str, channel: &str) -> Result<()> {
        let (mut record, plain) = self.kept_archive(id)?;
        let mut archive = Archive::decode(&plain)?;
        archive.remove_channel(channel)?;
        let encoded = Zeroizing::new(archive.encode()?);
        let digest = hex::encode(*sha256(&encoded));
        let old_parts = record.parts;
        record.parts = self.store_archive_parts(&encoded, &digest)?;
        record.archive_digest = digest;
        record.content_id = archive.content_id();
        record.objects = archive.inventory.objects.len() as u64;
        record.gaps = legacy::verify(&archive).gaps.len() as u64;
        record.frozen.remove(channel);
        let (row, _) = self.current(id)?;
        self.write(
            MIGRATION,
            id.into(),
            Some(row.revision),
            false,
            &canonical(&record),
            None,
        )?;

        // A part reused by another archive must stay available there.
        let mut referenced = std::collections::BTreeSet::new();
        for (archive, _) in self.workspace.store.heads(MIGRATION)? {
            referenced.extend(self.archive_record(&archive)?.parts);
        }
        for part in old_parts
            .into_iter()
            .filter(|part| !referenced.contains(part))
        {
            let (row, stored) = self.current(&part)?;
            if !stored.tombstone {
                let payload = stored.open(self.environment(), self.roots()?)?;
                self.write(
                    MIGRATION_PART,
                    part,
                    Some(row.revision),
                    true,
                    &payload,
                    None,
                )?;
            }
        }
        Ok(())
    }

    /// The kept archives.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn legacy_list(&self) -> Result<Vec<Json>> {
        let mut list = vec![];
        for (id, _) in self.workspace.store.heads(MIGRATION)? {
            let Ok(record) = self.archive_record(&id) else {
                continue;
            };
            list.push(json!({
                "id": id,
                "principal": record.principal,
                "mode": record.mode,
                "objects": record.objects,
                "gaps": record.gaps,
                "imported_at": record.imported_at,
                "parts": record.parts.len(),
                "synced": self.archive_synced(&id, &record),
            }));
        }
        Ok(list)
    }

    /// Check a kept archive offline again and report it: its gaps, the old
    /// profile, files, resources and entitlements, the frozen comparison,
    /// and `limit` messages from `offset`.
    ///
    /// # Errors
    /// As [`Self::kept_archive`].
    pub fn legacy_report(&self, id: &str, offset: usize, limit: usize) -> Result<Json> {
        self.legacy_channel_report(id, None, offset, limit)
    }

    /// Personal history from every kept archive, independent of gaps in
    /// other channels. An archive still downloading does not hide the rest.
    ///
    /// # Errors
    /// Returns a storage error if the archive index cannot be read.
    pub fn legacy_channels(&self) -> Result<Json> {
        let mut channels = Vec::new();
        let mut errors = Vec::new();
        for (id, _) in self.workspace.store.heads(MIGRATION)? {
            match self.legacy_report(&id, 0, 0) {
                Ok(report) => {
                    for channel in report["channels"].as_array().into_iter().flatten() {
                        let mut channel = channel.clone();
                        channel["archive"] = id.clone().into();
                        channel["principal"] = report["principal"].clone();
                        channel["synced"] = report["synced"].clone();
                        channels.push(channel);
                    }
                }
                Err(error) => errors.push(json!({"archive": id, "error": format!("{error:?}")})),
            }
        }
        Ok(json!({"channels": channels, "errors": errors}))
    }

    /// A kept archive, optionally limited to one old channel. This reads
    /// personal history and never creates new shared-channel membership.
    ///
    /// # Errors
    /// As [`Self::kept_archive`], or `Error::NotFound` for another channel.
    pub fn legacy_channel_report(
        &self,
        id: &str,
        channel: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Json> {
        let (record, plain) = self.kept_archive(id)?;
        let archive = Archive::decode(&plain)?;
        let checked = legacy::verify(&archive);
        let summary = legacy::summary(&archive, &checked);
        let channels: Vec<_> = summary
            .resources
            .iter()
            .map(|resource| {
                let source = resource["source"].as_str().unwrap_or_default();
                let prefix = format!("{source}/message/");
                let total = archive
                    .inventory
                    .objects
                    .iter()
                    .filter(|o| o.kind == "message" && o.key.starts_with(&prefix))
                    .count();
                let readable = checked
                    .messages
                    .iter()
                    .filter(|m| m.source.starts_with(&prefix))
                    .count();
                json!({
                    "source": source,
                    "name": resource["name"],
                    "messages": readable,
                    "unreadable": total.saturating_sub(readable),
                })
            })
            .collect();
        let selected = channel
            .map(|source| {
                channels
                    .iter()
                    .find(|c| c["source"] == source)
                    .cloned()
                    .ok_or(Error::NotFound)
            })
            .transpose()?;
        let prefix = channel.map(|source| format!("{source}/message/"));
        let readable: Vec<_> = checked
            .messages
            .iter()
            .filter(|m| prefix.as_ref().is_none_or(|p| m.source.starts_with(p)))
            .collect();
        let messages: Vec<_> = readable.iter().skip(offset).take(limit.min(500)).collect();
        Ok(json!({
            "id": id,
            "principal": record.principal,
            "mode": record.mode,
            "objects": record.objects,
            "synced": self.archive_synced(id, &record),
            "snapshot": archive.inventory.snapshot,
            "stage": if checked.partial { "partial" } else { "content_verified" },
            "gaps": checked.gaps,
            "checks": checked.checks.len(),
            "profile": {
                "name": summary.name,
                "username": summary.username,
                "bio": summary.bio,
                "image": summary.image,
                "links": summary.links,
                "extra": summary.extra,
            },
            "files": summary.files,
            "resources": summary.resources,
            "entitlements": summary.entitlements,
            "messages": messages,
            "total_messages": readable.len(),
            "channel": selected,
            "channels": channels,
            "frozen": frozen_view(&archive, &record),
        }))
    }

    /// Verify an old attachment or the old avatar and write it to `out`,
    /// which must not exist yet.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for no such file, `Error::RecoveryIncomplete`
    /// for an incomplete upload or a missing key, `Error::IntegrityFailed`
    /// when it does not authenticate and `Error::InvalidInput` when `out`
    /// exists.
    pub fn legacy_file(&self, id: &str, source: &str, out: &Path) -> Result<Json> {
        ensure_valid(out.is_absolute() && !out.exists(), "out")?;
        let (_, plain) = self.kept_archive(id)?;
        let archive = Archive::decode(&plain)?;
        let file = legacy::open_file(&archive, source)?;
        let name = out
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| dmsg_types::invalid("out"))?;
        let partial = out.with_file_name(format!(".{name}.dmsg-part"));
        let written = std::fs::write(&partial, &*file.bytes)
            .and_then(|_| std::fs::rename(&partial, out))
            .map_err(io);
        if written.is_err() {
            let _ = std::fs::remove_file(&partial);
        }
        written?;
        Ok(json!({
            "name": file.name,
            "size": file.bytes.len(),
            "sha256": hex::encode(*sha256(&file.bytes)),
        }))
    }

    /// Compare a kept archive with the old service's final history in the
    /// frozen-history file at `path` (one or more channels, as the old site
    /// exports them) and record the result per channel.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` without a reviewed legacy freeze, and
    /// errors of [`legacy_proof::compare_frozen`].
    pub async fn legacy_frozen(&mut self, id: &str, path: &Path) -> Result<Json> {
        let legacy = self
            .config
            .legacy
            .clone()
            .ok_or_else(|| Error::Unavailable("legacy freeze is not configured".into()))?;
        let metadata = std::fs::metadata(path).map_err(io)?;
        ensure(metadata.len() <= MAX_FROZEN, Error::QuotaExceeded)?;
        let proofs: Vec<FrozenChannel> = cbor2::from_slice(&std::fs::read(path).map_err(io)?)
            .map_err(|_| Error::IntegrityFailed)?;
        ensure(proofs.len() <= 10_000, Error::QuotaExceeded)?;
        let (mut record, plain) = self.kept_archive(id)?;
        let archive = Archive::decode(&plain)?;
        let root_key = self.reader().await?.root_key();
        let changes = legacy_proof::compare_frozen(
            &archive,
            &proofs,
            &legacy.channels,
            &legacy.buckets,
            &Trust {
                root_key: &root_key,
                cutover: &legacy.cutover,
                now: now_ms(),
            },
        )?;
        for (channel, list) in changes {
            let total = list.len() as u64;
            record.frozen.insert(
                channel,
                (list.into_iter().take(MAX_CHANGES).collect(), total),
            );
        }
        let (row, _) = self.current(id)?;
        self.write(
            MIGRATION,
            id.into(),
            Some(row.revision),
            false,
            &canonical(&record),
            None,
        )?;
        Ok(frozen_view(&archive, &record))
    }
}

/// The frozen comparison so far: channels without frozen evidence count as
/// differences.
fn frozen_view(archive: &Archive, record: &ArchiveRecord) -> Json {
    let mut changes = vec![];
    let mut total = 0;
    for (list, count) in record.frozen.values() {
        changes.extend(list.iter().cloned());
        total += count;
    }
    for object in archive
        .inventory
        .objects
        .iter()
        .filter(|o| o.kind == "channel")
    {
        if !record.frozen.contains_key(&object.key) {
            total += 1;
            changes.push(Change {
                source: object.key.clone(),
                reason: "frozen-proof-missing".into(),
            });
        }
    }
    json!({
        "checked_channels": record.frozen.len(),
        "matched": total == 0 && !record.frozen.is_empty(),
        "total_changes": total,
        "changes": changes,
    })
}

fn io(error: std::io::Error) -> Error {
    Error::InvalidInput(format!("file: {error}"))
}

#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;
