use crate::agent::{agent_id, controller_item, sign_delegation, DelegationKind, MAX_GRANT_DAYS};
use crate::agents::{agent_item, covers, validate_capability};
use crate::crypto::signing_public;
use crate::secure::SecureStorage;
use crate::testing::{account, unlocked, MemoryStorage, TempDir};
use agent_protocols::delegation::{
    materialize_delegation_credential, validate_delegation_acceptance, DelegationStatus,
    PrincipalDocument,
};
use agent_protocols::identity::{verify_envelope, AgentSigner};
use candid::Principal;
use dmsg_protocol::agent::render_principal_document;
use dmsg_types::{
    agent::{DelegationAuthority, DirectoryInit, HostedController, PrincipalState, PrincipalType},
    Environment, Error, Hash, DAY,
};
use serde_json::json;
use std::sync::Arc;

const NOW: u64 = 1_790_000_000_000;
const SEED: [u8; 32] = [3; 32];

/// The principal document the directory would publish for the test account
/// with one restricted controller, and its URL.
fn document() -> (PrincipalDocument, String) {
    let config = DirectoryInit {
        environment: Environment::Local,
        issuer_namespace: "https://dmsg.test/u/".into(),
        user_homes: vec![Principal::from_slice(&[1])],
        principal_origin: "https://id.dmsg.test".into(),
        controller_source: "https://dmsg.test".into(),
        delegation_service: "https://api.dmsg.test".into(),
        profile_url_prefix: "https://dmsg.test/".into(),
        custom_domains: vec!["id.dmsg.test".into()],
        governance: Principal::from_slice(&[9]),
    };
    let state = PrincipalState {
        principal_type: PrincipalType::Person,
        controllers: vec![HostedController {
            generation: 1,
            public_key: Hash::new(signing_public(&SEED)),
            name: None,
            valid_from: NOW - DAY,
            delegation: DelegationAuthority::Restricted {
                scopes: vec!["message.draft".into()],
                audiences: vec!["https://dmsg.net".into()],
            },
            retired_at: None,
            invalid_from: None,
        }],
        version: 2,
        updated_at: NOW - DAY,
    };
    let bytes = render_principal_document(&config, &account().id, &state).unwrap();
    let document: PrincipalDocument = serde_json::from_slice(&bytes).unwrap();
    let url = document.id.clone();
    (document, url)
}

fn grant(id: &str, scope: &str) -> DelegationKind {
    DelegationKind::Grant {
        id: id.into(),
        subject: AgentSigner::from_seed([4; 32]).agent_id().to_string(),
        scopes: vec![scope.into()],
        audiences: vec!["https://dmsg.net".into()],
        expires_at: NOW + 30 * DAY,
        relationship: Some("assistant".into()),
    }
}

#[test]
fn locally_signed_events_pass_the_delegation_rules() {
    let (document, url) = document();
    let id = format!("{}.abc", account().id);
    let (envelope, next) =
        sign_delegation(&SEED, &url, &grant(&id, "message.draft"), NOW - DAY, 1, NOW).unwrap();
    verify_envelope(&envelope).unwrap();
    assert_eq!(
        envelope.event.actor.to_string(),
        agent_id(&signing_public(&SEED))
    );
    // The clock-derived nonce: max(last + 1, created_at).
    assert_eq!(envelope.event.nonce, NOW);
    assert_eq!(next, NOW + 1);
    validate_delegation_acceptance(&envelope, &document, &url, NOW as i64, None).unwrap();

    // The service keeps the ceiling: a scope outside it is refused.
    let (wide, _) = sign_delegation(
        &SEED,
        &url,
        &grant(&id, "inbox.screen"),
        NOW - DAY,
        next,
        NOW,
    )
    .unwrap();
    assert!(validate_delegation_acceptance(&wide, &document, &url, NOW as i64, None).is_err());

    // A revocation of that credential follows with a larger nonce.
    let credential =
        materialize_delegation_credential(&envelope, DelegationStatus::Active, NOW as i64, None)
            .unwrap();
    let revoke = DelegationKind::Revoke {
        id: id.clone(),
        reason: None,
    };
    let (revocation, after) = sign_delegation(&SEED, &url, &revoke, NOW - DAY, next, NOW).unwrap();
    assert_eq!(revocation.event.nonce, NOW + 1);
    assert_eq!(after, NOW + 2);
    validate_delegation_acceptance(&revocation, &document, &url, NOW as i64, Some(&credential))
        .unwrap();

    // Nothing is backdated before a binding starts, and a grant must end
    // within the policy's lifetime of its signing.
    let (late, _) = sign_delegation(&SEED, &url, &revoke, NOW + 10, after, NOW).unwrap();
    assert_eq!(late.event.created_at, (NOW + 10) as i64);
    let mut stale = grant(&id, "message.draft");
    if let DelegationKind::Grant { expires_at, .. } = &mut stale {
        *expires_at = NOW + (MAX_GRANT_DAYS + 1) * DAY;
    }
    assert_eq!(
        sign_delegation(&SEED, &url, &stale, NOW - DAY, after, NOW).unwrap_err(),
        Error::Expired
    );
}

