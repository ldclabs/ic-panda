//! The dmsg-cloud/1 device API (`docs/protocol/cloud_zh.md`): certified
//! security evidence, device-signed commands and HTTP proof of possession
//! (COSE_Sign1), and ciphertext chunks.
//!
//! Requests carry no `Origin` and no cookies, follow no redirects and bound
//! every response. Nothing is retried here: callers reconcile mutations by
//! their request ID. A [`Presigned`] command carries its own proofs, so it
//! can be submitted, and its outcome read, without the device key until its
//! deadline.

use crate::codec::{b64, canonical, decode, decode_value, sha256, unb64};
use crate::crypto::{Key, sign};
use cbor2::Value;
use dmsg_protocol::verify;
use dmsg_types::{Error, Hash, Result, ensure, user::Device};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_bytes::{ByteBuf, Bytes};
use serde_json::Value as Json;
use std::collections::BTreeMap;
use std::time::Duration;
use url::Url;

/// Protocol identifier inside every signed payload.
pub const CLOUD_PROTOCOL: &str = "dmsg-cloud/1";
/// COSE `typ` of a device command.
pub const COMMAND_PROFILE: &str = "application/vnd.dmsg.command+cose;v=1";
/// COSE `typ` of an HTTP proof of possession.
pub const HTTP_PROFILE: &str = "application/vnd.dmsg.http-pop+cose;v=1";
/// Largest ciphertext chunk: 1 MiB of plaintext plus the AEAD framing.
pub const MAX_CHUNK: usize = 1024 * 1024 + 64;
/// Largest security evidence the relay accepts.
pub const MAX_EVIDENCE: usize = 256 * 1024;
/// A signed request is valid for at most this long.
pub const FRESHNESS_MS: u64 = 60_000;
/// Deadline the client gives its own requests.
const REQUEST_MS: u64 = 45_000;
const MAX_JSON: usize = 2 * 1024 * 1024;

/// A device signing cloud requests for one account.
pub struct Signer<'a> {
    /// The account's issuer URI.
    pub issuer: &'a str,
    /// The signing device.
    pub device_id: Hash,
    /// Security epoch of the evidence in use.
    pub security_epoch: u64,
    /// The device's Ed25519 seed.
    pub seed: &'a Key,
    /// When the evidence in use stops being fresh, in Unix milliseconds.
    pub expires_at: u64,
}

impl Signer<'_> {
    /// The deadline of a request signed at `now`, within the evidence lifetime.
    ///
    /// # Errors
    /// Returns `Error::PolicyStale` when the evidence leaves no valid deadline.
    pub fn deadline(&self, now: u64) -> Result<u64> {
        let deadline = (now + REQUEST_MS).min(self.expires_at);
        check_deadline(deadline, now)?;
        Ok(deadline)
    }
}

/// `now < deadline <= now + 60 s`.
///
/// # Errors
/// Returns `Error::PolicyStale` otherwise.
pub fn check_deadline(deadline: u64, now: u64) -> Result<()> {
    ensure(
        now < deadline && deadline <= now + FRESHNESS_MS,
        Error::PolicyStale,
    )
}

/// Certified account evidence the relay verifies before trusting a device.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Evidence {
    /// Always 1.
    pub schema: u8,
    /// The user home, as Principal text.
    pub canister: String,
    /// The IC certificate, base64url.
    pub certificate: String,
    /// One entry per account.
    pub entries: Vec<EvidenceEntry>,
}

/// One account's certified leaf.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EvidenceEntry {
    /// Account ID text.
    pub account_id: String,
    /// The certified security snapshot, base64url.
    pub value: String,
    /// The hash-tree witness, base64url.
    pub witness: String,
    /// Canonical CBOR of the complete device map, base64url.
    pub devices: String,
}

#[derive(Serialize)]
struct Command<'a, P> {
    protocol: &'a str,
    action: &'a str,
    security_epoch: u64,
    request_id: &'a str,
    deadline: u64,
    payload: &'a P,
}

#[derive(Serialize)]
struct HttpProof<'a> {
    protocol: &'a str,
    security_epoch: u64,
    request_id: &'a str,
    deadline: u64,
    audience: &'a str,
    method: &'a str,
    target: &'a str,
    body_digest: &'a str,
}

