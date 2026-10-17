//! Legacy migration formats (`docs/protocol/legacy_archive_zh.md`): the
//! `dmsg-legacy-pair/2` offer a device makes, the `dmsg-legacy-transfer/1`
//! file the original site's `/legacy` page seals to it, and the
//! `dmsg-legacy-archive/1` that file carries.
//!
//! An archive is checked offline: every old message, attachment and avatar
//! is opened again with the archived historical keys, and the exporter's
//! own checks are only compared against the result. Old authorship and
//! times stay claims of the old service. A scoped archive (shared history)
//! keeps one channel's content and its two channel keys, nothing else.

use crate::codec::{b64, canonical, hex_sha256, principal};
use crate::crypto::{HpkeEnvelope, Key, aes_gcm_open, hpke_open_info, hpke_public, random_id};
use candid::Principal;
use cbor2::Value;
use dmsg_types::{Error, Result, ensure, ensure_valid};
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteBuf, Bytes};
use std::collections::{BTreeMap, BTreeSet};
use zeroize::{Zeroize, Zeroizing};

/// Pairing offer format.
pub const PAIR: &str = "dmsg-legacy-pair/2";
/// Encrypted transfer file format.
pub const TRANSFER: &str = "dmsg-legacy-transfer/1";
/// Archive format.
pub const ARCHIVE: &str = "dmsg-legacy-archive/1";
/// Largest archive, and largest transfer file including its framing.
pub const MAX_ARCHIVE: usize = 256 * 1024 * 1024;
/// Plaintext bytes per transfer frame.
pub const TRANSFER_CHUNK: usize = 192 * 1024;
/// How long a new offer stays valid.
pub const PAIR_MS: u64 = 10 * 60 * 1000;
/// The two origins of the old site.
pub const ORIGINS: [&str; 2] = ["https://dmsg.net", "https://panda.fans"];

const MAX_TEXT: usize = 2048;
const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 2_000_000;
const MAX_ITEMS: u64 = 100_000;
const MAX_AVATAR: usize = 2 * 1024 * 1024;

fn is_hash(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// JavaScript string length, which the old schema bounds.
pub(crate) fn js_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// JavaScript's default string order: UTF-16 code units.
pub(crate) fn js_order(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// Why one source object could not be checked.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    /// The old service could not be reached.
    Network,
    /// The old identity may not read it.
    Permission,
    /// It does not exist, or only partly.
    Missing,
    /// Its historical key is not in the archive.
    MissingKey,
    /// An unknown old format.
    Version,
    /// It changed or does not authenticate.
    Corrupt,
    /// It exceeds a bound.
    Limit,
    /// Collection stopped.
    Cancelled,
}

impl Failure {
    fn name(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Permission => "permission",
            Self::Missing => "missing",
            Self::MissingKey => "missing_key",
            Self::Version => "version",
            Self::Corrupt => "corrupt",
            Self::Limit => "limit",
            Self::Cancelled => "cancelled",
        }
    }
}

/// A failure while reading old content, with its reason.
#[derive(Debug)]
struct Fault(Failure, &'static str);

type Checked<T> = std::result::Result<T, Fault>;

fn fault<T>(failure: Failure, detail: &'static str) -> Checked<T> {
    Err(Fault(failure, detail))
}

// ---------------------------------------------------------------------------
// Bounded CBOR as the old site writes it.

/// Check one CBOR item of the old formats: definite lengths, no tags or
/// floats, at most 32 levels, 2,000,000 items and 100,000 entries per
/// container, no repeated map key and nothing after it.
fn scan(bytes: &[u8], max: usize) -> Result<()> {
    ensure(bytes.len() <= max, Error::QuotaExceeded)?;
    let mut scanner = Scanner {
        data: bytes,
        offset: 0,
        nodes: 0,
    };
    scanner.item(0)?;
    ensure(scanner.offset == bytes.len(), Error::IntegrityFailed)
}

/// Decode a [`scan`]ned item.
fn unpack(bytes: &[u8], max: usize) -> Result<Value> {
    unpack_as(bytes, max)
}

/// Decode a [`scan`]ned item into a typed value; unknown fields are refused.
fn unpack_as<T: for<'de> Deserialize<'de>>(bytes: &[u8], max: usize) -> Result<T> {
    scan(bytes, max)?;
    cbor2::from_slice(bytes).map_err(|_| Error::IntegrityFailed)
}

struct Scanner<'a> {
    data: &'a [u8],
    offset: usize,
    nodes: usize,
}

impl Scanner<'_> {
    fn header(&mut self) -> Result<(u8, u8, u64)> {
        let first = *self.data.get(self.offset).ok_or(Error::IntegrityFailed)?;
        self.offset += 1;
        let (major, info) = (first >> 5, first & 31);
        ensure(info < 28 && major != 6, Error::UnsupportedProtocol)?;
        if info < 24 {
            return Ok((major, info, info.into()));
        }
        let bytes = self.take(1 << (info - 24))?;
        Ok((
            major,
            info,
            bytes.iter().fold(0, |n, b| (n << 8) | u64::from(*b)),
        ))
    }

    fn take(&mut self, size: u64) -> Result<&[u8]> {
        let remaining = self.data.len() - self.offset;
        ensure(size <= remaining as u64, Error::IntegrityFailed)?;
        let start = self.offset;
        self.offset += size as usize;
        Ok(&self.data[start..self.offset])
    }

    fn item(&mut self, depth: usize) -> Result<()> {
        self.nodes += 1;
        ensure(
            depth <= MAX_DEPTH && self.nodes <= MAX_NODES,
            Error::QuotaExceeded,
        )?;
        let (major, info, argument) = self.header()?;
        match major {
            0 | 1 => Ok(()),
            2 | 3 => self.take(argument).map(|_| ()),
            4 => {
                ensure(argument <= MAX_ITEMS, Error::QuotaExceeded)?;
                (0..argument).try_for_each(|_| self.item(depth + 1))
            }
            5 => {
                ensure(argument <= MAX_ITEMS, Error::QuotaExceeded)?;
                let mut keys = BTreeSet::new();
                for _ in 0..argument {
                    let start = self.offset;
                    self.item(depth + 1)?;
                    ensure(
                        keys.insert(&self.data[start..self.offset]),
                        Error::IntegrityFailed,
                    )?;
                    self.item(depth + 1)?;
                }
                Ok(())
            }
            _ => ensure(matches!(info, 20..=22), Error::UnsupportedProtocol),
        }
    }
}

