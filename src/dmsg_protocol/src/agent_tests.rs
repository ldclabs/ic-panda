use super::*;
use agent_protocols::{delegation as sdk, identity as sdk_id};
use candid::Principal;

const ORIGIN: &str = "https://id.dmsg.test";
const NOW: u64 = 1_790_000_000_000;

fn account() -> AccountId {
    AccountId([7; 12])
}

fn signer(n: u8) -> sdk_id::AgentSigner {
    sdk_id::AgentSigner::from_seed([n; 32])
}

fn public(n: u8) -> Hash {
    Hash::new(signer(n).verifying_key().to_bytes())
}

fn grant(created_at: u64, expires_at: Option<u64>) -> sdk::DelegationGrantPayload {
    let mut payload = sdk::DelegationGrantPayload::new(
        format!("{}.abc", account()),
        principal_id(ORIGIN, &account()),
        signer(9).agent_id(),
        vec!["message.draft".into()],
        vec!["https://dmsg.net".into()],
    );
    payload.expires_at = expires_at.map(|t| t as i64);
    payload.not_before = Some(created_at as i64);
    payload
}

fn grant_event(created_at: u64, payload: sdk::DelegationGrantPayload) -> Vec<u8> {
    let event = sdk_id::Event::new(
        sdk::PROTOCOL,
        sdk::DELEGATION_GRANT,
        signer(1).agent_id(),
        created_at as i64,
        created_at,
        sdk::DelegationPayload::Grant(payload),
    );
    sdk_id::canonical_event_bytes(&event).unwrap()
}

fn controller(delegation: DelegationAuthority) -> HostedController {
    HostedController {
        generation: 1,
        public_key: public(1),
        name: None,
        valid_from: NOW - DAY,
        delegation,
        supersedes: vec![],
        retired_at: None,
        invalid_from: None,
    }
}

fn restricted() -> DelegationAuthority {
    DelegationAuthority::Restricted {
        scopes: vec!["message.draft".into(), "inbox.screen".into()],
        audiences: vec!["https://dmsg.net".into()],
    }
}

fn config() -> DirectoryInit {
    DirectoryInit {
        environment: Environment::Local,
        issuer_namespace: "https://dmsg.test/u/".into(),
        user_homes: vec![Principal::from_slice(&[1])],
        principal_origin: ORIGIN.into(),
        controller_source: "https://dmsg.net".into(),
        delegation_query_url: "https://agents.dmsg.test/v1/delegations/query".into(),
        profile_url_prefix: "https://dmsg.test/u/".into(),
        custom_domains: vec!["id.dmsg.test".into()],
    }
}

#[test]
fn canonical_grant_parses_with_the_sdk_event_hash() {
    let bytes = grant_event(NOW, grant(NOW, Some(NOW + DAY)));
    let event = parse_delegation_event(&bytes).unwrap();
    let text: sdk_id::Event<sdk::DelegationPayload> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        event.hash.into_array(),
        sdk_id::event_hash_bytes(&text).unwrap()
    );
    assert_eq!(event_hash(&bytes), event.hash);
    assert_eq!(event.actor, public(1));
    assert_eq!(event.principal_id, principal_id(ORIGIN, &account()));
    assert_eq!(event.nonce, NOW);
    let grant = event.grant.as_ref().unwrap();
    assert_eq!(grant.expires_at, Some(NOW + DAY));
    assert_eq!(grant.constraints_bytes, 0);
    check_hosted_event(
        &event,
        &account(),
        &principal_id(ORIGIN, &account()),
        &controller(restricted()),
        NOW,
    )
    .unwrap();

    let revoke = sdk_id::Event::new(
        sdk::PROTOCOL,
        sdk::DELEGATION_REVOKE,
        signer(1).agent_id(),
        NOW as i64,
        NOW + 1,
        sdk::DelegationPayload::Revoke(sdk::DelegationRevokePayload {
            id: format!("{}.abc", account()),
            principal_id: principal_id(ORIGIN, &account()),
            reason: Some("done".into()),
        }),
    );
    // A room_id is not a delegation event field.
    let mut open = revoke.with_room_id("x");
    assert!(parse_delegation_event(&sdk_id::canonical_event_bytes(&open).unwrap()).is_err());
    open.room_id = None;
    let event = parse_delegation_event(&sdk_id::canonical_event_bytes(&open).unwrap()).unwrap();
    assert!(event.grant.is_none());
}

