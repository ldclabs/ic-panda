//! Frozen-source evidence of the old service
//! (`docs/protocol/channel_migration_zh.md`, `legacy_freeze_zh.md`).
//!
//! The frozen old canisters answer `legacy_snapshot` as an ingress update,
//! so each page comes with the IC certificate of its reply; the old OSS
//! buckets answer `legacy_folder_manifest` the same way. A proof binds the
//! canister, the caller, the exact Candid arguments (only their SHA-256 for
//! a folder, which takes a bearer token) and the certified reply; pages
//! chain into a complete, unchanged inventory under one cutover.
//!
//! On that evidence rest the shared channel inheritance digests (source key,
//! proposal, manager consent, member claim), the frozen source of a shared
//! channel, an old identity's explicit approval, and the final comparison of
//! a personal archive against the frozen service.

use crate::codec::{base64url, canonical, hex_sha256, principal, sha256};
use crate::legacy::{Archive, js_len, js_order, observation_value};
use candid::{CandidType, Principal};
use dmsg_types::{Error, Result, ensure};
use ic_certification_v3::LookupResult;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::collections::{BTreeMap, BTreeSet};

/// Snapshot proof format.
pub const SNAPSHOT_PROOF: &str = "dmsg-legacy-snapshot-proof/1";
/// Folder proof format.
pub const FOLDER_PROOF: &str = "dmsg-legacy-folder-proof/1";
/// Longest an old approval may stay valid.
pub const APPROVAL_MS: u64 = 7 * dmsg_types::DAY;

/// What a `legacy_snapshot` page covers.
#[derive(CandidType, Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum SnapshotScope {
    /// Name registrations.
    Names,
    /// Name roles.
    Authorities,
    /// Profiles.
    Profile,
    /// Channel list.
    Channels,
    /// One channel.
    Channel(u32),
    /// One channel's frozen ACL.
    ChannelAuthority(u32),
    /// One channel's messages.
    Messages(u32),
    /// The caller's explicit approval of a migration intent.
    Attestation {
        /// The intent digest.
        digest: ByteBuf,
        /// Unix milliseconds.
        expires_at: u64,
    },
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq, Eq)]
enum LegacyMode {
    Active,
    Draining,
    ReadOnly,
}

#[derive(CandidType, Deserialize, Clone, Debug)]
struct FreezeStatus {
    mode: LegacyMode,
    epoch: u64,
    cutover_id: Option<ByteBuf>,
    draining_at: Option<u64>,
    readonly_at: Option<u64>,
    pending: u64,
    unresolved_after_upgrade: u64,
    baseline_needed: bool,
    baseline_evidence: Option<ByteBuf>,
}

#[derive(CandidType, Deserialize, Clone, Debug)]
struct SnapshotPage {
    schema: u8,
    source: Principal,
    subject: Principal,
    freeze: FreezeStatus,
    scope: SnapshotScope,
    source_context: ByteBuf,
    count: u64,
    initial_digest: ByteBuf,
    inventory_digest: ByteBuf,
    entries: Vec<(String, ByteBuf)>,
    next: Option<String>,
    complete: bool,
}

/// One certified `legacy_snapshot` page, as the old site exported it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotProof {
    /// [`SNAPSHOT_PROOF`].
    pub format: String,
    /// The old canister.
    pub canister: String,
    /// The old identity that called it.
    pub caller: String,
    /// The Candid arguments.
    #[serde(with = "serde_bytes")]
    pub args: Vec<u8>,
    /// Ingress expiry in nanoseconds, decimal.
    pub expiry: String,
    /// The ingress nonce.
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    /// The request ID.
    #[serde(with = "serde_bytes")]
    pub request_id: Vec<u8>,
    /// The IC certificate of the reply.
    #[serde(with = "serde_bytes")]
    pub certificate: Vec<u8>,
    /// The Candid reply.
    #[serde(with = "serde_bytes")]
    pub reply: Vec<u8>,
}

