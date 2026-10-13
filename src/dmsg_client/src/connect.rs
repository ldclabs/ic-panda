//! The device side of dmsg-connect/1 (`docs/protocol/connect-v1_zh.md`):
//! browser pairing, the relay mailbox, the checks of §6.3, the approval and
//! its compare-and-swap authorization, execution and the sealed replies.
//!
//! Locked, a device only stores the sealed envelopes it fetches. Unlocked,
//! it opens each one, checks the pairing grant, the request and page
//! signatures, the certified application registration and the payload, and
//! either refuses it at once or puts it in the approval center. Approving
//! journals the decision, authorizes the request on the relay (the request
//! is then pinned to this device), executes and replies with HPKE in auth
//! mode, so a reply proves which device wrote it. A request for an operation
//! the device already has becomes one more delivery channel of it.

use crate::approval::{self, Action, State};
use crate::attest::{statement_view, Subject};
use crate::client::Client;
use crate::cloud::verify_signed_command;
use crate::codec::{b64, canonical, decode, sha256, unb64};
use crate::crypto::{
    aes_gcm_open, hpke_open_info, hpke_seal_auth, p256_verify, random_id, random_key, HpkeEnvelope,
    Key,
};
use crate::now_ms;
use base64::{engine::general_purpose::STANDARD, Engine};
use candid::Principal;
use cbor2::Value;
use dmsg_protocol::{
    approval_message,
    authentication::{verify_authentication, verify_certified_leaf},
    connect::*,
    digest,
    integration::{validate_app, validate_authentication},
    validate_statement,
};
use dmsg_types::{
    connect::*,
    ensure,
    integration::{AppCapability, AppRegistration, AuthenticationRequest, SigningProfile},
    integration_billing::CheckoutRequest,
    user::{Capability, UserInit},
    AccountId, Approval, CertifiedBatch, Ed25519Signature, Error, Hash, Result, Statement,
    StatementContent,
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use serde_json::{json, Value as Json};
use std::collections::BTreeMap;

/// Path of the pairing page on the bridge origin.
pub const PAIR_PATH: &str = "/v1/pair";
/// Device command creating an invitation.
pub const INVITE_ACTION: &str = "dmsg/connect/invite/v1";
/// Device command granting a pairing.
pub const GRANT_ACTION: &str = "dmsg/connect/pairing/grant/v1";
/// Device command revoking a pairing.
pub const REVOKE_ACTION: &str = "dmsg/connect/pairing/revoke/v1";
/// Device command authorizing a create request.
pub const AUTHORIZE_ACTION: &str = "dmsg/connect/authorize/v1";
/// Device command writing a reply.
pub const RESPOND_ACTION: &str = "dmsg/connect/respond/v1";
/// Device command registering a hint token.
pub const HINT_CREATE_ACTION: &str = "dmsg/hint/token/create/v1";
/// Device command revoking a hint token.
pub const HINT_REVOKE_ACTION: &str = "dmsg/hint/token/revoke/v1";
/// Largest envelope a device opens.
pub const MAX_ENVELOPE: usize = 192 * 1024;
/// A hint token is replaced this long before it expires.
const HINT_RENEW_MS: u64 = 7 * dmsg_types::DAY;
const OP: &str = "connect-op:";
const INVITE: &str = "connect-invite:";
const CURSOR: &str = "connect-cursor";
const AUTH_DOMAIN: &str = "dmsg/authentication/approve/v1";

/// The protected header of a P-256 COSE_Sign1:
/// `{1: -7, 3: "application/cbor", 4: kid, 16: typ}`.
pub fn es256_header(kid: &Hash, typ: &str) -> Vec<u8> {
    canonical(&Value::Map(vec![
        (1.into(), (-7).into()),
        (3.into(), "application/cbor".into()),
        (4.into(), Value::Bytes(kid.to_vec())),
        (16.into(), typ.into()),
    ]))
}

fn sig_structure(protected: &[u8], payload: &[u8]) -> Vec<u8> {
    canonical(&(
        "Signature1",
        serde_bytes::Bytes::new(protected),
        serde_bytes::Bytes::new(&[]),
        serde_bytes::Bytes::new(payload),
    ))
}

/// A tagged COSE_Sign1 signed by a P-256 scalar (RFC 6979), as the bridge
/// page builds requests and proofs; for tests and vectors.
///
/// # Errors
/// An invalid scalar returns `Error::IntegrityFailed`.
#[cfg(test)]
pub fn es256_sign1(secret: &Key, kid: &Hash, typ: &str, payload: &[u8]) -> Result<Vec<u8>> {
    let protected = es256_header(kid, typ);
    let signature = crate::crypto::p256_sign(secret, &sig_structure(&protected, payload))?;
    let mut encoded = vec![0xd2];
    encoded.extend(canonical(&(
        serde_bytes::Bytes::new(&protected),
        Value::Map(vec![]),
        serde_bytes::Bytes::new(payload),
        serde_bytes::Bytes::new(&signature),
    )));
    Ok(encoded)
}

/// Verify a P-256 COSE_Sign1 with exactly the header of `kid` and `typ`
/// under `spki`, and return its payload.
///
/// # Errors
/// Anything else returns `Error::IntegrityFailed`.
pub fn es256_open(cose: &[u8], kid: &Hash, typ: &str, spki: &[u8]) -> Result<Vec<u8>> {
    ensure(cose.first() == Some(&0xd2), Error::IntegrityFailed)?;
    let Value::Array(items) = crate::codec::decode_value(&cose[1..], MAX_ENVELOPE)? else {
        return Err(Error::IntegrityFailed);
    };
    let [Value::Bytes(protected), Value::Map(unprotected), Value::Bytes(payload), Value::Bytes(signature)] =
        items.as_slice()
    else {
        return Err(Error::IntegrityFailed);
    };
    ensure(
        *protected == es256_header(kid, typ) && unprotected.is_empty(),
        Error::IntegrityFailed,
    )?;
    p256_verify(spki, &sig_structure(protected, payload), signature)?;
    Ok(payload.clone())
}

/// Seal a signed request to its recipients, as the bridge page does; for
/// tests and vectors. `cek` and `nonce` are the caller's random values.
///
/// # Errors
/// An invalid recipient key returns `Error::IntegrityFailed`.
#[cfg(test)]
pub fn seal_request(
    cose: &[u8],
    account: &AccountId,
    pairing: &Hash,
    request: &Hash,
    recipients: &[(Hash, Key)],
    cek: &Key,
    nonce: &[u8; 12],
) -> Result<Envelope> {
    let recipients = recipients
        .iter()
        .map(|(device, public)| {
            let wrapped = crate::crypto::hpke_seal_info(
                public,
                cek,
                &wrap_info(account, pairing, request, device),
            )?;
            Ok(Recipient {
                device_id: *device,
                enc: Hash::new(wrapped.enc.try_into().map_err(|_| Error::IntegrityFailed)?),
                wrapped: wrapped.ciphertext.into(),
            })
        })
        .collect::<Result<_>>()?;
    Ok(Envelope {
        v: 1,
        recipients,
        nonce: serde_bytes::ByteArray::new(*nonce),
        ciphertext: crate::crypto::aes_gcm_seal(
            cek,
            nonce,
            cose,
            &request_aad(account, pairing, request),
        )
        .into(),
    })
}

/// Open the request sealed to `device`.
///
/// # Errors
/// Returns `Error::NotFound` when the device is not a recipient and
/// `Error::IntegrityFailed` when anything does not open.
pub fn open_request(
    envelope: &Envelope,
    account: &AccountId,
    pairing: &Hash,
    request: &Hash,
    device: &Hash,
    hpke_seed: &Key,
) -> Result<Vec<u8>> {
    ensure(envelope.v == 1, Error::UnsupportedProtocol)?;
    let recipient = envelope
        .recipients
        .iter()
        .find(|r| r.device_id == *device)
        .ok_or(Error::NotFound)?;
    let cek = hpke_open_info(
        hpke_seed,
        &HpkeEnvelope {
            enc: recipient.enc.to_vec(),
            ciphertext: recipient.wrapped.to_vec(),
        },
        &wrap_info(account, pairing, request, device),
    )?;
    let cek: Key = cek
        .as_slice()
        .try_into()
        .map_err(|_| Error::IntegrityFailed)?;
    Ok(aes_gcm_open(
        &cek,
        &envelope.nonce,
        &envelope.ciphertext,
        &request_aad(account, pairing, request),
    )?
    .to_vec())
}

/// Seal a reply in HPKE auth mode from the device's HPKE seed to the bridge
/// page's result key.
///
/// # Errors
/// An invalid result key returns `Error::IntegrityFailed`.
pub fn seal_response(
    response: &ConnectResponse,
    account: &AccountId,
    pairing: &Hash,
    device: &Hash,
    hpke_seed: &Key,
    result_key: &Hash,
) -> Result<Sealed> {
    let envelope = hpke_seal_auth(
        hpke_seed,
        result_key,
        &canonical(response),
        &response_info(
            account,
            pairing,
            &response.request_id,
            response.version,
            device,
        ),
    )?;
    Ok(Sealed {
        enc: Hash::new(
            envelope
                .enc
                .try_into()
                .map_err(|_| Error::IntegrityFailed)?,
        ),
        ciphertext: envelope.ciphertext.into(),
    })
}

/// Open a reply as the bridge page does, checking it came from `device`
/// with HPKE public key `device_hpke`.
///
/// # Errors
/// Anything that does not open or does not match returns `Error::IntegrityFailed`.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub fn open_response(
    sealed: &Sealed,
    account: &AccountId,
    pairing: &Hash,
    request: &Hash,
    version: u64,
    device: &Hash,
    device_hpke: &Key,
    result_seed: &Key,
) -> Result<ConnectResponse> {
    let plain = crate::crypto::hpke_open_auth(
        result_seed,
        device_hpke,
        &HpkeEnvelope {
            enc: sealed.enc.to_vec(),
            ciphertext: sealed.ciphertext.to_vec(),
        },
        &response_info(account, pairing, request, version, device),
    )?;
    let response: ConnectResponse = decode(&plain, 640 * 1024)?;
    ensure(
        response.protocol == CONNECT_PROTOCOL
            && response.request_id == *request
            && response.version == version,
        Error::IntegrityFailed,
    )?;
    Ok(response)
}