#[test]
fn noncanonical_or_open_events_are_rejected() {
    let bytes = grant_event(NOW, grant(NOW, Some(NOW + DAY)));
    let text = String::from_utf8(bytes.clone()).unwrap();
    let mutations = [
        // Whitespace is not JCS.
        text.replacen("{", "{ ", 1),
        // Duplicate member names are rejected before typed decoding.
        text.replacen("{", "{\"nonce\":1,", 1),
        // Unknown payload members and explicit nulls are dropped by typed re-encoding.
        text.replacen("\"audiences\"", "\"audience\":1,\"audiences\"", 1),
        text.replacen("\"audiences\"", "\"constraints\":null,\"audiences\"", 1),
        // Foreign or unknown event types and protocols.
        text.replace("delegation.grant", "delegation.revoke"),
        text.replace("delegation.grant", "delegation.other"),
        text.replace("agent-delegation/1.0", "agent-delegation/2.0"),
        // Unsafe integers.
        text.replacen(&format!("\"nonce\":{NOW}"), "\"nonce\":9007199254740992", 1),
    ];
    for mutated in mutations {
        assert!(
            parse_delegation_event(mutated.as_bytes()).is_err(),
            "{mutated}"
        );
    }
    let mut large = grant(NOW, Some(NOW + DAY));
    large.relationship = Some("x".repeat(MAX_AGENT_EVENT_BYTES));
    assert_eq!(
        parse_delegation_event(&grant_event(NOW, large)),
        Err(Error::QuotaExceeded)
    );
    let mut wildcard = grant(NOW, Some(NOW + DAY));
    wildcard.scopes = vec!["*".into()];
    assert!(parse_delegation_event(&grant_event(NOW, wildcard)).is_err());
}

#[test]
fn hosted_policy_binds_key_principal_prefix_time_and_ceiling() {
    let principal = principal_id(ORIGIN, &account());
    let check = |payload: sdk::DelegationGrantPayload, c: &HostedController, now: u64| {
        let event = parse_delegation_event(&grant_event(NOW, payload)).unwrap();
        check_hosted_event(&event, &account(), &principal, c, now)
    };
    let ok = grant(NOW, Some(NOW + DAY));
    let c = controller(restricted());
    check(ok.clone(), &c, NOW).unwrap();
    check(ok.clone(), &c, NOW + EVENT_PAST_SKEW).unwrap();
    assert_eq!(
        check(ok.clone(), &c, NOW + EVENT_PAST_SKEW + 1),
        Err(Error::Expired)
    );
    assert_eq!(
        check(ok.clone(), &c, NOW - EVENT_FUTURE_SKEW - 1),
        Err(Error::Expired)
    );
    let mut early = c.clone();
    early.valid_from = NOW + 1;
    assert_eq!(check(ok.clone(), &early, NOW), Err(Error::Expired));
    let mut retired = c.clone();
    retired.retired_at = Some(NOW);
    assert_eq!(check(ok.clone(), &retired, NOW), Err(Error::Forbidden));
    let mut other = c.clone();
    other.public_key = public(2);
    assert_eq!(check(ok.clone(), &other, NOW), Err(Error::IntegrityFailed));

    let mut foreign = ok.clone();
    foreign.principal_id = principal_id(ORIGIN, &AccountId([8; 12]));
    assert_eq!(check(foreign, &c, NOW), Err(Error::IntegrityFailed));
    for id in ["abc".to_string(), format!("{}.", account())] {
        let mut unprefixed = ok.clone();
        unprefixed.id = id;
        assert!(matches!(
            check(unprefixed, &c, NOW),
            Err(Error::InvalidInput(_))
        ));
    }
    let mut outside = ok.clone();
    outside.audiences = vec!["https://tokenlist.ing".into()];
    assert_eq!(check(outside.clone(), &c, NOW), Err(Error::Forbidden));
    check(outside, &controller(DelegationAuthority::Unrestricted), NOW).unwrap();
    for expires_at in [None, Some(NOW + MAX_GRANT_LIFETIME + 1)] {
        assert!(matches!(
            check(grant(NOW, expires_at), &c, NOW),
            Err(Error::InvalidInput(_))
        ));
    }
    let mut constrained = ok;
    constrained.constraints = Some(
        [(
            "note".to_string(),
            serde_json::Value::String("x".repeat(MAX_GRANT_CONSTRAINTS_BYTES)),
        )]
        .into(),
    );
    assert!(matches!(
        check(constrained, &c, NOW),
        Err(Error::InvalidInput(_))
    ));
}

