//! `dmsg-nearby-share/1` Commitment-then-Reveal handshake, credit-window flow
//! control, file streaming via `library::import` and Vault snapshot streaming
//! via `dmsg-secret-package/1`.

use super::wire::{
    Body, Channel, ClientHello, ClientReveal, EphemeralKey, FileSummary, FrameTransport,
    HandshakeSecrets, IDENTITY_DOMAIN, OfferKind, ServerHello, commitment, derive_session,
    handshake_transcript, qr_proof, recv_plain, send_plain,
};
use super::{
    CONFIRM_TIMEOUT, CREDIT_REFILL, DEFAULT_CREDITS, FORMAT, HELLO_TIMEOUT, IDLE_TIMEOUT,
    MAX_SESSION_BYTES, MAX_SESSION_FILES,
};
use crate::codec::digest;
use crate::crypto::{sign, signing_public};
use crate::files::{FileManifest, MAX_FILE, chunk_count, read_chunk};
use crate::hold::Hold;
use crate::secrets::receiving::stage_nearby_package;
use dmsg_types::{Error, Hash, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::future::Future;
use zeroize::{Zeroize, Zeroizing};

/// Optional peer identity verified inside the encrypted channel after confirmation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PeerIdentity {
    /// Deployment environment claimed by the peer.
    pub environment: String,
    /// Account ID text (or standalone account ID).
    pub account: String,
    /// Peer device ID (hex).
    pub device: String,
    /// Optional public handle hint.
    pub handle: Option<String>,
    /// Verification level (`signature_only` when offline/standalone, `same_account` or `verified`).
    pub status: String,
}

/// Live transfer progress snapshot.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// Total files (or 1 for a secret package).
    pub total_items: u32,
    /// Completed files (or 1 for a secret package).
    pub completed_items: u32,
    /// Total plaintext bytes expected.
    pub total_bytes: u64,
    /// Plaintext bytes transferred so far.
    pub bytes: u64,
    /// Current item name being moved (never a Vault secret value).
    pub current_name: Option<String>,
}

/// Committed nearby file record returned on completion.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ReceivedFile {
    /// Zero-based index from the offer.
    pub index: u32,
    /// New library file ID on the receiver.
    pub file_id: String,
    /// New library version ID on the receiver.
    pub version_id: String,
    /// Stored display name on the receiver.
    pub name: String,
    /// Exact plaintext length in bytes.
    pub size: u64,
}

/// Outcome of a completed receive session.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ReceiveOutcome {
    /// Kind of payload received.
    pub kind: OfferKind,
    /// Committed files when `kind == OfferKind::Files`.
    pub files: Vec<ReceivedFile>,
    /// Staged `secrets/receiving` preview payload when `kind == OfferKind::SecretPackage`.
    pub package_preview: Option<Value>,
    /// Optional post-confirm verified identity of the sender.
    pub identity: Option<PeerIdentity>,
}

/// Outcome of a completed send session.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SendOutcome {
    /// Kind of payload sent.
    pub kind: OfferKind,
    /// Number of files acknowledged by the receiver (or 1 for a secret package).
    pub sent_items: u32,
    /// Total plaintext bytes sent.
    pub bytes: u64,
    /// Optional verified package digest acknowledged by receiver.
    pub package_digest: Option<String>,
    /// Optional post-confirm verified identity of the receiver.
    pub identity: Option<PeerIdentity>,
}

/// Receiver decision after checking the 6-digit SAS and offer metadata.
#[derive(Clone, Debug)]
pub enum ReceiveDecision {
    /// Accept selected file indices (or empty vec for `SecretPackage`) into `folder`.
    Accept {
        /// Destination Files folder ID (e.g. `"root"`).
        folder: String,
        /// Selected file indices from `OfferMeta.files`.
        selected: Vec<u32>,
        /// Whether to share this device's signed identity after mutual confirmation.
        share_identity: bool,
    },
    /// Reject the transfer with a reason code.
    Reject {
        /// Stable reason code.
        code: String,
    },
}

/// Sender's file source item prepared from the local Files library.
#[derive(Clone, Debug)]
pub struct SendFileItem {
    /// Library file ID.
    pub file_id: String,
    /// Optional exact version ID (defaults to current version).
    pub version_id: Option<String>,
}

fn random_16() -> [u8; 16] {
    let mut out = [0u8; 16];
    getrandom::fill(&mut out).expect("operating system random source");
    out
}

