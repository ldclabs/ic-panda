use super::*;
use crate::codec::{b64, canonical, sha256};
use crate::crypto::tests::Replay;
use crate::crypto::{hpke_public, hpke_seal, hpke_seal_with, seal_with_nonce};
use crate::files::{CHUNK, FileChunk, chunk_aad, nonce};
use crate::upload::ChunkRef;
use crate::vectors::{bytes, key, load, text};
use hpke::OpModeS;
use serde_json::json;
use std::path::Path;

/// `receipt_key`: only a sender's page uses it; the formats stay in the vector.
fn receipt_key(page_key: &Key, account: &str, request_id: &str) -> Zeroizing<Key> {
    derive(page_key, &("dmsg/request-receipt/1", account, request_id))
}

/// A file sealed as a browser does, with its plan.
fn sealed_file(
    seed: &Key,
    account: &str,
    request_id: &str,
    plain: &[u8],
    edit: impl FnOnce(&mut RequestFile),
) -> (UploadPlan, Vec<u8>) {
    let (id, version, upload) = ("6a".repeat(32), "6b".repeat(32), "6c".repeat(32));
    let file_key: Key = [9; 32];
    let count = chunk_count(plain.len() as u64);
    let mut chunks = Vec::new();
    let mut refs = Vec::new();
    for (i, part) in plain.chunks(CHUNK).enumerate() {
        let aad = chunk_aad(&id, &version, i as u64, count, part.len() as u64);
        let sealed = seal_with_nonce(&file_key, part, &aad, &nonce(i as u64));
        let digest = hex::encode(*sha256(&sealed));
        chunks.push(FileChunk {
            id: format!("{id}:{version}:{i}"),
            digest: digest.clone(),
            size: part.len() as u64,
        });
        refs.push(ChunkRef {
            digest,
            size: sealed.len() as u64,
        });
    }
    let mut opened = RequestFile {
        format: REQUEST_FILE_FORMAT.into(),
        request_id: request_id.into(),
        upload_id: upload.clone(),
        file: FileManifest {
            id: id.clone(),
            version: version.clone(),
            name: "report.pdf".into(),
            mime: "application/pdf".into(),
            size: plain.len() as u64,
            sha256: hex::encode(*sha256(plain)),
            chunks,
            key: b64(&file_key),
        },
        sender: Some(RequestSender {
            name: "Ada".into(),
            note: "As agreed".into(),
        }),
        replaces: None,
        sent_at: 1_760_000_000_000,
    };
    edit(&mut opened);
    let envelope = hpke_seal(
        &hpke_public(seed),
        &canonical(&opened),
        &request_file_info(account, request_id, &upload, &id, &version),
    )
    .unwrap();
    let sealed = [envelope.enc, envelope.ciphertext].concat();
    let plan = UploadPlan {
        management: None,
        upload_id: upload,
        object_id: id,
        version_id: version,
        root_generation: 0,
        epoch: 0,
        control_head: None,
        kind: REQUEST_FILE.into(),
        chunks: refs,
        manifest_digest: hex::encode(*sha256(&sealed)),
        manifest_size: sealed.len() as u64,
        expires_at: 1_760_000_300_000,
    };
    (plan, sealed)
}

#[test]
fn opens_a_file_only_as_its_plan_describes_it() {
    let seed: Key = [3; 32];
    let (account, request) = ("040g2081040g2081040g", "7a".repeat(32));
    let plain = vec![7u8; CHUNK + 10];
    let (plan, sealed) = sealed_file(&seed, account, &request, &plain, |_| {});
    let opened = open_request_file(&seed, account, &request, &plan, &sealed).unwrap();
    assert_eq!(opened.file.size, plain.len() as u64);
    assert_eq!(opened.sender.unwrap().name, "Ada");

    let integrity =
        |result: Result<RequestFile>| assert_eq!(result.err(), Some(Error::IntegrityFailed));
    // Another seed, another link, another account.
    integrity(open_request_file(
        &[4; 32], account, &request, &plan, &sealed,
    ));
    integrity(open_request_file(
        &seed,
        account,
        &"7b".repeat(32),
        &plan,
        &sealed,
    ));
    integrity(open_request_file(
        &seed,
        "040g2081040g2081040h",
        &request,
        &plan,
        &sealed,
    ));
    // Another upload ID changes the info it was sealed to.
    let mut other = plan.clone();
    other.upload_id = "6d".repeat(32);
    integrity(open_request_file(&seed, account, &request, &other, &sealed));
    // A changed byte, and a plan of another kind.
    let mut flipped = sealed.clone();
    *flipped.last_mut().unwrap() ^= 1;
    integrity(open_request_file(&seed, account, &request, &plan, &flipped));
    let mut kind = plan.clone();
    kind.kind = "backup".into();
    integrity(open_request_file(&seed, account, &request, &kind, &sealed));
    // A chunk the plan does not list.
    let mut swapped = plan.clone();
    swapped.chunks[0].digest = "00".repeat(32);
    integrity(open_request_file(
        &seed, account, &request, &swapped, &sealed,
    ));
}