// ---------------------------------------------------------------------------
// Observations: the archive's tagged tree of old Candid values.

fn field<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    match value {
        Value::Map(entries) => entries
            .iter()
            .find(|(k, _)| matches!(k, Value::Text(t) if t == name))
            .map(|(_, v)| v),
        _ => None,
    }
}

/// The only entry of a `{bytes}`, `{integer}`, `{principal}` or `{map}` wrapper.
fn wrapped<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    match value {
        Value::Map(entries) if entries.len() == 1 => field(value, name),
        _ => None,
    }
}

/// Bytes, as a byte string, a `{bytes}` wrapper or an array of octets.
fn bytes_of(value: &Value) -> Option<Vec<u8>> {
    match value {
        Value::Bytes(b) => Some(b.clone()),
        Value::Array(items) => items
            .iter()
            .map(|i| match i {
                Value::Integer(n) => u8::try_from(i128::from(*n)).ok(),
                _ => None,
            })
            .collect(),
        _ => match wrapped(value, "bytes") {
            Some(Value::Bytes(b)) => Some(b.clone()),
            _ => None,
        },
    }
}

/// An integer, plain or as an `{integer}` wrapper of its decimal text.
fn integer_of(value: &Value) -> Option<i128> {
    match value {
        Value::Integer(n) => Some(i128::from(*n)),
        _ => match wrapped(value, "integer") {
            Some(Value::Text(t)) => t.parse().ok(),
            _ => None,
        },
    }
}

/// The text of a `{principal}` wrapper.
fn principal_of(value: &Value) -> Option<String> {
    match wrapped(value, "principal") {
        Some(Value::Text(t)) => Some(t.clone()),
        _ => None,
    }
}

fn text_of(value: &Value) -> Option<&str> {
    match value {
        Value::Text(t) => Some(t),
        _ => None,
    }
}

/// The bytes of field `name` of an object's observation.
pub(crate) fn observation_value(object: &SourceObject, name: &str) -> Option<Vec<u8>> {
    unpack(&object.bytes, MAX_ARCHIVE)
        .ok()
        .and_then(|v| field(&v, name).and_then(bytes_of))
}

/// The value as the old app showed it: an integer's decimal digits, a
/// Principal's text, otherwise the text itself.
fn display(value: Option<&Value>) -> String {
    match value {
        Some(v) => integer_of(v)
            .map(|n| n.to_string())
            .or_else(|| principal_of(v))
            .or_else(|| text_of(v).map(str::to_string))
            .unwrap_or_default(),
        None => String::new(),
    }
}

/// An observation as JSON for display: integers as decimal text,
/// Principals as text and bytes as hex.
pub fn observation_json(value: &Value) -> serde_json::Value {
    use serde_json::Value as Json;
    if let Some(n) = integer_of(value).filter(|_| !matches!(value, Value::Integer(_))) {
        return Json::String(n.to_string());
    }
    if let Some(p) = principal_of(value) {
        return Json::String(p);
    }
    if let Some(Value::Bytes(b)) = wrapped(value, "bytes") {
        return Json::String(hex::encode(b));
    }
    match value {
        Value::Integer(n) => Json::String(i128::from(*n).to_string()),
        Value::Bytes(b) => Json::String(hex::encode(b)),
        Value::Text(t) => Json::String(t.clone()),
        Value::Bool(b) => Json::Bool(*b),
        Value::Array(items) => Json::Array(items.iter().map(observation_json).collect()),
        Value::Map(entries) => match wrapped(value, "map") {
            Some(Value::Array(pairs)) => Json::Array(
                pairs
                    .iter()
                    .map(|p| match p {
                        Value::Array(kv) => Json::Array(kv.iter().map(observation_json).collect()),
                        other => observation_json(other),
                    })
                    .collect(),
            ),
            _ => Json::Object(
                entries
                    .iter()
                    .map(|(k, v)| (display(Some(k)), observation_json(v)))
                    .collect(),
            ),
        },
        _ => Json::Null,
    }
}

// ---------------------------------------------------------------------------
// Pairing and the transfer file.

/// The offer a device opens on the old site, at `<origin>/legacy#pair=…`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Offer {
    /// [`PAIR`].
    pub version: String,
    /// Random 32 bytes, hex; names the pairing.
    pub nonce: String,
    /// The one-time HPKE receiving key, hex.
    pub public_key: String,
    /// The old site, one of [`ORIGINS`].
    pub origin: String,
    /// The receiving device's ID, hex.
    pub target: String,
    /// Unix milliseconds.
    pub expires_at: u64,
}

impl Offer {
    /// SHA-256 of the offer's CBOR, hex; both sides show it.
    pub fn fingerprint(&self) -> String {
        hex_sha256(&canonical(self))
    }

    /// The page link with the offer in its fragment, which no server sees.
    pub fn link(&self) -> String {
        format!("{}/legacy#pair={}", self.origin, b64(&canonical(self)))
    }

    /// The offer is well formed, for `origin` and `target`, and valid at `now`.
    fn check(&self, origin: &str, target: &str, now: u64) -> Result<()> {
        ensure(
            self.version == PAIR
                && is_hash(&self.nonce)
                && is_hash(&self.public_key)
                && is_hash(&self.target)
                && ORIGINS.contains(&self.origin.as_str()),
            Error::IntegrityFailed,
        )?;
        ensure(
            self.origin == origin && self.target == target,
            Error::Forbidden,
        )?;
        ensure(
            self.expires_at > now && self.expires_at <= now + PAIR_MS,
            Error::Expired,
        )
    }
}

