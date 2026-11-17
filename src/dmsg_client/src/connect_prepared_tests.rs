use super::*;
use crate::testing::{TempDir, account_state, unlocked};

fn staged(client: &mut Client) -> (Operation, Execution, AccountState) {
    let now = now_ms();
    let state = account_state(client);
    let mut request = client
        .approval_create(
            "gui",
            Action::Connect {
                operation: "browser-original".into(),
                digest: Hash::new([4; 32]),
                checkout: None,
            },
            now + 60_000,
        )
        .unwrap();
    request.state = State::Executing;
    request.started = true;
    client.save_approval(&request).unwrap();
    let statement = dmsg_types::Statement {
        issuer: client.account().unwrap().issuer.clone(),
        subject: None,
        issued_at: None,
        content: dmsg_types::StatementContent::Text("Reviewed browser statement".into()),
    };
    let (attestation, _) = client
        .attest_unsigned(
            &request.id,
            Subject::Statement {
                statement,
                origin: "https://example.com".into(),
            },
            &state,
            request.expires_at,
            now,
        )
        .unwrap();
    let execution = Execution {
        review: request.digest(),
        version: state.info.account_version,
        kind: Kind::Attest(attestation),
    };
    let op = Operation {
        key: "browser-original".into(),
        origin: "https://example.com".into(),
        command: ConnectCommand {
            method: "signDocument".into(),
            app_id: "example".into(),
            operation_id: "a1".repeat(32),
            public_key: "".into(),
            payload: None,
            result_digest: None,
        },
        digest: Hash::new([4; 32]),
        kind: "document".into(),
        label: "Test browser".into(),
        app_version: 1,
        schema: None,
        approval: request.id,
        channels: vec![],
        state: OpState::Authorizing,
        pinned: None,
        execution: None,
        auth: None,
        prepared: Some(execution.clone()),
        checkout: None,
        result: None,
        result_digest: None,
        error: None,
        requested_at: now,
        expires_at: now + 60_000,
    };
    client.save_operation(&op).unwrap();
    (op, execution, state)
}

#[tokio::test]
async fn a_proved_browser_message_is_not_executable_before_its_original_request_is_pinned_here() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let (op, execution, state) = staged(&mut client);
    let Kind::Attest(attestation) = &execution.kind else {
        panic!("statement");
    };
    assert_eq!(
        activate(&mut client, &op.key, &execution, &state, now_ms()),
        Err(Error::Forbidden)
    );
    assert!(
        client
            .attestation(&attestation.execution_id())
            .unwrap()
            .is_none()
    );
    let device = client.workspace.meta.device_id;
    assert!(
        pin_state(&client.hold(), &op.key, "authorized", Some(device))
            .await
            .unwrap()
    );
    assert!(activate(&mut client, &op.key, &execution, &state, now_ms()).unwrap());
    assert!(client.operation(&op.key).unwrap().prepared.is_none());
    assert_eq!(
        client.approval_record(&op.approval).unwrap().execution,
        Some(attestation.execution_id())
    );
    assert_eq!(
        client.attestation(&attestation.execution_id()).unwrap(),
        Some(attestation.clone())
    );
    // A second runner cannot claim or replace the already journaled request.
    assert!(!activate(&mut client, &op.key, &execution, &state, now_ms()).unwrap());
}

#[tokio::test]
async fn pending_or_foreign_pins_drop_the_prepared_execution_and_preserve_the_correct_review_state()
{
    for (remote, pinned, expected) in [
        ("pending", None, State::AwaitingUser),
        ("authorized", Some(Hash::new([0; 32])), State::Elsewhere),
        ("cancelled", None, State::Cancelled),
    ] {
        let dir = TempDir::new();
        let mut client = unlocked(&dir.0, false, None);
        let (op, execution, _) = staged(&mut client);
        assert!(
            !pin_state(&client.hold(), &op.key, remote, pinned)
                .await
                .unwrap()
        );
        assert!(client.operation(&op.key).unwrap().prepared.is_none());
        assert_eq!(
            client.approval_record(&op.approval).unwrap().state,
            expected
        );
        let Kind::Attest(attestation) = execution.kind else {
            panic!("statement");
        };
        assert!(
            client
                .attestation(&attestation.execution_id())
                .unwrap()
                .is_none()
        );
    }
}

#[tokio::test]
async fn authentication_cannot_rebase_a_browser_execution_onto_a_changed_account_or_review() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let (op, execution, state) = staged(&mut client);
    let device = client.workspace.meta.device_id;
    pin_state(&client.hold(), &op.key, "authorized", Some(device))
        .await
        .unwrap();
    for field in 0..4 {
        let mut stale = state.clone();
        match field {
            0 => stale.info.account_version += 1,
            1 => stale.info.security_epoch += 1,
            2 => {
                stale
                    .security
                    .devices
                    .get_mut(&device)
                    .unwrap()
                    .next_sequence += 1
            }
            _ => stale.security.devices.get_mut(&device).unwrap().revoked_at = Some(now_ms()),
        }
        assert!(activate(&mut client, &op.key, &execution, &stale, now_ms()).is_err());
    }
    assert_eq!(
        activate(
            &mut client,
            &op.key,
            &execution,
            &state,
            execution.approval().expires_at
        ),
        Err(Error::Expired)
    );
    let mut changed = client.approval_record(&op.approval).unwrap();
    changed.action = Action::DeviceRevoke { device_id: device };
    client.save_approval(&changed).unwrap();
    assert_eq!(
        activate(&mut client, &op.key, &execution, &state, now_ms()),
        Err(Error::IntegrityFailed)
    );
    assert!(client.operation(&op.key).unwrap().prepared.is_some());
}