/// [`SnapshotProof`] as JSON carries it: bytes as base64url.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProofWire {
    /// [`SNAPSHOT_PROOF`].
    pub format: String,
    /// The old canister.
    pub canister: String,
    /// The old identity that called it.
    pub caller: String,
    /// The Candid arguments.
    #[serde(with = "base64url")]
    pub args: Vec<u8>,
    /// Ingress expiry in nanoseconds, decimal.
    pub expiry: String,
    /// The ingress nonce.
    #[serde(with = "base64url")]
    pub nonce: Vec<u8>,
    /// The request ID.
    #[serde(with = "base64url")]
    pub request_id: Vec<u8>,
    /// The IC certificate of the reply.
    #[serde(with = "base64url")]
    pub certificate: Vec<u8>,
    /// The Candid reply.
    #[serde(with = "base64url")]
    pub reply: Vec<u8>,
}

impl From<&ProofWire> for SnapshotProof {
    fn from(w: &ProofWire) -> Self {
        Self {
            format: w.format.clone(),
            canister: w.canister.clone(),
            caller: w.caller.clone(),
            args: w.args.clone(),
            expiry: w.expiry.clone(),
            nonce: w.nonce.clone(),
            request_id: w.request_id.clone(),
            certificate: w.certificate.clone(),
            reply: w.reply.clone(),
        }
    }
}

/// The trusted inputs of a proof check.
#[derive(Clone, Copy)]
pub struct Trust<'a> {
    /// The IC root key.
    pub root_key: &'a [u8],
    /// The reviewed cutover of the freeze.
    pub cutover: &'a [u8; 32],
    /// Unix milliseconds.
    pub now: u64,
}

/// The representation-independent hash of an ingress call whose argument
/// hashes to `arg`.
fn request_id(
    canister: &Principal,
    sender: &Principal,
    method: &str,
    arg: &[u8; 32],
    expiry: u64,
    nonce: &[u8],
) -> [u8; 32] {
    let mut leb = vec![];
    let mut n = expiry;
    loop {
        let byte = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            leb.push(byte);
            break;
        }
        leb.push(byte | 0x80);
    }
    let mut fields: Vec<([u8; 32], [u8; 32])> = vec![
        (*sha256(b"request_type"), *sha256(b"call")),
        (*sha256(b"canister_id"), *sha256(canister.as_slice())),
        (*sha256(b"method_name"), *sha256(method.as_bytes())),
        (*sha256(b"arg"), *arg),
        (*sha256(b"sender"), *sha256(sender.as_slice())),
        (*sha256(b"ingress_expiry"), *sha256(&leb)),
    ];
    if !nonce.is_empty() {
        fields.push((*sha256(b"nonce"), *sha256(nonce)));
    }
    fields.sort();
    let mut data = Vec::with_capacity(fields.len() * 64);
    for (key, value) in &fields {
        data.extend_from_slice(key);
        data.extend_from_slice(value);
    }
    *sha256(&data)
}

/// What an ingress proof carries besides the call it names.
struct Ingress<'a> {
    expiry: &'a str,
    nonce: &'a [u8],
    request_id: &'a [u8],
    certificate: &'a [u8],
    reply: &'a [u8],
}

/// Check one ingress proof and return its reply and certified time (ms).
fn certified_reply(
    canister: &Principal,
    caller: &Principal,
    method: &str,
    arg: &[u8; 32],
    ingress: Ingress,
    trust: &Trust,
) -> Result<u64> {
    let Ingress {
        expiry,
        nonce,
        request_id: claimed,
        certificate,
        reply,
    } = ingress;
    ensure(
        !expiry.is_empty()
            && expiry.len() <= 20
            && expiry.bytes().all(|b| b.is_ascii_digit())
            && nonce.len() <= 32
            && certificate.len() <= 65_536
            && reply.len() <= 196_608,
        Error::IntegrityFailed,
    )?;
    let expiry: u64 = expiry.parse().map_err(|_| Error::IntegrityFailed)?;
    let id = request_id(canister, caller, method, arg, expiry, nonce);
    ensure(claimed == id, Error::IntegrityFailed)?;
    let certificate = ic_auth_verifier::parse_certificate_cbor(certificate)
        .map_err(|_| Error::IntegrityFailed)?;
    // Frozen evidence is old by design: any certificate time verifies, and
    // only one from the future is refused below.
    ic_auth_verifier::verify_certificate(
        &certificate,
        canister.as_slice(),
        trust.root_key,
        u128::from(trust.now) * 1_000_000,
        u128::MAX,
    )
    .map_err(|_| Error::IntegrityFailed)?;
    let lookup = |leaf: &[u8]| match certificate.tree.lookup_path([
        b"request_status".as_slice(),
        id.as_slice(),
        leaf,
    ]) {
        LookupResult::Found(value) => Some(value.to_vec()),
        _ => None,
    };
    let LookupResult::Found(time) = certificate.tree.lookup_path([b"time".as_slice()]) else {
        return Err(Error::IntegrityFailed);
    };
    ensure(
        lookup(b"status").as_deref() == Some(b"replied".as_slice())
            && lookup(b"reply").as_deref() == Some(reply),
        Error::IntegrityFailed,
    )?;
    let at = timestamp(time)?;
    ensure(at <= trust.now + 5_000, Error::IntegrityFailed)?;
    Ok(at)
}