/// Execute the receiver side of the Commitment-then-Reveal handshake.
///
/// When `require_qr` is `true` (default QR-only mode), a peer that cannot prove
/// possession of `qrs` is rejected immediately before notifying the UI.
/// `hello_wait` bounds the wait for the sender's first message: a direct
/// connection arrives with it, a relay room waits until a sender joins.
pub async fn receiver_handshake<T: FrameTransport>(
    transport: &mut T,
    r_key: &EphemeralKey,
    qrs: &[u8; 16],
    require_qr: bool,
    hello_wait: std::time::Duration,
) -> Result<HandshakeSecrets> {
    let hello: ClientHello = tokio::time::timeout(hello_wait, recv_plain(transport))
        .await
        .map_err(|_| Error::Unavailable("nearby: hello timed out".into()))??;
    ensure(hello.v == FORMAT, Error::UnsupportedProtocol)?;

    let sid = random_16();
    let r_nonce = random_16();
    let r_pk = r_key.public_bytes();
    let server_hello = ServerHello {
        v: FORMAT,
        sid: ByteArray::from(sid),
        r_pk: ByteArray::from(r_pk),
        r_nonce: ByteArray::from(r_nonce),
    };
    tokio::time::timeout(HELLO_TIMEOUT, send_plain(transport, &server_hello))
        .await
        .map_err(|_| Error::Unavailable("nearby: hello send timed out".into()))??;

    let reveal: ClientReveal = tokio::time::timeout(HELLO_TIMEOUT, recv_plain(transport))
        .await
        .map_err(|_| Error::Unavailable("nearby: reveal timed out".into()))??;
    let s_pk: [u8; 32] = *reveal.s_pk;
    let s_nonce: [u8; 16] = *reveal.s_nonce;
    ensure(
        commitment(&s_pk, &s_nonce) == hello.commit,
        Error::IntegrityFailed,
    )?;

    let transcript = handshake_transcript(&sid, &hello.commit, &s_pk, &s_nonce, &r_pk, &r_nonce);
    let expected_qr = qr_proof(qrs, &transcript);
    let bound_qrs = match reveal.qr_proof {
        Some(proof) => {
            ensure(proof == expected_qr, Error::Forbidden)?;
            Some(qrs)
        }
        None => {
            ensure(!require_qr, Error::Forbidden)?;
            None
        }
    };

    let shared = r_key.diffie_hellman(&s_pk)?;
    Ok(derive_session(sid, &shared, transcript, bound_qrs))
}

/// Execute the sender side of the Commitment-then-Reveal handshake.
///
/// In QR mode, `expected_r_pk` pins the receiver's ephemeral public key before
/// revealing `(s_pk, s_nonce)` or computing `qr_proof`.
pub async fn sender_handshake<T: FrameTransport>(
    transport: &mut T,
    expected_r_pk: Option<&[u8; 32]>,
    qrs: Option<&[u8; 16]>,
) -> Result<HandshakeSecrets> {
    let s_key = EphemeralKey::generate();
    let s_pk = s_key.public_bytes();
    let s_nonce = random_16();
    let commit = commitment(&s_pk, &s_nonce);

    let hello = ClientHello { v: FORMAT, commit };
    tokio::time::timeout(HELLO_TIMEOUT, send_plain(transport, &hello))
        .await
        .map_err(|_| Error::Unavailable("nearby: hello send timed out".into()))??;

    let server_hello: ServerHello = tokio::time::timeout(HELLO_TIMEOUT, recv_plain(transport))
        .await
        .map_err(|_| Error::Unavailable("nearby: server hello timed out".into()))??;
    ensure(server_hello.v == FORMAT, Error::UnsupportedProtocol)?;

    let sid: [u8; 16] = *server_hello.sid;
    let r_pk: [u8; 32] = *server_hello.r_pk;
    let r_nonce: [u8; 16] = *server_hello.r_nonce;
    if let Some(expected) = expected_r_pk {
        ensure(&r_pk == expected, Error::IntegrityFailed)?;
    }

    let transcript = handshake_transcript(&sid, &commit, &s_pk, &s_nonce, &r_pk, &r_nonce);
    let proof = qrs.map(|secret| qr_proof(secret, &transcript));
    let reveal = ClientReveal {
        s_pk: ByteArray::from(s_pk),
        s_nonce: ByteArray::from(s_nonce),
        qr_proof: proof,
    };
    tokio::time::timeout(HELLO_TIMEOUT, send_plain(transport, &reveal))
        .await
        .map_err(|_| Error::Unavailable("nearby: reveal send timed out".into()))??;

    let shared = s_key.diffie_hellman(&r_pk)?;
    Ok(derive_session(sid, &shared, transcript, qrs))
}