/// A new offer from `device` for the old site at `origin`, and the seed of
/// its receiving key.
///
/// # Errors
/// Returns `Error::InvalidInput` for another origin or device ID.
pub fn new_offer(origin: &str, device: &str, now: u64) -> Result<(Offer, Zeroizing<Key>)> {
    ensure_valid(ORIGINS.contains(&origin), "origin")?;
    ensure_valid(is_hash(device), "device")?;
    let seed = Zeroizing::new(random_id());
    let offer = Offer {
        version: PAIR.into(),
        nonce: hex::encode(random_id()),
        public_key: hex::encode(hpke_public(&seed)),
        origin: origin.into(),
        target: device.into(),
        expires_at: now + PAIR_MS,
    };
    Ok((offer, seed))
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct Frame {
    index: u64,
    previous: String,
    enc: ByteBuf,
    ciphertext: ByteBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferWire {
    format: String,
    offer: Offer,
    transfer: String,
    count: u64,
    size: u64,
    frames: Vec<Frame>,
}

#[derive(Serialize)]
struct Header<'a> {
    format: &'a str,
    offer: &'a Offer,
    transfer: &'a str,
    count: u64,
    size: u64,
}

/// An opened transfer file.
pub struct Opened {
    /// The archive's encoding.
    pub plain: Zeroizing<Vec<u8>>,
    /// The transfer's random ID, hex.
    pub transfer: String,
}

/// Open a transfer file sealed to `offer`, whose receiving key is `seed`:
/// the file must carry exactly this offer, every frame in order and
/// authenticated, and the declared length. `opened_at` is when the device
/// first accepted the file, which must fall inside the offer's validity.
///
/// # Errors
/// Returns `Error::Forbidden` for another origin, device or offer,
/// `Error::Expired` past the offer, `Error::QuotaExceeded` beyond 256 MiB
/// and `Error::IntegrityFailed` for missing, repeated, reordered or changed
/// frames.
pub fn open_transfer(bytes: &[u8], offer: &Offer, seed: &Key, opened_at: u64) -> Result<Opened> {
    offer.check(&offer.origin, &offer.target, opened_at)?;
    ensure(
        hex::encode(hpke_public(seed)) == offer.public_key,
        Error::Forbidden,
    )?;
    let value: TransferWire = unpack_as(bytes, MAX_ARCHIVE)?;
    let frames = TRANSFER_CHUNK as u64;
    ensure(value.offer == *offer, Error::Forbidden)?;
    ensure(
        value.format == TRANSFER
            && is_hash(&value.transfer)
            && (1..=MAX_ARCHIVE as u64).contains(&value.size)
            && value.count == value.size.div_ceil(frames)
            && value.frames.len() as u64 == value.count,
        Error::IntegrityFailed,
    )?;
    let header = Header {
        format: &value.format,
        offer: &value.offer,
        transfer: &value.transfer,
        count: value.count,
        size: value.size,
    };
    let mut plain = Zeroizing::new(Vec::with_capacity(value.size as usize));
    let mut previous = String::new();
    for (index, frame) in value.frames.iter().enumerate() {
        let index = index as u64;
        ensure(
            frame.index == index
                && frame.previous == previous
                && frame.enc.len() == 32
                && frame.ciphertext.len() >= 16
                && frame.ciphertext.len() <= TRANSFER_CHUNK + 16,
            Error::IntegrityFailed,
        )?;
        let context = canonical(&(&header, index, &previous));
        let block = hpke_open_info(
            seed,
            &HpkeEnvelope {
                enc: frame.enc.to_vec(),
                ciphertext: frame.ciphertext.to_vec(),
            },
            &context,
        )?;
        ensure(
            block.len() as u64 == frames.min(value.size - index * frames),
            Error::IntegrityFailed,
        )?;
        plain.extend_from_slice(&block);
        previous = hex_sha256(&canonical(frame));
    }
    Ok(Opened {
        plain,
        transfer: value.transfer,
    })
}

// ---------------------------------------------------------------------------
// The archive.

/// The read-only inventory of one old identity.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Inventory {
    /// `dmsg-legacy-inventory/1`.
    pub format: String,
    /// The old Principal.
    pub principal: String,
    /// The old message canister.
    pub message_canister: String,
    /// `Local`, `ECDH` or `VetKey`.
    pub mode: String,
    /// `pre_migration`.
    pub snapshot: String,
    /// What was read.
    pub objects: Vec<SourceObject>,
    /// What could not be read.
    pub gaps: Vec<Gap>,
    /// The old calls made, without arguments.
    pub calls: Vec<Call>,
}

/// One old record exactly as observed.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceObject {
    /// Stable source key, such as `<canister>/channel/1/message/2`.
    pub key: String,
    /// `identity`, `profile`, `channel`, `message`, `file`, `local`,
    /// `setting`, `entitlement` or `avatar`.
    pub kind: String,
    /// The observation's CBOR.
    pub bytes: ByteBuf,
    /// SHA-256 of `bytes`, hex.
    pub digest: String,
    /// `query_observation`, `local_cache` or `derived_archive`.
    pub trust: String,
    /// When it was read, by the old site's clock.
    pub observed_at: u64,
}

/// Something not read, and why.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Gap {
    /// Its source key.
    pub source: String,
    /// Why.
    pub code: Failure,
    /// A short explanation.
    pub detail: String,
}

/// One old call, without its arguments.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Call {
    canister: String,
    method: String,
    at: u64,
    outcome: String,
}

/// A historical key; it only exists inside the transfer and the encrypted
/// archive copy.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoricalKey {
    /// The source it opens.
    pub source: String,
    /// The old Principal.
    pub principal: String,
    /// `master`, `channel_dek`, `channel_kek` or `static_ecdh`.
    pub purpose: String,
    /// The COSE_Key.
    pub cose_key: ByteBuf,
    /// How it was obtained, as an observation.
    pub parameters: ByteBuf,
}

