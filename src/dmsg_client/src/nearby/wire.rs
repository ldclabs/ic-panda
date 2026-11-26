//! Wire framing, Commitment-then-Reveal X25519 handshake, 6-digit SAS derivation,
//! and AES-256-GCM encrypted session channel for `dmsg-nearby-share/1`.

use super::{FORMAT, MAX_BODY, MAX_HANDSHAKE_FRAME};
use crate::codec::{canonical, decode, digest};
use crate::crypto::{aes_gcm_open, aes_gcm_seal};
use dmsg_types::{Ed25519Signature, Error, Hash, Result, ensure};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf, Bytes};
use sha2::Sha256;
use std::future::Future;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

const COMMIT_DOMAIN: &str = "dmsg/nearby-share/commit/v1";
const HANDSHAKE_DOMAIN: &str = "dmsg/nearby-share/handshake/v1";
const QR_PROOF_DOMAIN: &str = "dmsg/nearby-share/qr-proof/v1";
const KEY_DOMAIN: &str = "dmsg/nearby-share/key/v1";
const SAS_DOMAIN: &str = "dmsg/nearby-share/sas/v1";
const FRAME_DOMAIN: &str = "dmsg/nearby-share/frame/v1";
/// Domain tag signed by an optional post-confirm `Identity` frame.
pub const IDENTITY_DOMAIN: &str = "dmsg/nearby-share/id/v1";

/// AES-256-GCM authentication tag length.
const TAG: usize = 16;
/// Sender direction byte in frame nonces and AAD.
pub const SENDER: u8 = 0;
/// Receiver direction byte in frame nonces and AAD.
pub const RECEIVER: u8 = 1;

/// Ordered binary-frame transport shared by TCP streams, WebSocket relays and
/// Apple AWDL `NWConnection` byte bridges.
pub trait FrameTransport: Send {
    /// Send one complete frame payload.
    fn send_frame(&mut self, bytes: &[u8]) -> impl Future<Output = Result<()>> + Send;

    /// Receive the next complete frame payload, enforcing `max` bytes.
    fn recv_frame(&mut self, max: usize) -> impl Future<Output = Result<Vec<u8>>> + Send;
}

/// Length-prefixed (`u32` big-endian) frame transport over any `AsyncRead + AsyncWrite`.
pub struct StreamTransport<S> {
    stream: S,
}

impl<S> StreamTransport<S> {
    /// Wrap a byte stream (such as a `TcpStream` or `DuplexStream`).
    pub fn new(stream: S) -> Self {
        Self { stream }
    }
}

fn io_err(e: std::io::Error) -> Error {
    match e.kind() {
        std::io::ErrorKind::UnexpectedEof => Error::Unavailable("nearby: connection closed".into()),
        _ => Error::Unavailable(format!("nearby: {e}")),
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin + Send> FrameTransport for StreamTransport<S> {
    async fn send_frame(&mut self, bytes: &[u8]) -> Result<()> {
        let len = u32::try_from(bytes.len()).map_err(|_| Error::QuotaExceeded)?;
        self.stream
            .write_all(&len.to_be_bytes())
            .await
            .map_err(io_err)?;
        self.stream.write_all(bytes).await.map_err(io_err)?;
        self.stream.flush().await.map_err(io_err)
    }

    async fn recv_frame(&mut self, max: usize) -> Result<Vec<u8>> {
        let mut head = [0u8; 4];
        self.stream.read_exact(&mut head).await.map_err(io_err)?;
        let len = u32::from_be_bytes(head) as usize;
        ensure(len > 0 && len <= max, Error::QuotaExceeded)?;
        let mut buf = vec![0u8; len];
        self.stream.read_exact(&mut buf).await.map_err(io_err)?;
        Ok(buf)
    }
}

/// An ephemeral X25519 keypair used for one nearby session or receive window.
pub struct EphemeralKey {
    secret: StaticSecret,
    public: [u8; 32],
}

impl EphemeralKey {
    /// Generate a fresh random X25519 keypair from the OS CSPRNG.
    pub fn generate() -> Self {
        let mut bytes = Zeroizing::new([0u8; 32]);
        getrandom::fill(bytes.as_mut()).expect("operating system random source");
        Self::from_seed(*bytes)
    }

    /// Construct from a 32-byte seed (deterministic for test vectors).
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let secret = StaticSecret::from(seed);
        let public = PublicKey::from(&secret).to_bytes();
        Self { secret, public }
    }

    /// Raw 32-byte X25519 public key.
    pub fn public_bytes(&self) -> [u8; 32] {
        self.public
    }

    /// Compute X25519 Diffie-Hellman shared secret, rejecting non-contributory all-zero outputs.
    pub fn diffie_hellman(&self, peer_public: &[u8; 32]) -> Result<Zeroizing<[u8; 32]>> {
        let shared = self.secret.diffie_hellman(&PublicKey::from(*peer_public));
        ensure(shared.was_contributory(), Error::IntegrityFailed)?;
        Ok(Zeroizing::new(shared.to_bytes()))
    }
}

/// Step 1 (Sender -> Receiver, plaintext): commitment before seeing receiver's key/nonce.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClientHello {
    /// Protocol format version (`1`).
    pub v: u8,
    /// `SHA-256(CBOR(["dmsg/nearby-share/commit/v1", s_pk, s_nonce]))`.
    pub commit: Hash,
}