#[test]
fn refuses_what_a_hostile_sender_writes_in_the_manifest() {
    let seed: Key = [3; 32];
    let (account, request) = ("040g2081040g2081040g", "7a".repeat(32));
    let plain = vec![7u8; 100];
    let refused = |edit: fn(&mut RequestFile)| {
        let (plan, sealed) = sealed_file(&seed, account, &request, &plain, edit);
        assert_eq!(
            open_request_file(&seed, account, &request, &plan, &sealed).err(),
            Some(Error::IntegrityFailed)
        );
    };
    refused(|f| f.format = "other".into());
    refused(|f| f.request_id = "7b".repeat(32));
    refused(|f| f.upload_id = "7b".repeat(32));
    refused(|f| f.file.id = "7b".repeat(32));
    refused(|f| f.file.version = "7b".repeat(32));
    refused(|f| f.file.size = 101);
    refused(|f| f.file.size = 0);
    refused(|f| f.file.size = MAX_BACKUP + 1);
    refused(|f| f.file.chunks[0].id = "x".into());
    refused(|f| f.file.chunks[0].size = 99);
    refused(|f| f.file.key = "short".into());
    refused(|f| f.replaces = Some("not an id".into()));
    refused(|f| {
        f.sender = Some(RequestSender {
            name: "a".repeat(MAX_SENDER),
            note: "b".into(),
        })
    });
}

#[test]
fn descriptor_names_its_link() {
    let page_key: Key = [5; 32];
    let (account, request) = ("040g2081040g2081040g", "7a".repeat(32));
    let descriptor = RequestDescriptor {
        format: REQUEST_FORMAT.into(),
        account: account.into(),
        request_id: request.clone(),
        hpke_pub: b64(&hpke_public(&[3; 32])),
        title: "Contract".into(),
        instructions: "PDF".into(),
    };
    let sealed = seal_descriptor(&page_key, &descriptor);
    assert_eq!(
        open_descriptor(&page_key, account, &request, &sealed).unwrap(),
        descriptor
    );
    assert_eq!(
        open_descriptor(&[6; 32], account, &request, &sealed).err(),
        Some(Error::IntegrityFailed)
    );
    assert_eq!(
        open_descriptor(&page_key, account, &"7b".repeat(32), &sealed).err(),
        Some(Error::IntegrityFailed)
    );
}

