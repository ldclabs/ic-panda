//! Formal channels (PRD §8, implementation design §7): end-to-end encrypted
//! group conversations whose membership is a signed control log the relay
//! orders and every member replays ([`crate::channel_log`]).
//!
//! What the account keeps across its devices is one synced content object
//! per channel (`formal_channel`): its name, the genesis it trusts and every
//! epoch key it holds, sealed under the content root. A device that joins
//! the account later reads old epochs with those keys. Everything else is
//! device-local and sealed under the LDK: the verified control log with the
//! channel after each control, received messages, and the journals of
//! operations in flight, so a lost reply is reconciled by its request ID and
//! never signed into a second, different operation.
//!
//! Epoch keys reach devices as HPKE envelopes: a coordinator holding a
//! rotation lease seals a random key to every active `ContentSign` device of
//! every member, uploads the envelopes and activates the epoch. Joining
//! members read only new epochs unless a manager seals older keys to their
//! devices (a history grant). Messages are sealed under a per-message key
//! derived from the epoch key and signed by the device; readers check each
//! against the evidence the relay stored with it.

use crate::channel_log::{
    Action, Activation, ChannelType, Control, EnvelopeRef, Genesis, Lease, Ledger, MessagePayload,
    Reactivate, Recipient, Role, Status, advance, is_hash, may_publish, may_rotate, readable,
    recipients_digest, start,
};
use crate::client::Client;
use crate::cloud::{CLOUD_PROTOCOL, Evidence, Prepared, sign_command};
use crate::codec::{MAX_RECORD, b64, canonical, decode, digest, hex_sha256, unb64};
use crate::content::empty_file;
use crate::crypto::{
    HpkeEnvelope, derive, hpke_open, hpke_seal, open, open_bytes, random_key, seal, seal_bytes,
    sign,
};
use crate::evidence::{Trust, Verified};
use crate::files::{
    BlobStore, FileManifest, MAX_FILE, Source, chunk_count, export_manifest, read_file,
};
use crate::hold::Hold;
use crate::library::catalog::Origin;
use crate::library::import::Imported;
use crate::now_ms;
use crate::upload::{ChunkRef, UploadPlan, hex_id, random_hex};
use dmsg_types::{AccountId, Error, Hash, Result, ensure, ensure_valid, user::Capability};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_bytes::{ByteBuf, Bytes};
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use zeroize::Zeroizing;

/// Content kind of a channel's synced record.
pub const CHANNEL: &str = "formal_channel";
/// Largest message text in bytes.
pub const MAX_TEXT: usize = 16_000;
/// Largest channel name in characters.
pub const MAX_NAME: usize = 100;

const FORMAT: &str = "dmsg-channel-client/2";
const GENESIS: &str = "dmsg/channel/genesis/v1";
const CONTROL: &str = "dmsg/channel/control/v1";
const ACTIVATE: &str = "dmsg/channel/activate/v1";
const REACTIVATE: &str = "dmsg/channel/reactivate/v1";
const MESSAGE: &str = "dmsg/channel/message/v1";
const RESERVE: &str = "dmsg/upload/reserve/v1";
const BILLING_TRANSFER: &str = "dmsg/channel/billing/transfer/v1";
const CURSOR: &str = "dmsg/channel/cursor/v1";
const LOCAL: &str = "channel:";
const LOG: &str = "channel-control:";
pub(crate) const INBOX: &str = "channel-msg:";
const JOB: &str = "channel-job:";
/// The chunks of channel uploads in flight, by upload, until their plans lapse.
const UPLOADS: &str = "channel-uploads";
/// Envelopes per upload.
const GROUP: usize = 120;
/// How long an invitation stays open.
const INVITATION_MS: u64 = dmsg_types::DAY;
/// Upload plans lapse after an hour.
const PLAN_MS: u64 = 3_600_000;
/// Largest image shown in place.
const PREVIEW: u64 = 4 * 1024 * 1024;

/// What the account keeps of a channel on every device.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Keyring {
    format: String,
    id: String,
    name: String,
    /// SHA-256 of the genesis command this account trusts, hex.
    genesis: String,
    /// Epoch keys, base64url.
    keys: BTreeMap<u64, String>,
    /// No notifications for it.
    #[serde(default)]
    muted: bool,
    /// Kept out of the channel list, as after leaving.
    #[serde(default)]
    hidden: bool,
}

/// One device's replay of a channel, sealed under the LDK.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
struct Local {
    /// Relay cursor of the control log.
    control: u64,
    /// Relay sequence of the last message read.
    message: u64,
    /// The channel after the latest control.
    ledger: Option<Ledger>,
    /// Control sequence of each head.
    heads: BTreeMap<String, u64>,
    /// Control sequence of each epoch's activation.
    epochs: BTreeMap<u64, u64>,
    /// Control sequences of history grants and sealed invitations to this
    /// account.
    histories: Vec<u64>,
    /// Epoch ranges granted to this account whose messages are still to be
    /// read again.
    granted: Vec<(u64, u64)>,
    /// The first and last sequence of each epoch's messages this device
    /// could not read, so a grant reads only them again.
    skipped: BTreeMap<u64, (u64, u64)>,
    /// Edits and retractions read before the message they name, by its
    /// sequence.
    amends: BTreeMap<u64, Vec<u64>>,
    /// Messages read through this sequence.
    read: u64,
    /// Others' messages after `read`.
    unread: u64,
    /// The latest message, for the channel list.
    last: Option<Preview>,
    /// The unsent text of the composer.
    draft: String,
    /// The relay's billing of the channel, as last read.
    billing: Option<Json>,
    /// Invitations this device made, to hand over again.
    invites: BTreeMap<String, Invitation>,
    /// The delivery cursor last reported to the relay.
    delivered: u64,
}

/// The latest message of a channel, for its list.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// Relay sequence.
    pub seq: u64,
    /// The author's account.
    pub account: String,
    /// When the relay stored it.
    pub stored_at: u64,
    /// Its text's start, or the attached file's name.
    pub text: String,
}

/// A control this device verified, and the channel after it.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Stored {
    seq: u64,
    head: String,
    stored_at: u64,
    account: String,
    device: String,
    action: String,
    payload: Json,
    acceptance: Option<Evidence>,
    ledger: Ledger,
}

/// A control event as the relay returns it.
#[derive(Deserialize)]
struct Event {
    signed: SignedRef,
    head: String,
    control_seq: u64,
    evidence: Evidence,
    #[serde(default)]
    acceptance_evidence: Option<Evidence>,
    stored_at: u64,
}

#[derive(Deserialize)]
struct SignedRef {
    cose_sign1: String,
}

/// A signed operation in flight, journaled before it is sent.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Operation {
    action: String,
    path: String,
    payload: Json,
    request_id: String,
    cose: String,
    deadline: u64,
    /// Another account approved these exact bytes: never signed again.
    fixed: bool,
    result: Option<Json>,
}

/// A message to send, kept until the relay stores it.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Outgoing {
    draft: Draft,
    payload: MessagePayload,
}

/// A channel upload in flight: what the chunk collectors keep for it.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Held {
    expires_at: u64,
    chunks: Vec<String>,
}

/// An upload a channel operation needs; its chunks are in the chunk store.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Upload {
    pub(crate) plan: UploadPlan,
    manifest: ByteBuf,
    recipients: Vec<String>,
}

/// A rotation under way.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Rotation {
    head: String,
    lease_id: String,
    lease: Option<Lease>,
}

/// The key a rotation seals, kept so a retry seals the same one.
#[derive(Serialize, Deserialize)]
struct Candidate {
    lease: Lease,
    key: ByteBuf,
    groups: Vec<Upload>,
}

/// A history grant being prepared.
#[derive(Serialize, Deserialize)]
struct HistoryPlan {
    scope: HistoryScope,
    uploads: Vec<Upload>,
}

/// What a history grant covers; the envelopes are bound to it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HistoryScope {
    /// Channel ID.
    pub channel: String,
    /// The receiving account.
    pub target: String,
    /// First epoch.
    pub from: u64,
    /// Last epoch.
    pub to: u64,
    /// Grant ID.
    pub grant_id: String,
    /// The head the grant builds on.
    pub head: String,
    /// The epoch then.
    pub epoch: u64,
    /// Digest of the target's devices.
    pub recipients_digest: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryRef {
    recipient: String,
    from: u64,
    to: u64,
    upload_id: String,
    chunk: u64,
    digest: String,
    size: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryManifest {
    format: String,
    scope: HistoryScope,
    envelopes: Vec<HistoryRef>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EpochManifest {
    format: String,
    channel: String,
    epoch: u64,
    lease_id: String,
    head: String,
    fencing: u64,
    recipients_digest: String,
    key_confirmation: String,
    group: u64,
}

/// A file attached to a message; the key stays in the daemon.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileRef {
    /// The upload holding the file.
    pub upload_id: String,
    /// File ID.
    pub file_id: String,
    /// File version.
    pub version: String,
    /// The epoch it was shared in.
    pub epoch: u64,
    /// SHA-256 of the sealed manifest.
    pub manifest_digest: String,
    /// File name.
    pub name: String,
    /// Length.
    pub size: u64,
    /// Media type.
    pub mime: String,
    /// The file key, base64url; empty in views.
    pub key: String,
}

/// A message another one names: its relay sequence and the digest of its
/// signed command, so the reference binds those exact bytes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MessageRef {
    /// Relay sequence.
    pub seq: u64,
    /// SHA-256 of its signed command, hex.
    pub digest: String,
}

/// The Agent ID that wrote a message through this account's device: the
/// account's claim, signed by its device like the rest of the body.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentRef {
    /// The Agent ID.
    pub id: String,
    /// Its name in the account.
    pub name: String,
}

/// What a message says, as its author writes it; the file goes apart.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    /// The text.
    #[serde(default)]
    pub text: String,
    /// The message it answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<MessageRef>,
    /// An earlier message of the same author whose text this replaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit_of: Option<MessageRef>,
    /// An earlier message of the same author it withdraws: readers drop
    /// its text and file, but copies already taken stay with them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retract: Option<MessageRef>,
    /// The Agent ID that wrote it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentRef>,
    /// An ownership or billing transfer packet handed over in the channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packet: Option<Json>,
}

/// The sealed body of a message.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file: Option<FileRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply_to: Option<MessageRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    edit_of: Option<MessageRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    retract: Option<MessageRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    agent: Option<AgentRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    packet: Option<Json>,
}

/// A file being shared, kept until its message is stored: the shared
/// edition's upload and the reference the message carries.
#[derive(Serialize, Deserialize)]
struct Attachment {
    /// The library version shared, `file@version`.
    source: String,
    reference: FileRef,
    upload: Upload,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelFile {
    format: String,
    channel: String,
    epoch: u64,
    file: FileManifest,
}

/// A message read from a channel.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Message {
    /// The relay accepted deletion of this attachment; the message history remains.
    #[serde(default)]
    pub file_deleted: bool,
    /// Relay sequence.
    pub seq: u64,
    /// The author's account.
    pub account: String,
    /// The author's device.
    pub device: String,
    /// Message ID.
    pub message_id: String,
    /// Epoch.
    pub epoch: u64,
    /// The author's clock.
    pub created_at: u64,
    /// When the relay stored it.
    pub stored_at: u64,
    /// The text.
    pub text: String,
    /// An attached file.
    pub file: Option<FileRef>,
    /// SHA-256 of its signed command, hex: what references name.
    pub digest: String,
    /// The message it answers.
    pub reply_to: Option<MessageRef>,
    /// The message whose text it replaces; applied to that one.
    pub edit_of: Option<MessageRef>,
    /// The message it withdraws; applied to that one.
    pub retract: Option<MessageRef>,
    /// The Agent ID that wrote it.
    pub agent: Option<AgentRef>,
    /// A transfer packet handed over in the channel.
    pub packet: Option<Json>,
    /// Its body did not open, or its epoch's key is not here: the cursor
    /// moved past it, and reading its epoch again retries it.
    pub unreadable: bool,
    /// When its author last edited it.
    pub edited_at: Option<u64>,
    /// Withdrawn by its author: its text and file were dropped here.
    pub retracted: bool,
    /// Hidden on this device.
    pub hidden: bool,
}

impl Message {
    /// An edit or a retraction, shown through the message it names.
    pub fn amends(&self) -> bool {
        self.edit_of.is_some() || self.retract.is_some()
    }

    /// The message as it may leave the daemon: no file key.
    pub fn public(&self) -> Self {
        let mut message = self.clone();
        if let Some(file) = &mut message.file {
            file.key = String::new();
        }
        message
    }
}

/// A channel as this device knows it.
#[derive(Serialize, Clone, Debug, Default)]
pub struct ChannelView {
    /// Channel ID.
    pub id: String,
    /// Name, as this account calls it.
    pub name: String,
    /// The trusted genesis.
    pub genesis: String,
    /// The channel, once its log was read.
    pub ledger: Option<Ledger>,
    /// Relay sequence of the last message read.
    pub message_cursor: u64,
    /// Epochs this account holds keys for.
    pub readable_epochs: Vec<u64>,
    /// No notifications for it.
    pub muted: bool,
    /// Kept out of the channel list.
    pub hidden: bool,
    /// An unactivated legacy draft that can be safely discarded.
    pub deletable_legacy_draft: bool,
    /// Messages read through this sequence.
    pub read: u64,
    /// Others' messages after `read`.
    pub unread: u64,
    /// The latest message.
    pub last: Option<Preview>,
    /// The composer's unsent text.
    pub draft: String,
    /// The relay's billing of the channel: payer, version and lifecycle,
    /// with the billing transfer under way, as last read.
    pub billing: Option<Json>,
    /// Open invitations this device made, by ID.
    pub invites: BTreeMap<String, Invitation>,
}

/// What a channel sync read.
#[derive(Serialize, Clone, Debug)]
pub struct ChannelSync {
    /// Relay sequence read through.
    pub through: u64,
    /// New messages.
    pub received: usize,
}

/// The step a channel read takes next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// A page of the control log.
    Control,
    /// The relay's view of the channel, once the log is read.
    Head,
    /// A page of messages.
    Messages,
    /// The keys of the epochs a page of messages needs, then the page again.
    Epochs,
    /// The keys of the epochs granted to this account, whose messages this
    /// device skipped, then those messages again.
    Grants,
    /// Read through.
    Done,
}

/// A read of a channel, page by page, carried out by [`read`]: each page
/// is asked for and applied under the client and waited for without it.
#[derive(Clone, Debug)]
pub struct Reading {
    /// Channel ID.
    pub channel: String,
    step: Step,
    /// The control cursor this device held when the page of the log, or the
    /// head, now on its way was asked for.
    control: u64,
    /// Read messages after the control log.
    messages: bool,
    /// Messages read again in `(after, through]`, the cursor untouched.
    range: Option<(u64, u64)>,
    /// The range reads epochs granted to this account.
    granting: bool,
    invitation: Option<String>,
    pinned: Option<String>,
    /// The messages' cursor within this read.
    after: Option<u64>,
    /// The head a message page outran, which the log must move past.
    stale: Option<String>,
    ledger: Option<Ledger>,
    /// Epochs whose keys the page waits for.
    opening: Vec<u64>,
    /// Epochs this read tried to open; their messages left unread stay so.
    tried: BTreeSet<u64>,
    /// New messages read, edits and withdrawals aside.
    pub received: usize,
}

impl Reading {
    /// The control log, then the new messages.
    pub fn new(channel: &str) -> Self {
        Self {
            messages: true,
            ..Self::controls(channel)
        }
    }

    /// The control log alone.
    pub fn controls(channel: &str) -> Self {
        Self {
            channel: channel.into(),
            step: Step::Control,
            control: 0,
            messages: false,
            range: None,
            granting: false,
            invitation: None,
            pinned: None,
            after: None,
            stale: None,
            ledger: None,
            opening: vec![],
            tried: BTreeSet::new(),
            received: 0,
        }
    }

    /// Whether it is through.
    pub fn done(&self) -> bool {
        self.step == Step::Done
    }

    /// The channel as verified, once its log was read.
    pub fn ledger(&self) -> Option<&Ledger> {
        self.ledger.as_ref()
    }
}

/// A journaled command, ready to send without holding the client.
pub enum Posting {
    /// The relay stored it already: its outcome.
    Done(Json),
    /// The request to send.
    Send(Prepared),
}

/// An image a message carries, downloaded without holding the client: the
/// plan of its upload as checked, then its sealed manifest and the chunks
/// not kept here, by path and digest. It holds no key; the image opens
/// under the client's copy of the message.
#[derive(Clone, Debug)]
struct Fetching {
    channel: String,
    seq: u64,
    plan: UploadPlan,
    /// The sealed manifest first, then the chunks.
    parts: Vec<(String, String)>,
}

/// An invitation the inviter hands to the invited account.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Invitation {
    /// `dmsg-channel-invitation/1`.
    pub format: String,
    /// Channel ID.
    pub channel: String,
    /// The genesis to trust.
    pub genesis: String,
    /// The head after the invitation.
    pub head: String,
    /// Invitation ID.
    pub invitation: String,
    /// The invited account.
    pub account: String,
    /// The channel's name.
    pub name: String,
    /// Unix milliseconds.
    pub expires_at: u64,
}

/// The command context a proposal fixes: the owner signs the final command
/// with exactly these values.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Context {
    /// The proposing account.
    pub account_id: String,
    /// Its issuer.
    pub issuer: String,
    /// The proposing device, hex.
    pub device_id: String,
    /// Security epoch of the command.
    pub security_epoch: u64,
    /// Request ID of the command.
    pub request_id: String,
    /// Deadline of the command.
    pub deadline: u64,
}

/// What both owners approve: the command without its signature.
#[derive(Serialize)]
struct Unsigned<'a, P> {
    protocol: &'a str,
    action: &'a str,
    security_epoch: u64,
    request_id: &'a str,
    deadline: u64,
    payload: &'a P,
    issuer: &'a str,
    account_id: &'a str,
    device_id: &'a str,
}

fn unsigned<'a, P>(context: &'a Context, action: &'a str, payload: &'a P) -> Unsigned<'a, P> {
    Unsigned {
        protocol: CLOUD_PROTOCOL,
        action,
        security_epoch: context.security_epoch,
        request_id: &context.request_id,
        deadline: context.deadline,
        payload,
        issuer: &context.issuer,
        account_id: &context.account_id,
        device_id: &context.device_id,
    }
}

/// A transfer without the target's acceptance signature.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TransferIntent {
    /// The head it builds on.
    pub expected_head: String,
    /// The access-list version it builds on.
    pub expected_acl_version: u64,
    /// The transfer.
    pub action: TransferAction,
}

/// `{type: "transfer", account, device}`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TransferAction {
    /// Always `transfer`.
    #[serde(rename = "type")]
    pub kind: String,
    /// The new owner.
    pub account: String,
    /// The new owner's accepting device, hex.
    pub device: String,
}

/// An owner transfer: proposed by the owner, accepted by the target's
/// device, committed by the owner.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OwnerTransfer {
    /// `dmsg-channel-owner-transfer/1`.
    pub format: String,
    /// Channel ID.
    pub channel: String,
    /// The genesis both trust.
    pub genesis: String,
    /// The owner's command context.
    pub context: Context,
    /// The transfer.
    pub payload: TransferIntent,
    /// The owner's evidence.
    pub evidence: Evidence,
    /// Unix milliseconds.
    pub proposed_at: u64,
    /// The owner device's signature, base64url.
    pub proposal_signature: String,
    /// The target device's signature, base64url.
    pub acceptance: Option<String>,
    /// The target's evidence.
    pub target_evidence: Option<Evidence>,
    /// Unix milliseconds.
    pub accepted_at: Option<u64>,
}

/// A channel's billing moving to another account.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BillingIntent {
    /// Transition ID.
    pub transition_id: String,
    /// The billing version it builds on.
    pub expected_billing_version: u64,
    /// The relay's transition manifest.
    pub manifest_digest: String,
    /// Bytes moving.
    pub bytes: u64,
    /// The billing slot.
    pub slot_id: String,
    /// The new payer.
    pub to_payer: String,
    /// The payer's accepting device, hex.
    pub device: String,
}

