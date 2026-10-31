use super::*;
use crate::wallet::*;

fn get<'a>(value: &'a mut Value, name: &str) -> &'a mut Value {
    let Value::Map(fields) = value else {
        panic!("map")
    };
    &mut fields
        .iter_mut()
        .find(|(key, _)| key == &Value::Text(name.into()))
        .unwrap()
        .1
}

fn fixture() -> WalletView {
    let vectors: Json = serde_json::from_str(include_str!(
        "../../../../tests/vectors/wallet/plans-v1.json"
    ))
    .unwrap();
    let mut view: WalletView = decode(
        &hex::decode(vectors["view_cbor_hex"].as_str().unwrap()).unwrap(),
        MAX_PRIVATE,
    )
    .unwrap();
    view.members.push(Member {
        id: MemberId::Account(crate::testing::account().id),
        role: Role::Admin,
        label: "Original member".into(),
        status: MemberStatus::Active,
        agent_ids: vec![],
        bindings: vec![Binding {
            principal: Principal::from_slice(&[3; 29]),
            device: None,
            bound_at_ms: 0,
        }],
        added_at_ms: 1,
    });
    view
}

#[test]
fn wallet_snapshots_keep_current_bytes_and_reject_malformed_members() {
    let view = fixture();
    let encoded = canonical(&view);
    let restored: WalletSnapshot = decode(&encoded, MAX_PRIVATE).unwrap();
    assert_eq!(restored.view(), &view);
    assert_eq!(canonical(&restored), encoded);
    assert_eq!(canonical(&WalletSnapshot::from(view)), encoded);

    for case in [
        "unknown",
        "both",
        "wrong_type",
        "bad_principal",
        "duplicate",
        "missing",
    ] {
        let mut raw: Value = decode(&encoded, MAX_PRIVATE).unwrap();
        let Value::Array(members) = get(&mut raw, "members") else {
            panic!("members")
        };
        let Value::Map(fields) = &mut members[0] else {
            panic!("member")
        };
        let at = fields
            .iter()
            .position(|(key, _)| key == &Value::Text("bindings".into()))
            .unwrap();
        match case {
            "unknown" => fields.push((Value::Text("unrecognized".into()), Value::Bool(true))),
            "both" => fields.push((Value::Text("principals".into()), Value::Array(vec![]))),
            "missing" => {
                fields.remove(at);
            }
            _ => {
                fields[at] = (
                    Value::Text("principals".into()),
                    match case {
                        "wrong_type" => Value::Bool(false),
                        "bad_principal" => Value::Array(vec![Value::Bytes(vec![1; 30])]),
                        _ => Value::Array(vec![]),
                    },
                );
                if case == "duplicate" {
                    fields.push(fields[at].clone());
                }
            }
        }
        let bytes = if case == "duplicate" {
            cbor2::to_vec(&raw).unwrap()
        } else {
            canonical(&raw)
        };
        assert_eq!(
            decode::<WalletSnapshot>(&bytes, MAX_PRIVATE),
            Err(Error::IntegrityFailed),
            "{case}"
        );
    }
}

#[tokio::test]
async fn approval_history_reads_pre_1_2_wallet_members_without_changing_commitments() {
    use crate::approval::{Action, State};
    use crate::codec::{MAX_RECORD, canonical, decode_value};
    use crate::testing::{TempDir, unlocked};
    use cbor2::Value;

    let view = fixture();
    let proposal = Proposal {
        id: 1,
        op_id: Hash::new([1; 32]),
        intent: Intent::RemoveCosigner(7),
        policy_version: view.policy.version,
        proposer: view.members[0].id.clone(),
        via_agent: None,
        digest: Hash::new([2; 32]),
        created_at_ms: 1,
        expires_at_ms: u64::MAX,
        requirement: Requirement {
            approvals: 1,
            admin: false,
            delay_ms: 0,
            window_amount: 0,
        },
        status: ProposalStatus::Ready,
        votes: vec![],
        ready_at_ms: Some(1),
        plans: 0,
        signed: vec![],
        broadcasts: vec![],
        outcome: None,
        failure: None,
        updated_at_ms: 1,
    };
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let now = now_ms();
    for (index, kind) in [
        JobKind::Decide {
            wallet: view.wallet,
            name: view.name.clone(),
            context: Box::new(view.clone().into()),
            proposal: Box::new(proposal.clone()),
            decision: Decision::Approve,
        },
        JobKind::Cosign {
            wallet: view.wallet,
            name: view.name.clone(),
            context: Box::new(view.clone().into()),
            proposal: Box::new(proposal),
            plan: None,
            cosigners: vec![],
            collect_only: false,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let job = WalletJob {
            id: hex::encode([index as u8; 32]),
            kind,
            approval: None,
            result: Some(json!({"recorded": true})),
        };
        let mut old = decode_value(&canonical(&job), MAX_RECORD).unwrap();
        let field = |value: &mut Value, name: &str| {
            let Value::Map(fields) = value else {
                panic!("map")
            };
            fields
                .iter()
                .position(|(key, _)| key == &Value::Text(name.into()))
                .unwrap()
        };
        let kind = get(&mut old, "kind");
        let Value::Map(variants) = kind else {
            panic!("kind")
        };
        let context = get(&mut variants[0].1, "context");
        let Value::Array(members) = get(context, "members") else {
            panic!("members")
        };
        for member in members {
            let at = field(member, "bindings");
            let Value::Map(fields) = member else {
                panic!("member")
            };
            let (_, Value::Array(mut bindings)) = fields.remove(at) else {
                panic!("bindings")
            };
            let principals = bindings
                .iter_mut()
                .map(|b| get(b, "principal").clone())
                .collect();
            fields.push((Value::Text("principals".into()), Value::Array(principals)));
        }
        let original_digest = digest("dmsg/wallet-job/v1", &(&job.id, get(&mut old, "kind")));
        let original = canonical(&old);
        let key = format!("{JOB}{}", job.id);
        client
            .workspace
            .put_sealed(client.keys().unwrap(), &key, &original)
            .unwrap();
        let mut request = client
            .approval_create_at(
                "gui",
                Action::Wallet {
                    job: job.id.clone(),
                    digest: original_digest,
                },
                now,
                now + 60_000,
            )
            .unwrap();
        request.state = if index == 0 {
            State::Done
        } else {
            State::Rejected
        };
        client.save_approval(&request).unwrap();

        let shown = client.approval(&request.id).unwrap();
        assert_eq!(shown["detail"]["proposal"]["id"], 1);
        assert_eq!(
            shown["detail"]["before"]["members"][0]["principals"],
            wallet_view(&view, None)["members"][0]["principals"]
        );
        assert_eq!(shown["detail"]["result"]["recorded"], true);
        assert_eq!(client.wallet_job_digest(&job.id).unwrap(), original_digest);
        let restored = client.wallet_job(&job.id).unwrap();
        assert_eq!(canonical(&restored), original);
        client.save_wallet_job(&restored).unwrap();
        assert_eq!(
            client
                .workspace
                .open_sealed(client.keys().unwrap(), &key)
                .unwrap()
                .unwrap()
                .as_slice(),
            original
        );
    }
    let history = client.approvals(true).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(
        crate::approval::list(&client.hold(), true).await.unwrap(),
        history
    );
    assert!(
        crate::approval::list(&client.hold(), false)
            .await
            .unwrap()
            .is_empty()
    );
    // Synthetic backend output for the real Electron history smoke test.
    if let Ok(path) = std::env::var("DMSG_WALLET_APPROVAL_FIXTURE") {
        std::fs::write(path, serde_json::to_vec(&history).unwrap()).unwrap();
    }
}
