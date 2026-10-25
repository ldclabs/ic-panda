use super::*;
use crate::{
    approval::Decision,
    testing::{TempDir, unlocked},
    vault::{Changes, FieldChange},
};

const TOKEN: &str = "dmsg-test-credential-never-publish-93ae";

#[tokio::test]
async fn review_old_requests_cannot_revive_after_pause_or_regrant() {
    for pause in [true, false] {
        let (_dir, mut c, actor, p) = setup();
        let policy = configure(&mut c, &actor, p).await;
        let requested = c.secret_use_request(&actor, input(&policy)).unwrap();
        let id = requested["request_id"].as_str().unwrap();
        if pause {
            c.agent_pause(&actor, true).unwrap();
            c.agent_pause(&actor, false).unwrap();
        } else {
            let grant = c.capability_grants().unwrap()[0].id.clone();
            c.capability_revoke("gui", &grant).unwrap();
            c.capability_grant("gui", &actor, vec![format!("secret.use:{policy}")], None)
                .unwrap();
        }
        let view = c.secret_use_get(id, Some(&actor)).unwrap();
        assert_eq!(
            view["state"], "failed",
            "old request must remain invalid after authority changes"
        );
        assert_eq!(view["error"], "SECRET_SESSION_ENDED");
        let mut next = input(&policy);
        next.idempotency_key = "new-request".into();
        assert_eq!(
            c.secret_use_request(&actor, next).unwrap()["state"],
            "awaiting_user"
        );
    }
}

#[tokio::test]
async fn review_any_unrecognized_post_dispatch_error_is_lost_and_never_replayed() {
    for error in [
        Error::Unavailable("adapter failure".into()),
        Error::IntegrityFailed,
        Error::Expired,
    ] {
        let (_dir, mut c, actor, p) = setup();
        let policy = configure(&mut c, &actor, p).await;
        let requested = c.secret_use_request(&actor, input(&policy)).unwrap();
        let id = requested["request_id"].as_str().unwrap();
        approve(&mut c, id)
            .await
            .run_with(&c.hold(), |_, _, _, _| async move { Err(error) })
            .await
            .unwrap();
        let view = c.secret_use_get(id, Some(&actor)).unwrap();
        assert_eq!(view["state"], "lost");
        assert_eq!(view["error"], "EXECUTION_UNKNOWN");
        for _ in 0..2 {
            assert_eq!(c.approvals_reconcile_others().await.unwrap(), 0);
        }
        assert_eq!(
            c.secret_use_request(&actor, input(&policy)).unwrap()["request_id"],
            id
        );
        assert_eq!(c.secret_use_get(id, Some(&actor)).unwrap()["state"], "lost");
    }
}

#[tokio::test]
async fn review_unknown_records_and_abandoned_started_requests_are_terminal_without_send() {
    for state in [State::Executing, State::Unknown] {
        let (_dir, mut c, actor, p) = setup();
        let policy = configure(&mut c, &actor, p).await;
        let requested = c.secret_use_request(&actor, input(&policy)).unwrap();
        let id = requested["request_id"].as_str().unwrap();
        let _runner = approve(&mut c, id).await;
        let mut record = c.approval_record(id).unwrap();
        record.state = state;
        record.started = true;
        c.save_approval(&record).unwrap();
        c.secret_active.clear();
        c.running.clear();
        assert_eq!(c.approvals_reconcile_others().await.unwrap(), 1);
        assert_eq!(c.approval_record(id).unwrap().state, State::Lost);
        assert_eq!(c.approvals_reconcile_others().await.unwrap(), 0);
        assert_eq!(
            c.secret_use_request(&actor, input(&policy)).unwrap()["state"],
            "lost"
        );
        assert!(!c.secret_use_reviewed(id).unwrap()["reviewed_at"].is_null());
        assert_eq!(
            c.secret_use_request(&actor, input(&policy)).unwrap()["state"],
            "lost"
        );
    }
}

#[test]
fn reference_vector_has_independent_cbor_and_json_encoding() {
    let v: Value = serde_json::from_str(include_str!(
        "../../../../tests/vectors/client/secrets-v1.json"
    ))
    .unwrap();
    let reference: SecretRef = serde_json::from_value(v["reference"].clone()).unwrap();
    assert_eq!(reference.uri(), v["uri"]);
    assert_eq!(
        SecretRef::parse(v["uri"].as_str().unwrap()).unwrap(),
        reference
    );
    let bytes = canonical(&reference);
    assert_eq!(hex::encode(&bytes), v["canonical_cbor_hex"]);
    assert_eq!(hex::encode(Sha256::digest(&bytes)), v["sha256"]);
    for bad in v["invalid"].as_array().unwrap() {
        assert!(SecretRef::parse(bad.as_str().unwrap()).is_err());
    }
}

fn setup() -> (TempDir, Client, String, PolicyInput) {
    let dir = TempDir::new();
    let mut c = unlocked(&dir.0, false, None);
    let agent = c.agent_create("builder").unwrap();
    c.agent_credential(&agent.agent_id).unwrap();
    let entry = c
        .vault_create(
            "api",
            &Changes {
                title: Some("GitHub fixture".into()),
                set: Some(vec![FieldChange {
                    id: Some("credential".into()),
                    value: Some(TOKEN.into()),
                    ..Default::default()
                }]),
                ..Default::default()
            },
        )
        .unwrap();
    let policy = PolicyInput {
        schema_version: 1,
        name: "Test repository".into(),
        enabled: true,
        adapter: "github.issues/1".into(),
        credential: SecretRef {
            schema_version: 1,
            account_id: c.account().unwrap().id.to_string(),
            item_id: entry.id,
            field_id: "credential".into(),
        },
        target: Target {
            api_origin: "https://api.github.com".into(),
            owner: "example".into(),
            repo: "test".into(),
        },
        operations: vec!["issue.create".into()],
    };
    (dir, c, agent.agent_id, policy)
}