/// Build an optional post-confirmation `Identity` frame signed by this device.
pub async fn build_identity<H: Hold>(h: &H, transcript: &Hash, role: &str) -> Result<Body> {
    h.with(|c| {
        let keys = c.keys()?;
        let environment = c.workspace.meta.environment.clone();
        let account = c.content_id()?.to_string();
        let device = c.workspace.meta.device_id;
        let sign_pk = signing_public(&keys.signing);
        let msg = digest(IDENTITY_DOMAIN, &(transcript, role, &account, device));
        let signature = sign(&keys.signing, msg.as_slice());
        Ok(Body::Identity {
            environment,
            account,
            device,
            sign_pk: ByteArray::from(sign_pk),
            handle: None,
            signature: ByteArray::from(signature),
        })
    })
    .await
}

/// Verify an optional peer `Identity` frame against `transcript` and expected peer `role`.
pub async fn verify_identity<H: Hold>(
    h: &H,
    transcript: &Hash,
    expected_role: &str,
    body: Body,
) -> Result<PeerIdentity> {
    let Body::Identity {
        environment,
        account,
        device,
        sign_pk,
        handle,
        signature,
    } = body
    else {
        return Err(Error::IntegrityFailed);
    };
    let msg = digest(
        IDENTITY_DOMAIN,
        &(transcript, expected_role, &account, device),
    );
    let sign_pk = Hash::new(*sign_pk);
    dmsg_protocol::verify(&sign_pk, msg.as_slice(), signature.as_slice())?;
    let status = h
        .with(|c| {
            // An account ID is public: only a key the certified device map
            // registers for that device makes the sender one of our devices.
            let own = !c.is_local()
                && c.workspace.meta.environment == environment
                && c.content_id().is_ok_and(|id| id.to_string() == account)
                && c.security
                    .as_ref()
                    .and_then(|s| s.device(&device))
                    .is_some_and(|d| d.revoked_at.is_none() && d.input.signing_pub == sign_pk);
            Ok(if own {
                "same_account"
            } else {
                "signature_only"
            }
            .to_string())
        })
        .await?;
    Ok(PeerIdentity {
        environment,
        account,
        device: hex::encode(*device),
        handle,
        status,
    })
}

