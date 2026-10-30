//! The two sides of a transfer: [`send`] on the device whose content moves
//! and [`receive`] on the device it moves to. The receiver drives: it asks
//! for chunks before the records that reference them, commits each record
//! as it arrives, and keeps the manifest sealed under its LDK, so a new
//! connection with a new key goes on from what is here.

use super::library::{self, Outcome, PHASES};
use super::wire::{self, Body, Channel, Header, Hello, RECEIVER, SENDER};
use super::{
    Category, DOMAIN, Devices, Entry, FORMAT, MAX_ENTRIES, MAX_WANT, Report, SessionId, info,
    manifest_digest,
};
use crate::client::Client;
use crate::codec::{digest, sha256};
use crate::crypto::{HpkeEnvelope, hpke_open_info, hpke_seal_info, random_key, sign};
use crate::files::BlobStore;
use crate::hold::Hold;
use crate::{error_code, now_ms};
use dmsg_protocol::verify;
use dmsg_types::{Ed25519Signature, Error, Hash, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteArray;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use zeroize::Zeroizing;

/// How long a receiver waits for the hello once connected.
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
/// How long either side waits for the other's next frame.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(120);

/// The receiver's sealed records: `transfer:<session>` and the manifest's
/// pages, `transfer:<session>:<page>`.
const PROGRESS: &str = "transfer:";
/// Most manifest entries in a frame or a stored page.
const PAGE: usize = 2048;
/// About the most bytes of a page.
const PAGE_BYTES: usize = 768 * 1024;

/// A session as the receiver keeps it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct Progress {
    from: Hash,
    manifest: Hash,
    pages: u32,
    created_at_ms: u64,
    done_at_ms: Option<u64>,
}

fn progress_key(id: &SessionId) -> String {
    format!("{PROGRESS}{}", hex::encode(id))
}

fn page_key(id: &SessionId, page: u32) -> String {
    format!("{}:{page:06}", progress_key(id))
}

/// The manifest in pages that each fit a frame.
fn pages(entries: &[Entry]) -> Vec<&[Entry]> {
    let mut pages = vec![];
    let (mut start, mut bytes) = (0, 0);
    for (i, entry) in entries.iter().enumerate() {
        let size = entry.key.len() + entry.blobs.len() + 96;
        if i > start && (i - start == PAGE || bytes + size > PAGE_BYTES) {
            pages.push(&entries[start..i]);
            (start, bytes) = (i, 0);
        }
        bytes += size;
    }
    pages.push(&entries[start..]);
    pages
}

impl Client {
    /// The manifest kept for session `id`, when it is the one `header` names
    /// and the session has not finished.
    fn transfer_manifest(&self, id: &SessionId, header: &Header) -> Result<Option<Vec<Entry>>> {
        let keys = self.keys()?;
        let Some(progress) = self
            .workspace
            .private::<Progress>(keys, &progress_key(id))?
            .filter(|p| p.from == header.from && p.manifest == header.manifest)
            .filter(|p| p.done_at_ms.is_none())
        else {
            return Ok(None);
        };
        let mut entries = vec![];
        for page in 0..progress.pages {
            match self
                .workspace
                .private::<Vec<Entry>>(keys, &page_key(id, page))?
            {
                Some(part) => entries.extend(part),
                None => return Ok(None),
            }
        }
        Ok(Some(entries))
    }

    fn drop_transfer_pages(&self, id: &SessionId) -> Result<()> {
        let pages = format!("{}:", progress_key(id));
        for key in self.workspace.store.sealed_keys(&pages)? {
            self.workspace.delete_private(&key)?;
        }
        Ok(())
    }

    fn save_transfer(&self, id: &SessionId, header: &Header, entries: &[Entry]) -> Result<()> {
        let keys = self.keys()?;
        self.drop_transfer_pages(id)?;
        let parts = pages(entries);
        for (page, part) in parts.iter().enumerate() {
            self.workspace
                .put_private(keys, &page_key(id, page as u32), part)?;
        }
        let progress = Progress {
            from: header.from,
            manifest: header.manifest,
            pages: parts.len() as u32,
            created_at_ms: now_ms(),
            done_at_ms: None,
        };
        self.workspace
            .put_private(keys, &progress_key(id), &progress)
    }

