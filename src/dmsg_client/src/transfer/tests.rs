use super::library;
use super::wire::{self, Body, Channel, Header, Hello, RECEIVER, SENDER};
use super::*;
use crate::client::Client;
use crate::codec::{canonical, sha256};
use crate::crypto::tests::Replay;
use crate::crypto::{
    HpkeEnvelope, hpke_open_info, hpke_public, hpke_seal_info, sign, signing_public,
};
use crate::files::CHUNK;
use crate::library::catalog::{FOLDER, ROOT, VERSION};
use crate::testing::{TempDir, account, unlocked};
use dmsg_types::user::ControllerRole;
use dmsg_types::{Ed25519Signature, Error};
use serde_bytes::ByteArray;
use serde_json::{Value as Json, json};
use std::collections::BTreeSet;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf, duplex};
use zeroize::Zeroizing;

const VECTORS: &str = "../../tests/vectors/transfer/transfer-v1.json";

#[tokio::test]
async fn an_export_requires_a_reviewed_one_use_snapshot_and_the_same_unlock_session() {
    let mut pair = pair();
    let to = id(&pair.new);
    let session = session_id();
    let requested = request(&pair.old.hold(), "gui", to, session).await.unwrap();
    let (approval, digest) = (
        requested["id"].as_str().unwrap(),
        requested["digest"].as_str().unwrap(),
    );
    assert_eq!(requested["risk"], "high");
    assert!(matches!(
        claim(&pair.old.hold(), approval, to, session).await,
        Err(Error::Forbidden)
    ));
    pair.old
        .approval_respond(approval, digest, true, "gui", false)
        .await
        .unwrap();
    assert!(matches!(
        claim(&pair.old.hold(), approval, to, session_id()).await,
        Err(Error::IntegrityFailed)
    ));
    pair.old.files_mkdir(ROOT, "after review").unwrap();
    assert!(matches!(
        claim(&pair.old.hold(), approval, to, session).await,
        Err(Error::VersionConflict)
    ));

    let requested = request(&pair.old.hold(), "gui", to, session).await.unwrap();
    let (approval, digest) = (
        requested["id"].as_str().unwrap(),
        requested["digest"].as_str().unwrap(),
    );
    pair.old
        .approval_respond(approval, digest, true, "gui", false)
        .await
        .unwrap();
    let permit = claim(&pair.old.hold(), approval, to, session)
        .await
        .unwrap();
    assert!(matches!(
        claim(&pair.old.hold(), approval, to, session).await,
        Err(Error::Forbidden)
    ));
    pair.old.lock();
    pair.old.keys = Some(pair.old.workspace.unlock(&crate::testing::SECRET).unwrap());
    let (stream, _receiver) = duplex(4096);
    let report = Mutex::new(Report::default());
    assert_eq!(
        send_approved(&pair.old.hold(), &pair.devices, stream, permit, &report).await,
        Err(Error::Locked)
    );
}

#[tokio::test]
async fn an_export_approval_requires_system_presence_when_available() {
    use crate::secure::{Presence, PresenceKey};
    struct Hardware;
    impl PresenceKey for Hardware {
        fn presence(&self) -> Option<Presence> {
            Some(Presence::Password)
        }

        fn create(&self, _: bool) -> Result<(Vec<u8>, Vec<u8>, Zeroizing<[u8; 32]>)> {
            Err(Error::Forbidden)
        }

        fn open(&self, _: &[u8], _: &[u8], _: bool) -> Result<Zeroizing<[u8; 32]>> {
            Err(Error::Forbidden)
        }

        fn biometry_state(&self) -> Option<Hash> {
            None
        }

        fn confirm(&self, _: &str) -> Result<()> {
            Err(Error::Forbidden)
        }
    }
    let mut pair = pair();
    pair.old.set_presence_key(std::sync::Arc::new(Hardware));
    let requested = request(&pair.old.hold(), "gui", id(&pair.new), session_id())
        .await
        .unwrap();
    assert_eq!(
        pair.old
            .approval_respond(
                requested["id"].as_str().unwrap(),
                requested["digest"].as_str().unwrap(),
                true,
                "gui",
                false
            )
            .await,
        Err(Error::Forbidden)
    );
}

