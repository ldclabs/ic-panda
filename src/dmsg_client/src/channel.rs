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
    advance, is_hash, may_publish, readable, recipients_digest, start, Action, Activation,
    ChannelType, Control, Genesis, Lease, Ledger, MessagePayload, Reactivate, Recipient, Role,
    Status,
};
use crate::client::Client;
use crate::cloud::{sign_command, Evidence, CLOUD_PROTOCOL};
use crate::codec::{b64, canonical, decode, digest, sha256, unb64, MAX_RECORD};
use crate::content::empty_file;
use crate::crypto::{derive, hpke_open, hpke_seal, open, random_key, seal, sign, HpkeEnvelope};
use crate::evidence::{Trust, Verified};
use crate::files::{export_manifest, read_file, BlobStore, FileManifest, Source, MAX_FILE};
use crate::library::catalog::Origin;
use crate::library::import::Imported;
use crate::now_ms;
use crate::upload::{hex_id, random_hex, ChunkRef, UploadPlan};
use dmsg_types::{ensure, ensure_valid, user::Capability, AccountId, Error, Hash, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_bytes::{ByteBuf, Bytes};
use serde_json::{json, Value as Json};
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
const LOCAL: &str = "channel:";
const LOG: &str = "channel-control:";
const INBOX: &str = "channel-msg:";
const JOB: &str = "channel-job:";
/// The chunks of channel uploads in flight, by upload, until their plans lapse.
const UPLOADS: &str = "channel-uploads";
/// Envelopes per upload.
const GROUP: usize = 120;
/// How long an invitation stays open.
const INVITATION_MS: u64 = dmsg_types::DAY;
/// Upload plans lapse after an hour.
const PLAN_MS: u64 = 3_600_000;

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
}

/// One device's replay of a channel, sealed under the LDK.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
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
    /// Control sequences of history grants to this account.
    histories: Vec<u64>,
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
    text: String,
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

/// The sealed body of a message.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file: Option<FileRef>,
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
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Message {
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
}

impl Message {
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
#[derive(Serialize, Clone, Debug)]
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
}