/// The file vector: everything but the `browser` section, which the page's
/// test writes (`DMSG_WRITE_CLIENT_VECTORS=1 pnpm exec vitest run src/lib/request`),
/// is reproduced here byte for byte. Its HPKE ephemeral key comes from the
/// recorded `ekm`.
#[test]
fn reproduces_the_request_vector() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/vectors/client/request-v1.json");
    let existing: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .map(|text| serde_json::from_str(&text).unwrap())
        .unwrap_or(serde_json::Value::Null);

    let account = "040g2081040g2081040g";
    let request = "7a".repeat(32);
    let page_key: Key = [0x51; 32];
    let seed: Key = [0x52; 32];
    let file_key: Key = [0x53; 32];
    let descriptor_nonce = [0x54; 12];
    let ekm = [0x55; 32];
    let receipt_nonce = [0x56; 12];
    let (id, version, upload) = ("6a".repeat(32), "6b".repeat(32), "6c".repeat(32));
    let plaintext = b"A customer's small file. ".repeat(10);

    let descriptor = RequestDescriptor {
        format: REQUEST_FORMAT.into(),
        account: account.into(),
        request_id: request.clone(),
        hpke_pub: b64(&hpke_public(&seed)),
        title: "Signed contract".into(),
        instructions: "PDF, please. 合同请用 PDF。".into(),
    };
    let descriptor_plain = canonical(&descriptor);
    let descriptor_sealed = b64(&seal_with_nonce(
        &page_key,
        &descriptor_plain,
        &descriptor_aad(account, &request),
        &descriptor_nonce,
    ));

    let token = upload_token(&page_key, account, &request);
    let receipt_key = receipt_key(&page_key, account, &request);
    let receipt_plain = canonical(&json!({
        "name": "contract.pdf",
        "size": plaintext.len(),
        "mime": "application/pdf",
        "sha256": hex::encode(*sha256(&plaintext)),
        "sent_at": 1_760_000_000_000u64,
        "replaces": null,
    }));
    let receipt_aad = ("dmsg/request-receipt/1", request.as_str(), upload.as_str());
    let receipt = b64(&seal_with_nonce(
        &receipt_key,
        &receipt_plain,
        &receipt_aad,
        &receipt_nonce,
    ));

    let aad = chunk_aad(&id, &version, 0, 1, plaintext.len() as u64);
    let sealed_chunk = seal_with_nonce(&file_key, &plaintext, &aad, &nonce(0));
    let digest = hex::encode(*sha256(&sealed_chunk));
    let file = RequestFile {
        format: REQUEST_FILE_FORMAT.into(),
        request_id: request.clone(),
        upload_id: upload.clone(),
        file: FileManifest {
            id: id.clone(),
            version: version.clone(),
            name: "contract.pdf".into(),
            mime: "application/pdf".into(),
            size: plaintext.len() as u64,
            sha256: hex::encode(*sha256(&plaintext)),
            chunks: vec![FileChunk {
                id: format!("{id}:{version}:0"),
                digest: digest.clone(),
                size: plaintext.len() as u64,
            }],
            key: b64(&file_key),
        },
        sender: Some(RequestSender {
            name: "Ada".into(),
            note: "签好了".into(),
        }),
        replaces: Some("6d".repeat(32)),
        sent_at: 1_760_000_000_000,
    };
    let file_plain = canonical(&file);
    let info = request_file_info(account, &request, &upload, &id, &version);
    let envelope = hpke_seal_with(
        &OpModeS::Base,
        &hpke_public(&seed),
        &file_plain,
        &canonical(&info),
        &mut Replay(ekm.to_vec()),
    )
    .unwrap();
    let manifest = [envelope.enc, envelope.ciphertext].concat();

    let mut vector = json!({
        "schema": 1,
        "test_only": true,
        "generator": "dmsg_client library::request tests",
        "description": "File requests: the descriptor under the page key, the upload token, the receipt, a manifest sealed to the link's HPKE key, enc || ciphertext, and its chunk. `browser` is the same manifest format sealed by the upload page, for the Rust client to open.",
        "account": account,
        "request_id": request,
        "page_key_hex": hex::encode(page_key),
        "page_key_fragment": b64(&page_key),
        "hpke_seed_hex": hex::encode(seed),
        "hpke_pub": descriptor.hpke_pub,
        "descriptor": {
            "aad_hex": hex::encode(canonical(&descriptor_aad(account, &request))),
            "plaintext_hex": hex::encode(&descriptor_plain),
            "nonce_hex": hex::encode(descriptor_nonce),
            "sealed": descriptor_sealed,
        },
        "upload_token": {
            "context_hex": hex::encode(canonical(&("dmsg/request-upload-token/1", account, &request))),
            "token": b64(&token[..]),
            "hash": token_hash(&token),
        },
        "receipt": {
            "context_hex": hex::encode(canonical(&("dmsg/request-receipt/1", account, &request))),
            "key_hex": hex::encode(&receipt_key[..]),
            "aad_hex": hex::encode(canonical(&receipt_aad)),
            "plaintext_hex": hex::encode(&receipt_plain),
            "nonce_hex": hex::encode(receipt_nonce),
            "upload_id": upload,
            "sealed": receipt,
        },
        "file": {
            "upload_id": upload,
            "object_id": id,
            "version_id": version,
            "file_key_hex": hex::encode(file_key),
            "info_hex": hex::encode(canonical(&info)),
            "plaintext_hex": hex::encode(&file_plain),
            "ekm_hex": hex::encode(ekm),
            "manifest_hex": hex::encode(&manifest),
            "manifest_digest": hex::encode(*sha256(&manifest)),
            "chunks": [{
                "index": 0,
                "plaintext_hex": hex::encode(&plaintext),
                "aad_hex": hex::encode(canonical(&aad)),
                "nonce_hex": hex::encode(nonce(0)),
                "sealed_hex": hex::encode(&sealed_chunk),
                "digest": digest,
                "size": sealed_chunk.len(),
            }],
        },
    });
    let plan = UploadPlan {
        management: None,
        upload_id: upload.clone(),
        object_id: id.clone(),
        version_id: version.clone(),
        root_generation: 0,
        epoch: 0,
        control_head: None,
        kind: REQUEST_FILE.into(),
        chunks: vec![ChunkRef {
            digest: digest.clone(),
            size: sealed_chunk.len() as u64,
        }],
        manifest_digest: hex::encode(*sha256(&manifest)),
        manifest_size: manifest.len() as u64,
        expires_at: 1_760_000_300_000,
    };
    assert_eq!(
        open_request_file(&seed, account, &request, &plan, &manifest).unwrap(),
        file
    );
    vector["browser"] = existing["browser"].clone();
    if std::env::var_os("DMSG_WRITE_CLIENT_VECTORS").is_some() {
        std::fs::write(&path, serde_json::to_string_pretty(&vector).unwrap() + "\n").unwrap();
        return;
    }
    assert_eq!(vector, existing, "rerun with DMSG_WRITE_CLIENT_VECTORS=1");
}

