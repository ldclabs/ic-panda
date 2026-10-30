use super::*;
use crate::approval::{MAX_PENDING, MAX_PER_SOURCE, State};
use crate::testing::{TempDir, unlocked};
use dmsg_types::MINUTE;
use dmsg_types::app_action::{
    ActionArgs, ActionCommand, ChoiceOption, CommandSchema, SchemaFields,
};

fn text(text: &str) -> StatementInput {
    StatementInput::Text {
        text: text.into(),
        subject: None,
    }
}

fn file_statement(text: &str, sha256: Option<String>) -> StatementInput {
    StatementInput::FileStatement {
        text: text.into(),
        sha256,
        content_type: None,
        location: None,
        subject: None,
    }
}

fn label(text: &str) -> Vec<ActionLabel> {
    vec![
        ActionLabel {
            locale: "en".into(),
            text: text.into(),
        },
        ActionLabel {
            locale: "zh".into(),
            text: format!("{text}（中文）"),
        },
    ]
}

/// A schema with one command over a number, a choice and a flag.
fn schema() -> ActionSchema {
    ActionSchema {
        version: 1,
        commands: vec![CommandSchema {
            name: "Publish".into(),
            title: label("Publish the list"),
            fields: SchemaFields(vec![
                FieldSchema {
                    name: "revision".into(),
                    label: label("Revision"),
                    ty: FieldType::Nat { min: 1, max: 100 },
                },
                FieldSchema {
                    name: "channel".into(),
                    label: label("Channel"),
                    ty: FieldType::Choice {
                        options: vec![
                            ChoiceOption {
                                value: "stable".into(),
                                label: label("Stable"),
                            },
                            ChoiceOption {
                                value: "beta".into(),
                                label: label("Beta"),
                            },
                        ],
                    },
                },
                FieldSchema {
                    name: "notify".into(),
                    label: label("Notify watchers"),
                    ty: FieldType::Bool {
                        yes: label("Yes, notify"),
                        no: label("Do not notify"),
                    },
                },
            ]),
        }],
    }
}

fn action(schema_hash: Hash) -> AppAction {
    AppAction {
        version: 1,
        environment: dmsg_types::Environment::Local,
        app_id: "token-list".into(),
        app_config_version: 1,
        origin: "https://app.example".into(),
        receiver: Principal::from_slice(&[9; 10]),
        actor: vec![8; 12].into(),
        signing_account: crate::testing::account().id,
        operation_id: Hash::new([1; 32]),
        intent_hash: Hash::new([2; 32]),
        schema_hash,
        issued_at_ms: 1,
        expires_at_ms: 2,
        command: ActionCommand {
            name: "Publish".into(),
            args: ActionArgs(vec![
                ActionArg {
                    name: "revision".into(),
                    value: ActionValue::Nat(7),
                },
                ActionArg {
                    name: "channel".into(),
                    value: ActionValue::Choice("beta".into()),
                },
                ActionArg {
                    name: "notify".into(),
                    value: ActionValue::Bool(false),
                },
            ]),
        },
        files: vec![],
    }
}