struct Fixed(Mutex<BTreeMap<Hash, DeviceInput>>);

impl Devices for Fixed {
    fn active(&self) -> impl Future<Output = Result<BTreeMap<Hash, DeviceInput>>> + Send {
        let devices = self.0.lock().unwrap().clone();
        async move { Ok(devices) }
    }
}

impl Fixed {
    fn revoke(&self, device: &Hash) {
        self.0.lock().unwrap().remove(device);
    }
}

fn id(c: &Client) -> Hash {
    c.workspace.meta.device_id
}

/// Two active devices of the test account, the old one with a library.
struct Pair {
    old: Client,
    new: Client,
    devices: Fixed,
    files: TempDir,
    _dirs: (TempDir, TempDir),
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| ((i as u64 * 31 + seed as u64) % 251) as u8)
        .collect()
}

fn import(c: &mut Client, files: &TempDir, folder: &str, name: &str, bytes: &[u8]) -> String {
    let path = files.path(name);
    std::fs::write(&path, bytes).unwrap();
    c.files_import(&path, folder, false, &mut |_, _| {})
        .unwrap()
        .file
        .id
}

fn pair() -> Pair {
    let dirs = (TempDir::new(), TempDir::new());
    let mut old = unlocked(&dirs.0.0, false, None);
    let new = unlocked(&dirs.1.0, false, None);
    let devices = [&old, &new].map(|c| {
        (
            id(c),
            c.workspace.device_input(ControllerRole::Administrator),
        )
    });
    let files = TempDir::new();
    let docs = old.files_mkdir(ROOT, "docs").unwrap().id;
    old.files_mkdir_path("docs/2026").unwrap();
    import(&mut old, &files, &docs, "a.bin", &pattern(CHUNK * 2 + 5, 1));
    import(&mut old, &files, &docs, "a.bin", &pattern(CHUNK + 9, 2));
    import(&mut old, &files, ROOT, "b.txt", b"hello");
    let gone = import(&mut old, &files, ROOT, "old.txt", b"bye");
    old.files_trash(&[gone]).unwrap();
    Pair {
        old,
        new,
        devices: Fixed(Mutex::new(devices.into())),
        files,
        _dirs: dirs,
    }
}

/// Run both sides over one connection; `cut` ends the sender's side after
/// that many bytes.
async fn transfer(
    p: &mut Pair,
    session: SessionId,
    categories: &[Category],
    cut: Option<usize>,
) -> (Result<Report>, Result<Report>) {
    let (a, b) = duplex(256 * 1024);
    let to = id(&p.new);
    let (sent, got) = (Mutex::new(Report::default()), Mutex::new(Report::default()));
    let (old, new) = (p.old.hold(), p.new.hold());
    let a = Cut {
        inner: a,
        left: cut,
    };
    tokio::join!(
        send(&old, &p.devices, a, to, session, &sent),
        receive(&new, &p.devices, b, session, categories, &got)
    )
}

/// A connection that breaks after a number of written bytes.
struct Cut<S> {
    inner: S,
    left: Option<usize>,
}

impl<S: AsyncRead + Unpin> AsyncRead for Cut<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Cut<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let len = match self.left {
            Some(0) => return Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into())),
            Some(left) => buf.len().min(left),
            None => buf.len(),
        };
        let written = std::task::ready!(Pin::new(&mut self.inner).poll_write(cx, &buf[..len]))?;
        if let Some(left) = &mut self.left {
            *left -= written;
        }
        Poll::Ready(Ok(written))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

fn library(c: &mut Client) -> Vec<Entry> {
    library::entries(c).unwrap()
}