/// Receive a nearby session after [`receiver_handshake`] completed.
///
/// The caller provides `decide`, which receives the post-SAS `OfferMeta` and
/// waits for the user to verify the 6-digit SAS and accept or reject.
pub async fn receive_after_handshake<H, T, D, DFut, P>(
    h: &H,
    mut channel: Channel<T>,
    secrets: HandshakeSecrets,
    max_package_bytes: u64,
    decide: D,
    mut on_progress: P,
) -> Result<ReceiveOutcome>
where
    H: Hold,
    T: FrameTransport,
    D: FnOnce(OfferKind, Vec<FileSummary>, Option<u64>, Option<String>) -> DFut,
    DFut: Future<Output = Result<ReceiveDecision>>,
    P: FnMut(Progress),
{
    let first = channel.recv(CONFIRM_TIMEOUT).await?;
    let (kind, files, package_size, alias) = match first {
        Body::OfferMeta {
            kind,
            files,
            package_size,
            alias,
        } => (kind, files, package_size, alias),
        Body::Abort { code } | Body::Reject { code } => {
            return Err(Error::Unavailable(format!("nearby: peer stopped ({code})")));
        }
        _ => {
            channel.abort("PROTOCOL_ERROR").await;
            return Err(Error::IntegrityFailed);
        }
    };

    // Validate offer bounds before prompting the user.
    match kind {
        OfferKind::Files => {
            if files.is_empty() || files.len() > MAX_SESSION_FILES {
                channel.abort("QUOTA_EXCEEDED").await;
                return Err(Error::QuotaExceeded);
            }
            let max_file = h.with(|c| Ok(c.config.files_max_bytes)).await?;
            let mut total = 0u64;
            let mut seen = BTreeSet::new();
            for f in &files {
                ensure(
                    seen.insert(f.index) && f.size <= max_file,
                    Error::QuotaExceeded,
                )?;
                total = total.checked_add(f.size).ok_or(Error::QuotaExceeded)?;
                ensure(total <= MAX_SESSION_BYTES, Error::QuotaExceeded)?;
            }
        }
        OfferKind::SecretPackage => {
            let pkg_size = package_size.ok_or(Error::IntegrityFailed)?;
            let cap = max_package_bytes.min(MAX_FILE);
            if pkg_size == 0 || pkg_size > cap {
                channel.abort("QUOTA_EXCEEDED").await;
                return Err(Error::QuotaExceeded);
            }
        }
    }

    let alias = alias.map(|a| super::offer::sanitize_alias(&a));
    // The sender sends nothing until it is accepted: a frame, a closed
    // connection or its own timeout means it left while the person decides.
    let decision = tokio::select! {
        decision = decide(kind, files.clone(), package_size, alias.clone()) => decision?,
        early = channel.recv(CONFIRM_TIMEOUT) => {
            return Err(match early {
                Ok(Body::Abort { code }) => {
                    Error::Unavailable(format!("nearby: peer stopped ({code})"))
                }
                Ok(_) => Error::IntegrityFailed,
                Err(err) => err,
            });
        }
    };
    let (folder, selected, share_identity) = match decision {
        ReceiveDecision::Accept {
            folder,
            selected,
            share_identity,
        } => (folder, selected, share_identity),
        ReceiveDecision::Reject { code } => {
            let _ = channel.send(&Body::Reject { code }).await;
            return Err(Error::Unavailable("nearby: rejected".into()));
        }
    };

    let selected_set: BTreeSet<u32> = selected.iter().copied().collect();
    if kind == OfferKind::Files {
        ensure(
            !selected.is_empty()
                && selected_set.len() == selected.len()
                && selected_set
                    .iter()
                    .all(|idx| files.iter().any(|f| f.index == *idx)),
            Error::InvalidInput("nearby selected files".into()),
        )?;
    }

    channel
        .send(&Body::Accept {
            credits: DEFAULT_CREDITS,
            selected: selected.clone(),
        })
        .await?;

    if share_identity {
        let id_frame = build_identity(h, &secrets.transcript, "receiver").await?;
        channel.send(&id_frame).await?;
    }

    let mut peer_identity = None;

    match kind {
        OfferKind::Files => {
            let total_bytes: u64 = files
                .iter()
                .filter(|f| selected_set.contains(&f.index))
                .map(|f| f.size)
                .sum();
            let mut progress = Progress {
                total_items: selected.len() as u32,
                completed_items: 0,
                total_bytes,
                bytes: 0,
                current_name: None,
            };
            on_progress(progress.clone());
            let mut committed = Vec::with_capacity(selected.len());
            let mut remaining = selected_set;
            let mut pending_refill = 0u32;

            loop {
                let frame = channel.recv(IDLE_TIMEOUT).await?;
                match frame {
                    Body::Identity { .. } if peer_identity.is_none() => {
                        peer_identity =
                            Some(verify_identity(h, &secrets.transcript, "sender", frame).await?);
                    }
                    Body::FileBegin {
                        index,
                        name,
                        mime: _,
                        size,
                        sha256,
                    } => {
                        ensure(remaining.remove(&index), Error::IntegrityFailed)?;
                        let expected = files
                            .iter()
                            .find(|f| f.index == index)
                            .ok_or(Error::IntegrityFailed)?;
                        ensure(
                            expected.size == size && expected.name == name,
                            Error::IntegrityFailed,
                        )?;
                        progress.current_name = Some(name.clone());
                        on_progress(progress.clone());

                        let mut importing = Some(
                            h.with(|c| c.nearby_import_start(size, &name, &folder))
                                .await?,
                        );
                        let expected_chunks = chunk_count(size);
                        let mut got_chunks = 0u64;

                        let file_result: Result<ReceivedFile> = async {
                            while got_chunks < expected_chunks {
                                let next = channel.recv(IDLE_TIMEOUT).await?;
                                match next {
                                    Body::Chunk {
                                        index: chunk_idx,
                                        seq,
                                        bytes,
                                    } => {
                                        ensure(
                                            chunk_idx == index && u64::from(seq) == got_chunks,
                                            Error::IntegrityFailed,
                                        )?;
                                        let len = bytes.len() as u64;
                                        let plain = Zeroizing::new(bytes.into_vec());
                                        let imp =
                                            importing.as_mut().ok_or(Error::IntegrityFailed)?;
                                        h.with(|c| c.import_part(imp, got_chunks, &plain)).await?;
                                        got_chunks += 1;
                                        progress.bytes += len;
                                        on_progress(progress.clone());

                                        pending_refill += 1;
                                        if pending_refill >= CREDIT_REFILL {
                                            channel
                                                .send(&Body::Credit {
                                                    granted: pending_refill,
                                                })
                                                .await?;
                                            pending_refill = 0;
                                        }
                                    }
                                    Body::Abort { code } => {
                                        return Err(Error::Unavailable(format!(
                                            "nearby: sender aborted ({code})"
                                        )));
                                    }
                                    _ => return Err(Error::IntegrityFailed),
                                }
                            }
                            let end = channel.recv(IDLE_TIMEOUT).await?;
                            let Body::FileEnd {
                                index: end_idx,
                                sha256: end_sha,
                            } = end
                            else {
                                return Err(Error::IntegrityFailed);
                            };
                            let mut imp = importing.take().ok_or(Error::IntegrityFailed)?;
                            if end_idx != index || end_sha != sha256 || imp.digest_hex() != sha256 {
                                let _ = h
                                    .with(|c| {
                                        c.nearby_import_abort(imp);
                                        Ok(())
                                    })
                                    .await;
                                return Err(Error::IntegrityFailed);
                            }
                            let thumb = imp.thumbnail_async().await;
                            let saved = h.with(|c| c.import_finish(imp, thumb)).await?;
                            Ok(ReceivedFile {
                                index,
                                file_id: saved.file.id,
                                version_id: saved.version,
                                name: saved.file.name,
                                size,
                            })
                        }
                        .await;

                        match file_result {
                            Ok(rec) => {
                                progress.completed_items += 1;
                                on_progress(progress.clone());
                                channel
                                    .send(&Body::FileAck {
                                        index,
                                        file_id: rec.file_id.clone(),
                                        version_id: rec.version_id.clone(),
                                    })
                                    .await?;
                                committed.push(rec);
                            }
                            Err(err) => {
                                if let Some(imp) = importing.take() {
                                    let _ = h
                                        .with(|c| {
                                            c.nearby_import_abort(imp);
                                            Ok(())
                                        })
                                        .await;
                                }
                                channel.abort(&crate::error_code(&err)).await;
                                return Err(err);
                            }
                        }
                    }
                    Body::Done => {
                        ensure(remaining.is_empty(), Error::RecoveryIncomplete)?;
                        return Ok(ReceiveOutcome {
                            kind,
                            files: committed,
                            package_preview: None,
                            identity: peer_identity,
                        });
                    }
                    Body::Abort { code } => {
                        return Err(Error::Unavailable(format!(
                            "nearby: sender aborted ({code})"
                        )));
                    }
                    _ => {
                        channel.abort("PROTOCOL_ERROR").await;
                        return Err(Error::IntegrityFailed);
                    }
                }
            }
        }
        OfferKind::SecretPackage => {
            let expected_size = package_size.ok_or(Error::IntegrityFailed)?;
            let mut progress = Progress {
                total_items: 1,
                completed_items: 0,
                total_bytes: expected_size,
                bytes: 0,
                current_name: Some("Vault item".into()),
            };
            on_progress(progress.clone());

            let (size, sha256) = loop {
                // Sender may take up to CONFIRM_TIMEOUT to finish high-risk SecretShare approval.
                let next = channel.recv(CONFIRM_TIMEOUT).await?;
                match next {
                    Body::Identity { .. } if peer_identity.is_none() => {
                        peer_identity =
                            Some(verify_identity(h, &secrets.transcript, "sender", next).await?);
                    }
                    Body::PackageBegin { size, sha256 } => {
                        ensure(
                            size == expected_size && size <= max_package_bytes.min(MAX_FILE),
                            Error::QuotaExceeded,
                        )?;
                        break (size, sha256);
                    }
                    Body::Abort { code } => {
                        return Err(Error::Unavailable(format!(
                            "nearby: sender aborted ({code})"
                        )));
                    }
                    _ => {
                        channel.abort("PROTOCOL_ERROR").await;
                        return Err(Error::IntegrityFailed);
                    }
                }
            };

            let expected_chunks = chunk_count(size);
            let mut got_chunks = 0u64;
            let mut raw = Zeroizing::new(Vec::with_capacity(size as usize));
            let mut pending_refill = 0u32;

            while got_chunks < expected_chunks {
                let next = channel.recv(IDLE_TIMEOUT).await?;
                match next {
                    Body::Chunk {
                        index: 0,
                        seq,
                        bytes,
                    } => {
                        ensure(u64::from(seq) == got_chunks, Error::IntegrityFailed)?;
                        ensure(
                            raw.len() as u64 + bytes.len() as u64 <= size,
                            Error::IntegrityFailed,
                        )?;
                        raw.extend_from_slice(&bytes);
                        got_chunks += 1;
                        progress.bytes = raw.len() as u64;
                        on_progress(progress.clone());

                        pending_refill += 1;
                        if pending_refill >= CREDIT_REFILL {
                            channel
                                .send(&Body::Credit {
                                    granted: pending_refill,
                                })
                                .await?;
                            pending_refill = 0;
                        }
                    }
                    Body::Abort { code } => {
                        return Err(Error::Unavailable(format!(
                            "nearby: sender aborted ({code})"
                        )));
                    }
                    _ => {
                        channel.abort("PROTOCOL_ERROR").await;
                        return Err(Error::IntegrityFailed);
                    }
                }
            }

            let end = channel.recv(IDLE_TIMEOUT).await?;
            let Body::PackageEnd { sha256: end_sha } = end else {
                channel.abort("PROTOCOL_ERROR").await;
                return Err(Error::IntegrityFailed);
            };
            ensure(
                end_sha == sha256 && raw.len() as u64 == size,
                Error::IntegrityFailed,
            )?;

            let preview = stage_nearby_package(
                h,
                raw,
                &sha256,
                max_package_bytes.min(MAX_FILE),
                alias.as_deref(),
            )
            .await?;

            channel
                .send(&Body::PackageAck {
                    digest: sha256.clone(),
                })
                .await?;
            progress.completed_items = 1;
            on_progress(progress);

            let _ = channel.recv(IDLE_TIMEOUT).await;
            Ok(ReceiveOutcome {
                kind,
                files: vec![],
                package_preview: Some(preview),
                identity: peer_identity,
            })
        }
    }
}