/// The certificate time in milliseconds.
fn timestamp(data: &[u8]) -> Result<u64> {
    ensure(!data.is_empty() && data.len() <= 10, Error::IntegrityFailed)?;
    let mut value: u128 = 0;
    for (i, byte) in data.iter().enumerate() {
        let last = i == data.len() - 1;
        ensure((byte & 0x80 == 0) == last, Error::IntegrityFailed)?;
        value |= u128::from(byte & 0x7f) << (7 * i);
    }
    u64::try_from(value / 1_000_000).map_err(|_| Error::IntegrityFailed)
}

/// Check one snapshot page: the call was exactly `scope` after `after` on
/// `canister`, the reply is certified, and it is a frozen, read-only page.
fn verify_page(
    proof: &SnapshotProof,
    canister: &str,
    caller: &str,
    scope: &SnapshotScope,
    after: Option<&str>,
    trust: &Trust,
) -> Result<SnapshotPage> {
    ensure(
        proof.format == SNAPSHOT_PROOF
            && proof.canister == canister
            && proof.caller == caller
            && proof.args.len() <= 2048,
        Error::IntegrityFailed,
    )?;
    let canister = principal(canister)?;
    let caller = principal(caller)?;
    let (called, cursor): (SnapshotScope, Option<String>) =
        candid::decode_args(&proof.args).map_err(|_| Error::IntegrityFailed)?;
    ensure(
        called == *scope && cursor.as_deref() == after,
        Error::Forbidden,
    )?;
    let at = certified_reply(
        &canister,
        &caller,
        "legacy_snapshot",
        &sha256(&proof.args),
        Ingress {
            expiry: &proof.expiry,
            nonce: &proof.nonce,
            request_id: &proof.request_id,
            certificate: &proof.certificate,
            reply: &proof.reply,
        },
        trust,
    )?;
    let reply: std::result::Result<SnapshotPage, String> =
        candid::decode_one(&proof.reply).map_err(|_| Error::IntegrityFailed)?;
    let page = reply.map_err(|_| Error::UnsupportedProtocol)?;
    let freeze = &page.freeze;
    ensure(
        page.schema == 1
            && page.source == canister
            && page.subject == caller
            && freeze.mode == LegacyMode::ReadOnly
            && freeze.pending == 0
            && !freeze.baseline_needed
            && freeze.readonly_at.is_some_and(|r| r <= at)
            && page.count <= 100_000
            && page.entries.len() <= 64
            && page.initial_digest.len() == 32
            && page.inventory_digest.len() == 32
            && page.scope == *scope,
        Error::IntegrityFailed,
    )?;
    let mut previous = after;
    for (key, _) in &page.entries {
        ensure(
            js_len(key) <= 128 && previous.is_none_or(|p| js_order(key, p).is_gt()),
            Error::IntegrityFailed,
        )?;
        previous = Some(key);
    }
    ensure(
        page.next.as_deref() == page.entries.last().map(|(k, _)| k.as_str()).or(after)
            && (page.complete || !page.entries.is_empty()),
        Error::IntegrityFailed,
    )?;
    Ok(page)
}

/// SHA-256 of the rolling inventory digest after one more row.
fn fold(previous: &[u8], key: &str, value: &[u8]) -> [u8; 32] {
    let mut data = previous.to_vec();
    data.extend_from_slice(&(key.len() as u64).to_be_bytes());
    data.extend_from_slice(key.as_bytes());
    data.extend_from_slice(&(value.len() as u64).to_be_bytes());
    data.extend_from_slice(value);
    *sha256(&data)
}

/// A complete frozen inventory read through a series of pages.
struct Series {
    epoch: u64,
    rows: Vec<(String, Vec<u8>)>,
}