#[tokio::test]
async fn the_library_moves_whole_and_reads_the_same() {
    let mut p = pair();
    let (sent, got) = transfer(&mut p, [1; 16], &Category::ALL, None).await;
    let (sent, got) = (sent.unwrap(), got.unwrap());
    // Two folders, three files, four versions; five chunks of 1 MiB at most.
    assert_eq!(
        (got.entries, got.records, got.kept, got.skipped),
        (9, 9, 0, 0)
    );
    assert_eq!((sent.records, sent.blobs), (9, got.blobs));
    assert_eq!(got.bytes, got.total_bytes);
    assert_eq!(library(&mut p.new), library(&mut p.old));
    let docs = p.new.files_folder_by_path("docs").unwrap();
    let a = p.new.files_find(&docs, "a.bin").unwrap().unwrap();
    assert_eq!(p.new.files_versions(&a).unwrap().len(), 2);
    let out = p.files.path("out.bin");
    p.new.files_export(&a, None, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), pattern(CHUNK + 9, 2));
    // The trashed file stays in the trash; the sender changed nothing.
    assert_eq!(p.new.files_list(ROOT, true).unwrap().files.len(), 1);
    // A second transfer of the same content keeps everything as it is.
    let (_, again) = transfer(&mut p, [2; 16], &Category::ALL, None).await;
    let again = again.unwrap();
    assert_eq!((again.records, again.blobs), (0, 0));
    assert_eq!(again.kept, 9);
}

#[tokio::test]
async fn a_transfer_fills_chunks_of_a_version_already_known_to_the_receiver() {
    let mut p = pair();
    transfer(&mut p, [1; 16], &Category::ALL, None)
        .await
        .1
        .unwrap();
    let id = p.new.files_find(ROOT, "b.txt").unwrap().unwrap();
    let current = p.new.files_show(&id).unwrap().file.current;
    let mut version = p.new.version(&id, &current).unwrap();
    let digest = &version.chunks[0].digest;
    std::fs::remove_file(
        p.new
            .files_blobs_dir()
            .unwrap()
            .join(&digest[..2])
            .join(digest),
    )
    .unwrap();
    version.local = false;
    p.new.save_version(&id, &current, &version).unwrap();
    let received = transfer(&mut p, [2; 16], &Category::ALL, None)
        .await
        .1
        .unwrap();
    assert_eq!(received.blobs, 1);
    assert!(p.new.version(&id, &current).unwrap().local);
    let out = p.files.path("restored.txt");
    p.new.files_export(&id, None, &out).unwrap();
    assert_eq!(std::fs::read(out).unwrap(), b"hello");
}

#[test]
fn an_incomplete_source_cannot_offer_a_complete_library() {
    let mut p = pair();
    let id = p.old.files_find(ROOT, "b.txt").unwrap().unwrap();
    let current = p.old.files_show(&id).unwrap().file.current;
    let version = p.old.version(&id, &current).unwrap();
    let digest = &version.chunks[0].digest;
    std::fs::remove_file(
        p.old
            .files_blobs_dir()
            .unwrap()
            .join(&digest[..2])
            .join(digest),
    )
    .unwrap();
    assert_eq!(library::entries(&mut p.old), Err(Error::RecoveryIncomplete));
}

#[tokio::test]
async fn a_file_transfer_reports_changes_it_does_not_sync() {
    let mut p = pair();
    p.old
        .vault_create(
            "note",
            &serde_json::from_value(json!({"title": "not synced"})).unwrap(),
        )
        .unwrap();
    let (sent, got) = transfer(&mut p, [1; 16], &Category::ALL, None).await;
    assert_eq!(sent.unwrap().pending_sync, 1);
    assert_eq!(got.unwrap().pending_sync, 1);
}

