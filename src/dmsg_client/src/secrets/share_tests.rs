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

struct Shared(Arc<Mutex<Client>>);
impl Hold for Shared {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        step(&mut self.0.lock().unwrap())
    }
}

#[tokio::test]
async fn lost_publication_response_queries_original_id_without_create_replay() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (_dir, mut c, id) = fixture();
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