/// Check that `proofs` are every page of one frozen `scope` of `canister`
/// under the cutover, in order, unchanged between pages.
fn verify_series(
    proofs: &[SnapshotProof],
    canister: &str,
    scope: &SnapshotScope,
    trust: &Trust,
) -> Result<Series> {
    ensure(
        !proofs.is_empty() && proofs.len() <= 10_000,
        Error::QuotaExceeded,
    )?;
    let caller = proofs[0].caller.as_str();
    let mut after: Option<String> = None;
    let mut fixed = None;
    let mut rolling: Option<[u8; 32]> = None;
    let mut rows = vec![];
    for (index, proof) in proofs.iter().enumerate() {
        let page = verify_page(proof, canister, caller, scope, after.as_deref(), trust)?;
        ensure(
            page.freeze.cutover_id.as_ref().map(|c| c.as_slice()) == Some(trust.cutover.as_slice()),
            Error::UnsupportedProtocol,
        )?;
        let descriptor = (
            page.source_context.to_vec(),
            page.initial_digest.to_vec(),
            page.inventory_digest.to_vec(),
            page.count,
            page.freeze.epoch,
        );
        ensure(
            fixed.as_ref().is_none_or(|f| *f == descriptor),
            Error::IntegrityFailed,
        )?;
        let mut digest: [u8; 32] = match rolling {
            Some(digest) => digest,
            None => page
                .initial_digest
                .as_slice()
                .try_into()
                .map_err(|_| Error::IntegrityFailed)?,
        };
        for (key, value) in &page.entries {
            digest = fold(&digest, key, value);
            rows.push((key.clone(), value.to_vec()));
        }
        rolling = Some(digest);
        ensure(
            page.complete == (index == proofs.len() - 1),
            Error::RecoveryIncomplete,
        )?;
        after = page.next.clone();
        fixed = Some(descriptor);
    }
    let (_, _, inventory, count, epoch) = fixed.ok_or(Error::IntegrityFailed)?;
    ensure(
        rows.len() as u64 == count && rolling.map(|r| r.to_vec()) == Some(inventory),
        Error::RecoveryIncomplete,
    )?;
    Ok(Series { epoch, rows })
}

// ---------------------------------------------------------------------------
// Shared channel inheritance.

/// `dmsg-legacy-shared-proposal/1`: one version of the official successor.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SharedProposal {
    /// `dmsg-legacy-shared-proposal/1`.
    pub format: String,
    /// The old channel canister.
    pub source: String,
    /// The old channel ID.
    pub channel: u32,
    /// The frozen source's digest.
    pub source_digest: String,
    /// Proposal version, from 1.
    pub version: u64,
    /// The new owner account.
    pub owner: String,
    /// The new channel.
    pub channel_id: String,
    /// SHA-256 of its signed genesis.
    pub genesis_digest: String,
    /// Random, hex.
    pub nonce: String,
}

impl SharedProposal {
    /// Its digest, hex.
    pub fn digest(&self) -> String {
        hex::encode(*dmsg_protocol::digest(
            "dmsg/legacy-shared-proposal/v1",
            self,
        ))
    }

    /// What an old manager approves for it.
    pub fn manager_consent(&self, manager: &str) -> [u8; 32] {
        *dmsg_protocol::digest("dmsg/legacy-manager-consent/v1", &(self.digest(), manager))
    }
}

/// `dmsg-legacy-member-claim/1`: an old member's new account and device.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MemberClaim {
    /// `dmsg-legacy-member-claim/1`.
    pub format: String,
    /// The proposal it joins.
    pub proposal_digest: String,
    /// The old member.
    pub member: String,
    /// The new account.
    pub account: String,
    /// The new device.
    pub device: String,
    /// That device's HPKE public key, hex.
    pub hpke_pub: String,
    /// The account's security epoch.
    pub security_epoch: u64,
}

impl MemberClaim {
    /// What the old member approves.
    pub fn digest(&self) -> [u8; 32] {
        *dmsg_protocol::digest("dmsg/legacy-member-claim/v1", self)
    }
}

/// The directory key of one old channel.
pub fn source_key(source: &str, channel: u32) -> String {
    hex::encode(*dmsg_protocol::digest(
        "dmsg/legacy-channel-key/v1",
        &(source, channel),
    ))
}