/// A sponsor transfer: proposed by the owner, accepted by the payer's
/// device, committed by the owner.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Sponsor {
    /// `dmsg-channel-sponsor/1`.
    pub format: String,
    /// Channel ID.
    pub channel: String,
    /// The genesis.
    pub genesis: String,
    /// The owner's command context.
    pub context: Context,
    /// The billing change.
    pub payload: BillingIntent,
    /// The owner's evidence.
    pub evidence: Evidence,
    /// Unix milliseconds.
    pub proposed_at: u64,
    /// The owner device's signature, base64url.
    pub owner_signature: String,
    /// The payer device's signature, base64url.
    pub acceptance: Option<String>,
    /// The payer's evidence.
    pub target_evidence: Option<Evidence>,
    /// Unix milliseconds.
    pub accepted_at: Option<u64>,
}

/// Recipients of an epoch and the versions they were read at.
struct Current {
    recipients: Vec<Recipient>,
    versions: BTreeMap<String, u64>,
    expires_at: u64,
}

/// What opening an epoch's key takes.
enum Opening {
    /// The key is here.
    Held,
    /// This device has no envelope of the epoch: a history grant to the
    /// account, or the invitation it joined with, carries the key.
    History,
    /// This device's envelope of the epoch.
    Envelope(Box<Envelope>),
}

/// An epoch's activation and this device's envelope in it.
struct Envelope {
    stored: Stored,
    activation: Activation,
    reference: EnvelopeRef,
    /// The channel the epoch rotated from.
    before: Ledger,
    /// This device as a recipient.
    mine: String,
}

/// A history grant to this account, or a sealed invitation.
struct Grant {
    stored: Stored,
    control: Control,
    from_epoch: u64,
    to_epoch: u64,
    upload: String,
}

/// Carry out `reading`: each page is asked for and applied under the
/// client and waited for without it; the keys of the epochs a page needs,
/// and those history grants carry, open on the way.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for a page that does not continue what
/// this device verified.
pub async fn read<H: Hold>(h: &H, reading: &mut Reading) -> Result<()> {
    let trust = h.trust().await?;
    for _ in 0..100_000 {
        match reading.step {
            Step::Done => return Ok(()),
            Step::Epochs => {
                // An epoch that does not open leaves its messages unread.
                while let Some(epoch) = reading.opening.pop() {
                    reading.tried.insert(epoch);
                    match Box::pin(open_epoch(h, &reading.channel, epoch)).await {
                        Ok(())
                        | Err(Error::RecoveryIncomplete | Error::NotFound | Error::Forbidden) => {}
                        Err(error) => return Err(error),
                    }
                }
                reading.step = Step::Messages;
            }
            Step::Grants => {
                let epochs = h.with(|c| c.granted_epochs(&reading.channel)).await?;
                let mut opened = vec![];
                for (epoch, range) in epochs {
                    match Box::pin(open_epoch(h, &reading.channel, epoch)).await {
                        Ok(()) => opened.push(range),
                        // The relay or a user home is out of reach: the
                        // next read tries again.
                        Err(error @ Error::Unavailable(_)) => return Err(error),
                        // A grant that does not open here leaves its epoch
                        // skipped.
                        Err(_) => {}
                    }
                }
                h.with(|c| c.grants_opened(reading, &opened)).await?;
            }
            Step::Control | Step::Head | Step::Messages => {
                let page = h.send(|c| c.channel_read_request(reading)).await?;
                h.with(|c| c.channel_read_settle(reading, page, &trust))
                    .await?;
            }
        }
    }
    Err(Error::QuotaExceeded)
}

/// Read the control log to the relay's head, verifying every new event,
/// and check the relay's view against it. An invited account reads with
/// its invitation; `pinned` must be in the verified history.
///
/// # Errors
/// As [`read`].
pub async fn pull<H: Hold>(
    h: &H,
    channel: &str,
    invitation: Option<&str>,
    pinned: Option<&str>,
) -> Result<Ledger> {
    let mut reading = Reading {
        invitation: invitation.map(str::to_string),
        pinned: pinned.map(str::to_string),
        ..Reading::controls(channel)
    };
    read(h, &mut reading).await?;
    reading.ledger.ok_or(Error::IntegrityFailed)
}

/// Upload a channel upload whose chunks are in `source`.
async fn send_upload<H: Hold>(h: &H, channel: &str, upload: &Upload, source: Source) -> Result<()> {
    crate::upload::upload(
        h,
        &base(channel)?,
        &upload.plan,
        &upload.manifest,
        0,
        source,
        |_, _| Ok(()),
    )
    .await
    .map(|_| ())
}

/// A channel upload's signed plan, checked against the evidence the relay
/// stored with it; `uploader`, when given, must have signed it.
async fn descriptor<H: Hold>(
    h: &H,
    channel: &str,
    upload: &str,
    uploader: Option<&str>,
) -> Result<UploadPlan> {
    let value = h
        .get(&format!("{}/objects/{upload}", base(channel)?))
        .await?;
    plan_of(&h.trust().await?, &value, upload, uploader)
}

/// The plan in the relay's answer `value` for the upload `upload`, as
/// [`descriptor`] checks it.
fn plan_of(
    trust: &Trust,
    value: &Json,
    upload: &str,
    uploader: Option<&str>,
) -> Result<UploadPlan> {
    let event: Event = from(&json!({
        "signed": value["signed"],
        "head": "",
        "control_seq": 0,
        "evidence": value["evidence"],
        "stored_at": value["stored_at"],
    }))?;
    let verified: Verified<UploadPlan> = trust.command(
        &event.signed.cose_sign1,
        &event.evidence,
        event.stored_at,
        Some(RESERVE),
        now_ms(),
    )?;
    let plan: UploadPlan = from(&value["plan"])?;
    ensure(
        verified.body.payload == plan && plan.upload_id == upload,
        Error::IntegrityFailed,
    )?;
    if let Some(uploader) = uploader {
        ensure(
            verified.account.to_string() == uploader,
            Error::AuthRequired,
        )?;
    }
    Ok(plan)
}

/// One chunk, or the manifest, of a channel upload.
async fn channel_chunk<H: Hold>(
    h: &H,
    channel: &str,
    upload: &str,
    chunk: &str,
    digest: &str,
) -> Result<Vec<u8>> {
    h.chunk(
        &format!("{}/objects/{upload}/chunks/{chunk}", base(channel)?),
        digest,
    )
    .await
}

/// Install an epoch's key from this device's envelope; a device outside
/// the epoch's recipients needs a history grant. Each read of the relay
/// waits without the client.
///
/// # Errors
/// Returns `Error::Forbidden` for an epoch this account may not read and
/// `Error::RecoveryIncomplete` without an envelope or grant.
pub async fn open_epoch<H: Hold>(h: &H, channel: &str, epoch: u64) -> Result<()> {
    let Envelope {
        stored,
        activation,
        reference,
        before,
        mine,
    } = match h.with(|c| c.epoch_opening(channel, epoch)).await? {
        Opening::Held => return Ok(()),
        Opening::History => return Box::pin(open_history(h, channel, epoch)).await,
        Opening::Envelope(envelope) => *envelope,
    };
    let trust = h.trust().await?;
    let mut recipients = vec![];
    let mut accounts = BTreeSet::new();
    let mut cursor = 0;
    while accounts.len() < before.members.len() {
        let page = h
            .get(&format!(
                "{}/epochs/{epoch}/security?after={cursor}",
                base(channel)?
            ))
            .await?;
        let entries = page["entries"].as_array().ok_or(Error::IntegrityFailed)?;
        ensure(
            page["epoch"].as_u64() == Some(epoch)
                && page["head"] == stored.head.as_str()
                && page["recipients_digest"] == activation.recipients_digest.as_str()
                && page["count"].as_u64() == Some(before.members.len() as u64)
                && !entries.is_empty()
                && entries.len() <= 5,
            Error::IntegrityFailed,
        )?;
        for entry in entries {
            let account = entry["value"]["account"]
                .as_str()
                .ok_or(Error::IntegrityFailed)?
                .to_string();
            let at = entry["cursor"].as_u64().ok_or(Error::IntegrityFailed)?;
            ensure(
                before.members.contains_key(&account)
                    && !accounts.contains(&account)
                    && at > cursor,
                Error::IntegrityFailed,
            )?;
            let evidence: Evidence = from(&entry["value"]["evidence"])?;
            let id = AccountId::from_str(&account).map_err(|_| Error::IntegrityFailed)?;
            let security = trust.account(&evidence, &id, stored.stored_at, now_ms())?;
            recipients.extend(devices_of(&account, &security));
            accounts.insert(account);
            cursor = at;
        }
        ensure(
            page["next_cursor"].as_u64() == Some(cursor),
            Error::IntegrityFailed,
        )?;
    }
    recipients.sort();
    ensure(
        recipients_digest(&recipients) == activation.recipients_digest
            && recipients.len() == activation.envelopes.len()
            && recipients.iter().all(|r| {
                activation
                    .envelopes
                    .iter()
                    .any(|e| e.recipient == r.recipient)
            }),
        Error::IntegrityFailed,
    )?;
    let plan = descriptor(h, channel, &reference.upload_id, None).await?;
    let chunk = plan.chunks.get(reference.chunk as usize);
    ensure(
        plan.kind == "envelopes"
            && plan.epoch == epoch
            && plan.control_head.as_deref() == Some(&activation.expected_head)
            && chunk.is_some_and(|c| c.digest == reference.digest && c.size == reference.size),
        Error::IntegrityFailed,
    )?;
    let manifest: EpochManifest = decode(
        &channel_chunk(
            h,
            channel,
            &reference.upload_id,
            "manifest",
            &plan.manifest_digest,
        )
        .await?,
        65_536,
    )?;
    ensure(
        manifest.format == "dmsg-channel-epoch/1"
            && manifest.channel == channel
            && manifest.epoch == epoch
            && manifest.key_confirmation == activation.key_confirmation
            && manifest.recipients_digest == activation.recipients_digest,
        Error::IntegrityFailed,
    )?;
    let bytes = channel_chunk(
        h,
        channel,
        &reference.upload_id,
        &reference.chunk.to_string(),
        &reference.digest,
    )
    .await?;
    // The key goes into the synced record once the account's root is the
    // committed one.
    h.signed(|c| {
        c.root_now()?;
        let envelope: HpkeEnvelope = decode(&bytes, 4096)?;
        let key = hpke_open(
            &c.keys()?.hpke,
            &envelope,
            &envelope_context(channel, &activation, &mine),
        )?;
        ensure(
            key.len() == 32
                && key_confirmation(channel, epoch, &key) == activation.key_confirmation,
            Error::IntegrityFailed,
        )?;
        c.install_keys(channel, BTreeMap::from([(epoch, b64(&key))]))
    })
    .await?;
    Ok(())
}

/// Install epoch keys from the latest history grant to this account, or
/// from the invitation it joined with ([`Client::history_grant`]).
async fn open_history<H: Hold>(h: &H, channel: &str, epoch: u64) -> Result<()> {
    let (grant, me, mine, own) = h
        .with(|c| {
            let me = c.me()?;
            let mine = recipient(&me, &c.workspace.meta.device_id);
            Ok((c.history_grant(channel, epoch)?, me, mine, c.account()?.id))
        })
        .await?;
    let Grant {
        stored,
        control,
        from_epoch,
        to_epoch,
        upload: envelope_upload,
    } = grant;
    let first = descriptor(h, channel, &envelope_upload, Some(&stored.account)).await?;
    let manifest: HistoryManifest = decode(
        &channel_chunk(
            h,
            channel,
            &envelope_upload,
            "manifest",
            &first.manifest_digest,
        )
        .await?,
        65_536,
    )?;
    let scope = manifest.scope.clone();
    ensure(
        manifest.format == "dmsg-channel-history/1"
            && scope.channel == channel
            && scope.target == me
            && scope.from == from_epoch
            && scope.to == to_epoch
            && scope.head == control.expected_head
            && first.epoch == scope.epoch
            && first.control_head.as_deref() == Some(&scope.head)
            && first.kind == "envelopes"
            && manifest.envelopes.len() <= 192,
        Error::IntegrityFailed,
    )?;
    let acceptance = stored.acceptance.as_ref().ok_or(Error::IntegrityFailed)?;
    let trust = h.trust().await?;
    let security = trust.account(acceptance, &own, stored.stored_at, now_ms())?;
    let mut recipients = devices_of(&me, &security);
    recipients.sort();
    ensure(
        recipients_digest(&recipients) == scope.recipients_digest,
        Error::PolicyStale,
    )?;
    let references: Vec<&HistoryRef> = manifest
        .envelopes
        .iter()
        .filter(|r| r.recipient == mine)
        .collect();
    ensure(
        !references.is_empty() && references.len() <= 32,
        Error::RecoveryIncomplete,
    )?;
    let confirmations = h
        .with(|c| c.confirmations(channel, scope.from, scope.to))
        .await?;
    let mut keys = BTreeMap::new();
    for reference in references {
        let plan = descriptor(h, channel, &reference.upload_id, Some(&stored.account)).await?;
        let chunk = plan.chunks.get(reference.chunk as usize);
        ensure(
            plan.kind == "envelopes"
                && plan.epoch == scope.epoch
                && plan.control_head.as_deref() == Some(&scope.head)
                && plan.manifest_digest == first.manifest_digest
                && chunk.is_some_and(|c| c.digest == reference.digest && c.size == reference.size)
                && reference.from >= scope.from
                && reference.to <= scope.to
                && reference.to >= reference.from
                && reference.to - reference.from < 32,
            Error::IntegrityFailed,
        )?;
        let bytes = channel_chunk(
            h,
            channel,
            &reference.upload_id,
            &reference.chunk.to_string(),
            &reference.digest,
        )
        .await?;
        let opened: Vec<(u64, ByteBuf)> = h
            .with(|c| {
                let envelope: HpkeEnvelope = decode(&bytes, 4096)?;
                let plain = hpke_open(
                    &c.keys()?.hpke,
                    &envelope,
                    &history_context(&scope, &mine, reference.from, reference.to),
                )?;
                decode(&plain, 65_536)
            })
            .await?;
        ensure(
            opened.len() as u64 == reference.to - reference.from + 1,
            Error::IntegrityFailed,
        )?;
        for (index, (epoch, key)) in opened.iter().enumerate() {
            ensure(
                *epoch == reference.from + index as u64
                    && key.len() == 32
                    && confirmations.get(epoch) == Some(&key_confirmation(channel, *epoch, key))
                    && !keys.contains_key(epoch),
                Error::IntegrityFailed,
            )?;
            keys.insert(*epoch, b64(key));
        }
    }
    ensure(
        keys.len() as u64 == scope.to - scope.from + 1,
        Error::RecoveryIncomplete,
    )?;
    h.signed(|c| {
        c.root_now()?;
        c.install_keys(channel, keys.clone())
    })
    .await?;
    Ok(())
}

/// A channel command sent once: the journal keeps its request ID, a stored
/// outcome is read back by it, and only a command that can no longer
/// commit is signed again under the same ID. The stored outcome when the
/// relay has the command already, or the request to send.
pub(crate) async fn journaled<H: Hold>(
    h: &H,
    channel: &str,
    key: &str,
    base: &str,
    action: &str,
    payload: Json,
    suffix: &str,
) -> Result<Posting> {
    let path = format!("{base}{suffix}");
    let job: Option<Operation> = h
        .with(|c| {
            let job: Option<Operation> = c.job(channel, key)?;
            if let Some(job) = &job {
                ensure(
                    job.action == action && job.path == path && job.payload == payload,
                    Error::IdempotencyConflict,
                )?;
            }
            Ok(job)
        })
        .await?;
    if let Some(job) = &job {
        if let Some(result) = &job.result {
            return Ok(Posting::Done(result.clone()));
        }
        let found = h
            .find(&format!("{base}/operations/{}", job.request_id))
            .await?;
        if let Some(found) = found.filter(|f| f["found"] == true) {
            let result = found["result"].clone();
            h.with(|c| {
                let job = Operation {
                    result: Some(result.clone()),
                    ..job.clone()
                };
                c.save_job(channel, key, &job)
            })
            .await?;
            return Ok(Posting::Done(result));
        }
    }
    let (_, posting) = h
        .signed(|c| {
            let job = match &job {
                Some(job) if job.deadline > now_ms() + 5_000 => job.clone(),
                Some(job) if job.fixed => return Err(Error::Expired),
                previous => {
                    let request_id = previous
                        .as_ref()
                        .map_or_else(random_hex, |j| j.request_id.clone());
                    let job = c.sign_now(action, path.clone(), payload.clone(), request_id)?;
                    if action == GENESIS {
                        c.renew_genesis(channel, &job.cose)?;
                    }
                    c.save_job(channel, key, &job)?;
                    job
                }
            };
            let state = c.fresh()?;
            Ok(Posting::Send(c.cloud.prepare_signed(
                &c.signer(&state)?,
                &job.path,
                &job.cose,
                &job.request_id,
                job.deadline,
                now_ms(),
            )?))
        })
        .await?;
    Ok(posting)
}