/// The protected header `{1: -19, 2: [15, 16], 3: "application/cbor",
/// 4: kid, 15: {1: issuer}, 16: typ}`.
fn protected_header(kid: &[u8], issuer: &str, typ: &str) -> Vec<u8> {
    canonical(&Value::Map(vec![
        (1.into(), (-19).into()),
        (2.into(), Value::Array(vec![15.into(), 16.into()])),
        (3.into(), "application/cbor".into()),
        (4.into(), Value::Bytes(kid.to_vec())),
        (15.into(), Value::Map(vec![(1.into(), issuer.into())])),
        (16.into(), typ.into()),
    ]))
}

pub(crate) fn sig_structure(protected: &[u8], payload: &[u8]) -> Vec<u8> {
    canonical(&(
        "Signature1",
        Bytes::new(protected),
        Bytes::new(&[]),
        Bytes::new(payload),
    ))
}

/// Tagged COSE_Sign1 (Ed25519, empty external AAD) over a canonical payload,
/// as base64url.
fn sign_profile<T: Serialize>(signer: &Signer<'_>, typ: &str, body: &T) -> String {
    let protected = protected_header(signer.device_id.as_slice(), signer.issuer, typ);
    let payload = canonical(body);
    let signature = sign(signer.seed, &sig_structure(&protected, &payload));
    let mut encoded = vec![0xd2];
    encoded.extend(canonical(&(
        Bytes::new(&protected),
        Value::Map(vec![]),
        Bytes::new(&payload),
        Bytes::new(&signature),
    )));
    b64(&encoded)
}

/// A device command `{protocol, action, security_epoch, request_id, deadline, payload}`.
pub fn sign_command<P: Serialize>(
    signer: &Signer<'_>,
    action: &str,
    payload: &P,
    request_id: &str,
    deadline: u64,
) -> String {
    sign_profile(
        signer,
        COMMAND_PROFILE,
        &Command {
            protocol: CLOUD_PROTOCOL,
            action,
            security_epoch: signer.security_epoch,
            request_id,
            deadline,
            payload,
        },
    )
}

/// A device command as signed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CommandBody<P> {
    /// Always [`CLOUD_PROTOCOL`].
    pub protocol: String,
    /// The action, such as `dmsg/channel/control/v1`.
    pub action: String,
    /// Security epoch of the evidence the device signed with.
    pub security_epoch: u64,
    /// Request ID, hex.
    pub request_id: String,
    /// Submission deadline in Unix milliseconds.
    pub deadline: u64,
    /// The action's payload.
    pub payload: P,
}

/// A tagged COSE_Sign1 device command, parsed once. The issuer and device
/// it claims are read without trusting them: the caller finds the device
/// among the issuer's certified devices, then [`Self::verify`]s it.
pub struct SignedCommand {
    protected: Vec<u8>,
    payload: Vec<u8>,
    signature: Vec<u8>,
    /// The issuer the header claims.
    pub issuer: String,
    /// The device the header claims.
    pub kid: Hash,
}

impl SignedCommand {
    /// Parse a command whose header is exactly the one its claimed signer
    /// would produce.
    ///
    /// # Errors
    /// A malformed command returns `Error::IntegrityFailed`.
    pub fn parse(cose: &str) -> Result<Self> {
        let bytes = unb64(cose)?;
        ensure(
            bytes.len() <= 262_144 && bytes.first() == Some(&0xd2),
            Error::IntegrityFailed,
        )?;
        let Value::Array(items) = decode_value(&bytes[1..], 262_144)? else {
            return Err(Error::IntegrityFailed);
        };
        let [
            Value::Bytes(protected),
            Value::Map(unprotected),
            Value::Bytes(payload),
            Value::Bytes(signature),
        ] = <[Value; 4]>::try_from(items).map_err(|_| Error::IntegrityFailed)?
        else {
            return Err(Error::IntegrityFailed);
        };
        ensure(unprotected.is_empty(), Error::IntegrityFailed)?;
        let header = decode_value(&protected, 4096)?;
        let field = |key: i64| match &header {
            Value::Map(entries) => entries
                .iter()
                .find(|(k, _)| *k == Value::from(key))
                .map(|(_, v)| v),
            _ => None,
        };
        let Some(Value::Bytes(kid)) = field(4) else {
            return Err(Error::IntegrityFailed);
        };
        let issuer = match field(15) {
            Some(Value::Map(claims)) => claims
                .iter()
                .find(|(k, _)| *k == Value::from(1))
                .and_then(|(_, v)| v.as_text())
                .ok_or(Error::IntegrityFailed)?,
            _ => return Err(Error::IntegrityFailed),
        };
        let kid = Hash::new(
            kid.as_slice()
                .try_into()
                .map_err(|_| Error::IntegrityFailed)?,
        );
        // The whole header must be the one this signer would produce.
        ensure(
            protected_header(kid.as_slice(), issuer, COMMAND_PROFILE) == protected,
            Error::IntegrityFailed,
        )?;
        let issuer = issuer.to_string();
        Ok(Self {
            protected,
            payload,
            signature,
            issuer,
            kid,
        })
    }