/// A check of one source object.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Check {
    /// Its source key.
    pub source: String,
    /// The object's digest.
    pub digest: String,
    /// `aead_verified`, `source_plaintext`, `deleted` or `ciphertext_only`.
    pub result: String,
    /// SHA-256 of the opened content, hex.
    pub plaintext_digest: Option<String>,
}

/// A `dmsg-legacy-archive/1`.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Archive {
    /// [`ARCHIVE`].
    pub format: String,
    /// What was read.
    pub inventory: Inventory,
    /// The historical keys.
    pub keys: Vec<HistoricalKey>,
    /// The exporter's checks; recomputed, never trusted.
    pub checks: Vec<Check>,
    /// When it was collected, by the old site's clock.
    pub created_at: u64,
}

impl Drop for Archive {
    fn drop(&mut self) {
        for key in &mut self.keys {
            key.cose_key.zeroize();
        }
    }
}

const KINDS: [&str; 9] = [
    "identity",
    "profile",
    "channel",
    "message",
    "file",
    "local",
    "setting",
    "entitlement",
    "avatar",
];
const TRUSTS: [&str; 3] = ["query_observation", "local_cache", "derived_archive"];
const PURPOSES: [&str; 4] = ["master", "channel_dek", "channel_kek", "static_ecdh"];
const RESULTS: [&str; 4] = [
    "aead_verified",
    "source_plaintext",
    "deleted",
    "ciphertext_only",
];
const OUTCOMES: [&str; 9] = [
    "ok",
    "network",
    "permission",
    "missing",
    "missing_key",
    "version",
    "corrupt",
    "limit",
    "cancelled",
];

impl Archive {
    /// Decode and check an archive.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for anything outside the format, a
    /// repeated or substituted object, a key of another identity or a check
    /// of another object.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let archive: Self = unpack_as(bytes, MAX_ARCHIVE)?;
        archive.validate()?;
        Ok(archive)
    }

    /// The archive's CBOR.
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = canonical(self);
        ensure(bytes.len() <= MAX_ARCHIVE, Error::QuotaExceeded)?;
        Ok(bytes)
    }

    fn validate(&self) -> Result<()> {
        let inventory = &self.inventory;
        let text = |t: &str| js_len(t) <= MAX_TEXT;
        principal(&inventory.principal)?;
        principal(&inventory.message_canister)?;
        ensure(
            self.format == ARCHIVE
                && inventory.format == "dmsg-legacy-inventory/1"
                && ["Local", "ECDH", "VetKey"].contains(&inventory.mode.as_str())
                && inventory.snapshot == "pre_migration"
                && inventory.objects.len() <= MAX_ITEMS as usize
                && inventory.gaps.len() <= MAX_ITEMS as usize
                && inventory.calls.len() <= MAX_ITEMS as usize
                && self.keys.len() <= 10_000
                && self.checks.len() <= MAX_ITEMS as usize
                && inventory
                    .gaps
                    .iter()
                    .all(|g| text(&g.source) && text(&g.detail))
                && inventory.calls.iter().all(|c| {
                    principal(&c.canister).is_ok()
                        && js_len(&c.method) <= 80
                        && OUTCOMES.contains(&c.outcome.as_str())
                }),
            Error::IntegrityFailed,
        )?;
        let mut objects = BTreeMap::new();
        for object in &inventory.objects {
            ensure(
                text(&object.key)
                    && KINDS.contains(&object.kind.as_str())
                    && TRUSTS.contains(&object.trust.as_str())
                    && is_hash(&object.digest)
                    && hex_sha256(&object.bytes) == object.digest
                    && objects
                        .insert(object.key.as_str(), object.digest.as_str())
                        .is_none(),
                Error::IntegrityFailed,
            )?;
        }
        let mut keys = BTreeSet::new();
        for key in &self.keys {
            ensure(
                text(&key.source)
                    && key.principal == inventory.principal
                    && PURPOSES.contains(&key.purpose.as_str())
                    && (1..=2048).contains(&key.cose_key.len())
                    && key.parameters.len() <= 8192
                    && keys.insert((key.source.as_str(), key.purpose.as_str())),
                Error::IntegrityFailed,
            )?;
        }
        let mut checks = BTreeSet::new();
        for check in &self.checks {
            ensure(
                objects.get(check.source.as_str()) == Some(&check.digest.as_str())
                    && RESULTS.contains(&check.result.as_str())
                    && check.plaintext_digest.as_deref().is_none_or(is_hash)
                    && checks.insert(check.source.as_str()),
                Error::IntegrityFailed,
            )?;
        }
        Ok(())
    }

    /// The archive's identity without collection times or transport, so a
    /// second export of the same content is recognized.
    pub fn content_id(&self) -> String {
        let inventory = &self.inventory;
        let mut objects: Vec<_> = inventory
            .objects
            .iter()
            .map(|o| (&o.key, &o.kind, &o.digest, &o.trust))
            .collect();
        objects.sort_by(|a, b| js_order(a.0, b.0));
        let mut keys: Vec<_> = self
            .keys
            .iter()
            .map(|k| {
                [
                    k.source.clone(),
                    k.purpose.clone(),
                    hex_sha256(&k.cose_key),
                    hex_sha256(&k.parameters),
                ]
            })
            .collect();
        keys.sort_by(|a, b| js_order(&a.join(":"), &b.join(":")));
        let mut checks: Vec<_> = self
            .checks
            .iter()
            .map(|c| (&c.source, &c.digest, &c.result, &c.plaintext_digest))
            .collect();
        checks.sort_by(|a, b| js_order(a.0, b.0));
        let mut gaps: Vec<_> = inventory
            .gaps
            .iter()
            .map(|g| [g.source.clone(), g.code.name().into(), g.detail.clone()])
            .collect();
        gaps.sort_by(|a, b| js_order(&a.join(":"), &b.join(":")));
        #[derive(Serialize)]
        struct Content<'a, O, K, C, G> {
            format: &'a str,
            principal: &'a str,
            source: &'a str,
            mode: &'a str,
            snapshot: &'a str,
            objects: O,
            keys: K,
            checks: C,
            gaps: G,
        }
        hex_sha256(&canonical(&Content {
            format: "dmsg-legacy-content/1",
            principal: &inventory.principal,
            source: &inventory.message_canister,
            mode: &inventory.mode,
            snapshot: &inventory.snapshot,
            objects,
            keys,
            checks,
            gaps,
        }))
    }

    /// The observation of one object.
    fn observation(object: &SourceObject) -> Checked<Value> {
        unpack(&object.bytes, MAX_ARCHIVE).or(fault(Failure::Corrupt, "Invalid observation"))
    }

    /// The historical channel DEK of `channel`.
    fn channel_key(&self, channel: &str) -> Checked<Option<Zeroizing<Key>>> {
        self.keys
            .iter()
            .find(|k| k.source == channel && k.purpose == "channel_dek")
            .map(|k| cose_key(&k.cose_key))
            .transpose()
    }
}

