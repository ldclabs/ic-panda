//! `dmsg-device-transfer/1`: a one-time, end-to-end encrypted transfer of
//! local content from one active device of an account to another over a
//! local connection, as [docs/protocol/device_transfer.md] describes.
//!
//! The receiver opens a session and waits; the sender connects, wraps a new
//! session key to the receiver's HPKE key and signs a header that names both
//! devices and the manifest. Every later frame is AES-256-GCM under that key
//! with a nonce of its direction and sequence number. The receiver asks for
//! what it lacks, stores chunks first and commits a record only once every
//! chunk it references is here; it only adds, never replaces a record it
//! has or deletes one. The manifest it received is sealed under its LDK, so
//! a broken connection resumes with a new key. The sender changes nothing.
//!
//! The transport is the host's: any `AsyncRead + AsyncWrite`, a TCP
//! connection found through Bonjour ([`SERVICE`], [`advert`]) in the apps,
//! which keep their one transfer at a time in [`Transfers`].
//!
//! [docs/protocol/device_transfer.md]: ../../../../docs/protocol/device_transfer.md

mod hosted;
pub(crate) mod library;
mod session;
mod wire;

use crate::codec::digest;
use crate::hold::Hold;
use dmsg_types::user::DeviceInput;
use dmsg_types::{AccountId, Hash, Result};
use serde::{Deserialize, Serialize, Serializer};
use serde_bytes::{ByteBuf, Bytes};
use std::collections::BTreeMap;
use std::future::Future;

pub use hosted::{Start, Ticket, Transfers, listen, local_address};
pub use session::{HELLO_TIMEOUT, IDLE_TIMEOUT, receive, send, serve};
pub use wire::{Body, Header, Hello, MAX_BODY};

/// The format a header names.
pub const FORMAT: u8 = 1;
/// The Bonjour service type a receiving host advertises.
pub const SERVICE: &str = "_dmsg-xfer._tcp";
/// The domain of the header signature, the HPKE info and the frames' AAD.
pub(crate) const DOMAIN: &str = "dmsg/device-transfer/v1";
const ADVERT: &str = "dmsg/device-transfer/advert/v1";
const MANIFEST: &str = "dmsg/device-transfer/manifest/v1";
/// Records and chunks one request asks for.
pub const MAX_WANT: usize = 256;
/// Entries a manifest lists.
pub const MAX_ENTRIES: usize = 1_000_000;

/// A receiving session's ID, random.
pub type SessionId = [u8; 16];

/// A new receiving session's ID.
///
/// # Panics
/// Panics if the operating system has no random source.
pub fn session_id() -> SessionId {
    let mut id = [0; 16];
    getrandom::fill(&mut id).expect("operating system random source");
    id
}

/// The Bonjour instance name a receiver advertises session `id` under: the
/// first 8 bytes of its tag, in hex. Only a device that knows the ID finds
/// the session.
pub fn advert(id: &SessionId) -> String {
    hex::encode(&digest(ADVERT, &Bytes::new(id))[..8])
}

/// What a transfer moves; the receiver chooses.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// dMsg Files: folders, files, versions and their sealed chunks.
    Library,
}

impl Category {
    /// Every category.
    pub const ALL: [Category; 1] = [Category::Library];
}

/// Where a workspace record goes in a transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Transferred in its category.
    Transfer(Category),
    /// Stays on the sending device: its keys, its own state, journals of
    /// what it sent, or what the account's cloud holds.
    Stays,
}