    /// Verify the command with its device's signing key and return the
    /// whole body; `action`, when given, must be the command's. A historical
    /// command stays valid after its deadline; only submission checks
    /// freshness.
    ///
    /// # Errors
    /// Another action or a bad signature returns `Error::IntegrityFailed`.
    pub fn verify<P: DeserializeOwned + Serialize>(
        &self,
        signing_pub: &Hash,
        action: Option<&str>,
    ) -> Result<CommandBody<P>> {
        verify(
            signing_pub,
            &sig_structure(&self.protected, &self.payload),
            &self.signature,
        )?;
        let body: CommandBody<P> = decode(&self.payload, 262_144)?;
        ensure(
            body.protocol == CLOUD_PROTOCOL && action.is_none_or(|a| body.action == a),
            Error::IntegrityFailed,
        )?;
        Ok(body)
    }
}

/// Verify a stored device command of `action` from one of the account's
/// certified devices and return its payload. A historical command stays
/// valid after its deadline; only submission checks freshness.
///
/// # Errors
/// An unknown device, another issuer or action, or a bad signature returns
/// `Error::IntegrityFailed`.
pub fn verify_command<P: DeserializeOwned + Serialize>(
    cose: &str,
    issuer: &str,
    devices: &BTreeMap<Hash, Device>,
    action: &str,
) -> Result<P> {
    verify_signed_command(cose, issuer, devices, action).map(|(_, payload)| payload)
}

/// [`verify_command`], also returning the signing device.
///
/// # Errors
/// As [`verify_command`].
pub fn verify_signed_command<P: DeserializeOwned + Serialize>(
    cose: &str,
    issuer: &str,
    devices: &BTreeMap<Hash, Device>,
    action: &str,
) -> Result<(Hash, P)> {
    let command = SignedCommand::parse(cose)?;
    ensure(command.issuer == issuer, Error::IntegrityFailed)?;
    let device = devices.get(&command.kid).ok_or(Error::IntegrityFailed)?;
    let body = command.verify(&device.input.signing_pub, Some(action))?;
    Ok((command.kid, body.payload))
}

/// The `X-Dmsg-Pop` proof over the exact request bytes.
///
/// # Errors
/// A URL with a fragment, credentials, encoded slashes or a repeated query
/// key returns `Error::InvalidInput`.
pub fn sign_http(
    signer: &Signer<'_>,
    url: &Url,
    method: &str,
    body: &[u8],
    request_id: &str,
    deadline: u64,
) -> Result<String> {
    let target = canonical_target(url)?;
    Ok(sign_profile(
        signer,
        HTTP_PROFILE,
        &HttpProof {
            protocol: CLOUD_PROTOCOL,
            security_epoch: signer.security_epoch,
            request_id,
            deadline,
            audience: &url.origin().ascii_serialization(),
            method,
            target: &target,
            body_digest: &hex::encode(*sha256(body)),
        },
    ))
}

/// Path plus sorted query, as the HTTP proof signs it.
///
/// # Errors
/// Returns `Error::InvalidInput` for a URL the relay would refuse.
pub fn canonical_target(url: &Url) -> Result<String> {
    let path = url.path();
    let lower = path.to_ascii_lowercase();
    ensure(
        url.fragment().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && !lower.contains("%2f")
            && !lower.contains("%5c")
            && !path.contains('\\'),
        dmsg_types::invalid("target"),
    )?;
    let mut pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    let mut keys: Vec<&String> = pairs.iter().map(|(k, _)| k).collect();
    keys.sort();
    keys.dedup();
    ensure(keys.len() == pairs.len(), dmsg_types::invalid("query"))?;
    pairs.sort();
    if pairs.is_empty() {
        return Ok(path.to_string());
    }
    let query = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish();
    Ok(format!("{path}?{query}"))
}