#[tokio::test]
async fn a_listening_session_outlasts_a_connection_that_fails() {
    let mut p = pair();
    let (from, to) = (id(&p.old), id(&p.new));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let session = session_id();
    let (sent, got) = (Mutex::new(Report::default()), Mutex::new(Report::default()));
    let (old, new) = (p.old.hold(), p.new.hold());
    let sender = async {
        // A connection that sends no hello fails on its own.
        let mut stranger = tokio::net::TcpStream::connect(address).await.unwrap();
        tokio::io::AsyncWriteExt::write_all(&mut stranger, &[0xff; 8])
            .await
            .unwrap();
        while got.lock().unwrap().last_error.is_none() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert_eq!(
            got.lock().unwrap().last_error.as_deref(),
            Some("QUOTA_EXCEEDED")
        );
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        send(&old, &p.devices, stream, to, session, &sent).await
    };
    let (received, sent) = tokio::join!(
        serve(&new, &p.devices, &listener, session, &Category::ALL, &got),
        sender
    );
    let (received, sent) = (received.unwrap(), sent.unwrap());
    assert_eq!((received.records, received.peer), (9, Some(from)));
    assert_eq!((sent.records, sent.peer), (9, Some(to)));
    assert_eq!(*got.lock().unwrap(), received);
    let view = serde_json::to_value(&received).unwrap();
    assert_eq!(view["peer"], hex::encode(*from));
}

#[test]
fn frames_open_only_in_place() {
    let (key, session) = ([7; 32], [1; 16]);
    let body = Body::Want {
        records: vec!["lib-file:x".into()],
        blobs: vec![],
    };
    let sealed = wire::seal(&key, &session, SENDER, 3, &body).unwrap();
    assert_eq!(wire::open(&key, &session, SENDER, 3, &sealed), Ok(body));
    // Changed, replayed or reordered, reflected to the other direction, or
    // moved to another session: none opens.
    let mut changed = sealed.clone();
    changed[5] ^= 1;
    for (direction, sequence, session, sealed) in [
        (SENDER, 3, session, &changed),
        (SENDER, 4, session, &sealed),
        (SENDER, 2, session, &sealed),
        (RECEIVER, 3, session, &sealed),
        (SENDER, 3, [2; 16], &sealed),
    ] {
        assert_eq!(
            wire::open(&key, &session, direction, sequence, sealed),
            Err(Error::IntegrityFailed)
        );
    }
    let large = Body::Blob {
        digest: Hash::new([0; 32]),
        bytes: vec![0; MAX_BODY].into(),
    };
    assert_eq!(
        wire::seal(&key, &session, SENDER, 0, &large),
        Err(Error::QuotaExceeded)
    );
}

/// A hello from `from` to `to`'s session, changed before it is signed.
fn hello(
    from: &Client,
    to: &Client,
    session: SessionId,
    change: impl FnOnce(&mut Header),
) -> Hello {
    let account = from.account().unwrap().id;
    let (me, them) = (id(from), id(to));
    let sealed = hpke_seal_info(
        &to.workspace.meta.hpke_pub,
        &[3; 32],
        &info(&account, &me, &them, &session),
    )
    .unwrap();
    let mut header = Header {
        format: FORMAT,
        environment: from.workspace.meta.environment.clone(),
        account,
        from: me,
        to: them,
        session_id: ByteArray::new(session),
        enc: sealed.enc.into(),
        key: sealed.ciphertext.into(),
        manifest: manifest_digest(&[]),
    };
    change(&mut header);
    let signature = Ed25519Signature::new(sign(
        &from.keys().unwrap().signing,
        crate::codec::digest(DOMAIN, &header).as_slice(),
    ));
    Hello { header, signature }
}

async fn refused(p: &mut Pair, session: SessionId, hello: &Hello) -> Error {
    let (mut a, b) = duplex(4096);
    wire::write_hello(&mut a, hello).await.unwrap();
    let report = Mutex::new(Report::default());
    receive(
        &p.new.hold(),
        &p.devices,
        b,
        session,
        &Category::ALL,
        &report,
    )
    .await
    .unwrap_err()
}