/// A finished attestation of `device`, as the home would have recorded it.
fn finished(device: Hash, text: &str, created_at: u64) -> Attestation {
    let account = crate::testing::account();
    Attestation {
        approval: "ab".repeat(32),
        request: Signed::Document(AttestRequest {
            account_id: account.id,
            statement: Statement {
                issuer: account.issuer.clone(),
                subject: None,
                issued_at: None,
                content: StatementContent::Text(text.into()),
            },
            origin: DMSG_ORIGIN.into(),
            signature: Ed25519Signature::new([3; 64]),
            approval: Approval {
                device_id: device,
                security_epoch: 1,
                sequence: created_at,
                request_id: Hash::new([created_at as u8; 32]),
                expires_at: created_at + 60_000,
                signature: Ed25519Signature::new([4; 64]),
                hardware: None,
            },
        }),
        stage: Stage::Complete,
        artifact: Some(SignedArtifact {
            cose_sign1: vec![0xd2, 1, 2].into(),
            cose_key: vec![0xa1].into(),
        }),
        receipt: Some(CertifiedBatch {
            schema: 1,
            canister: account.home,
            certificate: vec![5; 8].into(),
            entries: vec![],
        }),
        error: None,
        created_at,
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
    assert_eq!(request["risk"], "normal");
    assert_eq!(request["detail"]["origin"], DMSG_ORIGIN);
    assert_eq!(request["detail"]["purpose"], "Statement");
    assert_eq!(
        request["detail"]["statement"]["issuer"],
        crate::testing::account().issuer
    );
    assert_eq!(client.approvals_waiting().unwrap(), 1);
    // Only known sources; a web origin comes only from the connect path.
    assert!(client.attest_request("browser", text("x"), None).is_err());
    assert!(client.attest_request("cli", text(""), None).is_err());

    // A file statement hashes the file now and checks it again on
    // approval; the review names it and compares it by size and time.
    let file = dir.path("release.bin");
    std::fs::write(&file, b"artifact").unwrap();
    let input = file_statement("I approve this build", None);
    let request = client
        .attest_request("gui", input, Some(local_file(&file).unwrap()))
        .unwrap();
    assert_eq!(request["detail"]["file_checked"], true);
    assert_eq!(request["detail"]["file"]["name"], "release.bin");
    assert_eq!(request["detail"]["file"]["size"], 8);
    assert_eq!(
        request["detail"]["statement"]["content"]["sha256"],
        hex::encode(*file_sha256(&file).unwrap())
    );
    std::fs::write(&file, b"changed").unwrap();
    assert_eq!(
        client.approval(request["id"].as_str().unwrap()).unwrap()["detail"]["file_checked"],
        false
    );

    // A digest given as such names no file.
    let input = file_statement("I approve this version", Some("ab".repeat(32)));
    let request = client.attest_request("gui", input, None).unwrap();
    assert_eq!(
        request["detail"]["statement"]["content"]["sha256"],
        "ab".repeat(32)
    );
    assert!(request["detail"]["file_checked"].is_null());
    let both = file_statement("both", Some("ab".repeat(32)));
    assert!(
        client
            .attest_request("gui", both, Some(local_file(&file).unwrap()))
            .is_err()
    );
}

#[test]
fn a_library_version_is_checked_against_the_library() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let docs = client.files_mkdir_path("docs").unwrap();
    let imported = client
        .files_write(&docs.id, "report.pdf", b"report bytes")
        .unwrap();
    let source = FileSource::Library(&imported.file.id, &imported.version);
    let request = client
        .attest_request("gui", file_statement("Final", None), Some(source))
        .unwrap();
    let detail = &request["detail"];
    assert_eq!(detail["file"]["name"], "report.pdf");
    assert_eq!(detail["file"]["library"]["file"], imported.file.id.as_str());
    assert_eq!(detail["file_checked"], true);
    assert_eq!(
        detail["statement"]["content"]["sha256"],
        crate::codec::hex_sha256(b"report bytes")
    );
    // An unknown version names nothing.
    assert!(
        client
            .attest_request(
                "gui",
                file_statement("Final", None),
                Some(FileSource::Library(&imported.file.id, "00"))
            )
            .is_err()
    );
}

#[test]
fn a_person_compares_a_file_with_a_digest() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let sha = crate::codec::hex_sha256(b"the original");
    let request = client
        .attest_request("cli", file_statement("Release", Some(sha.clone())), None)
        .unwrap();
    let id = request["id"].as_str().unwrap();
    let digest = request["digest"].clone();
    let other = Hash::new(Sha256::digest(b"another file").into());
    let checked = client
        .approval_file_checked(id, "copy.bin", 12, other)
        .unwrap();
    assert_eq!(checked["file_check"]["matches"], false);
    let original = Hash::new(Sha256::digest(b"the original").into());
    let checked = client
        .approval_file_checked(id, "original.bin", 12, original)
        .unwrap();
    assert_eq!(checked["file_check"]["name"], "original.bin");
    assert_eq!(checked["file_check"]["matches"], true);
    // Comparing changes nothing that is approved.
    assert_eq!(checked["digest"], digest);
    // A text statement has no digest to compare.
    let plain = client.attest_request("cli", text("plain"), None).unwrap();
    assert!(matches!(
        client.approval_file_checked(plain["id"].as_str().unwrap(), "x", 1, original),
        Err(Error::InvalidInput(_))
    ));
}

