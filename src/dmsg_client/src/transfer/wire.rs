//! The frames: a length-prefixed hello in plain text, then length-prefixed
//! AES-256-GCM frames under the session key. A frame's nonce is its
//! direction byte, three zero bytes and its sequence number; its AAD binds
//! the session, the direction and the sequence number, so a frame replayed,
//! reordered or moved to the other direction does not open.

use super::{Category, DOMAIN, Entry, SessionId};
use crate::codec::{canonical, decode};
use crate::crypto::{aes_gcm_open, aes_gcm_seal};
use dmsg_types::{AccountId, Ed25519Signature, Error, Hash, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf, Bytes};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use zeroize::Zeroizing;

/// The largest frame body: a sealed 1 MiB chunk with its framing, or a
/// manifest page.
pub const MAX_BODY: usize = 1024 * 1024 + 64 * 1024;
/// The largest hello.
const MAX_HELLO: usize = 4096;
/// AES-GCM's tag.
const TAG: usize = 16;

/// What the sender signs.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// [`super::FORMAT`].
    pub format: u8,
    /// The deployment, such as `production`.
    pub environment: String,
    /// The account both devices belong to.
    pub account: AccountId,
    /// The sending device.
    pub from: Hash,
    /// The receiving device.
    pub to: Hash,
    /// The receiver's session.
    pub session_id: ByteArray<16>,
    /// The HPKE encapsulated key.
    pub enc: ByteBuf,
    /// The session key sealed to the receiver's HPKE key.
    pub key: ByteBuf,
    /// The manifest's digest.
    pub manifest: Hash,
}

/// A transfer's first frame: the header and the sender's Ed25519 signature
/// of `digest("dmsg/device-transfer/v1", header)`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    /// The header.
    pub header: Header,
    /// The sending device's signature.
    pub signature: Ed25519Signature,
}

/// An encrypted frame's content.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Body {
    /// Part of the manifest, entries in name order; the last part says so.
    Manifest {
        /// Entries.
        entries: Vec<Entry>,
        /// The manifest ends here.
        last: bool,
    },
    /// The receiver's first frame: the manifest it already holds from an
    /// earlier connection, which the sender then does not send again.
    Have {
        /// That manifest's digest.
        manifest: Option<Hash>,
    },
    /// Chunks and records the receiver asks for, at most
    /// [`super::MAX_WANT`] in all; the replies follow in that order, the
    /// chunks first.
    Want {
        /// Record names from the manifest.
        records: Vec<String>,
        /// Chunk digests the manifest references.
        blobs: Vec<Hash>,
    },
    /// A record's canonical CBOR; empty when the record is gone.
    Record {
        /// Its name.
        key: String,
        /// Its value.
        value: ByteBuf,
    },
    /// A sealed chunk.
    Blob {
        /// Its SHA-256.
        digest: Hash,
        /// Its bytes.
        bytes: ByteBuf,
    },
    /// The receiver went through every entry of `categories`, whose
    /// manifest digest `committed` is.
    Done {
        /// The categories chosen.
        categories: Vec<Category>,
        /// Their entries' manifest digest.
        committed: Hash,
    },
    /// The sender checked [`Body::Done`].
    Ack,
    /// Either side stops, with an error code.
    Abort {
        /// Such as `DEVICE_NOT_APPROVED`.
        code: String,
    },
}

fn io(e: std::io::Error) -> Error {
    match e.kind() {
        std::io::ErrorKind::UnexpectedEof => {
            Error::Unavailable("transfer: connection closed".into())
        }
        _ => Error::Unavailable(format!("transfer: {e}")),
    }
}

async fn write_frame<S: AsyncWrite + Unpin>(stream: &mut S, bytes: &[u8]) -> Result<()> {
    let length = u32::try_from(bytes.len()).map_err(|_| Error::QuotaExceeded)?;
    stream.write_all(&length.to_be_bytes()).await.map_err(io)?;
    stream.write_all(bytes).await.map_err(io)?;
    stream.flush().await.map_err(io)
}