/// What the upload page seals, the Rust client opens.
#[test]
fn opens_a_manifest_the_browser_sealed() {
    let v = load("request-v1.json");
    let browser = &v["browser"];
    let seed = key(&v["hpke_seed_hex"]);
    let (account, request) = (text(&v["account"]), text(&v["request_id"]));
    let plan: UploadPlan = serde_json::from_value(browser["plan"].clone()).unwrap();
    let manifest = bytes(&browser["manifest_hex"]);
    let opened = open_request_file(&seed, account, request, &plan, &manifest).unwrap();
    assert_eq!(opened.file.name, text(&browser["name"]));
    assert_eq!(
        opened.sender.as_ref().unwrap().name,
        text(&browser["sender"])
    );
    // The chunk opens under the manifest's key, as the page sealed it.
    let sealed = bytes(&browser["chunk_hex"]);
    let plain = crate::crypto::open_bytes(
        &opened.file.file_key().unwrap(),
        &sealed,
        &chunk_aad(
            &opened.file.id,
            &opened.file.version,
            0,
            1,
            opened.file.size,
        ),
    )
    .unwrap();
    assert_eq!(hex::encode(*sha256(&plain)), opened.file.sha256);
    assert_eq!(*plain, bytes(&browser["plaintext_hex"]));
}

fn secrets(title: &str, label: &str) -> RequestSecrets {
    RequestSecrets {
        hpke_seed: b64(&[1; 32]),
        page_key: b64(&[2; 32]),
        label: label.into(),
        title: title.into(),
        instructions: "As PDF.".into(),
    }
}

fn remote(request_id: &str, management: ManagementEnvelope) -> RemoteLink {
    RemoteLink {
        request_id: request_id.into(),
        expires_at: 4_000_000_000_000,
        max_total_bytes: MIB,
        management,
        created_at: 1,
        revoked_at: None,
        status: "open".into(),
        files: 2,
        bytes: 9,
        last_stored_at: Some(100),
    }
}