// ---------------------------------------------------------------------------
// Old ciphertext.

/// The secret of a symmetric A256GCM COSE_Key.
fn cose_key(bytes: &[u8]) -> Checked<Zeroizing<Key>> {
    let Ok(Value::Map(entries)) = cbor2::from_slice::<Value>(bytes) else {
        return fault(Failure::Corrupt, "Invalid legacy COSE_Key");
    };
    let label = |n: i128| {
        entries
            .iter()
            .find(|(k, _)| matches!(k, Value::Integer(l) if i128::from(*l) == n))
            .map(|(_, v)| v)
    };
    let symmetric = matches!(label(1), Some(Value::Integer(t)) if i128::from(*t) == 4);
    let alg = match label(3) {
        None => true,
        Some(Value::Integer(a)) => i128::from(*a) == 3,
        _ => false,
    };
    match label(-1) {
        Some(Value::Bytes(k)) if symmetric && alg && k.len() == 32 => {
            let mut key = Zeroizing::new([0u8; 32]);
            key.copy_from_slice(k);
            Ok(key)
        }
        _ => fault(Failure::Corrupt, "Invalid legacy AES key"),
    }
}

/// Open an old COSE_Encrypt0 (A256GCM, tagged or not, full IV) with no
/// external AAD.
fn legacy_decrypt(key: &Key, data: &[u8]) -> Checked<Zeroizing<Vec<u8>>> {
    let corrupt = || Fault(Failure::Corrupt, "Legacy AEAD authentication failed");
    if data.is_empty() || data.len() > MAX_ARCHIVE {
        return fault(Failure::Limit, "Invalid legacy ciphertext size");
    }
    let value: Value = cbor2::from_slice(data).map_err(|_| corrupt())?;
    let value = match value {
        Value::Tag(16, inner) => *inner,
        other => other,
    };
    let Value::Array(items) = value else {
        return Err(corrupt());
    };
    let [
        Value::Bytes(protected),
        Value::Map(unprotected),
        Value::Bytes(ciphertext),
    ] = items.as_slice()
    else {
        return Err(corrupt());
    };
    let header: Vec<(Value, Value)> = match protected.is_empty() {
        true => vec![],
        false => match cbor2::from_slice::<Value>(protected) {
            Ok(Value::Map(entries)) => entries,
            _ => return Err(corrupt()),
        },
    };
    let label = |entries: &[(Value, Value)], n: i128| {
        entries
            .iter()
            .find(|(k, _)| matches!(k, Value::Integer(l) if i128::from(*l) == n))
            .map(|(_, v)| v.clone())
    };
    let alg = label(&header, 1).or_else(|| label(unprotected, 1));
    let partial = label(&header, 6).or_else(|| label(unprotected, 6));
    let iv = label(&header, 5).or_else(|| label(unprotected, 5));
    let ok_alg = matches!(alg, Some(Value::Integer(a)) if i128::from(a) == 3);
    let (Some(Value::Bytes(iv)), true, None, None) = (iv, ok_alg, partial, label(&header, 2))
    else {
        return Err(corrupt());
    };
    let nonce: &[u8; 12] = iv.as_slice().try_into().map_err(|_| corrupt())?;
    let aad = canonical(&("Encrypt0", Bytes::new(protected), Bytes::new(&[])));
    aes_gcm_open(key, nonce, ciphertext, &aad).map_err(|_| corrupt())
}

/// One old message as opened.
struct Decoded {
    deleted: bool,
    text: String,
    file: Option<Value>,
}

impl Decoded {
    fn digest(&self) -> String {
        let value = Value::Map(vec![
            (Value::Text("deleted".into()), Value::Bool(self.deleted)),
            (Value::Text("text".into()), Value::Text(self.text.clone())),
            (
                Value::Text("file".into()),
                self.file.clone().unwrap_or(Value::Null),
            ),
        ]);
        hex_sha256(&canonical(&value))
    }
}

fn decode_message(key: Option<&Key>, kind: Option<i128>, payload: &[u8]) -> Checked<Decoded> {
    if payload.is_empty() {
        return Ok(Decoded {
            deleted: true,
            text: String::new(),
            file: None,
        });
    }
    let plain = match (kind, key) {
        (Some(1), _) => Zeroizing::new(payload.to_vec()),
        (Some(0), Some(key)) => legacy_decrypt(key, payload)?,
        (Some(0), None) => return fault(Failure::MissingKey, "Channel DEK is missing"),
        _ => return fault(Failure::Version, "Unknown legacy message kind"),
    };
    let value = unpack(&plain, 32_768).or(fault(Failure::Corrupt, "Invalid message"))?;
    let (text, file) = match value {
        Value::Text(text) => (text, None),
        Value::Array(items) => match items.as_slice() {
            [
                Value::Text(text),
                Value::Text(kind),
                Value::Bytes(reference),
            ] if kind == "File" => {
                let file = unpack(reference, 8192).or(fault(
                    Failure::Corrupt,
                    "Invalid legacy attachment reference",
                ))?;
                ensure_file(&file)?;
                (text.clone(), Some(file))
            }
            _ => return fault(Failure::Version, "Unsupported legacy message payload"),
        },
        _ => return fault(Failure::Version, "Unsupported legacy message payload"),
    };
    Ok(Decoded {
        deleted: false,
        text,
        file,
    })
}