/// The frozen source of a shared channel.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SharedSource {
    /// The old channel canister.
    pub source: String,
    /// The old name-identity canister.
    pub identity: String,
    /// The old channel ID.
    pub channel: u32,
    /// The cutover, hex.
    pub cutover: String,
    /// The channel canister's freeze epoch.
    pub epoch: String,
    /// The name-identity canister's freeze epoch.
    pub identity_epoch: String,
    /// Digest of the whole source.
    pub digest: String,
    /// Frozen managers.
    pub managers: Vec<String>,
    /// Frozen members, managers included.
    pub members: Vec<String>,
    /// Name identities and their frozen administrators.
    pub named: BTreeMap<String, Vec<String>>,
    /// First frozen message ID.
    pub message_start: u32,
    /// Last frozen message ID.
    pub message_end: u32,
    /// SHA-256 of the wrapped DEK, hex.
    pub dek_digest: String,
}

impl SharedSource {
    /// Whether `caller` may act for the frozen identity: a named identity
    /// only through one of its frozen administrators, anyone else itself.
    pub fn controls(&self, identity: &str, caller: &str) -> bool {
        match self.named.get(identity) {
            Some(admins) => caller != identity && admins.iter().any(|a| a == caller),
            None => caller == identity,
        }
    }
}

/// The digest of a shared channel's frozen source: the channel's
/// authority row and the complete name roles, under one cutover.
fn source_digest(
    (source, channel, epoch): (&str, u32, u64),
    cutover: &[u8; 32],
    authority: &[u8],
    (identity, identity_epoch): (&str, u64),
    names: &[(String, Vec<u8>)],
) -> String {
    let rows: Vec<(&str, &serde_bytes::Bytes)> = names
        .iter()
        .map(|(k, v)| (k.as_str(), serde_bytes::Bytes::new(v)))
        .collect();
    hex::encode(*dmsg_protocol::digest(
        "dmsg/legacy-channel-source/v1",
        &(
            source,
            channel,
            epoch,
            serde_bytes::Bytes::new(cutover),
            serde_bytes::Bytes::new(authority),
            identity,
            identity_epoch,
            hex_sha256(&canonical(&rows)),
        ),
    ))
}

#[derive(CandidType, Deserialize)]
struct Authority {
    id: u32,
    managers: Vec<Principal>,
    members: Vec<Principal>,
    message_start: u32,
    latest_message_id: u32,
    dek_digest: ByteBuf,
    storage: Option<(Principal, u32)>,
}

/// The frozen ACL and name roles behind a shared channel.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceInput {
    /// The old channel ID.
    pub channel: u32,
    /// The channel's `ChannelAuthority` page.
    pub authority: ProofWire,
    /// Every `Authorities` page of the name-identity canister.
    pub authorities: Vec<ProofWire>,
}

/// Verify the frozen ACL of `input.channel` on `source` and the complete
/// name roles of `identity`.
///
/// # Errors
/// Anything uncertified, out of the freeze, incomplete or inconsistent
/// returns an error; nothing is guessed.
pub fn verify_source(
    input: &SourceInput,
    source: &str,
    identity: &str,
    trust: &Trust,
) -> Result<SharedSource> {
    let control = verify_series(
        &[SnapshotProof::from(&input.authority)],
        source,
        &SnapshotScope::ChannelAuthority(input.channel),
        trust,
    )?;
    let names = verify_series(
        &input
            .authorities
            .iter()
            .map(SnapshotProof::from)
            .collect::<Vec<_>>(),
        identity,
        &SnapshotScope::Authorities,
        trust,
    )?;
    let [(key, row)] = control.rows.as_slice() else {
        return Err(Error::IntegrityFailed);
    };
    ensure(
        *key == format!("{:010}", input.channel),
        Error::IntegrityFailed,
    )?;
    let authority: Authority = candid::decode_one(row).map_err(|_| Error::IntegrityFailed)?;
    let text = |list: &[Principal]| list.iter().map(Principal::to_text).collect::<Vec<_>>();
    let managers = text(&authority.managers);
    let members = text(&authority.members);
    let unique = |list: &[String]| list.iter().collect::<BTreeSet<_>>().len() == list.len();
    ensure(
        authority.id == input.channel
            && !managers.is_empty()
            && managers.len() <= 100
            && members.len() <= 100
            && unique(&managers)
            && unique(&members)
            && authority.dek_digest.len() == 32,
        Error::IntegrityFailed,
    )?;
    let mut named = BTreeMap::new();
    for (key, value) in &names.rows {
        let (name, owner, roles): (String, Principal, Vec<(Principal, i8)>) =
            candid::decode_args(value).map_err(|_| Error::IntegrityFailed)?;
        let admins = roles
            .iter()
            .filter(|(_, role)| *role == 1)
            .map(|(p, _)| p.to_text())
            .collect();
        ensure(
            name == *key && named.insert(owner.to_text(), admins).is_none(),
            Error::IntegrityFailed,
        )?;
    }
    let digest = source_digest(
        (source, input.channel, control.epoch),
        trust.cutover,
        row,
        (identity, names.epoch),
        &names.rows,
    );
    let mut everyone = members.clone();
    for manager in &managers {
        if !everyone.contains(manager) {
            everyone.push(manager.clone());
        }
    }
    Ok(SharedSource {
        source: source.into(),
        identity: identity.into(),
        channel: input.channel,
        cutover: hex::encode(trust.cutover),
        epoch: control.epoch.to_string(),
        identity_epoch: names.epoch.to_string(),
        digest,
        managers,
        members: everyone,
        named,
        message_start: authority.message_start,
        message_end: authority.latest_message_id,
        dek_digest: hex::encode(&authority.dek_digest),
    })
}