#[tokio::test]
async fn answers_bind_the_request_as_shown() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let request = client.attest_request("cli", text("shown"), None).unwrap();
    let id = request["id"].as_str().unwrap();
    let digest = request["digest"].as_str().unwrap();
    assert_eq!(
        client
            .approval_respond(id, &"00".repeat(32), true, "gui", false)
            .await,
        Err(Error::IntegrityFailed)
    );
    // Approving needs a login: without one nothing runs and it keeps waiting.
    assert_eq!(
        client
            .approval_respond(id, digest, true, "gui", false)
            .await,
        Err(Error::AuthRequired)
    );
    let record = client.approval_record(id).unwrap();
    assert_eq!(record.state, State::AwaitingUser);
    assert_eq!(record.error.as_deref(), Some("AUTH_REQUIRED"));
    let rejected = client
        .approval_respond(id, digest, false, "cli", false)
        .await
        .unwrap();
    assert_eq!(rejected["state"], "rejected");
    // Who decided, where, and when.
    assert_eq!(rejected["decided_via"], "cli");
    assert!(rejected["decided_at"].as_u64().unwrap() >= rejected["created_at"].as_u64().unwrap());
    assert_eq!(
        client
            .approval_respond(id, digest, false, "cli", false)
            .await,
        Err(Error::Expired)
    );

    let withdrawn = client
        .attest_request("cli", text("withdrawn"), None)
        .unwrap();
    let cancelled = client
        .approval_cancel(withdrawn["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(cancelled["state"], "cancelled");
    // A unique prefix of six characters or more names a request.
    let id = withdrawn["id"].as_str().unwrap();
    assert_eq!(client.approval_id(&id[..6]).unwrap(), id);
    assert_eq!(client.approval_id(id).unwrap(), id);
    assert!(matches!(
        client.approval_id(&id[..5]),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        client.approval_id(""),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(client.approval_id("zzzzzz"), Err(Error::NotFound));

    // Past its deadline a request expires undecided, and cannot be withdrawn.
    let late = client.attest_request("cli", text("late"), None).unwrap();
    let mut record = client
        .approval_record(late["id"].as_str().unwrap())
        .unwrap();
    record.expires_at = crate::now_ms() - 1;
    client.save_approval(&record).unwrap();
    assert_eq!(
        client.approval_cancel(late["id"].as_str().unwrap()).err(),
        Some(Error::Expired)
    );
    assert_eq!(
        client.approval(late["id"].as_str().unwrap()).unwrap()["state"],
        "expired"
    );
}

#[tokio::test]
async fn a_high_risk_approval_waits_for_the_person() {
    use crate::secure::{Presence, PresenceKey};
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
    struct Asking(AtomicBool);
    impl PresenceKey for Asking {
        fn presence(&self) -> Option<Presence> {
            Some(Presence::Biometrics)
        }

        fn create(
            &self,
            _: bool,
        ) -> Result<(Vec<u8>, Vec<u8>, zeroize::Zeroizing<crate::crypto::Key>)> {
            Err(Error::Unavailable("test".into()))
        }

        fn open(
            &self,
            _: &[u8],
            _: &[u8],
            _: bool,
        ) -> Result<zeroize::Zeroizing<crate::crypto::Key>> {
            Err(Error::Unavailable("test".into()))
        }

        fn biometry_state(&self) -> Option<Hash> {
            None
        }

        fn confirm(&self, what: &str) -> Result<()> {
            assert_eq!(what, "device_revoke");
            match self.0.load(SeqCst) {
                true => Ok(()),
                false => Err(Error::Forbidden),
            }
        }
    }
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let request = client
        .approval_create(
            "gui",
            Action::DeviceRevoke {
                device_id: Hash::new([7; 32]),
            },
            now_ms() + 60_000,
        )
        .unwrap();
    let shown = client.approval(&request.id).unwrap();
    assert_eq!(shown["risk"], "high");
    let digest = shown["digest"].as_str().unwrap().to_string();
    // A device that cannot ask: nothing to confirm.
    assert!(client.approval_presence(&request.id).unwrap().is_none());
    let hardware = std::sync::Arc::new(Asking(AtomicBool::new(false)));
    client.set_presence_key(hardware.clone());
    let (crate::approval::Ask::Presence(key), what) =
        client.approval_presence(&request.id).unwrap().unwrap()
    else {
        panic!("a presence check")
    };
    // Declined: the request keeps waiting and says why.
    let error = key.confirm(&what).unwrap_err();
    client.approval_unconfirmed(&request.id, &error).unwrap();
    assert_eq!(
        client.approval(&request.id).unwrap()["error"],
        "PRESENCE_DECLINED"
    );
    // Unconfirmed, approving is refused before anything runs.
    assert_eq!(
        client
            .approval_respond(&request.id, &digest, true, "gui", false)
            .await,
        Err(Error::Forbidden)
    );
    let record = client.approval_record(&request.id).unwrap();
    assert_eq!(
        (record.state, record.error.as_deref()),
        (State::AwaitingUser, Some("PRESENCE_REQUIRED"))
    );
    // Rejecting needs no confirmation; a normal request asks nothing.
    let plain = client.attest_request("gui", text("plain"), None).unwrap();
    assert!(
        client
            .approval_presence(plain["id"].as_str().unwrap())
            .unwrap()
            .is_none()
    );
    let rejected = client
        .approval_respond(&request.id, &digest, false, "gui", false)
        .await
        .unwrap();
    assert_eq!(rejected["state"], "rejected");
}

#[test]
fn approval_history_keeps_a_cancelled_request_without_its_checkout() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let now = now_ms();
    let pending = client
        .attest_request("gui", text("still waiting"), None)
        .unwrap();
    // The checkout journal was discarded, but its approval is retained.
    let request = client
        .approval_create(
            "gui",
            Action::Checkout {
                id: hex::encode([42; 32]),
                digest: [43; 32].into(),
                fresh: false,
            },
            now + MINUTE,
        )
        .unwrap();
    let cancelled = client.approval_cancel(&request.id).unwrap();
    assert_eq!(cancelled["state"], "cancelled");
    assert_eq!(cancelled["detail"], Json::Null);
    assert_eq!(cancelled["digest"], hex::encode(*request.digest()));
    assert_eq!(client.approval(&request.id).unwrap(), cancelled);
    let history = client.approvals(true).unwrap();
    assert_eq!(history.len(), 2);
    assert!(history.contains(&cancelled));
    assert!(history.contains(&pending));
    assert_eq!(client.approvals(false).unwrap(), vec![pending]);
}

#[test]
fn missing_approval_details_are_allowed_only_for_finished_requests() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let now = now_ms();
    for action in [
        Action::Checkout {
            id: hex::encode([42; 32]),
            digest: [43; 32].into(),
            fresh: false,
        },
        Action::Handle {
            id: hex::encode([44; 32]),
            digest: [45; 32].into(),
        },
        Action::Connect {
            operation: "forgotten".into(),
            digest: [46; 32].into(),
            checkout: None,
        },
    ] {
        let mut request = client.approval_create("gui", action, now + MINUTE).unwrap();
        for state in [State::AwaitingUser, State::Executing, State::Unknown] {
            request.state = state;
            assert_eq!(client.approval_view(&request), Err(Error::NotFound));
        }
        for state in [
            State::Rejected,
            State::Expired,
            State::Cancelled,
            State::Done,
            State::Failed,
            State::Elsewhere,
            State::Lost,
        ] {
            request.state = state;
            let view = client.approval_view(&request).unwrap();
            assert_eq!(view["state"], state.name());
            assert_eq!(view["detail"], Json::Null);
        }
    }
}

#[test]
fn high_risk_requests_are_the_ones_held() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let mut risk = |action: Action| {
        let request = client
            .approval_create("gui", action, now_ms() + 60_000)
            .unwrap();
        client.approval_view(&request).unwrap()["risk"].clone()
    };
    assert_eq!(
        risk(Action::DeviceRevoke {
            device_id: Hash::new([1; 32])
        }),
        "high"
    );
    assert_eq!(
        risk(Action::Account {
            change: crate::security::AccountChange::RecoveryDelay {
                delay_ms: 2 * dmsg_types::DAY
            }
        }),
        "high"
    );
    assert_eq!(
        risk(Action::Account {
            change: crate::security::AccountChange::DisputeRecovery {
                op_id: Hash::new([2; 32])
            }
        }),
        "normal"
    );
    assert_eq!(
        risk(Action::DeviceCapabilities {
            device_id: Hash::new([1; 32]),
            capabilities: vec![Capability::FormalApprove],
        }),
        "high"
    );
    // Enrolling a device is held too, and its sealed record reads back.
    let pairing = client
        .approval_create(
            "gui",
            Action::DevicePair {
                token: "token".into(),
                device: dmsg_types::user::DeviceInput {
                    device_id: Hash::new([4; 32]),
                    signing_pub: Hash::new([5; 32]),
                    hpke_pub: Hash::new([6; 32]),
                    role: dmsg_types::user::ControllerRole::Administrator,
                    capabilities: vec![Capability::RootManage, Capability::VaultUnlock],
                },
                succession: false,
                moved: None,
            },
            now_ms() + 60_000,
        )
        .unwrap();
    let shown = client.approval(&pairing.id).unwrap();
    assert_eq!(shown["type"], "device_pair");
    assert_eq!(shown["risk"], "high");
    assert_eq!(shown["detail"]["role"], "Administrator");
    assert_eq!(shown["detail"]["device_id"], "04".repeat(32));
    let request = client
        .approval_create(
            "gui",
            Action::Handle {
                id: "a".repeat(64),
                digest: Hash::new([3; 32]),
            },
            now_ms() + 60_000,
        )
        .unwrap();
    // Paying for a name or giving one away is held; accepting or claiming one is not.
    let name = |action: &str, total: Json| {
        crate::approval::high_risk(&request, &json!({"action": action, "total": total}))
    };
    assert!(name("Transfer", Json::Null));
    assert!(name("Register", json!("10000000000")));
    assert!(!name("AcceptTransfer", Json::Null));
    assert!(!name("ClaimLegacy", Json::Null));
}

