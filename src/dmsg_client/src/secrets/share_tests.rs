use super::*;
use crate::{
    approval::Decision,
    testing::{TempDir, unlocked},
    vault::{Changes, FieldChange},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

fn fixture() -> (TempDir, Client, String) {
    let dir = TempDir::new();
    let mut c = unlocked(&dir.0, false, None);
    let entry = c
        .vault_create(
            "api",
            &Changes {
                title: Some("Snapshot fixture".into()),
                set: Some(vec![FieldChange {
                    id: Some("credential".into()),
                    value: Some("SECRET_SNAPSHOT_ONLY_6c9d".into()),
                    ..Default::default()
                }]),
                ..Default::default()
            },
        )
        .unwrap();
    let prepared = c
        .secret_share_prepare(ShareInput {
            item_id: entry.id,
            revision: entry.revision,
            field_ids: vec!["credential".into()],
            include_notes: false,
            attachments: vec![],
            expires_in_ms: None,
            max_download_sessions: None,
        })
        .unwrap();
    let view = c.secret_share_save_preparation(prepared).unwrap();
    (dir, c, view["id"].as_str().unwrap().into())
}

#[test]
fn preparation_budget_rejects_before_persisting_an_oversized_job() {
    let (_dir, mut c, id) = fixture();
    let job = c.share_job(&id).unwrap();
    let before = c.secret_shares().unwrap()["shares"]
        .as_array()
        .unwrap()
        .len();
    let prepared = c.secret_share_prepare(job.plan.input.clone()).unwrap();
    assert!(matches!(
        prepared.limit_size(job.plan.plain_bytes - 1),
        Err(Error::QuotaExceeded)
    ));
    assert_eq!(
        c.secret_shares().unwrap()["shares"]
            .as_array()
            .unwrap()
            .len(),
        before
    );
    let prepared = c
        .secret_share_prepare(job.plan.input.clone())
        .unwrap()
        .limit_size(job.plan.plain_bytes)
        .unwrap();
    assert!(c.secret_share_save_preparation(prepared).is_ok());
}

fn assert_no_fixture_plaintext(path: &std::path::Path) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            assert_no_fixture_plaintext(&path);
        } else if path.is_file() {
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !bytes
                    .windows(b"SECRET_SNAPSHOT_ONLY_6c9d".len())
                    .any(|s| s == b"SECRET_SNAPSHOT_ONLY_6c9d"),
                "plaintext persisted in {}",
                path.display()
            );
        }
    }
}