#[tokio::test]
async fn hellos_and_manifests_that_do_not_fit_are_refused() {
    let mut p = pair();
    let session = [5; 16];
    let other_account = AccountId([7; 12]);
    for change in [
        Box::new(move |h: &mut Header| h.account = other_account) as Box<dyn FnOnce(&mut Header)>,
        Box::new(|h| h.environment = "production".into()),
        Box::new(|h| h.to = Hash::new([9; 32])),
        Box::new(|h| h.session_id = ByteArray::new([6; 16])),
        Box::new(|h| h.format = 2),
        Box::new(|h| h.key[0] ^= 1),
    ] {
        let forged = hello(&p.old, &p.new, session, change);
        assert_eq!(
            refused(&mut p, session, &forged).await,
            Error::IntegrityFailed
        );
    }
    // A header changed after it was signed.
    let mut forged = hello(&p.old, &p.new, session, |_| {});
    forged.header.manifest = Hash::new([1; 32]);
    assert_eq!(
        refused(&mut p, session, &forged).await,
        Error::IntegrityFailed
    );
    // A device that is no longer active.
    let valid = hello(&p.old, &p.new, session, |_| {});
    p.devices.revoke(&id(&p.old));
    assert_eq!(
        refused(&mut p, session, &valid).await,
        Error::DeviceNotApproved
    );
    p.devices.0.lock().unwrap().insert(
        id(&p.old),
        p.old.workspace.device_input(ControllerRole::Administrator),
    );

    // A manifest other than the one the header names.
    let (mut a, b) = duplex(64 * 1024);
    wire::write_hello(&mut a, &valid).await.unwrap();
    let report = Mutex::new(Report::default());
    let new = p.new.hold();
    let forger = async {
        let mut channel = Channel::new(a, Zeroizing::new([3; 32]), session, SENDER);
        assert_eq!(channel.recv().await.unwrap(), Body::Have { manifest: None });
        let entry = Entry {
            key: "lib-folder:x".into(),
            sha256: Hash::new([0; 32]),
            size: 1,
            blobs: vec![].into(),
            blob_bytes: 0,
        };
        let page = Body::Manifest {
            entries: vec![entry],
            last: true,
        };
        channel.send(&page).await.unwrap();
        channel.recv().await
    };
    let (received, answer) = tokio::join!(
        receive(&new, &p.devices, b, session, &Category::ALL, &report),
        forger
    );
    assert_eq!(received.unwrap_err(), Error::IntegrityFailed);
    assert_eq!(
        answer.unwrap(),
        Body::Abort {
            code: "INTEGRITY_FAILED".into()
        }
    );
}

#[tokio::test]
async fn a_sender_serves_only_its_manifest() {
    let mut p = pair();
    p.old
        .workspace
        .put_private(p.old.keys().unwrap(), "approval:x", &1u8)
        .unwrap();
    let session = [7; 16];
    let (a, mut b) = duplex(4 * 1024 * 1024);
    let to = id(&p.new);
    let report = Mutex::new(Report::default());
    let old = p.old.hold();
    let seed = Zeroizing::new(*p.new.keys().unwrap().hpke);
    let asker = async {
        let hello = wire::read_hello(&mut b).await.unwrap();
        let header = hello.header;
        let envelope = HpkeEnvelope {
            enc: header.enc.to_vec(),
            ciphertext: header.key.to_vec(),
        };
        let opened = hpke_open_info(
            &seed,
            &envelope,
            &info(&header.account, &header.from, &to, &session),
        )
        .unwrap();
        let key: [u8; 32] = opened.as_slice().try_into().unwrap();
        let mut channel = Channel::new(b, Zeroizing::new(key), session, RECEIVER);
        channel.send(&Body::Have { manifest: None }).await.unwrap();
        while let Body::Manifest { last: false, .. } = channel.recv().await.unwrap() {}
        let want = Body::Want {
            records: vec!["approval:x".into()],
            blobs: vec![],
        };
        channel.send(&want).await.unwrap();
        channel.recv().await
    };
    let (sent, answer) = tokio::join!(send(&old, &p.devices, a, to, session, &report), asker);
    assert_eq!(sent.unwrap_err(), Error::Forbidden);
    assert_eq!(
        answer.unwrap(),
        Body::Abort {
            code: "FORBIDDEN".into()
        }
    );
}