#[test]
fn management_opens_only_for_its_link_and_while_the_root_is_here() {
    let dir = crate::testing::TempDir::new();
    let c = crate::testing::unlocked(&dir.0, false, None);
    let id = "ab".repeat(32);
    let sealed = c.seal_request_secrets(&id, &secrets("T", "L")).unwrap();
    assert_eq!(
        c.open_request_secrets(&id, &sealed).unwrap(),
        Some(secrets("T", "L"))
    );
    assert_eq!(
        c.open_request_secrets(&"cd".repeat(32), &sealed).err(),
        Some(Error::IntegrityFailed)
    );
    let later = ManagementEnvelope {
        generation: sealed.generation + 1,
        ..sealed
    };
    assert_eq!(c.open_request_secrets(&id, &later).unwrap(), None);
}

#[test]
fn a_link_reads_as_the_relay_last_said_and_keeps_its_secrets() {
    let dir = crate::testing::TempDir::new();
    let c = crate::testing::unlocked(&dir.0, false, None);
    let id = "ab".repeat(32);
    let sealed = c.seal_request_secrets(&id, &secrets("T", "L")).unwrap();
    c.keep_link(remote(&id, sealed.clone()), 50).unwrap();
    let view = &c.files_requests().unwrap()[0];
    let account = c.account().unwrap().id.to_string();
    let key = b64(&[2; 32]);
    assert!(
        view.url
            .as_deref()
            .unwrap()
            .ends_with(&format!("/r/{account}/{id}#{key}"))
    );
    assert_eq!(view.title.as_deref(), Some("T"));
    assert_eq!(view.label.as_deref(), Some("L"));
    assert_eq!(view.status, "open");
    assert!(view.unread);
    // Listing the files clears unread; a newer file brings it back.
    let mut record = c.request_record(&id).unwrap().unwrap();
    record.seen_at = Some(100);
    c.save_request(&record).unwrap();
    c.keep_link(remote(&id, sealed.clone()), 60).unwrap();
    assert!(!c.files_requests().unwrap()[0].unread);
    let newer = RemoteLink {
        last_stored_at: Some(101),
        ..remote(&id, sealed.clone())
    };
    c.keep_link(newer, 70).unwrap();
    assert!(c.files_requests().unwrap()[0].unread);
    let closed = RemoteLink {
        revoked_at: Some(80),
        ..remote(&id, sealed.clone())
    };
    c.keep_link(closed, 80).unwrap();
    assert_eq!(c.files_requests().unwrap()[0].status, "revoked");
    let lapsed = RemoteLink {
        expires_at: 1,
        ..remote(&id, sealed)
    };
    c.keep_link(lapsed, 90).unwrap();
    assert_eq!(c.files_requests().unwrap()[0].status, "expired");
    // Made on a device with a root this one lacks: listed, with no link to give.
    let other = "cd".repeat(32);
    let unknown = ManagementEnvelope {
        generation: 99,
        ciphertext: "AAAA".into(),
    };
    c.keep_link(remote(&other, unknown), 90).unwrap();
    let list = c.files_requests().unwrap();
    let shown = list.iter().find(|v| v.request_id == other).unwrap();
    assert_eq!(shown.url, None);
    assert_eq!(shown.title, None);
}

#[test]
fn a_received_file_shows_what_it_is_and_what_became_of_it() {
    let plan = UploadPlan {
        management: None,
        upload_id: "11".repeat(32),
        object_id: "22".repeat(32),
        version_id: "33".repeat(32),
        root_generation: 0,
        epoch: 0,
        control_head: None,
        kind: REQUEST_FILE.into(),
        chunks: vec![],
        manifest_digest: "44".repeat(32),
        manifest_size: 64,
        expires_at: 1,
    };
    let mut record = ReceivedRecord {
        request_id: "ab".repeat(32),
        plan,
        file: None,
        stored_at: 5,
        received_at: None,
        withdrawn_at: None,
        saved: None,
    };
    assert_eq!(record.view().state, "unreadable");
    assert_eq!(record.view().bytes, 64);
    record.withdrawn_at = Some(6);
    assert_eq!(record.view().state, "withdrawn");
    record.saved = Some(("f".into(), "v".into()));
    assert_eq!(record.view().state, "saved");
    assert_eq!(record.view().saved_file.as_deref(), Some("f"));
}