fn ensure_file(file: &Value) -> Checked<()> {
    let int = |name| match field(file, name) {
        Some(Value::Integer(n)) => Some(i128::from(*n)),
        _ => None,
    };
    let valid = matches!(field(file, "canister"), Some(Value::Bytes(_)))
        && int("id").is_some_and(|n| (0..=0xffff_ffff).contains(&n))
        && int("size").is_some_and(|n| (0..=(1 << 53) - 1).contains(&n))
        && field(file, "name").and_then(text_of).is_some()
        && field(file, "type").and_then(text_of).is_some();
    match valid {
        true => Ok(()),
        false => fault(Failure::Corrupt, "Invalid legacy attachment reference"),
    }
}

/// The attachment a message refers to.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct FileLink {
    /// `<bucket>/file/<id>`.
    pub source: String,
    /// Old file name.
    pub name: String,
    /// Old length.
    pub size: u64,
    /// Old media type.
    pub mime: String,
}

fn file_link(file: &Value) -> Option<FileLink> {
    let canister = match field(file, "canister") {
        Some(Value::Bytes(b)) => Principal::try_from_slice(b).ok()?,
        _ => return None,
    };
    Some(FileLink {
        source: format!(
            "{}/file/{}",
            canister.to_text(),
            integer_of(field(file, "id")?)?
        ),
        name: text_of(field(file, "name")?)?.into(),
        size: u64::try_from(integer_of(field(file, "size")?)?).ok()?,
        mime: text_of(field(file, "type")?)?.into(),
    })
}

/// A message read from the archive. Author and time are the old service's
/// claims, not device signatures.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct LegacyMessage {
    /// Its source key.
    pub source: String,
    /// Text.
    pub text: String,
    /// Deleted in the old service.
    pub deleted: bool,
    /// A system record.
    pub system: bool,
    /// The old author's Principal, as the old service recorded it.
    pub author: String,
    /// The old time, as recorded.
    pub time: String,
    /// The attachment.
    pub file: Option<FileLink>,
}

/// The offline check of an archive.
#[derive(Serialize, Clone, Debug)]
pub struct Verification {
    /// Every object opened.
    pub checks: Vec<Check>,
    /// The archive's gaps, then every object not opened.
    pub gaps: Vec<Gap>,
    /// Messages opened, in archive order.
    pub messages: Vec<LegacyMessage>,
    /// Anything is missing.
    pub partial: bool,
}

/// Open every message, attachment and avatar of `archive` again.
pub fn verify(archive: &Archive) -> Verification {
    let mut gaps = archive.inventory.gaps.clone();
    let mut checks = Vec::new();
    let mut messages = Vec::new();
    for object in &archive.inventory.objects {
        if !["message", "file", "avatar"].contains(&object.kind.as_str()) {
            continue;
        }
        if let Err(Fault(code, detail)) = check_object(archive, object, &mut checks, &mut messages)
        {
            gaps.push(Gap {
                source: object.key.clone(),
                code,
                detail: detail.into(),
            });
        }
    }
    for claimed in &archive.checks {
        if let Some(actual) = checks.iter().find(|c| c.source == claimed.source)
            && (actual.result != claimed.result
                || actual.plaintext_digest != claimed.plaintext_digest)
        {
            gaps.push(Gap {
                source: claimed.source.clone(),
                code: Failure::Corrupt,
                detail: "Exporter verification disagrees with offline verification".into(),
            });
        }
    }
    Verification {
        partial: !gaps.is_empty(),
        checks,
        gaps,
        messages,
    }
}

fn check_object(
    archive: &Archive,
    object: &SourceObject,
    checks: &mut Vec<Check>,
    messages: &mut Vec<LegacyMessage>,
) -> Checked<()> {
    let record = Archive::observation(object)?;
    let check = |result: &str, plain: Option<String>| Check {
        source: object.key.clone(),
        digest: object.digest.clone(),
        result: result.into(),
        plaintext_digest: plain,
    };
    match object.kind.as_str() {
        "avatar" => {
            let bytes = field(&record, "bytes")
                .and_then(bytes_of)
                .unwrap_or_default();
            let size = field(&record, "info")
                .and_then(|i| field(i, "size"))
                .and_then(integer_of);
            let digest = hex_sha256(&bytes);
            if bytes.len() > MAX_AVATAR
                || field(&record, "digest").and_then(text_of) != Some(digest.as_str())
                || size != Some(bytes.len() as i128)
            {
                return fault(Failure::Corrupt, "Avatar length or digest mismatch");
            }
            checks.push(check("source_plaintext", Some(digest)));
        }
        "message" => {
            let channel = object.key.split("/message/").next().unwrap_or_default();
            let key = archive.channel_key(channel)?;
            let kind = field(&record, "kind").and_then(integer_of);
            let payload = field(&record, "payload")
                .and_then(bytes_of)
                .ok_or(Fault(Failure::Corrupt, "Invalid message"))?;
            let decoded = decode_message(key.as_deref(), kind, &payload)?;
            let result = match (decoded.deleted, kind) {
                (true, _) => "deleted",
                (false, Some(1)) => "source_plaintext",
                _ => "aead_verified",
            };
            checks.push(check(result, (!decoded.deleted).then(|| decoded.digest())));
            messages.push(LegacyMessage {
                source: object.key.clone(),
                deleted: decoded.deleted,
                system: kind == Some(1),
                author: field(&record, "created_by")
                    .and_then(principal_of)
                    .unwrap_or_else(|| "unknown".into()),
                time: display(field(&record, "created_at")),
                file: decoded.file.as_ref().and_then(file_link),
                text: decoded.text,
            });
        }
        _ => {
            let plain = open_file_record(archive, &record)?;
            checks.push(check("aead_verified", Some(hex_sha256(&plain))));
        }
    }
    Ok(())
}

