use super::*;
use crate::crypto::{hpke_seal, sign, signing_public};
use crate::testing::{TempDir, unlocked};

const NOW: u64 = 1_790_000_000_000;
const SECRET: &str = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
const UPSTREAM: [u8; 32] = [4; 32];

fn delivery(id: &str, signed_at: u64, body: &str, signature: String) -> Delivery {
    Delivery {
        headers: BTreeMap::from([
            ("webhook-id".into(), id.into()),
            ("webhook-timestamp".into(), (signed_at / 1000).to_string()),
            ("webhook-signature".into(), signature),
        ]),
        body: ByteBuf::from(body.as_bytes().to_vec()),
        received_at: NOW,
    }
}

fn v1(secret: &str, id: &str, signed_at: u64, body: &str) -> String {
    format!(
        "v1,{}",
        webhook_signature(secret, id, &(signed_at / 1000).to_string(), body.as_bytes()).unwrap()
    )
}

fn v1a(id: &str, signed_at: u64, body: &str) -> String {
    let content = signed_content(id, &(signed_at / 1000).to_string(), body.as_bytes());
    format!("v1a,{}", STANDARD.encode(sign(&UPSTREAM, &content)))
}

fn secret(retire_at: Option<u64>) -> Secret {
    Secret {
        secret: SECRET.into(),
        created_at: 0,
        retire_at,
    }
}

#[test]
fn formats_event_timestamps() {
    assert_eq!(timestamp(0), "1970-01-01T00:00:00.000Z");
    assert_eq!(timestamp(NOW + 123), "2026-09-21T14:13:20.123Z");
    assert_eq!(timestamp(951_782_400_000), "2000-02-29T00:00:00.000Z");
}

#[test]
fn checks_v1_and_v1a_signatures() {
    let body = r#"{"eventId":"evt_1","name":"x","timestamp":"t","data":{}}"#;
    let ok = delivery("evt_1", NOW, body, v1(SECRET, "evt_1", NOW, body));
    let keys = [Hash::new(signing_public(&UPSTREAM))];
    assert_eq!(
        verify(&[secret(None)], &ok, None),
        Some(Verified {
            v1: true,
            v1a: None
        })
    );
    // Known upstream keys and no end-to-end signature.
    assert_eq!(
        verify(&[secret(None)], &ok, Some(&keys)),
        Some(Verified {
            v1: true,
            v1a: Some(false)
        })
    );
    // A secret retired before the delivery, another secret, an old timestamp.
    assert!(!verify(&[secret(Some(NOW - 1))], &ok, None).unwrap().v1);
    assert!(
        !verify(
            &[Secret {
                secret: "whsec_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
                ..secret(None)
            }],
            &ok,
            None
        )
        .unwrap()
        .v1
    );
    let stale = NOW - SKEW_MS - 1000;
    let old = delivery("evt_1", stale, body, v1(SECRET, "evt_1", stale, body));
    assert!(!verify(&[secret(None)], &old, None).unwrap().v1);

    let both = format!(
        "{} {}",
        v1(SECRET, "evt_1", NOW, body),
        v1a("evt_1", NOW, body)
    );
    let signed = delivery("evt_1", NOW, body, both);
    assert_eq!(
        verify(&[secret(None)], &signed, Some(&keys)),
        Some(Verified {
            v1: true,
            v1a: Some(true)
        })
    );
    // A v1a no published key verifies: the relay changed the body (DT16).
    let tampered = Delivery {
        body: ByteBuf::from(body.replace("evt_1", "evt_2").into_bytes()),
        ..delivery(
            "evt_1",
            NOW,
            body,
            format!(
                "{} {}",
                v1(SECRET, "evt_1", NOW, body),
                v1a("evt_1", NOW, body)
            ),
        )
    };
    assert_eq!(verify(&[secret(None)], &tampered, Some(&keys)), None);
    // Without published keys the v1a cannot be judged.
    assert_eq!(
        verify(&[secret(None)], &tampered, None),
        Some(Verified {
            v1: false,
            v1a: None
        })
    );
}