#[tokio::test]
async fn snapshot_scope_and_interruption_keep_ids_and_require_fresh_approval() {
    let (dir, mut c, id) = fixture();
    assert_no_fixture_plaintext(&dir.0);
    let original = c.share_job(&id).unwrap();
    let manifest = c.snapshot(&original.plan).unwrap();
    assert_eq!(manifest.item.fields.len(), 1);
    assert_eq!(manifest.item.fields[0].value, "SECRET_SNAPSHOT_ONLY_6c9d");
    assert!(
        !serde_json::to_string(&original.plan)
            .unwrap()
            .contains("SECRET_SNAPSHOT_ONLY")
    );
    let r = c.secret_share_request("gui", &id).unwrap();
    let request = r["id"].as_str().unwrap();
    let run = c
        .approval_decide(request, r["digest"].as_str().unwrap(), true, "gui", false)
        .await
        .unwrap();
    assert!(matches!(run, Decision::SecretShare(_)));
    c.running.clear();
    assert_eq!(c.approvals_reconcile_others().await.unwrap(), 1);
    assert_eq!(
        c.secret_share_get(&id, false).unwrap()["state"],
        "needs_approval"
    );
    let retry = c.secret_share_request("gui", &id).unwrap();
    assert_ne!(retry["id"], r["id"]);
    let resumed = c.share_job(&id).unwrap();
    assert_eq!(resumed.plan, original.plan);
    assert_eq!(resumed.key, original.key);
    c.vault_edit(
        &original.plan.input.item_id,
        None,
        &Changes {
            title: Some("changed".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let _ = c
        .approval_decide(
            retry["id"].as_str().unwrap(),
            retry["digest"].as_str().unwrap(),
            true,
            "gui",
            false,
        )
        .await;
    assert_eq!(
        c.approval(retry["id"].as_str().unwrap()).unwrap()["state"],
        "failed"
    );
}

#[tokio::test]
async fn a_live_upload_step_keeps_the_reconciler_away_past_its_idle_window() {
    let (_dir, mut c, id) = fixture();
    let r = c.secret_share_request("gui", &id).unwrap();
    let request = r["id"].as_str().unwrap().to_owned();
    let run = c
        .approval_decide(&request, r["digest"].as_str().unwrap(), true, "gui", false)
        .await
        .unwrap();
    assert!(matches!(run, Decision::SecretShare(_)));
    // Approved six minutes ago; a step of the same run just made progress.
    c.running
        .insert(request.clone(), now_ms() - 6 * dmsg_types::MINUTE);
    let job = c.share_job(&id).unwrap();
    c.share_step(&job, now_ms()).unwrap();
    assert_eq!(c.approvals_reconcile_others().await.unwrap(), 0);
    assert_eq!(
        c.secret_share_get(&id, false).unwrap()["state"],
        "encrypting"
    );
    assert_eq!(c.approval(&request).unwrap()["state"], "executing");
}

struct Shared(Arc<Mutex<Client>>);
impl Hold for Shared {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        step(&mut self.0.lock().unwrap())
    }
}

#[tokio::test]
async fn lost_publication_response_queries_original_id_without_create_replay() {
    lost_publication(false).await;
    lost_publication(true).await;
}

async fn lost_publication(channel: bool) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (_dir, mut c, id) = fixture();
    let ledger = if channel {
        let (channel_id, ledger) = crate::channel::tests::active_channel(&mut c);
        let mut job = c.share_job(&id).unwrap();
        job.plan.channel = Some(c.snapshot_channel_target(&channel_id).unwrap());
        c.save_share_job(&job).unwrap();
        Some(ledger)
    } else {
        None
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    crate::testing::relay_to(
        &mut c,
        &format!("http://{}", listener.local_addr().unwrap()),
    );
    crate::testing::warm(&mut c);
    let r = c.secret_share_request("gui", &id).unwrap();
    let run = match c
        .approval_decide(
            r["id"].as_str().unwrap(),
            r["digest"].as_str().unwrap(),
            true,
            "gui",
            false,
        )
        .await
        .unwrap()
    {
        Decision::SecretShare(run) => *run,
        _ => panic!("share"),
    };
    let shared = Arc::new(Mutex::new(c));
    let h = Shared(shared.clone());
    let source = shared.clone();
    let job_id = id.clone();
    let creates = Arc::new(AtomicUsize::new(0));
    let count = creates.clone();
    let receipt = Arc::new(Mutex::new(None::<Value>));
    let received_receipt = receipt.clone();
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut block = [0u8; 4096];
            let head = loop {
                let n = socket.read(&mut block).await.unwrap();
                if n == 0 {
                    break None;
                }
                bytes.extend_from_slice(&block[..n]);
                assert!(bytes.len() < 1024 * 1024);
                if let Some(at) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
                    break Some(at + 4);
                }
            };
            let Some(head) = head else { continue };
            let header = String::from_utf8_lossy(&bytes[..head]).into_owned();
            let length = header
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|v| v.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            while bytes.len() < head + length {
                let n = socket.read(&mut block).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&block[..n]);
            }
            assert!(
                !bytes
                    .windows(b"SECRET_SNAPSHOT_ONLY_6c9d".len())
                    .any(|v| v == b"SECRET_SNAPSHOT_ONLY_6c9d")
            );
            let job = source.lock().unwrap().share_job(&job_id).unwrap();
            let path = header.lines().next().unwrap();
            let (status, data) = if path.starts_with("GET ") && path.contains("/uploads/") {
                (200, json!({"status":"committed","plan":job.upload}))
            } else if path.starts_with("GET ") && path.contains("/operations/") {
                (
                    200,
                    json!({"found":true,"result":received_receipt.lock().unwrap().clone().unwrap()}),
                )
            } else if path.starts_with("GET ") && path.contains("/sync?") {
                (
                    200,
                    json!({"head":ledger.as_ref().unwrap().head,"entries":[],"skipped":[],"next_seq":0,"latest_seq":0}),
                )
            } else if path.starts_with("GET ")
                && ledger
                    .as_ref()
                    .is_some_and(|l| path.contains(&format!("/channels/{} ", l.channel_id)))
            {
                (200, json!(ledger))
            } else if path.starts_with("POST ") && path.contains("/messages ") {
                count.fetch_add(1, Ordering::SeqCst);
                let body: Value =
                    crate::codec::decode(&bytes[head..head + length], 1024 * 1024).unwrap();
                let cose = body["signed"]["cose_sign1"]
                    .as_str()
                    .or_else(|| body["cose_sign1"].as_str())
                    .expect("signed request");
                *received_receipt.lock().unwrap() = Some(
                    json!({"seq":1,"stored_at":now_ms(),"digest":crate::codec::hex_sha256(&crate::codec::unb64(cose).unwrap())}),
                );
                drop(socket);
                continue;
            } else if path.starts_with("POST ") && path.contains("/shares ") {
                count.fetch_add(1, Ordering::SeqCst);
                drop(socket);
                continue;
            } else if path.starts_with("GET ") && path.contains("/shares?") {
                (
                    200,
                    json!({"entries":[{"share_id":job.plan.share_id,"expires_at":job.plan.expires_at,"max_downloads":1,"password":false,"created_at":job.plan.created_at,"revoked_at":null,"downloads":0}],"end":true,"next_cursor":0}),
                )
            } else {
                panic!("unexpected mock request {path}")
            };
            let body = json!({"ok":true,"data":data}).to_string();
            socket.write_all(format!("HTTP/1.1 {status} Test\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        }
    });
    let result = run.run(&h).await.unwrap();
    assert_eq!(result["state"], "unknown", "{result}");
    assert_eq!(creates.load(Ordering::SeqCst), 1);
    h.with(|c| {
        assert!(c.secret_share_request("gui", &id).is_err());
        Ok(())
    })
    .await
    .unwrap();
    for _ in 0..2 {
        assert_eq!(reconcile(&h, &id).await.unwrap()["state"], "published");
    }
    assert_eq!(creates.load(Ordering::SeqCst), 1);
    let text = h
        .with(|c| Ok(c.secret_share_get(&id, false)?.to_string()))
        .await
        .unwrap();
    assert!(!text.contains("SECRET_SNAPSHOT_ONLY"));
    assert!(!text.contains("#"));
    h.with(|c| {
        if channel {
            assert!(c.secret_share_cancel(&id).is_err());
            assert!(
                c.channel_outbox(&ledger_for_job(c, &id))
                    .unwrap()
                    .is_empty()
            );
            assert!(c.secret_share_get(&id, true)?["link"].is_null());
            return Ok(());
        }
        c.secret_share_cancel(&id)?;
        assert!(c.secret_share_get(&id, true)?["link"].is_null());
        c.share_settle(&id, "revoked", None, State::Done)?;
        assert_eq!(
            c.share_settle(&id, "published", None, State::Done)?["state"],
            "revoked"
        );
        Ok(())
    })
    .await
    .unwrap();
    server.abort();
}