/// A request in the relay mailbox.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    seq: u64,
    request_id: Hash,
    pairing_id: Hash,
    kind: RequestKind,
    created_at: u64,
    expires_at: u64,
    envelope: ByteBuf,
    state: String,
    version: u64,
    pinned: Option<Hash>,
    acked: Option<Hash>,
}

fn hex_hash(value: &Json) -> Result<Hash> {
    value
        .as_str()
        .and_then(|t| hex::decode(t).ok())
        .and_then(|b| b.try_into().ok())
        .map(Hash::new)
        .ok_or(Error::IntegrityFailed)
}

fn optional_hash(value: &Json) -> Result<Option<Hash>> {
    match value {
        Json::Null => Ok(None),
        other => hex_hash(other).map(Some),
    }
}

fn number(value: &Json) -> Result<u64> {
    value.as_u64().ok_or(Error::IntegrityFailed)
}

impl Entry {
    fn parse(value: &Json) -> Result<Self> {
        let kind = match value["kind"].as_str() {
            Some("create") => RequestKind::Create,
            Some("read") => RequestKind::Read,
            Some("open") => RequestKind::Open,
            _ => return Err(Error::IntegrityFailed),
        };
        Ok(Self {
            seq: number(&value["seq"])?,
            request_id: hex_hash(&value["request_id"])?,
            pairing_id: hex_hash(&value["pairing_id"])?,
            kind,
            created_at: number(&value["created_at"])?,
            expires_at: number(&value["expires_at"])?,
            envelope: unb64(value["envelope"].as_str().ok_or(Error::IntegrityFailed)?)?.into(),
            state: value["state"]
                .as_str()
                .ok_or(Error::IntegrityFailed)?
                .into(),
            version: number(&value["version"])?,
            pinned: optional_hash(&value["pinned_device"])?,
            acked: optional_hash(&value["acked_digest"])?,
        })
    }
}

/// Where an operation stands, as pages see it (connect-v1 §4.6).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpState {
    /// Waiting for the user.
    AwaitingUser,
    /// Approved and journaled; the relay authorization is being taken.
    Authorizing,
    /// Authorized on the relay and executing.
    Authorized,
    /// Executed with an unknown result; reconciling.
    ExecutionUnknown,
    /// The result is ready.
    Signed,
    /// The page confirmed the result.
    Returned,
    /// Refused by the user or the device.
    Rejected,
    /// Withdrawn by the page before authorization.
    Cancelled,
    /// Undecided past its deadline.
    Expired,
    /// The execution failed.
    Failed,
    /// Another device took it.
    Elsewhere,
}

impl OpState {
    fn name(self) -> &'static str {
        match self {
            OpState::AwaitingUser => "awaiting_user",
            OpState::Authorizing | OpState::Authorized => "authorized",
            OpState::ExecutionUnknown => "execution_unknown",
            OpState::Signed => "signed",
            OpState::Returned => "returned",
            OpState::Rejected => "rejected",
            OpState::Cancelled => "cancelled",
            OpState::Expired => "expired",
            OpState::Failed => "failed",
            OpState::Elsewhere => "elsewhere",
        }
    }

    fn terminal(self) -> bool {
        !matches!(
            self,
            OpState::AwaitingUser
                | OpState::Authorizing
                | OpState::Authorized
                | OpState::ExecutionUnknown
        )
    }
}

/// One bridge page waiting for an operation's replies.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Channel {
    request_id: Hash,
    pairing_id: Hash,
    result_key: Hash,
    /// The relay's version of this request, as last seen or written.
    version: u64,
    /// The state the last reply carried.
    sent: Option<String>,
    closed: bool,
}

/// The signed approval of an authentication, journaled before it is sent.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct AuthJob {
    request: AuthenticationRequest,
    approval: Approval,
}

/// A web operation, sealed under the LDK.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Operation {
    key: String,
    origin: String,
    command: ConnectCommand,
    digest: Hash,
    kind: String,
    label: String,
    app_version: u64,
    approval: String,
    channels: Vec<Channel>,
    state: OpState,
    pinned: Option<Hash>,
    execution: Option<String>,
    auth: Option<AuthJob>,
    /// The checkout journal an approved checkout runs.
    checkout: Option<String>,
    result: Option<String>,
    result_digest: Option<Hash>,
    error: Option<String>,
    requested_at: u64,
    expires_at: u64,
}

impl Operation {
    fn value(&self) -> Json {
        let mut value = json!({
            "operationId": self.command.operation_id,
            "kind": self.kind,
            "state": self.state.name(),
        });
        if let (Some(result), Some(digest)) = (&self.result, &self.result_digest) {
            if matches!(self.state, OpState::Signed | OpState::Returned) {
                value["result"] = serde_json::from_str(result).unwrap_or(Json::Null);
                value["resultDigest"] = hex::encode(**digest).into();
            }
        }
        if let Some(error) = &self.error {
            value["error"] = error.clone().into();
        }
        value
    }
}

/// The key of an operation: its origin, application and operation ID.
fn operation_key(origin: &str, command: &ConnectCommand) -> String {
    hex::encode(*digest(
        "dmsg/connect-key/v1",
        &(origin, &command.app_id, &command.operation_id),
    ))
}

/// A signature request as `signDocument` carries it (UTF-8 JSON).
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SignatureRequest {
    protocol: String,
    method: String,
    request_id: String,
    account_id: String,
    nonce: String,
    expires_at: String,
    statement: RequestStatement,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RequestStatement {
    issuer: String,
    subject: Option<String>,
    issued_at: Option<String>,
    content: RequestContent,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RequestContent {
    Text {
        text: String,
    },
    Digest {
        sha256: String,
        #[serde(rename = "contentType")]
        content_type: Option<String>,
        location: Option<String>,
    },
    FileStatement {
        text: String,
        sha256: String,
        #[serde(rename = "contentType")]
        content_type: Option<String>,
        location: Option<String>,
    },
}

/// A failure that refuses a request with a page-visible code.
struct Refusal(&'static str);

fn refusal(error: &Error) -> Refusal {
    Refusal(match error {
        Error::Forbidden => "FORBIDDEN",
        Error::Expired => "EXPIRED",
        Error::PolicyStale => "POLICY_STALE",
        Error::Locked => "LOCKED",
        Error::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
        Error::QuotaExceeded => "QUOTA_EXCEEDED",
        Error::UnsupportedProtocol => "UNSUPPORTED_PROTOCOL",
        Error::NotFound => "NOT_FOUND",
        Error::InvalidInput(_) => "INVALID_INPUT",
        _ => "INTEGRITY_FAILED",
    })
}

/// A verified pairing.
#[derive(Clone, Debug)]
struct Pairing {
    public: Vec<u8>,
    label: String,
}

/// Certified data one pass over the mailbox checks requests against.
struct Checks {
    pairings: BTreeMap<Hash, Pairing>,
    commerce: Option<Principal>,
    apps: BTreeMap<String, AppRegistration>,
}

/// An invitation this device made, sealed under the LDK.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct Invitation {
    secret: Hash,
    expires_at: u64,
}

/// A hint token: the bearer secret and when it expires.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HintToken {
    /// The 32-byte bearer token, base64url.
    pub token: String,
    /// Unix milliseconds.
    pub expires_at: u64,
}

/// What one mailbox pass did.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectReport {
    /// Requests fetched from the relay.
    pub fetched: u64,
    /// New requests put in the approval center.
    pub queued: u64,
    /// Requests refused before reaching the user.
    pub refused: u64,
    /// The codes those refusals carried, in order.
    pub refusals: Vec<String>,
    /// Requests this device could not open or attribute to a valid pairing.
    pub ignored: u64,
    /// Requests left for a later pass, with the passing error that held them.
    pub deferred: Vec<String>,
    /// Replies written.
    pub replied: u64,
    /// Sealed requests waiting for an unlock.
    pub sealed: u64,
}