#[test]
fn reads_upstream_keys_and_checks_servers_and_context() {
    let jwks = json!({"keys": [
        {"kty": "OKP", "crv": "Ed25519", "kid": "a", "x": crate::codec::b64(&signing_public(&UPSTREAM))},
        {"kty": "EC", "crv": "P-256", "kid": "b", "x": "AA"},
    ]});
    assert_eq!(jwks_keys(&jwks), vec![Hash::new(signing_public(&UPSTREAM))]);
    assert_eq!(
        server_origin("https://mcp.example.com/mcp?x=1", false).unwrap(),
        "https://mcp.example.com"
    );
    assert!(server_origin("http://mcp.example.com/mcp", true).is_err());
    assert!(server_origin("http://127.0.0.1:9/mcp", false).is_err());
    assert!(server_origin("http://127.0.0.1:9/mcp", true).is_ok());
    let mut context = BTreeMap::from([("session".to_string(), "t".repeat(256))]);
    assert!(check_context(&context).is_ok());
    context.insert("instructions".into(), "i".repeat(4097));
    assert!(check_context(&context).is_err());
    assert!(check_context(&BTreeMap::from([("other".to_string(), String::new())])).is_err());
}

fn endpoint(id: &str, agent: Option<&str>, session: &str, mcp: Option<&str>) -> Endpoint {
    Endpoint {
        endpoint_id: id.into(),
        url: format!("https://api.dmsg.test/mcp-events/{id}"),
        secrets: vec![secret(None)],
        agent: agent.map(str::to_string),
        mcp_session: mcp.map(str::to_string),
        context: BTreeMap::from([("session".to_string(), session.to_string())]),
        binding: Binding::default(),
        state: "active".into(),
        relay_seq: 0,
        created_at: NOW,
        ended_at: None,
    }
}

/// A relay record of `body` sealed to the client's device as the relay
/// seals it, with `extra` after the `v1` signature.
fn sealed_with(
    client: &Client,
    id: &str,
    seq: u64,
    webhook: &str,
    body: &str,
    extra: &str,
) -> Json {
    let now = now_ms();
    let delivery = Delivery {
        headers: BTreeMap::from([
            ("webhook-id".into(), webhook.into()),
            ("webhook-timestamp".into(), (now / 1000).to_string()),
            (
                "webhook-signature".into(),
                format!("{} {extra}", v1(SECRET, webhook, now, body))
                    .trim()
                    .to_string(),
            ),
            ("x-mcp-subscription-id".into(), "sub_1".into()),
        ]),
        body: ByteBuf::from(body.as_bytes().to_vec()),
        received_at: now,
    };
    let account = client.account().unwrap().id.to_string();
    let device = hex::encode(*client.workspace.meta.device_id);
    let envelope = hpke_seal(
        &client.workspace.meta.hpke_pub,
        &canonical(&delivery),
        &(INFO, account.as_str(), device.as_str(), id, seq),
    )
    .unwrap();
    let sealed = Sealed {
        enc: ByteBuf::from(envelope.enc),
        ciphertext: ByteBuf::from(envelope.ciphertext),
    };
    let kind = if body.contains("\"type\"") {
        "verification"
    } else {
        "event"
    };
    json!({
        "seq": seq,
        "kind": kind,
        "webhook_id": webhook,
        "received_at": now,
        "sealed": crate::codec::b64(&canonical(&sealed)),
    })
}

fn sealed_record(client: &Client, id: &str, seq: u64, webhook: &str, body: &str) -> Json {
    sealed_with(client, id, seq, webhook, body, "")
}

