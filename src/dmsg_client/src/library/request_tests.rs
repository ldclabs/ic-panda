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