/// Step 2 (Receiver -> Sender, plaintext): receiver's session ID, ephemeral key and nonce.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ServerHello {
    /// Protocol format version (`1`).
    pub v: u8,
    /// Receiver-generated 16-byte session identifier.
    pub sid: ByteArray<16>,
    /// Receiver's ephemeral X25519 public key (`R_pk`).
    pub r_pk: ByteArray<32>,
    /// Receiver's 16-byte random nonce.
    pub r_nonce: ByteArray<16>,
}

/// Step 3 (Sender -> Receiver, plaintext): reveal committed key/nonce and optional QR proof.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClientReveal {
    /// Sender's ephemeral X25519 public key (`S_pk`).
    pub s_pk: ByteArray<32>,
    /// Sender's 16-byte random nonce.
    pub s_nonce: ByteArray<16>,
    /// Present when the sender holds the offer's `qrs`: proves possession bound to `base_transcript`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qr_proof: Option<Hash>,
}

/// Derived handshake output shared by both peers.
#[derive(Clone, Debug)]
pub struct HandshakeSecrets {
    /// Receiver-assigned session ID.
    pub sid: [u8; 16],
    /// Canonical handshake transcript hash (binds `v`, `sid`, `commit`, `s_pk`, `s_nonce`, `r_pk`, `r_nonce`).
    pub transcript: Hash,
    /// 32-byte AES-256-GCM session frame key.
    pub key: Zeroizing<[u8; 32]>,
    /// 6-digit zero-padded visual verification code (`000000`..=`999999`).
    pub sas: String,
    /// Whether the session proved possession of the offer's `qr_secret`.
    pub qr_bound: bool,
}

/// Compute `ClientHello.commit` over `(s_pk, s_nonce)`.
pub fn commitment(s_pk: &[u8; 32], s_nonce: &[u8; 16]) -> Hash {
    digest(COMMIT_DOMAIN, &(Bytes::new(s_pk), Bytes::new(s_nonce)))
}

/// Compute the canonical handshake transcript hash.
pub fn handshake_transcript(
    sid: &[u8; 16],
    commit: &Hash,
    s_pk: &[u8; 32],
    s_nonce: &[u8; 16],
    r_pk: &[u8; 32],
    r_nonce: &[u8; 16],
) -> Hash {
    digest(
        HANDSHAKE_DOMAIN,
        &(
            FORMAT,
            Bytes::new(sid),
            commit,
            Bytes::new(s_pk),
            Bytes::new(s_nonce),
            Bytes::new(r_pk),
            Bytes::new(r_nonce),
        ),
    )
}

/// Compute the QR possession proof bound to `qrs` and `transcript`.
pub fn qr_proof(qrs: &[u8; 16], transcript: &Hash) -> Hash {
    digest(QR_PROOF_DOMAIN, &(Bytes::new(qrs), transcript))
}