/// Send selected files from the local library after [`sender_handshake`] completed.
///
/// `confirm_sas` is awaited before sending `OfferMeta` in browse mode so zero
/// filenames leak to an unverified peer.
#[allow(clippy::too_many_arguments)]
pub async fn send_files_after_handshake<H, T, C, CFut, P>(
    h: &H,
    mut channel: Channel<T>,
    secrets: HandshakeSecrets,
    items: &[SendFileItem],
    alias: Option<String>,
    share_identity: bool,
    max_total_bytes: u64,
    confirm_sas: C,
    mut on_progress: P,
) -> Result<SendOutcome>
where
    H: Hold,
    T: FrameTransport,
    C: FnOnce(&HandshakeSecrets) -> CFut,
    CFut: Future<Output = Result<()>>,
    P: FnMut(Progress),
{
    ensure(
        !items.is_empty() && items.len() <= MAX_SESSION_FILES,
        Error::QuotaExceeded,
    )?;

    // Confirm SAS before leaking any filenames in browse mode.
    if !secrets.qr_bound {
        confirm_sas(&secrets).await?;
    }

    // Resolve immutable manifests from the library (pinning exact versions).
    let manifests: Vec<(u32, FileManifest)> = h
        .with(|c| {
            let mut out = Vec::with_capacity(items.len());
            let mut total = 0u64;
            for (i, item) in items.iter().enumerate() {
                let (_, _, manifest) =
                    c.library_manifest(&item.file_id, item.version_id.as_deref())?;
                total = total
                    .checked_add(manifest.size)
                    .ok_or(Error::QuotaExceeded)?;
                ensure(total <= max_total_bytes, Error::QuotaExceeded)?;
                out.push((i as u32, manifest));
            }
            Ok(out)
        })
        .await?;

    let summaries: Vec<FileSummary> = manifests
        .iter()
        .map(|(index, m)| FileSummary {
            index: *index,
            name: m.name.clone(),
            mime: m.mime.clone(),
            size: m.size,
        })
        .collect();

    channel
        .send(&Body::OfferMeta {
            kind: OfferKind::Files,
            files: summaries,
            package_size: None,
            alias,
        })
        .await?;

    let accept = channel.recv(CONFIRM_TIMEOUT).await?;
    let (mut credits, selected) = match accept {
        Body::Accept { credits, selected } => (credits.max(1), selected),
        Body::Reject { code } | Body::Abort { code } => {
            return Err(Error::Unavailable(format!(
                "nearby: receiver rejected ({code})"
            )));
        }
        _ => {
            channel.abort("PROTOCOL_ERROR").await;
            return Err(Error::IntegrityFailed);
        }
    };

    if share_identity {
        let id_frame = build_identity(h, &secrets.transcript, "sender").await?;
        channel.send(&id_frame).await?;
    }

    let selected_set: BTreeSet<u32> = selected.iter().copied().collect();
    let total_bytes: u64 = manifests
        .iter()
        .filter(|(idx, _)| selected_set.contains(idx))
        .map(|(_, m)| m.size)
        .sum();
    let mut progress = Progress {
        total_items: selected_set.len() as u32,
        completed_items: 0,
        total_bytes,
        bytes: 0,
        current_name: None,
    };
    on_progress(progress.clone());

    let mut peer_identity = None;
    let mut sent_items = 0u32;

    for (index, mut manifest) in manifests {
        if !selected_set.contains(&index) {
            manifest.key.zeroize();
            continue;
        }
        progress.current_name = Some(manifest.name.clone());
        on_progress(progress.clone());

        channel
            .send(&Body::FileBegin {
                index,
                name: manifest.name.clone(),
                mime: manifest.mime.clone(),
                size: manifest.size,
                sha256: manifest.sha256.clone(),
            })
            .await?;

        let count = chunk_count(manifest.size);
        let mut overall = Sha256::new();
        for seq in 0..count {
            while credits == 0 {
                let ctrl = channel.recv(IDLE_TIMEOUT).await?;
                match ctrl {
                    Body::Credit { granted } => credits = credits.saturating_add(granted),
                    Body::Identity { .. } if peer_identity.is_none() => {
                        peer_identity =
                            Some(verify_identity(h, &secrets.transcript, "receiver", ctrl).await?);
                    }
                    Body::Abort { code } => {
                        manifest.key.zeroize();
                        return Err(Error::Unavailable(format!(
                            "nearby: receiver aborted ({code})"
                        )));
                    }
                    _ => {
                        manifest.key.zeroize();
                        channel.abort("PROTOCOL_ERROR").await;
                        return Err(Error::IntegrityFailed);
                    }
                }
            }
            let plain = h
                .with(|c| read_chunk(c.blobs()?, &manifest, seq))
                .await
                .inspect_err(|_| manifest.key.zeroize())?;
            overall.update(&plain);
            let len = plain.len() as u64;
            channel
                .send(&Body::Chunk {
                    index,
                    seq: seq as u32,
                    bytes: ByteBuf::from(plain.to_vec()),
                })
                .await
                .inspect_err(|_| manifest.key.zeroize())?;
            credits -= 1;
            progress.bytes += len;
            on_progress(progress.clone());
        }
        ensure(
            hex::encode(overall.finalize()) == manifest.sha256,
            Error::IntegrityFailed,
        )?;
        let sha256 = manifest.sha256.clone();
        manifest.key.zeroize();

        channel.send(&Body::FileEnd { index, sha256 }).await?;

        // Wait for per-file FileAck (draining any credit or optional identity frames first).
        loop {
            let ack = channel.recv(IDLE_TIMEOUT).await?;
            match ack {
                Body::Credit { granted } => credits = credits.saturating_add(granted),
                Body::Identity { .. } if peer_identity.is_none() => {
                    peer_identity =
                        Some(verify_identity(h, &secrets.transcript, "receiver", ack).await?);
                }
                Body::FileAck { index: ack_idx, .. } => {
                    ensure(ack_idx == index, Error::IntegrityFailed)?;
                    sent_items += 1;
                    progress.completed_items = sent_items;
                    on_progress(progress.clone());
                    break;
                }
                Body::Abort { code } => {
                    return Err(Error::Unavailable(format!(
                        "nearby: receiver aborted ({code})"
                    )));
                }
                _ => {
                    channel.abort("PROTOCOL_ERROR").await;
                    return Err(Error::IntegrityFailed);
                }
            }
        }
    }

    channel.send(&Body::Done).await?;
    Ok(SendOutcome {
        kind: OfferKind::Files,
        sent_items,
        bytes: progress.bytes,
        package_digest: None,
        identity: peer_identity,
    })
}