/// Whether the relay stored the journaled command `key`.
async fn committed<H: Hold>(h: &H, channel: &str, key: &str) -> Result<bool> {
    let Some(job) = h.with(|c| c.job::<Operation>(channel, key)).await? else {
        return Ok(false);
    };
    if job.result.is_some() {
        return Ok(true);
    }
    let found = h
        .find(&format!("{}/operations/{}", base(channel)?, job.request_id))
        .await?;
    match found.filter(|f| f["found"] == true) {
        Some(found) => {
            let job = Operation {
                result: Some(found["result"].clone()),
                ..job
            };
            h.with(|c| c.save_job(channel, key, &job)).await?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Send `draft`, with the file prepared under the same message ID, under
/// the current epoch, after the control log is read. The message is
/// journaled before it is signed, so a retry with the same ID sends the
/// same message; one the channel moved past before the relay stored it is
/// sealed again under what holds now (C06). The new messages are read once
/// the relay stored it.
///
/// # Errors
/// Returns `Error::Forbidden` for a member who may not publish and
/// `Error::RekeyRequired` while the channel waits for a new epoch.
pub async fn send<H: Hold>(
    h: &H,
    channel: &str,
    draft: &Draft,
    message_id: Option<&str>,
) -> Result<Json> {
    let message_id = message_id.map_or_else(random_hex, str::to_string);
    ensure_valid(is_hash(&message_id), "message_id")?;
    ensure_valid(draft.text.len() <= MAX_TEXT, "text")?;
    let ledger = Box::pin(pull(h, channel, None, None)).await?;
    h.signed(Client::root_now).await?;
    let key_name = format!("message:{message_id}");
    let send = format!("send:{message_id}");
    let (mut outgoing, draft) = h
        .with(|c| {
            let outgoing = c.job::<Outgoing>(channel, &key_name)?;
            let mut draft = draft.clone();
            if let Some(prior) = &outgoing {
                // A retry resends the journaled message, with the Agent ID
                // that wrote it, which a caller leaves out.
                if draft.agent.is_none() {
                    draft.agent.clone_from(&prior.draft.agent);
                }
                ensure(prior.draft == draft, Error::IdempotencyConflict)?;
            }
            Ok((outgoing, draft))
        })
        .await?;
    if let Some(prior) = &outgoing {
        let current = prior.payload.epoch == ledger.epoch
            && prior.payload.control_head == ledger.head
            && prior.payload.acl_version == ledger.acl_version;
        if !current && !committed(h, channel, &send).await? {
            // Never stored, and the channel moved on: seal it again under
            // what holds now (C06).
            h.with(|c| {
                c.forget_job(channel, &send)?;
                c.forget_job(channel, &key_name)
            })
            .await?;
            outgoing = None;
        }
    }
    let outgoing = match outgoing {
        Some(outgoing) => outgoing,
        None => {
            if h.with(|c| c.seal_check(channel, &ledger, &draft, &message_id))
                .await?
            {
                Box::pin(open_epoch(h, channel, ledger.epoch)).await?;
            }
            h.with(|c| c.seal_message(channel, &ledger, &draft, &message_id))
                .await?
        }
    };
    let posting = journaled(
        h,
        channel,
        &send,
        &base(channel)?,
        MESSAGE,
        json(&outgoing.payload),
        "/messages",
    )
    .await?;
    let reply = match posting {
        Posting::Done(result) => Ok(result),
        Posting::Send(request) => h.with(|c| Ok(c.relay())).await?.send(request).await,
    };
    let receipt = h
        .with(|c| c.channel_send_settle(channel, &message_id, reply))
        .await?;
    // The relay stored it: a read that fails now never fails the send, and
    // the next read takes the message.
    if let Ok(mut reading) = h.with(|c| c.message_reading(channel)).await {
        let _ = Box::pin(read(h, &mut reading)).await;
    }
    Ok(json!({"message_id": message_id, "receipt": receipt}))
}

/// Share a library file's version under the current epoch as a shared
/// edition: the version re-encrypted under a fresh key with a fresh
/// identity ([`Client::share_edition`]), uploaded to the channel with a
/// manifest sealed under that key, which the message carries. The edition
/// shares neither its key nor a chunk byte with the library's own copy or
/// a backup, so no one can relate the two. A retry after the epoch changed
/// shares it again under the new one.
///
/// # Errors
/// Returns `Error::NotFound` for an unknown file or version and
/// `Error::QuotaExceeded` for a version over 100 MiB.
pub async fn send_file<H: Hold>(
    h: &H,
    channel: &str,
    file: &str,
    version: Option<&str>,
    draft: &Draft,
    message_id: Option<&str>,
) -> Result<Json> {
    let message_id = message_id.map_or_else(random_hex, str::to_string);
    ensure_valid(is_hash(&message_id), "message_id")?;
    let manifest = h.with(|c| Ok(c.library_manifest(file, version)?.2)).await?;
    let source = format!("{file}@{}", manifest.version);
    let ledger = Box::pin(pull(h, channel, None, None)).await?;
    h.signed(Client::root_now).await?;
    let key_name = format!("file:{message_id}");
    let mut attachment = h.with(|c| c.job::<Attachment>(channel, &key_name)).await?;
    if let Some(prior) = &attachment {
        ensure(prior.source == source, Error::IdempotencyConflict)?;
        if prior.reference.epoch != ledger.epoch
            && !committed(h, channel, &format!("send:{message_id}")).await?
        {
            h.with(|c| c.forget_job(channel, &key_name)).await?;
            attachment = None;
        }
    }
    let attachment = match attachment {
        Some(attachment) => attachment,
        None => {
            h.with(|c| {
                ensure(ledger.status == Status::Active, Error::RekeyRequired)?;
                let edition = c.share_edition(&manifest)?;
                let (reference, mut upload) =
                    c.prepare_upload(channel, &ledger, &edition, c.blobs()?)?;
                upload.plan.management = Some(c.seal_management(&upload.plan, &json!({
                    "name":manifest.name,"source_file":file,"source_version":manifest.version,"plain_bytes":manifest.size,
                    "intent":{"kind":"channel","message_id":message_id,"draft":draft,"reference":reference,"source":source}
                }))?);
                let attachment = Attachment {
                    source,
                    reference,
                    upload,
                };
                c.save_job(channel, &key_name, &attachment)?;
                Ok(attachment)
            })
            .await?
        }
    };
    send_upload(h, channel, &attachment.upload, Source::Library).await?;
    Box::pin(send(h, channel, draft, Some(&message_id))).await
}

/// Resume a committed attachment whose message publication was interrupted.
/// The reference and draft were encrypted to the uploader's account root.
pub async fn resume_file<H: Hold>(
    h: &H,
    channel: &str,
    upload_id: &str,
    intent: &Json,
) -> Result<Json> {
    let mut after = 0;
    loop {
        let page = h
            .get(&format!(
                "{}/storage?after={after}&limit=100",
                base(channel)?
            ))
            .await?;
        if let Some(item) = page["entries"]
            .as_array()
            .ok_or(Error::IntegrityFailed)?
            .iter()
            .find(|i| i["upload_id"] == upload_id)
        {
            if item["publication"].is_object() {
                return Ok(item["publication"].clone());
            }
            break;
        }
        if page["end"] == true {
            return Err(Error::NotFound);
        }
        let next = page["next_cursor"].as_u64().ok_or(Error::IntegrityFailed)?;
        ensure(next > after, Error::IntegrityFailed)?;
        after = next;
    }
    let reference: FileRef =
        serde_json::from_value(intent["reference"].clone()).map_err(|_| Error::IntegrityFailed)?;
    ensure(reference.upload_id == upload_id, Error::IntegrityFailed)?;
    let draft: Draft =
        serde_json::from_value(intent["draft"].clone()).map_err(|_| Error::IntegrityFailed)?;
    let message_id = intent["message_id"]
        .as_str()
        .ok_or(Error::IntegrityFailed)?;
    let plan = descriptor(h, channel, upload_id, None).await?;
    h.with(|c| {
        c.save_job(
            channel,
            &format!("file:{message_id}"),
            &Attachment {
                source: intent["source"]
                    .as_str()
                    .ok_or(Error::IntegrityFailed)?
                    .to_owned(),
                reference,
                upload: Upload {
                    plan,
                    manifest: ByteBuf::new(),
                    recipients: vec![],
                },
            },
        )
    })
    .await?;
    Box::pin(send(h, channel, &draft, Some(message_id))).await
}

/// Read new messages, or with `backfill` every readable one again. A
/// history grant to this account has its epochs read again on its own.
/// Messages of epochs this account may not read are skipped by the relay
/// without their ciphertext.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for pages that do not verify; the
/// cursor stays.
pub async fn sync<H: Hold>(h: &H, channel: &str, backfill: bool) -> Result<ChannelSync> {
    let mut reading = Reading::new(channel);
    if backfill {
        let through = h.with(|c| Ok(c.local(channel)?.message)).await?;
        reading.range = Some((0, through));
    }
    read(h, &mut reading).await?;
    sync_file_deletions(h, channel).await?;
    Ok(ChannelSync {
        through: h.with(|c| Ok(c.local(channel)?.message)).await?,
        received: reading.received,
    })
}

async fn sync_file_deletions<H: Hold>(h: &H, channel: &str) -> Result<()> {
    let key = format!("channel-deletions:{channel}");
    let mut after = h
        .with(|c| Ok(c.workspace.private::<u64>(c.keys()?, &key)?.unwrap_or(0)))
        .await?;
    loop {
        let page = h
            .get(&format!(
                "{}/storage/deletions?after={after}&limit=100",
                base(channel)?
            ))
            .await?;
        let next = page["next_cursor"].as_u64().ok_or(Error::IntegrityFailed)?;
        let entries = page["entries"].as_array().ok_or(Error::IntegrityFailed)?;
        h.with(|c| {
            for entry in entries {
                let upload = entry["upload_id"].as_str().ok_or(Error::IntegrityFailed)?;
                hex_id(upload)?;
                c.workspace.put_private(
                    c.keys()?,
                    &format!("channel-deleted:{channel}:{upload}"),
                    entry,
                )?;
            }
            c.workspace.put_private(c.keys()?, &key, &next)
        })
        .await?;
        if page["end"] == true {
            return Ok(());
        }
        ensure(next > after, Error::IntegrityFailed)?;
        after = next;
    }
}

/// Fetch the file `reference` names, uploaded by `uploader`: its plan,
/// sealed manifest and chunks, each checked; the chunks go to the store.
pub(crate) async fn fetch_file<H: Hold>(
    h: &H,
    channel: &str,
    reference: &FileRef,
    uploader: &str,
) -> Result<FileManifest> {
    let plan = descriptor(h, channel, &reference.upload_id, Some(uploader)).await?;
    ensure(of_file(&plan, reference), Error::IntegrityFailed)?;
    let sealed = channel_chunk(
        h,
        channel,
        &reference.upload_id,
        "manifest",
        &reference.manifest_digest,
    )
    .await?;
    let file = shared_manifest(channel, reference, &plan, &sealed)?;
    let objects = format!("{}/objects/{}/chunks", base(channel)?, reference.upload_id);
    let missing = h
        .with(|c| {
            let mut missing = vec![];
            for (index, chunk) in file.chunks.iter().enumerate() {
                if !c.workspace.store.has_chunk(&chunk.digest)? {
                    missing.push((format!("{objects}/{index}"), chunk.digest.clone()));
                }
            }
            Ok(missing)
        })
        .await?;
    crate::upload::download_chunks(h, Source::Store, &missing, |_| {}).await?;
    Ok(FileManifest {
        key: reference.key.clone(),
        ..file
    })
}

/// Verify and decrypt a shared file and write it to `out`.
///
/// # Errors
/// Returns `Error::NotFound` for a message without a file.
pub async fn download<H: Hold>(
    h: &H,
    channel: &str,
    seq: u64,
    out: &std::path::Path,
) -> Result<FileManifest> {
    Box::pin(pull(h, channel, None, None)).await?;
    let message = h.with(|c| c.message_at(channel, seq)).await?;
    let reference = message.file.clone().ok_or(Error::NotFound)?;
    let manifest = fetch_file(h, channel, &reference, &message.account).await?;
    h.with(|c| export_manifest(&c.workspace.store, &manifest, MAX_FILE, out))
        .await
}

/// Save a shared file into the library, in `folder`: its chunks are
/// fetched and checked, then the plaintext is imported like a file of this
/// device, so the library's copy has its own key and chunks. A file of the
/// same name in `folder` takes it as a new version.
///
/// # Errors
/// Returns `Error::NotFound` for a message without a file or an unknown
/// folder.
pub async fn save_shared<H: Hold>(
    h: &H,
    channel: &str,
    seq: u64,
    folder: &str,
    progress: &mut (dyn FnMut(u64, u64) + Send),
) -> Result<Imported> {
    h.with(|c| c.catalog()?.live_folder(folder)).await?;
    Box::pin(pull(h, channel, None, None)).await?;
    let message = h.with(|c| c.message_at(channel, seq)).await?;
    let reference = message.file.clone().ok_or(Error::NotFound)?;
    let manifest = fetch_file(h, channel, &reference, &message.account).await?;
    h.with(|c| {
        let mut plain = Zeroizing::new(Vec::with_capacity(manifest.size as usize));
        read_file(&c.workspace.store, &manifest, MAX_FILE, Some(&mut *plain))?;
        let name = crate::library::catalog::safe_name(&manifest.name);
        let into = c.catalog()?.find(folder, &name);
        c.import_stream(
            &mut std::io::Cursor::new(&plain[..]),
            manifest.size,
            &name,
            folder,
            into,
            Origin::SharedIn {
                channel: channel.into(),
                seq,
            },
            progress,
        )
    })
    .await
}

/// An image a message carries, as a data URL to show in place: the plan of
/// its upload, then its sealed manifest and the chunks not kept here.
///
/// # Errors
/// Returns `Error::NotFound` for a message without an image and
/// `Error::QuotaExceeded` for one over 4 MiB.
pub async fn preview_image<H: Hold>(h: &H, channel: &str, seq: u64) -> Result<Json> {
    let (reference, _) = h.with(|c| c.image(channel, seq)).await?;
    let value = h
        .get(&format!(
            "{}/objects/{}",
            base(channel)?,
            reference.upload_id
        ))
        .await?;
    let trust = h.trust().await?;
    let fetching = h
        .with(|c| c.channel_preview_plan(channel, seq, &value, &trust))
        .await?;
    let (relay, downloads) = h.signed(|c| c.channel_preview_downloads(&fetching)).await?;
    let relay = &relay;
    let parts = futures_util::future::try_join_all(
        downloads
            .into_iter()
            .map(|(request, digest)| async move { relay.send_chunk(request, &digest).await }),
    )
    .await?;
    h.with(|c| c.channel_preview_settle(&fetching, parts)).await
}

/// Open a message's body under its epoch's key. A body that does not open,
/// or an epoch whose key is not here, leaves the message unreadable instead
/// of stopping the channel: any member could otherwise stall everyone's
/// reading with one bad message.
fn open_message(
    channel: &str,
    ring: &Keyring,
    mut message: Message,
    payload: &MessagePayload,
) -> Message {
    let Some(root) = ring
        .keys
        .get(&payload.epoch)
        .and_then(|key| unb64(key).ok())
    else {
        message.unreadable = true;
        return message;
    };
    let key = message_key(
        &root,
        channel,
        payload.epoch,
        &payload.message_id,
        &message.device,
    );
    let body = open(
        &key,
        &payload.ciphertext,
        &message_aad(&message.account, &message.device, payload),
    )
    .ok()
    .and_then(|plain| decode::<Body>(&plain, MAX_RECORD).ok())
    .filter(|b| {
        b.text.len() <= MAX_TEXT
            && (b.edit_of.is_none() || b.retract.is_none())
            && (b.edit_of.is_none() && b.retract.is_none()
                || b.file.is_none() && b.packet.is_none())
    });
    match body {
        Some(body) => {
            message.text = body.text;
            message.file = body.file;
            message.reply_to = body.reply_to;
            message.edit_of = body.edit_of;
            message.retract = body.retract;
            message.agent = body.agent;
            message.packet = body.packet;
        }
        None => message.unreadable = true,
    }
    message
}

/// `/v1/channels/<id>`.
fn base(channel: &str) -> Result<String> {
    ensure_valid(is_hash(channel), "channel")?;
    Ok(format!("/v1/channels/{channel}"))
}

fn json<T: Serialize>(value: &T) -> Json {
    serde_json::to_value(value).expect("serializable")
}

fn from<T: DeserializeOwned>(value: &Json) -> Result<T> {
    serde_json::from_value(value.clone()).map_err(|_| Error::IntegrityFailed)
}

fn key32(bytes: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    Ok(Zeroizing::new(
        bytes.try_into().map_err(|_| Error::IntegrityFailed)?,
    ))
}

/// HPKE context of an epoch envelope.
pub fn envelope_context<'a>(
    channel: &'a str,
    activation: &'a Activation,
    recipient: &'a str,
) -> (
    &'static str,
    &'a str,
    u64,
    &'a str,
    u64,
    &'a str,
    &'a str,
    &'a str,
) {
    (
        "dmsg/channel-epoch-envelope/1",
        channel,
        activation.epoch,
        &activation.lease_id,
        activation.fencing,
        &activation.expected_head,
        &activation.recipients_digest,
        recipient,
    )
}

/// `dmsg/channel-epoch-confirmation/v1` of an epoch key, hex.
pub fn key_confirmation(channel: &str, epoch: u64, key: &[u8]) -> String {
    hex::encode(*digest(
        "dmsg/channel-epoch-confirmation/v1",
        &(channel, epoch, Bytes::new(key)),
    ))
}

/// The key of one message, from its epoch key.
pub fn message_key(
    epoch_key: &[u8],
    channel: &str,
    epoch: u64,
    message_id: &str,
    device: &str,
) -> Zeroizing<[u8; 32]> {
    derive(
        epoch_key,
        &(
            "dmsg/channel-message-key/1",
            channel,
            epoch,
            message_id,
            device,
        ),
    )
}

/// The message payload without its ciphertext: the AAD's last element.
#[derive(Serialize)]
struct MessageAad<'a> {
    channel_id: &'a str,
    epoch: u64,
    control_head: &'a str,
    acl_version: u64,
    message_id: &'a str,
    client_created_at: u64,
}

fn message_aad<'a>(
    account: &'a str,
    device: &'a str,
    payload: &'a MessagePayload,
) -> (&'static str, &'a str, &'a str, MessageAad<'a>) {
    (
        "dmsg/channel-message/1",
        account,
        device,
        MessageAad {
            channel_id: &payload.channel_id,
            epoch: payload.epoch,
            control_head: &payload.control_head,
            acl_version: payload.acl_version,
            message_id: &payload.message_id,
            client_created_at: payload.client_created_at,
        },
    )
}

/// HPKE context of a history envelope.
pub fn history_context<'a>(
    scope: &'a HistoryScope,
    recipient: &'a str,
    from: u64,
    to: u64,
) -> (&'static str, &'a HistoryScope, &'a str, u64, u64) {
    (
        "dmsg/channel-history-envelope/1",
        scope,
        recipient,
        from,
        to,
    )
}

fn manifest_context<'a>(
    channel: &'a str,
    epoch: u64,
    upload_id: &'a str,
    file_id: &'a str,
    version: &'a str,
) -> (&'static str, &'a str, u64, &'a str, &'a str, &'a str) {
    (
        "dmsg/channel-file-manifest/1",
        channel,
        epoch,
        upload_id,
        file_id,
        version,
    )
}

fn recipient(account: &str, device: &Hash) -> String {
    format!("device:{account}:{}", hex::encode(**device))
}

/// The active `ContentSign` devices of an account's security, as recipients.
fn devices_of(account: &str, security: &crate::account::Security) -> Vec<Recipient> {
    security
        .devices
        .iter()
        .filter(|(_, d)| {
            d.revoked_at.is_none() && d.input.capabilities.contains(&Capability::ContentSign)
        })
        .map(|(id, d)| Recipient {
            recipient: recipient(account, id),
            hpke_pub: hex::encode(*d.input.hpke_pub),
        })
        .collect()
}

impl Client {
    pub(crate) fn me(&self) -> Result<String> {
        Ok(self.account()?.id.to_string())
    }

    pub(crate) fn device_hex(&self) -> String {
        hex::encode(*self.workspace.meta.device_id)
    }

    /// The synced record of a channel, with every key a conflict copy adds.
    fn keyring(&self, channel: &str) -> Result<(String, Keyring)> {
        let (row, record) = self.current(channel)?;
        ensure(row.kind == CHANNEL && !record.tombstone, Error::NotFound)?;
        let open = |record: &crate::content::EncryptedObject| -> Result<Keyring> {
            let ring: Keyring =
                decode(&record.open(self.environment(), self.roots()?)?, MAX_RECORD)?;
            ensure(
                ring.format == FORMAT && ring.id == channel,
                Error::IntegrityFailed,
            )?;
            Ok(ring)
        };
        let mut ring = open(&record)?;
        for copy in self.workspace.store.conflicts(CHANNEL)? {
            if copy.item != channel {
                continue;
            }
            for (epoch, key) in open(&crate::content::record(&copy)?)?.keys {
                match ring.keys.get(&epoch) {
                    Some(known) => ensure(*known == key, Error::IntegrityFailed)?,
                    None => {
                        ring.keys.insert(epoch, key);
                    }
                }
            }
        }
        Ok((row.revision, ring))
    }

    fn save_keyring(&mut self, parent: Option<String>, ring: &Keyring) -> Result<()> {
        let payload = Zeroizing::new(canonical(ring));
        self.write(CHANNEL, ring.id.clone(), parent, false, &payload, None)
            .map(|_| ())
    }

    fn local(&self, channel: &str) -> Result<Local> {
        Ok(self
            .workspace
            .private(self.keys()?, &format!("{LOCAL}{channel}"))?
            .unwrap_or_default())
    }