/// Derive session key `K_tx` and 6-digit SAS from the X25519 shared secret and transcript.
pub fn derive_session(
    sid: [u8; 16],
    shared: &[u8; 32],
    transcript: Hash,
    qrs: Option<&[u8; 16]>,
) -> HandshakeSecrets {
    let hk = Hkdf::<Sha256>::new(Some(&[0u8; 32]), shared);
    let qr_bound = qrs.is_some();
    let qrs_slice: &[u8] = qrs.map_or(&[], |b| b.as_slice());

    let mut key = Zeroizing::new([0u8; 32]);
    let key_info = canonical(&(KEY_DOMAIN, &transcript, qr_bound, Bytes::new(qrs_slice)));
    hk.expand(&key_info, key.as_mut())
        .expect("32 bytes is valid HKDF-SHA-256 length");

    let mut sas_raw = [0u8; 4];
    let sas_info = canonical(&(SAS_DOMAIN, &transcript));
    hk.expand(&sas_info, &mut sas_raw)
        .expect("4 bytes is valid HKDF-SHA-256 length");
    let sas_num = u32::from_be_bytes(sas_raw) % 1_000_000;
    let sas = format!("{sas_num:06}");

    HandshakeSecrets {
        sid,
        transcript,
        key,
        sas,
        qr_bound,
    }
}

/// Encode a handshake message into canonical CBOR and send it.
pub async fn send_plain<T: FrameTransport, M: Serialize>(transport: &mut T, msg: &M) -> Result<()> {
    let bytes = canonical(msg);
    ensure(bytes.len() <= MAX_HANDSHAKE_FRAME, Error::QuotaExceeded)?;
    transport.send_frame(&bytes).await
}

/// Receive and decode a plaintext handshake message.
pub async fn recv_plain<T: FrameTransport, M: for<'de> Deserialize<'de> + Serialize>(
    transport: &mut T,
) -> Result<M> {
    let bytes = transport.recv_frame(MAX_HANDSHAKE_FRAME).await?;
    decode(&bytes, MAX_HANDSHAKE_FRAME)
}

/// Kind of nearby share payload.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OfferKind {
    /// One or more files from Files.
    Files,
    /// One `dmsg-secret-package/1` Vault snapshot package.
    SecretPackage,
}

/// File entry advertised in `OfferMeta` after SAS confirmation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileSummary {
    /// Zero-based index within the offer.
    pub index: u32,
    /// Untrusted display filename (sanitized via `catalog::safe_name` on receipt).
    pub name: String,
    /// Declared MIME type.
    pub mime: String,
    /// Exact plaintext length in bytes.
    pub size: u64,
}

/// Encrypted payload frame body (`dmsg-nearby-share/1`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Body {
    /// Post-SAS metadata describing what is offered (never sent before confirmation in browse mode).
    OfferMeta {
        /// Payload kind.
        kind: OfferKind,
        /// Offered files when `kind == OfferKind::Files`.
        files: Vec<FileSummary>,
        /// Total plaintext byte length of a `dmsg-secret-package/1` when `kind == OfferKind::SecretPackage`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        package_size: Option<u64>,
        /// Optional sender display alias.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alias: Option<String>,
    },
    /// Optional post-confirmation identity proof (off by default).
    Identity {
        /// Deployment environment.
        environment: String,
        /// Account ID text (or standalone ID).
        account: String,
        /// Device ID (32 bytes).
        device: Hash,
        /// Device Ed25519 verifying key (32 bytes).
        sign_pk: ByteArray<32>,
        /// Optional public handle hint.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        handle: Option<String>,
        /// Ed25519 signature over `digest(IDENTITY_DOMAIN, (transcript, role, account, device))`.
        signature: Ed25519Signature,
    },
    /// Receiver acceptance with initial chunk credit window and selected file indices.
    Accept {
        /// Initial chunk credits granted to the sender (`W`).
        credits: u32,
        /// File indices selected by the receiver (empty for `SecretPackage`).
        selected: Vec<u32>,
    },
    /// Receiver explicit rejection.
    Reject {
        /// Stable error/reason code.
        code: String,
    },
    /// Chunk-level credit replenishment for backpressure on LAN and relay alike.
    Credit {
        /// Additional chunks the sender may transmit.
        granted: u32,
    },
    /// Start of one file stream.
    FileBegin {
        /// File index from `OfferMeta`.
        index: u32,
        /// Untrusted display filename.
        name: String,
        /// Declared MIME type.
        mime: String,
        /// Exact plaintext byte length.
        size: u64,
        /// Expected hex SHA-256 of the complete plaintext file.
        sha256: String,
    },
    /// Start of a `dmsg-secret-package/1` stream.
    PackageBegin {
        /// Exact plaintext package length (`<= MAX_FILE`).
        size: u64,
        /// Expected hex SHA-256 of the complete plaintext package.
        sha256: String,
    },
    /// One plaintext chunk (up to 1 MiB) encrypted inside the session frame.
    Chunk {
        /// File index (0 for `SecretPackage`).
        index: u32,
        /// Zero-based chunk sequence within the file or package.
        seq: u32,
        /// Plaintext bytes.
        bytes: ByteBuf,
    },
    /// End of one file stream.
    FileEnd {
        /// File index.
        index: u32,
        /// Complete plaintext SHA-256 hex.
        sha256: String,
    },
    /// Per-file durable commit acknowledgment.
    FileAck {
        /// File index.
        index: u32,
        /// Committed library file ID on receiver.
        file_id: String,
        /// Committed library version ID on receiver.
        version_id: String,
    },
    /// End of a `dmsg-secret-package/1` stream.
    PackageEnd {
        /// Complete plaintext SHA-256 hex.
        sha256: String,
    },
    /// Receiver confirmation that the package passed integrity validation and was staged.
    PackageAck {
        /// Verified SHA-256 hex digest.
        digest: String,
    },
    /// Sender finished all selected payloads cleanly.
    Done,
    /// Immediate termination by either side.
    Abort {
        /// Stable reason code.
        code: String,
    },
}