#[tokio::test]
async fn a_broken_connection_resumes_where_it_stopped() {
    let mut p = pair();
    let session = [3; 16];
    let (sent, got) = transfer(&mut p, session, &Category::ALL, Some(CHUNK * 3 / 2)).await;
    assert!(matches!(sent, Err(Error::Unavailable(_))));
    assert!(matches!(got, Err(Error::Unavailable(_))));
    let first = p.new.files_blobs_dir().map(|d| d.exists()).unwrap();
    assert!(first);
    // The new connection names the kept manifest and fetches only the rest.
    let (sent, got) = transfer(&mut p, session, &Category::ALL, None).await;
    let (sent, got) = (sent.unwrap(), got.unwrap());
    assert!(got.blobs > 0 && got.bytes < got.total_bytes);
    assert_eq!(sent.blobs, got.blobs);
    assert_eq!(library(&mut p.new), library(&mut p.old));
}

#[cfg(unix)]
#[tokio::test]
async fn a_full_disk_stops_before_a_version_without_its_chunks() {
    use std::os::unix::fs::PermissionsExt;
    let mut p = pair();
    let blobs = p.new.files_blobs_dir().unwrap();
    std::fs::create_dir_all(&blobs).unwrap();
    std::fs::set_permissions(&blobs, std::fs::Permissions::from_mode(0o500)).unwrap();
    let (_, got) = transfer(&mut p, [4; 16], &Category::ALL, None).await;
    assert!(matches!(got, Err(Error::Unavailable(_))));
    // Folders need no chunks; no version or file is here without them.
    assert_eq!(p.new.workspace.store.sealed_keys(FOLDER).unwrap().len(), 2);
    assert!(
        p.new
            .workspace
            .store
            .sealed_keys(VERSION)
            .unwrap()
            .is_empty()
    );
    std::fs::set_permissions(&blobs, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (_, got) = transfer(&mut p, [4; 16], &Category::ALL, None).await;
    assert_eq!(got.unwrap().kept, 2);
    assert_eq!(library(&mut p.new), library(&mut p.old));
}

#[tokio::test]
async fn records_here_stay_and_files_gain_the_versions_that_arrive() {
    let mut p = pair();
    transfer(&mut p, [1; 16], &Category::ALL, None)
        .await
        .1
        .unwrap();
    let docs = p.old.files_folder_by_path("docs").unwrap();
    let a = p.old.files_find(&docs, "a.bin").unwrap().unwrap();
    let b = p.old.files_find(ROOT, "b.txt").unwrap().unwrap();
    // The receiver renames and trashes; the sender adds a version and a file.
    p.new.files_rename(&a, "mine.bin").unwrap();
    p.new.files_trash(std::slice::from_ref(&b)).unwrap();
    let files = TempDir::new();
    import(&mut p.old, &files, &docs, "a.bin", &pattern(77, 3));
    import(&mut p.old, &files, &docs, "c.txt", b"new");
    let got = transfer(&mut p, [2; 16], &Category::ALL, None)
        .await
        .1
        .unwrap();
    // The new version and file, and both their records.
    assert_eq!(got.records, 4);
    let shown = p.new.files_show(&a).unwrap();
    assert_eq!(shown.file.name, "mine.bin");
    assert_eq!(shown.file.versions, 3);
    assert_eq!(p.new.files_list(ROOT, true).unwrap().files.len(), 2);
    assert!(p.new.files_find(&docs, "c.txt").unwrap().is_some());
    // Only the chosen categories move; none chosen moves nothing.
    let got = transfer(&mut p, [3; 16], &[], None).await.1.unwrap();
    assert_eq!((got.entries, got.records, got.blobs), (0, 0, 0));
}

#[test]
fn every_record_prefix_is_classified() {
    let mut found = BTreeSet::new();
    let mut dirs = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            // `const NAME: &str = "prefix:";`
            for line in text.lines() {
                let Some((_, rest)) = line.split_once("const ") else {
                    continue;
                };
                let Some((_, value)) = rest.split_once(": &str = \"") else {
                    continue;
                };
                let Some(value) = value.strip_suffix("\";") else {
                    continue;
                };
                if value.ends_with(':')
                    && value.len() > 1
                    && value[..value.len() - 1]
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b == b'-')
                {
                    found.insert(value.to_string());
                }
            }
        }
    }
    let listed: BTreeSet<String> = PREFIXES.iter().map(|(p, _)| p.to_string()).collect();
    assert_eq!(found, listed);
    assert_eq!(
        classify("lib-version:a:b"),
        Class::Transfer(Category::Library)
    );
    assert_eq!(classify("channel-msg:x"), Class::Stays);
    assert_eq!(classify("something-new:x"), Class::Stays);
}