/// An old identity's approval of a migration intent.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    /// The `Attestation` page.
    pub proof: ProofWire,
    /// Unix milliseconds.
    pub expires_at: u64,
}

/// Check that the old caller approved `digest` until `expires_at` on the
/// frozen source, and return that caller.
///
/// # Errors
/// Returns `Error::Expired` past the approval and `Error::Forbidden` when
/// it approved anything else.
pub fn verify_approval(
    approval: &Approval,
    digest: &[u8; 32],
    source: &SharedSource,
    trust: &Trust,
) -> Result<String> {
    ensure(trust.now < approval.expires_at, Error::Expired)?;
    let proof = SnapshotProof::from(&approval.proof);
    let scope = SnapshotScope::Attestation {
        digest: ByteBuf::from(digest.to_vec()),
        expires_at: approval.expires_at,
    };
    let series = verify_series(std::slice::from_ref(&proof), &source.source, &scope, trust)?;
    let [(key, row)] = series.rows.as_slice() else {
        return Err(Error::Forbidden);
    };
    ensure(
        series.epoch.to_string() == source.epoch && *key == proof.caller,
        Error::Forbidden,
    )?;
    let (caller, value, expires): (Principal, ByteBuf, u64) =
        candid::decode_args(row).map_err(|_| Error::IntegrityFailed)?;
    ensure(
        caller.to_text() == proof.caller
            && value.as_slice() == digest.as_slice()
            && expires == approval.expires_at,
        Error::Forbidden,
    )?;
    Ok(proof.caller)
}

// ---------------------------------------------------------------------------
// Frozen comparison of a personal archive.

#[derive(CandidType, Deserialize)]
enum FolderMode {
    Draining,
    ReadOnly,
}

#[derive(CandidType, Deserialize)]
struct FolderStatus {
    folder: u32,
    cutover_id: ByteBuf,
    baseline_evidence: ByteBuf,
    mode: FolderMode,
    frozen_at: u64,
    files: u64,
    digest: ByteBuf,
}

#[derive(CandidType, Deserialize)]
struct FrozenFile {
    id: u32,
    metadata: ByteBuf,
    chunks: Vec<(u32, u32, ByteBuf)>,
    missing: Vec<u32>,
    complete: bool,
    ciphertext_digest: ByteBuf,
}

#[derive(CandidType, Deserialize)]
struct FolderPage {
    status: FolderStatus,
    entries: Vec<FrozenFile>,
    next: Option<u32>,
    complete: bool,
}

/// One certified `legacy_folder_manifest` page; only the argument's SHA-256
/// is kept, since the argument carries a download token.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FolderProof {
    /// [`FOLDER_PROOF`].
    pub format: String,
    /// The old bucket.
    pub canister: String,
    /// The old identity.
    pub caller: String,
    /// SHA-256 of the Candid arguments.
    #[serde(with = "serde_bytes")]
    pub argument_digest: Vec<u8>,
    /// Ingress expiry in nanoseconds, decimal.
    pub expiry: String,
    /// The ingress nonce.
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    /// The request ID.
    #[serde(with = "serde_bytes")]
    pub request_id: Vec<u8>,
    /// The IC certificate of the reply.
    #[serde(with = "serde_bytes")]
    pub certificate: Vec<u8>,
    /// The Candid reply.
    #[serde(with = "serde_bytes")]
    pub reply: Vec<u8>,
}