fn frame_nonce(direction: u8, sequence: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[0] = direction;
    nonce[4..].copy_from_slice(&sequence.to_be_bytes());
    nonce
}

fn frame_aad(sid: &[u8; 16], direction: u8, sequence: u64) -> Vec<u8> {
    canonical(&(FRAME_DOMAIN, Bytes::new(sid), direction, sequence))
}

/// Seal a frame body for `direction` and `sequence`.
pub fn seal_frame(
    key: &[u8; 32],
    sid: &[u8; 16],
    direction: u8,
    sequence: u64,
    body: &Body,
) -> Result<Vec<u8>> {
    let plain = Zeroizing::new(canonical(body));
    ensure(plain.len() <= MAX_BODY, Error::QuotaExceeded)?;
    Ok(aes_gcm_seal(
        key,
        &frame_nonce(direction, sequence),
        &plain,
        &frame_aad(sid, direction, sequence),
    ))
}

/// Open and decode a sealed frame body for `direction` and `sequence`.
pub fn open_frame(
    key: &[u8; 32],
    sid: &[u8; 16],
    direction: u8,
    sequence: u64,
    sealed: &[u8],
) -> Result<Body> {
    let plain = aes_gcm_open(
        key,
        &frame_nonce(direction, sequence),
        sealed,
        &frame_aad(sid, direction, sequence),
    )?;
    decode(&plain, MAX_BODY)
}

/// Authenticated encrypted frame channel over any [`FrameTransport`].
pub struct Channel<T> {
    transport: T,
    key: Zeroizing<[u8; 32]>,
    sid: [u8; 16],
    direction: u8,
    sent: u64,
    received: u64,
}

impl<T: FrameTransport> Channel<T> {
    /// Create a channel for `direction` (`SENDER` or `RECEIVER`).
    pub fn new(transport: T, secrets: &HandshakeSecrets, direction: u8) -> Self {
        Self {
            transport,
            key: secrets.key.clone(),
            sid: secrets.sid,
            direction,
            sent: 0,
            received: 0,
        }
    }

    /// Encrypt and transmit the next frame.
    pub async fn send(&mut self, body: &Body) -> Result<()> {
        let sealed = seal_frame(&self.key, &self.sid, self.direction, self.sent, body)?;
        self.sent += 1;
        self.transport.send_frame(&sealed).await
    }

    /// Receive, decrypt and authenticate the next frame within `timeout`.
    pub async fn recv(&mut self, timeout: std::time::Duration) -> Result<Body> {
        let sealed = tokio::time::timeout(timeout, self.transport.recv_frame(MAX_BODY + TAG))
            .await
            .map_err(|_| Error::Unavailable("nearby: peer timed out".into()))??;
        let body = open_frame(
            &self.key,
            &self.sid,
            1 - self.direction,
            self.received,
            &sealed,
        )?;
        self.received += 1;
        Ok(body)
    }

    /// Best-effort abort frame before dropping the transport.
    pub async fn abort(&mut self, code: &str) {
        let _ = self
            .send(&Body::Abort {
                code: code.to_string(),
            })
            .await;
    }
}