async fn configure(c: &mut Client, actor: &str, policy: PolicyInput) -> String {
    let request = c
        .secret_policy_request("gui", None, None, Some(policy), Some(actor.into()))
        .unwrap();
    c.approval_respond(
        request["id"].as_str().unwrap(),
        request["digest"].as_str().unwrap(),
        true,
        "gui",
        false,
    )
    .await
    .unwrap();
    c.secret_policies().unwrap()[0].id.clone()
}

fn input(policy: &str) -> UseRequestInput {
    UseRequestInput {
        idempotency_key: "request-1".into(),
        policy_id: policy.into(),
        operation: "issue.create".into(),
        arguments: github::IssueInput {
            title: "A real title".into(),
            body: Some("<script>alert(1)</script>\nfull body".into()),
        },
        reason: Some("untrusted reason".into()),
    }
}

async fn approve(c: &mut Client, id: &str) -> SecretRun {
    let view = c.approval(id).unwrap();
    match c
        .approval_decide(id, view["digest"].as_str().unwrap(), true, "gui", false)
        .await
        .unwrap()
    {
        Decision::SecretUse(run) => *run,
        _ => panic!("expected deferred use"),
    }
}

#[test]
fn references_are_canonical_and_bound_to_current_safe_fields() {
    let (_dir, mut c, _actor, p) = setup();
    let uri = p.credential.uri();
    assert_eq!(SecretRef::parse(&uri).unwrap(), p.credential);
    for bad in [
        format!("{uri}?x=1"),
        format!("{uri}#x"),
        uri.replace("credential", "%63redential"),
        uri.replace("credential", "../credential"),
        uri.replace("//", "//user@"),
        uri.to_uppercase(),
    ] {
        assert!(SecretRef::parse(&bad).is_err(), "{bad}");
    }
    let old = c.secret_resolve(&p.credential, true).unwrap();
    assert!(!serde_json::to_string(&old).unwrap().contains(TOKEN));
    c.vault_edit(
        &p.credential.item_id,
        None,
        &Changes {
            title: Some("Renamed".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let new = c.secret_resolve(&p.credential, true).unwrap();
    assert_ne!(old.item_revision, new.item_revision);
    c.vault_delete(&p.credential.item_id).unwrap();
    assert!(c.secret_resolve(&p.credential, true).is_err());
}

#[tokio::test]
async fn use_is_single_send_deduplicated_and_returns_only_filtered_result() {
    let (_dir, mut c, actor, p) = setup();
    let policy = configure(&mut c, &actor, p).await;
    let requested = c.secret_use_request(&actor, input(&policy)).unwrap();
    let id = requested["request_id"].as_str().unwrap().to_owned();
    assert_eq!(
        c.secret_use_request(&actor, input(&policy)).unwrap(),
        requested
    );
    let mut changed = input(&policy);
    changed.arguments.title = "Changed".into();
    assert_eq!(
        c.secret_use_request(&actor, changed),
        Err(Error::IdempotencyConflict)
    );
    let runner = approve(&mut c, &id).await;
    let digest = c.approval(&id).unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(matches!(
        c.approval_decide(&id, &digest, true, "gui", false).await,
        Err(Error::Expired)
    ));
    runner.run_with(&c.hold(), |_,request,target,token| async move {
        assert_eq!(request.url().as_str(),"https://api.github.com/repos/example/test/issues");
        assert_eq!(request.headers()["authorization"],format!("Bearer {TOKEN}"));
        assert!(!format!("{request:?}").contains(TOKEN));
        github::response(201, br#"{"number":7,"html_url":"https://github.com/example/test/issues/7","body":"ignored"}"#,&target,&token)
    }).await.unwrap();
    let result = c.secret_use_get(&id, Some(&actor)).unwrap();
    assert_eq!(result["state"], "done");
    assert_eq!(result["result"]["number"], 7);
    assert!(!result.to_string().contains(TOKEN));
    assert_eq!(
        c.secret_use_request(&actor, input(&policy)).unwrap()["state"],
        "done"
    );
    assert_eq!(c.approvals_reconcile_others().await.unwrap(), 0);
}

#[tokio::test]
async fn stale_revision_and_session_stop_before_dispatch() {
    for change in ["revision", "lock", "credential", "grant", "policy"] {
        let (_dir, mut c, actor, p) = setup();
        let field = p.credential.clone();
        let policy = configure(&mut c, &actor, p).await;
        let requested = c.secret_use_request(&actor, input(&policy)).unwrap();
        let id = requested["request_id"].as_str().unwrap();
        let runner = approve(&mut c, id).await;
        match change {
            "revision" => {
                c.vault_edit(
                    &field.item_id,
                    None,
                    &Changes {
                        notes: Some("changed".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            "lock" => {
                c.lock();
                c.keys = Some(c.workspace.unlock(&crate::testing::SECRET).unwrap());
            }
            "credential" => {
                c.agent_credential(&actor).unwrap();
            }
            "grant" => {
                let g = c.capability_grants().unwrap()[0].id.clone();
                c.capability_revoke("gui", &g).unwrap();
            }
            _ => {
                c.secret_policy_disable(&policy).unwrap();
            }
        }
        let _ = runner
            .run_with(&c.hold(), |_, _, _, _| async { panic!("must never send") })
            .await;
        c.approvals_reconcile_others().await.unwrap();
        assert_eq!(
            c.approval_record(id).unwrap().state,
            State::Failed,
            "{change}"
        );
    }
}

#[tokio::test]
async fn uncertain_post_is_lost_and_is_never_replayed() {
    let (_dir, mut c, actor, p) = setup();
    let policy = configure(&mut c, &actor, p).await;
    let requested = c.secret_use_request(&actor, input(&policy)).unwrap();
    let id = requested["request_id"].as_str().unwrap();
    approve(&mut c, id)
        .await
        .run_with(&c.hold(), |_, _, _, _| async {
            Err(Error::ExecutionUnknown)
        })
        .await
        .unwrap();
    assert_eq!(c.secret_use_get(id, Some(&actor)).unwrap()["state"], "lost");
    assert_eq!(c.approvals_reconcile_others().await.unwrap(), 0);
    assert_eq!(c.secret_use_reviewed(id).unwrap()["state"], "lost");
}

#[test]
fn adapter_rejects_extra_inputs_destinations_and_secret_echo() {
    let (_dir, _c, _a, p) = setup();
    for status in [301, 302, 500, 503] {
        assert_eq!(
            github::response(status, b"", &p.target, TOKEN),
            Err(Error::ExecutionUnknown)
        );
    }
    assert_eq!(
        github::response(401, TOKEN.as_bytes(), &p.target, TOKEN),
        Err(Error::Forbidden)
    );
    assert_eq!(crate::error_code(&github::response(201,format!(r#"{{"number":7,"html_url":"https://github.com/example/test/issues/7","x":"{TOKEN}"}}"#).as_bytes(),&p.target,TOKEN).unwrap_err()),"SECRET_OUTPUT_BLOCKED");
    assert!(
        serde_json::from_value::<github::IssueInput>(
            json!({"title":"ok","url":"https://evil.test"})
        )
        .is_err()
    );
    for owner in ["..", "a/b", "a%2fb", "x@evil.test"] {
        let mut target = p.target.clone();
        target.owner = owner.into();
        assert!(github::validate_target(&target).is_err());
    }
    assert!(!crate::agents::covers(
        "secret.use:*",
        "secret.use:0123456789abcdef0123456789abcdef"
    ));
    assert!(!crate::agents::covers("vault.secret:*", "vault.secret:any"));
}

struct Shared(tokio::sync::Mutex<Client>);

impl crate::hold::Hold for Shared {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        step(&mut *self.0.lock().await)
    }
}

#[tokio::test]
async fn slow_dispatch_releases_client_and_lock_aborts_without_replay() {
    use crate::hold::Hold;
    let (_dir, mut c, actor, p) = setup();
    let policy = configure(&mut c, &actor, p).await;
    let requested = c.secret_use_request(&actor, input(&policy)).unwrap();
    let id = requested["request_id"].as_str().unwrap().to_owned();
    let run = approve(&mut c, &id).await;
    let h = Shared(tokio::sync::Mutex::new(c));
    let sent = tokio::sync::Notify::new();
    let (outcome, ()) = tokio::join!(
        run.run_with(&h, |_, _, _, _token| async {
            sent.notify_one();
            std::future::pending::<Result<github::IssueResult>>().await
        }),
        async {
            sent.notified().await;
            tokio::time::timeout(
                std::time::Duration::from_secs(1),
                h.with(|c| {
                    c.lock();
                    Ok(())
                }),
            )
            .await
            .unwrap()
            .unwrap();
        }
    );
    assert_eq!(outcome, Err(Error::Locked));
    h.with(|c| {
        c.keys = Some(c.workspace.unlock(&crate::testing::SECRET)?);
        let mut r = c.approval_record(&id)?;
        assert!(r.started);
        assert_eq!(r.state, State::Executing);
        c.secret_reconcile(&mut r)?;
        assert_eq!(r.state, State::Lost);
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn request_ownership_limits_recovery_and_configuration_are_enforced() {
    let (_dir, mut c, actor, p) = setup();
    let policy = configure(&mut c, &actor, p.clone()).await;
    let other = c.agent_create("other").unwrap().agent_id;
    c.agent_credential(&other).unwrap();
    assert_eq!(
        c.secret_use_request(&other, input(&policy)),
        Err(Error::Forbidden)
    );
    c.capability_grant("gui", &other, vec![format!("secret.use:{policy}")], None)
        .unwrap();
    let requested = c.secret_use_request(&actor, input(&policy)).unwrap();
    let id = requested["request_id"].as_str().unwrap();
    assert_eq!(c.secret_use_get(id, Some(&other)), Err(Error::Forbidden));
    assert_eq!(c.secret_use_cancel(id, Some(&other)), Err(Error::Forbidden));
    let view = c.approval(id).unwrap();
    assert!(matches!(
        c.approval_decide(id, view["digest"].as_str().unwrap(), true, "cli", true)
            .await,
        Err(Error::Forbidden)
    ));
    let _abandoned = approve(&mut c, id).await;
    c.secret_active.clear();
    c.running.clear(); // a new daemon has no active task
    c.approvals_reconcile_others().await.unwrap();
    assert_eq!(c.approval_record(id).unwrap().state, State::Failed);
    assert!(!c.approval_record(id).unwrap().started);
    let change = c
        .secret_policy_request("gui", Some(policy.clone()), Some(1), Some(p), None)
        .unwrap();
    c.secret_policy_disable(&policy).unwrap();
    assert!(
        c.approval_respond(
            change["id"].as_str().unwrap(),
            change["digest"].as_str().unwrap(),
            true,
            "gui",
            false
        )
        .await
        .is_err()
    );
    assert!(!c.secret_policies().unwrap()[0].policy.enabled);
}

#[tokio::test]
async fn conflicts_reserved_fields_other_accounts_and_changed_types_are_not_resolved() {
    let (_dir, mut c, actor, p) = setup();
    let mut reference = p.credential.clone();
    reference.schema_version = 2;
    assert!(c.secret_resolve(&reference, true).is_err());
    reference = p.credential.clone();
    reference.account_id = "080g2081040g2081040g".into();
    assert!(c.secret_resolve(&reference, true).is_err());
    reference = p.credential.clone();
    reference.field_id = "unknown".into();
    assert!(c.secret_resolve(&reference, true).is_err());
    reference = p.credential.clone();
    reference.item_id = crate::agents::agent_item(&reference.account_id, "builder");
    assert!(c.secret_resolve(&reference, false).is_err());
    let policy = configure(&mut c, &actor, p.clone()).await;
    let old = c.vault_show(&p.credential.item_id).unwrap();
    c.vault_edit(
        &old.id,
        None,
        &Changes {
            notes: Some("current".into()),
            ..Default::default()
        },
    )
    .unwrap();
    c.vault_edit(
        &old.id,
        Some(&old.revision),
        &Changes {
            notes: Some("conflict".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        crate::error_code(&c.secret_use_request(&actor, input(&policy)).unwrap_err()),
        "SECRET_VERSION_STALE"
    );
}

#[tokio::test]
async fn pending_and_execution_quotas_are_shared_and_bounded() {
    let (_dir, mut c, actor, p) = setup();
    let policy = configure(&mut c, &actor, p).await;
    let mut requests = Vec::new();
    for i in 0..5 {
        let mut v = input(&policy);
        v.idempotency_key = format!("key-{i}");
        requests.push(c.secret_use_request(&actor, v).unwrap());
    }
    let mut extra = input(&policy);
    extra.idempotency_key = "overflow".into();
    assert_eq!(
        c.secret_use_request(&actor, extra),
        Err(Error::QuotaExceeded)
    );
    let run = approve(&mut c, requests[0]["request_id"].as_str().unwrap()).await;
    let second = requests[1]["request_id"].as_str().unwrap();
    let view = c.approval(second).unwrap();
    assert!(matches!(
        c.approval_decide(second, view["digest"].as_str().unwrap(), true, "gui", false)
            .await
            .unwrap(),
        Decision::Done(_)
    ));
    assert_eq!(
        c.approval_record(second).unwrap().error.as_deref(),
        Some("QUOTA_EXCEEDED")
    );
    run.run_with(&c.hold(), |_, _, _, _| async { Err(Error::Forbidden) })
        .await
        .unwrap();
    assert_eq!(c.secret_active.len(), 0);
}

#[tokio::test]
async fn policy_grants_and_results_are_sealed_and_device_local() {
    let (dir, mut c, actor, p) = setup();
    let reference = p.credential.clone();
    let policy = configure(&mut c, &actor, p).await;
    let requested = c.secret_use_request(&actor, input(&policy)).unwrap();
    let id = requested["request_id"].as_str().unwrap();
    approve(&mut c, id)
        .await
        .run_with(&c.hold(), |_, _, target, _| async move {
            Ok(github::IssueResult {
                number: 9,
                html_url: format!(
                    "https://github.com/{}/{}/issues/9",
                    target.owner, target.repo
                ),
            })
        })
        .await
        .unwrap();
    // Read actual SQLite files, including WAL, rather than serializing a model.
    fn scan(path: &std::path::Path) {
        for entry in std::fs::read_dir(path).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                scan(&p)
            } else if let Ok(bytes) = std::fs::read(p) {
                for needle in [TOKEN.as_bytes(), b"A real title", b"Test repository"] {
                    assert!(!bytes.windows(needle.len()).any(|w| w == needle));
                }
            }
        }
    }
    scan(&dir.0);
    let second = TempDir::new();
    let other = unlocked(&second.0, false, None);
    assert!(other.secret_policies().unwrap().is_empty());
    assert!(other.secret_resolve(&reference, true).is_err());
}

#[tokio::test]
async fn http_mock_checks_no_redirect_no_replay_size_and_timeout() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    for mode in ["success", "redirect", "server_error", "oversize", "timeout"] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = stream.read(&mut buf).await.unwrap();
                got.extend_from_slice(&buf[..n]);
                if n == 0 || got.windows(4).any(|s| s == b"\r\n\r\n") {
                    break;
                }
            }
            assert!(
                String::from_utf8_lossy(&got).contains(&format!("authorization: Bearer {TOKEN}"))
            );
            if mode == "timeout" {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                return;
            }
            let body = r#"{"number":12,"html_url":"https://github.com/example/test/issues/12"}"#;
            let reply=match mode {
                "success" => format!("HTTP/1.1 201 Created\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()),
                "redirect" => format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{address}/stolen\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
                "oversize" => "HTTP/1.1 201 Created\r\nContent-Length: 1048577\r\nConnection: close\r\n\r\n".into(),
                _ => "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            };
            stream.write_all(reply.as_bytes()).await.unwrap();
            drop(stream);
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(200), listener.accept())
                    .await
                    .is_err(),
                "no retry or redirected request"
            );
        });
        let target = Target {
            api_origin: "https://api.github.com".into(),
            owner: "example".into(),
            repo: "test".into(),
        };
        let http = github::transport_builder()
            .https_only(false)
            .timeout(std::time::Duration::from_millis(150))
            .build()
            .unwrap();
        let mut request =
            github::request(&http, &target, &input("unused").arguments, TOKEN).unwrap();
        // Only this test substitutes a loopback address. The production entry
        // constructs a fresh fixed-origin request and has no override option.
        *request.url_mut() = format!("http://{address}/repos/example/test/issues")
            .parse()
            .unwrap();
        let result = github::send(http, request, &target, TOKEN).await;
        if mode == "success" {
            assert_eq!(result.unwrap().number, 12)
        } else {
            assert_eq!(result, Err(Error::ExecutionUnknown), "{mode}")
        }
        server.await.unwrap();
    }
}

fn database(dir: &std::path::Path) -> rusqlite::Connection {
    fn find(dir: &std::path::Path) -> Option<std::path::PathBuf> {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|n| n == "workspace.sqlite") {
                return Some(path);
            }
            if path.is_dir()
                && let Some(found) = find(&path)
            {
                return Some(found);
            }
        }
        None
    }
    rusqlite::Connection::open(find(dir).unwrap()).unwrap()
}

#[tokio::test]
async fn storage_faults_before_dispatch_and_after_success_never_resend() {
    for before_dispatch in [true, false] {
        let (dir, mut c, actor, p) = setup();
        let policy = configure(&mut c, &actor, p).await;
        let requested = c.secret_use_request(&actor, input(&policy)).unwrap();
        let id = requested["request_id"].as_str().unwrap();
        let run = approve(&mut c, id).await;
        let db = database(&dir.0);
        let state = if before_dispatch { "executing" } else { "done" };
        db.execute_batch(&format!("CREATE TRIGGER fail_approval BEFORE INSERT ON approvals WHEN NEW.state = '{state}' BEGIN SELECT RAISE(FAIL, 'injected'); END")).unwrap();
        let answer = run
            .run_with(&c.hold(), |_, _, target, _| async move {
                assert!(!before_dispatch, "failed durable commit must never send");
                Ok(github::IssueResult {
                    number: 7,
                    html_url: format!(
                        "https://github.com/{}/{}/issues/7",
                        target.owner, target.repo
                    ),
                })
            })
            .await;
        db.execute_batch("DROP TRIGGER fail_approval").unwrap();
        if before_dispatch {
            assert_eq!(answer.unwrap()["state"], "failed");
            assert!(!c.approval_record(id).unwrap().started);
        } else {
            assert!(matches!(answer, Err(Error::Unavailable(_))));
            assert!(c.approval_record(id).unwrap().started);
            c.approvals_reconcile_others().await.unwrap();
            assert_eq!(c.approval_record(id).unwrap().state, State::Lost);
        }
    }
}

#[tokio::test]
async fn policy_and_capability_rollback_when_configuration_settlement_fails() {
    let (dir, mut c, actor, p) = setup();
    let request = c
        .secret_policy_request("gui", None, None, Some(p), Some(actor))
        .unwrap();
    let db = database(&dir.0);
    db.execute_batch("CREATE TRIGGER fail_policy BEFORE INSERT ON approvals WHEN NEW.state = 'done' BEGIN SELECT RAISE(FAIL, 'injected'); END").unwrap();
    assert!(
        c.approval_respond(
            request["id"].as_str().unwrap(),
            request["digest"].as_str().unwrap(),
            true,
            "gui",
            false
        )
        .await
        .is_err()
    );
    assert!(c.secret_policies().unwrap().is_empty());
    assert!(c.capability_grants().unwrap().is_empty());
}

#[tokio::test]
async fn configuration_reclaims_expired_grants_like_the_existing_grant_flow() {
    let (_dir, mut c, actor, p) = setup();
    let grants: Vec<_> = (0..crate::agents::MAX_GRANTS)
        .map(|i| crate::agents::CapabilityGrant {
            id: format!("{i:032x}"),
            grantee: actor.clone(),
            capabilities: vec!["vault.list".into()],
            created_at: 0,
            expires_at: Some(1),
            revoked_at: None,
        })
        .collect();
    c.workspace
        .put_private(c.keys().unwrap(), "agent-grants", &grants)
        .unwrap();
    let policy = configure(&mut c, &actor, p).await;
    assert_eq!(c.capability_grants().unwrap().len(), 1);
    c.capability_check(&actor, &format!("secret.use:{policy}"))
        .unwrap();
}

#[tokio::test]
async fn presence_and_full_plan_digest_are_required_before_execution() {
    use crate::secure::{Presence, PresenceKey};
    struct Decline;
    impl PresenceKey for Decline {
        fn presence(&self) -> Option<Presence> {
            Some(Presence::Password)
        }

        fn create(&self, _: bool) -> Result<(Vec<u8>, Vec<u8>, Zeroizing<[u8; 32]>)> {
            Err(Error::Forbidden)
        }

        fn open(&self, _: &[u8], _: &[u8], _: bool) -> Result<Zeroizing<[u8; 32]>> {
            Err(Error::Forbidden)
        }

        fn biometry_state(&self) -> Option<dmsg_types::Hash> {
            None
        }

        fn confirm(&self, _: &str) -> Result<()> {
            Err(Error::Forbidden)
        }
    }
    let (_dir, mut c, actor, p) = setup();
    let policy = configure(&mut c, &actor, p).await;
    let requested = c.secret_use_request(&actor, input(&policy)).unwrap();
    let id = requested["request_id"].as_str().unwrap();
    c.set_presence_key(std::sync::Arc::new(Decline));
    let view = c.approval(id).unwrap();
    assert_eq!(
        c.approval_respond(id, view["digest"].as_str().unwrap(), true, "gui", false)
            .await,
        Err(Error::Forbidden)
    );
    let (crate::approval::Ask::Presence(hardware), what) =
        c.approval_presence(id).unwrap().unwrap()
    else {
        panic!("a presence check")
    };
    assert_eq!(hardware.confirm(&what), Err(Error::Forbidden));
    let mut record = c.approval_record(id).unwrap();
    let old = record.digest();
    let Action::SecretUse { plan } = &mut record.action else {
        panic!()
    };
    plan.input.arguments.body = Some("A different full body".into());
    assert_ne!(record.digest(), old);
    c.save_approval(&record).unwrap();
    assert_eq!(
        c.approval_respond(id, &hex::encode(*old), true, "gui", true)
            .await,
        Err(Error::IntegrityFailed)
    );
    assert!(!c.approval_record(id).unwrap().started);
}

fn process_input(reference: &SecretRef) -> process::RunInput {
    let refs = format!("TOKEN={}\n", reference.uri());
    process::RunInput {
        executable: process::FileIdentity {
            path: "/usr/bin/env".into(),
            identity: "1:2".into(),
            sha256: "ab".repeat(32),
        },
        arg0: "env".into(),
        argv: vec![],
        cwd: "/tmp".into(),
        refs_file: Some(process::FileIdentity {
            path: "/tmp/.env.refs".into(),
            identity: "1:3".into(),
            sha256: hex::encode(Sha256::digest(refs.as_bytes())),
        }),
        refs,
        files: vec![],
    }
}

#[tokio::test]
async fn process_claim_is_once_bound_to_approval_revision_and_session() {
    let (_dir, mut c, _, p) = setup();
    let r = c
        .secret_process_request(process_input(&p.credential))
        .unwrap();
    let id = r["request_id"].as_str().unwrap();
    let ticket = r["claim"].as_str().unwrap();
    assert!(!r.to_string().contains(TOKEN));
    assert!(process::claim(&c.hold(), id, ticket).await.is_err());
    let digest = c.approval(id).unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    c.approval_respond(id, &digest, true, "gui", false)
        .await
        .unwrap();
    assert!(
        process::claim(&c.hold(), id, &crate::codec::b64(&[1; 32]))
            .await
            .is_err()
    );
    let values = process::claim(&c.hold(), id, ticket).await.unwrap();
    assert_eq!(values["env"][0]["value"], TOKEN);
    assert!(process::claim(&c.hold(), id, ticket).await.is_err());
    assert_eq!(c.approval(id).unwrap()["state"], "done");
    assert!(!c.approval(id).unwrap().to_string().contains(TOKEN));
    c.secret_process_report(id, ticket, true, Some(19)).unwrap();
    assert!(c.secret_process_report(id, ticket, true, Some(0)).is_err());
    assert_eq!(c.approvals_reconcile_others().await.unwrap(), 0);
    let r = c
        .secret_process_request(process_input(&p.credential))
        .unwrap();
    let id = r["request_id"].as_str().unwrap();
    c.vault_edit(
        &p.credential.item_id,
        None,
        &Changes {
            set: Some(vec![FieldChange {
                id: Some("credential".into()),
                value: Some("changed".into()),
                ..Default::default()
            }]),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(c.approval(id).unwrap()["state"], "failed");
}

#[test]
fn process_refs_reject_shell_duplicates_limits_and_unknown_fields() {
    let (_dir, mut c, _, p) = setup();
    let uri = p.credential.uri();
    assert_eq!(
        process::parse_refs(format!("# comment\nTOKEN={uri}\n").as_bytes())
            .unwrap()
            .len(),
        1
    );
    for text in [
        format!("TOKEN={uri}\nTOKEN={uri}"),
        format!("export TOKEN={uri}"),
        "TOKEN=$(cat secret)".into(),
        format!("TOKEN='{uri}'"),
        format!("A= {uri}"),
        format!("1A={uri}"),
    ] {
        assert!(process::parse_refs(text.as_bytes()).is_err(), "{text}");
    }
    assert!(process::parse_refs(&vec![b' '; 65537]).is_err());
    let mut input = process_input(&p.credential);
    input.refs.push(' ');
    assert!(c.secret_process_request(input).is_err());
    for arg0 in ["", "env\0x"] {
        let mut input = process_input(&p.credential);
        input.arg0 = arg0.into();
        assert!(c.secret_process_request(input).is_err(), "{arg0:?}");
    }
    let view = c
        .secret_process_request(process_input(&p.credential))
        .unwrap();
    assert_eq!(view["approval"]["detail"]["arg0"], "env");
}

#[tokio::test]
async fn process_abandoned_claim_and_changed_session_never_deliver_twice() {
    for started in [false, true] {
        let (_dir, mut c, _, p) = setup();
        let r = c
            .secret_process_request(process_input(&p.credential))
            .unwrap();
        let id = r["request_id"].as_str().unwrap();
        let digest = c.approval(id).unwrap()["digest"]
            .as_str()
            .unwrap()
            .to_owned();
        c.approval_respond(id, &digest, true, "gui", false)
            .await
            .unwrap();
        let mut record = c.approval_record(id).unwrap();
        record.started = started;
        c.save_approval(&record).unwrap();
        c.running.clear();
        c.secret_boot = "another-process".into();
        c.approvals_reconcile_others().await.unwrap();
        assert_eq!(
            c.approval_record(id).unwrap().state,
            if started { State::Lost } else { State::Failed }
        );
        assert!(
            process::claim(&c.hold(), id, r["claim"].as_str().unwrap())
                .await
                .is_err()
        );
    }
}

fn git_sign_bytes(namespace: &[u8]) -> Vec<u8> {
    let mut b = b"SSHSIG".to_vec();
    ssh::put(&mut b, namespace);
    ssh::put(&mut b, b"");
    ssh::put(&mut b, b"sha512");
    ssh::put(&mut b, &[42; 64]);
    b
}

#[tokio::test]
async fn ssh_sign_requires_binding_current_revision_original_connection_and_one_approval() {
    use signature::Verifier;
    let (_dir, mut c, _, _) = setup();
    let key = crate::vault::ssh::generate("synthetic");
    let entry = c
        .vault_create(
            "ssh",
            &Changes {
                title: Some("Synthetic SSH".into()),
                set: Some(vec![FieldChange {
                    id: Some("private_key".into()),
                    value: Some(key.private_key.clone()),
                    ..Default::default()
                }]),
                ..Default::default()
            },
        )
        .unwrap();
    let r = SecretRef {
        schema_version: 1,
        account_id: c.account().unwrap().id.to_string(),
        item_id: entry.id,
        field_id: "private_key".into(),
    };
    let public = ssh_key::PublicKey::from_openssh(&key.public_key).unwrap();
    let bytes = git_sign_bytes(b"git");
    let conn = c
        .secret_ssh_open(ssh::Peer {
            uid: 501,
            pid: None,
        })
        .unwrap();
    assert!(
        c.secret_ssh_request(&conn, &public.to_bytes().unwrap(), &bytes)
            .is_err()
    );
    let enable = c
        .secret_ssh_binding_request("gui", r.clone(), vec!["git_sign".into()])
        .unwrap();
    c.approval_respond(
        enable["id"].as_str().unwrap(),
        enable["digest"].as_str().unwrap(),
        true,
        "gui",
        false,
    )
    .await
    .unwrap();
    let pending = c
        .secret_ssh_request(&conn, &public.to_bytes().unwrap(), &bytes)
        .unwrap();
    let id = pending["id"].as_str().unwrap();
    assert!(!pending.to_string().contains("PRIVATE KEY"));
    assert!(c.secret_ssh_sign(id, &conn, &bytes).is_err());
    c.approval_respond(id, pending["digest"].as_str().unwrap(), true, "gui", false)
        .await
        .unwrap();
    assert!(c.secret_ssh_sign(id, "another", &bytes).is_err());
    assert!(
        c.secret_ssh_sign(id, &conn, &git_sign_bytes(b"wrong"))
            .is_err()
    );
    let sig = c.secret_ssh_sign(id, &conn, &bytes).unwrap();
    public
        .key_data()
        .verify(
            &bytes,
            &ssh_key::Signature::try_from(sig.as_slice()).unwrap(),
        )
        .unwrap();
    assert!(c.secret_ssh_sign(id, &conn, &bytes).is_err());
    let pending = c
        .secret_ssh_request(&conn, &public.to_bytes().unwrap(), &bytes)
        .unwrap();
    c.secret_ssh_close(&conn);
    assert_eq!(
        c.approval(pending["id"].as_str().unwrap()).unwrap()["state"],
        "failed"
    );
    let conn = c
        .secret_ssh_open(ssh::Peer {
            uid: 501,
            pid: None,
        })
        .unwrap();
    let pending = c
        .secret_ssh_request(&conn, &public.to_bytes().unwrap(), &bytes)
        .unwrap();
    c.secret_ssh_disable(&c.secret_ssh_bindings().unwrap()[0].id)
        .unwrap();
    assert_eq!(
        c.approval(pending["id"].as_str().unwrap()).unwrap()["state"],
        "failed"
    );
    assert!(c.secret_ssh_identities().unwrap().is_empty());
    c.lock();
    assert!(c.secret_ssh_identities().unwrap().is_empty());
}

#[test]
fn ssh_protocol_verifies_host_session_and_refuses_forwarding_arbitrary_data() {
    use signature::{SignatureEncoding, Signer};
    let host = crate::vault::ssh::generate("host");
    let key = ssh_key::PrivateKey::from_openssh(&host.private_key).unwrap();
    let public = key.public_key().to_bytes().unwrap();
    let session = [7; 32];
    let signature: ssh_key::Signature = key.try_sign(&session).unwrap();
    let mut extension = Vec::new();
    ssh::put(&mut extension, b"session-bind@openssh.com");
    ssh::put(&mut extension, &public);
    ssh::put(&mut extension, &session);
    ssh::put(&mut extension, &signature.to_vec());
    extension.push(0);
    let bound = ssh::session_bind(&extension).unwrap();
    let mut auth = Vec::new();
    ssh::put(&mut auth, &session);
    auth.push(50);
    ssh::put(&mut auth, b"test");
    ssh::put(&mut auth, b"ssh-connection");
    ssh::put(&mut auth, b"publickey");
    auth.push(1);
    ssh::put(&mut auth, b"ssh-ed25519");
    ssh::put(&mut auth, &public);
    assert_eq!(
        ssh::purpose(&auth, &public, Some(&bound)).unwrap().kind,
        "ssh_auth"
    );
    assert!(ssh::purpose(&auth, &public, None).is_err());
    let mut tampered = auth.clone();
    tampered[4] ^= 1;
    assert!(ssh::purpose(&tampered, &public, Some(&bound)).is_err());
    *extension.last_mut().unwrap() = 1;
    assert!(ssh::session_bind(&extension).is_err());
    assert!(ssh::purpose(b"arbitrary message", &public, Some(&bound)).is_err());
    assert!(ssh::purpose(&git_sign_bytes(b"file"), &public, None).is_err());
    assert!(ssh::purpose(&vec![0; ssh::MAX_FRAME + 1], &public, None).is_err());
    for n in 0..auth.len() {
        assert!(ssh::purpose(&auth[..n], &public, Some(&bound)).is_err());
    }
}

#[test]
fn staged_attachment_and_vault_outbox_commit_together_or_not_at_all() {
    for inject in [false, true] {
        let (dir, mut c, _, _) = setup();
        let before = c
            .vault_list(&crate::vault::Filter::default())
            .unwrap()
            .len();
        let mut staged = c
            .secret_stage_start("fixture.txt", "text/plain", 4)
            .unwrap();
        let file = staged.id().to_owned();
        c.secret_stage_part(&mut staged, b"test").unwrap();
        c.secret_stage_finish(&mut staged, Some(&hex::encode(Sha256::digest(b"test"))))
            .unwrap();
        let mut item = crate::vault::Item::new("note", "Atomic import", now_ms());
        item.files.push(file.clone());
        let db = database(&dir.0);
        if inject {
            db.execute_batch("CREATE TRIGGER fail_import BEFORE INSERT ON objects BEGIN SELECT RAISE(FAIL, 'injected'); END").unwrap();
        }
        let added = c.secret_import_commit(vec![item], vec![staged], "root");
        if inject {
            assert!(added.is_err());
            assert_eq!(
                c.vault_list(&crate::vault::Filter::default())
                    .unwrap()
                    .len(),
                before
            );
            assert!(c.files_list("root", false).unwrap().files.is_empty());
            c.held_chunks(now_ms() + 11 * dmsg_types::MINUTE).unwrap();
            assert!(c.collect_blobs().unwrap().0 > 0);
        } else {
            let ids = added.unwrap();
            assert_eq!(
                c.vault_show(&ids[0]).unwrap().item.files.as_slice(),
                std::slice::from_ref(&file)
            );
            assert!(c.files_show(&file).unwrap().versions[0].backup.is_none());
            let output = dir.0.join("checked.txt");
            c.files_export(&file, None, &output).unwrap();
            assert_eq!(std::fs::read(output).unwrap(), b"test");
        }
    }
}

#[test]
fn rotation_is_an_ordered_owner_report_not_an_upstream_revocation_proof() {
    let (_dir, mut c, _, p) = setup();
    let r = c.secret_rotation_create(p.credential.clone()).unwrap();
    let id = r["id"].as_str().unwrap();
    assert!(c.secret_rotation_update(id, "created", None).is_err());
    assert!(
        c.secret_rotation_update(id, "upstream_revoked", None)
            .is_err()
    );
    c.vault_edit(
        &p.credential.item_id,
        None,
        &Changes {
            set: Some(vec![FieldChange {
                id: Some("credential".into()),
                value: Some("replacement-fixture".into()),
                ..Default::default()
            }]),
            ..Default::default()
        },
    )
    .unwrap();
    for step in [
        "created",
        "tested",
        "references_checked",
        "upstream_revoked",
    ] {
        let index = c.secret_rotation_dependencies(id).unwrap();
        c.secret_rotation_update(id, step, Some(&index.digest))
            .unwrap();
    }
    let report = c.secret_rotations().unwrap();
    assert_eq!(report[0].evidence, "owner_statement");
    assert!(report[0].completed_at.is_some());
    assert_eq!(c.secret_health().unwrap()["upstream_revocation"], "unknown");
}

#[test]
fn external_import_receipts_do_not_expose_guessable_source_digests() {
    let (_dir, mut c, _, _) = setup();
    let digest = "ab".repeat(32);
    let before = c
        .vault_list(&crate::vault::Filter::default())
        .unwrap()
        .len();
    let item = crate::vault::Item::new("note", "Receipt fixture", now_ms());
    let first = c
        .secret_import_commit_once(&digest, vec![item.clone()], vec![], "root")
        .unwrap();
    assert_eq!(
        c.secret_import_commit_once(&digest, vec![item], vec![], "root")
            .unwrap(),
        first
    );
    let keys = c
        .workspace
        .store
        .sealed_keys("secret-external-import:")
        .unwrap();
    assert_eq!(keys.len(), 1);
    assert!(!keys[0].contains(&digest));
    assert_eq!(
        c.vault_list(&crate::vault::Filter::default())
            .unwrap()
            .len(),
        before + 1
    );
}

#[tokio::test]
async fn ignored_stale_ssh_binding_is_reevaluated_after_a_new_source_revision() {
    let (_dir, mut c, _, _) = setup();
    let key = crate::vault::ssh::generate("health-fixture");
    let item = c
        .vault_create(
            "ssh",
            &Changes {
                title: Some("Health key".into()),
                set: Some(vec![FieldChange {
                    id: Some("private_key".into()),
                    value: Some(key.private_key),
                    ..Default::default()
                }]),
                ..Default::default()
            },
        )
        .unwrap();
    let reference = SecretRef {
        schema_version: 1,
        account_id: c.account().unwrap().id.to_string(),
        item_id: item.id.clone(),
        field_id: "private_key".into(),
    };
    let r = c
        .secret_ssh_binding_request("gui", reference, vec!["git_sign".into()])
        .unwrap();
    c.approval_respond(
        r["id"].as_str().unwrap(),
        r["digest"].as_str().unwrap(),
        true,
        "gui",
        false,
    )
    .await
    .unwrap();
    c.vault_edit(
        &item.id,
        None,
        &Changes {
            title: Some("First revision".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let find = |health: Value| {
        health["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["rule"] == "invalid_ssh_reference")
            .unwrap()
            .clone()
    };
    let first = find(c.secret_health().unwrap());
    c.secret_health_dismiss(first["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(find(c.secret_health().unwrap())["dismissed"], true);
    c.vault_edit(
        &item.id,
        None,
        &Changes {
            title: Some("Second revision".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let next = find(c.secret_health().unwrap());
    assert_ne!(first["id"], next["id"]);
    assert_eq!(next["dismissed"], false);
}

#[tokio::test]
async fn dependency_inventory_is_value_free_and_rotation_rejects_stale_acknowledgement() {
    let (dir, mut c, actor, p) = setup();
    let reference = p.credential.clone();
    let policy = configure(&mut c, &actor, p).await;
    let index = c.secret_references(&reference).unwrap();
    assert!(
        index
            .dependencies
            .iter()
            .any(|d| d.kind == "policy" && d.id == policy)
    );
    assert!(index.dependencies.iter().any(|d| d.kind == "policy_grant"));
    assert!(!serde_json::to_string(&index).unwrap().contains(TOKEN));
    let mut wrong = reference.clone();
    wrong.account_id = "00000000000000000000".into();
    assert!(c.secret_references(&wrong).is_err());
    let rotation = c.secret_rotation_create(reference.clone()).unwrap();
    let id = rotation["id"].as_str().unwrap();
    c.vault_edit(
        &reference.item_id,
        None,
        &Changes {
            set: Some(vec![FieldChange {
                id: Some(reference.field_id.clone()),
                value: Some("ROTATED_SYNTHETIC_ONLY".into()),
                ..Default::default()
            }]),
            ..Default::default()
        },
    )
    .unwrap();
    for step in ["created", "tested"] {
        c.secret_rotation_update(id, step, None).unwrap();
    }
    assert!(
        c.secret_rotation_update(id, "references_checked", Some(&index.digest))
            .is_err()
    );
    let current = c.secret_rotation_dependencies(id).unwrap();
    assert!(
        c.secret_rotation_update(id, "references_checked", None)
            .is_err()
    );
    c.secret_rotation_update(id, "references_checked", Some(&current.digest))
        .unwrap();
    c.secret_policy_disable(&policy).unwrap();
    assert!(
        c.secret_rotation_update(id, "upstream_revoked", None)
            .is_err()
    );
    let next = c.secret_rotation_dependencies(id).unwrap();
    assert_ne!(current.digest, next.digest);
    c.secret_rotation_update(id, "references_checked", Some(&next.digest))
        .unwrap();
    c.secret_rotation_update(id, "upstream_revoked", None)
        .unwrap();
    assert_eq!(c.secret_references(&reference).unwrap().digest, next.digest);
    c.lock();
    assert!(c.secret_references(&reference).is_err());
    drop(dir);
}

#[test]
fn each_unbacked_attachment_has_its_own_finding() {
    let dir = TempDir::new();
    let mut c = unlocked(&dir.0, false, None);
    let mut item = crate::vault::Item::new("note", "Two attachments", 0);
    let mut files = Vec::new();
    for name in ["a.txt", "b.txt"] {
        let mut file = c.secret_stage_start(name, "text/plain", 1).unwrap();
        c.secret_stage_part(&mut file, b"x").unwrap();
        c.secret_stage_finish(&mut file, None).unwrap();
        item.files.push(file.id().into());
        files.push(file);
    }
    c.secret_import_commit_with(vec![item], files, "root", |_, _| Ok(()))
        .unwrap();
    let health = c.secret_health().unwrap();
    let ids: Vec<_> = health["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["rule"] == "attachment_backup_unconfirmed")
        .map(|f| f["id"].clone())
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
}