/// A device command signed ahead together with the proofs to submit it and
/// to read its operation; none of it is secret.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Presigned {
    /// The request ID, hex.
    pub request_id: String,
    /// The deadline of the command and both proofs.
    pub deadline: u64,
    /// Path of the POST.
    pub path: String,
    /// Canonical CBOR `{cose_sign1}`.
    pub body: ByteBuf,
    /// Proof of the POST.
    pub post: String,
    /// Path of the operation the request creates.
    pub status_path: String,
    /// Proof of the GET of that operation.
    pub status: String,
}

#[derive(Serialize)]
struct WireBody<'a> {
    cose_sign1: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    sealed: Option<&'a Bytes>,
}

/// Largest command body: a connect reply carries up to 512 KiB beside its command.
const MAX_BODY: usize = 640 * 1024;

/// The relay's error code mapped onto the client errors.
fn relay_error(code: &str, message: &str) -> Error {
    match code {
        "NOT_FOUND" => Error::NotFound,
        "POLICY_STALE" => Error::PolicyStale,
        "AUTH_REQUIRED" => Error::AuthRequired,
        "FORBIDDEN" => Error::Forbidden,
        "VERSION_CONFLICT" => Error::VersionConflict,
        "IDEMPOTENCY_CONFLICT" => Error::IdempotencyConflict,
        "QUOTA_EXCEEDED" => Error::QuotaExceeded,
        "INTEGRITY_FAILED" => Error::IntegrityFailed,
        "EXPIRED" => Error::Expired,
        "RESULT_EXPIRED" => Error::ResultExpired,
        "REKEY_REQUIRED" => Error::RekeyRequired,
        "RECOVERY_INCOMPLETE" => Error::RecoveryIncomplete,
        "UNSUPPORTED_PROTOCOL" => Error::UnsupportedProtocol,
        "PENDING" => Error::Pending,
        "LOCKED" => Error::Locked,
        "EXECUTION_UNKNOWN" => Error::ExecutionUnknown,
        "DEVICE_NOT_APPROVED" => Error::DeviceNotApproved,
        "FEE_BLOCKED" => Error::FeeBlocked,
        "TOO_LARGE" => Error::QuotaExceeded,
        "MEMBERSHIP_STALE" => Error::MembershipStale,
        "MEMBERSHIP_CLOSING" => Error::MembershipClosing,
        // The channel waits for a new epoch key before anything is published.
        "EPOCH_ROTATING" => Error::RekeyRequired,
        // The channel's head moved since the command was built.
        "UNVERIFIED_HEAD" => Error::VersionConflict,
        "INVALID_INPUT" => Error::InvalidInput(message.into()),
        _ => Error::Unavailable(format!("relay {code}: {message}")),
    }
}

/// A device-signed relay request, signed under the client and sent after
/// it: it holds a proof of possession until its deadline, never the key,
/// so the daemon can wait on the network without holding the client.
pub struct Prepared {
    method: reqwest::Method,
    url: Url,
    /// Canonical CBOR of a POST.
    body: Option<Vec<u8>>,
    pop: String,
}

/// A relay answer with an absent resource as `None`.
///
/// # Errors
/// Relay errors other than `NotFound`.
pub fn found(reply: Result<Json>) -> Result<Option<Json>> {
    match reply {
        Ok(value) => Ok(Some(value)),
        Err(Error::NotFound) => Ok(None),
        Err(error) => Err(error),
    }
}

/// HTTP client of one relay origin; a clone shares its connections.
#[derive(Clone)]
pub struct Cloud {
    http: reqwest::Client,
    origin: Url,
}

