use super::*;
use crate::approval::{State, MAX_PENDING};
use crate::testing::{unlocked, TempDir};

fn text(text: &str) -> StatementInput {
    StatementInput::Text {
        text: text.into(),
        subject: None,
    }
}

#[test]
fn statement_requests_wait_for_a_person() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let request = client
        .attest_request("cli", text("release 1.0"), None)
        .unwrap();
    assert_eq!(request["state"], "awaiting_user");
    assert_eq!(request["source"], "cli");
    assert_eq!(request["detail"]["origin"], DMSG_ORIGIN);
    assert_eq!(
        request["detail"]["statement"]["issuer"],
        crate::testing::account().issuer
    );
    assert_eq!(client.approvals_waiting().unwrap(), 1);
    // Only known sources; a web origin comes only from the connect path.
    assert!(client.attest_request("browser", text("x"), None).is_err());
    assert!(client.attest_request("cli", text(""), None).is_err());

    // A file statement hashes the file now and checks it again on approval.
    let file = dir.path("release.bin");
    std::fs::write(&file, b"artifact").unwrap();
    let input = StatementInput::FileStatement {
        text: "I approve this build".into(),
        content_type: None,
        location: None,
        subject: None,
    };
    let request = client.attest_request("gui", input, Some(&file)).unwrap();
    assert_eq!(request["detail"]["file_checked"], true);
    assert_eq!(
        request["detail"]["statement"]["content"]["sha256"],
        hex::encode(*file_sha256(&file).unwrap())
    );
    std::fs::write(&file, b"changed").unwrap();
    assert_eq!(
        client.approval(request["id"].as_str().unwrap()).unwrap()["detail"]["file_checked"],
        false
    );
}

#[tokio::test]
async fn answers_bind_the_request_as_shown() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let request = client.attest_request("cli", text("shown"), None).unwrap();
    let id = request["id"].as_str().unwrap();
    let digest = request["digest"].as_str().unwrap();
    assert_eq!(
        client.approval_respond(id, &"00".repeat(32), true).await,
        Err(Error::IntegrityFailed)
    );
    // Approving needs a login: without one nothing runs and it keeps waiting.
    assert_eq!(
        client.approval_respond(id, digest, true).await,
        Err(Error::AuthRequired)
    );
    let record = client.approval_record(id).unwrap();
    assert_eq!(record.state, State::AwaitingUser);
    assert_eq!(record.error.as_deref(), Some("AUTH_REQUIRED"));
    let rejected = client.approval_respond(id, digest, false).await.unwrap();
    assert_eq!(rejected["state"], "rejected");
    assert_eq!(
        client.approval_respond(id, digest, false).await,
        Err(Error::Expired)
    );

    let withdrawn = client
        .attest_request("cli", text("withdrawn"), None)
        .unwrap();
    let cancelled = client
        .approval_cancel(withdrawn["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(cancelled["state"], "cancelled");

    // Past its deadline a request expires undecided.
    let late = client.attest_request("cli", text("late"), None).unwrap();
    let mut record = client
        .approval_record(late["id"].as_str().unwrap())
        .unwrap();
    record.expires_at = crate::now_ms() - 1;
    client.save_approval(&record).unwrap();
    assert_eq!(
        client.approval(late["id"].as_str().unwrap()).unwrap()["state"],
        "expired"
    );
}

#[test]
fn the_queue_is_bounded_and_sealed() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    for i in 0..MAX_PENDING {
        client
            .attest_request("cli", text(&format!("sealed-statement-marker {i}")), None)
            .unwrap();
    }
    assert_eq!(
        client.attest_request("cli", text("one more"), None).err(),
        Some(Error::QuotaExceeded)
    );
    assert_eq!(client.approvals(false).unwrap().len(), MAX_PENDING);
    // Locked: the count stays visible, the content does not.
    client.lock();
    assert_eq!(client.approvals_waiting().unwrap(), MAX_PENDING as u64);
    assert_eq!(client.approvals(false).err(), Some(Error::Locked));
    let rows = client.workspace.store.approvals(&[]).unwrap();
    assert!(rows.iter().all(|r| r.class == "attest"));
    for name in ["workspace.sqlite", "workspace.sqlite-wal"] {
        let raw = std::fs::read(dir.path(name)).unwrap_or_default();
        assert!(!raw.windows(23).any(|w| w == b"sealed-statement-marker"));
    }
}
