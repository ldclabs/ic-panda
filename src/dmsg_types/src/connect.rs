//! dmsg-connect/1: web applications reach a dMsg device through the
//! `https://connect.dmsg.net` bridge page and the relay mailbox
//! (`docs/protocol/connect-v1_zh.md`).
//!
//! Relay bodies and signed objects are canonical CBOR with binary values as
//! byte strings. Constructing or decoding a value checks nothing; use
//! `dmsg_protocol::connect` for the rules.
use crate::{AccountId, Environment, Hash};
use candid::Principal;
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};

/// Protocol identifier in frames, requests, responses and proofs.
pub const CONNECT_PROTOCOL: &str = "dmsg-connect/1";

/// A page command as the page signed it: six text keys, each value text or null.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConnectCommand {
    /// One of the eight methods.
    pub method: String,
    /// The registered application.
    #[serde(rename = "appId")]
    pub app_id: String,
    /// Nonzero 32-byte operation ID, lowercase hex.
    #[serde(rename = "operationId")]
    pub operation_id: String,
    /// The page session key: P-256 SPKI DER (91 bytes), standard base64.
    #[serde(rename = "publicKey")]
    pub public_key: String,
    /// Standard base64 payload of a creating method; null otherwise.
    pub payload: Option<String>,
    /// SHA-256 of the delivered result, lowercase hex; only for `acknowledge`.
    #[serde(rename = "resultDigest")]
    pub result_digest: Option<String>,
}

/// What a relay request asks of the devices.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum RequestKind {
    /// Create an operation for approval.
    Create,
    /// Read an operation's current state.
    Read,
    /// Show an operation again and read its state.
    Open,
}

/// A request the bridge page signs with its pairing key, sealed to the
/// account's devices that hold `FormalApprove`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConnectRequest {
    /// [`CONNECT_PROTOCOL`].
    pub protocol: String,
    /// The bridge page's deployment.
    pub environment: Environment,
    /// The paired account.
    pub account_id: AccountId,
    /// The pairing that signs the request.
    pub pairing_id: Hash,
    /// Random relay request ID.
    pub request_id: Hash,
    /// Create, read or open.
    pub kind: RequestKind,
    /// The page origin the bridge page took from `MessageEvent.origin`.
    pub origin: String,
    /// The bridge connection the page proved.
    pub connection_id: Hash,
    /// The one-time nonce the page signed.
    pub nonce: Hash,
    /// The page command.
    pub command: ConnectCommand,
    /// The page's P-256 signature (IEEE P1363) of the proof message.
    pub page_signature: ByteArray<64>,
    /// The bridge page's X25519 key that replies are sealed to.
    pub result_key: Hash,
    /// Unix milliseconds.
    pub created_at: u64,
    /// Exclusive deadline in Unix milliseconds.
    pub expires_at: u64,
}

/// One device's wrapping of the content key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Recipient {
    /// The device; visible to the relay.
    pub device_id: Hash,
    /// HPKE encapsulated key.
    pub enc: Hash,
    /// HPKE base-mode ciphertext of the content key.
    pub wrapped: ByteBuf,
}

/// The sealed form of a signed [`ConnectRequest`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    /// Always 1.
    pub v: u8,
    /// One entry per approving device.
    pub recipients: Vec<Recipient>,
    /// AES-256-GCM nonce.
    pub nonce: ByteArray<12>,
    /// AES-256-GCM ciphertext of the request's COSE_Sign1.
    pub ciphertext: ByteBuf,
}

/// A device's reply to one request; each new version replaces the last.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConnectResponse {
    /// [`CONNECT_PROTOCOL`].
    pub protocol: String,
    /// The request answered.
    pub request_id: Hash,
    /// Strictly increasing from 1 per request.
    pub version: u64,
    /// The operation's commitment.
    pub operation_digest: Hash,
    /// UTF-8 JSON of the operation value.
    pub value: ByteBuf,
}

/// HPKE output: the encapsulated key and the ciphertext.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Sealed {
    /// HPKE encapsulated key.
    pub enc: Hash,
    /// AES-256-GCM ciphertext and tag.
    pub ciphertext: ByteBuf,
}