    fn save_local(&self, channel: &str, local: &Local) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{LOCAL}{channel}"), local)
    }

    fn stored(&self, channel: &str, seq: u64) -> Result<Stored> {
        self.workspace
            .private(self.keys()?, &format!("{LOG}{channel}:{seq:020}"))?
            .ok_or(Error::RecoveryIncomplete)
    }

    /// The channel as it was at `head`.
    fn ledger_at(&self, channel: &str, local: &Local, head: &str) -> Result<Ledger> {
        let seq = local.heads.get(head).ok_or(Error::IntegrityFailed)?;
        Ok(self.stored(channel, *seq)?.ledger)
    }

    pub(crate) fn job<T: DeserializeOwned + Serialize>(
        &self,
        channel: &str,
        key: &str,
    ) -> Result<Option<T>> {
        self.workspace
            .private(self.keys()?, &format!("{JOB}{channel}:{key}"))
    }

    pub(crate) fn save_job<T: Serialize>(&self, channel: &str, key: &str, value: &T) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{JOB}{channel}:{key}"), value)
    }

    /// Delete a job whose outcome is settled.
    fn forget_job(&self, channel: &str, key: &str) -> Result<()> {
        self.workspace
            .delete_private(&format!("{JOB}{channel}:{key}"))
    }

    /// Keep the chunks of channel uploads until their plans lapse, whatever
    /// becomes of the jobs that planned them.
    pub(crate) fn hold_uploads<'a>(
        &self,
        plans: impl IntoIterator<Item = &'a UploadPlan>,
    ) -> Result<()> {
        let keys = self.keys()?;
        let mut held: BTreeMap<String, Held> =
            self.workspace.private(keys, UPLOADS)?.unwrap_or_default();
        for plan in plans {
            held.insert(
                plan.upload_id.clone(),
                Held {
                    expires_at: plan.expires_at,
                    chunks: plan.chunks.iter().map(|c| c.digest.clone()).collect(),
                },
            );
        }
        self.workspace.put_private(keys, UPLOADS, &held)
    }

    /// The chunks of the channel uploads whose plans have not lapsed at
    /// `now`, from the workspace's store or the library's blobs alike; the
    /// lapsed ones are forgotten.
    pub(crate) fn held_chunks(&self, now: u64) -> Result<BTreeSet<String>> {
        let keys = self.keys()?;
        let Some(mut held) = self
            .workspace
            .private::<BTreeMap<String, Held>>(keys, UPLOADS)?
        else {
            return Ok(BTreeSet::new());
        };
        let before = held.len();
        held.retain(|_, h| h.expires_at > now);
        if held.len() != before {
            self.workspace.put_private(keys, UPLOADS, &held)?;
        }
        Ok(held.into_values().flat_map(|h| h.chunks).collect())
    }

    /// The account may write content: its root is the committed one.
    pub(crate) async fn root_ready(&mut self) -> Result<()> {
        self.hold().signed(Client::root_now).await.map(|_| ())
    }

    /// [`Self::root_ready`] against the relay's evidence at hand;
    /// `Error::PolicyStale` while that is not fresh.
    pub(crate) fn root_now(&mut self) -> Result<()> {
        let generation = self.roots()?.generation;
        let state = self.fresh()?;
        ensure(
            state.snapshot.vault_write_state == dmsg_types::user::VaultWriteState::Ready
                && state.snapshot.content_root_generation == generation,
            Error::RekeyRequired,
        )
    }

    /// Sign `payload` as a new operation under `request_id`.
    async fn sign_operation(
        &mut self,
        action: &str,
        path: String,
        payload: Json,
        request_id: String,
    ) -> Result<Operation> {
        self.cloud_state().await?;
        self.sign_now(action, path, payload, request_id)
    }

    /// [`Self::sign_operation`] with the relay's evidence at hand;
    /// `Error::PolicyStale` while that is not fresh.
    fn sign_now(
        &self,
        action: &str,
        path: String,
        payload: Json,
        request_id: String,
    ) -> Result<Operation> {
        let state = self.fresh_state().ok_or(Error::PolicyStale)?;
        let signer = self.signer(state)?;
        let deadline = signer.deadline(now_ms())?;
        let cose = sign_command(&signer, action, &payload, &request_id, deadline);
        Ok(Operation {
            action: action.into(),
            path,
            payload,
            request_id,
            cose,
            deadline,
            fixed: false,
            result: None,
        })
    }

    /// Send a channel command once: the journal keeps its request ID, a
    /// stored outcome is read back by it, and only a command that can no
    /// longer commit is signed again under the same ID.
    async fn channel_post(
        &mut self,
        channel: &str,
        key: &str,
        action: &str,
        payload: Json,
        suffix: &str,
    ) -> Result<Json> {
        let base = base(channel)?;
        self.post_journaled(channel, key, &base, action, payload, suffix)
            .await
    }

    /// [`Self::channel_post`] to any relay resource under `base` that reads
    /// stored outcomes back at `{base}/operations/{request_id}`, journaled
    /// under the channel's `key`.
    pub(crate) async fn post_journaled(
        &mut self,
        channel: &str,
        key: &str,
        base: &str,
        action: &str,
        payload: Json,
        suffix: &str,
    ) -> Result<Json> {
        match self
            .journaled_request(channel, key, base, action, payload, suffix)
            .await?
        {
            Posting::Done(result) => Ok(result),
            Posting::Send(request) => {
                let reply = self.cloud.send(request).await;
                self.journaled_settle(channel, key, reply)
            }
        }
    }

    /// [`Self::post_journaled`] up to the request, as [`journaled`].
    pub(crate) async fn journaled_request(
        &mut self,
        channel: &str,
        key: &str,
        base: &str,
        action: &str,
        payload: Json,
        suffix: &str,
    ) -> Result<Posting> {
        journaled(&self.hold(), channel, key, base, action, payload, suffix).await
    }

    /// Record the reply to a request of [`journaled`].
    pub(crate) fn journaled_settle(
        &self,
        channel: &str,
        key: &str,
        reply: Result<Json>,
    ) -> Result<Json> {
        let result = reply?;
        let mut job: Operation = self.job(channel, key)?.ok_or(Error::IntegrityFailed)?;
        job.result = Some(result.clone());
        self.save_job(channel, key, &job)?;
        Ok(result)
    }

    /// A genesis signed again before the relay stored the first one.
    fn renew_genesis(&mut self, channel: &str, cose: &str) -> Result<()> {
        let (revision, mut ring) = self.keyring(channel)?;
        let genesis = hex_sha256(&unb64(cose)?);
        if ring.genesis != genesis {
            ensure(
                self.local(channel)?.ledger.is_none(),
                Error::IntegrityFailed,
            )?;
            ring.genesis = genesis;
            self.save_keyring(Some(revision), &ring)?;
        }
        Ok(())
    }

    /// Verify one control event against the channel before it.
    fn consume(
        &self,
        trust: &Trust,
        previous: Option<&Ledger>,
        event: &Event,
        genesis: &str,
    ) -> Result<Stored> {
        let cose = &event.signed.cose_sign1;
        ensure(
            hex_sha256(&unb64(cose)?) == event.head,
            Error::IntegrityFailed,
        )?;
        let now = now_ms();
        let verified: Verified<Json> =
            trust.command(cose, &event.evidence, event.stored_at, None, now)?;
        let account = verified.account.to_string();
        let body = &verified.body;
        let ledger = match previous {
            None => {
                ensure(
                    event.control_seq == 0 && event.head == genesis && body.action == GENESIS,
                    Error::IntegrityFailed,
                )?;
                start(&from::<Genesis>(&body.payload)?, &account, &event.head)?
            }
            Some(previous) => {
                ensure(
                    event.control_seq == previous.control_seq + 1,
                    Error::IntegrityFailed,
                )?;
                match body.action.as_str() {
                    CONTROL => {
                        let control: Control = from(&body.payload)?;
                        if let Action::Transfer {
                            account: target,
                            device,
                            signature,
                        } = &control.action
                        {
                            let acceptance = event
                                .acceptance_evidence
                                .as_ref()
                                .ok_or(Error::AuthRequired)?;
                            let target_id =
                                AccountId::from_str(target).map_err(|_| Error::IntegrityFailed)?;
                            let (_, target_device) = trust.device(
                                acceptance,
                                &target_id,
                                &hex_id(device)?,
                                event.stored_at,
                                now,
                            )?;
                            let intent = TransferIntent {
                                expected_head: control.expected_head.clone(),
                                expected_acl_version: control.expected_acl_version,
                                action: TransferAction {
                                    kind: "transfer".into(),
                                    account: target.clone(),
                                    device: device.clone(),
                                },
                            };
                            let context = Context {
                                account_id: account.clone(),
                                issuer: trust.issuer(&verified.account),
                                device_id: hex::encode(*verified.device),
                                security_epoch: body.security_epoch,
                                request_id: body.request_id.clone(),
                                deadline: body.deadline,
                            };
                            dmsg_protocol::verify(
                                &target_device.input.signing_pub,
                                digest(
                                    "dmsg/channel/owner-acceptance/v1",
                                    &unsigned(&context, CONTROL, &intent),
                                )
                                .as_slice(),
                                &unb64(signature)?,
                            )
                            .map_err(|_| Error::AuthRequired)?;
                        }
                        advance(previous, &control, &account, &event.head, event.stored_at)?
                    }
                    REACTIVATE => {
                        let input: Reactivate = from(&body.payload)?;
                        ensure(
                            previous.owner == account
                                && previous.status == Status::Archived
                                && input.expected_head == previous.head,
                            Error::IntegrityFailed,
                        )?;
                        Ledger {
                            status: Status::RotationRequired,
                            control_seq: event.control_seq,
                            acl_version: previous.acl_version + 1,
                            head: event.head.clone(),
                            ..previous.clone()
                        }
                    }
                    ACTIVATE => {
                        let activation: Activation = from(&body.payload)?;
                        activation.validate()?;
                        let unique: BTreeSet<&String> =
                            activation.envelopes.iter().map(|e| &e.recipient).collect();
                        ensure(
                            may_rotate(previous, &account)
                                && previous.status != Status::Archived
                                && activation.expected_head == previous.head
                                && activation.epoch == previous.epoch + 1
                                && unique.len() == activation.envelopes.len(),
                            Error::IntegrityFailed,
                        )?;
                        Ledger {
                            epoch: activation.epoch,
                            status: Status::Active,
                            head: event.head.clone(),
                            control_seq: event.control_seq,
                            manifest: None,
                            ..previous.clone()
                        }
                    }
                    _ => return Err(Error::UnsupportedProtocol),
                }
            }
        };
        Ok(Stored {
            seq: event.control_seq,
            head: event.head.clone(),
            stored_at: event.stored_at,
            account,
            device: hex::encode(*verified.device),
            action: body.action.clone(),
            payload: body.payload.clone(),
            acceptance: event.acceptance_evidence.clone(),
            ledger,
        })
    }

    /// Read the control log to the relay's head, verifying every new event,
    /// and check the relay's view against it. An invited account reads with
    /// its invitation; `pinned` must be in the verified history.
    pub(crate) async fn channel_pull(
        &mut self,
        channel: &str,
        invitation: Option<&str>,
        pinned: Option<&str>,
    ) -> Result<Ledger> {
        pull(&self.hold(), channel, invitation, pinned).await
    }

    /// The new messages, against the control log as last read.
    fn message_reading(&self, channel: &str) -> Result<Reading> {
        let local = self.local(channel)?;
        Ok(Reading {
            step: Step::Messages,
            messages: true,
            after: Some(local.message),
            ledger: Some(local.ledger.ok_or(Error::NotFound)?),
            ..Reading::controls(channel)
        })
    }

    /// The relay path of `reading`'s next page. The log and the head are
    /// asked for against the control cursor this device holds now, which
    /// `reading` notes.
    fn channel_read_path(&self, reading: &mut Reading) -> Result<String> {
        let base = base(&reading.channel)?;
        if matches!(reading.step, Step::Control | Step::Head) {
            reading.control = self.local(&reading.channel)?.control;
        }
        let invitation = reading.invitation.as_deref();
        Ok(match reading.step {
            Step::Control => format!(
                "{base}/sync?kind=control&after={}&limit=5{}",
                reading.control,
                invitation
                    .map(|i| format!("&invitation={i}"))
                    .unwrap_or_default()
            ),
            Step::Head => format!(
                "{base}{}",
                invitation
                    .map(|i| format!("?invitation={i}"))
                    .unwrap_or_default()
            ),
            Step::Messages => format!(
                "{base}/sync?after={}&limit=20",
                reading.after.unwrap_or_default()
            ),
            Step::Epochs | Step::Grants | Step::Done => return Err(Error::IntegrityFailed),
        })
    }

    /// The signed request of `reading`'s next page, to send without holding
    /// the client; [`Self::channel_read_settle`] takes the answer.
    ///
    /// # Errors
    /// Returns `Error::PolicyStale` until a [`Self::warmup`] ran.
    pub(crate) fn channel_read_request(&self, reading: &mut Reading) -> Result<Prepared> {
        let path = self.channel_read_path(reading)?;
        self.signed_get(&path)
    }

    /// Apply the page `reading` asked for and move it on; `trust` checks
    /// the evidence the relay stored with each entry.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for a page that does not continue
    /// what this device verified.
    pub(crate) fn channel_read_settle(
        &mut self,
        reading: &mut Reading,
        page: Json,
        trust: &Trust,
    ) -> Result<()> {
        match reading.step {
            Step::Control => self.control_page(reading, &page, trust),
            Step::Head => self.head_page(reading, &page),
            Step::Messages => self.messages_page(reading, &page, trust),
            Step::Epochs | Step::Grants | Step::Done => Err(Error::IntegrityFailed),
        }
    }

    /// Verify and keep a page of the control log.
    fn control_page(&mut self, reading: &mut Reading, page: &Json, trust: &Trust) -> Result<()> {
        let channel = reading.channel.clone();
        let mut local = self.local(&channel)?;
        // Another read of the channel kept this page, or more, while it was
        // on its way: ask again from there. A page asked for against the
        // cursor held now must continue it.
        if local.control > reading.control {
            return Ok(());
        }
        let (_, ring) = self.keyring(&channel)?;
        let me = self.me()?;
        let entries = page["entries"].as_array().ok_or(Error::IntegrityFailed)?;
        let next = page["next_cursor"].as_u64().ok_or(Error::IntegrityFailed)?;
        ensure(
            entries.len() <= 5 && next >= local.control,
            Error::IntegrityFailed,
        )?;
        let mut verified = Vec::with_capacity(entries.len());
        for row in entries {
            let cursor = row["cursor"].as_u64().ok_or(Error::IntegrityFailed)?;
            ensure(cursor > local.control, Error::IntegrityFailed)?;
            let event: Event = from(&row["value"])?;
            let stored = self.consume(trust, local.ledger.as_ref(), &event, &ring.genesis)?;
            local.heads.insert(stored.head.clone(), stored.seq);
            match &stored.action[..] {
                ACTIVATE => {
                    local.epochs.insert(stored.ledger.epoch, stored.seq);
                }
                // History grants and sealed invitations to this account
                // carry epoch keys for its devices; a grant's messages are
                // read again.
                CONTROL => match from::<Control>(&stored.payload).map(|c| c.action) {
                    Ok(Action::History {
                        account,
                        from_epoch,
                        to_epoch,
                        ..
                    }) if account == me => {
                        local.histories.push(stored.seq);
                        local.granted.push((from_epoch, to_epoch));
                    }
                    Ok(Action::Invite {
                        account,
                        envelope_upload: Some(_),
                        ..
                    }) if account == me => local.histories.push(stored.seq),
                    _ => {}
                },
                _ => {}
            }
            local.ledger = Some(stored.ledger.clone());
            local.control = cursor;
            verified.push(stored);
        }
        let ledger = local.ledger.clone().ok_or(Error::IntegrityFailed)?;
        ensure(
            next == local.control && ledger.channel_id == channel,
            Error::IntegrityFailed,
        )?;
        // The page's controls and the cursor past them in one write.
        let keys = self.keys()?;
        self.workspace.store.atomic(|_| {
            for stored in &verified {
                let name = format!("{LOG}{channel}:{:020}", stored.seq);
                self.workspace.put_private(keys, &name, stored)?;
            }
            self.save_local(&channel, &local)
        })?;
        if page["head"] == ledger.head.as_str() {
            if let Some(pinned) = &reading.pinned {
                ensure(local.heads.contains_key(pinned), Error::IntegrityFailed)?;
            }
            reading.step = Step::Head;
        } else {
            // A page without entries below the head is a gap.
            ensure(!entries.is_empty(), Error::IntegrityFailed)?;
        }
        Ok(())
    }

    /// Check the relay's view of the channel against the verified log, and
    /// keep its billing.
    fn head_page(&mut self, reading: &mut Reading, remote: &Json) -> Result<()> {
        let channel = reading.channel.clone();
        let mut local = self.local(&channel)?;
        // Another read kept controls the relay's view may predate: read the
        // log again, then the head.
        if local.control > reading.control {
            reading.step = Step::Control;
            return Ok(());
        }
        let mut ledger = local.ledger.clone().ok_or(Error::IntegrityFailed)?;
        ensure(
            remote["head"] == ledger.head.as_str()
                && remote["control_seq"].as_u64() == Some(ledger.control_seq)
                && remote["epoch"].as_u64() == Some(ledger.epoch)
                && remote["acl_version"].as_u64() == Some(ledger.acl_version)
                && reading
                    .stale
                    .take()
                    .is_none_or(|stale| stale != ledger.head),
            Error::IntegrityFailed,
        )?;
        let status: Status = from(&remote["status"])?;
        // The relay asks for a new epoch when a member's device goes.
        if status == Status::RotationRequired && ledger.status == Status::Active {
            ledger.status = status;
        } else {
            ensure(status == ledger.status, Error::IntegrityFailed)?;
        }
        if let Some(members) = remote.get("members") {
            let members: BTreeMap<String, crate::channel_log::Member> = from(members)?;
            ensure(
                members == ledger.members && remote["owner"] == ledger.owner.as_str(),
                Error::IntegrityFailed,
            )?;
        }
        if let Some(billing) = remote.get("billing").filter(|b| b.is_object()) {
            let mut billing = billing.clone();
            billing["transition"] = remote["billing_transition"].clone();
            local.billing = Some(billing);
        }
        local.ledger = Some(ledger.clone());
        self.save_local(&channel, &local)?;
        reading.ledger = Some(ledger);
        reading.step = if reading.messages {
            reading
                .after
                .get_or_insert(reading.range.map_or(local.message, |(after, _)| after));
            Step::Messages
        } else {
            Step::Done
        };
        Ok(())
    }

    /// Create a channel; this account owns it and its first epoch follows.
    ///
    /// # Errors
    /// Returns `Error::RekeyRequired` while the account waits for a new root.
    pub async fn channel_create(&mut self, name: &str, kind: ChannelType) -> Result<String> {
        ensure_valid(
            !name.is_empty()
                && name.chars().count() <= MAX_NAME
                && !name.chars().any(char::is_control),
            "name",
        )?;
        self.root_ready().await?;
        let channel = random_hex();
        let genesis = Genesis {
            channel_id: channel.clone(),
            nonce: random_hex(),
            kind,
        };
        let job = self
            .sign_operation(GENESIS, base(&channel)?, json(&genesis), random_hex())
            .await?;
        self.save_keyring(
            None,
            &Keyring {
                format: FORMAT.into(),
                id: channel.clone(),
                name: name.into(),
                genesis: hex_sha256(&unb64(&job.cose)?),
                keys: BTreeMap::new(),
                muted: false,
                hidden: false,
            },
        )?;
        self.save_job(&channel, "genesis", &job)?;
        self.channel_post(&channel, "genesis", GENESIS, job.payload, "")
            .await?;
        self.channel_pull(&channel, None, None).await?;
        self.channel_rotate(&channel).await?;
        Ok(channel)
    }

    /// The channels this account knows.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn channel_list(&self) -> Result<Vec<ChannelView>> {
        let mut channels = Vec::new();
        for (id, _) in self.workspace.store.heads(CHANNEL)? {
            if !self.current(&id)?.1.tombstone {
                channels.push(self.channel_view(&id)?);
            }
        }
        Ok(channels)
    }

    fn deletable_legacy_draft(&self, id: &str, ring: &Keyring, local: &Local) -> Result<bool> {
        Ok(ring.keys.is_empty()
            && local.ledger.is_none()
            && local.control == 0
            && local.message == 0
            && self.job::<cbor2::Value>(id, "legacy-draft")?.is_some()
            && self.workspace.store.sealed_keys(&format!("{JOB}{id}:"))?
                == vec![format!("{JOB}{id}:legacy-draft")])
    }

    /// Remove a leftover legacy inheritance draft and sync its deletion.
    /// Drafts with an activation in flight or any channel history are refused.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` unless the record is an unactivated draft.
    pub fn channel_delete_legacy_draft(&mut self, id: &str) -> Result<()> {
        let (parent, ring) = self.keyring(id)?;
        ensure(
            self.deletable_legacy_draft(id, &ring, &self.local(id)?)?,
            Error::Forbidden,
        )?;
        self.write(
            CHANNEL,
            id.into(),
            Some(parent),
            true,
            &canonical(&ring),
            None,
        )?;
        self.forget_job(id, "legacy-draft")?;
        self.workspace.delete_private(&format!("{LOCAL}{id}"))
    }

    /// Unread messages of the channels that are neither muted nor hidden.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn channels_unread(&self) -> Result<u64> {
        Ok(self
            .channel_list()?
            .iter()
            .filter(|c| !c.muted && !c.hidden)
            .map(|c| c.unread)
            .sum())
    }

    /// One channel as this device knows it.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown channel.
    pub fn channel_view(&self, channel: &str) -> Result<ChannelView> {
        let (_, ring) = self.keyring(channel)?;
        let local = self.local(channel)?;
        let deletable_legacy_draft = self.deletable_legacy_draft(channel, &ring, &local)?;
        let invites = local
            .invites
            .into_iter()
            .filter(|(id, invite)| {
                invite.expires_at > now_ms()
                    && local
                        .ledger
                        .as_ref()
                        .is_some_and(|l| l.invitations.contains_key(id))
            })
            .collect();
        Ok(ChannelView {
            id: ring.id,
            name: ring.name,
            genesis: ring.genesis,
            ledger: local.ledger,
            message_cursor: local.message,
            readable_epochs: ring.keys.keys().copied().collect(),
            muted: ring.muted,
            hidden: ring.hidden,
            deletable_legacy_draft,
            read: local.read,
            unread: local.unread,
            last: local.last,
            draft: local.draft,
            billing: local.billing,
            invites,
        })
    }

    /// Accept an invitation: trust its genesis, check the head it names is
    /// in the verified log, then accept.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for an invitation to another account or
    /// one that expired.
    pub async fn channel_join(&mut self, invitation: &Invitation) -> Result<Ledger> {
        let me = self.me()?;
        ensure(
            invitation.format == "dmsg-channel-invitation/1"
                && invitation.account == me
                && invitation.expires_at > now_ms()
                && is_hash(&invitation.invitation)
                && is_hash(&invitation.genesis),
            Error::Forbidden,
        )?;
        self.root_ready().await?;
        match self.keyring(&invitation.channel) {
            Ok((_, ring)) => ensure(ring.genesis == invitation.genesis, Error::IntegrityFailed)?,
            Err(Error::NotFound) => self.save_keyring(
                None,
                &Keyring {
                    format: FORMAT.into(),
                    id: invitation.channel.clone(),
                    name: invitation.name.chars().take(MAX_NAME).collect(),
                    genesis: invitation.genesis.clone(),
                    keys: BTreeMap::new(),
                    muted: false,
                    hidden: false,
                },
            )?,
            Err(error) => return Err(error),
        }
        let ledger = self
            .channel_pull(
                &invitation.channel,
                Some(&invitation.invitation),
                Some(&invitation.head),
            )
            .await?;
        if !ledger.members.contains_key(&me) {
            ensure(
                ledger
                    .invitations
                    .get(&invitation.invitation)
                    .is_some_and(|i| i.account == me),
                Error::Forbidden,
            )?;
            let control = Control {
                expected_head: ledger.head.clone(),
                expected_acl_version: ledger.acl_version,
                action: Action::Accept {
                    invitation_id: invitation.invitation.clone(),
                },
            };
            self.channel_post(
                &invitation.channel,
                &format!("accept:{}", invitation.invitation),
                CONTROL,
                json(&control),
                "/control",
            )
            .await?;
        }
        let ledger = self.channel_pull(&invitation.channel, None, None).await?;
        // A sealed invitation joined the current epoch: open its key now.
        if ledger.epoch > 0
            && ledger
                .members
                .get(&me)
                .is_some_and(|m| m.joined_epoch == ledger.epoch)
        {
            self.channel_open_epoch(&invitation.channel, ledger.epoch)
                .await?;
        }
        Ok(ledger)
    }

    /// Apply a control action, checked locally against the current channel
    /// first. `operation` names it for a retry.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` when the rules refuse it.
    pub async fn channel_control(
        &mut self,
        channel: &str,
        action: Action,
        operation: &str,
    ) -> Result<Ledger> {
        ensure_valid(!matches!(action, Action::Leave {}), "action")?;
        self.control_post(channel, action, operation).await?;
        self.channel_pull(channel, None, None).await
    }

    /// Leave a channel. The relay serves nothing more to a member that left,
    /// so this device keeps the last state it verified.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for the owner, who transfers or archives.
    pub async fn channel_leave(&mut self, channel: &str, operation: &str) -> Result<()> {
        self.control_post(channel, Action::Leave {}, operation)
            .await
    }

    async fn control_post(&mut self, channel: &str, action: Action, operation: &str) -> Result<()> {
        ensure_valid(
            !matches!(
                action,
                Action::Accept { .. } | Action::Transfer { .. } | Action::History { .. }
            ),
            "action",
        )?;
        let key = format!("control:{operation}");
        let payload = match self.job::<Operation>(channel, &key)? {
            Some(job) => job.payload,
            None => {
                let ledger = self.channel_pull(channel, None, None).await?;
                let control = Control {
                    expected_head: ledger.head.clone(),
                    expected_acl_version: ledger.acl_version,
                    action,
                };
                advance(&ledger, &control, &self.me()?, &"0".repeat(64), now_ms())?;
                json(&control)
            }
        };
        self.channel_post(channel, &key, CONTROL, payload, "/control")
            .await
            .map(|_| ())
    }

    /// Invite `account`; returns the invitation to hand to it. The epoch's
    /// key is sealed to the account's devices and goes with the invitation,
    /// so the member reads that epoch from its start, what was written
    /// before it joined included; a manager who wants otherwise rekeys
    /// first. An invitation is accepted only in the epoch it was issued in,
    /// so a channel waiting for keys rotates first; when this account may
    /// not rotate it, the invitation carries no key and lapses with that
    /// rotation.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` unless this account manages the channel.
    pub async fn channel_invite(
        &mut self,
        channel: &str,
        account: &AccountId,
        role: Role,
    ) -> Result<Invitation> {
        let invitation = random_hex();
        let target = account.to_string();
        let me = self.me()?;
        let mut ledger = self.channel_pull(channel, None, None).await?;
        if ledger.status == Status::RotationRequired && may_rotate(&ledger, &me) {
            ledger = self
                .channel_rotate(channel)
                .await?
                .ledger
                .ok_or(Error::IntegrityFailed)?;
        }
        self.root_ready().await?;
        let now = now_ms();
        let expires_at = now + INVITATION_MS;
        let mut control = Control {
            expected_head: ledger.head.clone(),
            expected_acl_version: ledger.acl_version,
            action: Action::Invite {
                invitation_id: invitation.clone(),
                account: target.clone(),
                role,
                expires_at,
                envelope_upload: None,
            },
        };
        advance(&ledger, &control, &me, &"0".repeat(64), now)?;
        if ledger.status == Status::Active {
            self.channel_open_epoch(channel, ledger.epoch).await?;
            let current = self.recipients(std::slice::from_ref(&target)).await?;
            let uploads = self.history_prepare(
                channel,
                &ledger,
                &target,
                ledger.epoch,
                ledger.epoch,
                &current.recipients,
                &invitation,
            )?;
            for upload in &uploads {
                self.channel_upload(channel, upload, Source::Store).await?;
            }
            if let Action::Invite {
                envelope_upload, ..
            } = &mut control.action
            {
                *envelope_upload = Some(uploads[0].plan.upload_id.clone());
            }
        }
        self.channel_post(
            channel,
            &format!("control:{invitation}"),
            CONTROL,
            json(&control),
            "/control",
        )
        .await?;
        self.forget_job(channel, &format!("history-plan:{invitation}"))?;
        let ledger = self.channel_pull(channel, None, None).await?;
        let (_, ring) = self.keyring(channel)?;
        let packet = Invitation {
            format: "dmsg-channel-invitation/1".into(),
            channel: channel.into(),
            genesis: ring.genesis,
            head: ledger.head,
            invitation: invitation.clone(),
            account: account.to_string(),
            name: ring.name,
            expires_at,
        };
        // Kept to hand over again while it is open.
        let now = now_ms();
        let mut local = self.local(channel)?;
        local
            .invites
            .retain(|id, i| i.expires_at > now && ledger.invitations.contains_key(id));
        local.invites.insert(invitation, packet.clone());
        self.save_local(channel, &local)?;
        Ok(packet)
    }

    /// The recipients of an epoch for `accounts`, from their certified
    /// security; the relay is handed the same evidence.
    async fn recipients(&mut self, accounts: &[String]) -> Result<Current> {
        let mut current = Current {
            recipients: vec![],
            versions: BTreeMap::new(),
            expires_at: u64::MAX,
        };
        let ids = accounts
            .iter()
            .map(|a| AccountId::from_str(a).map_err(|_| Error::IntegrityFailed))
            .collect::<Result<Vec<_>>>()?;
        for (id, security) in self.peers_security(&ids).await? {
            let account = id.to_string();
            current.expires_at = current.expires_at.min(security.expires_at());
            current.recipients.extend(devices_of(&account, &security));
            current
                .versions
                .insert(account, security.snapshot.security_epoch);
        }
        current.recipients.sort();
        Ok(current)
    }

    /// Start a new epoch now and seal it to the members' devices: who joins
    /// later reads nothing before it. A channel already waiting for one
    /// only rotates.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` unless this account may rotate.
    pub async fn channel_rekey(&mut self, channel: &str) -> Result<ChannelView> {
        let ledger = self.channel_pull(channel, None, None).await?;
        if ledger.status == Status::Active {
            self.control_post(channel, Action::Rekey {}, &format!("rekey:{}", ledger.head))
                .await?;
        }
        self.channel_rotate(channel).await
    }

    /// Bring the channel to an active epoch this device can read: rotate
    /// when a membership change requires it, under a relay lease, sealing a
    /// new key to every member device. A retry continues the same lease.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a device that may not coordinate and
    /// `Error::PolicyStale` when the members' devices changed meanwhile.
    pub async fn channel_rotate(&mut self, channel: &str) -> Result<ChannelView> {
        let mut ledger = self.channel_pull(channel, None, None).await?;
        self.root_ready().await?;
        let members: Vec<String> = ledger.members.keys().cloned().collect();
        let mut current = self.recipients(&members).await?;
        let observed = ledger.head.clone();
        ledger = self.channel_pull(channel, None, None).await?;
        if ledger.head != observed {
            let members: Vec<String> = ledger.members.keys().cloned().collect();
            current = self.recipients(&members).await?;
        }
        if ledger.status == Status::Active {
            self.forget_rotation(channel)?;
            return self.channel_open_epoch(channel, ledger.epoch).await;
        }
        ensure(ledger.status == Status::RotationRequired, Error::Forbidden)?;
        let now = now_ms();
        let mut pending = match self.job::<Rotation>(channel, "rotation")? {
            Some(p)
                if p.head == ledger.head && p.lease.as_ref().is_none_or(|l| l.expires_at > now) =>
            {
                p
            }
            _ => Rotation {
                head: ledger.head.clone(),
                lease_id: random_hex(),
                lease: None,
            },
        };
        self.save_job(channel, "rotation", &pending)?;
        let result = self
            .channel_post(
                channel,
                &format!("lease:{}", pending.lease_id),
                "dmsg/channel/lease/v1",
                json!({"expected_head": ledger.head, "lease_id": pending.lease_id}),
                "/rotation/lease",
            )
            .await?;
        let lease: Lease = from(&result["lease"])?;
        let recipients: Vec<Recipient> = from(&result["recipients"])?;
        ensure(
            recipients == current.recipients
                && recipients_digest(&current.recipients) == lease.recipients_digest
                && current.versions == lease.security_versions
                && lease.expires_at <= current.expires_at
                && lease.expires_at > now_ms(),
            Error::PolicyStale,
        )?;
        pending.lease = Some(lease.clone());
        self.save_job(channel, "rotation", &pending)?;
        let (activation, uploads) = self.rotation_prepare(channel, &ledger, &lease, &recipients)?;
        for upload in &uploads {
            self.channel_upload(channel, upload, Source::Store).await?;
        }
        self.channel_post(
            channel,
            &format!("activate:{}", lease.id),
            ACTIVATE,
            json(&activation),
            "/rotation/activate",
        )
        .await?;
        self.channel_pull(channel, None, None).await?;
        self.forget_rotation(channel)?;
        self.channel_open_epoch(channel, activation.epoch).await
    }

    /// Forget the journals of a rotation that ended, by this device or by
    /// another one: nothing of it is sent again, and the epoch key it
    /// sealed is in the keyring once opened.
    fn forget_rotation(&self, channel: &str) -> Result<()> {
        let Some(rotation) = self.job::<Rotation>(channel, "rotation")? else {
            return Ok(());
        };
        self.forget_job(channel, &format!("lease:{}", rotation.lease_id))?;
        if let Some(lease) = &rotation.lease {
            self.forget_job(channel, &format!("rotation-key:{}", lease.id))?;
            self.forget_job(channel, &format!("activate:{}", lease.id))?;
        }
        self.forget_job(channel, "rotation")
    }

    /// Seal the lease's key to every recipient, in groups of 120 per
    /// upload. The key and the groups are kept, so a retry uploads the same.
    fn rotation_prepare(
        &self,
        channel: &str,
        ledger: &Ledger,
        lease: &Lease,
        recipients: &[Recipient],
    ) -> Result<(Activation, Vec<Upload>)> {
        let unique: BTreeSet<&String> = recipients.iter().map(|r| &r.recipient).collect();
        ensure(
            ledger.head == lease.head
                && ledger.epoch + 1 == lease.epoch
                && ledger.status != Status::Archived
                && lease.account == self.me()?
                && lease.device == self.device_hex()
                && lease.expires_at > now_ms()
                && !recipients.is_empty()
                && recipients.len() <= crate::channel_log::MAX_RECIPIENTS
                && unique.len() == recipients.len()
                && recipients_digest(recipients) == lease.recipients_digest,
            Error::RekeyRequired,
        )?;
        let key_name = format!("rotation-key:{}", lease.id);
        let candidate = match self.job::<Candidate>(channel, &key_name)? {
            Some(candidate) => {
                ensure(candidate.lease == *lease, Error::IdempotencyConflict)?;
                candidate
            }
            None => {
                let key = Zeroizing::new(random_key());
                let mut activation = Activation {
                    lease_id: lease.id.clone(),
                    fencing: lease.fencing,
                    expected_head: lease.head.clone(),
                    epoch: lease.epoch,
                    recipients_digest: lease.recipients_digest.clone(),
                    manifest_upload: String::new(),
                    key_confirmation: key_confirmation(channel, lease.epoch, key.as_slice()),
                    envelopes: vec![],
                };
                let mut groups = vec![];
                for (index, selected) in recipients.chunks(GROUP).enumerate() {
                    let mut chunks = vec![];
                    for r in selected {
                        let public =
                            hex::decode(&r.hpke_pub).map_err(|_| Error::IntegrityFailed)?;
                        ensure(
                            r.recipient.starts_with("device:") && public.len() == 32,
                            Error::IntegrityFailed,
                        )?;
                        let public: [u8; 32] = public.try_into().expect("32 bytes");
                        let envelope = canonical(&hpke_seal(
                            &public,
                            key.as_slice(),
                            &envelope_context(channel, &activation, &r.recipient),
                        )?);
                        let digest = hex_sha256(&envelope);
                        self.workspace.store.put_chunk(&digest, &envelope)?;
                        chunks.push(ChunkRef {
                            digest,
                            size: envelope.len() as u64,
                        });
                    }
                    let manifest = canonical(&EpochManifest {
                        format: "dmsg-channel-epoch/1".into(),
                        channel: channel.into(),
                        epoch: lease.epoch,
                        lease_id: lease.id.clone(),
                        head: lease.head.clone(),
                        fencing: lease.fencing,
                        recipients_digest: lease.recipients_digest.clone(),
                        key_confirmation: activation.key_confirmation.clone(),
                        group: (index * GROUP) as u64,
                    });
                    groups.push(Upload {
                        plan: UploadPlan {
                            management: None,
                            upload_id: random_hex(),
                            object_id: channel.into(),
                            version_id: random_hex(),
                            root_generation: 0,
                            epoch: lease.epoch,
                            control_head: Some(lease.head.clone()),
                            kind: "envelopes".into(),
                            chunks,
                            manifest_digest: hex_sha256(&manifest),
                            manifest_size: manifest.len() as u64,
                            expires_at: now_ms() + PLAN_MS,
                        },
                        manifest: ByteBuf::from(manifest),
                        recipients: selected.iter().map(|r| r.recipient.clone()).collect(),
                    });
                }
                activation.manifest_upload = groups[0].plan.upload_id.clone();
                let candidate = Candidate {
                    lease: lease.clone(),
                    key: ByteBuf::from(key.to_vec()),
                    groups,
                };
                self.hold_uploads(candidate.groups.iter().map(|g| &g.plan))?;
                self.save_job(channel, &key_name, &candidate)?;
                candidate
            }
        };
        let mut activation = Activation {
            lease_id: lease.id.clone(),
            fencing: lease.fencing,
            expected_head: lease.head.clone(),
            epoch: lease.epoch,
            recipients_digest: lease.recipients_digest.clone(),
            manifest_upload: candidate.groups[0].plan.upload_id.clone(),
            key_confirmation: key_confirmation(channel, lease.epoch, &candidate.key),
            envelopes: vec![],
        };
        for group in &candidate.groups {
            for (chunk, recipient) in group.recipients.iter().enumerate() {
                let reference = &group.plan.chunks[chunk];
                activation.envelopes.push(crate::channel_log::EnvelopeRef {
                    recipient: recipient.clone(),
                    digest: reference.digest.clone(),
                    size: reference.size,
                    upload_id: group.plan.upload_id.clone(),
                    chunk: chunk as u64,
                });
            }
        }
        activation.validate()?;
        Ok((activation, candidate.groups))
    }

    /// Upload a channel upload whose chunks are in `source`.
    pub(crate) async fn channel_upload(
        &mut self,
        channel: &str,
        upload: &Upload,
        source: Source,
    ) -> Result<()> {
        send_upload(&self.hold(), channel, upload, source).await
    }

    /// Install an epoch's key, as [`open_epoch`] does.
    ///
    /// # Errors
    /// As [`open_epoch`].
    pub async fn channel_open_epoch(&mut self, channel: &str, epoch: u64) -> Result<ChannelView> {
        open_epoch(&self.hold(), channel, epoch).await?;
        self.channel_view(channel)
    }

    /// What opening `epoch`'s key takes.
    fn epoch_opening(&self, channel: &str, epoch: u64) -> Result<Opening> {
        let local = self.local(channel)?;
        let ledger = local.ledger.clone().ok_or(Error::NotFound)?;
        let me = self.me()?;
        ensure(readable(&ledger, &me, epoch), Error::Forbidden)?;
        let (_, ring) = self.keyring(channel)?;
        if ring.keys.contains_key(&epoch) {
            return Ok(Opening::Held);
        }
        let seq = local.epochs.get(&epoch).ok_or(Error::NotFound)?;
        let stored = self.stored(channel, *seq)?;
        let activation: Activation = from(&stored.payload)?;
        let mine = recipient(&me, &self.workspace.meta.device_id);
        let Some(reference) = activation
            .envelopes
            .iter()
            .find(|e| e.recipient == mine)
            .cloned()
        else {
            return Ok(Opening::History);
        };
        let before = self.ledger_at(channel, &local, &activation.expected_head)?;
        Ok(Opening::Envelope(Box::new(Envelope {
            stored,
            activation,
            reference,
            before,
            mine,
        })))
    }

    /// Add epoch keys to the synced record; a key already held must match.
    fn install_keys(&mut self, channel: &str, keys: BTreeMap<u64, String>) -> Result<()> {
        let (revision, mut ring) = self.keyring(channel)?;
        let mut changed = false;
        for (epoch, key) in keys {
            match ring.keys.get(&epoch) {
                Some(known) => ensure(*known == key, Error::IdempotencyConflict)?,
                None => {
                    ring.keys.insert(epoch, key);
                    changed = true;
                }
            }
        }
        if changed {
            self.save_keyring(Some(revision), &ring)?;
        }
        Ok(())
    }

    /// The latest history grant to this account that covers `epoch`, or
    /// the invitation it joined with, which seals the epoch it was issued
    /// in.
    fn history_grant(&self, channel: &str, epoch: u64) -> Result<Grant> {
        let local = self.local(channel)?;
        for seq in local.histories.iter().rev() {
            let stored = self.stored(channel, *seq)?;
            let control: Control = from(&stored.payload)?;
            let (from_epoch, to_epoch, upload) = match &control.action {
                Action::History {
                    from_epoch,
                    to_epoch,
                    envelope_upload,
                    ..
                } => (*from_epoch, *to_epoch, envelope_upload.clone()),
                Action::Invite {
                    envelope_upload: Some(upload),
                    ..
                } => (stored.ledger.epoch, stored.ledger.epoch, upload.clone()),
                _ => continue,
            };
            if from_epoch <= epoch && epoch <= to_epoch {
                return Ok(Grant {
                    stored,
                    control,
                    from_epoch,
                    to_epoch,
                    upload,
                });
            }
        }
        Err(Error::RecoveryIncomplete)
    }

    /// The key confirmation of each epoch in `first..=last`.
    fn confirmations(&self, channel: &str, first: u64, last: u64) -> Result<BTreeMap<u64, String>> {
        let local = self.local(channel)?;
        let mut confirmations = BTreeMap::new();
        for (epoch, seq) in local.epochs.range(first..=last) {
            let activation: Activation = from(&self.stored(channel, *seq)?.payload)?;
            confirmations.insert(*epoch, activation.key_confirmation);
        }
        Ok(confirmations)
    }

    /// Record the relay's reply to a message [`send`] sent: stored, its
    /// journals go, and the relay refuses this message ID with other bytes
    /// from now on.
    ///
    /// # Errors
    /// The reply's error; `Error::IntegrityFailed` for a receipt of other
    /// bytes.
    fn channel_send_settle(
        &mut self,
        channel: &str,
        message_id: &str,
        reply: Result<Json>,
    ) -> Result<Json> {
        let send = format!("send:{message_id}");
        let receipt = self.journaled_settle(channel, &send, reply)?;
        let operation: Operation = self.job(channel, &send)?.ok_or(Error::IntegrityFailed)?;
        ensure(
            receipt["digest"] == hex_sha256(&unb64(&operation.cose)?).as_str()
                && receipt["seq"].as_u64().is_some_and(|s| s > 0),
            Error::IntegrityFailed,
        )?;
        for key in [
            format!("message:{message_id}"),
            send,
            format!("file:{message_id}"),
        ] {
            self.forget_job(channel, &key)?;
        }
        Ok(receipt)
    }

    /// Check `draft` against the channel as [`Self::seal_message`] does;
    /// whether the epoch's key is still to open here.
    fn seal_check(
        &self,
        channel: &str,
        ledger: &Ledger,
        draft: &Draft,
        message_id: &str,
    ) -> Result<bool> {
        let me = self.me()?;
        ensure(ledger.status == Status::Active, Error::RekeyRequired)?;
        ensure(may_publish(ledger, &me), Error::Forbidden)?;
        let attachment: Option<Attachment> = self.job(channel, &format!("file:{message_id}"))?;
        if let Some(attachment) = &attachment {
            ensure(
                attachment.reference.epoch == ledger.epoch,
                Error::RekeyRequired,
            )?;
        }
        self.check_draft(channel, &me, draft, attachment.is_some())?;
        Ok(!self.keyring(channel)?.1.keys.contains_key(&ledger.epoch))
    }

    /// Check `draft` against the channel and seal it as a new message under
    /// the epoch's key, which must be here.
    fn seal_message(
        &self,
        channel: &str,
        ledger: &Ledger,
        draft: &Draft,
        message_id: &str,
    ) -> Result<Outgoing> {
        self.seal_check(channel, ledger, draft, message_id)?;
        let me = self.me()?;
        let attachment: Option<Attachment> = self.job(channel, &format!("file:{message_id}"))?;
        let (_, ring) = self.keyring(channel)?;
        let root = unb64(
            ring.keys
                .get(&ledger.epoch)
                .ok_or(Error::RecoveryIncomplete)?,
        )?;
        let mut payload = MessagePayload {
            attachment_upload: None,
            channel_id: channel.into(),
            epoch: ledger.epoch,
            control_head: ledger.head.clone(),
            acl_version: ledger.acl_version,
            message_id: message_id.into(),
            client_created_at: now_ms(),
            ciphertext: String::new(),
        };
        payload.attachment_upload = attachment.as_ref().map(|a| a.reference.upload_id.clone());
        let device = self.device_hex();
        let key = message_key(&root, channel, ledger.epoch, message_id, &device);
        let body = Zeroizing::new(canonical(&Body {
            text: draft.text.clone(),
            file: attachment.map(|a| a.reference),
            reply_to: draft.reply_to.clone(),
            edit_of: draft.edit_of.clone(),
            retract: draft.retract.clone(),
            agent: draft.agent.clone(),
            packet: draft.packet.clone(),
        }));
        payload.ciphertext = seal(&key, &body, &message_aad(&me, &device, &payload));
        let outgoing = Outgoing {
            draft: draft.clone(),
            payload,
        };
        self.save_job(channel, &format!("message:{message_id}"), &outgoing)?;
        Ok(outgoing)
    }

    /// A draft says something, names messages this device read and edits
    /// or withdraws only this account's own.
    fn check_draft(&self, channel: &str, me: &str, draft: &Draft, file: bool) -> Result<()> {
        let stored = |target: &MessageRef| -> Result<Message> {
            self.workspace
                .private::<Message>(
                    self.keys()?,
                    &format!("{INBOX}{channel}:{:020}", target.seq),
                )?
                .filter(|m| m.digest == target.digest && !m.unreadable && !m.amends())
                .ok_or(Error::NotFound)
        };
        if let Some(target) = &draft.reply_to {
            stored(target)?;
        }
        let amended = draft.edit_of.as_ref().or(draft.retract.as_ref());
        if let Some(target) = amended {
            let target = stored(target)?;
            ensure(target.account == me && !target.retracted, Error::Forbidden)?;
        }
        let packet = draft.packet.as_ref().map(canonical);
        ensure_valid(
            (draft.edit_of.is_none() || draft.retract.is_none())
                && (draft.edit_of.is_none() || (!draft.text.is_empty() && !file))
                && (draft.retract.is_none() || (draft.text.is_empty() && !file))
                && (amended.is_none() || draft.packet.is_none())
                && draft.packet.as_ref().is_none_or(|p| {
                    p["channel"] == channel
                        && matches!(
                            p["format"].as_str(),
                            Some(
                                "dmsg-channel-owner-transfer/1"
                                    | "dmsg-channel-sponsor/1"
                                    | "dmsg-wallet-event/1"
                            )
                        )
                })
                && packet.is_none_or(|p| p.len() <= MAX_TEXT)
                && draft.agent.as_ref().is_none_or(|a| {
                    !a.id.is_empty() && a.id.len() <= 256 && a.name.chars().count() <= MAX_NAME
                })
                && (!draft.text.is_empty()
                    || file
                    || draft.retract.is_some()
                    || draft.packet.is_some()),
            "draft",
        )
    }

    /// Messages of a channel still on their way: sealed or prepared on this
    /// device and not yet stored by the relay, oldest first.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn channel_outbox(&self, channel: &str) -> Result<Vec<Json>> {
        let keys = self.keys()?;
        let prefix = format!("{JOB}{channel}:");
        let mut items: BTreeMap<String, Json> = BTreeMap::new();
        for key in self
            .workspace
            .store
            .sealed_keys(&format!("{prefix}message:"))?
        {
            let id = key[prefix.len() + "message:".len()..].to_string();
            if let Some(outgoing) = self.workspace.private::<Outgoing>(keys, &key)? {
                let item = items
                    .entry(id.clone())
                    .or_insert_with(|| json!({"message_id": id}));
                item["draft"] = json(&outgoing.draft);
                item["created_at"] = outgoing.payload.client_created_at.into();
            }
        }
        for key in self
            .workspace
            .store
            .sealed_keys(&format!("{prefix}file:"))?
        {
            let id = key[prefix.len() + "file:".len()..].to_string();
            if let Some(attachment) = self.workspace.private::<Attachment>(keys, &key)? {
                let item = items
                    .entry(id.clone())
                    .or_insert_with(|| json!({"message_id": id}));
                item["file"] = json!({
                    "name": attachment.reference.name,
                    "size": attachment.reference.size,
                    "source": attachment.source,
                });
            }
        }
        let mut items: Vec<Json> = items.into_values().collect();
        items.sort_by_key(|i| i["created_at"].as_u64().unwrap_or(u64::MAX));
        Ok(items)
    }

    /// Give up a message of the outbox. One the relay stored meanwhile still
    /// arrives with the next read.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for a malformed message ID.
    pub fn channel_discard(&self, channel: &str, message_id: &str) -> Result<()> {
        base(channel)?;
        ensure_valid(is_hash(message_id), "message_id")?;
        for key in ["message", "send", "file"] {
            self.forget_job(channel, &format!("{key}:{message_id}"))?;
        }
        Ok(())
    }

    /// The upload that shares `file`, whose chunks are in `blobs`, under
    /// the ledger's epoch, and the reference that carries its key.
    pub(crate) fn prepare_upload(
        &self,
        channel: &str,
        ledger: &Ledger,
        file: &FileManifest,
        blobs: &dyn BlobStore,
    ) -> Result<(FileRef, Upload)> {
        let mut chunks = vec![];
        for chunk in &file.chunks {
            let bytes = blobs
                .chunk(&chunk.digest)?
                .ok_or(Error::RecoveryIncomplete)?;
            chunks.push(ChunkRef {
                digest: chunk.digest.clone(),
                size: bytes.len() as u64,
            });
        }
        if chunks.is_empty() {
            let empty = empty_file();
            let digest = hex_sha256(&empty);
            blobs.put_chunk(&digest, &empty)?;
            chunks.push(ChunkRef {
                digest,
                size: empty.len() as u64,
            });
        }
        let upload_id = random_hex();
        let key = key32(&unb64(&file.key)?)?;
        let manifest = seal_bytes(
            &key,
            &canonical(&ChannelFile {
                format: "dmsg-channel-file/1".into(),
                channel: channel.into(),
                epoch: ledger.epoch,
                file: file.public(),
            }),
            &manifest_context(channel, ledger.epoch, &upload_id, &file.id, &file.version),
        );
        let plan = UploadPlan {
            management: None,
            upload_id: upload_id.clone(),
            object_id: file.id.clone(),
            version_id: file.version.clone(),
            root_generation: 0,
            epoch: ledger.epoch,
            control_head: Some(ledger.head.clone()),
            kind: "file".into(),
            chunks,
            manifest_digest: hex_sha256(&manifest),
            manifest_size: manifest.len() as u64,
            expires_at: now_ms() + PLAN_MS,
        };
        let reference = FileRef {
            upload_id,
            file_id: file.id.clone(),
            version: file.version.clone(),
            epoch: ledger.epoch,
            manifest_digest: plan.manifest_digest.clone(),
            name: file.name.clone(),
            size: file.size,
            mime: file.mime.clone(),
            key: file.key.clone(),
        };
        self.hold_uploads([&plan])?;
        Ok((
            reference,
            Upload {
                plan,
                manifest: ByteBuf::from(manifest),
                recipients: vec![],
            },
        ))
    }

    /// Verify, open and keep a page of messages.
    fn messages_page(&mut self, reading: &mut Reading, page: &Json, trust: &Trust) -> Result<()> {
        let channel = reading.channel.clone();
        let ledger = reading.ledger.clone().ok_or(Error::IntegrityFailed)?;
        if page["head"] != ledger.head.as_str() {
            // A control landed since the log was read: read it, then the
            // page again.
            reading.stale = Some(ledger.head);
            reading.step = Step::Control;
            return Ok(());
        }
        let me = self.me()?;
        let local = self.local(&channel)?;
        let ring = self.keyring(&channel)?.1;
        let after = reading.after.unwrap_or_default();
        let next = page["next_seq"].as_u64().ok_or(Error::IntegrityFailed)?;
        let latest = page["latest_seq"].as_u64().ok_or(Error::IntegrityFailed)?;
        let entries = page["entries"].as_array().ok_or(Error::IntegrityFailed)?;
        let skipped = page["skipped"].as_array().ok_or(Error::IntegrityFailed)?;
        ensure(
            next >= after && latest >= next && entries.len() <= 20 && next - after <= 20,
            Error::IntegrityFailed,
        )?;
        let mut seen = BTreeSet::new();
        let mut unread_epochs = vec![];
        for row in skipped {
            let seq = row["seq"].as_u64().ok_or(Error::IntegrityFailed)?;
            let epoch = row["epoch"].as_u64().ok_or(Error::IntegrityFailed)?;
            ensure(
                seq > after
                    && seq <= next
                    && seen.insert(seq)
                    && epoch > 0
                    && epoch <= ledger.epoch
                    && !readable(&ledger, &me, epoch),
                Error::IntegrityFailed,
            )?;
            unread_epochs.push((epoch, seq));
        }
        let mut rows = vec![];
        let mut ledgers: BTreeMap<String, Ledger> = BTreeMap::new();
        for row in entries {
            let seq = row["seq"].as_u64().ok_or(Error::IntegrityFailed)?;
            let receipt = &row["receipt"];
            let stored_at = receipt["stored_at"]
                .as_u64()
                .ok_or(Error::IntegrityFailed)?;
            let cose = row["signed"]["cose_sign1"]
                .as_str()
                .ok_or(Error::IntegrityFailed)?;
            let evidence: Evidence = from(&row["evidence"])?;
            let verified: Verified<MessagePayload> =
                trust.command(cose, &evidence, stored_at, Some(MESSAGE), now_ms())?;
            let payload = verified.body.payload.clone();
            let digest = hex_sha256(&unb64(cose)?);
            ensure(
                payload.channel_id == channel
                    && row["epoch"].as_u64() == Some(payload.epoch)
                    && receipt["seq"].as_u64() == Some(seq)
                    && receipt["digest"] == digest.as_str()
                    && seq > after
                    && seq <= next
                    && seen.insert(seq),
                Error::IntegrityFailed,
            )?;
            let author = verified.account.to_string();
            let historical = match ledgers.get(&payload.control_head) {
                Some(ledger) => ledger.clone(),
                None => {
                    let found = self.ledger_at(&channel, &local, &payload.control_head)?;
                    ledgers.insert(payload.control_head.clone(), found.clone());
                    found
                }
            };
            let role = historical.members.get(&author).map(|m| m.role);
            ensure(
                role.is_some()
                    && historical.epoch == payload.epoch
                    && historical.acl_version == payload.acl_version
                    && (historical.kind != ChannelType::Distribution || role != Some(Role::Member))
                    && readable(&ledger, &me, payload.epoch),
                Error::Forbidden,
            )?;
            rows.push((seq, stored_at, author, verified.device, payload, digest));
        }
        ensure(seen.len() as u64 == next - after, Error::IntegrityFailed)?;
        ensure(next > after || next == latest, Error::IntegrityFailed)?;
        // Kept already, unless it could not be read then.
        let mut fresh = Vec::with_capacity(rows.len());
        for row in rows {
            let key_name = format!("{INBOX}{channel}:{:020}", row.0);
            if !self
                .workspace
                .private::<Message>(self.keys()?, &key_name)?
                .is_some_and(|m| !m.unreadable)
            {
                fresh.push((key_name, row));
            }
        }
        // The keys of the page's epochs open first, outside the client; the
        // page is asked for again then.
        let missing: BTreeSet<u64> = fresh
            .iter()
            .map(|(_, row)| row.4.epoch)
            .filter(|epoch| !ring.keys.contains_key(epoch) && !reading.tried.contains(epoch))
            .collect();
        if !missing.is_empty() {
            reading.opening = missing.into_iter().collect();
            reading.step = Step::Epochs;
            return Ok(());
        }
        let mut opened = Vec::with_capacity(fresh.len());
        for (key_name, (seq, stored_at, author, device, payload, digest)) in fresh {
            let message = Message {
                seq,
                account: author,
                device: hex::encode(*device),
                message_id: payload.message_id.clone(),
                epoch: payload.epoch,
                created_at: payload.client_created_at,
                stored_at,
                digest,
                ..Message::default()
            };
            opened.push((key_name, open_message(&channel, &ring, message, &payload)));
        }
        let reread = reading.range.is_some();
        let keys = self.keys()?;
        // The page's messages and the cursor past them in one write. Another
        // read of the channel may have kept the page first: only what is past
        // the cursor is news, and the cursor never goes back.
        self.workspace.store.atomic(|_| {
            let mut local = self.local(&channel)?;
            let cursor = local.message;
            for (epoch, seq) in &unread_epochs {
                extend(&mut local.skipped, *epoch, *seq);
            }
            for (key, message) in &opened {
                if message.unreadable {
                    extend(&mut local.skipped, message.epoch, message.seq);
                }
                self.workspace.put_private(keys, key, message)?;
                self.amend(&channel, &mut local, message)?;
                if !reread && !message.amends() && message.seq > cursor {
                    local.last = Some(preview(message));
                    if message.account != me {
                        local.unread += 1;
                    }
                }
            }
            if !reread {
                local.message = cursor.max(next);
            }
            self.save_local(&channel, &local)
        })?;
        reading.received += opened.iter().filter(|(_, m)| !m.amends()).count();
        reading.after = Some(next);
        let end = reading
            .range
            .map_or(latest, |(_, through)| through.min(latest));
        if next >= end {
            let mut local = self.local(&channel)?;
            reading.step = Step::Done;
            if reading.granting {
                local.granted.clear();
                self.save_local(&channel, &local)?;
            } else if reading.range.is_none() && !local.granted.is_empty() {
                reading.step = Step::Grants;
            }
        }
        Ok(())
    }

    /// Apply `message` when it edits or withdraws an earlier message of its
    /// author, and the edits and withdrawals of `message` read before it.
    fn amend(&self, channel: &str, local: &mut Local, message: &Message) -> Result<()> {
        let keys = self.keys()?;
        let name = |seq: u64| format!("{INBOX}{channel}:{seq:020}");
        if let Some(target) = message.edit_of.as_ref().or(message.retract.as_ref()) {
            match self.workspace.private::<Message>(keys, &name(target.seq))? {
                Some(mut stored) if !stored.unreadable => {
                    if amendable(&stored, target, message) {
                        apply(&mut stored, message);
                        self.workspace
                            .put_private(keys, &name(stored.seq), &stored)?;
                        if local.last.as_ref().is_some_and(|l| l.seq == stored.seq) {
                            local.last = Some(preview(&stored));
                        }
                    }
                }
                _ => local
                    .amends
                    .entry(target.seq)
                    .or_default()
                    .push(message.seq),
            }
        }
        if !message.unreadable
            && let Some(pending) = local.amends.remove(&message.seq)
        {
            let mut current = message.clone();
            for seq in pending {
                let Some(amend) = self.workspace.private::<Message>(keys, &name(seq))? else {
                    continue;
                };
                let target = amend.edit_of.as_ref().or(amend.retract.as_ref());
                if target.is_some_and(|t| amendable(&current, t, &amend)) {
                    apply(&mut current, &amend);
                }
            }
            self.workspace
                .put_private(keys, &name(current.seq), &current)?;
        }
        Ok(())
    }

    /// The epochs granted to this account whose messages this device
    /// skipped, each with the span of them, for a read to open.
    fn granted_epochs(&self, channel: &str) -> Result<Vec<(u64, (u64, u64))>> {
        let local = self.local(channel)?;
        let mut epochs = vec![];
        for &(from, to) in &local.granted {
            for (epoch, &range) in local.skipped.range(from..=to) {
                epochs.push((*epoch, range));
            }
        }
        Ok(epochs)
    }

    /// Read again the skipped spans of the granted epochs that `opened`;
    /// with none, the grants are through. The spans stay, so a read cut
    /// short starts again with the next one, which finds the grants still
    /// to read.
    fn grants_opened(&mut self, reading: &mut Reading, opened: &[(u64, u64)]) -> Result<()> {
        let span = opened
            .iter()
            .fold(None, |span: Option<(u64, u64)>, &(first, last)| {
                Some(span.map_or((first, last), |(a, b)| (a.min(first), b.max(last))))
            });
        match span {
            Some((first, last)) => {
                reading.range = Some((first - 1, last));
                reading.after = Some(first - 1);
                reading.granting = true;
                reading.step = Step::Messages;
            }
            None => {
                let mut local = self.local(&reading.channel)?;
                local.granted.clear();
                self.save_local(&reading.channel, &local)?;
                reading.step = Step::Done;
            }
        }
        Ok(())
    }

    /// The messages this device read, after `after`, at most `limit`;
    /// edits and withdrawals show through the messages they name.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    fn public_message(&self, channel: &str, message: &Message) -> Result<Message> {
        let mut result = message.public();
        if let Some(file) = &result.file {
            result.file_deleted = self
                .workspace
                .private::<Json>(
                    self.keys()?,
                    &format!("channel-deleted:{channel}:{}", file.upload_id),
                )?
                .is_some();
        }
        Ok(result)
    }

    /// Messages with local deletion markers applied; signed history is retained.
    pub fn channel_messages(
        &self,
        channel: &str,
        after: u64,
        limit: usize,
    ) -> Result<Vec<Message>> {
        base(channel)?;
        // Messages are named by their zero-padded sequence: the page is a
        // range of names, and only its messages are opened.
        let prefix = format!("{INBOX}{channel}:");
        let mut shown = vec![];
        let mut from = after;
        while shown.len() < limit {
            let page: Vec<Message> = self.workspace.private_after(
                self.keys()?,
                &prefix,
                &format!("{prefix}{from:020}"),
                limit,
            )?;
            let Some(last) = page.last() else {
                break;
            };
            from = last.seq;
            shown.extend(page.into_iter().filter(|m| !m.amends()));
        }
        shown.truncate(limit);
        shown
            .iter()
            .map(|m| self.public_message(channel, m))
            .collect()
    }

    /// The `limit` messages before `before`, oldest first, as
    /// [`Self::channel_messages`] shows them.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn channel_messages_before(
        &self,
        channel: &str,
        before: u64,
        limit: usize,
    ) -> Result<Vec<Message>> {
        base(channel)?;
        let prefix = format!("{INBOX}{channel}:");
        let mut shown = vec![];
        let mut until = before;
        while shown.len() < limit && until > 1 {
            let page: Vec<Message> = self.workspace.private_before(
                self.keys()?,
                &prefix,
                &format!("{prefix}{until:020}"),
                limit,
            )?;
            let Some(first) = page.first() else {
                break;
            };
            until = first.seq;
            let mut kept: Vec<Message> = page.into_iter().filter(|m| !m.amends()).collect();
            kept.extend(shown);
            shown = kept;
        }
        let skip = shown.len().saturating_sub(limit);
        shown[skip..]
            .iter()
            .map(|m| self.public_message(channel, m))
            .collect()
    }

    /// The messages of a channel whose text or file name holds `query`,
    /// newest first, at most `limit`.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn channel_search(&self, channel: &str, query: &str, limit: usize) -> Result<Vec<Message>> {
        base(channel)?;
        let query = query.trim().to_lowercase();
        ensure_valid(!query.is_empty() && query.chars().count() <= 200, "query")?;
        let prefix = format!("{INBOX}{channel}:");
        let mut found = vec![];
        let mut from = 0;
        loop {
            let page: Vec<Message> = self.workspace.private_after(
                self.keys()?,
                &prefix,
                &format!("{prefix}{from:020}"),
                500,
            )?;
            let Some(last) = page.last() else {
                break;
            };
            from = last.seq;
            found.extend(page.into_iter().filter(|m| {
                !m.amends()
                    && !m.retracted
                    && !m.hidden
                    && (m.text.to_lowercase().contains(&query)
                        || m.file
                            .as_ref()
                            .is_some_and(|f| f.name.to_lowercase().contains(&query)))
            }));
        }
        found.reverse();
        found.truncate(limit);
        found
            .iter()
            .map(|m| self.public_message(channel, m))
            .collect()
    }

    /// Mark the messages through `through` read; the unread count follows.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown channel.
    pub fn channel_mark(&self, channel: &str, through: u64) -> Result<ChannelView> {
        let me = self.me()?;
        let mut local = self.local(channel)?;
        local.read = local.read.max(through.min(local.message));
        let prefix = format!("{INBOX}{channel}:");
        let after: Vec<Message> = self.workspace.private_after(
            self.keys()?,
            &prefix,
            &format!("{prefix}{:020}", local.read),
            1000,
        )?;
        local.unread = after
            .iter()
            .filter(|m| m.account != me && !m.amends())
            .count() as u64;
        self.save_local(channel, &local)?;
        self.channel_view(channel)
    }

    /// Keep the composer's unsent text on this device.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for a text over the message limit.
    pub fn channel_draft(&self, channel: &str, text: &str) -> Result<()> {
        ensure_valid(text.len() <= MAX_TEXT, "text")?;
        let mut local = self.local(channel)?;
        local.draft = text.into();
        self.save_local(channel, &local)
    }

    /// Rename a channel for this account, or mute or hide it; every device
    /// of the account follows.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for a malformed name.
    pub fn channel_settings(
        &mut self,
        channel: &str,
        name: Option<&str>,
        muted: Option<bool>,
        hidden: Option<bool>,
    ) -> Result<ChannelView> {
        let (revision, mut ring) = self.keyring(channel)?;
        if let Some(name) = name {
            let name = name.trim();
            ensure_valid(
                !name.is_empty()
                    && name.chars().count() <= MAX_NAME
                    && !name.chars().any(char::is_control),
                "name",
            )?;
            ring.name = name.into();
        }
        ring.muted = muted.unwrap_or(ring.muted);
        ring.hidden = hidden.unwrap_or(ring.hidden);
        self.save_keyring(Some(revision), &ring)?;
        self.channel_view(channel)
    }

    /// Hide a message on this device, or show it again.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for a message this device did not read.
    pub fn channel_hide(&self, channel: &str, seq: u64, hidden: bool) -> Result<Message> {
        base(channel)?;
        let keys = self.keys()?;
        let name = format!("{INBOX}{channel}:{seq:020}");
        let mut message: Message = self
            .workspace
            .private(keys, &name)?
            .ok_or(Error::NotFound)?;
        message.hidden = hidden;
        self.workspace.put_private(keys, &name, &message)?;
        Ok(message.public())
    }

    /// The image message `seq` of `channel` carries, and its author.
    fn image(&self, channel: &str, seq: u64) -> Result<(FileRef, String)> {
        let message: Message = self
            .workspace
            .private(self.keys()?, &format!("{INBOX}{channel}:{seq:020}"))?
            .ok_or(Error::NotFound)?;
        let reference = message.file.ok_or(Error::NotFound)?;
        ensure(
            matches!(
                reference.mime.as_str(),
                "image/png" | "image/jpeg" | "image/gif" | "image/webp"
            ),
            Error::NotFound,
        )?;
        ensure(reference.size <= PREVIEW, Error::QuotaExceeded)?;
        Ok((reference, message.account))
    }

    /// Check the plan [`preview_image`] read, which the image's author signed for
    /// that very file, and list what is still to download.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for the plan of anything else.
    fn channel_preview_plan(
        &self,
        channel: &str,
        seq: u64,
        value: &Json,
        trust: &Trust,
    ) -> Result<Fetching> {
        let (reference, author) = self.image(channel, seq)?;
        let plan = plan_of(trust, value, &reference.upload_id, Some(&author))?;
        ensure(of_file(&plan, &reference), Error::IntegrityFailed)?;
        let objects = format!("{}/objects/{}/chunks", base(channel)?, reference.upload_id);
        let mut parts = vec![(
            format!("{objects}/manifest"),
            reference.manifest_digest.clone(),
        )];
        // An empty file's plan holds a placeholder chunk nothing reads.
        let count = chunk_count(reference.size) as usize;
        for (index, chunk) in plan.chunks.iter().enumerate().take(count) {
            if !self.workspace.store.has_chunk(&chunk.digest)? {
                parts.push((format!("{objects}/{index}"), chunk.digest.clone()));
            }
        }
        Ok(Fetching {
            channel: channel.into(),
            seq,
            plan,
            parts,
        })
    }

    /// The downloads `fetching` still needs, each with the digest it must
    /// have, signed to send without holding the client.
    ///
    /// # Errors
    /// Returns `Error::PolicyStale` until a [`Self::warmup`] ran.
    fn channel_preview_downloads(&self, fetching: &Fetching) -> Result<Vec<(Prepared, String)>> {
        fetching
            .parts
            .iter()
            .map(|(path, digest)| Ok((self.signed_get(path)?, digest.clone())))
            .collect()
    }

    /// Open the image `fetching` downloaded, `parts` in its order: the
    /// sealed manifest is checked against the plan and the chunks are kept.
    /// Returns a data URL to show in place.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for downloads that do not match.
    fn channel_preview_settle(&self, fetching: &Fetching, parts: Vec<Vec<u8>>) -> Result<Json> {
        let (reference, _) = self.image(&fetching.channel, fetching.seq)?;
        ensure(
            parts.len() == fetching.parts.len() && hex_sha256(&parts[0]) == fetching.parts[0].1,
            Error::IntegrityFailed,
        )?;
        let file = shared_manifest(&fetching.channel, &reference, &fetching.plan, &parts[0])?;
        // The store checks each chunk against its digest.
        for ((_, digest), bytes) in fetching.parts.iter().zip(&parts).skip(1) {
            self.workspace.store.put_chunk(digest, bytes)?;
        }
        let manifest = FileManifest {
            key: reference.key.clone(),
            ..file
        };
        let mut plain = Zeroizing::new(Vec::with_capacity(manifest.size as usize));
        read_file(&self.workspace.store, &manifest, PREVIEW, Some(&mut *plain))?;
        use base64::Engine;
        let data = base64::engine::general_purpose::STANDARD.encode(&plain[..]);
        Ok(json!({"mime": reference.mime, "url": format!("data:{};base64,{data}", reference.mime)}))
    }

    /// A message this device read.
    fn message_at(&self, channel: &str, seq: u64) -> Result<Message> {
        self.workspace
            .private(self.keys()?, &format!("{INBOX}{channel}:{seq:020}"))?
            .ok_or(Error::NotFound)
    }

    /// Seal keys of epochs `from..=to` to `target`'s devices and grant them.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` unless this account manages the channel
    /// and reads every epoch in the range.
    pub async fn channel_grant_history(
        &mut self,
        channel: &str,
        target: &AccountId,
        from_epoch: u64,
        to_epoch: u64,
    ) -> Result<Ledger> {
        let target = target.to_string();
        let ledger = self.channel_pull(channel, None, None).await?;
        self.root_ready().await?;
        let me = self.me()?;
        ensure(
            ledger.status == Status::Active
                && ledger
                    .members
                    .get(&me)
                    .is_some_and(|m| matches!(m.role, Role::Owner | Role::Admin))
                && ledger.members.contains_key(&target)
                && from_epoch > 0
                && to_epoch >= from_epoch
                && to_epoch <= ledger.epoch
                && to_epoch - from_epoch < 1000,
            Error::Forbidden,
        )?;
        for epoch in from_epoch..=to_epoch {
            ensure(readable(&ledger, &me, epoch), Error::Forbidden)?;
            self.channel_open_epoch(channel, epoch).await?;
        }
        let grant_id = random_hex();
        let current = self.recipients(std::slice::from_ref(&target)).await?;
        let uploads = self.history_prepare(
            channel,
            &ledger,
            &target,
            from_epoch,
            to_epoch,
            &current.recipients,
            &grant_id,
        )?;
        for upload in &uploads {
            self.channel_upload(channel, upload, Source::Store).await?;
        }
        let fresh = self.recipients(std::slice::from_ref(&target)).await?;
        ensure(
            fresh.versions == current.versions && fresh.recipients == current.recipients,
            Error::PolicyStale,
        )?;
        let control = Control {
            expected_head: ledger.head.clone(),
            expected_acl_version: ledger.acl_version,
            action: Action::History {
                account: target,
                from_epoch,
                to_epoch,
                envelope_upload: uploads[0].plan.upload_id.clone(),
            },
        };
        self.channel_post(
            channel,
            &format!("history:{grant_id}"),
            CONTROL,
            json(&control),
            "/control",
        )
        .await?;
        // Each grant has its own ID, so nothing reads its journals again.
        self.forget_job(channel, &format!("history-plan:{grant_id}"))?;
        self.forget_job(channel, &format!("history:{grant_id}"))?;
        self.channel_pull(channel, None, None).await
    }

    /// The envelopes of a history grant: per recipient device, the keys of
    /// up to 32 epochs each, at most 192 in all as the reader takes them.
    #[allow(clippy::too_many_arguments)]
    fn history_prepare(
        &self,
        channel: &str,
        ledger: &Ledger,
        target: &str,
        from_epoch: u64,
        to_epoch: u64,
        recipients: &[Recipient],
        grant_id: &str,
    ) -> Result<Vec<Upload>> {
        let spans = (to_epoch - from_epoch) / 32 + 1;
        ensure(
            !recipients.is_empty()
                && recipients.len() as u64 * spans <= 192
                && recipients
                    .iter()
                    .all(|r| r.recipient.starts_with(&format!("device:{target}:"))),
            Error::Forbidden,
        )?;
        let (_, ring) = self.keyring(channel)?;
        let scope = HistoryScope {
            channel: channel.into(),
            target: target.into(),
            from: from_epoch,
            to: to_epoch,
            grant_id: grant_id.into(),
            head: ledger.head.clone(),
            epoch: ledger.epoch,
            recipients_digest: recipients_digest(recipients),
        };
        let key_name = format!("history-plan:{grant_id}");
        if let Some(plan) = self.job::<HistoryPlan>(channel, &key_name)? {
            ensure(plan.scope == scope, Error::IdempotencyConflict)?;
            return Ok(plan.uploads);
        }
        let groups: Vec<String> = (0..(recipients.len() as u64 * spans).div_ceil(GROUP as u64))
            .map(|_| random_hex())
            .collect();
        let mut chunks: Vec<ChunkRef> = vec![];
        let mut references = vec![];
        for r in recipients {
            let public: [u8; 32] = hex::decode(&r.hpke_pub)
                .ok()
                .and_then(|p| p.try_into().ok())
                .ok_or(Error::IntegrityFailed)?;
            let mut start = from_epoch;
            while start <= to_epoch {
                let end = (start + 31).min(to_epoch);
                let mut keys: Vec<(u64, ByteBuf)> = vec![];
                for epoch in start..=end {
                    let key = ring.keys.get(&epoch).ok_or(Error::RecoveryIncomplete)?;
                    keys.push((epoch, ByteBuf::from(unb64(key)?)));
                }
                let plain = Zeroizing::new(canonical(&keys));
                let envelope = canonical(&hpke_seal(
                    &public,
                    &plain,
                    &history_context(&scope, &r.recipient, start, end),
                )?);
                ensure(envelope.len() <= 2048, Error::QuotaExceeded)?;
                let index = chunks.len();
                let digest = hex_sha256(&envelope);
                self.workspace.store.put_chunk(&digest, &envelope)?;
                references.push(HistoryRef {
                    recipient: r.recipient.clone(),
                    from: start,
                    to: end,
                    upload_id: groups[index / GROUP].clone(),
                    chunk: (index % GROUP) as u64,
                    digest: digest.clone(),
                    size: envelope.len() as u64,
                });
                chunks.push(ChunkRef {
                    digest,
                    size: envelope.len() as u64,
                });
                start = end + 1;
            }
        }
        let manifest = canonical(&HistoryManifest {
            format: "dmsg-channel-history/1".into(),
            scope: scope.clone(),
            envelopes: references,
        });
        ensure(manifest.len() <= 65_536, Error::QuotaExceeded)?;
        let uploads: Vec<Upload> = groups
            .iter()
            .enumerate()
            .map(|(index, upload_id)| {
                let part: Vec<ChunkRef> = chunks
                    .iter()
                    .skip(index * GROUP)
                    .take(GROUP)
                    .cloned()
                    .collect();
                Upload {
                    plan: UploadPlan {
                        management: None,
                        upload_id: upload_id.clone(),
                        object_id: channel.into(),
                        version_id: random_hex(),
                        root_generation: 0,
                        epoch: ledger.epoch,
                        control_head: Some(ledger.head.clone()),
                        kind: "envelopes".into(),
                        chunks: part,
                        manifest_digest: hex_sha256(&manifest),
                        manifest_size: manifest.len() as u64,
                        expires_at: now_ms() + PLAN_MS,
                    },
                    manifest: ByteBuf::from(manifest.clone()),
                    recipients: vec![],
                }
            })
            .collect();
        self.hold_uploads(uploads.iter().map(|u| &u.plan))?;
        self.save_job(
            channel,
            &key_name,
            &HistoryPlan {
                scope,
                uploads: uploads.clone(),
            },
        )?;
        Ok(uploads)
    }

    /// A command context for a proposal another account approves.
    async fn proposal_context(&mut self) -> Result<(Context, Evidence)> {
        let state = self.cloud_state().await?;
        let deadline = self.signer(&state)?.deadline(now_ms())?;
        let account = self.account()?;
        Ok((
            Context {
                account_id: account.id.to_string(),
                issuer: account.issuer.clone(),
                device_id: self.device_hex(),
                security_epoch: state.snapshot.security_epoch,
                request_id: random_hex(),
                deadline,
            },
            state.evidence,
        ))
    }

    /// Sign the final command of a proposal with exactly its context.
    async fn sign_fixed<P: Serialize>(
        &mut self,
        context: &Context,
        action: &str,
        path: String,
        payload: &P,
    ) -> Result<Operation> {
        let state = self.cloud_state().await?;
        let signer = self.signer(&state)?;
        ensure(
            signer.security_epoch == context.security_epoch && context.deadline > now_ms(),
            Error::Expired,
        )?;
        let cose = sign_command(
            &signer,
            action,
            payload,
            &context.request_id,
            context.deadline,
        );
        Ok(Operation {
            action: action.into(),
            path,
            payload: json(payload),
            request_id: context.request_id.clone(),
            cose,
            deadline: context.deadline,
            fixed: true,
            result: None,
        })
    }

    /// The device of `account` named in a packet, from its evidence at `at`.
    fn packet_device(
        trust: &Trust,
        evidence: &Evidence,
        account: &str,
        device: &str,
        at: u64,
    ) -> Result<(crate::account::Security, dmsg_types::user::Device)> {
        let id = AccountId::from_str(account).map_err(|_| Error::IntegrityFailed)?;
        trust.device(evidence, &id, &hex_id(device)?, at, now_ms())
    }

    /// Propose handing ownership to `target`, whose device accepts it.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` unless this account owns the channel and
    /// `target` is a member.
    pub async fn channel_owner_propose(
        &mut self,
        channel: &str,
        target: &AccountId,
    ) -> Result<OwnerTransfer> {
        let target = target.to_string();
        let ledger = self.channel_pull(channel, None, None).await?;
        let me = self.me()?;
        ensure(
            ledger.owner == me && ledger.members.contains_key(&target) && target != me,
            Error::Forbidden,
        )?;
        let id = AccountId::from_str(&target).map_err(|_| Error::IntegrityFailed)?;
        let security = self.peer_security(&id).await?;
        let device = security
            .devices
            .iter()
            .find(|(_, d)| {
                d.revoked_at.is_none() && d.input.capabilities.contains(&Capability::ContentSign)
            })
            .map(|(id, _)| hex::encode(**id))
            .ok_or(Error::AuthRequired)?;
        let (context, evidence) = self.proposal_context().await?;
        let payload = TransferIntent {
            expected_head: ledger.head.clone(),
            expected_acl_version: ledger.acl_version,
            action: TransferAction {
                kind: "transfer".into(),
                account: target,
                device,
            },
        };
        let signature = sign(
            &self.keys()?.signing,
            digest(
                "dmsg/channel/owner-proposal/v1",
                &unsigned(&context, CONTROL, &payload),
            )
            .as_slice(),
        );
        let (_, ring) = self.keyring(channel)?;
        let packet = OwnerTransfer {
            format: "dmsg-channel-owner-transfer/1".into(),
            channel: channel.into(),
            genesis: ring.genesis,
            context,
            payload,
            evidence,
            proposed_at: now_ms(),
            proposal_signature: b64(&signature),
            acceptance: None,
            target_evidence: None,
            accepted_at: None,
        };
        self.save_job(
            channel,
            &format!("owner:{}", packet.context.request_id),
            &packet,
        )?;
        Ok(packet)
    }

    /// Accept ownership proposed to this device.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a proposal to another device and
    /// `Error::AuthRequired` for one the owner's device did not sign.
    pub async fn channel_owner_accept(&mut self, packet: &OwnerTransfer) -> Result<OwnerTransfer> {
        let me = self.me()?;
        ensure(
            packet.format == "dmsg-channel-owner-transfer/1"
                && packet.payload.action.kind == "transfer"
                && packet.payload.action.account == me
                && packet.payload.action.device == self.device_hex()
                && packet.context.deadline > now_ms(),
            Error::Forbidden,
        )?;
        let ledger = self.channel_pull(&packet.channel, None, None).await?;
        let (_, ring) = self.keyring(&packet.channel)?;
        ensure(
            ring.genesis == packet.genesis
                && ledger.head == packet.payload.expected_head
                && ledger.acl_version == packet.payload.expected_acl_version
                && ledger.owner == packet.context.account_id,
            Error::IntegrityFailed,
        )?;
        let trust = self.trust().await?;
        let (security, device) = Self::packet_device(
            &trust,
            &packet.evidence,
            &packet.context.account_id,
            &packet.context.device_id,
            packet.proposed_at,
        )?;
        let message = digest(
            "dmsg/channel/owner-proposal/v1",
            &unsigned(&packet.context, CONTROL, &packet.payload),
        );
        ensure(
            security.snapshot.security_epoch == packet.context.security_epoch
                && security.snapshot.issuer == packet.context.issuer
                && dmsg_protocol::verify(
                    &device.input.signing_pub,
                    message.as_slice(),
                    &unb64(&packet.proposal_signature)?,
                )
                .is_ok(),
            Error::AuthRequired,
        )?;
        let state = self.cloud_state().await?;
        state.active(&self.workspace.meta.device_id)?;
        let acceptance = sign(
            &self.keys()?.signing,
            digest(
                "dmsg/channel/owner-acceptance/v1",
                &unsigned(&packet.context, CONTROL, &packet.payload),
            )
            .as_slice(),
        );
        Ok(OwnerTransfer {
            acceptance: Some(b64(&acceptance)),
            target_evidence: Some(state.evidence),
            accepted_at: Some(now_ms()),
            ..packet.clone()
        })
    }

    /// Commit an accepted ownership transfer this device proposed.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` when the acceptance does not verify and
    /// `Error::Expired` once the proposal's deadline passed.
    pub async fn channel_owner_commit(&mut self, packet: &OwnerTransfer) -> Result<Ledger> {
        let channel = packet.channel.clone();
        let original: OwnerTransfer = self
            .job(&channel, &format!("owner:{}", packet.context.request_id))?
            .ok_or(Error::NotFound)?;
        let acceptance = packet.acceptance.as_ref().ok_or(Error::AuthRequired)?;
        let target_evidence = packet.target_evidence.as_ref().ok_or(Error::AuthRequired)?;
        let accepted_at = packet.accepted_at.ok_or(Error::AuthRequired)?;
        ensure(
            original.context == packet.context
                && original.payload == packet.payload
                && packet.context.account_id == self.me()?,
            Error::AuthRequired,
        )?;
        let key = format!("control:{}", packet.context.request_id);
        if self.job::<Operation>(&channel, &key)?.is_none() {
            let ledger = self.channel_pull(&channel, None, None).await?;
            ensure(
                ledger.head == packet.payload.expected_head && ledger.owner == self.me()?,
                Error::VersionConflict,
            )?;
            let trust = self.trust().await?;
            let (_, device) = Self::packet_device(
                &trust,
                target_evidence,
                &packet.payload.action.account,
                &packet.payload.action.device,
                accepted_at,
            )?;
            dmsg_protocol::verify(
                &device.input.signing_pub,
                digest(
                    "dmsg/channel/owner-acceptance/v1",
                    &unsigned(&packet.context, CONTROL, &packet.payload),
                )
                .as_slice(),
                &unb64(acceptance)?,
            )
            .map_err(|_| Error::AuthRequired)?;
            let control = Control {
                expected_head: packet.payload.expected_head.clone(),
                expected_acl_version: packet.payload.expected_acl_version,
                action: Action::Transfer {
                    account: packet.payload.action.account.clone(),
                    device: packet.payload.action.device.clone(),
                    signature: acceptance.clone(),
                },
            };
            let operation = self
                .sign_fixed(
                    &packet.context,
                    CONTROL,
                    format!("{}/control", base(&channel)?),
                    &control,
                )
                .await?;
            self.save_job(&channel, &key, &operation)?;
        }
        let operation: Operation = self.job(&channel, &key)?.ok_or(Error::IntegrityFailed)?;
        self.channel_post(&channel, &key, CONTROL, operation.payload, "/control")
            .await?;
        self.channel_pull(&channel, None, None).await
    }

    /// Propose moving the channel's billing to `payer`, whose device accepts.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` unless this account owns the channel.
    pub async fn channel_sponsor_propose(
        &mut self,
        channel: &str,
        payer: &AccountId,
    ) -> Result<Sponsor> {
        let payer = payer.to_string();
        let ledger = self.channel_pull(channel, None, None).await?;
        let current = self.relay_get(&base(channel)?).await?;
        let version = current["billing"]["billing_version"]
            .as_u64()
            .ok_or(Error::Forbidden)?;
        ensure(ledger.owner == self.me()?, Error::Forbidden)?;
        let transition_id = random_hex();
        let prepare = json!({
            "transition_id": transition_id,
            "to_payer": payer,
            "expected_billing_version": version,
        });
        let operation = self
            .sign_operation(
                "dmsg/channel/billing/prepare/v1",
                format!("{}/billing/prepare", base(channel)?),
                prepare,
                random_hex(),
            )
            .await?;
        self.save_job(
            channel,
            &format!("sponsor-prepare:{transition_id}"),
            &operation,
        )?;
        let transition = self
            .channel_post(
                channel,
                &format!("sponsor-prepare:{transition_id}"),
                "dmsg/channel/billing/prepare/v1",
                operation.payload,
                "/billing/prepare",
            )
            .await?;
        ensure(transition["state"] == "prepared", Error::ExecutionUnknown)?;
        let id = AccountId::from_str(&payer).map_err(|_| Error::IntegrityFailed)?;
        let security = self.peer_security(&id).await?;
        let device = security
            .devices
            .iter()
            .find(|(_, d)| {
                d.revoked_at.is_none() && d.input.capabilities.contains(&Capability::ContentSign)
            })
            .map(|(id, _)| hex::encode(**id))
            .ok_or(Error::AuthRequired)?;
        let payload = BillingIntent {
            transition_id,
            expected_billing_version: version,
            manifest_digest: transition["manifest_digest"]
                .as_str()
                .ok_or(Error::IntegrityFailed)?
                .into(),
            bytes: transition["bytes"].as_u64().ok_or(Error::IntegrityFailed)?,
            slot_id: transition["slot_id"]
                .as_str()
                .ok_or(Error::IntegrityFailed)?
                .into(),
            to_payer: payer,
            device,
        };
        let (context, evidence) = self.proposal_context().await?;
        let signature = sign(
            &self.keys()?.signing,
            digest(
                "dmsg/channel/sponsor-proposal/v1",
                &unsigned(&context, BILLING_TRANSFER, &payload),
            )
            .as_slice(),
        );
        let (_, ring) = self.keyring(channel)?;
        let packet = Sponsor {
            format: "dmsg-channel-sponsor/1".into(),
            channel: channel.into(),
            genesis: ring.genesis,
            context,
            payload,
            evidence,
            proposed_at: now_ms(),
            owner_signature: b64(&signature),
            acceptance: None,
            target_evidence: None,
            accepted_at: None,
        };
        self.save_job(
            channel,
            &format!("sponsor:{}", packet.context.request_id),
            &packet,
        )?;
        Ok(packet)
    }

    /// Accept paying for a channel, as proposed to this device.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a proposal to another device.
    pub async fn channel_sponsor_accept(&mut self, packet: &Sponsor) -> Result<Sponsor> {
        ensure(
            packet.format == "dmsg-channel-sponsor/1"
                && packet.payload.to_payer == self.me()?
                && packet.payload.device == self.device_hex()
                && packet.context.deadline > now_ms(),
            Error::Forbidden,
        )?;
        let trust = self.trust().await?;
        let (_, device) = Self::packet_device(
            &trust,
            &packet.evidence,
            &packet.context.account_id,
            &packet.context.device_id,
            packet.proposed_at,
        )?;
        dmsg_protocol::verify(
            &device.input.signing_pub,
            digest(
                "dmsg/channel/sponsor-proposal/v1",
                &unsigned(&packet.context, BILLING_TRANSFER, &packet.payload),
            )
            .as_slice(),
            &unb64(&packet.owner_signature)?,
        )
        .map_err(|_| Error::AuthRequired)?;
        let state = self.cloud_state().await?;
        let acceptance = sign(
            &self.keys()?.signing,
            digest(
                "dmsg/channel/billing-acceptance/v1",
                &unsigned(&packet.context, BILLING_TRANSFER, &packet.payload),
            )
            .as_slice(),
        );
        Ok(Sponsor {
            acceptance: Some(b64(&acceptance)),
            target_evidence: Some(state.evidence),
            accepted_at: Some(now_ms()),
            ..packet.clone()
        })
    }

    /// Commit an accepted sponsor transfer this device proposed.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` when the acceptance does not verify.
    pub async fn channel_sponsor_commit(&mut self, packet: &Sponsor) -> Result<Json> {
        let channel = packet.channel.clone();
        let original: Sponsor = self
            .job(&channel, &format!("sponsor:{}", packet.context.request_id))?
            .ok_or(Error::NotFound)?;
        let acceptance = packet.acceptance.as_ref().ok_or(Error::AuthRequired)?;
        let target_evidence = packet.target_evidence.as_ref().ok_or(Error::AuthRequired)?;
        let accepted_at = packet.accepted_at.ok_or(Error::AuthRequired)?;
        ensure(
            original.context == packet.context
                && original.payload == packet.payload
                && packet.context.account_id == self.me()?,
            Error::AuthRequired,
        )?;
        let key = format!("sponsor-commit:{}", packet.context.request_id);
        if self.job::<Operation>(&channel, &key)?.is_none() {
            let trust = self.trust().await?;
            let (_, device) = Self::packet_device(
                &trust,
                target_evidence,
                &packet.payload.to_payer,
                &packet.payload.device,
                accepted_at,
            )?;
            dmsg_protocol::verify(
                &device.input.signing_pub,
                digest(
                    "dmsg/channel/billing-acceptance/v1",
                    &unsigned(&packet.context, BILLING_TRANSFER, &packet.payload),
                )
                .as_slice(),
                &unb64(acceptance)?,
            )
            .map_err(|_| Error::AuthRequired)?;
            let mut payload = json(&packet.payload);
            payload["signature"] = acceptance.clone().into();
            let operation = self
                .sign_fixed(
                    &packet.context,
                    BILLING_TRANSFER,
                    format!("{}/billing/transfer", base(&channel)?),
                    &payload,
                )
                .await?;
            self.save_job(&channel, &key, &operation)?;
        }
        let operation: Operation = self.job(&channel, &key)?.ok_or(Error::IntegrityFailed)?;
        let result = self
            .channel_post(
                &channel,
                &key,
                BILLING_TRANSFER,
                operation.payload,
                "/billing/transfer",
            )
            .await?;
        ensure(
            result["billing_account"] == packet.payload.to_payer.as_str(),
            Error::IntegrityFailed,
        )?;
        Ok(result)
    }

    /// Call off a billing transfer this account's owner proposed and the
    /// payer has not completed: the relay pauses the channel's writes while
    /// one is under way, so a payer who never accepts would hold them.
    ///
    /// # Errors
    /// Returns `Error::NotFound` without a transfer under way and
    /// `Error::Forbidden` unless this account owns the channel.
    pub async fn channel_sponsor_cancel(&mut self, channel: &str) -> Result<Ledger> {
        let ledger = self.channel_pull(channel, None, None).await?;
        ensure(ledger.owner == self.me()?, Error::Forbidden)?;
        let current = self.relay_get(&base(channel)?).await?;
        let transition = current["billing_transition"]["id"]
            .as_str()
            .filter(|id| is_hash(id))
            .ok_or(Error::NotFound)?
            .to_string();
        self.channel_post(
            channel,
            &format!("sponsor-cancel:{transition}"),
            "dmsg/channel/billing/cancel/v1",
            json!({"transition_id": transition}),
            "/billing/cancel",
        )
        .await?;
        // The proposal's packet cannot complete any more.
        let prefix = format!("{JOB}{channel}:");
        for key in self
            .workspace
            .store
            .sealed_keys(&format!("{prefix}sponsor"))?
        {
            let key = &key[prefix.len()..];
            if key.starts_with("sponsor:") || key.ends_with(&transition) {
                self.forget_job(channel, key)?;
            }
        }
        self.channel_pull(channel, None, None).await
    }

    /// Reopen an archived channel this account owns; a new epoch follows.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a channel that is not archived.
    pub async fn channel_reactivate(&mut self, channel: &str) -> Result<ChannelView> {
        let ledger = self.channel_pull(channel, None, None).await?;
        let current = self.relay_get(&base(channel)?).await?;
        let version = current["billing"]["billing_version"].as_u64();
        ensure(
            ledger.status == Status::Archived && ledger.owner == self.me()? && version.is_some(),
            Error::Forbidden,
        )?;
        let version = version.expect("checked");
        self.channel_post(
            channel,
            &format!("reactivate:{version}"),
            REACTIVATE,
            json(&Reactivate {
                expected_head: ledger.head,
                expected_billing_version: version,
            }),
            "/reactivate",
        )
        .await?;
        self.channel_pull(channel, None, None).await?;
        self.channel_rotate(channel).await
    }

    /// The delivery cursor to report: the messages this device read past
    /// what it reported last, if any.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn channel_delivery_due(&self, channel: &str) -> Result<Option<u64>> {
        let local = self.local(channel)?;
        Ok((local.message > local.delivered).then_some(local.message))
    }

    /// Tell the relay this account's devices took the messages through
    /// `delivered`, and, when it chooses to say so, read those through
    /// `read`: members see both (C04). Signed to send without the client;
    /// [`Self::channel_cursor_settle`] notes a delivery sent.
    ///
    /// # Errors
    /// Returns `Error::PolicyStale` until a [`Self::warmup`] ran.
    pub fn channel_cursor_request(
        &self,
        channel: &str,
        delivered: u64,
        read: Option<u64>,
    ) -> Result<Prepared> {
        self.signed_post(
            &format!("{}/cursor", base(channel)?),
            CURSOR,
            &json!({"delivered": delivered, "read": read}),
            &Hash::new(crate::crypto::random_id()),
        )
    }

    /// Note the delivery cursor the relay took.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn channel_cursor_settle(&self, channel: &str, delivered: u64) -> Result<()> {
        let mut local = self.local(channel)?;
        local.delivered = local.delivered.max(delivered);
        self.save_local(channel, &local)
    }

    /// The request for the members' delivery and read cursors, as the relay
    /// keeps them, signed for the caller to send without the client; the
    /// relay refuses a channel this account left with `Error::Forbidden`.
    ///
    /// # Errors
    /// Returns `Error::PolicyStale` until a [`Self::warmup`] ran.
    pub fn channel_receipts_request(&self, channel: &str) -> Result<Prepared> {
        self.signed_get(&format!("{}/cursors", base(channel)?))
    }

    /// Operations of a channel still waiting for their outcome.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn channel_pending(&self, channel: &str) -> Result<Vec<Json>> {
        let prefix = format!("{JOB}{channel}:");
        let mut pending = vec![];
        for key in self.workspace.store.sealed_keys(&prefix)? {
            let Ok(Some(operation)) = self.workspace.private::<Operation>(self.keys()?, &key)
            else {
                continue;
            };
            if operation.result.is_none() {
                pending.push(json!({
                    "key": &key[prefix.len()..],
                    "action": operation.action,
                    "request_id": operation.request_id,
                    "deadline": operation.deadline,
                }));
            }
        }
        Ok(pending)
    }

    /// Settle a pending operation: its stored outcome, or send it again.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown operation.
    pub async fn channel_resume(&mut self, channel: &str, key: &str) -> Result<Json> {
        let operation: Operation = self.job(channel, key)?.ok_or(Error::NotFound)?;
        let suffix = operation
            .path
            .strip_prefix(&base(channel)?)
            .ok_or(Error::IntegrityFailed)?
            .to_string();
        let result = self
            .channel_post(channel, key, &operation.action, operation.payload, &suffix)
            .await?;
        self.channel_pull(channel, None, None).await?;
        Ok(result)
    }
}