/// Every record name prefix the client writes and where it goes; a prefix
/// not listed stays. A test fails when a new prefix is missing here.
pub const PREFIXES: &[(&str, Class)] = &[
    ("lib-folder:", Class::Transfer(Category::Library)),
    ("lib-file:", Class::Transfer(Category::Library)),
    ("lib-version:", Class::Transfer(Category::Library)),
    // Plain-text opened copies, import staging, cloud copies (read again
    // from the account), share links and backup jobs of this device.
    ("lib-open:", Class::Stays),
    ("lib-import:", Class::Stays),
    ("lib-cloud:", Class::Stays),
    ("lib-link:", Class::Stays),
    ("backup:", Class::Stays),
    ("mobile-transfer:", Class::Stays),
    // Wallet jobs, quotes, drafts and activations bind this device's
    // wallet key; payer and checkout journals record what it sent. The old
    // device settles them before it is revoked.
    ("wallet-job:", Class::Stays),
    ("wallet-quote:", Class::Stays),
    ("wallet-proposal:", Class::Stays),
    ("wallet-proposal-result:", Class::Stays),
    ("wallet-activation:", Class::Stays),
    // Native Taproot keys derive from this device's own root; the moves to
    // them, their sweeps and archived addresses are this device's record.
    ("wallet-native:", Class::Stays),
    ("wallet-nc-migrate:", Class::Stays),
    ("wallet-sweep:", Class::Stays),
    ("wallet-archived:", Class::Stays),
    ("payer:", Class::Stays),
    ("checkout:", Class::Stays),
    ("usage:", Class::Stays),
    // Channels and the inbox sync through the relay.
    ("channel:", Class::Stays),
    ("channel-msg:", Class::Stays),
    ("channel-job:", Class::Stays),
    ("channel-control:", Class::Stays),
    ("inbox-job:", Class::Stays),
    ("inbox-letter:", Class::Stays),
    ("letter-intent:", Class::Stays),
    ("send-intent:", Class::Stays),
    // Approvals, attestations, connections and grants of this device.
    ("approval:", Class::Stays),
    ("attest:", Class::Stays),
    ("connect-invite:", Class::Stays),
    ("connect-op:", Class::Stays),
    ("binding:", Class::Stays),
    ("delegation:", Class::Stays),
    ("delegation-nonce:", Class::Stays),
    ("handle:", Class::Stays),
    ("agent-mail-keys:", Class::Stays),
    ("agent-mail-push:", Class::Stays),
    ("secret-package-import:", Class::Stays),
    ("secret-scan:", Class::Stays),
    ("secret-share:", Class::Stays),
    ("safety-report:", Class::Stays),
    ("legacy-pair:", Class::Stays),
    ("recover-root:", Class::Stays),
    ("succession:", Class::Stays),
    ("transfer:", Class::Stays),
];

/// Where record `key` goes.
pub fn classify(key: &str) -> Class {
    PREFIXES
        .iter()
        .find(|(prefix, _)| key.starts_with(prefix))
        .map_or(Class::Stays, |(_, class)| *class)
}

/// A record the sender offers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The record's name, such as `lib-file:<id>`.
    pub key: String,
    /// SHA-256 of its canonical CBOR.
    pub sha256: Hash,
    /// Its length.
    pub size: u64,
    /// The SHA-256 of each sealed chunk it references, 32 bytes each.
    pub blobs: ByteBuf,
    /// The chunks' total length.
    pub blob_bytes: u64,
}

impl Entry {
    /// The chunks it references.
    pub fn digests(&self) -> impl Iterator<Item = Hash> + '_ {
        self.blobs.as_chunks::<32>().0.iter().map(|d| Hash::new(*d))
    }

    /// Its category, if it is transferred at all.
    pub fn category(&self) -> Option<Category> {
        match classify(&self.key) {
            Class::Transfer(category) => Some(category),
            Class::Stays => None,
        }
    }
}

/// The digest a header names and [`Body::Done`] confirms: of every entry in
/// manifest order.
pub fn manifest_digest(entries: &[&Entry]) -> Hash {
    digest(MANIFEST, &entries)
}

/// The HPKE info that binds the session key to the account, both devices
/// and the session.
pub(crate) fn info(account: &AccountId, from: &Hash, to: &Hash, id: &SessionId) -> Vec<u8> {
    crate::codec::canonical(&(DOMAIN, account, from, to, Bytes::new(id)))
}

/// How far a transfer got, on either side.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// The other device, once its hello named it.
    #[serde(serialize_with = "hex_device")]
    pub peer: Option<Hash>,
    /// Entries of the chosen categories.
    pub entries: usize,
    /// Records added or merged here, or sent from here.
    pub records: usize,
    /// Records this device had and kept as they were.
    pub kept: usize,
    /// Records left out: one that changed on the sender meanwhile, or a
    /// file none of whose versions arrived.
    pub skipped: usize,
    /// Chunks stored here, or sent from here.
    pub blobs: usize,
    /// Their bytes.
    pub bytes: u64,
    /// The chunks' total bytes in the chosen categories.
    pub total_bytes: u64,
    /// Why the last connection of a listening session failed.
    pub last_error: Option<String>,
}

fn hex_device<S: Serializer>(peer: &Option<Hash>, s: S) -> std::result::Result<S::Ok, S::Error> {
    peer.map(|p| hex::encode(*p)).serialize(s)
}

/// The account's certified devices, which each side reads when the session
/// starts and before it finishes.
pub trait Devices: Sync {
    /// The active devices, by ID.
    fn active(&self) -> impl Future<Output = Result<BTreeMap<Hash, DeviceInput>>> + Send;
}

/// [`Devices`] read from the account's user home.
pub struct Certified<'a, H>(pub &'a H);

impl<H: Hold> Devices for Certified<'_, H> {
    async fn active(&self) -> Result<BTreeMap<Hash, DeviceInput>> {
        let state = crate::account::refreshed(self.0).await?;
        Ok(state
            .info
            .devices
            .into_iter()
            .filter(|(_, d)| d.revoked_at.is_none())
            .map(|(id, d)| (id, d.input))
            .collect())
    }
}

#[cfg(test)]
mod tests;