/// Unlock a locked test client again.
fn reopen(client: &mut crate::Client) {
    client.keys = Some(client.workspace.unlock(&crate::testing::SECRET).unwrap());
    client.root = Some(crate::root::RootKeys::single(1, [5; 32]));
}

#[test]
fn controller_and_agent_keys_live_in_the_vault() {
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    let account = account().id.to_string();

    let seed = client.controller_seed(1, true).unwrap();
    assert_eq!(*client.controller_seed(1, false).unwrap(), *seed);
    let entry = client.vault_show(&controller_item(&account, 1)).unwrap();
    assert_eq!(entry.item.tags, ["dmsg-controller"]);
    assert_eq!(
        client.controller_seed(2, false).unwrap_err(),
        Error::NotFound
    );

    let agent = client.agent_create("claude").unwrap();
    assert_eq!(client.agent_create("claude").unwrap(), agent, "idempotent");
    assert!(client.vault_show(&agent_item(&account, "claude")).is_ok());
    assert!(client.agent_create("Not A Label").is_err());
    let signer = client.agent_signer("claude").unwrap();
    assert_eq!(signer.agent_id().to_string(), agent.agent_id);

    // Its MCP credential: only the digest is kept, and a new one replaces it.
    let credential = client.agent_credential("claude").unwrap();
    assert_eq!(
        client
            .agent_by_credential(&credential)
            .unwrap()
            .unwrap()
            .agent_id,
        agent.agent_id
    );
    let rotated = client.agent_credential("claude").unwrap();
    assert!(client.agent_by_credential(&credential).unwrap().is_none());
    assert!(client.agent_by_credential(&rotated).unwrap().is_some());

    // Locked, only a resident Agent ID signs.
    client.lock();
    assert_eq!(client.agent_signer("claude").err(), Some(Error::Locked));
    assert_eq!(
        client.agent_resident("claude", true).unwrap_err(),
        Error::Locked
    );
    reopen(&mut client);
    client.agent_resident("claude", true).unwrap();
    client.lock();
    assert_eq!(
        client
            .agent_signer(&agent.agent_id)
            .unwrap()
            .agent_id()
            .to_string(),
        agent.agent_id
    );
    // Turning residency off deletes the copy.
    client.agent_resident("claude", false).unwrap();
    assert_eq!(client.agent_signer("claude").err(), Some(Error::Locked));
    // A credential still names the Agent ID while locked.
    assert!(client.agent_by_credential(&rotated).unwrap().is_some());
}