fn state() -> PrincipalState {
    let mut first = controller(restricted());
    first.retired_at = Some(NOW);
    first.name = Some("dMsg hosted signer #1".into());
    let mut second = controller(DelegationAuthority::Unrestricted);
    second.generation = 2;
    second.public_key = public(2);
    second.valid_from = NOW;
    second.supersedes = vec![1];
    PrincipalState {
        principal_type: PrincipalType::Person,
        controllers: vec![first, second],
        version: 3,
        updated_at: NOW + 1,
    }
}

#[test]
fn rendered_document_passes_sdk_validation_and_is_jcs() {
    let bytes = render_principal_document(&config(), &account(), &state()).unwrap();
    let document: sdk::PrincipalDocument = serde_json::from_slice(&bytes).unwrap();
    sdk::validate_principal_document(&document).unwrap();
    assert_eq!(serde_jcs::to_vec(&document).unwrap(), bytes);
    assert_eq!(document.id, principal_id(ORIGIN, &account()));
    assert_eq!(document.kind.as_deref(), Some("person"));
    assert_eq!(document.controllers.len(), 1);
    assert_eq!(document.retired_controllers.len(), 1);
    assert_eq!(
        document.controllers[0].id.to_string(),
        sdk_id::AgentId::from_public_key(&public(2)).to_string()
    );
    assert_eq!(
        document.controllers[0].supersedes.as_deref(),
        Some(&[document.retired_controllers[0].id.clone()][..])
    );
    assert_eq!(document.retired_controllers[0].retired_at, Some(NOW as i64));
    assert_eq!(
        document.delegation_query_url.as_deref(),
        Some("https://agents.dmsg.test/v1/delegations/query")
    );
    assert_eq!(
        document.links[0].url,
        format!("https://dmsg.test/u/{}", account())
    );
    assert_eq!(
        sdk::controller_lineage(&document, &document.controllers[0].id).len(),
        2
    );

    let empty = PrincipalState {
        principal_type: PrincipalType::Project,
        controllers: vec![],
        version: 1,
        updated_at: NOW,
    };
    let bytes = render_principal_document(&config(), &account(), &empty).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("\"controllers\":[]") && !text.contains("retired_controllers"));
}

#[test]
fn invalid_states_are_rejected() {
    let mut cases = Vec::new();
    let mut s = state();
    s.controllers[1].generation = 1;
    cases.push(s);
    let mut s = state();
    s.controllers[1].public_key = s.controllers[0].public_key;
    cases.push(s);
    let mut s = state();
    s.controllers[1].supersedes = vec![2];
    cases.push(s);
    let mut s = state();
    s.controllers[1].valid_from = s.controllers[0].valid_from;
    cases.push(s);
    let mut s = state();
    s.controllers[1].invalid_from = Some(NOW);
    cases.push(s);
    let mut s = state();
    s.controllers[0].invalid_from = Some(NOW + 1);
    cases.push(s);
    let mut s = state();
    s.updated_at = NOW - 1;
    cases.push(s);
    let mut s = state();
    s.controllers[0].name = Some(" ".into());
    cases.push(s);
    let mut s = state();
    s.controllers[0].delegation = DelegationAuthority::Restricted {
        scopes: vec![],
        audiences: vec!["https://dmsg.net".into()],
    };
    cases.push(s);
    let mut s = state();
    s.controllers[0].delegation = DelegationAuthority::Restricted {
        scopes: vec!["a".into()],
        audiences: vec!["http://dmsg.net".into()],
    };
    cases.push(s);
    for s in cases {
        assert!(validate_principal_state(&s).is_err(), "{s:?}");
        assert!(render_principal_document(&config(), &account(), &s).is_err());
    }
    let mut s = state();
    s.controllers = (1..=MAX_CURRENT_CONTROLLERS as u32 + 1)
        .map(|g| {
            let mut c = controller(DelegationAuthority::Unrestricted);
            c.generation = g;
            c.public_key = public(g as u8 + 10);
            c
        })
        .collect();
    assert_eq!(validate_principal_state(&s), Err(Error::QuotaExceeded));
}