/// What a channel sync read.
#[derive(Serialize, Clone, Debug)]
pub struct ChannelSync {
    /// Relay sequence read through.
    pub through: u64,
    /// New messages.
    pub received: usize,
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

fn hex_sha256(bytes: &[u8]) -> String {
    hex::encode(*sha256(bytes))
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
        ensure(row.kind == CHANNEL, Error::NotFound)?;
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
        let generation = self.roots()?.generation;
        let state = self.cloud_state().await?;
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
        let state = self.cloud_state().await?;
        let signer = self.signer(&state)?;
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
        let path = format!("{base}{suffix}");
        let job: Option<Operation> = self.job(channel, key)?;
        if let Some(job) = &job {
            ensure(
                job.action == action && job.path == path && job.payload == payload,
                Error::IdempotencyConflict,
            )?;
            if let Some(result) = &job.result {
                return Ok(result.clone());
            }
            let found = self
                .relay_find(&format!("{base}/operations/{}", job.request_id))
                .await?;
            if let Some(found) = found.filter(|f| f["found"] == true) {
                let mut job = job.clone();
                job.result = Some(found["result"].clone());
                self.save_job(channel, key, &job)?;
                return Ok(found["result"].clone());
            }
        }
        let mut job = match job {
            Some(job) if job.deadline > now_ms() + 5_000 => job,
            Some(job) if job.fixed => return Err(Error::Expired),
            previous => {
                let request_id = previous.map_or_else(random_hex, |j| j.request_id);
                let job = self
                    .sign_operation(action, path.clone(), payload, request_id)
                    .await?;
                if action == GENESIS {
                    self.renew_genesis(channel, &job.cose)?;
                }
                self.save_job(channel, key, &job)?;
                job
            }
        };
        let state = self.cloud_state().await?;
        let result = self
            .cloud
            .post_signed(
                &self.signer(&state)?,
                &job.path,
                &job.cose,
                &job.request_id,
                job.deadline,
                now_ms(),
            )
            .await?;
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
                        let coordinator = previous.members.get(&account);
                        ensure(
                            coordinator.is_some_and(|c| {
                                c.can_rotate
                                    && (previous.epoch == 0 || c.joined_epoch <= previous.epoch)
                            }) && previous.status != Status::Archived
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
        let base = base(channel)?;
        let (_, ring) = self.keyring(channel)?;
        let mut local = self.local(channel)?;
        let trust = self.trust().await?;
        let me = self.me()?;
        let extra = invitation
            .map(|i| format!("&invitation={i}"))
            .unwrap_or_default();
        for _ in 0..10_000 {
            let page = self
                .relay_get(&format!(
                    "{base}/sync?kind=control&after={}&limit=5{extra}",
                    local.control
                ))
                .await?;
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
                let stored = self.consume(&trust, local.ledger.as_ref(), &event, &ring.genesis)?;
                local.heads.insert(stored.head.clone(), stored.seq);
                match &stored.action[..] {
                    ACTIVATE => {
                        local.epochs.insert(stored.ledger.epoch, stored.seq);
                    }
                    CONTROL => {
                        if let Ok(Control {
                            action: Action::History { account, .. },
                            ..
                        }) = from::<Control>(&stored.payload)
                            && account == me
                        {
                            local.histories.push(stored.seq);
                        }
                    }
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
                self.save_local(channel, &local)
            })?;
            if page["head"] == ledger.head.as_str() {
                if let Some(pinned) = pinned {
                    ensure(local.heads.contains_key(pinned), Error::IntegrityFailed)?;
                }
                let query = invitation
                    .map(|i| format!("?invitation={i}"))
                    .unwrap_or_default();
                let remote = self.relay_get(&format!("{base}{query}")).await?;
                ensure(
                    remote["head"] == ledger.head.as_str()
                        && remote["control_seq"].as_u64() == Some(ledger.control_seq)
                        && remote["epoch"].as_u64() == Some(ledger.epoch)
                        && remote["acl_version"].as_u64() == Some(ledger.acl_version),
                    Error::IntegrityFailed,
                )?;
                let mut ledger = ledger;
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
                local.ledger = Some(ledger.clone());
                self.save_local(channel, &local)?;
                return Ok(ledger);
            }
            // A page without entries below the head is a gap.
            ensure(!entries.is_empty(), Error::IntegrityFailed)?;
        }
        Err(Error::QuotaExceeded)
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
            },
        )?;
        self.save_job(&channel, "genesis", &job)?;
        self.channel_post(&channel, "genesis", GENESIS, job.payload, "")
            .await?;
        self.channel_pull(&channel, None, None).await?;
        self.channel_rotate(&channel).await?;
        Ok(channel)
    }

    /// Sign the genesis of a collaboration channel that the legacy
    /// directory creates once every frozen manager agreed, and remember the
    /// channel by it. Returns the channel ID and the signed genesis.
    pub(crate) async fn legacy_genesis(&mut self, name: &str) -> Result<(String, String)> {
        self.root_ready().await?;
        let channel = random_hex();
        let genesis = Genesis {
            channel_id: channel.clone(),
            nonce: random_hex(),
            kind: ChannelType::Collaboration,
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
            },
        )?;
        Ok((channel, job.cose))
    }

    /// Remember a channel by the genesis digest the legacy directory names.
    pub(crate) fn remember_channel(
        &mut self,
        channel: &str,
        name: &str,
        genesis: &str,
    ) -> Result<()> {
        match self.keyring(channel) {
            Ok((_, ring)) => ensure(ring.genesis == genesis, Error::IntegrityFailed),
            Err(Error::NotFound) => self.save_keyring(
                None,
                &Keyring {
                    format: FORMAT.into(),
                    id: channel.into(),
                    name: name.into(),
                    genesis: genesis.into(),
                    keys: BTreeMap::new(),
                },
            ),
            Err(error) => Err(error),
        }
    }

    /// Have the relay create the canonical successor of an old channel
    /// from the directory's genesis, then read its log and open its first
    /// epoch.
    pub(crate) async fn channel_activate_legacy(
        &mut self,
        channel: &str,
        payload: Json,
    ) -> Result<ChannelView> {
        self.channel_post(
            channel,
            "legacy-activate",
            "dmsg/legacy/activate/v1",
            payload,
            "/legacy-activate",
        )
        .await?;
        self.channel_pull(channel, None, None).await?;
        self.channel_rotate(channel).await
    }

    /// The channels this account knows.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn channel_list(&self) -> Result<Vec<ChannelView>> {
        self.workspace
            .store
            .heads(CHANNEL)?
            .iter()
            .map(|(id, _)| self.channel_view(id))
            .collect()
    }

    /// One channel as this device knows it.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown channel.
    pub fn channel_view(&self, channel: &str) -> Result<ChannelView> {
        let (_, ring) = self.keyring(channel)?;
        let local = self.local(channel)?;
        Ok(ChannelView {
            id: ring.id,
            name: ring.name,
            genesis: ring.genesis,
            ledger: local.ledger,
            message_cursor: local.message,
            readable_epochs: ring.keys.keys().copied().collect(),
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
        self.channel_pull(&invitation.channel, None, None).await
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

    /// Invite `account`; returns the invitation to hand to it.
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
        let expires_at = now_ms() + INVITATION_MS;
        let ledger = self
            .channel_control(
                channel,
                Action::Invite {
                    invitation_id: invitation.clone(),
                    account: account.to_string(),
                    role,
                    expires_at,
                },
                &invitation,
            )
            .await?;
        let (_, ring) = self.keyring(channel)?;
        Ok(Invitation {
            format: "dmsg-channel-invitation/1".into(),
            channel: channel.into(),
            genesis: ring.genesis,
            head: ledger.head,
            invitation,
            account: account.to_string(),
            name: ring.name,
            expires_at,
        })
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
        self.upload(
            &base(channel)?,
            &upload.plan,
            &upload.manifest,
            0,
            source,
            |_, _| Ok(()),
        )
        .await
    }

    /// A channel upload's signed plan, checked against the evidence the
    /// relay stored with it; `uploader`, when given, must have signed it.
    async fn descriptor(
        &mut self,
        channel: &str,
        upload: &str,
        uploader: Option<&str>,
    ) -> Result<UploadPlan> {
        let value = self
            .relay_get(&format!("{}/objects/{upload}", base(channel)?))
            .await?;
        let event: Event = from(&json!({
            "signed": value["signed"],
            "head": "",
            "control_seq": 0,
            "evidence": value["evidence"],
            "stored_at": value["stored_at"],
        }))?;
        let trust = self.trust().await?;
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

    async fn channel_chunk(
        &mut self,
        channel: &str,
        upload: &str,
        chunk: &str,
        digest: &str,
    ) -> Result<Vec<u8>> {
        self.relay_chunk(
            &format!("{}/objects/{upload}/chunks/{chunk}", base(channel)?),
            digest,
        )
        .await
    }

    /// Install an epoch's key from this device's envelope; a device outside
    /// the epoch's recipients needs a history grant.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for an epoch this account may not read and
    /// `Error::RecoveryIncomplete` without an envelope or grant.
    pub async fn channel_open_epoch(&mut self, channel: &str, epoch: u64) -> Result<ChannelView> {
        let local = self.local(channel)?;
        let ledger = local.ledger.clone().ok_or(Error::NotFound)?;
        let me = self.me()?;
        ensure(readable(&ledger, &me, epoch), Error::Forbidden)?;
        let (_, ring) = self.keyring(channel)?;
        if ring.keys.contains_key(&epoch) {
            return self.channel_view(channel);
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
            return self.channel_open_history(channel, epoch).await;
        };
        let before = self.ledger_at(channel, &local, &activation.expected_head)?;
        let trust = self.trust().await?;
        let mut recipients = vec![];
        let mut accounts = BTreeSet::new();
        let mut cursor = 0;
        while accounts.len() < before.members.len() {
            let page = self
                .relay_get(&format!(
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
        let plan = self.descriptor(channel, &reference.upload_id, None).await?;
        let chunk = plan.chunks.get(reference.chunk as usize);
        ensure(
            plan.kind == "envelopes"
                && plan.epoch == epoch
                && plan.control_head.as_deref() == Some(&activation.expected_head)
                && chunk.is_some_and(|c| c.digest == reference.digest && c.size == reference.size),
            Error::IntegrityFailed,
        )?;
        let manifest: EpochManifest = decode(
            &self
                .channel_chunk(
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
        let bytes = self
            .channel_chunk(
                channel,
                &reference.upload_id,
                &reference.chunk.to_string(),
                &reference.digest,
            )
            .await?;
        self.root_ready().await?;
        let envelope: HpkeEnvelope = decode(&bytes, 4096)?;
        let key = hpke_open(
            &self.keys()?.hpke,
            &envelope,
            &envelope_context(channel, &activation, &mine),
        )?;
        ensure(
            key.len() == 32
                && key_confirmation(channel, epoch, &key) == activation.key_confirmation,
            Error::IntegrityFailed,
        )?;
        self.install_keys(channel, BTreeMap::from([(epoch, b64(&key))]))?;
        self.channel_view(channel)
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

    /// Install older epoch keys from the latest history grant to this account.
    async fn channel_open_history(&mut self, channel: &str, epoch: u64) -> Result<ChannelView> {
        let local = self.local(channel)?;
        let me = self.me()?;
        let mut grant = None;
        for seq in local.histories.iter().rev() {
            let stored = self.stored(channel, *seq)?;
            if let Control {
                action:
                    Action::History {
                        from_epoch,
                        to_epoch,
                        ..
                    },
                ..
            } = from::<Control>(&stored.payload)?
                && from_epoch <= epoch
                && epoch <= to_epoch
            {
                grant = Some(stored);
                break;
            }
        }
        let stored = grant.ok_or(Error::RecoveryIncomplete)?;
        let control: Control = from(&stored.payload)?;
        let Action::History {
            from_epoch,
            to_epoch,
            envelope_upload,
            ..
        } = control.action
        else {
            return Err(Error::IntegrityFailed);
        };
        let first = self
            .descriptor(channel, &envelope_upload, Some(&stored.account))
            .await?;
        let manifest: HistoryManifest = decode(
            &self
                .channel_chunk(
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
        let trust = self.trust().await?;
        let own = self.account()?.id;
        let security = trust.account(acceptance, &own, stored.stored_at, now_ms())?;
        let mut recipients = devices_of(&me, &security);
        recipients.sort();
        ensure(
            recipients_digest(&recipients) == scope.recipients_digest,
            Error::PolicyStale,
        )?;
        let mine = recipient(&me, &self.workspace.meta.device_id);
        let references: Vec<&HistoryRef> = manifest
            .envelopes
            .iter()
            .filter(|r| r.recipient == mine)
            .collect();
        ensure(
            !references.is_empty() && references.len() <= 32,
            Error::RecoveryIncomplete,
        )?;
        let mut confirmations = BTreeMap::new();
        for (epoch, seq) in local.epochs.range(scope.from..=scope.to) {
            let activation: Activation = from(&self.stored(channel, *seq)?.payload)?;
            confirmations.insert(*epoch, activation.key_confirmation);
        }
        let mut keys = BTreeMap::new();
        for reference in references {
            let plan = self
                .descriptor(channel, &reference.upload_id, Some(&stored.account))
                .await?;
            let chunk = plan.chunks.get(reference.chunk as usize);
            ensure(
                plan.kind == "envelopes"
                    && plan.epoch == scope.epoch
                    && plan.control_head.as_deref() == Some(&scope.head)
                    && plan.manifest_digest == first.manifest_digest
                    && chunk
                        .is_some_and(|c| c.digest == reference.digest && c.size == reference.size)
                    && reference.from >= scope.from
                    && reference.to <= scope.to
                    && reference.to >= reference.from
                    && reference.to - reference.from < 32,
                Error::IntegrityFailed,
            )?;
            let bytes = self
                .channel_chunk(
                    channel,
                    &reference.upload_id,
                    &reference.chunk.to_string(),
                    &reference.digest,
                )
                .await?;
            let envelope: HpkeEnvelope = decode(&bytes, 4096)?;
            let plain = hpke_open(
                &self.keys()?.hpke,
                &envelope,
                &history_context(&scope, &mine, reference.from, reference.to),
            )?;
            let opened: Vec<(u64, ByteBuf)> = decode(&plain, 65_536)?;
            ensure(
                opened.len() as u64 == reference.to - reference.from + 1,
                Error::IntegrityFailed,
            )?;
            for (index, (epoch, key)) in opened.iter().enumerate() {
                ensure(
                    *epoch == reference.from + index as u64
                        && key.len() == 32
                        && confirmations.get(epoch)
                            == Some(&key_confirmation(channel, *epoch, key))
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
        self.root_ready().await?;
        self.install_keys(channel, keys)?;
        self.channel_view(channel)
    }

    /// Send `text`, with the file prepared under the same message ID, under
    /// the current epoch. The message is journaled before it is signed, so a
    /// retry with the same ID sends the same message.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a member who may not publish and
    /// `Error::RekeyRequired` while the channel waits for a new epoch.
    pub async fn channel_send(
        &mut self,
        channel: &str,
        text: &str,
        message_id: Option<&str>,
    ) -> Result<Json> {
        let message_id = message_id.map_or_else(random_hex, str::to_string);
        ensure_valid(is_hash(&message_id), "message_id")?;
        ensure_valid(text.len() <= MAX_TEXT, "text")?;
        let ledger = self.channel_pull(channel, None, None).await?;
        self.root_ready().await?;
        let key_name = format!("message:{message_id}");
        let outgoing = match self.job::<Outgoing>(channel, &key_name)? {
            Some(outgoing) => {
                ensure(outgoing.text == text, Error::IdempotencyConflict)?;
                outgoing
            }
            None => {
                let me = self.me()?;
                ensure(ledger.status == Status::Active, Error::RekeyRequired)?;
                ensure(may_publish(&ledger, &me), Error::Forbidden)?;
                let (_, ring) = self.keyring(channel)?;
                if !ring.keys.contains_key(&ledger.epoch) {
                    self.channel_open_epoch(channel, ledger.epoch).await?;
                }
                let (_, ring) = self.keyring(channel)?;
                let root = unb64(
                    ring.keys
                        .get(&ledger.epoch)
                        .ok_or(Error::RecoveryIncomplete)?,
                )?;
                let attachment: Option<Attachment> =
                    self.job(channel, &format!("file:{message_id}"))?;
                ensure_valid(!text.is_empty() || attachment.is_some(), "text")?;
                if let Some(attachment) = &attachment {
                    ensure(
                        attachment.reference.epoch == ledger.epoch,
                        Error::RekeyRequired,
                    )?;
                }
                let mut payload = MessagePayload {
                    channel_id: channel.into(),
                    epoch: ledger.epoch,
                    control_head: ledger.head.clone(),
                    acl_version: ledger.acl_version,
                    message_id: message_id.clone(),
                    client_created_at: now_ms(),
                    ciphertext: String::new(),
                };
                let device = self.device_hex();
                let key = message_key(&root, channel, ledger.epoch, &message_id, &device);
                let body = Zeroizing::new(canonical(&Body {
                    text: text.into(),
                    file: attachment.map(|a| a.reference),
                }));
                payload.ciphertext = seal(&key, &body, &message_aad(&me, &device, &payload));
                let outgoing = Outgoing {
                    text: text.into(),
                    payload,
                };
                self.save_job(channel, &key_name, &outgoing)?;
                outgoing
            }
        };
        let send = format!("send:{message_id}");
        let receipt = self
            .channel_post(
                channel,
                &send,
                MESSAGE,
                json(&outgoing.payload),
                "/messages",
            )
            .await?;
        let operation: Operation = self.job(channel, &send)?.ok_or(Error::IntegrityFailed)?;
        ensure(
            receipt["digest"] == hex_sha256(&unb64(&operation.cose)?).as_str()
                && receipt["seq"].as_u64().is_some_and(|s| s > 0),
            Error::IntegrityFailed,
        )?;
        // Stored: the message is the relay's now, and its journals go. The
        // relay refuses this message ID with other bytes from now on.
        for key in [key_name, send, format!("file:{message_id}")] {
            self.forget_job(channel, &key)?;
        }
        self.sync_messages(channel, ledger, false).await?;
        Ok(json!({"message_id": message_id, "receipt": receipt}))
    }

    /// Share a library file's version under the current epoch as a shared
    /// edition: the version re-encrypted under a fresh key with a fresh
    /// identity ([`Client::share_edition`]), uploaded to the channel with a
    /// manifest sealed under that key, which the message carries. The
    /// edition shares neither its key nor a chunk byte with the library's
    /// own copy or a backup, so no one can relate the two.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown file or version and
    /// `Error::QuotaExceeded` for a version over 100 MiB.
    pub async fn channel_send_file(
        &mut self,
        channel: &str,
        file: &str,
        version: Option<&str>,
        text: &str,
        message_id: Option<&str>,
    ) -> Result<Json> {
        let message_id = message_id.map_or_else(random_hex, str::to_string);
        ensure_valid(is_hash(&message_id), "message_id")?;
        let (_, _, manifest) = self.library_manifest(file, version)?;
        let source = format!("{file}@{}", manifest.version);
        let ledger = self.channel_pull(channel, None, None).await?;
        self.root_ready().await?;
        let key_name = format!("file:{message_id}");
        let attachment = match self.job::<Attachment>(channel, &key_name)? {
            Some(attachment) => {
                ensure(attachment.source == source, Error::IdempotencyConflict)?;
                attachment
            }
            None => {
                ensure(ledger.status == Status::Active, Error::RekeyRequired)?;
                let edition = self.share_edition(&manifest)?;
                let (reference, upload) =
                    self.prepare_upload(channel, &ledger, &edition, self.blobs()?)?;
                let attachment = Attachment {
                    source,
                    reference,
                    upload,
                };
                self.save_job(channel, &key_name, &attachment)?;
                attachment
            }
        };
        self.channel_upload(channel, &attachment.upload, Source::Library)
            .await?;
        self.channel_send(channel, text, Some(&message_id)).await
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
        let sealed = seal(
            &key,
            &canonical(&ChannelFile {
                format: "dmsg-channel-file/1".into(),
                channel: channel.into(),
                epoch: ledger.epoch,
                file: file.public(),
            }),
            &manifest_context(channel, ledger.epoch, &upload_id, &file.id, &file.version),
        );
        let manifest = unb64(&sealed)?;
        let plan = UploadPlan {
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

    /// Read new messages, or with `backfill` every readable one again, such
    /// as after a history grant. Messages of epochs this account may not
    /// read are skipped by the relay without their ciphertext.
    ///
    /// # Errors
    /// Returns `Error::RecoveryIncomplete` when an epoch's key is missing;
    /// the cursor stays.
    pub async fn channel_sync(&mut self, channel: &str, backfill: bool) -> Result<ChannelSync> {
        let ledger = self.channel_pull(channel, None, None).await?;
        self.sync_messages(channel, ledger, backfill).await
    }

    /// [`Self::channel_sync`] from the control log as `ledger` verified it.
    async fn sync_messages(
        &mut self,
        channel: &str,
        mut ledger: Ledger,
        backfill: bool,
    ) -> Result<ChannelSync> {
        let me = self.me()?;
        let trust = self.trust().await?;
        let mut local = self.local(channel)?;
        let mut ring = self.keyring(channel)?.1;
        let mut after = if backfill { 0 } else { local.message };
        let mut received = 0;
        let mut ledgers: BTreeMap<String, Ledger> = BTreeMap::new();
        for _ in 0..10_000 {
            let page = self
                .relay_get(&format!("{}/sync?after={after}&limit=20", base(channel)?))
                .await?;
            if page["head"] != ledger.head.as_str() {
                // A control landed since the log was read: read it, then
                // the page again.
                let pulled = self.channel_pull(channel, None, None).await?;
                ensure(pulled.head != ledger.head, Error::IntegrityFailed)?;
                ledger = pulled;
                local = self.local(channel)?;
                continue;
            }
            let next = page["next_seq"].as_u64().ok_or(Error::IntegrityFailed)?;
            let latest = page["latest_seq"].as_u64().ok_or(Error::IntegrityFailed)?;
            let entries = page["entries"].as_array().ok_or(Error::IntegrityFailed)?;
            let skipped = page["skipped"].as_array().ok_or(Error::IntegrityFailed)?;
            ensure(
                next >= after && latest >= next && entries.len() <= 20 && next - after <= 20,
                Error::IntegrityFailed,
            )?;
            let mut seen = BTreeSet::new();
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
            }
            let mut rows = vec![];
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
                ensure(
                    payload.channel_id == channel
                        && row["epoch"].as_u64() == Some(payload.epoch)
                        && receipt["seq"].as_u64() == Some(seq)
                        && receipt["digest"] == hex_sha256(&unb64(cose)?).as_str()
                        && seq > after
                        && seq <= next
                        && seen.insert(seq),
                    Error::IntegrityFailed,
                )?;
                let author = verified.account.to_string();
                let historical = match ledgers.get(&payload.control_head) {
                    Some(ledger) => ledger.clone(),
                    None => {
                        let found = self.ledger_at(channel, &local, &payload.control_head)?;
                        ledgers.insert(payload.control_head.clone(), found.clone());
                        found
                    }
                };
                let role = historical.members.get(&author).map(|m| m.role);
                ensure(
                    role.is_some()
                        && historical.epoch == payload.epoch
                        && historical.acl_version == payload.acl_version
                        && (historical.kind != ChannelType::Distribution
                            || role != Some(Role::Member))
                        && readable(&ledger, &me, payload.epoch),
                    Error::Forbidden,
                )?;
                rows.push((seq, stored_at, author, verified.device, payload));
            }
            ensure(seen.len() as u64 == next - after, Error::IntegrityFailed)?;
            ensure(next > after || next == latest, Error::IntegrityFailed)?;
            let mut opened = Vec::with_capacity(rows.len());
            for (seq, stored_at, author, device, payload) in rows {
                let key_name = format!("{INBOX}{channel}:{seq:020}");
                if self.workspace.store.sealed(&key_name)?.is_some() {
                    continue;
                }
                if !ring.keys.contains_key(&payload.epoch) {
                    self.channel_open_epoch(channel, payload.epoch).await?;
                    ring = self.keyring(channel)?.1;
                }
                let root = unb64(
                    ring.keys
                        .get(&payload.epoch)
                        .ok_or(Error::RecoveryIncomplete)?,
                )?;
                let device = hex::encode(*device);
                let key = message_key(&root, channel, payload.epoch, &payload.message_id, &device);
                let plain = open(
                    &key,
                    &payload.ciphertext,
                    &message_aad(&author, &device, &payload),
                )?;
                let body: Body = decode(&plain, MAX_RECORD)?;
                ensure(body.text.len() <= MAX_TEXT, Error::IntegrityFailed)?;
                opened.push((
                    key_name,
                    Message {
                        seq,
                        account: author,
                        device,
                        message_id: payload.message_id.clone(),
                        epoch: payload.epoch,
                        created_at: payload.client_created_at,
                        stored_at,
                        text: body.text,
                        file: body.file,
                    },
                ));
            }
            received += opened.len();
            // The page's messages and the cursor past them in one write.
            let keys = self.keys()?;
            self.workspace.store.atomic(|_| {
                for (key, message) in &opened {
                    self.workspace.put_private(keys, key, message)?;
                }
                if !backfill && next != local.message {
                    local = self.local(channel)?;
                    local.message = next;
                    self.save_local(channel, &local)?;
                }
                Ok(())
            })?;
            after = next;
            if after == latest {
                return Ok(ChannelSync {
                    through: after,
                    received,
                });
            }
        }
        Err(Error::QuotaExceeded)
    }

    /// The messages this device read, after `after`, at most `limit`.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
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
        let messages: Vec<Message> = self.workspace.private_after(
            self.keys()?,
            &prefix,
            &format!("{prefix}{after:020}"),
            limit,
        )?;
        Ok(messages.iter().map(Message::public).collect())
    }

    /// Verify and decrypt a shared file and write it to `out`.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for a message without a file.
    pub async fn channel_download(
        &mut self,
        channel: &str,
        seq: u64,
        out: &std::path::Path,
    ) -> Result<FileManifest> {
        self.channel_pull(channel, None, None).await?;
        let message: Message = self
            .workspace
            .private(self.keys()?, &format!("{INBOX}{channel}:{seq:020}"))?
            .ok_or(Error::NotFound)?;
        let reference = message.file.clone().ok_or(Error::NotFound)?;
        let manifest = self
            .channel_file(channel, &reference, &message.account)
            .await?;
        export_manifest(&self.workspace.store, &manifest, MAX_FILE, out)
    }

    /// Save a shared file into the library, in `folder`: its chunks are
    /// fetched and checked, then the plaintext is imported like a file of
    /// this device, so the library's copy has its own key and chunks. A
    /// file of the same name in `folder` takes it as a new version.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for a message without a file or an unknown
    /// folder.
    pub async fn files_save_shared(
        &mut self,
        channel: &str,
        seq: u64,
        folder: &str,
        progress: &mut (dyn FnMut(u64, u64) + Send),
    ) -> Result<Imported> {
        self.catalog()?.live_folder(folder)?;
        self.channel_pull(channel, None, None).await?;
        let message: Message = self
            .workspace
            .private(self.keys()?, &format!("{INBOX}{channel}:{seq:020}"))?
            .ok_or(Error::NotFound)?;
        let reference = message.file.clone().ok_or(Error::NotFound)?;
        let manifest = self
            .channel_file(channel, &reference, &message.account)
            .await?;
        let mut plain = Zeroizing::new(Vec::with_capacity(manifest.size as usize));
        read_file(
            &self.workspace.store,
            &manifest,
            MAX_FILE,
            Some(&mut *plain),
        )?;
        let name = crate::library::catalog::safe_name(&manifest.name);
        let into = self.catalog()?.find(folder, &name);
        self.import_stream(
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
    }

    /// Fetch the file `reference` names, uploaded by `uploader`: its plan,
    /// sealed manifest and chunks, each checked; the chunks go to the store.
    pub(crate) async fn channel_file(
        &mut self,
        channel: &str,
        reference: &FileRef,
        uploader: &str,
    ) -> Result<FileManifest> {
        let plan = self
            .descriptor(channel, &reference.upload_id, Some(uploader))
            .await?;
        ensure(
            plan.kind == "file"
                && plan.object_id == reference.file_id
                && plan.version_id == reference.version
                && plan.epoch == reference.epoch
                && plan.manifest_digest == reference.manifest_digest,
            Error::IntegrityFailed,
        )?;
        let sealed = self
            .channel_chunk(
                channel,
                &reference.upload_id,
                "manifest",
                &reference.manifest_digest,
            )
            .await?;
        let key = key32(&unb64(&reference.key)?)?;
        let plain = open(
            &key,
            &b64(&sealed),
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
        for chunk in &file.chunks {
            if !self.workspace.store.has_chunk(&chunk.digest)? {
                let index = file
                    .chunks
                    .iter()
                    .position(|c| c.digest == chunk.digest)
                    .unwrap_or(0);
                let bytes = self
                    .channel_chunk(
                        channel,
                        &reference.upload_id,
                        &index.to_string(),
                        &chunk.digest,
                    )
                    .await?;
                self.workspace.store.put_chunk(&chunk.digest, &bytes)?;
            }
        }
        Ok(FileManifest {
            key: reference.key.clone(),
            ..file
        })
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
    /// up to 32 epochs each.
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
        ensure(
            !recipients.is_empty()
                && recipients.len() <= 6
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
        let spans = (to_epoch - from_epoch) / 32 + 1;
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

    /// A single-use ticket for the channel's hint WebSocket: its URL, valid
    /// for 30 seconds. The socket only says new messages are available;
    /// reading them goes through [`Self::channel_sync`].
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for a channel this account left.
    pub async fn channel_ticket(&mut self, channel: &str) -> Result<String> {
        let ticket = random_hex();
        let value = self
            .relay_post(
                &format!("{}/ws-ticket", base(channel)?),
                "dmsg/channel/ws-ticket/v1",
                &json!({"ticket": ticket}),
                &Hash::new(crate::crypto::random_id()),
            )
            .await?;
        ensure(
            value["ticket"] == ticket.as_str()
                && value["expires_at"].as_u64().is_some_and(|t| t > now_ms()),
            Error::Expired,
        )?;
        let mut url = url::Url::parse(&self.config.relay_origin())
            .map_err(|_| Error::IntegrityFailed)?
            .join(&format!("{}/ws", base(channel)?))
            .map_err(|_| Error::IntegrityFailed)?;
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        url.set_scheme(scheme).map_err(|_| Error::IntegrityFailed)?;
        url.query_pairs_mut().append_pair("ticket", &ticket);
        Ok(url.to_string())
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