impl Client {
    fn op_key(key: &str) -> String {
        format!("{OP}{key}")
    }

    fn operation(&self, key: &str) -> Result<Operation> {
        self.workspace
            .private(self.keys()?, &Self::op_key(key))?
            .ok_or(Error::NotFound)
    }

    fn save_operation(&self, op: &Operation) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &Self::op_key(&op.key), op)
    }

    fn connect_path(&self, rest: &str) -> Result<String> {
        Ok(format!("{}/connect{rest}", self.base()?))
    }

    /// Start pairing a browser: an invitation on the relay and the pairing
    /// link with its one-time secret, for the system browser to open.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and relay errors.
    pub async fn connect_invite(&mut self) -> Result<Json> {
        self.keys()?;
        let account = self.account()?.clone();
        let invite_id = Hash::new(random_id());
        let secret = Hash::new(random_key());
        let expires_at = now_ms() + INVITE_TTL_MS;
        self.workspace.put_private(
            self.keys()?,
            &format!("{INVITE}{}", hex::encode(*invite_id)),
            &Invitation { secret, expires_at },
        )?;
        let path = self.connect_path("/invites")?;
        self.relay_post(
            &path,
            INVITE_ACTION,
            &InviteCommand {
                invite_id,
                expires_at,
            },
            &Hash::new(random_id()),
        )
        .await?;
        let invite = PairingInvite {
            v: 1,
            environment: self.config.environment.clone(),
            account_id: account.id,
            invite_id,
            secret,
        };
        let url = format!(
            "{}{PAIR_PATH}#{}",
            self.config.connect_url.trim_end_matches('/'),
            b64(&canonical(&invite))
        );
        Ok(json!({"invite_id": hex::encode(*invite_id), "url": url, "expires_at": expires_at}))
    }

    /// The candidates submitted for an invitation whose MAC proves they come
    /// from the page that opened its link.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown or expired invitation.
    pub async fn connect_candidates(&mut self, invite: &str) -> Result<Json> {
        let (invite_id, candidates, rejected) = self.verified_candidates(invite).await?;
        Ok(json!({
            "invite_id": hex::encode(*invite_id),
            "candidates": candidates.iter().map(|c| json!({
                "pairing_id": hex::encode(*pairing_id(&c.pairing_pub)),
                "label": c.label,
            })).collect::<Vec<_>>(),
            "rejected": rejected,
        }))
    }

    async fn verified_candidates(
        &mut self,
        invite: &str,
    ) -> Result<(Hash, Vec<PairingCandidate>, u64)> {
        let invite_id =
            hex_hash(&Json::String(invite.into())).map_err(|_| dmsg_types::invalid("invite"))?;
        let invitation: Invitation = self
            .workspace
            .private(self.keys()?, &format!("{INVITE}{invite}"))?
            .ok_or(Error::NotFound)?;
        ensure(now_ms() < invitation.expires_at, Error::Expired)?;
        let account = self.account()?.id;
        let path = self.connect_path(&format!("/invites/{invite}"))?;
        let data = self.relay_get(&path).await?;
        let mut valid = Vec::new();
        let mut rejected = 0;
        for c in data["candidates"]
            .as_array()
            .ok_or(Error::IntegrityFailed)?
        {
            let pairing_pub = unb64(c["pairing_pub"].as_str().unwrap_or_default())?;
            let label = c["label"].as_str().unwrap_or_default().to_string();
            let mac = hex_hash(&c["mac"])?;
            let expected = pairing_mac(
                &invitation.secret,
                &self.config.environment,
                &account,
                &invite_id,
                &pairing_pub,
                &label,
            );
            if mac == expected
                && validate_p256_spki(&pairing_pub).is_ok()
                && validate_label(&label).is_ok()
            {
                valid.push(PairingCandidate {
                    account_id: account,
                    invite_id,
                    pairing_pub: pairing_pub.into(),
                    label,
                    mac,
                });
            } else {
                rejected += 1;
            }
        }
        Ok((invite_id, valid, rejected))
    }

    /// Grant the candidate `pairing` of an invitation after the user
    /// confirmed its label. Only a device with `FormalApprove` grants.
    ///
    /// # Errors
    /// Returns `Error::NotFound` when no candidate with a valid MAC has
    /// that ID and `Error::Forbidden` for a device without `FormalApprove`.
    pub async fn connect_grant(&mut self, invite: &str, pairing: &str) -> Result<Json> {
        let (invite_id, candidates, _) = self.verified_candidates(invite).await?;
        let candidate = candidates
            .into_iter()
            .find(|c| hex::encode(*pairing_id(&c.pairing_pub)) == pairing)
            .ok_or(Error::NotFound)?;
        let account = self.account()?.clone();
        let security = self.cloud_state().await?;
        let device = security.active(&self.workspace.meta.device_id)?;
        ensure(
            device
                .input
                .capabilities
                .contains(&Capability::FormalApprove),
            Error::Forbidden,
        )?;
        let now = now_ms();
        let grant = PairingGrant {
            invite_id,
            pairing_id: pairing_id(&candidate.pairing_pub),
            pairing_pub: candidate.pairing_pub,
            label: candidate.label,
            home: account.home,
            granted_at: now,
            expires_at: now + PAIRING_TTL_MS,
        };
        validate_grant(&grant, now)?;
        let path = self.connect_path("/pairings")?;
        self.relay_post(&path, GRANT_ACTION, &grant, &Hash::new(random_id()))
            .await?;
        self.workspace
            .delete_private(&format!("{INVITE}{invite}"))?;
        Ok(json!({
            "pairing_id": hex::encode(*grant.pairing_id),
            "label": grant.label,
            "expires_at": grant.expires_at,
        }))
    }

    /// The account's browser pairings, each grant checked against the
    /// certified devices.
    ///
    /// # Errors
    /// Relay errors.
    pub async fn connect_pairings(&mut self) -> Result<Json> {
        let security = self.cloud_state().await?;
        let path = self.connect_path("/pairings")?;
        let data = self.relay_get(&path).await?;
        let now = now_ms();
        let mut list = Vec::new();
        for p in data["pairings"].as_array().ok_or(Error::IntegrityFailed)? {
            let grant = p["grant"].as_str().unwrap_or_default();
            let checked = self.check_grant(&security, grant, now);
            list.push(json!({
                "pairing_id": p["pairing_id"],
                "state": match (&checked, p["state"].as_str()) {
                    (Ok(_), Some("active")) => "active",
                    (_, Some("revoked")) => "revoked",
                    (Err(Error::Expired), _) => "expired",
                    _ => "invalid",
                },
                "label": checked.as_ref().map(|(g, _)| g.label.clone()).ok(),
                "issued_by": checked.as_ref().map(|(_, d)| hex::encode(**d)).ok(),
                "granted_at": checked.as_ref().map(|(g, _)| g.granted_at).ok(),
                "expires_at": checked.as_ref().map(|(g, _)| g.expires_at).ok(),
                "last_used_at": p["last_used_at"],
            }));
        }
        Ok(Json::Array(list))
    }

    /// Check a grant: a device command of an active device holding
    /// `FormalApprove`, for this account's home, unexpired.
    fn check_grant(
        &self,
        security: &crate::account::Security,
        grant: &str,
        now: u64,
    ) -> Result<(PairingGrant, Hash)> {
        let account = self.account()?;
        let (device, grant): (Hash, PairingGrant) = verify_signed_command(
            grant,
            &security.snapshot.issuer,
            &security.devices,
            GRANT_ACTION,
        )?;
        let signer = security.active(&device)?;
        ensure(
            signer
                .input
                .capabilities
                .contains(&Capability::FormalApprove)
                && grant.home == account.home,
            Error::IntegrityFailed,
        )?;
        validate_grant(&grant, now)?;
        Ok((grant, device))
    }

    /// Revoke a browser pairing.
    ///
    /// # Errors
    /// Relay errors; `Error::Forbidden` without `FormalApprove`.
    pub async fn connect_revoke(&mut self, pairing: &str) -> Result<Json> {
        let pairing_id =
            hex_hash(&Json::String(pairing.into())).map_err(|_| dmsg_types::invalid("pairing"))?;
        let path = self.connect_path(&format!("/pairings/{pairing}/revoke"))?;
        self.relay_post(
            &path,
            REVOKE_ACTION,
            &RevokePairing { pairing_id },
            &Hash::new(random_id()),
        )
        .await
    }

    /// Fetch the mailbox, then, unlocked, check new requests, follow the
    /// state of known operations and send pending replies. Locked with the
    /// device key resident, it only stores sealed envelopes.
    ///
    /// # Errors
    /// Returns `Error::Locked` without the device key and relay errors.
    pub async fn connect_sync(&mut self) -> Result<ConnectReport> {
        let mut report = ConnectReport {
            fetched: self.connect_fetch().await?,
            ..ConnectReport::default()
        };
        if self.keys.is_none() {
            report.sealed = self.workspace.store.inbox_entries::<Entry>(true)?.len() as u64;
            return Ok(report);
        }
        let mut checks: Option<Checks> = None;
        for entry in self.workspace.store.inbox_entries::<Entry>(true)? {
            if checks.is_none() {
                checks = Some(self.checks().await?);
            }
            let checks = checks.as_mut().expect("checks");
            match self.connect_handle(&entry, checks).await {
                Ok(Handled::Queued) => report.queued += 1,
                Ok(Handled::Refused(code)) => {
                    report.refused += 1;
                    report.refusals.push(code.into());
                }
                Ok(Handled::Replied) => report.replied += 1,
                Ok(Handled::Ignored) => report.ignored += 1,
                // Transient: the entry stays for the next pass.
                Err(
                    error @ (Error::Unavailable(_) | Error::PolicyStale | Error::ExecutionUnknown),
                ) => {
                    report.deferred.push(error.to_string());
                    continue;
                }
                Err(error) => return Err(error),
            }
            self.workspace.store.put_inbox(
                &hex::encode(*entry.request_id),
                entry.seq,
                &entry,
                true,
            )?;
        }
        report.replied += self.connect_follow().await?;
        Ok(report)
    }

    /// Store every mailbox entry changed since the cursor.
    async fn connect_fetch(&mut self) -> Result<u64> {
        let mut fetched = 0;
        loop {
            let after: u64 = self.workspace.store.meta(CURSOR)?.unwrap_or(0);
            let path = self.connect_path(&format!("/requests?after={after}&limit=100"))?;
            let data = self.relay_get(&path).await?;
            let items = data["requests"].as_array().ok_or(Error::IntegrityFailed)?;
            if items.is_empty() {
                return Ok(fetched);
            }
            let mut cursor = after;
            for item in items {
                let entry = Entry::parse(item)?;
                ensure(
                    entry.envelope.len() <= MAX_ENVELOPE && entry.seq > after,
                    Error::IntegrityFailed,
                )?;
                let id = hex::encode(*entry.request_id);
                let done = self
                    .workspace
                    .store
                    .inbox::<Entry>(&id)?
                    .is_some_and(|(_, done)| done);
                cursor = cursor.max(entry.seq);
                self.workspace
                    .store
                    .put_inbox(&id, entry.seq, &entry, done)?;
                fetched += 1;
            }
            self.workspace.store.set_meta(CURSOR, &cursor)?;
        }
    }

    /// The account's pairings, verified against fresh certified security.
    async fn checks(&mut self) -> Result<Checks> {
        let security = self.cloud_state().await?;
        let path = self.connect_path("/pairings")?;
        let data = self.relay_get(&path).await?;
        let now = now_ms();
        let mut pairings = BTreeMap::new();
        for p in data["pairings"].as_array().ok_or(Error::IntegrityFailed)? {
            if p["state"].as_str() != Some("active") {
                continue;
            }
            if let Ok((grant, _)) =
                self.check_grant(&security, p["grant"].as_str().unwrap_or_default(), now)
            {
                pairings.insert(
                    grant.pairing_id,
                    Pairing {
                        public: grant.pairing_pub.to_vec(),
                        label: grant.label,
                    },
                );
            }
        }
        Ok(Checks {
            pairings,
            commerce: None,
            apps: BTreeMap::new(),
        })
    }

    /// [`Client::app_registration`], once per pass.
    async fn registration(&mut self, checks: &mut Checks, app_id: &str) -> Result<AppRegistration> {
        if let Some(app) = checks.apps.get(app_id) {
            return Ok(app.clone());
        }
        let app = self.app_registration(&mut checks.commerce, app_id).await?;
        checks.apps.insert(app_id.into(), app.clone());
        Ok(app)
    }

    /// The certified registration of `app_id` from the home's commerce
    /// canister, fresh within 60 seconds; `commerce` keeps the canister.
    pub(crate) async fn app_registration(
        &mut self,
        commerce: &mut Option<Principal>,
        app_id: &str,
    ) -> Result<AppRegistration> {
        // Commerce is outside the login's delegation targets.
        let ic = self.anonymous().await?;
        let canister = match *commerce {
            Some(canister) => canister,
            None => {
                let init: UserInit = ic.query(self.account()?.home, "user_config", ()).await?;
                *commerce = Some(init.commerce_canister);
                init.commerce_canister
            }
        };
        let batch: Result<CertifiedBatch> = ic
            .query(
                canister,
                "integration_configuration_certificate",
                (app_id, None::<String>),
            )
            .await?;
        let key = digest("dmsg/registration/app/v1", &app_id).to_vec();
        let (value, _) = verify_certified_leaf(&batch?, &key, canister, &ic.root_key(), now_ms())?;
        let app: AppRegistration = dmsg_protocol::decode_canonical(&value)?;
        validate_app(&app)?;
        ensure(
            app.app_id == app_id && app.environment == self.config.environment,
            Error::IntegrityFailed,
        )?;
        Ok(app)
    }

    async fn connect_handle(&mut self, entry: &Entry, checks: &mut Checks) -> Result<Handled> {
        let account = self.account()?.clone();
        let device = self.workspace.meta.device_id;
        let envelope: Envelope = match decode(&entry.envelope, MAX_ENVELOPE) {
            Ok(envelope) => envelope,
            Err(_) => return Ok(Handled::Ignored),
        };
        let seed = self.keys()?.hpke.clone();
        let cose = match open_request(
            &envelope,
            &account.id,
            &entry.pairing_id,
            &entry.request_id,
            &device,
            &seed,
        ) {
            Ok(cose) => cose,
            // Not sealed to this device, or not openable: nobody to answer.
            Err(_) => return Ok(Handled::Ignored),
        };
        // An unknown, revoked or invalid pairing cannot speak for any page.
        let Some(pairing) = checks.pairings.get(&entry.pairing_id).cloned() else {
            return Ok(Handled::Ignored);
        };
        let Ok(payload) = es256_open(&cose, &entry.pairing_id, REQUEST_TYP, &pairing.public) else {
            return Ok(Handled::Ignored);
        };
        let Ok(request) = decode::<ConnectRequest>(&payload, MAX_ENVELOPE) else {
            return Ok(Handled::Ignored);
        };
        let channel = Channel {
            request_id: request.request_id,
            pairing_id: request.pairing_id,
            result_key: request.result_key,
            version: entry.version,
            sent: None,
            closed: false,
        };
        let now = now_ms();
        let checked = (|| -> Result<()> {
            ensure(
                request.account_id == account.id
                    && request.pairing_id == entry.pairing_id
                    && request.request_id == entry.request_id
                    && request.kind == entry.kind
                    && request.expires_at == entry.expires_at,
                Error::IntegrityFailed,
            )?;
            validate_request(&request, &self.config.environment, now)?;
            let message = proof_message(
                &request.nonce,
                &request.origin,
                &request.connection_id,
                &request.command,
            );
            p256_verify(
                &command_key(&request.command)?,
                message.as_slice(),
                request.page_signature.as_slice(),
            )
            .map_err(|_| Error::Forbidden)
        })();
        let key = operation_key(&request.origin, &request.command);
        if let Err(error) = checked {
            let reason = refusal(&error);
            let code = reason.0;
            return self
                .refuse(&request, channel, reason, &key)
                .await
                .map(|_| Handled::Refused(code));
        }
        match request.kind {
            RequestKind::Read | RequestKind::Open => {
                self.answer_read(&request, channel, &key).await?;
                Ok(Handled::Replied)
            }
            RequestKind::Create => {
                if let Ok(mut op) = self.operation(&key) {
                    let digest = operation_digest(&request.origin, &request.command)?;
                    if op.digest != digest {
                        self.refuse(&request, channel, Refusal("IDEMPOTENCY_CONFLICT"), &key)
                            .await?;
                        return Ok(Handled::Refused("IDEMPOTENCY_CONFLICT"));
                    }
                    // The same operation from another page: one more channel.
                    if !op
                        .channels
                        .iter()
                        .any(|c| c.request_id == channel.request_id)
                    {
                        op.channels.push(channel);
                        self.save_operation(&op)?;
                    }
                    self.deliver(&mut op).await?;
                    return Ok(Handled::Replied);
                }
                // Taken, withdrawn or over before this device saw it.
                if entry.state != "pending" {
                    return Ok(Handled::Ignored);
                }
                match self.admit(&request, &pairing, checks, now).await {
                    Ok(mut op) => {
                        let created = self.approval_create(
                            &format!("connect:{}", request.origin),
                            Action::Connect {
                                operation: op.key.clone(),
                                digest: op.digest,
                                checkout: None,
                            },
                            request.expires_at,
                        );
                        match created {
                            Ok(approval) => {
                                op.approval = approval.id;
                                self.save_operation(&op)?;
                                Ok(Handled::Queued)
                            }
                            Err(error) => {
                                // Over the limits: the operation ends here.
                                let code = refusal(&error).0;
                                op.state = OpState::Rejected;
                                op.error = Some(code.into());
                                self.save_operation(&op)?;
                                self.deliver(&mut op).await?;
                                Ok(Handled::Refused(code))
                            }
                        }
                    }
                    Err(Error::Unavailable(message)) => Err(Error::Unavailable(message)),
                    Err(error) => {
                        let reason = admission_refusal(&error);
                        let code = reason.0;
                        self.refuse(&request, channel, reason, &key).await?;
                        Ok(Handled::Refused(code))
                    }
                }
            }
        }
    }

    /// The checks of connect-v1 §4.5 and §6.3 on a create request, with the
    /// fresh certified registration; the new operation on success.
    async fn admit(
        &mut self,
        request: &ConnectRequest,
        pairing: &Pairing,
        checks: &mut Checks,
        now: u64,
    ) -> Result<Operation> {
        let command = &request.command;
        let app = match self.registration(checks, &command.app_id).await {
            Ok(app) => app,
            Err(Error::NotFound) => return Err(Error::InvalidInput("APP_NOT_REGISTERED".into())),
            Err(error) => return Err(error),
        };
        ensure(
            app.origins.contains(&request.origin),
            Error::InvalidInput("APP_NOT_REGISTERED".into()),
        )?;
        ensure(!app.paused, Error::InvalidInput("APP_PAUSED".into()))?;
        let account = self.account()?.clone();
        let payload = command_payload(command)?;
        let operation = command_operation(command)?;
        let (capability, deadline) = match command.method.as_str() {
            "authenticate" => {
                let auth: AuthenticationRequest = dmsg_protocol::decode_canonical(&payload)
                    .map_err(|_| Error::InvalidInput("INVALID_INPUT".into()))?;
                ensure(
                    auth.app_config_version == app.config_version,
                    Error::PolicyStale,
                )?;
                validate_authentication(&auth, &app, now)?;
                ensure(
                    auth.origin == request.origin
                        && auth.app_id == command.app_id
                        && auth.operation_id == operation
                        && auth.session_key_hash == sha256(&command_key(command)?),
                    Error::IntegrityFailed,
                )?;
                (AppCapability::Authenticate, auth.expires_at_ms)
            }
            "signDocument" => {
                let statement = self.document_statement(command, &payload)?;
                let profile = match statement.0.content {
                    StatementContent::Text(_) => SigningProfile::TextStatementV1,
                    StatementContent::Digest { .. } => SigningProfile::DigestStatementV1,
                    StatementContent::FileStatement { .. } => SigningProfile::FileStatementV1,
                    StatementContent::AppAction(_) => return Err(Error::UnsupportedProtocol),
                };
                ensure(app.profiles.contains(&profile), Error::Forbidden)?;
                (AppCapability::SignDocument, statement.1)
            }
            "signAction" => {
                let action: dmsg_types::app_action::AppAction =
                    dmsg_protocol::decode_canonical(&payload)
                        .map_err(|_| Error::InvalidInput("INVALID_INPUT".into()))?;
                ensure(
                    action.app_config_version == app.config_version,
                    Error::PolicyStale,
                )?;
                dmsg_protocol::app_action::validate_action_admission(&action, &app, now)?;
                ensure(
                    action.signing_account == account.id,
                    Error::InvalidInput("ACCOUNT_MISMATCH".into()),
                )?;
                ensure(
                    action.origin == request.origin
                        && action.app_id == command.app_id
                        && action.operation_id == operation,
                    Error::IntegrityFailed,
                )?;
                (AppCapability::SignAction, action.expires_at_ms)
            }
            "checkout" => {
                let checkout: CheckoutRequest = dmsg_protocol::decode_canonical(&payload)
                    .map_err(|_| Error::InvalidInput("INVALID_INPUT".into()))?;
                let offer = &checkout.offer;
                ensure(
                    offer.app_id == command.app_id && offer.operation_id == operation,
                    Error::IntegrityFailed,
                )?;
                ensure(offer.accept_by_ms > now, Error::Expired)?;
                ensure(
                    app.product_ids.contains(&offer.product_id)
                        && offer.allowed_settlement_methods.contains(&checkout.method),
                    Error::Forbidden,
                )?;
                ensure(
                    checkout.approving_account == account.id,
                    Error::InvalidInput("ACCOUNT_MISMATCH".into()),
                )?;
                (AppCapability::Checkout, offer.accept_by_ms)
            }
            _ => return Err(Error::UnsupportedProtocol),
        };
        ensure(app.capabilities.contains(&capability), Error::Forbidden)?;
        ensure(
            request.expires_at <= deadline.max(request.created_at + READ_TTL_MS),
            Error::IntegrityFailed,
        )?;
        // The account's sensitive policy, when a login can read it; the
        // home enforces it again on execution.
        if self.session().is_ok() {
            let state = self.refresh(account.home, &account.id).await?;
            let policy = &state.info.sensitive_policy;
            ensure(!policy.frozen, Error::Locked)?;
            if let Some(purpose) = match command.method.as_str() {
                "signDocument" => Some(dmsg_protocol::statement_purpose(
                    &self.document_statement(command, &payload)?.0,
                )),
                "signAction" => Some(dmsg_types::KeyPurpose::AppAction),
                _ => None,
            } {
                ensure(policy.allowed_purposes.contains(&purpose), Error::Forbidden)?;
            }
        }
        let op = Operation {
            key: operation_key(&request.origin, command),
            origin: request.origin.clone(),
            command: command.clone(),
            digest: operation_digest(&request.origin, command)?,
            kind: operation_kind(&command.method)
                .ok_or(Error::UnsupportedProtocol)?
                .into(),
            label: pairing.label.clone(),
            app_version: app.config_version,
            approval: String::new(),
            channels: vec![Channel {
                request_id: request.request_id,
                pairing_id: request.pairing_id,
                result_key: request.result_key,
                version: 0,
                sent: None,
                closed: false,
            }],
            state: OpState::AwaitingUser,
            pinned: None,
            execution: None,
            auth: None,
            checkout: None,
            result: None,
            result_digest: None,
            error: None,
            requested_at: request.created_at,
            expires_at: request.expires_at,
        };
        self.save_operation(&op)?;
        Ok(op)
    }

    /// The statement of a `signDocument` payload and its deadline.
    fn document_statement(
        &self,
        command: &ConnectCommand,
        payload: &[u8],
    ) -> Result<(Statement, u64)> {
        let request: SignatureRequest = serde_json::from_slice(payload)
            .map_err(|_| Error::InvalidInput("INVALID_INPUT".into()))?;
        let account = self.account()?;
        ensure(
            request.protocol == CONNECT_PROTOCOL
                && request.method == "signature.request"
                && request.request_id == command.operation_id
                && request.nonce.len() == 64,
            Error::IntegrityFailed,
        )?;
        ensure(
            request.account_id == account.id.to_string(),
            Error::InvalidInput("ACCOUNT_MISMATCH".into()),
        )?;
        let expires_at: u64 = request
            .expires_at
            .parse()
            .map_err(|_| Error::InvalidInput("INVALID_INPUT".into()))?;
        let digest_of = |text: &str| -> Result<Hash> {
            hex::decode(text)
                .ok()
                .and_then(|b| b.try_into().ok())
                .map(Hash::new)
                .ok_or_else(|| Error::InvalidInput("INVALID_INPUT".into()))
        };
        let content = match request.statement.content {
            RequestContent::Text { text } => StatementContent::Text(text),
            RequestContent::Digest {
                sha256,
                content_type,
                location,
            } => StatementContent::Digest {
                sha256: digest_of(&sha256)?,
                content_type,
                location,
            },
            RequestContent::FileStatement {
                text,
                sha256,
                content_type,
                location,
            } => StatementContent::FileStatement {
                text,
                sha256: digest_of(&sha256)?,
                content_type,
                location,
            },
        };
        let statement = Statement {
            issuer: request.statement.issuer,
            subject: request.statement.subject,
            issued_at: request
                .statement
                .issued_at
                .map(|t| {
                    t.parse()
                        .map_err(|_| Error::InvalidInput("INVALID_INPUT".into()))
                })
                .transpose()?,
            content,
        };
        ensure(statement.issuer == account.issuer, Error::Forbidden)?;
        validate_statement(&statement)?;
        Ok((statement, expires_at))
    }

    /// Refuse a request before it reaches the user.
    async fn refuse(
        &mut self,
        request: &ConnectRequest,
        mut channel: Channel,
        reason: Refusal,
        key: &str,
    ) -> Result<()> {
        let value = json!({
            "operationId": request.command.operation_id,
            "kind": operation_kind(&request.command.method).unwrap_or("document"),
            "state": "rejected",
            "error": reason.0,
        });
        let digest = operation_digest(&request.origin, &request.command)
            .unwrap_or_else(|_| digest("dmsg/connect-key/v1", &key));
        self.reply(&mut channel, digest, ResponseState::Closed, Some(&value))
            .await
    }

    /// Answer `read` and `open`: a snapshot when this device has the
    /// operation and no other device holds it, `unknown` otherwise.
    async fn answer_read(
        &mut self,
        request: &ConnectRequest,
        mut channel: Channel,
        key: &str,
    ) -> Result<()> {
        let device = self.workspace.meta.device_id;
        match self.operation(key) {
            Ok(op)
                if op.command.public_key == request.command.public_key
                    && op.pinned.is_none_or(|p| p == device)
                    && op.state != OpState::Elsewhere =>
            {
                if request.kind == RequestKind::Open && op.state == OpState::AwaitingUser {
                    // Shown again: refresh the request's place in the queue.
                    if let Ok(mut approval) = self.approval_record(&op.approval) {
                        approval.updated_at = now_ms();
                        self.save_approval(&approval)?;
                    }
                }
                self.reply(
                    &mut channel,
                    op.digest,
                    ResponseState::Closed,
                    Some(&op.value()),
                )
                .await
            }
            _ => {
                self.reply(
                    &mut channel,
                    Hash::new([0; 32]),
                    ResponseState::Unknown,
                    None,
                )
                .await
            }
        }
    }

    /// Write one reply to a channel, retrying once on a version conflict.
    async fn reply(
        &mut self,
        channel: &mut Channel,
        operation: Hash,
        state: ResponseState,
        value: Option<&Json>,
    ) -> Result<()> {
        let account = self.account()?.id;
        let device = self.workspace.meta.device_id;
        let seed = self.keys()?.hpke.clone();
        for attempt in 0..2 {
            let version = channel.version + 1;
            let sealed = value
                .map(|value| {
                    seal_response(
                        &ConnectResponse {
                            protocol: CONNECT_PROTOCOL.into(),
                            request_id: channel.request_id,
                            version,
                            operation_digest: operation,
                            value: serde_json::to_vec(value).expect("JSON").into(),
                        },
                        &account,
                        &channel.pairing_id,
                        &device,
                        &seed,
                        &channel.result_key,
                    )
                    .map(|s| canonical(&s))
                })
                .transpose()?;
            let path = self.connect_path(&format!(
                "/requests/{}/respond",
                hex::encode(*channel.request_id)
            ))?;
            let command = RespondCommand {
                request_id: channel.request_id,
                version,
                state,
                sealed_digest: sealed.as_ref().map(|s| sha256(s)),
            };
            match self
                .relay_post_sealed(
                    &path,
                    RESPOND_ACTION,
                    &command,
                    &Hash::new(random_id()),
                    sealed.as_deref(),
                )
                .await
            {
                Ok(_) => {
                    channel.version = version;
                    channel.sent = value.and_then(|v| v["state"].as_str().map(str::to_string));
                    channel.closed = state != ResponseState::Open;
                    return Ok(());
                }
                Err(Error::VersionConflict) if attempt == 0 => {
                    let path = self
                        .connect_path(&format!("/requests/{}", hex::encode(*channel.request_id)))?;
                    let entry = Entry::parse(&self.relay_get(&path).await?)?;
                    channel.version = entry.version;
                }
                // The relay closed the request: nobody reads this channel now.
                Err(Error::Expired | Error::NotFound | Error::Forbidden | Error::ResultExpired) => {
                    channel.closed = true;
                    return Ok(());
                }
                Err(error) => return Err(error),
            }
        }
        Err(Error::VersionConflict)
    }

    /// Reply the operation's current state on every channel that has not
    /// seen it.
    async fn deliver(&mut self, op: &mut Operation) -> Result<u64> {
        let state = if op.state.terminal() {
            ResponseState::Closed
        } else {
            ResponseState::Open
        };
        let value = op.value();
        let name = op.state.name().to_string();
        let mut written = 0;
        for i in 0..op.channels.len() {
            let mut channel = op.channels[i].clone();
            // A waiting operation has nothing new for its first page.
            if channel.closed
                || channel.sent.as_deref() == Some(name.as_str())
                || (i == 0 && op.state == OpState::AwaitingUser)
            {
                continue;
            }
            self.reply(&mut channel, op.digest, state, Some(&value))
                .await?;
            op.channels[i] = channel;
            written += 1;
            self.save_operation(op)?;
        }
        Ok(written)
    }

    /// Follow the relay state of every operation: taken by another device,
    /// cancelled, expired or acknowledged; then reconcile and deliver.
    async fn connect_follow(&mut self) -> Result<u64> {
        let device = self.workspace.meta.device_id;
        let mut written = 0;
        for key in self.workspace.store.sealed_keys(OP)? {
            let key = key.trim_start_matches(OP).to_string();
            let mut op = self.operation(&key)?;
            let Some(first) = op.channels.first() else {
                continue;
            };
            let id = hex::encode(*first.request_id);
            let Some((entry, _)) = self.workspace.store.inbox::<Entry>(&id)? else {
                continue;
            };
            let before = op.state;
            match (op.state, entry.state.as_str()) {
                (OpState::AwaitingUser | OpState::Authorizing, _)
                    if entry.pinned.is_some_and(|p| p != device) =>
                {
                    op.state = OpState::Elsewhere
                }
                (OpState::AwaitingUser, "cancelled") => op.state = OpState::Cancelled,
                (OpState::AwaitingUser, "expired") => op.state = OpState::Expired,
                // Ended without this device: another one answered it.
                (OpState::AwaitingUser, "closed" | "unknown" | "abandoned" | "device_revoked") => {
                    op.state = OpState::Elsewhere
                }
                (OpState::Signed, _)
                    if entry.acked.is_some() && entry.acked == op.result_digest =>
                {
                    op.state = OpState::Returned
                }
                _ => {}
            }
            for channel in op.channels.iter_mut().skip(1) {
                if let Some((attached, _)) = self
                    .workspace
                    .store
                    .inbox::<Entry>(&hex::encode(*channel.request_id))?
                {
                    if matches!(attached.state.as_str(), "cancelled" | "expired" | "closed") {
                        channel.closed = true;
                    }
                }
            }
            if op.state != before {
                self.save_operation(&op)?;
                let state = match op.state {
                    OpState::Elsewhere => Some(State::Elsewhere),
                    OpState::Cancelled => Some(State::Cancelled),
                    OpState::Expired => Some(State::Expired),
                    _ => None,
                };
                if let (Some(state), Ok(mut approval)) = (state, self.approval_record(&op.approval))
                {
                    if approval.state == State::AwaitingUser {
                        approval.state = state;
                        approval.updated_at = now_ms();
                        self.save_approval(&approval)?;
                    }
                }
            }
            if matches!(
                op.state,
                OpState::Authorizing | OpState::Authorized | OpState::ExecutionUnknown
            ) && op.pinned.is_none_or(|p| p == device)
            {
                match self.connect_reconcile(&key).await {
                    Ok(()) | Err(Error::ExecutionUnknown) => {}
                    Err(Error::Unavailable(_)) => continue,
                    Err(error) => return Err(error),
                }
                op = self.operation(&key)?;
            }
            if op.pinned.is_none_or(|p| p == device) && op.state != OpState::Elsewhere {
                written += self.deliver(&mut op).await?;
            }
        }
        Ok(written)
    }

    /// Mark a waiting operation expired; the relay expires its requests.
    pub(crate) fn connect_expire(&mut self, key: &str) -> Result<()> {
        let mut op = self.operation(key)?;
        if op.state == OpState::AwaitingUser {
            op.state = OpState::Expired;
            self.save_operation(&op)?;
        }
        Ok(())
    }

    /// The user refused: reply `rejected` everywhere.
    pub(crate) async fn connect_reject(&mut self, key: &str) -> Result<()> {
        let mut op = self.operation(key)?;
        ensure(op.state == OpState::AwaitingUser, Error::Expired)?;
        op.state = OpState::Rejected;
        self.save_operation(&op)?;
        self.deliver(&mut op).await.map(|_| ())
    }

    /// Approve: journal, authorize on the relay (compare-and-swap from
    /// `pending`), execute, reply. A lost authorization leaves the request
    /// `cancelled`, `expired` or with another device; nothing executes.
    pub(crate) async fn connect_approve(
        &mut self,
        key: &str,
        approval: &mut approval::Request,
    ) -> Result<()> {
        let mut op = self.operation(key)?;
        ensure(op.state == OpState::AwaitingUser, Error::Expired)?;
        // Executing needs a login; without one the request keeps waiting.
        self.session()?;
        let mut checks = self.checks().await?;
        let request = op.channels[0].clone();
        let pairing = checks
            .pairings
            .get(&request.pairing_id)
            .cloned()
            .ok_or(Error::Forbidden)?;
        let app = self.registration(&mut checks, &op.command.app_id).await?;
        ensure(
            app.config_version == op.app_version && !app.paused && app.origins.contains(&op.origin),
            Error::PolicyStale,
        )?;
        ensure(pairing.public.len() == 91, Error::IntegrityFailed)?;
        // Checks before the request is pinned here, while a refusal still
        // leaves it to the page: the quoted terms are the approved ones, and
        // the product authority accepts the action.
        let payload = command_payload(&op.command)?;
        let checked = match op.command.method.as_str() {
            "checkout" => match &approval.action {
                Action::Connect {
                    checkout: Some((id, digest)),
                    ..
                } => ensure(
                    self.plan_checkout(id)?.digest() == *digest,
                    Error::IntegrityFailed,
                ),
                _ => Err(Error::IntegrityFailed),
            },
            "signAction" => match dmsg_protocol::decode_canonical(&payload) {
                Ok(action) => self.inspect_action(&action).await,
                Err(error) => Err(error),
            },
            _ => Ok(()),
        };
        if let Err(error) = checked {
            // Nothing ran: without a login or network the request waits.
            if !matches!(
                error,
                Error::AuthRequired | Error::Unavailable(_) | Error::PolicyStale
            ) {
                op.state = OpState::Rejected;
                op.error = Some(refusal(&error).0.into());
                self.save_operation(&op)?;
                let _ = self.deliver(&mut op).await;
            }
            return Err(error);
        }
        op.state = OpState::Authorizing;
        self.save_operation(&op)?;
        let path = self.connect_path(&format!(
            "/requests/{}/authorize",
            hex::encode(*request.request_id)
        ))?;
        let authorized = self
            .relay_post(
                &path,
                AUTHORIZE_ACTION,
                &AuthorizeCommand {
                    request_id: request.request_id,
                    operation_digest: op.digest,
                },
                &Hash::new(random_id()),
            )
            .await;
        let device = self.workspace.meta.device_id;
        match authorized {
            Ok(_) => {}
            Err(error) => {
                // Read where the request stands: a lost reply may still have
                // pinned it here.
                let path =
                    self.connect_path(&format!("/requests/{}", hex::encode(*request.request_id)))?;
                let entry = match self.relay_get(&path).await {
                    Ok(value) => Entry::parse(&value)?,
                    Err(_) if error == Error::ExecutionUnknown => {
                        return Err(Error::ExecutionUnknown)
                    }
                    Err(_) => {
                        op.state = OpState::AwaitingUser;
                        self.save_operation(&op)?;
                        return Err(error);
                    }
                };
                if entry.state == "pending" {
                    // Never authorized: the request waits for the person again.
                    op.state = OpState::AwaitingUser;
                    self.save_operation(&op)?;
                    approval.state = State::AwaitingUser;
                    approval.error = Some(crate::error_code(&error));
                    return Ok(());
                }
                if !(entry.state == "authorized" && entry.pinned == Some(device)) {
                    op.state = match entry.state.as_str() {
                        "authorized" | "device_revoked" | "abandoned" => OpState::Elsewhere,
                        "cancelled" => OpState::Cancelled,
                        "expired" => OpState::Expired,
                        _ => OpState::Failed,
                    };
                    self.save_operation(&op)?;
                    approval.state = match op.state {
                        OpState::Elsewhere => State::Elsewhere,
                        OpState::Cancelled => State::Cancelled,
                        OpState::Expired => State::Expired,
                        _ => State::Failed,
                    };
                    return Ok(());
                }
            }
        }
        op.pinned = Some(device);
        op.state = OpState::Authorized;
        approval.started = true;
        self.save_operation(&op)?;
        // Every page learns it can no longer be cancelled.
        let _ = self.deliver(&mut op).await;
        self.connect_execute(key, approval).await
    }

    /// Execute an authorized operation and reply its outcome.
    async fn connect_execute(&mut self, key: &str, approval: &mut approval::Request) -> Result<()> {
        let mut op = self.operation(key)?;
        let payload = command_payload(&op.command)?;
        let outcome: Result<Json> = match op.command.method.as_str() {
            "signDocument" | "signAction" => {
                let subject = if op.command.method == "signDocument" {
                    Subject::Statement {
                        statement: self.document_statement(&op.command, &payload)?.0,
                        origin: op.origin.clone(),
                    }
                } else {
                    Subject::Action(Box::new(dmsg_protocol::decode_canonical(&payload)?))
                };
                match self.attest_prepare(&approval.id, subject).await {
                    Ok(attestation) => {
                        op.execution = Some(attestation.execution_id());
                        approval.execution = op.execution.clone();
                        self.save_operation(&op)?;
                        self.save_approval(approval)?;
                        self.attest_dispatch(attestation)
                            .await
                            .and_then(|a| attestation_result(&a))
                    }
                    Err(error) => Err(error),
                }
            }
            "authenticate" => self.authenticate(&mut op, &payload).await,
            "checkout" => match &approval.action {
                Action::Connect {
                    checkout: Some((id, _)),
                    ..
                } => {
                    op.checkout = Some(id.clone());
                    self.save_operation(&op)?;
                    self.checkout_result(id).await
                }
                _ => Err(Error::IntegrityFailed),
            },
            _ => Err(Error::UnsupportedProtocol),
        };
        self.connect_outcome(key, outcome).await
    }

    /// Run an approved checkout, or resume it, and report the order's
    /// progress; the page then asks its own authority.
    async fn checkout_result(&mut self, id: &str) -> Result<Json> {
        self.plan_execute(id).await?;
        let order = self.plan_order(id).await?;
        Ok(json!({
            "orderId": id,
            "checkoutCbor": STANDARD.encode(order.canonical()),
        }))
    }

    /// The method of an operation.
    pub(crate) fn connect_method(&self, key: &str) -> Result<String> {
        Ok(self.operation(key)?.command.method)
    }

    /// Quote the checkout of an operation waiting for approval.
    pub(crate) async fn connect_quote(
        &mut self,
        key: &str,
        selection: &str,
    ) -> Result<crate::commerce::Checkout> {
        let op = self.operation(key)?;
        ensure(
            op.command.method == "checkout" && op.state == OpState::AwaitingUser,
            Error::Forbidden,
        )?;
        let request: CheckoutRequest =
            dmsg_protocol::decode_canonical(&command_payload(&op.command)?)?;
        self.quote_request(request, &op.origin, selection).await
    }

    /// Record an execution's outcome on the operation and reply it.
    async fn connect_outcome(&mut self, key: &str, outcome: Result<Json>) -> Result<()> {
        let mut op = self.operation(key)?;
        let result = match outcome {
            Ok(result) => {
                op.result_digest = Some(sha256(&canonical(&result)));
                op.result = Some(result.to_string());
                op.state = OpState::Signed;
                Ok(())
            }
            Err(Error::ExecutionUnknown) => {
                op.state = OpState::ExecutionUnknown;
                Err(Error::ExecutionUnknown)
            }
            Err(error) => {
                op.state = OpState::Failed;
                op.error = Some(match error {
                    Error::Locked => "LOCKED".into(),
                    Error::Forbidden => "FORBIDDEN".into(),
                    Error::Expired => "EXPIRED".into(),
                    Error::QuotaExceeded => "QUOTA_EXCEEDED".into(),
                    _ => "UNAVAILABLE".into(),
                });
                Err(error)
            }
        };
        self.save_operation(&op)?;
        let _ = self.deliver(&mut op).await;
        result
    }

    /// Settle an operation pinned here whose outcome is unknown.
    pub(crate) async fn connect_reconcile(&mut self, key: &str) -> Result<()> {
        let op = self.operation(key)?;
        let device = self.workspace.meta.device_id;
        if op.state == OpState::Authorizing {
            let path = self.connect_path(&format!(
                "/requests/{}",
                hex::encode(*op.channels[0].request_id)
            ))?;
            let entry = Entry::parse(&self.relay_get(&path).await?)?;
            let mut op = op;
            if entry.state == "authorized" && entry.pinned == Some(device) {
                op.pinned = Some(device);
                op.state = OpState::Authorized;
            } else if entry.state == "pending" {
                // Never authorized: the approval stands; the person approves again.
                op.state = OpState::AwaitingUser;
            } else {
                op.state = OpState::Elsewhere;
            }
            self.save_operation(&op)?;
            return Ok(());
        }
        let outcome = match (&op.execution, &op.auth) {
            (Some(execution), _) => {
                let execution = execution.clone();
                self.attest_reconcile(&execution)
                    .await
                    .and_then(|a| attestation_result(&a))
            }
            (None, Some(job)) => {
                let job = job.clone();
                self.authentication_result(&job).await
            }
            (None, None) => match op.checkout.clone() {
                Some(id) => self.checkout_result(&id).await,
                // Authorized, never executed: the authorization is spent.
                None => Err(Error::Expired),
            },
        };
        self.connect_outcome(key, outcome).await
    }

    /// Approve an authentication with this device and read its certificate.
    async fn authenticate(&mut self, op: &mut Operation, payload: &[u8]) -> Result<Json> {
        let request: AuthenticationRequest = dmsg_protocol::decode_canonical(payload)?;
        let account = self.account()?.clone();
        let state = self.refresh(account.home, &account.id).await?;
        let device_id = self.workspace.meta.device_id;
        let device = state.active(&device_id)?;
        ensure(
            device
                .input
                .capabilities
                .contains(&Capability::FormalApprove),
            Error::Forbidden,
        )?;
        let mut approval = Approval {
            device_id,
            security_epoch: state.info.security_epoch,
            sequence: device.next_sequence,
            request_id: request.operation_id,
            expires_at: request.expires_at_ms,
            signature: Ed25519Signature::new([0; 64]),
        };
        let message = approval_message(account.home, &account.id, AUTH_DOMAIN, &request, &approval);
        approval.signature = Ed25519Signature::new(crate::crypto::sign(
            &self.keys()?.signing,
            message.as_slice(),
        ));
        let job = AuthJob { request, approval };
        op.auth = Some(job.clone());
        self.save_operation(op)?;
        let reply: Result<dmsg_types::integration::AuthenticationResult> = self
            .session()?
            .ic
            .update(
                account.home,
                "approve_authentication",
                (account.id, &job.request, &job.approval),
            )
            .await?;
        reply?;
        self.authentication_result(&job).await
    }

    async fn authentication_result(&mut self, job: &AuthJob) -> Result<Json> {
        let account = self.account()?.clone();
        let batch: Result<CertifiedBatch> = self
            .session()?
            .ic
            .query(
                account.home,
                "authentication_certificate",
                (account.id, job.request.operation_id),
            )
            .await?;
        let batch = match batch {
            Ok(batch) => batch,
            Err(Error::NotFound) if now_ms() < job.request.expires_at_ms => {
                let reply: Result<dmsg_types::integration::AuthenticationResult> = self
                    .session()?
                    .ic
                    .update(
                        account.home,
                        "approve_authentication",
                        (account.id, &job.request, &job.approval),
                    )
                    .await?;
                reply?;
                return Box::pin(self.authentication_result(job)).await;
            }
            Err(error) => return Err(error),
        };
        let root = self.session()?.ic.root_key();
        let result = verify_authentication(&batch, &job.request, account.home, &root, now_ms())?;
        ensure(result.account_id == account.id, Error::IntegrityFailed)?;
        Ok(json!({"proof": STANDARD.encode(canonical(&batch))}))
    }

    /// The review of a web request: what the bridge page relayed and what
    /// approving does.
    pub(crate) fn connect_view(&self, key: &str) -> Result<Json> {
        let op = self.operation(key)?;
        let payload = command_payload(&op.command).unwrap_or_default();
        let content = match op.command.method.as_str() {
            "signDocument" => self
                .document_statement(&op.command, &payload)
                .map(|(s, deadline)| json!({"statement": statement_view(&s), "deadline": deadline}))
                .unwrap_or(Json::Null),
            "signAction" => {
                dmsg_protocol::decode_canonical::<dmsg_types::app_action::AppAction>(&payload)
                    .map(|a| {
                        statement_view(&Statement {
                            issuer: self.account().map(|a| a.issuer.clone()).unwrap_or_default(),
                            subject: None,
                            issued_at: None,
                            content: StatementContent::AppAction(Box::new(a)),
                        })
                    })
                    .unwrap_or(Json::Null)
            }
            "checkout" => dmsg_protocol::decode_canonical::<CheckoutRequest>(&payload)
                .map(|c| {
                    json!({
                        "product_id": c.offer.product_id,
                        "sku": c.offer.sku,
                        "amount_usd_micros": c.offer.amount_usd_micros.to_string(),
                        "method": format!("{:?}", c.method),
                        "beneficiary": format!("{:?}", c.offer.beneficiary),
                        "starts_at": c.offer.starts_at_ms,
                        "ends_at": c.offer.expires_at_ms,
                        "accept_by": c.offer.accept_by_ms,
                    })
                })
                .unwrap_or(Json::Null),
            "authenticate" => dmsg_protocol::decode_canonical::<AuthenticationRequest>(&payload)
                .map(|a| {
                    json!({
                        "purpose": format!("{:?}", a.purpose),
                        "receiver": a.receiver.to_text(),
                        "expires_at": a.expires_at_ms,
                    })
                })
                .unwrap_or(Json::Null),
            _ => Json::Null,
        };
        Ok(json!({
            "origin": op.origin,
            "app_id": op.command.app_id,
            "app_version": op.app_version,
            "pairing_label": op.label,
            "account": self.account()?.id.to_string(),
            "method": op.command.method,
            "kind": op.kind,
            "operation_id": op.command.operation_id,
            "requested_at": op.requested_at,
            "expires_at": op.expires_at,
            "state": op.state.name(),
            "channels": op.channels.len(),
            // The relay requests a page's `dmsg://connect/request` link names.
            "requests": op.channels.iter().map(|c| hex::encode(*c.request_id)).collect::<Vec<_>>(),
            "relayed_by": "connect.dmsg.net",
            "content": content,
        }))
    }

    fn hint_name(&self) -> Result<String> {
        Ok(format!("{}:hint", self.workspace.name()?))
    }

    /// The device's hint token, registering a new one when none is stored
    /// or the stored one expires within seven days, and revoking the one it
    /// replaces. The token lives in the credential store, or in the
    /// workspace's plain metadata without one: it only receives
    /// content-free hints.
    ///
    /// # Errors
    /// Returns `Error::Locked` without the device key and relay errors.
    pub async fn hint_token(&mut self) -> Result<HintToken> {
        let name = self.hint_name()?;
        let stored: Option<HintToken> = match self.secure() {
            Some(secure) => secure
                .get(&name)?
                .and_then(|bytes| serde_json::from_slice(&bytes).ok()),
            None => self.workspace.store.meta(&name)?,
        };
        let now = now_ms();
        if let Some(token) = &stored {
            if token.expires_at > now + HINT_RENEW_MS {
                return Ok(token.clone());
            }
        }
        let secret = random_key();
        let token = HintToken {
            token: b64(&secret),
            expires_at: now + HINT_TTL_MS,
        };
        let path = format!("{}/hint-tokens", self.base()?);
        self.relay_post(
            &path,
            HINT_CREATE_ACTION,
            &dmsg_types::connect::HintToken {
                token_hash: sha256(&secret),
                expires_at: token.expires_at,
            },
            &Hash::new(random_id()),
        )
        .await?;
        match self.secure() {
            Some(secure) => secure.set(&name, &serde_json::to_vec(&token).expect("JSON"))?,
            None => self.workspace.store.set_meta(&name, &token)?,
        }
        if let Some(old) = stored {
            let path = format!("{}/hint-tokens/revoke", self.base()?);
            let _ = self
                .relay_post(
                    &path,
                    HINT_REVOKE_ACTION,
                    &RevokeHintToken {
                        token_hash: sha256(&unb64(&old.token)?),
                    },
                    &Hash::new(random_id()),
                )
                .await;
        }
        Ok(token)
    }

    /// Web requests fetched but not yet opened.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn sealed_requests(&self) -> Result<u64> {
        Ok(self.workspace.store.inbox_entries::<Entry>(true)?.len() as u64)
    }

    /// The stored hint token, without contacting the relay; usable locked.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn stored_hint_token(&self) -> Result<Option<HintToken>> {
        let Ok(name) = self.hint_name() else {
            return Ok(None);
        };
        let token: Option<HintToken> = match self.secure() {
            Some(secure) => secure
                .get(&name)?
                .and_then(|bytes| serde_json::from_slice(&bytes).ok()),
            None => self.workspace.store.meta(&name)?,
        };
        Ok(token.filter(|t| t.expires_at > now_ms()))
    }
}