#[test]
fn directory_config_and_allocator_routing() {
    validate_directory_init(&config()).unwrap();
    for edit in [
        |c: &mut DirectoryInit| c.principal_origin = "https://id.dmsg.test/".into(),
        |c: &mut DirectoryInit| c.controller_source = "http://dmsg.net".into(),
        |c: &mut DirectoryInit| c.profile_url_prefix = "https://dmsg.test/u".into(),
        |c: &mut DirectoryInit| c.user_homes.push(c.user_homes[0]),
        |c: &mut DirectoryInit| c.custom_domains = vec!["ID.dmsg.test".into()],
    ] {
        let mut c = config();
        edit(&mut c);
        assert!(validate_directory_init(&c).is_err(), "{c:?}");
    }
    let home = Principal::from_slice(&[1]);
    let digest = account_allocator_digest(&Environment::Local, "https://dmsg.test/u/", home);
    let mut id = [0; 12];
    id[4..9].copy_from_slice(&digest[..5]);
    assert!(allocated_by(&AccountId(id), &digest));
    id[8] ^= 1;
    assert!(!allocated_by(&AccountId(id), &digest));
}

#[test]
fn directory_document_urls_are_bounded_and_need_no_json_escaping() {
    for edit in [
        |c: &mut DirectoryInit| {
            c.principal_origin = format!("https://{}", "a".repeat(MAX_PRINCIPAL_ORIGIN_BYTES))
        },
        |c: &mut DirectoryInit| {
            c.controller_source = format!("https://{}", "a".repeat(MAX_PRINCIPAL_ORIGIN_BYTES))
        },
        |c: &mut DirectoryInit| {
            c.delegation_query_url =
                format!("https://dmsg.test/{}", "a".repeat(MAX_DIRECTORY_URL_BYTES))
        },
        |c: &mut DirectoryInit| {
            c.profile_url_prefix =
                format!("https://dmsg.test/{}/", "a".repeat(MAX_DIRECTORY_URL_BYTES))
        },
        |c: &mut DirectoryInit| c.profile_url_prefix = "https://dmsg.test/\"/".into(),
    ] {
        let mut c = config();
        edit(&mut c);
        assert!(validate_directory_init(&c).is_err(), "{c:?}");
    }
    let mut c = config();
    c.profile_url_prefix = "https://dmsg.test/%22/".into();
    validate_directory_init(&c).unwrap();
}

#[test]
fn document_budget_covers_maximum_urls_and_later_safety_changes() {
    let mut config = config();
    config.principal_origin = format!("https://{}", "a".repeat(MAX_PRINCIPAL_ORIGIN_BYTES - 8));
    config.controller_source = config.principal_origin.clone();
    config.profile_url_prefix = format!(
        "https://dmsg.test/{}/",
        "p".repeat(MAX_DIRECTORY_URL_BYTES - "https://dmsg.test//".len())
    );
    config.delegation_query_url = format!(
        "https://dmsg.test/{}",
        "q".repeat(MAX_DIRECTORY_URL_BYTES - "https://dmsg.test/".len())
    );
    validate_directory_init(&config).unwrap();
    let mut state = PrincipalState {
        principal_type: PrincipalType::Organization,
        controllers: vec![],
        version: 1,
        updated_at: sdk_id::MAX_SAFE_NONCE,
    };
    // Exercise six-byte control escapes, two-byte escapes and multi-byte UTF-8.
    for escaped in ["\u{1}", "\"", "\\", "\n", "界"] {
        state.controllers.clear();
        let mut accepted = 0;
        for g in 1..=MAX_CONTROLLER_RECORDS as u32 {
            let fill = escaped.repeat((MAX_SCOPE_BYTES - 2) / escaped.len());
            state.controllers.push(HostedController {
                generation: g,
                public_key: public(g as u8),
                name: None,
                valid_from: g as u64,
                delegation: DelegationAuthority::Restricted {
                    scopes: (0..MAX_CEILING_SCOPES)
                        .map(|i| format!("{i}:{fill}"))
                        .collect(),
                    audiences: (0..MAX_CEILING_AUDIENCES)
                        .map(|i| format!("https://{i}{}", "a".repeat(MAX_AUDIENCE_BYTES - 9)))
                        .collect(),
                },
                supersedes: (1..g).collect(),
                retired_at: Some(g as u64 + 1),
                invalid_from: None,
            });
            let budget = principal_document_size_bound(&state);
            if budget > MAX_PRINCIPAL_DOCUMENT_BYTES {
                assert_eq!(validate_principal_state(&state), Err(Error::QuotaExceeded));
                break;
            }
            accepted += 1;
            // Name, retirement and compromise changes do not increase the budget.
            let mut changed = state.clone();
            for c in &mut changed.controllers {
                c.name = Some("\"".repeat(MAX_CONTROLLER_NAME_BYTES));
                c.retired_at = Some(sdk_id::MAX_SAFE_NONCE);
                c.invalid_from = Some(sdk_id::MAX_SAFE_NONCE);
            }
            assert_eq!(principal_document_size_bound(&changed), budget);
            let bytes = render_principal_document(&config, &account(), &changed).unwrap();
            assert!(
                bytes.len() <= budget,
                "document={} budget={budget}",
                bytes.len()
            );
        }
        assert!(accepted > 0 && accepted < MAX_CONTROLLER_RECORDS);
    }
    // Ordinary single-predecessor rotations can still use all 32 generations.
    for c in &mut state.controllers {
        c.delegation = DelegationAuthority::Unrestricted;
        c.supersedes = c
            .generation
            .checked_sub(1)
            .filter(|g| *g > 0)
            .into_iter()
            .collect();
    }
    while state.controllers.len() < MAX_CONTROLLER_RECORDS {
        let g = state.controllers.len() as u32 + 1;
        let mut c = controller(DelegationAuthority::Unrestricted);
        c.generation = g;
        c.public_key = public(g as u8);
        c.valid_from = g as u64;
        c.retired_at = Some(g as u64 + 1);
        c.supersedes = vec![g - 1];
        state.controllers.push(c);
    }
    validate_principal_state(&state).unwrap();
    assert!(
        render_principal_document(&config, &account(), &state)
            .unwrap()
            .len()
            <= MAX_PRINCIPAL_DOCUMENT_BYTES
    );
}