fn seed(label: &str) -> [u8; 32] {
    *sha256(format!("transfer-v1:{label}").as_bytes())
}

fn h(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

/// The vector file: every encoding and seal, with fixed keys.
fn build() -> Json {
    let (signing, hpke) = (seed("sender-signing"), seed("receiver-hpke"));
    let session: SessionId = seed("session")[..16].try_into().unwrap();
    let key = seed("session-key");
    let account = account().id;
    let (from, to) = (Hash::new(seed("from")), Hash::new(seed("to")));
    let entries = [
        Entry {
            key: "lib-folder:f1".into(),
            sha256: Hash::new(seed("folder")),
            size: 61,
            blobs: vec![].into(),
            blob_bytes: 0,
        },
        Entry {
            key: "lib-version:f2:v1".into(),
            sha256: Hash::new(seed("version")),
            size: 412,
            blobs: [seed("chunk-0"), seed("chunk-1")].concat().into(),
            blob_bytes: 1_048_592 + 377,
        },
    ];
    let manifest = manifest_digest(&entries.iter().collect::<Vec<_>>());
    let info = info(&account, &from, &to, &session);
    let wrapped = crate::crypto::hpke_seal_with(
        &hpke::OpModeS::Base,
        &hpke_public(&hpke),
        &key,
        &info,
        &mut Replay(seed("ekm").to_vec()),
    )
    .unwrap();
    let header = Header {
        format: FORMAT,
        environment: "local".into(),
        account,
        from,
        to,
        session_id: ByteArray::new(session),
        enc: wrapped.enc.clone().into(),
        key: wrapped.ciphertext.clone().into(),
        manifest,
    };
    let signed = crate::codec::digest(DOMAIN, &header);
    let hello = Hello {
        signature: Ed25519Signature::new(sign(&signing, signed.as_slice())),
        header,
    };
    let frames = [
        (RECEIVER, 0, Body::Have { manifest: None }),
        (
            SENDER,
            0,
            Body::Manifest {
                entries: entries.to_vec(),
                last: true,
            },
        ),
        (
            RECEIVER,
            1,
            Body::Want {
                records: vec![],
                blobs: vec![Hash::new(seed("chunk-0"))],
            },
        ),
        (
            SENDER,
            1,
            Body::Blob {
                digest: Hash::new(seed("chunk-0")),
                bytes: b"sealed chunk".to_vec().into(),
            },
        ),
        (
            RECEIVER,
            2,
            Body::Want {
                records: vec!["lib-folder:f1".into()],
                blobs: vec![],
            },
        ),
        (
            SENDER,
            2,
            Body::Record {
                key: "lib-folder:f1".into(),
                value: canonical(&("folder", 1u8)).into(),
            },
        ),
        (
            RECEIVER,
            3,
            Body::Done {
                categories: vec![Category::Library],
                committed: manifest,
            },
        ),
        (SENDER, 3, Body::Ack { pending_sync: 0 }),
        (
            SENDER,
            4,
            Body::Abort {
                code: "DEVICE_NOT_APPROVED".into(),
            },
        ),
    ];
    json!({
        "description": "dmsg-device-transfer/1 byte rules with test-only keys. The HPKE ephemeral key comes from the given ekm. Readers rebuild every encoding and digest, verify the signature, open the session key with the receiver's HPKE seed and open every frame.",
        "generator": "dmsg_client transfer::tests::transfer_vectors_match (DMSG_WRITE_TRANSFER_VECTORS=1)",
        "keys": {
            "sender_signing_seed_hex": h(&signing),
            "sender_signing_public_hex": h(&signing_public(&signing)),
            "receiver_hpke_seed_hex": h(&hpke),
            "receiver_hpke_public_hex": h(&hpke_public(&hpke)),
            "session_key_hex": h(&key),
            "ekm_hex": h(&seed("ekm")),
        },
        "session_id_hex": h(&session),
        "advert": advert(&session),
        "manifest": {
            "entries_cbor_hex": entries.iter().map(|e| h(&canonical(e))).collect::<Vec<_>>(),
            "digest_hex": h(manifest.as_slice()),
        },
        "info_hex": h(&info),
        "hello": {
            "header_cbor_hex": h(&canonical(&hello.header)),
            "signed_digest_hex": h(signed.as_slice()),
            "signature_hex": h(hello.signature.as_slice()),
            "frame_hex": h(&[&(canonical(&hello).len() as u32).to_be_bytes()[..], &canonical(&hello)].concat()),
        },
        "frames": frames.iter().map(|(direction, sequence, body)| {
            let sealed = wire::seal(&key, &session, *direction, *sequence, body).unwrap();
            json!({
                "direction": direction,
                "sequence": sequence,
                "body_cbor_hex": h(&canonical(body)),
                "aad_hex": h(&canonical(&(DOMAIN, serde_bytes::Bytes::new(&session), direction, sequence))),
                "sealed_hex": h(&sealed),
            })
        }).collect::<Vec<_>>(),
    })
}

#[test]
fn transfer_vectors_match() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(VECTORS);
    let mut text = serde_json::to_string_pretty(&build()).unwrap();
    text.push('\n');
    if std::env::var_os("DMSG_WRITE_TRANSFER_VECTORS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        text,
        "rerun with DMSG_WRITE_TRANSFER_VECTORS=1"
    );
}