    fn finish_transfer(&self, id: &SessionId) -> Result<()> {
        let keys = self.keys()?;
        self.drop_transfer_pages(id)?;
        let Some(mut progress) = self
            .workspace
            .private::<Progress>(keys, &progress_key(id))?
        else {
            return Ok(());
        };
        progress.pages = 0;
        progress.done_at_ms = Some(now_ms());
        self.workspace
            .put_private(keys, &progress_key(id), &progress)
    }
}

/// The error for a frame out of place: the other side stopping, or a
/// broken protocol.
fn stopped(body: Body) -> Error {
    match body {
        Body::Abort { code } => {
            Error::Unavailable(format!("transfer: the other device stopped: {code}"))
        }
        _ => Error::IntegrityFailed,
    }
}

/// Tell the other side why this one stops, when the connection still
/// works.
async fn abort<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
    result: &Result<Report>,
) {
    if let Err(error) = result
        && !matches!(error, Error::Unavailable(_))
    {
        let code = error_code(error);
        let _ = channel.send(&Body::Abort { code }).await;
    }
}

fn publish(report: &Mutex<Report>, now: &Report) {
    *report.lock().unwrap_or_else(PoisonError::into_inner) = now.clone();
}

/// Offer this device's library to device `to` on its receiving session
/// `id`, over `stream`, which the host connected; `report` follows the
/// progress. Returns what was sent once the receiver confirmed it went
/// through every entry it chose.
///
/// # Errors
/// Returns `Error::DeviceNotApproved` when either device is not active,
/// `Error::IntegrityFailed` for a frame that does not open or is out of
/// place, `Error::Forbidden` for a request beyond the manifest, and
/// `Error::Unavailable` when the connection fails or the receiver stops.
pub async fn send<H, D, S>(
    h: &H,
    devices: &D,
    stream: S,
    to: Hash,
    id: SessionId,
    report: &Mutex<Report>,
) -> Result<Report>
where
    H: Hold,
    D: Devices,
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    send_inner(h, devices, stream, to, id, report, None).await
}

/// Send the exact library approved in the host's approval center.
///
/// # Errors
/// As [`send`], plus `Error::Locked` after the original unlock session ends
/// or `Error::VersionConflict` if the reviewed library changed.
pub async fn send_approved<H, D, S>(
    h: &H,
    devices: &D,
    stream: S,
    permit: super::Permit,
    report: &Mutex<Report>,
) -> Result<Report>
where
    H: Hold,
    D: Devices,
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    send_inner(
        h,
        devices,
        stream,
        permit.to,
        permit.session,
        report,
        Some(permit),
    )
    .await
}