/// Fixed approval digest shared with the extension's protocol tests.
#[test]
fn agent_event_approval_digest_vector() {
    let account = AccountId([7; 12]);
    let event = r#"{"actor":"did:agent:AQ","created_at":1,"nonce":1,"payload":{},"protocol":"agent-delegation/1.0","type":"delegation.revoke"}"#;
    let request = dmsg_types::agent::AgentEventSignRequest {
        account_id: account,
        generation: 2,
        event: event.into(),
        origin: "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        max_cycles: 100_000_000_000,
        approval: Approval {
            device_id: Hash::new([3; 32]),
            security_epoch: 4,
            sequence: 5,
            request_id: execution_request_id(&account, 4, Hash::new([3; 32]), 5),
            expires_at: 1_790_000_000_000,
            signature: Default::default(),
        },
    }
    .into_execution(principal_id(ORIGIN, &account));
    let home = candid::Principal::from_slice(&[9, 1]);
    assert_eq!(
        request
            .approval
            .request_id
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
        "3314ea786e3936efdc446d44c256d3806f9eda70a222abb76597c0a7cd73d5c9"
    );
    assert_eq!(
        approval_message(
            home,
            &request.account_id,
            "dmsg/execute/v3",
            &(&request.kind, request.max_cycles),
            &request.approval,
        )
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>(),
        "19dbc3fca708066f83258d4eb2c0cedfab5a45f7298c223af2f73840588f32bc"
    );
}

/// Fixed account-approval digest for a registration, shared with the extension.
#[test]
fn register_controller_approval_digest_vector() {
    use dmsg_types::user::*;
    let account = AccountId([7; 12]);
    let command = AccountCommand::RegisterController {
        generation: 2,
        public_key: Hash::new([5; 32]),
        name: Some("dMsg hosted signer #2".into()),
        delegation: DelegationAuthority::Restricted {
            scopes: vec!["message.draft".into()],
            audiences: vec!["https://dmsg.net".into()],
        },
        supersedes: vec![1],
    };
    let approval = Approval {
        device_id: Hash::new([3; 32]),
        security_epoch: 4,
        sequence: 5,
        request_id: Hash::new([6; 32]),
        expires_at: 1_790_000_000_000,
        signature: Default::default(),
    };
    let message = approval_message(
        candid::Principal::from_slice(&[9, 1]),
        &account,
        "dmsg/account/v2",
        &(&10u64, &command),
        &approval,
    );
    assert_eq!(
        message
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
        "e41be524bc2a4fa8893ae6ac85723da8f743cc2fc2aa3c8dc215bd6c5e2a542a"
    );
}