#[test]
fn the_vectors_verify_and_open() {
    let v = build();
    let unhex = |p: &str| hex::decode(v.pointer(p).unwrap().as_str().unwrap()).unwrap();
    let hello: Hello = crate::codec::decode(&unhex("/hello/frame_hex")[4..], 4096).unwrap();
    let signing = Hash::new(unhex("/keys/sender_signing_public_hex").try_into().unwrap());
    dmsg_protocol::verify(
        &signing,
        crate::codec::digest(DOMAIN, &hello.header).as_slice(),
        hello.signature.as_slice(),
    )
    .unwrap();
    let header = &hello.header;
    let seed: [u8; 32] = unhex("/keys/receiver_hpke_seed_hex").try_into().unwrap();
    let envelope = HpkeEnvelope {
        enc: header.enc.to_vec(),
        ciphertext: header.key.to_vec(),
    };
    let session = *header.session_id;
    let key = hpke_open_info(
        &seed,
        &envelope,
        &info(&header.account, &header.from, &header.to, &session),
    )
    .unwrap();
    assert_eq!(key.as_slice(), unhex("/keys/session_key_hex"));
    let key: [u8; 32] = key.as_slice().try_into().unwrap();
    for frame in v["frames"].as_array().unwrap() {
        let body = wire::open(
            &key,
            &session,
            frame["direction"].as_u64().unwrap() as u8,
            frame["sequence"].as_u64().unwrap(),
            &hex::decode(frame["sealed_hex"].as_str().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(h(&canonical(&body)), frame["body_cbor_hex"]);
    }
}