fn open_file_record(archive: &Archive, record: &Value) -> Checked<Zeroizing<Vec<u8>>> {
    if matches!(field(record, "complete"), Some(Value::Bool(false))) {
        return fault(
            Failure::Missing,
            "Incomplete upload: ciphertext fragments preserved without a complete COSE object",
        );
    }
    let channel = field(record, "channel")
        .and_then(text_of)
        .unwrap_or_default();
    let key = archive
        .channel_key(channel)?
        .ok_or(Fault(Failure::MissingKey, "Historical channel DEK missing"))?;
    let ciphertext = field(record, "ciphertext")
        .and_then(bytes_of)
        .ok_or(Fault(Failure::Corrupt, "Invalid file record"))?;
    legacy_decrypt(&key, &ciphertext)
}

/// An old attachment, opened.
pub struct LegacyFile {
    /// The name its message gave it, else the old file name.
    pub name: String,
    /// The plaintext.
    pub bytes: Zeroizing<Vec<u8>>,
}

/// Open the attachment or avatar `source`.
///
/// # Errors
/// Returns `Error::NotFound` for no such object, `Error::RecoveryIncomplete`
/// for an incomplete upload or a missing key and `Error::IntegrityFailed`
/// when it does not authenticate.
pub fn open_file(archive: &Archive, source: &str) -> Result<LegacyFile> {
    let object = archive
        .inventory
        .objects
        .iter()
        .find(|o| o.key == source && ["file", "avatar"].contains(&o.kind.as_str()))
        .ok_or(Error::NotFound)?;
    let failed = |Fault(code, _)| match code {
        Failure::Missing | Failure::MissingKey => Error::RecoveryIncomplete,
        _ => Error::IntegrityFailed,
    };
    let record = Archive::observation(object).map_err(failed)?;
    let info_name = field(&record, "info")
        .and_then(|i| field(i, "name"))
        .and_then(text_of)
        .map(str::to_string);
    if object.kind == "avatar" {
        let mut checks = vec![];
        check_object(archive, object, &mut checks, &mut vec![]).map_err(failed)?;
        return Ok(LegacyFile {
            name: info_name.unwrap_or_else(|| "legacy-avatar".into()),
            bytes: Zeroizing::new(
                field(&record, "bytes")
                    .and_then(bytes_of)
                    .unwrap_or_default(),
            ),
        });
    }
    let bytes = open_file_record(archive, &record).map_err(failed)?;
    let channel = field(&record, "channel")
        .and_then(text_of)
        .unwrap_or_default();
    let key = archive.channel_key(channel).map_err(failed)?;
    let prefix = format!("{channel}/message/");
    let name = archive
        .inventory
        .objects
        .iter()
        .filter(|o| o.kind == "message" && o.key.starts_with(&prefix))
        .find_map(|o| {
            let data = Archive::observation(o).ok()?;
            let kind = field(&data, "kind").and_then(integer_of);
            let payload = field(&data, "payload").and_then(bytes_of)?;
            let link = decode_message(key.as_deref(), kind, &payload)
                .ok()?
                .file
                .as_ref()
                .and_then(file_link)?;
            (link.source == source).then_some(link.name)
        });
    Ok(LegacyFile {
        name: name.or(info_name).unwrap_or_else(|| "legacy-file".into()),
        bytes,
    })
}

/// The media type of an image by its magic bytes.
pub fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]) {
        Some("image/png")
    } else if bytes.starts_with(&[255, 216, 255]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else {
        None
    }
}

/// What the archive says about the old account, for display.
#[derive(Serialize, Clone, Debug)]
pub struct Summary {
    /// Old display name.
    pub name: String,
    /// Old bio.
    pub bio: String,
    /// Old avatar URL.
    pub image: String,
    /// Old channels with their recorded payments, as raw values.
    pub resources: Vec<serde_json::Value>,
    /// Old balances and entitlements, not settled here.
    pub entitlements: Vec<serde_json::Value>,
    /// Attachments and the avatar: `source`, `name`, `verified`.
    pub files: Vec<serde_json::Value>,
}

/// The display summary of a verified archive.
pub fn summary(archive: &Archive, verification: &Verification) -> Summary {
    use serde_json::json;
    let find = |kind: &str| {
        archive
            .inventory
            .objects
            .iter()
            .find(|o| o.kind == kind)
            .and_then(|o| Archive::observation(o).ok())
    };
    let user = find("identity");
    let profile = find("profile");
    let get = |value: &Option<Value>, name: &str| {
        value
            .as_ref()
            .and_then(|v| field(v, name))
            .and_then(text_of)
            .unwrap_or_default()
            .to_string()
    };
    let verified = |source: &str| verification.checks.iter().any(|c| c.source == source);
    let mut files = vec![];
    let mut resources = vec![];
    let mut entitlements = vec![];
    for object in &archive.inventory.objects {
        let Ok(record) = Archive::observation(object) else {
            continue;
        };
        match object.kind.as_str() {
            "file" | "avatar" => {
                let named = verification
                    .messages
                    .iter()
                    .filter_map(|m| m.file.as_ref())
                    .find(|f| f.source == object.key)
                    .map(|f| f.name.clone());
                let info = field(&record, "info")
                    .and_then(|i| field(i, "name"))
                    .and_then(text_of)
                    .map(str::to_string);
                let bytes = field(&record, "bytes").and_then(bytes_of);
                files.push(json!({
                    "source": object.key,
                    "kind": object.kind,
                    "name": named.or(info).unwrap_or_else(|| "legacy-file".into()),
                    "verified": verified(&object.key),
                    "mime": bytes.as_deref().and_then(image_mime),
                }));
            }
            "channel" => resources.push(json!({
                "source": object.key,
                "name": display(field(&record, "name")),
                "paid": display(field(&record, "paid")),
                "gas": display(field(&record, "gas")),
            })),
            "entitlement" => entitlements.push(json!({
                "source": object.key,
                "value": observation_json(&record),
            })),
            _ => {}
        }
    }
    Summary {
        name: get(&user, "name"),
        bio: get(&profile, "bio"),
        image: get(&user, "image"),
        resources,
        entitlements,
        files,
    }
}