#[test]
fn local_retention_reports_loss_only_for_selected_endpoints() {
    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    client
        .save_endpoints(&[
            endpoint("a", Some("did:agent:a"), "a", None),
            endpoint("b", Some("did:agent:b"), "b", None),
        ])
        .unwrap();
    for (id, ep, at) in [
        ("1", "a", 1),
        ("2", "b", 100),
        ("3", "a", 3),
        ("4", "a", 100),
    ] {
        client
            .store_record(
                ep,
                id,
                &Record {
                    kind: "event".into(),
                    received_at: at,
                    subscription_id: None,
                    upstream: json!({"eventId": id}),
                    verified: None,
                    detail: Json::Null,
                },
            )
            .unwrap();
    }
    client.workspace.store.prune_mcp_events(10, 100).unwrap();
    let a = Filter {
        agent: Some("did:agent:a".into()),
        ..Default::default()
    };
    let b = Filter {
        agent: Some("did:agent:b".into()),
        ..Default::default()
    };
    let page = client.mcp_events_read(&a, 0, 100).unwrap();
    assert_eq!(page["truncated"], true);
    assert_eq!(page["events"][0]["eventId"], "a:4");
    assert_eq!(page["cursor"], "4");
    assert_eq!(
        client.mcp_events_read(&a, 4, 100).unwrap()["truncated"],
        false
    );
    assert_eq!(
        client.mcp_events_read(&b, 0, 100).unwrap()["truncated"],
        false
    );
    assert_eq!(
        client.mcp_events_read(
            &Filter {
                agent: Some("did:agent:b".into()),
                endpoint_ids: vec!["a".into()],
                ..Default::default()
            },
            0,
            100
        ),
        Err(Error::NotFound)
    );
    // A watermark must not skip an earlier surviving record on another endpoint.
    let page = client.mcp_events_read(&Filter::default(), 0, 1).unwrap();
    assert_eq!(page["events"][0]["eventId"], "b:2");
    assert_eq!(page["cursor"], "2");
    assert_eq!(page["hasMore"], true);
    let next = client.mcp_events_read(&Filter::default(), 2, 1).unwrap();
    assert_eq!(next["events"][0]["eventId"], "a:4");
    // Explicit acknowledgement creates no loss on the unaffected endpoint.
    client.mcp_events_ack(&b, 2).unwrap();
    assert_eq!(
        client.mcp_events_read(&b, 0, 100).unwrap()["truncated"],
        false
    );
}

#[test]
fn an_empty_truncated_page_advances_once_and_nonprefix_loss_keeps_survivors() {
    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    client
        .save_endpoints(&[endpoint("a", None, "a", None)])
        .unwrap();
    for (id, at) in [("1", 100), ("2", 100), ("3", 1), ("4", 1)] {
        client
            .store_record(
                "a",
                id,
                &Record {
                    kind: "event".into(),
                    received_at: at,
                    subscription_id: None,
                    upstream: json!({"eventId": id}),
                    verified: None,
                    detail: Json::Null,
                },
            )
            .unwrap();
    }
    client.workspace.store.prune_mcp_events(10, 2).unwrap();
    for after in 0..2 {
        let page = client
            .mcp_events_read(&Filter::default(), after, 1)
            .unwrap();
        assert_eq!(page["events"][0]["eventId"], format!("a:{}", after + 1));
        assert_eq!(page["cursor"], (after + 1).to_string());
        assert_eq!(page["truncated"], true);
    }
    let empty = client.mcp_events_read(&Filter::default(), 2, 100).unwrap();
    assert_eq!(empty["events"], json!([]));
    assert_eq!(empty["truncated"], true);
    assert_eq!(empty["cursor"], "4");
    let resumed = client.mcp_events_read(&Filter::default(), 4, 100).unwrap();
    assert_eq!(resumed["cursor"], "4");
    assert_eq!(resumed["truncated"], false);
    // A fully pruned inbox still has its durable loss position.
    client.workspace.store.prune_mcp_events(101, 0).unwrap();
    let empty = client.mcp_events_read(&Filter::default(), 0, 100).unwrap();
    assert_eq!(empty["events"], json!([]));
    assert_eq!(empty["cursor"], "4");
    assert_eq!(empty["truncated"], true);
}