fn ledger_for_job(c: &Client, id: &str) -> String {
    c.share_job(id)
        .unwrap()
        .plan
        .channel
        .as_ref()
        .unwrap()
        .channel_id
        .clone()
}

#[tokio::test]
async fn channel_snapshot_approval_is_fixed_to_membership_and_never_wraps_a_link() {
    let (_dir, mut c, id) = fixture();
    let (channel, _) = crate::channel::tests::active_channel(&mut c);
    let target = c.snapshot_channel_target(&channel).unwrap();
    let mut job = c.share_job(&id).unwrap();
    job.plan.channel = Some(target.clone());
    c.save_share_job(&job).unwrap();
    let request = c.secret_share_request("gui", &id).unwrap();
    assert!(request["detail"]["expires_at"].is_null());
    assert!(request["detail"]["max_download_sessions"].is_null());
    assert!(
        request["detail"]
            .to_string()
            .contains("including later joins")
    );
    let mut stale = target;
    stale.control_head = "ff".repeat(32);
    assert_eq!(
        c.snapshot_channel_check(&stale).unwrap_err(),
        Error::PolicyStale
    );
    job.plan.channel = Some(stale);
    c.save_share_job(&job).unwrap();
    assert!(c.secret_share_request("gui", &id).is_err());
    let result = c
        .approval_decide(
            request["id"].as_str().unwrap(),
            request["digest"].as_str().unwrap(),
            true,
            "gui",
            false,
        )
        .await;
    assert!(!matches!(result, Ok(Decision::SecretShare(_))));
    assert!(c.channel_outbox(&channel).unwrap().is_empty());
}