// ---------------------------------------------------------------------------
// Shared history.

/// What a scoped archive is cut to: one frozen channel.
#[derive(Clone, Debug)]
pub struct Scope<'a> {
    /// The old channel canister.
    pub source: &'a str,
    /// The old channel ID.
    pub channel: u32,
    /// SHA-256 of the frozen channel's wrapped DEK, hex.
    pub dek_digest: &'a str,
    /// First and last frozen message IDs.
    pub messages: (u32, u32),
    /// The frozen source's digest.
    pub digest: &'a str,
}

/// Keep only one channel's messages and attachments and its two historical
/// keys, checked against the frozen wrapper. Missing message slots become
/// gaps; no master key, ECDH key, profile or other channel is kept.
///
/// # Errors
/// Returns `Error::NotFound` without the original channel record,
/// `Error::RecoveryIncomplete` without both channel keys and
/// `Error::IntegrityFailed` when the keys do not match the frozen channel.
pub fn scoped(archive: &Archive, scope: &Scope, now: u64) -> Result<Archive> {
    let key = format!("{}/channel/{}", scope.source, scope.channel);
    let original = archive
        .inventory
        .objects
        .iter()
        .find(|o| o.kind == "channel" && o.key == key)
        .ok_or(Error::NotFound)?;
    let channel = Archive::observation(original).map_err(|_| Error::IntegrityFailed)?;
    let wrapped = field(&channel, "dek")
        .and_then(bytes_of)
        .ok_or(Error::IntegrityFailed)?;
    ensure(
        hex_sha256(&wrapped) == scope.dek_digest,
        Error::IntegrityFailed,
    )?;
    let find = |purpose: &str| {
        archive
            .keys
            .iter()
            .find(|k| k.source == key && k.purpose == purpose)
    };
    let (Some(kek), Some(dek)) = (find("channel_kek"), find("channel_dek")) else {
        return Err(Error::RecoveryIncomplete);
    };
    let unwrap = |bytes: &[u8]| cose_key(bytes).map_err(|_| Error::IntegrityFailed);
    let opened =
        legacy_decrypt(&*unwrap(&kek.cose_key)?, &wrapped).map_err(|_| Error::IntegrityFailed)?;
    ensure(
        *unwrap(&opened)? == *unwrap(&dek.cose_key)?,
        Error::IntegrityFailed,
    )?;
    let metadata = canonical(&Value::Map(vec![
        (
            Value::Text("id".into()),
            Value::Integer(scope.channel.into()),
        ),
        (
            Value::Text("canister".into()),
            field(&channel, "canister").cloned().unwrap_or(Value::Null),
        ),
        (
            Value::Text("dek".into()),
            Value::Map(vec![(Value::Text("bytes".into()), Value::Bytes(wrapped))]),
        ),
        (
            Value::Text("message_start".into()),
            Value::Integer(scope.messages.0.into()),
        ),
        (
            Value::Text("latest_message_id".into()),
            Value::Integer(scope.messages.1.into()),
        ),
    ]));
    let mut objects = vec![SourceObject {
        key: key.clone(),
        kind: "channel".into(),
        digest: hex_sha256(&metadata),
        bytes: ByteBuf::from(metadata),
        trust: "derived_archive".into(),
        observed_at: original.observed_at,
    }];
    let prefix = format!("{key}/message/");
    let mut observed = BTreeSet::new();
    for object in &archive.inventory.objects {
        let Ok(record) = Archive::observation(object) else {
            continue;
        };
        if object.kind == "message" && object.key.starts_with(&prefix) {
            let id = field(&record, "id").and_then(integer_of).unwrap_or(-1);
            if (i128::from(scope.messages.0)..=i128::from(scope.messages.1)).contains(&id) {
                objects.push(object.clone());
                observed.insert(id);
            }
        } else if object.kind == "file"
            && field(&record, "channel").and_then(text_of) == Some(key.as_str())
        {
            objects.push(object.clone());
        }
    }
    let included: BTreeSet<&str> = objects.iter().map(|o| o.key.as_str()).collect();
    let mut gaps: Vec<Gap> = archive
        .inventory
        .gaps
        .iter()
        .filter(|g| g.source.starts_with(&key) || included.contains(g.source.as_str()))
        .cloned()
        .collect();
    for id in scope.messages.0..=scope.messages.1 {
        if !observed.contains(&i128::from(id)) {
            gaps.push(Gap {
                source: format!("{key}/message/{id}"),
                code: Failure::Missing,
                detail: "Frozen message slot is not present in the supplied archive".into(),
            });
        }
    }
    #[derive(Serialize)]
    struct Parameters<'a> {
        source_key: &'a str,
        frozen_source_digest: &'a str,
    }
    let parameters = canonical(&Parameters {
        source_key: &key,
        frozen_source_digest: scope.digest,
    });
    let checks = archive
        .checks
        .iter()
        .filter(|c| included.contains(c.source.as_str()))
        .cloned()
        .collect();
    let result = Archive {
        format: ARCHIVE.into(),
        inventory: Inventory {
            objects,
            gaps,
            calls: vec![],
            ..archive.inventory.clone()
        },
        keys: [kek, dek]
            .into_iter()
            .map(|k| HistoricalKey {
                parameters: ByteBuf::from(parameters.clone()),
                ..k.clone()
            })
            .collect(),
        checks,
        created_at: now,
    };
    result.validate()?;
    Ok(result)
}

#[cfg(test)]
#[path = "legacy_tests.rs"]
mod tests;