#[test]
fn stores_reads_and_acknowledges_deliveries_per_caller() {
    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let mine = "00112233445566778899aabbccddeeff";
    let theirs = "ffeeddccbbaa99887766554433221100";
    client
        .save_endpoints(&[
            endpoint(mine, Some("did:agent:a"), "trigger-1", Some("mcp-1")),
            endpoint(theirs, Some("did:agent:b"), "trigger-2", None),
        ])
        .unwrap();
    let endpoints = client.endpoints().unwrap();
    let account = client.account().unwrap().id.to_string();
    let device = hex::encode(*client.workspace.meta.device_id);
    let verification = r#"{"type":"verification","challenge":"c"}"#;
    let event = r#"{"eventId":"evt_1","name":"issue.opened","timestamp":"2026-10-12T00:00:00Z","data":{"n":1},"cursor":"up_7"}"#;
    for (seq, webhook, body) in [(1, "msg_verification_1", verification), (2, "evt_1", event)] {
        let record = sealed_record(&client, mine, seq, webhook, body);
        assert_eq!(
            client
                .store_relay_record(&endpoints[0], &record, &account, &device, None)
                .unwrap(),
            Some(true)
        );
        // A repeated fetch stores nothing new.
        assert_eq!(
            client
                .store_relay_record(&endpoints[0], &record, &account, &device, None)
                .unwrap(),
            Some(false)
        );
    }
    // A record sealed for another place in the stream does not open.
    let moved = sealed_record(&client, mine, 9, "evt_9", event);
    let mut wrong = moved.clone();
    wrong["seq"] = 10.into();
    assert_eq!(
        client.store_relay_record(&endpoints[0], &wrong, &account, &device, None),
        Err(Error::IntegrityFailed)
    );
    client
        .store_record(
            theirs,
            "relay_gap:3",
            &Record {
                kind: "relay_gap".into(),
                received_at: NOW,
                subscription_id: None,
                upstream: Json::Null,
                verified: None,
                detail: json!({"dropped": 3}),
            },
        )
        .unwrap();

    let a = |filter: Filter| Filter {
        agent: Some("did:agent:a".into()),
        ..filter
    };
    let page = client
        .mcp_events_read(&a(Filter::default()), 0, 10)
        .unwrap();
    let events = page["events"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["name"], EVENT_NAME);
    assert_eq!(events[0]["data"]["kind"], "verification");
    assert_eq!(events[0]["data"]["upstream"]["challenge"], "c");
    assert_eq!(events[1]["eventId"], format!("{mine}:evt_1"));
    assert_eq!(events[1]["data"]["upstream"]["cursor"], "up_7");
    assert_eq!(events[1]["data"]["subscription_id"], "sub_1");
    assert_eq!(
        events[1]["data"]["verified"],
        json!({"v1": true, "v1a": null})
    );
    assert_eq!(events[1]["data"]["context"]["session"], "trigger-1");
    assert_eq!(page["cursor"], events[1]["cursor"]);
    assert_eq!(page["hasMore"], false);

    // Another agent's endpoint is invisible; the MCP session and the
    // context session select.
    assert_eq!(
        client.mcp_events_read(
            &a(Filter {
                endpoint_ids: vec![theirs.into()],
                ..Filter::default()
            }),
            0,
            10
        ),
        Err(Error::NotFound)
    );
    let session = |mcp: &str| {
        client
            .mcp_events_read(
                &a(Filter {
                    mcp_session: Some(mcp.into()),
                    ..Filter::default()
                }),
                0,
                10,
            )
            .unwrap()["events"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!((session("mcp-1"), session("mcp-2")), (2, 0));
    let owner = client
        .mcp_events_read(
            &Filter {
                session: Some("trigger-2".into()),
                ..Filter::default()
            },
            0,
            10,
        )
        .unwrap();
    assert_eq!(owner["events"][0]["data"]["kind"], "relay_gap");
    assert_eq!(owner["events"][0]["data"]["dropped"], 3);

    // An acknowledgement forgets; the cursor after it still reads on.
    let first: u64 = events[0]["cursor"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        client.mcp_events_ack(&a(Filter::default()), first).unwrap()["acked"],
        1
    );
    let rest = client
        .mcp_events_read(&a(Filter::default()), 0, 10)
        .unwrap();
    assert_eq!(rest["events"].as_array().unwrap().len(), 1);
    let after = client
        .mcp_events_read(&a(Filter::default()), first, 10)
        .unwrap();
    assert_eq!(after["events"][0]["eventId"], format!("{mine}:evt_1"));
}

#[test]
fn rejects_a_failed_v1a_and_reminds_before_refresh() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let id = "00112233445566778899aabbccddeeff";
    let mut bound = endpoint(id, None, "s", None);
    let now = now_ms();
    bound.binding = Binding {
        server: Some("https://mcp.example.com/mcp".into()),
        subscription_id: Some("sub_1".into()),
        refresh_before: Some(now + 60 * MINUTE),
        bound_at: now,
        due_for: None,
    };
    client.save_endpoints(&[bound]).unwrap();
    assert_eq!(
        client.mcp_servers().unwrap(),
        vec!["https://mcp.example.com"]
    );
    let endpoints = client.endpoints().unwrap();
    let account = client.account().unwrap().id.to_string();
    let device = hex::encode(*client.workspace.meta.device_id);
    let keys = [Hash::new(signing_public(&UPSTREAM))];
    let record = sealed_record(&client, id, 1, "evt_1", r#"{"eventId":"evt_1","name":"x"}"#);
    // A delivery without v1a from an upstream that publishes keys stays,
    // marked as not end-to-end verified; one whose v1a fails is dropped.
    assert_eq!(
        client.store_relay_record(&endpoints[0], &record, &account, &device, Some(&keys)),
        Ok(Some(true))
    );
    let page = client.mcp_events_read(&Filter::default(), 0, 10).unwrap();
    assert_eq!(
        page["events"][0]["data"]["verified"],
        json!({"v1": true, "v1a": false})
    );
    let forged = v1a("evt_2", now_ms(), "another body");
    let record = sealed_with(
        &client,
        id,
        2,
        "evt_2",
        r#"{"eventId":"evt_2","name":"x"}"#,
        &forged,
    );
    assert_eq!(
        client.store_relay_record(&endpoints[0], &record, &account, &device, Some(&keys)),
        Ok(None)
    );
    let signed = r#"{"eventId":"evt_3","name":"x"}"#;
    let genuine = v1a("evt_3", now_ms(), signed);
    let record = sealed_with(&client, id, 3, "evt_3", signed, &genuine);
    assert_eq!(
        client.store_relay_record(&endpoints[0], &record, &account, &device, Some(&keys)),
        Ok(Some(true))
    );
    let page = client.mcp_events_read(&Filter::default(), 1, 10).unwrap();
    assert_eq!(page["events"].as_array().unwrap().len(), 1);
    assert_eq!(
        page["events"][0]["data"]["verified"],
        json!({"v1": true, "v1a": true})
    );

    // The reminder comes ten minutes before an hour's grant, once.
    assert_eq!(client.mcp_refresh_due(now + 49 * MINUTE).unwrap(), 0);
    assert_eq!(client.mcp_refresh_due(now + 51 * MINUTE).unwrap(), 1);
    assert_eq!(client.mcp_refresh_due(now + 52 * MINUTE).unwrap(), 0);
    let page = client.mcp_events_read(&Filter::default(), 0, 10).unwrap();
    let due = &page["events"][2]["data"];
    assert_eq!(due["kind"], "refresh_due");
    assert_eq!(due["refresh_before"], now + 60 * MINUTE);
    assert_eq!(due["subscription_id"], "sub_1");
    // A short grant is reminded at a fifth of it.
    let mut endpoints = client.endpoints().unwrap();
    endpoints[0].binding.refresh_before = Some(now + 5 * MINUTE);
    client.save_endpoints(&endpoints).unwrap();
    assert_eq!(client.mcp_refresh_due(now + 3 * MINUTE).unwrap(), 0);
    assert_eq!(client.mcp_refresh_due(now + 4 * MINUTE).unwrap(), 1);
}