async fn send_inner<H, D, S>(
    h: &H,
    devices: &D,
    mut stream: S,
    to: Hash,
    id: SessionId,
    report: &Mutex<Report>,
    permit: Option<super::Permit>,
) -> Result<Report>
where
    H: Hold,
    D: Devices,
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let active = devices.active().await?;
    let (hello, key, entries, blobs) = h
        .with(|c| {
            if let Some(permit) = &permit {
                ensure(
                    c.session_generation == permit.generation
                        && c.workspace.meta.device_id == permit.device,
                    Error::Locked,
                )?;
            }
            let me = c.workspace.meta.device_id;
            ensure(
                to != me,
                Error::InvalidInput("the receiver is this device".into()),
            )?;
            ensure(active.contains_key(&me), Error::DeviceNotApproved)?;
            let receiver = active.get(&to).ok_or(Error::DeviceNotApproved)?;
            let entries = library::entries(c)?;
            ensure(entries.len() <= MAX_ENTRIES, Error::QuotaExceeded)?;
            if let Some(permit) = &permit {
                ensure(
                    manifest_digest(&entries.iter().collect::<Vec<_>>()) == permit.manifest,
                    Error::VersionConflict,
                )?;
            }
            let account = c.account()?.id;
            let key = Zeroizing::new(random_key());
            let sealed = hpke_seal_info(
                &receiver.hpke_pub,
                key.as_slice(),
                &info(&account, &me, &to, &id),
            )?;
            let header = Header {
                format: FORMAT,
                environment: c.workspace.meta.environment.clone(),
                account,
                from: me,
                to,
                session_id: ByteArray::new(id),
                enc: sealed.enc.into(),
                key: sealed.ciphertext.into(),
                manifest: manifest_digest(&entries.iter().collect::<Vec<_>>()),
            };
            let signature =
                Ed25519Signature::new(sign(&c.keys()?.signing, digest(DOMAIN, &header).as_slice()));
            Ok((
                Hello { header, signature },
                key,
                entries,
                c.blobs()?.clone(),
            ))
        })
        .await?;
    wire::write_hello(&mut stream, &hello).await?;
    let mut channel = Channel::new(stream, key, id, SENDER);
    let result = async {
        let me = hello.header.from;
        let index: BTreeMap<&str, &Entry> = entries.iter().map(|e| (e.key.as_str(), e)).collect();
        let referenced: BTreeSet<Hash> = entries.iter().flat_map(Entry::digests).collect();
        let mut sent = Report {
            peer: Some(to),
            entries: entries.len(),
            total_bytes: entries.iter().map(|e| e.blob_bytes).sum(),
            ..Report::default()
        };
        publish(report, &sent);
        match channel.recv().await? {
            Body::Have { manifest } if manifest == Some(hello.header.manifest) => {}
            Body::Have { .. } => {
                let parts = pages(&entries);
                let last = parts.len() - 1;
                for (page, part) in parts.into_iter().enumerate() {
                    let body = Body::Manifest {
                        entries: part.to_vec(),
                        last: page == last,
                    };
                    channel.send(&body).await?;
                }
            }
            body => return Err(stopped(body)),
        }
        loop {
            match channel.recv().await? {
                Body::Want {
                    records,
                    blobs: wanted,
                } => {
                    ensure(
                        records.len() + wanted.len() <= MAX_WANT,
                        Error::IntegrityFailed,
                    )?;
                    for digest in wanted {
                        ensure(referenced.contains(&digest), Error::Forbidden)?;
                        let bytes = blobs.chunk(&hex::encode(*digest))?.ok_or(Error::NotFound)?;
                        sent.blobs += 1;
                        sent.bytes += bytes.len() as u64;
                        let body = Body::Blob {
                            digest,
                            bytes: bytes.into(),
                        };
                        channel.send(&body).await?;
                    }
                    for key in records {
                        ensure(index.contains_key(key.as_str()), Error::Forbidden)?;
                        // A record gone since the manifest goes out empty.
                        let value = h
                            .with(|c| {
                                let value = c.workspace.open_sealed(c.keys()?, &key)?;
                                Ok(value.map(|v| v.to_vec()).unwrap_or_default())
                            })
                            .await?;
                        sent.records += 1;
                        let body = Body::Record {
                            key,
                            value: value.into(),
                        };
                        channel.send(&body).await?;
                    }
                    publish(report, &sent);
                }
                Body::Done {
                    categories,
                    committed,
                } => {
                    let chosen: Vec<&Entry> = entries
                        .iter()
                        .filter(|e| e.category().is_some_and(|c| categories.contains(&c)))
                        .collect();
                    ensure(
                        manifest_digest(&chosen) == committed,
                        Error::IntegrityFailed,
                    )?;
                    let active = devices.active().await?;
                    ensure(
                        active.contains_key(&me) && active.contains_key(&to),
                        Error::DeviceNotApproved,
                    )?;
                    sent.pending_sync = h
                        .with(|c| c.workspace.store.count_in(crate::store::LOCAL))
                        .await?;
                    publish(report, &sent);
                    channel
                        .send(&Body::Ack {
                            pending_sync: sent.pending_sync,
                        })
                        .await?;
                    return Ok(sent);
                }
                body => return Err(stopped(body)),
            }
        }
    }
    .await;
    abort(&mut channel, &result).await;
    result
}