fn verify_folder(
    proof: &FolderProof,
    canister: &str,
    caller: &str,
    folder: u32,
    trust: &Trust,
) -> Result<FolderPage> {
    ensure(
        proof.format == FOLDER_PROOF && proof.canister == canister && proof.caller == caller,
        Error::IntegrityFailed,
    )?;
    let arg: [u8; 32] = proof
        .argument_digest
        .as_slice()
        .try_into()
        .map_err(|_| Error::IntegrityFailed)?;
    certified_reply(
        &principal(canister)?,
        &principal(caller)?,
        "legacy_folder_manifest",
        &arg,
        Ingress {
            expiry: &proof.expiry,
            nonce: &proof.nonce,
            request_id: &proof.request_id,
            certificate: &proof.certificate,
            reply: &proof.reply,
        },
        trust,
    )?;
    let reply: std::result::Result<FolderPage, String> =
        candid::decode_one(&proof.reply).map_err(|_| Error::IntegrityFailed)?;
    let page = reply.map_err(|_| Error::UnsupportedProtocol)?;
    ensure(
        matches!(page.status.mode, FolderMode::ReadOnly)
            && page.status.folder == folder
            && page.status.cutover_id.as_slice() == trust.cutover.as_slice()
            && page.status.files <= 100_000
            && page.entries.len() <= 16,
        Error::IntegrityFailed,
    )?;
    Ok(page)
}

/// The final history of one old channel, exported after the freeze.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FrozenChannel {
    /// `<canister>/channel/<id>`.
    pub channel: String,
    /// Its `ChannelAuthority` page.
    pub authority: SnapshotProof,
    /// Every `Messages` page.
    pub messages: Vec<SnapshotProof>,
    /// Every attachment manifest page.
    pub folder: Vec<FolderProof>,
}

/// A difference between the archive and the frozen service.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// Its source key.
    pub source: String,
    /// What differs.
    pub reason: String,
}

#[derive(CandidType, Deserialize)]
struct FrozenAuthority {
    id: u32,
    message_start: u32,
    latest_message_id: u32,
    dek_digest: ByteBuf,
    storage: Option<(Principal, u32)>,
}

#[derive(CandidType, Deserialize)]
struct FrozenMessage {
    id: u32,
    kind: u8,
    reply_to: u32,
    created_by: Principal,
    created_at: u64,
    payload: ByteBuf,
}

/// The archive observation of a frozen message, as the collector made it.
fn message_digest(message: &FrozenMessage) -> String {
    use cbor2::Value;
    let wrap = |name: &str, value: Value| Value::Map(vec![(Value::Text(name.into()), value)]);
    let value = Value::Map(vec![
        (Value::Text("id".into()), Value::Integer(message.id.into())),
        (
            Value::Text("kind".into()),
            Value::Integer(message.kind.into()),
        ),
        (
            Value::Text("reply_to".into()),
            Value::Integer(message.reply_to.into()),
        ),
        (
            Value::Text("created_by".into()),
            wrap("principal", Value::Text(message.created_by.to_text())),
        ),
        (
            Value::Text("created_at".into()),
            wrap("integer", Value::Text(message.created_at.to_string())),
        ),
        (
            Value::Text("payload".into()),
            wrap("bytes", Value::Bytes(message.payload.to_vec())),
        ),
    ]);
    hex_sha256(&canonical(&value))
}