#[test]
fn drafts_and_folders_keep_to_their_bounds() {
    let draft = |title, instructions, label| RequestDraft {
        title,
        instructions,
        label,
    };
    assert!(check_draft(&draft("Contracts", "", "")).is_ok());
    let big = "x".repeat(MAX_TITLE + 1);
    assert!(check_draft(&draft("  ", "", "")).is_err());
    assert!(check_draft(&draft(&big, "", "")).is_err());
    let note = "x".repeat(MAX_INSTRUCTIONS + 1);
    assert!(check_draft(&draft("T", &note, "")).is_err());
    let label = "x".repeat(MAX_LABEL + 1);
    assert!(check_draft(&draft("T", "", &label)).is_err());
    // The longest draft still fits a descriptor.
    let page_key = [4; 32];
    let sealed = seal_descriptor(
        &page_key,
        &RequestDescriptor {
            format: REQUEST_FORMAT.into(),
            account: "aaaaa-aa".into(),
            request_id: "ab".repeat(32),
            hpke_pub: b64(&[5; 32]),
            title: "x".repeat(MAX_TITLE),
            instructions: "x".repeat(MAX_INSTRUCTIONS),
        },
    );
    assert!(sealed.len() <= MAX_DESCRIPTOR);
    // A title can't reach outside its folder, nor a label make another level.
    assert_eq!(
        default_folder("Received", &secrets("Q3/../contracts", "Ada / Lin")),
        "Received/Q3_.._contracts/Ada _ Lin"
    );
    assert_eq!(
        default_folder("Received", &secrets("Q3", "  ")),
        "Received/Q3"
    );
}

/// A file as a browser seals it, its chunks in the library's blobs.
fn in_blobs(c: &Client, name: &str, plain: &[u8]) -> FileManifest {
    let key = [7; 32];
    let (id, version) = (random_hex(), random_hex());
    let (sealed, digest) = crate::files::seal_chunk(&key, &id, &version, 0, 1, plain);
    c.blobs().unwrap().put_chunk(&digest, &sealed).unwrap();
    FileManifest {
        id: id.clone(),
        version: version.clone(),
        name: name.into(),
        mime: "text/plain".into(),
        size: plain.len() as u64,
        sha256: hex::encode(*sha256(plain)),
        chunks: vec![FileChunk {
            id: format!("{id}:{version}:0"),
            digest,
            size: plain.len() as u64,
        }],
        key: b64(&key),
    }
}

#[tokio::test]
async fn saving_keeps_senders_apart_under_the_librarys_own_key() {
    let dir = crate::testing::TempDir::new();
    let mut c = crate::testing::unlocked(&dir.0, false, None);
    let origin = || Origin::Received {
        request: "ab".repeat(32),
        upload: "cd".repeat(32),
    };
    let first = in_blobs(&c, "contract\u{202E}fdp.exe", b"from Ada");
    let second = in_blobs(&c, "contract\u{202E}fdp.exe", b"from Lin");
    let folder = c.files_mkdir_path("Received/Q3").unwrap().id;
    let h = c.cooperative_hold();
    let one = import::from_received(&h, &first, &folder, origin(), &mut |_, _| {})
        .await
        .unwrap();
    let two = import::from_received(&h, &second, &folder, origin(), &mut |_, _| {})
        .await
        .unwrap();
    assert_ne!(one.file.id, two.file.id);
    assert!(!one.file.name.contains('\u{202E}'));
    assert_ne!(one.file.name, two.file.name);
    let (.., version, manifest) = c.library_manifest(&one.file.id, None).unwrap();
    assert_eq!(version.origin, origin());
    // Encrypted anew: neither its key nor its chunks are the sender's.
    assert_ne!(manifest.key, first.key);
    assert_ne!(manifest.chunks[0].digest, first.chunks[0].digest);
    let out = dir.path("out.txt");
    c.files_export(&one.file.id, None, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"from Ada");
    // The download goes once nothing holds it; the saved copy stays.
    c.collect_blobs().unwrap();
    assert_eq!(
        c.blobs().unwrap().chunk(&first.chunks[0].digest).unwrap(),
        None
    );
    let out = dir.path("out-2.txt");
    c.files_export(&two.file.id, None, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"from Lin");
    // A file whose bytes are not what its manifest says never joins the library.
    let mut lying = in_blobs(&c, "lying.txt", b"not this");
    lying.sha256 = "00".repeat(32);
    let h = c.cooperative_hold();
    assert_eq!(
        import::from_received(&h, &lying, &folder, origin(), &mut |_, _| {})
            .await
            .err(),
        Some(Error::IntegrityFailed)
    );
    assert!(
        c.files_list(&folder, false)
            .unwrap()
            .files
            .iter()
            .all(|f| f.name != "lying.txt")
    );
}