#[test]
fn the_queue_is_bounded_and_sealed() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    // One agent cannot fill the queue; the owner's clients still can.
    let agent = "agent:did:agent:z6Mktest";
    for i in 0..MAX_PER_SOURCE {
        client
            .attest_request(agent, text(&format!("from the agent {i}")), None)
            .unwrap();
    }
    assert_eq!(
        client.attest_request(agent, text("one more"), None).err(),
        Some(Error::QuotaExceeded)
    );
    for i in MAX_PER_SOURCE..MAX_PENDING {
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

#[test]
fn an_application_action_survives_the_approval_record() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let schema = schema();
    let action = action(dmsg_protocol::app_action::action_schema_hash(&schema));
    // As a typed statement the sealed record would not decode: a candid
    // Principal inside an internally tagged enum.
    let typed = Action::Attest {
        statement: Statement {
            issuer: "x".into(),
            subject: None,
            issued_at: None,
            content: StatementContent::AppAction(Box::new(action.clone())),
        },
        origin: action.origin.clone(),
        file: None,
    };
    assert!(decode::<Action>(&canonical(&typed), 65_536).is_err());
    let request = client
        .approval_create(
            "gui",
            Action::AttestAction {
                action: canonical(&action).into(),
                schema: Some(canonical(&schema).into()),
            },
            now_ms() + 60_000,
        )
        .unwrap();
    let shown = client.approval(&request.id).unwrap();
    assert_eq!(shown["type"], "attest");
    assert_eq!(shown["detail"]["origin"], "https://app.example");
    assert_eq!(shown["detail"]["purpose"], "AppAction");
    let content = &shown["detail"]["statement"]["content"];
    assert_eq!(shown["detail"]["statement"]["profile"], "app_action");
    assert_eq!(
        (content["app_id"].as_str(), content["command"].as_str()),
        (Some("token-list"), Some("Publish"))
    );
    // Without the schema, values read plainly; with it, by its labels.
    assert_eq!(content["args"][0]["value"], "7");
    let rendered = &shown["detail"]["action"];
    assert_eq!(rendered["title"]["en"], "Publish the list");
    assert_eq!(rendered["fields"][0]["label"]["zh"], "Revision（中文）");
    assert_eq!(rendered["fields"][0]["value"]["text"], "7");
    assert_eq!(rendered["fields"][1]["value"]["label"]["en"], "Beta");
    assert_eq!(
        rendered["fields"][2]["value"]["label"]["en"],
        "Do not notify"
    );
}