/// Send an approved `dmsg-secret-package/1` Vault snapshot over a nearby session.
///
/// `await_approved_job` runs after SAS confirmation and receiver acceptance: it
/// prepares (`secret_nearby_preparation`), creates and completes `Action::SecretShare`
/// bound to `secrets.transcript`, and returns the approved share job ID.
#[allow(clippy::too_many_arguments)]
pub async fn send_secret_package_after_handshake<H, T, C, CFut, A, AFut, P>(
    h: &H,
    mut channel: Channel<T>,
    secrets: HandshakeSecrets,
    estimated_plain_bytes: u64,
    alias: Option<String>,
    share_identity: bool,
    confirm_sas: C,
    await_approved_job: A,
    mut on_progress: P,
) -> Result<SendOutcome>
where
    H: Hold,
    T: FrameTransport,
    C: FnOnce(&HandshakeSecrets) -> CFut,
    CFut: Future<Output = Result<()>>,
    A: FnOnce(&HandshakeSecrets) -> AFut,
    AFut: Future<Output = Result<String>>,
    P: FnMut(Progress),
{
    ensure(
        estimated_plain_bytes > 0 && estimated_plain_bytes <= MAX_FILE,
        Error::QuotaExceeded,
    )?;

    if !secrets.qr_bound {
        confirm_sas(&secrets).await?;
    }

    channel
        .send(&Body::OfferMeta {
            kind: OfferKind::SecretPackage,
            files: vec![],
            package_size: Some(estimated_plain_bytes),
            alias,
        })
        .await?;

    let accept = channel.recv(CONFIRM_TIMEOUT).await?;
    let mut credits = match accept {
        Body::Accept { credits, .. } => credits.max(1),
        Body::Reject { code } | Body::Abort { code } => {
            return Err(Error::Unavailable(format!(
                "nearby: receiver rejected ({code})"
            )));
        }
        _ => {
            channel.abort("PROTOCOL_ERROR").await;
            return Err(Error::IntegrityFailed);
        }
    };

    if share_identity {
        let id_frame = build_identity(h, &secrets.transcript, "sender").await?;
        channel.send(&id_frame).await?;
    }

    // Complete high-risk SecretShare approval bound to `secrets.transcript`, then claim staged bytes.
    let job_id = await_approved_job(&secrets).await?;
    let transcript_hex = hex::encode(*secrets.transcript);
    let mut manifest = h
        .with(|c| c.secret_nearby_claim(&job_id, &transcript_hex))
        .await?;

    let stream_result: Result<(String, u64, Option<PeerIdentity>)> = async {
        ensure(
            manifest.size == estimated_plain_bytes && manifest.size <= MAX_FILE,
            Error::IntegrityFailed,
        )?;
        let mut progress = Progress {
            total_items: 1,
            completed_items: 0,
            total_bytes: manifest.size,
            bytes: 0,
            current_name: Some("Vault item".into()),
        };
        on_progress(progress.clone());

        channel
            .send(&Body::PackageBegin {
                size: manifest.size,
                sha256: manifest.sha256.clone(),
            })
            .await?;

        let count = chunk_count(manifest.size);
        let mut peer_identity = None;
        for seq in 0..count {
            while credits == 0 {
                let ctrl = channel.recv(IDLE_TIMEOUT).await?;
                match ctrl {
                    Body::Credit { granted } => credits = credits.saturating_add(granted),
                    Body::Identity { .. } if peer_identity.is_none() => {
                        peer_identity =
                            Some(verify_identity(h, &secrets.transcript, "receiver", ctrl).await?);
                    }
                    Body::Abort { code } => {
                        return Err(Error::Unavailable(format!(
                            "nearby: receiver aborted ({code})"
                        )));
                    }
                    _ => {
                        channel.abort("PROTOCOL_ERROR").await;
                        return Err(Error::IntegrityFailed);
                    }
                }
            }
            let plain = h.with(|c| read_chunk(c.blobs()?, &manifest, seq)).await?;
            let len = plain.len() as u64;
            channel
                .send(&Body::Chunk {
                    index: 0,
                    seq: seq as u32,
                    bytes: ByteBuf::from(plain.to_vec()),
                })
                .await?;
            credits -= 1;
            progress.bytes += len;
            on_progress(progress.clone());
        }

        channel
            .send(&Body::PackageEnd {
                sha256: manifest.sha256.clone(),
            })
            .await?;

        let digest = loop {
            let ack = channel.recv(IDLE_TIMEOUT).await?;
            match ack {
                Body::Credit { granted } => credits = credits.saturating_add(granted),
                Body::Identity { .. } if peer_identity.is_none() => {
                    peer_identity =
                        Some(verify_identity(h, &secrets.transcript, "receiver", ack).await?);
                }
                Body::PackageAck { digest } => {
                    ensure(digest == manifest.sha256, Error::IntegrityFailed)?;
                    break digest;
                }
                Body::Abort { code } => {
                    return Err(Error::Unavailable(format!(
                        "nearby: receiver aborted ({code})"
                    )));
                }
                _ => {
                    channel.abort("PROTOCOL_ERROR").await;
                    return Err(Error::IntegrityFailed);
                }
            }
        };

        progress.completed_items = 1;
        on_progress(progress);
        channel.send(&Body::Done).await?;
        Ok((digest, manifest.size, peer_identity))
    }
    .await;

    manifest.key.zeroize();
    match stream_result {
        Ok((digest, bytes, identity)) => {
            let _ = h
                .with(|c| c.secret_nearby_finish(&job_id, true, Some(digest.clone()), None))
                .await;
            Ok(SendOutcome {
                kind: OfferKind::SecretPackage,
                sent_items: 1,
                bytes,
                package_digest: Some(digest),
                identity,
            })
        }
        Err(err) => {
            let _ = h
                .with(|c| {
                    c.secret_nearby_finish(&job_id, false, None, Some(crate::error_code(&err)))
                })
                .await;
            channel.abort(&crate::error_code(&err)).await;
            Err(err)
        }
    }
}