#[tokio::test]
async fn a_failed_received_file_does_not_resume_as_another_senders_file() {
    for bytes in [b"from Ada", b"from Lin"] {
        let dir = crate::testing::TempDir::new();
        let mut c = crate::testing::unlocked(&dir.0, false, None);
        let folder = c.files_mkdir_path("Received").unwrap().id;
        let origin = |upload: &str| Origin::Received {
            request: "ab".repeat(32),
            upload: upload.repeat(32),
        };
        let mut bad = in_blobs(&c, "contract.txt", b"from Ada");
        bad.sha256 = "00".repeat(32);
        let good = in_blobs(&c, "contract.txt", bytes);
        let h = c.cooperative_hold();
        assert_eq!(
            import::from_received(&h, &bad, &folder, origin("11"), &mut |_, _| {})
                .await
                .err(),
            Some(Error::IntegrityFailed)
        );
        // Same name and size, but a different upload: neither its chunks nor
        // its provenance may come from the unfinished import above.
        let saved = import::from_received(&h, &good, &folder, origin("22"), &mut |_, _| {})
            .await
            .unwrap();
        assert!(!saved.resumed);
        let (_, version, _) = c.library_manifest(&saved.file.id, None).unwrap();
        assert_eq!(version.origin, origin("22"));
        let out = dir.path("saved.txt");
        c.files_export(&saved.file.id, None, &out).unwrap();
        assert_eq!(std::fs::read(out).unwrap(), bytes);
    }
}

#[tokio::test]
async fn saving_a_received_file_reports_bytes_including_its_short_last_chunk() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let dir = crate::testing::TempDir::new();
    let mut c = crate::testing::unlocked(&dir.0, false, None);
    crate::testing::warm(&mut c);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    crate::testing::relay_to(
        &mut c,
        &format!("http://{}", listener.local_addr().unwrap()),
    );
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 8192];
        let read = socket.read(&mut request).await.unwrap();
        assert!(String::from_utf8_lossy(&request[..read]).contains("/files/ack"));
        let body = r#"{"ok":true,"data":{"received_at":100}}"#;
        socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    });
    let request = "ab".repeat(32);
    let account = c.account().unwrap().id.to_string();
    let plain = vec![7; CHUNK + 7];
    let (plan, sealed) = sealed_file(&[1; 32], &account, &request, &plain, |_| {});
    let opened = open_request_file(&[1; 32], &account, &request, &plan, &sealed).unwrap();
    for (i, part) in plain.chunks(CHUNK).enumerate() {
        let (bytes, digest) = crate::files::seal_chunk(
            &opened.file.file_key().unwrap(),
            &opened.file.id,
            &opened.file.version,
            i as u64,
            plan.chunks.len() as u64,
            part,
        );
        c.blobs().unwrap().put_chunk(&digest, &bytes).unwrap();
    }
    let management = c.seal_request_secrets(&request, &secrets("T", "")).unwrap();
    c.keep_link(remote(&request, management), 1).unwrap();
    c.keep_received(&ReceivedRecord {
        request_id: request.clone(),
        plan: plan.clone(),
        file: Some(opened),
        stored_at: 1,
        received_at: None,
        withdrawn_at: None,
        saved: None,
    })
    .unwrap();
    let mut progress = Vec::new();
    let saved = request_save(
        &c.hold(),
        &plan.upload_id,
        None,
        "Received",
        &mut |stage, done, total| {
            progress.push((stage, done, total));
        },
    )
    .await
    .unwrap();
    server.await.unwrap();
    let size = plain.len() as u64;
    assert_eq!(
        progress,
        vec![
            ("download", size, size),
            ("save", CHUNK as u64, size),
            ("save", size, size)
        ]
    );
    let out = dir.path("received.txt");
    c.files_export(&saved.id, None, &out).unwrap();
    assert_eq!(std::fs::read(out).unwrap(), plain);
}