#[test]
fn mail_keys_are_sealed_and_a_resident_agent_keeps_a_copy() {
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    let agent = client.agent_create("mailer").unwrap().agent_id;
    let keys = |pairs: &[(&str, &str)]| json!({"version": 1, "agent_id": agent, "keys": pairs.iter().copied().collect::<std::collections::BTreeMap<_, _>>()});
    assert_eq!(client.agent_mail_keys("mailer").unwrap(), keys(&[]));
    client
        .agent_mail_keys_save("mailer", &keys(&[("pk1", "s1")]))
        .unwrap();
    assert_eq!(
        client.agent_mail_keys(&agent).unwrap(),
        keys(&[("pk1", "s1")])
    );
    // Another Agent ID's keys are refused.
    let other = json!({"version": 1, "agent_id": "did:agent:other", "keys": {}});
    assert_eq!(
        client.agent_mail_keys_save("mailer", &other).unwrap_err(),
        Error::IntegrityFailed
    );

    // Locked, only a resident Agent ID reads and writes its keys.
    client.lock();
    assert_eq!(client.agent_mail_keys("mailer").unwrap_err(), Error::Locked);
    reopen(&mut client);
    client.agent_resident("mailer", true).unwrap();
    client.lock();
    assert_eq!(
        client.agent_mail_keys("mailer").unwrap(),
        keys(&[("pk1", "s1")])
    );
    client
        .agent_mail_keys_save("mailer", &keys(&[("pk1", "s1"), ("pk2", "s2")]))
        .unwrap();
    // Unlocked again, the keys made while locked join the sealed ones.
    reopen(&mut client);
    assert_eq!(
        client.agent_mail_keys("mailer").unwrap(),
        keys(&[("pk1", "s1"), ("pk2", "s2")])
    );
    client
        .agent_mail_keys_save("mailer", &keys(&[("pk2", "s2")]))
        .unwrap();
    assert_eq!(
        client.agent_mail_keys("mailer").unwrap(),
        keys(&[("pk2", "s2")])
    );

    // Turning residency off folds a locked-time key into the sealed ones,
    // which needs the device unlocked, then deletes the copy.
    client.lock();
    client
        .agent_mail_keys_save("mailer", &keys(&[("pk2", "s2"), ("pk3", "s3")]))
        .unwrap();
    assert_eq!(
        client.agent_resident("mailer", false).unwrap_err(),
        Error::Locked
    );
    reopen(&mut client);
    client.agent_resident("mailer", false).unwrap();
    assert_eq!(
        client.agent_mail_keys("mailer").unwrap(),
        keys(&[("pk2", "s2"), ("pk3", "s3")])
    );
    let workspace = client.workspace.name().unwrap();
    assert!(storage
        .get(&format!("{workspace}:agent-mail:{agent}"))
        .unwrap()
        .is_none());
}

#[test]
fn a_resident_agent_is_checked_against_a_copy_of_its_grants() {
    let dir = TempDir::new();
    let storage = Arc::new(MemoryStorage::default());
    let mut client = unlocked(&dir.0, false, Some(storage.clone()));
    let agent = client.agent_create("reader").unwrap().agent_id;
    let other = client.agent_create("other").unwrap().agent_id;
    client
        .capability_grant("cli", "reader", vec!["discourse.read:*".into()], None)
        .unwrap();
    client
        .capability_grant("cli", "other", vec!["knowledge.read:*".into()], None)
        .unwrap();
    client.agent_resident("reader", true).unwrap();
    let write = client
        .capability_grant("cli", "reader", vec!["discourse.write:*".into()], None)
        .unwrap();
    client.capability_revoke("cli", &write.id).unwrap();
    let (a, b) = ("https://a.example", "https://b.example");
    client
        .capability_grant("cli", "reader", vec![format!("discourse.write:{a}")], None)
        .unwrap();

    // Locked, the copy holds its live grants and nobody else's.
    client.lock();
    client.capability_check(&agent, "discourse.read:x").unwrap();
    let targets = |name: &str| {
        client
            .capability_targets(&agent, name, vec![a.into(), b.into()])
            .unwrap()
    };
    assert_eq!(targets("discourse.read"), [a, b]);
    assert_eq!(targets("discourse.write"), [a]);
    assert!(targets("mail.read").is_empty());
    for needed in ["discourse.write:x", "knowledge.read:x"] {
        assert_eq!(
            client.capability_check(&agent, needed).unwrap_err(),
            Error::Forbidden,
            "{needed}"
        );
    }
    assert_eq!(
        client
            .capability_check(&other, "knowledge.read:x")
            .unwrap_err(),
        Error::Locked
    );

    // Turning residency off deletes the copy.
    client.agent_resident("reader", false).unwrap();
    assert_eq!(
        client
            .capability_check(&agent, "discourse.read:x")
            .unwrap_err(),
        Error::Locked
    );
    let workspace = client.workspace.name().unwrap();
    assert!(storage
        .get(&format!("{workspace}:agent-grants:{agent}"))
        .unwrap()
        .is_none());
}