#[test]
fn an_action_renders_only_by_its_own_schema() {
    let schema = schema();
    let bytes = canonical(&schema);
    // The action names another schema: nothing renders.
    assert!(render_action(&action(Hash::new([3; 32])), Some(&bytes)).is_null());
    assert!(render_action(&action(Hash::new([3; 32])), None).is_null());
    let mut wrong = action(dmsg_protocol::app_action::action_schema_hash(&schema));
    wrong.command.args.0[1].value = ActionValue::Choice("nightly".into());
    assert!(render_action(&wrong, Some(&bytes)).is_null());
}

#[tokio::test]
async fn the_account_keeps_finished_attestations() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let own = client.workspace.meta.device_id;
    let mine = finished(own, "signed here", 10);
    client.save_attestation(&mine).unwrap();
    // Kept once, by its execution ID, and synced like any content object.
    assert_eq!(client.attest_backfill().unwrap(), 1);
    assert_eq!(client.attest_backfill().unwrap(), 0);
    let id = mine.execution_id();
    let (row, record) = client.current(&id).unwrap();
    assert_eq!(
        (row.kind.as_str(), record.kind.as_str()),
        (ATTESTATION, ATTESTATION)
    );
    assert_eq!(row.state, crate::store::LOCAL);
    assert_eq!(
        client.kept_attestation("wrong execution", &record).err(),
        Some(Error::IntegrityFailed)
    );

    // Another device's attestation reaches this one as a kept object.
    let theirs = finished(Hash::new([6; 32]), "signed elsewhere", 20);
    let evidence = Evidence {
        format: EVIDENCE_FORMAT.into(),
        home: "ryjl3-tyaaa-aaaaa-aaaba-cai".into(),
        attestation: theirs.clone(),
    };
    client
        .write(
            ATTESTATION,
            theirs.execution_id(),
            None,
            false,
            &canonical(&evidence),
            None,
        )
        .unwrap();
    let history = client.attest_history().unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0]["execution"], theirs.execution_id());
    assert_eq!(history[0]["this_device"], false);
    assert_eq!(history[1]["this_device"], true);
    assert_eq!(
        history[0]["statement"]["content"]["text"],
        "signed elsewhere"
    );
    // Its bundle names the home that certified it.
    let bundle = client.attest_bundle(&theirs.execution_id()).unwrap();
    assert_eq!(bundle.home, "ryjl3-tyaaa-aaaaa-aaaba-cai");
    assert_eq!(bundle.request_id, theirs.execution_id());
    // A device that lost its journal, as after a recovery, still exports its own.
    client
        .workspace
        .delete_private(&format!("{PREFIX}{id}"))
        .unwrap();
    assert_eq!(client.attest_bundle(&id).unwrap().request_id, id);
    assert_eq!(client.attest_history().unwrap().len(), 2);
    assert_eq!(
        crate::attest::history(&client.hold()).await.unwrap(),
        client.attest_history().unwrap()
    );

    // Exporting one execution must not scan/decrypt unrelated archive rows.
    let mut bad = row;
    bad.item = "ff".repeat(32);
    bad.revision = "bad".into();
    bad.key = format!("{}:{}", bad.item, bad.revision);
    bad.frame = vec![0];
    client.workspace.store.put_object(&bad).unwrap();
    client
        .workspace
        .store
        .set_head(&bad.item, &bad.revision, ATTESTATION)
        .unwrap();
    assert_eq!(
        client
            .attest_bundle(&theirs.execution_id())
            .unwrap()
            .request_id,
        theirs.execution_id()
    );
    assert!(client.attest_history().is_err());
}