/// How one mailbox entry ended.
enum Handled {
    Queued,
    Refused(&'static str),
    Replied,
    Ignored,
}

/// The page-visible refusal of an admission failure.
fn admission_refusal(error: &Error) -> Refusal {
    match error {
        Error::InvalidInput(code) => Refusal(match code.as_str() {
            "APP_NOT_REGISTERED" => "APP_NOT_REGISTERED",
            "APP_PAUSED" => "APP_PAUSED",
            "ACCOUNT_MISMATCH" => "ACCOUNT_MISMATCH",
            _ => "INVALID_INPUT",
        }),
        other => refusal(other),
    }
}

/// The page result of a finished attestation (browser-v4 shape).
fn attestation_result(attestation: &crate::attest::Attestation) -> Result<Json> {
    let (Some(artifact), Some(receipt)) = (&attestation.artifact, &attestation.receipt) else {
        return Err(Error::ExecutionUnknown);
    };
    Ok(json!({
        "artifact": {
            "cose_sign1": STANDARD.encode(&artifact.cose_sign1),
            "cose_key": STANDARD.encode(&artifact.cose_key),
        },
        "executionId": attestation.execution_id(),
        "executionCertificateCbor": STANDARD.encode(canonical(receipt)),
    }))
}

#[cfg(test)]
#[path = "connect_tests.rs"]
mod tests;