/// Compare a personal archive with the old service's final, frozen history
/// of each channel in `proofs`: changed, added or missing messages, a
/// changed channel key, and changed, missing or incomplete attachments are
/// listed by channel.
///
/// # Errors
/// Returns `Error::Forbidden` for an unreviewed channel or another
/// identity's evidence, `Error::RecoveryIncomplete` for an unreviewed bucket
/// or missing pages, and an error for evidence that does not verify.
pub fn compare_frozen(
    archive: &Archive,
    proofs: &[FrozenChannel],
    channels: &[String],
    buckets: &[String],
    trust: &Trust,
) -> Result<BTreeMap<String, Vec<Change>>> {
    let owner = &archive.inventory.principal;
    let find = |key: &str, kind: &str| {
        archive
            .inventory
            .objects
            .iter()
            .find(|o| o.key == key && o.kind == kind)
    };
    let mut covered = BTreeMap::new();
    for proof in proofs {
        let mut changes = vec![];
        let (source, id) = proof
            .channel
            .rsplit_once("/channel/")
            .ok_or(Error::Forbidden)?;
        let channel: u32 = id.parse().map_err(|_| Error::Forbidden)?;
        ensure(
            channels.iter().any(|c| c == source) && id == channel.to_string(),
            Error::Forbidden,
        )?;
        ensure(
            proof.authority.caller == *owner
                && proof.messages.first().is_some_and(|m| m.caller == *owner),
            Error::Forbidden,
        )?;
        let authority = verify_series(
            std::slice::from_ref(&proof.authority),
            source,
            &SnapshotScope::ChannelAuthority(channel),
            trust,
        )?;
        let rows = verify_series(
            &proof.messages,
            source,
            &SnapshotScope::Messages(channel),
            trust,
        )?;
        let current: FrozenAuthority =
            candid::decode_one(&authority.rows.first().ok_or(Error::IntegrityFailed)?.1)
                .map_err(|_| Error::IntegrityFailed)?;
        let wrapped = find(&proof.channel, "channel")
            .and_then(|o| observation_value(o, "dek"))
            .map(|dek| hex_sha256(&dek));
        if wrapped.as_deref() != Some(hex::encode(&current.dek_digest).as_str()) {
            changes.push(Change {
                source: proof.channel.clone(),
                reason: "channel-key-changed-or-missing".into(),
            });
        }
        for (_, bytes) in &rows.rows {
            let (id, message): (u32, Option<FrozenMessage>) =
                candid::decode_args(bytes).map_err(|_| Error::IntegrityFailed)?;
            let key = format!("{}/message/{id}", proof.channel);
            let object = find(&key, "message");
            let Some(message) = message else {
                if object
                    .and_then(|o| observation_value(o, "payload"))
                    .is_some_and(|p| !p.is_empty())
                {
                    changes.push(Change {
                        source: key,
                        reason: "source-slot-absent-cache-retained".into(),
                    });
                }
                continue;
            };
            if object.map(|o| o.digest.as_str()) != Some(message_digest(&message).as_str()) {
                changes.push(Change {
                    reason: match object {
                        Some(_) => "message-changed",
                        None => "message-added",
                    }
                    .into(),
                    source: key,
                });
            }
        }
        if let Some((bucket, folder)) = current.storage {
            let bucket = bucket.to_text();
            ensure(
                buckets.contains(&bucket) && !proof.folder.is_empty(),
                Error::RecoveryIncomplete,
            )?;
            let mut count = 0u64;
            let mut last: Option<u32> = None;
            let mut digest: Option<Vec<u8>> = None;
            for (index, frozen) in proof.folder.iter().enumerate() {
                let page = verify_folder(frozen, &bucket, owner, folder, trust)?;
                ensure(
                    digest
                        .as_ref()
                        .is_none_or(|d| *d == page.status.digest.to_vec()),
                    Error::IntegrityFailed,
                )?;
                digest = Some(page.status.digest.to_vec());
                ensure(
                    page.complete == (index == proof.folder.len() - 1),
                    Error::RecoveryIncomplete,
                )?;
                for file in &page.entries {
                    ensure(last.is_none_or(|l| file.id > l), Error::IntegrityFailed)?;
                    last = Some(file.id);
                    count += 1;
                    let key = format!("{bucket}/file/{}", file.id);
                    let stored = find(&key, "file")
                        .and_then(|o| observation_value(o, "ciphertext"))
                        .map(|c| hex_sha256(&c));
                    if !file.complete
                        || stored.as_deref() != Some(hex::encode(&file.ciphertext_digest).as_str())
                    {
                        changes.push(Change {
                            source: key,
                            reason: "file-changed-missing-or-incomplete".into(),
                        });
                    }
                }
                if page.complete {
                    ensure(count == page.status.files, Error::RecoveryIncomplete)?;
                }
            }
        }
        covered.insert(proof.channel.clone(), changes);
    }
    Ok(covered)
}

#[cfg(test)]
#[path = "legacy_proof_tests.rs"]
mod tests;