/// Widen an epoch's span of unread sequences to `seq`.
fn extend(spans: &mut BTreeMap<u64, (u64, u64)>, epoch: u64, seq: u64) {
    spans
        .entry(epoch)
        .and_modify(|(first, last)| {
            *first = (*first).min(seq);
            *last = (*last).max(seq);
        })
        .or_insert((seq, seq));
}

/// What a channel list shows of a message.
fn preview(message: &Message) -> Preview {
    let text = match &message.file {
        _ if message.retracted || message.unreadable => String::new(),
        Some(file) if message.text.is_empty() => file.name.clone(),
        _ => message.text.chars().take(80).collect(),
    };
    Preview {
        seq: message.seq,
        account: message.account.clone(),
        stored_at: message.stored_at,
        text,
    }
}

/// Whether `plan` is the upload of the file `reference` names.
fn of_file(plan: &UploadPlan, reference: &FileRef) -> bool {
    plan.kind == "file"
        && plan.object_id == reference.file_id
        && plan.version_id == reference.version
        && plan.epoch == reference.epoch
        && plan.manifest_digest == reference.manifest_digest
}

/// The keyless manifest of the file `reference` names, opened from the
/// `sealed` manifest of its upload and checked against the file and the
/// upload's `plan`.
fn shared_manifest(
    channel: &str,
    reference: &FileRef,
    plan: &UploadPlan,
    sealed: &[u8],
) -> Result<FileManifest> {
    let key = key32(&unb64(&reference.key)?)?;
    let plain = open_bytes(
        &key,
        sealed,
        &manifest_context(
            channel,
            reference.epoch,
            &reference.upload_id,
            &reference.file_id,
            &reference.version,
        ),
    )?;
    let shared: ChannelFile = decode(&plain, 65_536)?;
    let file = shared.file;
    ensure(
        shared.format == "dmsg-channel-file/1"
            && shared.channel == channel
            && shared.epoch == reference.epoch
            && file.id == reference.file_id
            && file.version == reference.version
            && file.key.is_empty()
            && file.name == reference.name
            && file.size == reference.size
            && file.mime == reference.mime
            && plan.chunks.len() == file.chunks.len().max(1)
            && (file.chunks.is_empty()
                || plan
                    .chunks
                    .iter()
                    .zip(&file.chunks)
                    .all(|(p, f)| p.digest == f.digest)),
        Error::IntegrityFailed,
    )?;
    Ok(file)
}

/// Whether `amend` edits or withdraws `target`, the message `reference`
/// names: the same author's, as read, neither withdrawn nor itself an edit.
fn amendable(target: &Message, reference: &MessageRef, amend: &Message) -> bool {
    target.digest == reference.digest
        && target.account == amend.account
        && !target.amends()
        && !target.retracted
}

/// Apply an edit or a withdrawal to the message it names.
fn apply(target: &mut Message, amend: &Message) {
    if amend.retract.is_some() {
        target.text.clear();
        target.file = None;
        target.packet = None;
        target.retracted = true;
    } else {
        target.text.clone_from(&amend.text);
        target.edited_at = Some(amend.stored_at);
    }
}

/// Members of a channel by role, for display.
pub fn role_name(role: Role) -> &'static str {
    match role {
        Role::Owner => "owner",
        Role::Admin => "admin",
        Role::Publisher => "publisher",
        Role::Member => "member",
    }
}

#[cfg(test)]
#[path = "channel_tests.rs"]
mod tests;