/// The shared bundle vector: a file statement, its certified receipt, the
/// file and the test root key.
fn vector() -> (Bundle, Vec<u8>, Vec<u8>, Principal) {
    let vector: Json = serde_json::from_str(include_str!(
        "../../dmsg_types/tests/attestation_vector.json"
    ))
    .unwrap();
    let bundle: Bundle = serde_json::from_value(vector["bundle"].clone()).unwrap();
    let home = Principal::from_text(&bundle.home).unwrap();
    (
        bundle,
        hex::decode(vector["file_hex"].as_str().unwrap()).unwrap(),
        hex::decode(vector["root_der_hex"].as_str().unwrap()).unwrap(),
        home,
    )
}

#[test]
fn the_shared_bundle_verifies_check_by_check() {
    let dir = TempDir::new();
    let (bundle, file, root, home) = vector();
    let path = dir.path("release.txt");
    std::fs::write(&path, &file).unwrap();
    let report = verify_bundle(&bundle, Some(&path), &root, &[home]).unwrap();
    assert!(report.valid, "{report:?}");
    assert_eq!(report.content, "matches_file");
    assert_eq!(
        (report.signature, report.evidence, report.binding),
        (true, true, true)
    );
    assert_eq!(report.origin.as_deref(), Some("https://dmsg.net"));
    assert_eq!(report.security_epoch, Some(3));
    assert_eq!(report.device_id, Some(hex::encode([7u8; 32])));
    // Without the file, only the digest; with another, the content fails
    // and the signature stands.
    assert_eq!(
        verify_bundle(&bundle, None, &root, &[home])
            .unwrap()
            .content,
        "digest_only"
    );
    std::fs::write(&path, b"another file").unwrap();
    let changed = verify_bundle(&bundle, Some(&path), &root, &[home]).unwrap();
    assert_eq!(
        (changed.valid, changed.content, changed.signature),
        (false, "mismatch", true)
    );
    assert_eq!(changed.failed(), ["content"]);
    // Another root or a home outside the deployment: no evidence.
    let mut other = root.clone();
    *other.last_mut().unwrap() ^= 1;
    let report = verify_bundle(&bundle, None, &other, &[home]).unwrap();
    assert_eq!((report.evidence, report.binding), (false, false));
    let foreign = Principal::from_slice(&[2, 2]);
    assert!(
        !verify_bundle(&bundle, None, &root, &[foreign])
            .unwrap()
            .evidence
    );
    // A receipt of another request does not bind this artifact.
    let rebound = Bundle {
        request_id: hex::encode([9u8; 32]),
        ..bundle.clone()
    };
    let report = verify_bundle(&rebound, None, &root, &[home]).unwrap();
    assert!(report.signature && !report.evidence && !report.valid);
    // A changed signature fails, and the content can no longer be read.
    let mut sign1 = unb64(&bundle.cose_sign1).unwrap();
    *sign1.last_mut().unwrap() ^= 1;
    let tampered = Bundle {
        cose_sign1: b64(&sign1),
        ..bundle
    };
    let report = verify_bundle(&tampered, None, &root, &[home]).unwrap();
    assert_eq!(
        (report.signature, report.binding, report.content),
        (false, false, "unknown")
    );
}