impl Cloud {
    /// A client of `origin`, which must be HTTPS or, locally, loopback HTTP.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for any other origin.
    pub fn new(origin: &str, local: bool) -> Result<Self> {
        let url = Url::parse(origin).map_err(|_| dmsg_types::invalid("relay"))?;
        let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
        ensure(
            url.origin().ascii_serialization() == origin.trim_end_matches('/')
                && (url.scheme() == "https" || (local && loopback && url.scheme() == "http")),
            dmsg_types::invalid("relay"),
        )?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| Error::Unavailable(format!("http: {e}")))?;
        Ok(Self { http, origin: url })
    }

    /// The relay's HTTP client, for the services at its origin: it follows
    /// no redirect and bounds every request's time.
    pub(crate) fn http(&self) -> reqwest::Client {
        self.http.clone()
    }

    fn url(&self, path: &str) -> Result<Url> {
        ensure(
            path.starts_with("/v1/") && !path.contains("//") && !path.contains('\\'),
            dmsg_types::invalid("path"),
        )?;
        let url = self
            .origin
            .join(path)
            .map_err(|_| dmsg_types::invalid("path"))?;
        ensure(url.origin() == self.origin.origin(), Error::Forbidden)?;
        canonical_target(&url)?;
        Ok(url)
    }

    async fn exchange(
        &self,
        method: reqwest::Method,
        url: Url,
        body: Option<(Vec<u8>, &str)>,
        pop: Option<String>,
        binary: bool,
    ) -> Result<Reply> {
        let mutation = method != reqwest::Method::GET;
        let mut request = self.http.request(method, url);
        if let Some((bytes, content_type)) = body {
            request = request
                .header(reqwest::header::CONTENT_TYPE, content_type)
                .body(bytes);
        }
        if let Some(pop) = pop {
            request = request.header("x-dmsg-pop", pop);
        }
        let failed = |e: reqwest::Error| {
            if mutation {
                Error::ExecutionUnknown
            } else {
                Error::Unavailable(format!("relay: {e}"))
            }
        };
        let mut response = request.send().await.map_err(failed)?;
        let status = response.status();
        let octets = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("application/octet-stream") || v.starts_with("image/"));
        let limit = if binary { MAX_CHUNK } else { MAX_JSON };
        let mut data = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(failed)? {
            ensure(data.len() + chunk.len() <= limit, Error::QuotaExceeded)?;
            data.extend_from_slice(&chunk);
        }
        if binary && status.is_success() && octets {
            return Ok(Reply::Bytes(data));
        }
        let envelope: Json = match serde_json::from_slice(&data) {
            Ok(envelope) => envelope,
            // An error page of the edge, not the relay's answer: as a lost reply.
            Err(_) if !status.is_success() => {
                return Err(match mutation {
                    true => Error::ExecutionUnknown,
                    false => Error::Unavailable(format!("relay: HTTP {status}")),
                });
            }
            Err(_) => return Err(Error::IntegrityFailed),
        };
        match envelope.get("ok").and_then(Json::as_bool) {
            Some(true) if status.is_success() => envelope
                .get("data")
                .cloned()
                .map(Reply::Json)
                .ok_or(Error::IntegrityFailed),
            Some(false) => {
                let error = &envelope["error"];
                let code = error["code"].as_str().ok_or(Error::IntegrityFailed)?;
                let message = error["message"].as_str().unwrap_or_default();
                Err(relay_error(code, message))
            }
            _ => Err(Error::IntegrityFailed),
        }
    }

    /// An admission ticket for `principal` to create an account at `home`,
    /// for a home that requires one.
    ///
    /// # Errors
    /// Relay errors as returned; a malformed ticket is `Error::IntegrityFailed`.
    pub async fn admission(
        &self,
        home: candid::Principal,
        principal: candid::Principal,
    ) -> Result<dmsg_types::user::AdmissionTicket> {
        #[derive(Serialize)]
        struct Body {
            home: candid::Principal,
            principal: candid::Principal,
        }
        let url = self.url("/v1/account-admission")?;
        let data = self
            .exchange(
                reqwest::Method::POST,
                url,
                Some((canonical(&Body { home, principal }), "application/cbor")),
                None,
                false,
            )
            .await?
            .json()?;
        let signature = crate::codec::unb64(data["signature"].as_str().unwrap_or_default())?;
        Ok(dmsg_types::user::AdmissionTicket {
            expires_at: data["expires_at"].as_u64().ok_or(Error::IntegrityFailed)?,
            signature: dmsg_types::Ed25519Signature::new(
                signature.try_into().map_err(|_| Error::IntegrityFailed)?,
            ),
        })
    }

    /// A public, unauthenticated GET under `/public/`; `None` when absent.
    ///
    /// # Errors
    /// Relay errors other than `NotFound`.
    pub async fn public(&self, path: &str) -> Result<Option<Json>> {
        ensure(
            path.starts_with("/public/") && !path.contains("//") && !path.contains('\\'),
            dmsg_types::invalid("path"),
        )?;
        let url = self
            .origin
            .join(path)
            .map_err(|_| dmsg_types::invalid("path"))?;
        ensure(url.origin() == self.origin.origin(), Error::Forbidden)?;
        match self
            .exchange(reqwest::Method::GET, url, None, None, false)
            .await
            .and_then(Reply::json)
        {
            Ok(value) => Ok(Some(value)),
            Err(Error::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// The bytes of a public, unauthenticated resource under `/public/`,
    /// such as an avatar; `None` when absent.
    ///
    /// # Errors
    /// Relay errors other than `NotFound`.
    pub async fn public_bytes(&self, path: &str) -> Result<Option<Vec<u8>>> {
        ensure(
            path.starts_with("/public/") && !path.contains("//") && !path.contains('\\'),
            dmsg_types::invalid("path"),
        )?;
        let url = self
            .origin
            .join(path)
            .map_err(|_| dmsg_types::invalid("path"))?;
        ensure(url.origin() == self.origin.origin(), Error::Forbidden)?;
        match self
            .exchange(reqwest::Method::GET, url, None, None, true)
            .await
        {
            Ok(Reply::Bytes(bytes)) => Ok(Some(bytes)),
            Ok(Reply::Json(_)) => Err(Error::IntegrityFailed),
            Err(Error::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Hand the relay certified account evidence.
    ///
    /// # Errors
    /// Relay errors as returned.
    pub async fn publish_security(&self, evidence: &Evidence) -> Result<()> {
        let body = canonical(evidence);
        ensure(body.len() <= MAX_EVIDENCE, Error::QuotaExceeded)?;
        let url = self.url("/v1/security/evidence")?;
        self.exchange(
            reqwest::Method::POST,
            url,
            Some((body, "application/cbor")),
            None,
            false,
        )
        .await
        .map(|_| ())
    }

    /// A device-authenticated GET.
    ///
    /// # Errors
    /// Relay errors as returned; `Error::NotFound` for an absent resource.
    pub async fn get(&self, signer: &Signer<'_>, path: &str, now: u64) -> Result<Json> {
        self.send(self.prepare_get(signer, path, now)?).await
    }

    /// [`Self::get`] reporting an absent resource as `None`.
    ///
    /// # Errors
    /// Relay errors other than `NotFound`.
    pub async fn find(&self, signer: &Signer<'_>, path: &str, now: u64) -> Result<Option<Json>> {
        found(self.get(signer, path, now).await)
    }

    /// [`Self::get`] signed now and sent later with [`Self::send`].
    pub(crate) fn prepare_get(
        &self,
        signer: &Signer<'_>,
        path: &str,
        now: u64,
    ) -> Result<Prepared> {
        let url = self.url(path)?;
        let request_id = hex::encode(crate::crypto::random_id());
        let deadline = signer.deadline(now)?;
        let pop = sign_http(signer, &url, "GET", &[], &request_id, deadline)?;
        Ok(Prepared {
            method: reqwest::Method::GET,
            url,
            body: None,
            pop,
        })
    }

    /// Send a request signed earlier, within its deadline.
    ///
    /// # Errors
    /// Relay errors as returned; `Error::NotFound` for an absent resource, and
    /// a lost response to a POST is `Error::ExecutionUnknown`.
    pub async fn send(&self, request: Prepared) -> Result<Json> {
        let body = request.body.map(|bytes| (bytes, "application/cbor"));
        self.exchange(request.method, request.url, body, Some(request.pop), false)
            .await?
            .json()
    }

    /// Sign and post a device command. The HTTP proof and the command share
    /// the request ID and deadline.
    ///
    /// # Errors
    /// Relay errors as returned; a lost response is `Error::ExecutionUnknown`.
    pub async fn post<P: Serialize>(
        &self,
        signer: &Signer<'_>,
        path: &str,
        action: &str,
        payload: &P,
        request_id: &Hash,
        now: u64,
    ) -> Result<Json> {
        self.post_sealed(signer, path, action, payload, request_id, None, now)
            .await
    }

    /// [`Self::post`] with a sealed reply beside the command; the command
    /// commits to its digest.
    ///
    /// # Errors
    /// Relay errors as returned; a lost response is `Error::ExecutionUnknown`.
    #[allow(clippy::too_many_arguments)]
    pub async fn post_sealed<P: Serialize>(
        &self,
        signer: &Signer<'_>,
        path: &str,
        action: &str,
        payload: &P,
        request_id: &Hash,
        sealed: Option<&[u8]>,
        now: u64,
    ) -> Result<Json> {
        let request = self.prepare_post(signer, path, action, payload, request_id, sealed, now)?;
        self.send(request).await
    }

    /// [`Self::post_sealed`] signed now and sent later with [`Self::send`].
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_post<P: Serialize>(
        &self,
        signer: &Signer<'_>,
        path: &str,
        action: &str,
        payload: &P,
        request_id: &Hash,
        sealed: Option<&[u8]>,
        now: u64,
    ) -> Result<Prepared> {
        let url = self.url(path)?;
        let request_id = hex::encode(**request_id);
        let deadline = signer.deadline(now)?;
        let command = sign_command(signer, action, payload, &request_id, deadline);
        let body = canonical(&WireBody {
            cose_sign1: &command,
            sealed: sealed.map(Bytes::new),
        });
        ensure(
            body.len() <= if sealed.is_some() { MAX_BODY } else { 262_144 },
            Error::QuotaExceeded,
        )?;
        let pop = sign_http(signer, &url, "POST", &body, &request_id, deadline)?;
        Ok(Prepared {
            method: reqwest::Method::POST,
            url,
            body: Some(body),
            pop,
        })
    }

    /// [`Self::post_signed`] as a request to send later, within the
    /// command's deadline.
    ///
    /// # Errors
    /// Returns `Error::Expired` past the deadline.
    pub(crate) fn prepare_signed(
        &self,
        signer: &Signer<'_>,
        path: &str,
        cose: &str,
        request_id: &str,
        deadline: u64,
        now: u64,
    ) -> Result<Prepared> {
        ensure(now < deadline, Error::Expired)?;
        let url = self.url(path)?;
        let proof = signer.deadline(now)?.min(deadline);
        let body = canonical(&WireBody {
            cose_sign1: cose,
            sealed: None,
        });
        ensure(body.len() <= 262_144, Error::QuotaExceeded)?;
        let pop = sign_http(signer, &url, "POST", &body, request_id, proof)?;
        Ok(Prepared {
            method: reqwest::Method::POST,
            url,
            body: Some(body),
            pop,
        })
    }

    /// Post a command signed earlier, such as one whose exact bytes another
    /// account approved, with a fresh proof of possession.
    ///
    /// # Errors
    /// Returns `Error::Expired` past the command's deadline; relay errors as
    /// returned, a lost response is `Error::ExecutionUnknown`.
    pub async fn post_signed(
        &self,
        signer: &Signer<'_>,
        path: &str,
        cose: &str,
        request_id: &str,
        deadline: u64,
        now: u64,
    ) -> Result<Json> {
        ensure(now < deadline, Error::Expired)?;
        let url = self.url(path)?;
        let proof = signer.deadline(now)?.min(deadline);
        let body = canonical(&WireBody {
            cose_sign1: cose,
            sealed: None,
        });
        ensure(body.len() <= 262_144, Error::QuotaExceeded)?;
        let pop = sign_http(signer, &url, "POST", &body, request_id, proof)?;
        self.exchange(
            reqwest::Method::POST,
            url,
            Some((body, "application/cbor")),
            Some(pop),
            false,
        )
        .await?
        .json()
    }

    /// A POST of a canonical CBOR body that is not a command, such as a
    /// payment proof, with a proof of possession.
    ///
    /// # Errors
    /// Relay errors as returned; a lost response is `Error::ExecutionUnknown`.
    pub async fn post_raw<B: Serialize>(
        &self,
        signer: &Signer<'_>,
        path: &str,
        body: &B,
        now: u64,
    ) -> Result<Json> {
        let url = self.url(path)?;
        let body = canonical(body);
        ensure(body.len() <= 262_144, Error::QuotaExceeded)?;
        let request_id = hex::encode(crate::crypto::random_id());
        let deadline = signer.deadline(now)?;
        let pop = sign_http(signer, &url, "POST", &body, &request_id, deadline)?;
        self.exchange(
            reqwest::Method::POST,
            url,
            Some((body, "application/cbor")),
            Some(pop),
            false,
        )
        .await?
        .json()
    }

    /// Sign a command for `path` and the proofs to post it and to read its
    /// operation at `status_path`, all valid until the same deadline.
    ///
    /// # Errors
    /// Returns `Error::PolicyStale` when the evidence leaves no deadline.
    #[allow(clippy::too_many_arguments)]
    pub fn presign<P: Serialize>(
        &self,
        signer: &Signer<'_>,
        path: &str,
        action: &str,
        payload: &P,
        request_id: &Hash,
        status_path: &str,
        now: u64,
    ) -> Result<Presigned> {
        let url = self.url(path)?;
        let status_url = self.url(status_path)?;
        let request_id = hex::encode(**request_id);
        let deadline = signer.deadline(now)?;
        let command = sign_command(signer, action, payload, &request_id, deadline);
        let body = canonical(&WireBody {
            cose_sign1: &command,
            sealed: None,
        });
        ensure(body.len() <= 262_144, Error::QuotaExceeded)?;
        Ok(Presigned {
            post: sign_http(signer, &url, "POST", &body, &request_id, deadline)?,
            status: sign_http(signer, &status_url, "GET", &[], &request_id, deadline)?,
            request_id,
            deadline,
            path: path.into(),
            body: ByteBuf::from(body),
            status_path: status_path.into(),
        })
    }

    /// The outcome of a presigned command: the stored operation's result,
    /// or, when the relay has none, the result of posting it now.
    ///
    /// # Errors
    /// Returns `Error::Expired` past the deadline; relay errors as returned,
    /// a lost response is `Error::ExecutionUnknown`.
    pub async fn submit(&self, presigned: &Presigned, now: u64) -> Result<Json> {
        ensure(now < presigned.deadline, Error::Expired)?;
        let status = self
            .exchange(
                reqwest::Method::GET,
                self.url(&presigned.status_path)?,
                None,
                Some(presigned.status.clone()),
                false,
            )
            .await;
        match status.and_then(Reply::json) {
            Ok(found) if found["found"] == true => return Ok(found["result"].clone()),
            Ok(_) | Err(Error::NotFound) => {}
            Err(error) => return Err(error),
        }
        self.exchange(
            reqwest::Method::POST,
            self.url(&presigned.path)?,
            Some((presigned.body.to_vec(), "application/cbor")),
            Some(presigned.post.clone()),
            false,
        )
        .await?
        .json()
    }

    /// Upload one ciphertext chunk or manifest.
    ///
    /// # Errors
    /// Relay errors as returned.
    pub async fn put_chunk(
        &self,
        signer: &Signer<'_>,
        path: &str,
        bytes: &[u8],
        now: u64,
    ) -> Result<()> {
        ensure(
            !bytes.is_empty() && bytes.len() <= MAX_CHUNK,
            Error::QuotaExceeded,
        )?;
        let url = self.url(path)?;
        let request_id = hex::encode(crate::crypto::random_id());
        let deadline = signer.deadline(now)?;
        let pop = sign_http(signer, &url, "PUT", bytes, &request_id, deadline)?;
        self.exchange(
            reqwest::Method::PUT,
            url,
            Some((bytes.to_vec(), "application/octet-stream")),
            Some(pop),
            true,
        )
        .await
        .map(|_| ())
    }

    /// Download one chunk or manifest and check its SHA-256.
    ///
    /// # Errors
    /// A digest mismatch returns `Error::IntegrityFailed`.
    pub async fn get_chunk(
        &self,
        signer: &Signer<'_>,
        path: &str,
        digest: &str,
        now: u64,
    ) -> Result<Vec<u8>> {
        self.send_chunk(self.prepare_get(signer, path, now)?, digest)
            .await
    }

    /// [`Self::get_chunk`] signed earlier with [`Self::prepare_get`], sent
    /// within its deadline.
    ///
    /// # Errors
    /// As [`Self::get_chunk`].
    pub async fn send_chunk(&self, request: Prepared, digest: &str) -> Result<Vec<u8>> {
        let Reply::Bytes(bytes) = self
            .exchange(request.method, request.url, None, Some(request.pop), true)
            .await?
        else {
            return Err(Error::IntegrityFailed);
        };
        ensure(
            hex::encode(*sha256(&bytes)) == digest,
            Error::IntegrityFailed,
        )?;
        Ok(bytes)
    }
}

enum Reply {
    Json(Json),
    Bytes(Vec<u8>),
}

impl Reply {
    fn json(self) -> Result<Json> {
        match self {
            Reply::Json(value) => Ok(value),
            Reply::Bytes(_) => Err(Error::IntegrityFailed),
        }
    }
}

#[cfg(test)]
#[path = "cloud_tests.rs"]
mod tests;
