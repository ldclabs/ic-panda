// Copied from agent-protocols v0.12.0; its SdkError is large by design.
#![allow(clippy::result_large_err)]

use agent_protocols::{
    identity::{self, AgentSigner, Envelope, NonceStore, RequestBinding, RequestJwtClaims},
    mail::*,
};
use serde_json::Value;
fn vectors() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/vectors/agent-protocols/agent-mail-1.0.vectors.json"
    ))
    .unwrap()
}
fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
fn card(v: &Value, name: &str) -> MailboxCard {
    validate_card(&v["envelopes"][name]).unwrap()
}
fn packet(v: &Value, name: &str) -> Packet {
    validate_packet(&v["encryptions"][name]["packet"]).unwrap()
}
fn key(v: &Value, card: &MailboxCard) -> MailEncryptionKey {
    let original = validate_card(&v["envelopes"]["card"]).unwrap();
    let name = if original.event.payload.public_key == card.event.payload.public_key {
        "recipient_secret_hex"
    } else {
        "rotated_recipient_secret_hex"
    };
    MailEncryptionKey::from_secret(&hex(v["keys"][name].as_str().unwrap())).unwrap()
}
fn sender_signer(v: &Value) -> AgentSigner {
    AgentSigner::from_seed(
        hex(v["keys"]["sender_seed_hex"].as_str().unwrap())
            .try_into()
            .unwrap(),
    )
}
fn owner_signer(v: &Value) -> AgentSigner {
    AgentSigner::from_seed(
        hex(v["keys"]["recipient_seed_hex"].as_str().unwrap())
            .try_into()
            .unwrap(),
    )
}
fn token(signer: &AgentSigner, origin: &str, now: i64) -> String {
    signer
        .sign_request_jwt(&RequestJwtClaims::new(
            signer.agent_id(),
            RequestBinding::new(origin),
            now / 1000,
            300,
        ))
        .unwrap()
}
fn keyring(v: &Value) -> MailKeyring {
    let c = card(v, "card");
    let mut keys = MailKeyring::new(c.event.actor.clone());
    for name in ["card", "rotated_card", "reopened_card"] {
        let c = card(v, name);
        keys.add(&c, key(v, &c)).unwrap();
    }
    keys
}
fn code<T>(result: agent_protocols::Result<T>) -> Option<String> {
    result.err().map(|e| e.code().unwrap_or("?").to_owned())
}
const ORIGIN: &str = "https://relay.example";
#[test]
fn shared_schema_signing_and_strict_json_vectors() {
    let v = vectors();
    assert_eq!(
        serde_json::from_str::<Value>(SCHEMA_JSON).unwrap(),
        serde_json::from_str::<Value>(include_str!(
            "../../../tests/vectors/agent-protocols/agent-mail-1.0.schema.json"
        ))
        .unwrap()
    );
    for case in v["schema_cases"].as_array().unwrap() {
        assert_eq!(
            validate_mail_schema(&case["value"], case["definition"].as_str().unwrap()).is_ok(),
            case["valid"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
    for (name, s) in v["signing"].as_object().unwrap() {
        let value = &v["envelopes"][name];
        let e: Envelope<Value> = serde_json::from_value(value.clone()).unwrap();
        let signer =
            AgentSigner::from_seed(hex(s["seed_hex"].as_str().unwrap()).try_into().unwrap());
        assert_eq!(
            canonical_bytes(&e.event).unwrap(),
            s["event_jcs"].as_str().unwrap().as_bytes()
        );
        assert_eq!(
            canonical_bytes(&e).unwrap(),
            s["envelope_jcs"].as_str().unwrap().as_bytes()
        );
        assert_eq!(
            serde_json::to_value(signer.sign_event(e.event).unwrap()).unwrap(),
            *value
        );
        if value["event"]["type"] == "mailbox.publish" {
            validate_card(value).unwrap();
        } else {
            validate_packet(value).unwrap();
        }
    }
    for text in v["strict_json_rejections"].as_array().unwrap() {
        assert!(identity::parse_strict_json(text.as_str().unwrap()).is_err());
    }
    let card_json = v["envelopes"]["card"].to_string().replacen(
        "\"nonce\":100",
        "\"nonce\":100,\"nonce\":100",
        1,
    );
    assert!(parse_card(&card_json).is_err());
}
#[test]
fn shared_hpke_vectors_and_random_roundtrips() {
    let v = vectors();
    let now = v["now"].as_i64().unwrap();
    for (name, e) in v["encryptions"].as_object().unwrap() {
        let c = card(&v, e["card"].as_str().unwrap());
        let p = packet(&v, name);
        let l = validate_message(&v["messages"][e["message"].as_str().unwrap()]).unwrap();
        assert_eq!(hex(e["info_hex"].as_str().unwrap()), PROTOCOL.as_bytes());
        assert_eq!(
            canonical_bytes(&p).unwrap(),
            e["packet_jcs"].as_str().unwrap().as_bytes()
        );
        assert_eq!(packet_id(&p).unwrap(), e["packet_id"]);
        assert_eq!(
            *frame_bytes(&canonical_bytes(&l).unwrap()).unwrap(),
            decode_bytes(e["plaintext_b64"].as_str().unwrap()).unwrap()
        );
        assert_eq!(
            decrypt_packet(&p, &c, &key(&v, &c), &c.event.actor, now).unwrap(),
            l
        );
        let fresh = encrypt_message(&l, &c, &sender_signer(&v), 800, now).unwrap();
        assert_ne!(fresh.event.payload.enc, p.event.payload.enc);
        assert_eq!(
            decrypt_packet(&fresh, &c, &key(&v, &c), &c.event.actor, now).unwrap(),
            l
        );
    }
    for f in v["framing_boundaries"].as_array().unwrap() {
        let bytes = vec![b'x'; f["json_byte_length"].as_u64().unwrap() as usize];
        let frame = frame_bytes(&bytes).unwrap();
        assert_eq!(frame.len() as u64, f["frame_byte_length"]);
        assert_eq!(frame[..4], hex(f["length_prefix_hex"].as_str().unwrap()));
        assert_eq!(hash_bytes(&frame), f["plaintext_sha3_256"]);
    }
    let generated = MailEncryptionKey::generate().unwrap();
    let restored = MailEncryptionKey::from_secret(&generated.export_secret()).unwrap();
    assert_eq!(generated.public_key(), restored.public_key());
}
#[test]
fn official_rfc9180_known_answer_with_production_primitive() {
    use hpke::{Deserializable, Kem, Serializable, kem::X25519HkdfSha256 as K};
    let v = vectors();
    let kat = &v["rfc9180_known_answer"];
    let sk = <K as Kem>::PrivateKey::from_bytes(&hex(kat["skRm"].as_str().unwrap())).unwrap();
    let enc = <K as Kem>::EncappedKey::from_bytes(&hex(kat["pkEm"].as_str().unwrap())).unwrap();
    assert_eq!(
        K::sk_to_pk(&sk).to_bytes().as_slice(),
        hex(kat["pkRm"].as_str().unwrap())
    );
    assert_eq!(
        K::decap(&sk, None, &enc).unwrap().0.as_slice(),
        hex(kat["shared_secret"].as_str().unwrap())
    );
    let pt = hpke::single_shot_open::<hpke::aead::ChaCha20Poly1305, hpke::kdf::HkdfSha256, K>(
        &hpke::OpModeR::Base,
        &sk,
        &enc,
        &hex(kat["info"].as_str().unwrap()),
        &hex(kat["ct"].as_str().unwrap()),
        &hex(kat["aad"].as_str().unwrap()),
    )
    .unwrap();
    assert_eq!(pt, hex(kat["pt"].as_str().unwrap()));
}
#[test]
fn every_shared_recipient_rejection() {
    let v = vectors();
    let now = v["now"].as_i64().unwrap();
    for case in v["recipient_rejections"].as_array().unwrap() {
        let c = card(&v, case["card"].as_str().unwrap_or("card"));
        let secret = case["secret_hex"]
            .as_str()
            .unwrap_or(v["keys"]["recipient_secret_hex"].as_str().unwrap());
        let k = MailEncryptionKey::from_secret(&hex(secret)).unwrap();
        let owner = case
            .get("owner")
            .map(|s| serde_json::from_value(s.clone()).unwrap())
            .unwrap_or(c.event.actor.clone());
        let outcome = (|| {
            let p = validate_packet(&case["packet"])?;
            if let Some(id) = case["packet_id"].as_str()
                && packet_id(&p)? != id
            {
                return Err(agent_protocols::SdkError::protocol(
                    "invalid_event",
                    "wrong id",
                ));
            }
            decrypt_packet(&p, &c, &k, &owner, now)
        })();
        assert_eq!(
            code(outcome).as_deref(),
            case["code"].as_str(),
            "{}",
            case["name"]
        );
    }
}
#[test]
fn shared_sender_discovery_and_authorization_cases() {
    let v = vectors();
    let c = card(&v, "card");
    let now = v["now"].as_i64().unwrap();
    for case in v["sender_card_cases"].as_array().unwrap() {
        assert!(
            validate_card_for_sending(&case["card"], &c.event.actor, case["now"].as_i64().unwrap())
                .is_err(),
            "{}",
            case["name"]
        );
    }
    for case in v["discovery_cases"].as_array().unwrap() {
        assert_eq!(
            validate_discovery(&case["value"], ORIGIN).is_ok(),
            case["expected"] == "valid",
            "{}",
            case["name"]
        );
    }
    for case in v["owner_jwt_cases"].as_array().unwrap() {
        let result = verify_owner_jwt(case["token"].as_str().unwrap(), &c.event.actor, ORIGIN, now);
        let expected = case["expected"].as_str().unwrap();
        assert_eq!(
            code(result).as_deref(),
            (expected != "valid").then_some(expected),
            "{}",
            case["name"]
        );
    }
    let jwt = v["owner_jwt_cases"][0]["token"].as_str().unwrap();
    assert_eq!(
        code(verify_owner_jwt(jwt, &c.event.actor, ORIGIN, now + 301_000)).as_deref(),
        Some("invalid_token")
    );
    let parts: Vec<_> = jwt.split('.').collect();
    let claims = String::from_utf8(decode_bytes(parts[1]).unwrap()).unwrap();
    let duplicate = claims.replacen("{", "{\"exp\":1790726700,", 1);
    let malicious = format!(
        "{}.{}.{}",
        parts[0],
        encode_bytes(duplicate.as_bytes()),
        parts[2]
    );
    assert!(verify_owner_jwt(&malicious, &c.event.actor, ORIGIN, now).is_err());
}
#[test]
fn shared_card_cache_lifecycles_survive_snapshots() {
    let v = vectors();
    let c = card(&v, "card");
    let now = v["now"].as_i64().unwrap();
    for scenario in ["card_cache", "persistent_card_pin"] {
        let mut cache = MailCardCache::new();
        for step in v["lifecycle"][scenario].as_array().unwrap() {
            if let Some(at) = step["prune"].as_i64() {
                cache.prune(at).unwrap();
            } else {
                let expected = match step["expected"].as_str().unwrap() {
                    "usable" => None,
                    "closed" => Some("mailbox_unavailable"),
                    _ => Some("stale_card"),
                };
                let result = cache.observe(
                    &v["envelopes"][step["card"].as_str().unwrap()],
                    &c.event.actor,
                    step["now"].as_i64().unwrap_or(now),
                );
                assert_eq!(code(result).as_deref(), expected, "{step}");
            }
            cache = MailCardCache::from_snapshot(&cache.snapshot().unwrap()).unwrap();
        }
    }
}
#[test]
fn shared_recipient_lifecycle_and_replies() {
    let v = vectors();
    let mut inbox = MailInbox::new(keyring(&v));
    for step in v["lifecycle"]["recipient"].as_array().unwrap() {
        let p = packet(&v, step["packet"].as_str().unwrap());
        let out = inbox
            .accept(&p, None, step["now"].as_i64().unwrap())
            .unwrap();
        assert_eq!(
            matches!(out, InboxAcceptance::Accepted(_)),
            step["expected"] == "accepted"
        );
        let snapshot = inbox.snapshot().unwrap();
        assert_eq!(
            snapshot["accepted"].as_object().unwrap().len() as u64,
            step["items"]
        );
        let keys = MailKeyring::from_snapshot(&inbox.keyring().snapshot().unwrap()).unwrap();
        inbox = MailInbox::from_snapshot(keys, &snapshot).unwrap();
    }
    let history = &v["lifecycle"]["historical_card"];
    assert!(
        MailInbox::new(keyring(&v))
            .accept(
                &packet(&v, "original"),
                None,
                history["now"].as_i64().unwrap()
            )
            .is_ok()
    );
    let expired = &v["lifecycle"]["expired_new_message"];
    assert_eq!(
        code(MailInbox::new(keyring(&v)).accept(
            &packet(&v, "original"),
            None,
            expired["now"].as_i64().unwrap()
        ))
        .as_deref(),
        Some("packet_expired")
    );
    let reply = &v["lifecycle"]["reply"];
    let message =
        |name: &str| validate_message(&v["messages"][reply[name].as_str().unwrap()]).unwrap();
    validate_reply(&message("valid"), &message("parent")).unwrap();
    assert!(validate_reply(&message("valid"), &message("wrong_parent")).is_err());
    let original = message("parent");
    let generated = create_mail_message(&original.sender,original.created_at,serde_json::json!({
        "to": original.to,"expires_at":original.expires_at,"thread_id":original.thread_id,"parts":original.parts
    })).unwrap();
    assert_eq!(generated.sender, original.sender);
    assert_ne!(generated.message_id, original.message_id);
}
#[test]
fn relay_shared_lifecycle_snapshots_and_owner_access() {
    let v = vectors();
    let now = v["now"].as_i64().unwrap();
    let jwt = token(&owner_signer(&v), ORIGIN, now);
    let mailbox = card(&v, "card").event.payload.mailbox_id;
    let mut relay = MailRelayStore::new(ORIGIN).unwrap();
    for step in v["lifecycle"]["relay"].as_array().unwrap() {
        let accepted_at = match step["op"].as_str().unwrap() {
            "publish" => relay
                .publish(
                    &v["envelopes"][step["card"].as_str().unwrap()],
                    step["now"].as_i64().unwrap(),
                )
                .map(|r| r.accepted_at),
            "deliver" => relay
                .deliver(
                    &mailbox,
                    &packet(&v, step["packet"].as_str().unwrap()),
                    step["now"].as_i64().unwrap(),
                )
                .map(|r| r.accepted_at),
            _ => relay
                .delete(
                    &mailbox,
                    &packet_id(&packet(&v, step["packet"].as_str().unwrap())).unwrap(),
                    &jwt,
                    now,
                )
                .map(|_| 0),
        };
        if let Some(expected) = step["accepted_at"].as_i64() {
            assert_eq!(accepted_at.unwrap(), expected, "{step}");
        } else if step["op"] != "delete" {
            assert_eq!(
                code(accepted_at).as_deref(),
                step["expected"].as_str(),
                "{step}"
            );
        }
        let page = relay.list(&mailbox, &jwt, now, 100, None).unwrap();
        if let Some(stored) = step["stored"].as_u64() {
            assert_eq!(page.result.len() as u64, stored, "{step}");
        }
        if let Some(seqs) = step["seqs"].as_array() {
            let actual: Vec<_> = page.result.iter().map(|r| r.seq).collect();
            let expected: Vec<_> = seqs.iter().map(|s| s.as_u64().unwrap()).collect();
            assert_eq!(actual, expected);
        }
        relay = MailRelayStore::from_snapshot(&relay.snapshot().unwrap(), 10_000, 1 << 26).unwrap();
    }
    for step in v["lifecycle"]["relay_registration"].as_array().unwrap() {
        let result = MailRelayStore::new(ORIGIN)
            .unwrap()
            .publish(&v["envelopes"][step["card"].as_str().unwrap()], now);
        assert_eq!(code(result).as_deref(), step["expected"].as_str());
    }
    for case in v["owner_jwt_cases"].as_array().unwrap() {
        let t = case["token"].as_str().unwrap();
        let expected = case["expected"].as_str().unwrap();
        assert_eq!(
            code(relay.list(&mailbox, t, now, 100, None)).as_deref(),
            (expected != "valid").then_some(expected)
        );
    }
    let unknown = random_id().unwrap();
    assert_eq!(
        code(relay.list(&unknown, &jwt, now, 100, None)).as_deref(),
        Some("mailbox_unavailable")
    );
    assert_eq!(
        code(relay.list(&mailbox, "bad", now, 100, None)).as_deref(),
        Some("invalid_token")
    );
}
#[test]
fn live_control_nonces_quota_pagination_and_failure_atomicity() {
    let v = vectors();
    let now = v["now"].as_i64().unwrap();
    let original = card(&v, "card");
    let mailbox = original.event.payload.mailbox_id.clone();
    let signer = owner_signer(&v);
    let jwt = token(&signer, ORIGIN, now);
    let mut relay = MailRelayStore::with_limits(ORIGIN, 2, 10000).unwrap();
    let mut shared = identity::MemoryNonceStore::new();
    relay
        .publish_with_nonce_store(&v["envelopes"]["card"], now, &mut shared)
        .unwrap();
    let mut p = original.event.payload.clone();
    p.mailbox_id = random_id().unwrap();
    let stale = sign_card(&signer, p.clone(), now - FUTURE_SKEW_MS - 1, 999).unwrap();
    assert_eq!(
        code(relay.publish_with_nonce_store(
            &serde_json::to_value(stale).unwrap(),
            now,
            &mut shared
        ))
        .as_deref(),
        Some("timestamp_out_of_window")
    );
    assert_eq!(shared.max_nonce(&signer.agent_id(), now), Some(100));
    shared
        .check_and_update(&signer.agent_id(), 200, now, 600000)
        .unwrap();
    assert_eq!(
        code(relay.publish_with_nonce_store(&v["envelopes"]["rotated_card"], now, &mut shared))
            .as_deref(),
        Some("nonce_not_greater")
    );
    assert_eq!(relay.card(&mailbox).unwrap().envelope, original);
    let a = relay
        .deliver(&mailbox, &packet(&v, "original"), now)
        .unwrap();
    relay
        .deliver(&mailbox, &packet(&v, "reencrypted"), now)
        .unwrap();
    let before = relay.snapshot().unwrap();
    assert_eq!(
        code(
            relay.deliver(
                &mailbox,
                &encrypt_message(
                    &validate_message(&v["messages"]["lower_nonce_message"]).unwrap(),
                    &card(&v, "card"),
                    &sender_signer(&v),
                    202,
                    now
                )
                .unwrap(),
                now
            )
        )
        .as_deref(),
        Some("rate_limited")
    );
    assert_eq!(relay.snapshot().unwrap(), before);
    let first = relay.list(&mailbox, &jwt, now, 1, None).unwrap();
    assert_eq!(first.result[0].seq, 1);
    assert_eq!(first.next_cursor.as_deref(), Some("1"));
    relay.delete(&mailbox, &a.packet_id, &jwt, now).unwrap();
    let second = relay
        .list(&mailbox, &jwt, now, 1, first.next_cursor.as_deref())
        .unwrap();
    assert_eq!(second.result[0].seq, 2);
    assert!(second.next_cursor.is_none());
    for cursor in ["", "-1", "01", "x"] {
        assert_eq!(
            code(relay.list(&mailbox, &jwt, now, 1, Some(cursor))).as_deref(),
            Some("invalid_request")
        );
    }
    let third = relay
        .deliver(
            &mailbox,
            &encrypt_message(
                &validate_message(&v["messages"]["lower_nonce_message"]).unwrap(),
                &card(&v, "card"),
                &sender_signer(&v),
                202,
                now,
            )
            .unwrap(),
            now,
        )
        .unwrap();
    assert_eq!(third.accepted_at, now);
    assert_eq!(
        relay
            .deliver(&mailbox, &packet(&v, "original"), now)
            .unwrap(),
        a
    );
    let seqs: Vec<_> = relay
        .list(&mailbox, &jwt, now, 100, None)
        .unwrap()
        .result
        .iter()
        .map(|r| r.seq)
        .collect();
    assert_eq!(seqs, [2, 3]);
    let expires = packet(&v, "original").event.payload.header.expires_at;
    relay.prune(expires).unwrap();
    assert_eq!(
        code(relay.deliver(&mailbox, &packet(&v, "original"), expires)).as_deref(),
        Some("stale_card")
    );
}
#[test]
fn mail_address_vectors_parse_and_format() {
    let v = vectors();
    for case in v["address_cases"].as_array().unwrap() {
        let value = case["value"].as_str().unwrap();
        if case["expected"] == "valid" {
            let parsed = parse_mail_address(value).unwrap();
            assert_eq!(serde_json::to_value(&parsed).unwrap(), case["parsed"]);
            assert_eq!(
                format_mail_address(&parsed.owner, &parsed.mailbox_id, &parsed.routes).unwrap(),
                value
            );
        } else {
            assert!(parse_mail_address(value).is_err(), "{}", case["name"]);
        }
    }
}
#[test]
fn relay_prune_forgets_mailbox_after_receive_until() {
    let v = vectors();
    let now = v["now"].as_i64().unwrap();
    let c = card(&v, "card");
    let mailbox = c.event.payload.mailbox_id.clone();
    let mut relay = MailRelayStore::new(ORIGIN).unwrap();
    relay.publish(&v["envelopes"]["card"], now).unwrap();
    relay
        .deliver(&mailbox, &packet(&v, "original"), now)
        .unwrap();
    relay.prune(c.event.payload.receive_until - 1).unwrap();
    assert_eq!(relay.card(&mailbox).unwrap().envelope, c);
    relay.prune(c.event.payload.receive_until).unwrap();
    assert_eq!(
        code(relay.card(&mailbox)).as_deref(),
        Some("mailbox_unavailable")
    );
    assert_eq!(
        code(relay.deliver(&mailbox, &packet(&v, "original"), now)).as_deref(),
        Some("mailbox_unavailable")
    );
    // A later card is a new registration and must list this relay again.
    assert_eq!(
        code(relay.publish(&v["envelopes"]["moved_card"], now)).as_deref(),
        Some("permission_denied")
    );
    assert_eq!(
        relay
            .publish(&v["envelopes"]["rotated_card"], now)
            .unwrap()
            .accepted_at,
        now
    );
}
#[test]
fn keyring_and_card_cache_seal_prune_and_bind_keys() {
    let v = vectors();
    let now = v["now"].as_i64().unwrap();
    let message = validate_message(&v["messages"]["message"]).unwrap();
    let mut cache = MailCardCache::new();
    let sealed = cache
        .seal(
            &message,
            &v["envelopes"]["card"],
            &sender_signer(&v),
            800,
            now,
        )
        .unwrap();
    assert_eq!(keyring(&v).open(&sealed, now).unwrap(), message);
    let owner = card(&v, "card").event.actor;
    cache
        .observe(&v["envelopes"]["rotated_card"], &owner, now)
        .unwrap();
    let rotated = card(&v, "rotated_card");
    assert_eq!(
        cache.get(&owner, &rotated.event.payload.mailbox_id),
        Some(&rotated)
    );
    assert_eq!(
        cache.get(
            &sender_signer(&v).agent_id(),
            &rotated.event.payload.mailbox_id
        ),
        None
    );
    assert_eq!(
        code(cache.seal(
            &message,
            &v["envelopes"]["card"],
            &sender_signer(&v),
            800,
            now
        ))
        .as_deref(),
        Some("stale_card")
    );
    assert_eq!(
        code(encrypt_message(
            &message,
            &card(&v, "closed_card"),
            &sender_signer(&v),
            800,
            now
        ))
        .as_deref(),
        Some("mailbox_unavailable")
    );
    let mut keys = keyring(&v);
    let c = card(&v, "card");
    assert_eq!(
        code(keys.add(&c, key(&v, &card(&v, "rotated_card")))).as_deref(),
        Some("invalid_private_key")
    );
    keys.prune(c.event.payload.receive_until).unwrap();
    assert_eq!(
        code(keys.open(&packet(&v, "original"), now)).as_deref(),
        Some("invalid_event")
    );
}

#[test]
fn sender_policy_is_checked_before_decryption_and_survives_restart() {
    let v = vectors();
    let now = v["now"].as_i64().unwrap();
    let c = card(&v, "card");
    let sender = sender_signer(&v).agent_id();
    let jwt = token(&owner_signer(&v), ORIGIN, now);
    let mut inbox = MailInbox::new(MailKeyring::new(c.event.actor.clone()));
    inbox.set_sender_blocked(sender.clone(), true);
    assert_eq!(
        code(inbox.accept(&packet(&v, "original"), None, now)).as_deref(),
        Some("permission_denied")
    );
    let mut broken = packet(&v, "original");
    broken.signature = encode_bytes(&[0; 64]);
    assert_eq!(
        code(inbox.accept(&broken, None, now)).as_deref(),
        Some("invalid_signature")
    );
    let mut restored = MailInbox::from_snapshot(
        MailKeyring::new(c.event.actor.clone()),
        &inbox.snapshot().unwrap(),
    )
    .unwrap();
    assert_eq!(
        code(restored.accept(&packet(&v, "original"), None, now)).as_deref(),
        Some("permission_denied")
    );
    let mut relay = MailRelayStore::new(ORIGIN).unwrap();
    let mailbox = &c.event.payload.mailbox_id;
    assert_eq!(
        code(relay.set_sender_blocked(mailbox, sender.clone(), true)).as_deref(),
        Some("mailbox_unavailable")
    );
    relay.publish(&v["envelopes"]["card"], now).unwrap();
    relay
        .set_sender_blocked(mailbox, sender.clone(), true)
        .unwrap();
    assert_eq!(
        code(relay.deliver(mailbox, &packet(&v, "original"), now)).as_deref(),
        Some("permission_denied")
    );
    relay
        .set_sender_blocked(mailbox, sender.clone(), false)
        .unwrap();
    let accepted = relay
        .deliver(mailbox, &packet(&v, "original"), now)
        .unwrap();
    relay.set_sender_blocked(mailbox, sender, true).unwrap();
    relay
        .delete(mailbox, &accepted.packet_id, &jwt, now)
        .unwrap();
    let mut relay =
        MailRelayStore::from_snapshot(&relay.snapshot().unwrap(), 100, MAX_PACKET_BYTES * 10)
            .unwrap();
    assert_eq!(
        relay
            .deliver(mailbox, &packet(&v, "original"), now)
            .unwrap(),
        accepted
    );
    assert_eq!(
        code(relay.deliver(mailbox, &packet(&v, "reencrypted"), now)).as_deref(),
        Some("permission_denied")
    );
    assert!(
        relay
            .list(mailbox, &jwt, now, 100, None)
            .unwrap()
            .result
            .is_empty()
    );
}
#[test]
fn packets_are_not_live_writes_and_logical_message_conflicts() {
    let v = vectors();
    let now = v["now"].as_i64().unwrap();
    let c = card(&v, "card");
    let mailbox = &c.event.payload.mailbox_id;
    let jwt = token(&owner_signer(&v), ORIGIN, now);
    let mut relay = MailRelayStore::new(ORIGIN).unwrap();
    relay.publish(&v["envelopes"]["card"], now).unwrap();
    let first = relay
        .deliver(mailbox, &packet(&v, "original"), now)
        .unwrap();
    let mut relay =
        MailRelayStore::from_snapshot(&relay.snapshot().unwrap(), 100, MAX_PACKET_BYTES * 10)
            .unwrap();
    // A lower nonce signed well outside the live-write window is still new mail.
    let late = now + 2 * FUTURE_SKEW_MS;
    relay
        .deliver(mailbox, &packet(&v, "lower_nonce"), late)
        .unwrap();
    relay
        .deliver(mailbox, &packet(&v, "reencrypted"), late)
        .unwrap();
    assert_eq!(
        relay
            .deliver(mailbox, &packet(&v, "original"), late)
            .unwrap(),
        first
    );
    assert_eq!(
        relay
            .list(mailbox, &jwt, now, 100, None)
            .unwrap()
            .result
            .len(),
        3
    );
    let message = validate_message(&v["messages"]["message"]).unwrap();
    let mut inbox = MailInbox::new(keyring(&v));
    inbox
        .accept(&packet(&v, "reencrypted"), None, now + 86_400_000)
        .unwrap();
    assert!(matches!(
        inbox
            .accept(&packet(&v, "original"), None, now + 86_400_000)
            .unwrap(),
        InboxAcceptance::Duplicate(_)
    ));
    assert!(matches!(
        inbox
            .accept(&packet(&v, "lower_nonce"), None, now + 86_400_000)
            .unwrap(),
        InboxAcceptance::Accepted(_)
    ));
    let before = inbox.snapshot().unwrap();
    assert_eq!(
        code(inbox.accept(&packet(&v, "conflicting"), None, now)).as_deref(),
        Some("invalid_event")
    );
    assert_eq!(inbox.snapshot().unwrap(), before);
    let mut other = message;
    other.sender = c.event.actor.clone();
    let other = encrypt_message(&other, &c, &owner_signer(&v), 1000, now).unwrap();
    assert!(matches!(
        inbox.accept(&other, None, now).unwrap(),
        InboxAcceptance::Accepted(_)
    ));
}