/// The pairing link a device opens, as `base64url(CBOR)` in the URL fragment.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PairingInvite {
    /// Always 1.
    pub v: u8,
    /// The device's deployment.
    pub environment: Environment,
    /// The account to pair with.
    pub account_id: AccountId,
    /// Random invitation ID.
    pub invite_id: Hash,
    /// One-time secret; never sent to the relay.
    pub secret: Hash,
}

/// A pairing candidate the bridge page submits for an invitation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PairingCandidate {
    /// The account invited.
    pub account_id: AccountId,
    /// The invitation.
    pub invite_id: Hash,
    /// P-256 SPKI DER (91 bytes) of the pairing key.
    pub pairing_pub: ByteBuf,
    /// Browser and system name, 1 to 64 UTF-8 bytes.
    pub label: String,
    /// HMAC-SHA-256 under the invitation secret.
    pub mac: Hash,
}

/// Payload of `dmsg/connect/invite/v1`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InviteCommand {
    /// The invitation.
    pub invite_id: Hash,
    /// At most ten minutes ahead.
    pub expires_at: u64,
}

/// Payload of `dmsg/connect/pairing/grant/v1`; the signed command is the grant.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PairingGrant {
    /// The invitation the candidate answered.
    pub invite_id: Hash,
    /// Digest of the pairing key.
    pub pairing_id: Hash,
    /// P-256 SPKI DER (91 bytes).
    pub pairing_pub: ByteBuf,
    /// The label the user confirmed.
    pub label: String,
    /// The account's user home.
    pub home: Principal,
    /// Unix milliseconds.
    pub granted_at: u64,
    /// At most 180 days after `granted_at`.
    pub expires_at: u64,
}

/// Payload of `dmsg/connect/pairing/revoke/v1`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RevokePairing {
    /// The pairing.
    pub pairing_id: Hash,
}

/// A sealed request as the bridge page posts it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreateRequest {
    /// The request ID inside the envelope.
    pub request_id: Hash,
    /// As inside the envelope.
    pub kind: RequestKind,
    /// As inside the envelope.
    pub expires_at: u64,
    /// The sealed request.
    pub envelope: Envelope,
}

/// Payload of `dmsg/connect/authorize/v1`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthorizeCommand {
    /// The create request.
    pub request_id: Hash,
    /// The operation's commitment.
    pub operation_digest: Hash,
}

/// How a reply changes the relay request.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ResponseState {
    /// The request stays open; the reply replaces the last.
    Open,
    /// The request ends with this reply.
    Closed,
    /// The device has no record of the operation; no reply.
    Unknown,
}

/// Payload of `dmsg/connect/respond/v1`; the sealed reply travels beside
/// the command in the request body.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RespondCommand {
    /// The request answered.
    pub request_id: Hash,
    /// The relay's current version plus one.
    pub version: u64,
    /// Open, closed or unknown.
    pub state: ResponseState,
    /// SHA-256 of the canonical [`Sealed`]; null exactly for `Unknown`.
    pub sealed_digest: Option<Hash>,
}

/// Payload of `dmsg/hint/token/create/v1`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HintToken {
    /// SHA-256 of the 32-byte bearer token.
    pub token_hash: Hash,
    /// At most 30 days ahead.
    pub expires_at: u64,
}

/// Payload of `dmsg/hint/token/revoke/v1`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RevokeHintToken {
    /// SHA-256 of the bearer token.
    pub token_hash: Hash,
}

/// The bridge page's proof of possession, the payload of the COSE_Sign1 in
/// `X-Dmsg-Connect-Pop`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConnectPop {
    /// [`CONNECT_PROTOCOL`].
    pub protocol: String,
    /// The paired account; selects the relay mailbox.
    pub account_id: AccountId,
    /// The pairing, or the candidate's key digest before it is granted.
    pub pairing_id: Hash,
    /// Random per HTTP request.
    pub request_id: Hash,
    /// `now < deadline <= now + 60 s`.
    pub deadline: u64,
    /// The relay origin.
    pub audience: String,
    /// HTTP method.
    pub method: String,
    /// Canonical path and query.
    pub target: String,
    /// SHA-256 of the exact body.
    pub body_digest: Hash,
}
