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

fn controller(delegation: DelegationAuthority) -> HostedController {
    HostedController {
        generation: 1,
        public_key: public(1),
        name: None,
        valid_from: NOW - DAY,
        delegation,
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
        delegation_service: "https://agents.dmsg.test".into(),
        profile_url_prefix: "https://dmsg.test/u/".into(),
        custom_domains: vec!["id.dmsg.test".into()],
        governance: Principal::from_slice(&[9]),
    }
}

fn state() -> PrincipalState {
    let mut first = controller(restricted());
    first.retired_at = Some(NOW);
    first.name = Some("dMsg hosted signer #1".into());
    let mut second = controller(DelegationAuthority::Unrestricted);
    second.generation = 2;
    second.public_key = public(2);
    second.valid_from = NOW;
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
    assert_eq!(document.retired_controllers[0].retired_at, Some(NOW as i64));
    assert_eq!(
        document.delegation_service.as_deref(),
        Some("https://agents.dmsg.test")
    );
    assert_eq!(
        document.links[0].url,
        format!("https://dmsg.test/u/{}", account())
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
        |c: &mut DirectoryInit| {
            c.delegation_service = "https://agents.dmsg.test/v1/delegations/query".into()
        },
        |c: &mut DirectoryInit| c.profile_url_prefix = "https://dmsg.test/u".into(),
        |c: &mut DirectoryInit| c.user_homes.push(c.user_homes[0]),
        |c: &mut DirectoryInit| c.custom_domains = vec!["ID.dmsg.test".into()],
        |c: &mut DirectoryInit| c.user_homes.clear(),
        |c: &mut DirectoryInit| c.governance = Principal::anonymous(),
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
fn user_homes_route_each_account_to_its_allocator() {
    let (env, ns) = (Environment::Local, "https://dmsg.test/u/");
    let homes: Vec<Principal> = (1..=3).map(|n| Principal::from_slice(&[n])).collect();
    validate_user_homes(&env, ns, &homes).unwrap();
    for (index, home) in homes.iter().enumerate() {
        let digest = account_allocator_digest(&env, ns, *home);
        let mut id = [0; 12];
        id[4..9].copy_from_slice(&digest[..5]);
        let id = AccountId(id);
        assert_eq!(account_home(&env, ns, &homes, &id), Some(*home));
        assert!(is_account_home(&env, ns, &homes, *home, &id));
        assert!(!is_account_home(&env, ns, &homes[..index], *home, &id));
        let other = homes[(index + 1) % homes.len()];
        assert!(!is_account_home(&env, ns, &homes, other, &id));
    }
    assert_eq!(account_home(&env, ns, &homes, &AccountId([0; 12])), None);

    let next = Principal::from_slice(&[5]);
    assert_eq!(check_user_home(&env, ns, &homes, next), Ok(true));
    assert_eq!(check_user_home(&env, ns, &homes, homes[1]), Ok(false));
    assert_eq!(
        check_user_home(&env, ns, &homes, Principal::anonymous()),
        Err(Error::AuthRequired)
    );
    let full: Vec<Principal> = (0..MAX_USER_HOMES as u16)
        .map(|n| Principal::from_slice(&n.to_be_bytes()))
        .collect();
    assert_eq!(
        check_user_home(&env, ns, &full, next),
        Err(Error::QuotaExceeded)
    );
    assert!(validate_user_homes(&env, ns, &[]).is_err());
    assert!(validate_user_homes(&env, ns, &[homes[0], homes[0]]).is_err());
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
            c.delegation_service = format!("https://{}", "a".repeat(MAX_PRINCIPAL_ORIGIN_BYTES))
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
    config.delegation_service = config.principal_origin.clone();
    config.profile_url_prefix = format!(
        "https://dmsg.test/{}/",
        "p".repeat(MAX_DIRECTORY_URL_BYTES - "https://dmsg.test//".len())
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
    // Ordinary rotations can still use all 32 generations.
    for c in &mut state.controllers {
        c.delegation = DelegationAuthority::Unrestricted;
    }
    while state.controllers.len() < MAX_CONTROLLER_RECORDS {
        let g = state.controllers.len() as u32 + 1;
        let mut c = controller(DelegationAuthority::Unrestricted);
        c.generation = g;
        c.public_key = public(g as u8);
        c.valid_from = g as u64;
        c.retired_at = Some(g as u64 + 1);
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
        proof: [8; 64].into(),
    };
    let approval = Approval {
        device_id: Hash::new([3; 32]),
        security_epoch: 4,
        sequence: 5,
        request_id: Hash::new([6; 32]),
        expires_at: 1_790_000_000_000,
        signature: Default::default(),
    };
    let home = candid::Principal::from_slice(&[9, 1]);
    let message = approval_message(home, &account, "dmsg/account/v2", &(&10u64, &command), &approval);
    let hex = |h: Hash| h.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        hex(message),
        "8124d3a5f7974f6735155e84f25efc1ca0dcc0608a9d273bb0f83569c1d1957b"
    );
    let AccountCommand::RegisterController {
        generation,
        delegation,
        ..
    } = &command
    else {
        panic!()
    };
    assert_eq!(
        hex(controller_pop_message(
            home,
            &account,
            *generation,
            delegation,
            approval.request_id
        )),
        "992ec6d7a7075d69c4c911aa4f218bbe884ca882d223d4e8f789d92c51089f31"
    );
}