#[test]
fn capabilities_cover_only_what_was_granted() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let agent = client.agent_create("helper").unwrap().agent_id;
    for capability in [
        "vault.read:work",
        "channel.read:*",
        "attest.request",
        "vault.list",
        "discourse.read:*",
        "mail.write:https://mail.dmsg.net",
        "agent.profile.write:http://127.0.0.1:8787",
    ] {
        validate_capability(capability).unwrap();
    }
    for capability in [
        "vault.read:",
        "payment",
        "account.read:x",
        "vault.read:a b",
        // dMsg's own keys are out of every agent's reach.
        "vault.write:dmsg-agent",
        "vault.secret:dmsg-controller",
        "file.read:dmsg-x",
        // A service capability names an origin, or every one.
        "discourse.read",
        "discourse.read:discourse.dmsg.net",
        "knowledge.read:https://knowledge.dmsg.net/",
        "mail.read:https://mail.dmsg.net/v1",
        "mail.read:ftp://mail.dmsg.net",
    ] {
        assert!(validate_capability(capability).is_err(), "{capability}");
    }
    assert!(covers("channel.read:*", "channel.read:abc"));
    assert!(!covers("channel.read:*", "channel.publish:abc"));
    assert!(!covers("vault.read:work", "vault.read:workshop"));

    // Nothing but the public profile without a grant.
    client.capability_check(&agent, "profile.read").unwrap();
    assert_eq!(
        client.capability_check(&agent, "vault.list").unwrap_err(),
        Error::Forbidden
    );
    let grant = client
        .capability_grant(
            "gui",
            "helper",
            vec!["vault.read:work".into(), "channel.read:*".into()],
            Some(7),
        )
        .unwrap();
    assert_eq!(grant.grantee, agent);
    client.capability_check(&agent, "vault.read:work").unwrap();
    client.capability_check(&agent, "channel.read:c1").unwrap();
    assert_eq!(
        client
            .capability_check(&agent, "vault.read:home")
            .unwrap_err(),
        Error::Forbidden
    );
    assert_eq!(
        client
            .capability_check(&agent, "channel.publish:c1")
            .unwrap_err(),
        Error::Forbidden
    );
    // Only a hosted Agent ID is granted anything.
    assert_eq!(
        client
            .capability_grant("cli", "did:agent:unknown", vec!["vault.list".into()], None)
            .unwrap_err(),
        Error::NotFound
    );
    client.capability_revoke("cli", &grant.id).unwrap();
    assert_eq!(
        client
            .capability_check(&agent, "vault.read:work")
            .unwrap_err(),
        Error::Forbidden
    );
    assert_eq!(client.capability_grants().unwrap().len(), 1);
    client.lock();
    // The grant and its revocation are in the audit record, which a
    // locked device still reads.
    let audit = client.audit_list(None, Some("owner"), 10).unwrap();
    let actions: Vec<(&str, &str)> = audit
        .iter()
        .map(|e| (e.action.as_str(), e.via.as_str()))
        .collect();
    assert_eq!(actions, [("grants/revoke", "cli"), ("grants/add", "gui")]);
    assert_eq!(audit[1].target.as_deref(), Some(grant.id.as_str()));
    assert_eq!(
        audit[1].detail.as_deref(),
        Some(format!("{agent} vault.read:work,channel.read:*").as_str())
    );
    assert_eq!(
        client.audit_list(Some(audit[0].seq), None, 10).unwrap(),
        audit[1..]
    );
    assert_eq!(
        client.capability_check(&agent, "vault.list").unwrap_err(),
        Error::Locked
    );
}