/// Receive content of `categories` into this device on its session `id`,
/// from the device the hello on `stream` names; `report` follows the
/// progress. Returns what was committed. A session broken before the end
/// resumes on a new connection, skipping what is here.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for a hello of another account,
/// environment, device or session, a bad signature or key, a forged
/// manifest or a frame that does not open or is out of place;
/// `Error::DeviceNotApproved` when either device is not active; and
/// `Error::Unavailable` when the connection fails or the sender stops.
pub async fn receive<H, D, S>(
    h: &H,
    devices: &D,
    mut stream: S,
    id: SessionId,
    categories: &[Category],
    report: &Mutex<Report>,
) -> Result<Report>
where
    H: Hold,
    D: Devices,
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let hello = tokio::time::timeout(HELLO_TIMEOUT, wire::read_hello(&mut stream))
        .await
        .map_err(|_| Error::Unavailable("transfer: no hello in time".into()))??;
    let header = hello.header;
    let active = devices.active().await?;
    let (key, stored, me) = h
        .with(|c| {
            let me = c.workspace.meta.device_id;
            ensure(
                header.format == FORMAT
                    && header.environment == c.workspace.meta.environment
                    && header.account == c.account()?.id
                    && header.to == me
                    && header.from != me
                    && *header.session_id == id,
                Error::IntegrityFailed,
            )?;
            ensure(active.contains_key(&me), Error::DeviceNotApproved)?;
            let sender = active.get(&header.from).ok_or(Error::DeviceNotApproved)?;
            verify(
                &sender.signing_pub,
                digest(DOMAIN, &header).as_slice(),
                hello.signature.as_slice(),
            )?;
            let envelope = HpkeEnvelope {
                enc: header.enc.to_vec(),
                ciphertext: header.key.to_vec(),
            };
            let opened = hpke_open_info(
                &c.keys()?.hpke,
                &envelope,
                &info(&header.account, &header.from, &me, &id),
            )?;
            let key: [u8; 32] = opened
                .as_slice()
                .try_into()
                .map_err(|_| Error::IntegrityFailed)?;
            Ok((Zeroizing::new(key), c.transfer_manifest(&id, &header)?, me))
        })
        .await?;
    let mut channel = Channel::new(stream, key, id, RECEIVER);
    let result = async {
        let have = Body::Have {
            manifest: stored.as_ref().map(|_| header.manifest),
        };
        channel.send(&have).await?;
        let entries = match stored {
            Some(entries) => entries,
            None => {
                let mut entries: Vec<Entry> = vec![];
                loop {
                    match channel.recv().await? {
                        Body::Manifest {
                            entries: part,
                            last,
                        } => {
                            ensure(
                                entries.len() + part.len() <= MAX_ENTRIES,
                                Error::QuotaExceeded,
                            )?;
                            entries.extend(part);
                            if last {
                                break;
                            }
                        }
                        body => return Err(stopped(body)),
                    }
                }
                ensure(
                    manifest_digest(&entries.iter().collect::<Vec<_>>()) == header.manifest,
                    Error::IntegrityFailed,
                )?;
                // In name order, each once, each transferred, with whole
                // digests.
                ensure(
                    entries.windows(2).all(|w| w[0].key < w[1].key)
                        && entries
                            .iter()
                            .all(|e| e.category().is_some() && e.blobs.len() % 32 == 0),
                    Error::IntegrityFailed,
                )?;
                h.with(|c| c.save_transfer(&id, &header, &entries)).await?;
                entries
            }
        };
        let chosen: Vec<&Entry> = entries
            .iter()
            .filter(|e| e.category().is_some_and(|c| categories.contains(&c)))
            .collect();
        let mut got = Report {
            peer: Some(header.from),
            entries: chosen.len(),
            total_bytes: chosen.iter().map(|e| e.blob_bytes).sum(),
            ..Report::default()
        };
        publish(report, &got);
        let blobs = h.with(|c| Ok(c.blobs()?.clone())).await?;
        for phase in PHASES {
            let named: Vec<&Entry> = chosen
                .iter()
                .copied()
                .filter(|e| e.key.starts_with(phase))
                .collect();
            let held: Vec<bool> = h
                .with(|c| named.iter().map(|e| library::held(c, &e.key)).collect())
                .await?;
            let todo: Vec<&Entry> = named
                .iter()
                .zip(held)
                .filter_map(|(entry, held)| (!held).then_some(*entry))
                .collect();
            got.kept += named.len() - todo.len();
            for batch in todo.chunks(MAX_WANT) {
                let mut missing = BTreeSet::new();
                for digest in batch.iter().flat_map(|e| e.digests()) {
                    if !blobs.has_chunk(&hex::encode(*digest))? {
                        missing.insert(digest);
                    }
                }
                let missing: Vec<Hash> = missing.into_iter().collect();
                for wanted in missing.chunks(MAX_WANT) {
                    let want = Body::Want {
                        records: vec![],
                        blobs: wanted.to_vec(),
                    };
                    channel.send(&want).await?;
                    for expected in wanted {
                        match channel.recv().await? {
                            Body::Blob { digest, bytes } if digest == *expected => {
                                blobs.put_chunk(&hex::encode(*digest), &bytes)?;
                                got.blobs += 1;
                                got.bytes += bytes.len() as u64;
                            }
                            body => return Err(stopped(body)),
                        }
                    }
                    publish(report, &got);
                }
                let want = Body::Want {
                    records: batch.iter().map(|e| e.key.clone()).collect(),
                    blobs: vec![],
                };
                channel.send(&want).await?;
                for entry in batch {
                    match channel.recv().await? {
                        Body::Record { key, value } if key == entry.key => {
                            // Changed or gone on the sender since the
                            // manifest: left for another transfer.
                            if sha256(&value) != entry.sha256 || value.len() as u64 != entry.size {
                                got.skipped += 1;
                                continue;
                            }
                            match h.with(|c| library::commit(c, &key, &value)).await? {
                                Outcome::Added => got.records += 1,
                                Outcome::Kept => got.kept += 1,
                                Outcome::Skipped => got.skipped += 1,
                            }
                        }
                        body => return Err(stopped(body)),
                    }
                }
                publish(report, &got);
            }
        }
        let active = devices.active().await?;
        ensure(
            active.contains_key(&me) && active.contains_key(&header.from),
            Error::DeviceNotApproved,
        )?;
        // A digest of the offered entries is not evidence that skipped
        // records arrived. Keep the session resumable until all do.
        ensure(got.skipped == 0, Error::RecoveryIncomplete)?;
        let done = Body::Done {
            categories: categories.to_vec(),
            committed: manifest_digest(&chosen),
        };
        channel.send(&done).await?;
        match channel.recv().await? {
            Body::Ack { pending_sync } => got.pending_sync = pending_sync,
            body => return Err(stopped(body)),
        }
        h.with(|c| c.finish_transfer(&id)).await?;
        publish(report, &got);
        Ok(got)
    }
    .await;
    abort(&mut channel, &result).await;
    result
}