#[test]
fn a_bundle_is_checked_check_by_check() {
    // Any canister can certify a receipt: one outside the deployment's user
    // homes vouches for no account, whatever it signed.
    let home = Principal::from_text("rrkah-fqaaa-aaaaa-aaaaq-cai").unwrap();
    let bundle = Bundle {
        format: BUNDLE_FORMAT.into(),
        account: crate::testing::account().id.to_string(),
        home: "ryjl3-tyaaa-aaaaa-aaaba-cai".into(),
        request_id: hex::encode([1u8; 32]),
        cose_sign1: String::new(),
        cose_key: String::new(),
        receipt: b64(&canonical(&CertifiedBatch {
            schema: 1,
            canister: home,
            certificate: vec![].into(),
            entries: vec![],
        })),
    };
    let report = verify_bundle(&bundle, None, &[], &[home]).unwrap();
    assert!(!report.valid);
    assert_eq!(
        (report.signature, report.evidence, report.binding),
        (false, false, false)
    );
    assert_eq!(report.content, "unknown");
    assert_eq!(report.failed(), ["signature", "evidence", "binding"]);
    assert!(report.device_id.is_none());
    // Another format is not a report.
    let other = Bundle {
        format: "something/1".into(),
        ..bundle
    };
    assert_eq!(
        verify_bundle(&other, None, &[], &[home]).err(),
        Some(Error::UnsupportedProtocol)
    );
}

#[test]
fn an_attestation_past_its_result_is_lost() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let mut request = client
        .approval_create(
            "cli",
            Action::Attest {
                statement: Statement {
                    issuer: crate::testing::account().issuer,
                    subject: None,
                    issued_at: None,
                    content: StatementContent::Text("gone".into()),
                },
                origin: DMSG_ORIGIN.into(),
                file: None,
            },
            now_ms() + 60_000,
        )
        .unwrap();
    request.state = State::Unknown;
    request.started = true;
    assert_eq!(
        client.finish_approval(&mut request, Err(Error::ResultExpired)),
        Err(Error::ResultExpired)
    );
    let shown = client.approval(&request.id).unwrap();
    assert_eq!(
        (shown["state"].as_str(), shown["error"].as_str()),
        (Some("lost"), Some("RESULT_EXPIRED"))
    );
}

#[test]
fn a_sequence_another_attestation_holds_is_not_signed_again() {
    use dmsg_types::user::{
        AccountInfo, ControllerRole, Device, DeviceInput, SensitivePolicy, VaultWriteState,
    };
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let account = crate::testing::account();
    let device_id = client.workspace.meta.device_id;
    let device = Device {
        input: DeviceInput {
            device_id,
            signing_pub: client.workspace.meta.signing_pub,
            hpke_pub: client.workspace.meta.hpke_pub,
            role: ControllerRole::Administrator,
            capabilities: vec![Capability::FormalApprove],
        },
        added_at: 1,
        added_by: None,
        revoked_at: None,
        next_sequence: 4,
    };
    let devices = std::collections::BTreeMap::from([(device_id, device)]);
    let state = crate::account::AccountState {
        info: AccountInfo {
            created_at_ms: 1,
            issuer: account.issuer.clone(),
            account_id: account.id,
            home_user: account.home,
            home_cose: account.home,
            auth_bindings: vec![],
            account_version: 1,
            security_epoch: 1,
            devices: devices.clone(),
            recovery_delay_ms: 0,
            pending_recovery: None,
            recovered_device: None,
            current_root: None,
            root_slot: None,
            vault_write_state: VaultWriteState::Ready,
            sensitive_policy: SensitivePolicy::default(),
            approval_keys: Default::default(),
            hardware_policy: vec![],
        },
        security: crate::testing::security(1, devices),
    };
    let subject = |text: &str| Subject::Statement {
        statement: Statement {
            issuer: account.issuer.clone(),
            subject: None,
            issued_at: None,
            content: StatementContent::Text(text.into()),
        },
        origin: DMSG_ORIGIN.into(),
    };
    // Two approvals read the account before either signed: the second does
    // not sign under the sequence the first took, nor replace its journal.
    let first = client
        .attest_sign_with("one", subject("one"), &state)
        .unwrap();
    assert_eq!(
        client.attest_sign_with("two", subject("two"), &state).err(),
        Some(Error::PolicyStale)
    );
    assert_eq!(
        client.attestation(&first.execution_id()).unwrap(),
        Some(first.clone())
    );
    // A sequence the home refused is free again.
    client
        .save_attestation(&Attestation {
            stage: Stage::Failed,
            ..first
        })
        .unwrap();
    let second = client
        .attest_sign_with("two", subject("two"), &state)
        .unwrap();
    assert_eq!(second.approval, "two");
}