async fn read_frame<S: AsyncRead + Unpin>(stream: &mut S, max: usize) -> Result<Vec<u8>> {
    let mut length = [0; 4];
    stream.read_exact(&mut length).await.map_err(io)?;
    let length = u32::from_be_bytes(length) as usize;
    ensure(length <= max, Error::QuotaExceeded)?;
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await.map_err(io)?;
    Ok(bytes)
}

/// Send the hello.
///
/// # Errors
/// Returns `Error::Unavailable` when the connection fails.
pub(super) async fn write_hello<S: AsyncWrite + Unpin>(
    stream: &mut S,
    hello: &Hello,
) -> Result<()> {
    write_frame(stream, &canonical(hello)).await
}

/// Read the hello; its checks are the receiver's.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for a malformed hello.
pub(super) async fn read_hello<S: AsyncRead + Unpin>(stream: &mut S) -> Result<Hello> {
    decode(&read_frame(stream, MAX_HELLO).await?, MAX_HELLO)
}

/// The sender's direction; the receiver's is 1.
pub(super) const SENDER: u8 = 0;
/// The receiver's direction.
pub(super) const RECEIVER: u8 = 1;

fn nonce(direction: u8, sequence: u64) -> [u8; 12] {
    let mut nonce = [0; 12];
    nonce[0] = direction;
    nonce[4..].copy_from_slice(&sequence.to_be_bytes());
    nonce
}

fn aad(session: &SessionId, direction: u8, sequence: u64) -> Vec<u8> {
    canonical(&(DOMAIN, Bytes::new(session), direction, sequence))
}

/// Seal `body` as the frame `sequence` of `direction`.
pub(super) fn seal(
    key: &[u8; 32],
    session: &SessionId,
    direction: u8,
    sequence: u64,
    body: &Body,
) -> Result<Vec<u8>> {
    let plain = Zeroizing::new(canonical(body));
    ensure(plain.len() <= MAX_BODY, Error::QuotaExceeded)?;
    Ok(aes_gcm_seal(
        key,
        &nonce(direction, sequence),
        &plain,
        &aad(session, direction, sequence),
    ))
}

/// Open the frame `sequence` of `direction`.
pub(super) fn open(
    key: &[u8; 32],
    session: &SessionId,
    direction: u8,
    sequence: u64,
    sealed: &[u8],
) -> Result<Body> {
    let plain = aes_gcm_open(
        key,
        &nonce(direction, sequence),
        sealed,
        &aad(session, direction, sequence),
    )?;
    decode(&plain, MAX_BODY)
}

/// One side of the encrypted frames.
pub(super) struct Channel<S> {
    stream: S,
    key: Zeroizing<[u8; 32]>,
    session: SessionId,
    direction: u8,
    sent: u64,
    received: u64,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Channel<S> {
    pub(super) fn new(
        stream: S,
        key: Zeroizing<[u8; 32]>,
        session: SessionId,
        direction: u8,
    ) -> Self {
        Self {
            stream,
            key,
            session,
            direction,
            sent: 0,
            received: 0,
        }
    }

    /// Send the next frame.
    pub(super) async fn send(&mut self, body: &Body) -> Result<()> {
        let sealed = seal(&self.key, &self.session, self.direction, self.sent, body)?;
        self.sent += 1;
        write_frame(&mut self.stream, &sealed).await
    }

    /// Receive the next frame, within the idle timeout.
    pub(super) async fn recv(&mut self) -> Result<Body> {
        let sealed = tokio::time::timeout(
            super::IDLE_TIMEOUT,
            read_frame(&mut self.stream, MAX_BODY + TAG),
        )
        .await
        .map_err(|_| Error::Unavailable("transfer: the other device went silent".into()))??;
        let body = open(
            &self.key,
            &self.session,
            1 - self.direction,
            self.received,
            &sealed,
        )?;
        self.received += 1;
        Ok(body)
    }
}