/// Receive on `listener` for session `id`, one connection at a time, each
/// as [`receive`] does, until one completes. A connection that fails, from
/// a device that is not the sender or one that broke, leaves the session
/// listening with its error in the report; the session ends when no
/// connection comes for [`IDLE_TIMEOUT`]. The host stops it when the device
/// locks.
///
/// # Errors
/// Returns `Error::Unavailable` when no sender came in time or the
/// listener fails, and `Error::Locked` or `Error::AuthRequired` once the
/// session's account is no longer unlocked.
pub async fn serve<H, D>(
    h: &H,
    devices: &D,
    listener: &TcpListener,
    id: SessionId,
    categories: &[Category],
    report: &Mutex<Report>,
) -> Result<Report>
where
    H: Hold,
    D: Devices,
{
    loop {
        let (stream, _) = tokio::time::timeout(IDLE_TIMEOUT, listener.accept())
            .await
            .map_err(|_| Error::Unavailable("transfer: no sender came in time".into()))?
            .map_err(|e| Error::Unavailable(format!("transfer: {e}")))?;
        match receive(h, devices, stream, id, categories, report).await {
            Ok(done) => return Ok(done),
            Err(error @ (Error::Locked | Error::AuthRequired)) => return Err(error),
            Err(error) => {
                report
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .last_error = Some(error_code(&error));
            }
        }
    }
}