#[tokio::test]
async fn the_reconciler_leaves_a_running_attestation_alone() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let created = client.attest_request("cli", text("running"), None).unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    let mut record = client.approval_record(&id).unwrap();
    record.state = State::Executing;
    client.save_approval(&record).unwrap();
    client.running.insert(id.clone(), now_ms());
    let reconcile = async |client: &mut Client| {
        crate::approval::reconcile_attestations(&client.hold())
            .await
            .unwrap()
    };
    assert_eq!(reconcile(&mut client).await, 0);
    assert_eq!(client.approval_record(&id).unwrap().state, State::Executing);
    // Left behind past its time, it is reconciled as any other: approved
    // and never signed, it cannot have run. The other requests' reconciler
    // leaves it alone.
    client.running.insert(id.clone(), 0);
    assert_eq!(client.approvals_reconcile().await.unwrap(), 0);
    assert_eq!(client.approval_record(&id).unwrap().state, State::Executing);
    assert_eq!(reconcile(&mut client).await, 1);
    assert_eq!(client.approval_record(&id).unwrap().state, State::Failed);
}

#[tokio::test]
async fn approval_lists_skip_file_metadata_and_queue_navigation_needs_only_the_index() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let path = dir.0.join("statement.bin");
    std::fs::write(&path, b"original bytes").unwrap();
    let request = client
        .attest_request(
            "gui",
            file_statement("original", None),
            Some(local_file(&path).unwrap()),
        )
        .unwrap();
    assert_eq!(request["detail"]["file_checked"], true);
    std::fs::remove_file(&path).unwrap();
    let listed = crate::approval::list(&client.hold(), true).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0]["detail"]["file_checked"].is_null());
    assert_eq!(
        client.approval(request["id"].as_str().unwrap()).unwrap()["detail"]["file_checked"],
        false
    );
    // Queue navigation and prefix lookup do not deserialize every request.
    let id = request["id"].as_str().unwrap();
    client
        .workspace
        .put_private(
            client.keys().unwrap(),
            &format!("approval:{id}"),
            &"corrupt request",
        )
        .unwrap();
    assert_eq!(
        crate::approval::waiting_ids(&client.hold()).await.unwrap(),
        vec![id]
    );
    assert_eq!(client.approval_id(id).unwrap(), id);
    assert!(client.approval(id).is_err());
}

#[tokio::test]
async fn current_device_checks_run_together_and_failure_keeps_recorded_evidence() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (bundle, _, root, home) = vector();
    let report = verify_bundle(&bundle, None, &root, &[home]).unwrap();
    assert!(report.valid);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = crate::testing::config(false);
    config.ic_host = format!("http://{}", listener.local_addr().unwrap());
    config.root_key = Some(root);
    let ic = crate::ic::Ic::anonymous(&config).await.unwrap();
    let read = tokio::spawn(async move { verify_current(&ic, report).await });
    // Each IC query also reads its subnet proof. Hold all four requests
    // before answering, so a sequential implementation cannot pass.
    let mut sockets = Vec::new();
    let mut requests = String::new();
    for _ in 0..4 {
        let mut socket = tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept())
            .await
            .expect("both queries and their proofs must start together")
            .unwrap()
            .0;
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 8192];
            let n = socket.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        requests.push_str(&String::from_utf8_lossy(&bytes));
        sockets.push(socket);
    }
    assert!(requests.contains("security_snapshot_batch") && requests.contains("get_device_bundle"));
    for mut socket in sockets {
        let _ = socket
            .write_all(
                b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await;
    }
    let report = tokio::time::timeout(std::time::Duration::from_secs(3), read)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(report.current, "unknown");
    assert!(report.valid && report.signature && report.evidence && report.binding);
}
