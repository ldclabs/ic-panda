use super::*;
use agent_protocols::{delegation as sdk, identity as sdk_id};
use dmsg_types::agent::*;
use ic_http_certification::{HttpRequest, HttpResponse};

const QUERY_URL: &str = "https://agents.dmsg.test/v1/delegations/query";

impl Fixture {
    fn register_controller(
        &self,
        n: u8,
        id: &AccountId,
        command: AccountCommand,
    ) -> Result<OperationReceipt> {
        let s = self.account_id(n, id);
        let mut m = AccountMutation {
            account_id: *id,
            expected_version: s.account_version,
            command,
            approval: Approval {
                device_id: Hash::new([n; 32]),
                security_epoch: s.security_epoch,
                sequence: s.devices[&Hash::new([n; 32])].next_sequence,
                request_id: digest("test-operation", &(id, s.account_version)),
                expires_at: time(&self.ic) + MINUTE,
                signature: Default::default(),
            },
        };
        m.approval.signature = key(n)
            .sign(
                approval_message(
                    self.user,
                    id,
                    "dmsg/account/v2",
                    &(&m.expected_version, &m.command),
                    &m.approval,
                )
                .as_slice(),
            )
            .to_bytes()
            .into();
        update(&self.ic, self.user, person(n), "register_controller", (m,))
    }

    fn controller_key(&self, id: &AccountId, generation: u32) -> KeyDescriptor {
        let key: Result<KeyDescriptor> = query(
            &self.ic,
            self.cose,
            Principal::anonymous(),
            "public_key",
            (id, KeySelector::AgentController { generation }),
        );
        key.unwrap()
    }

    fn principal(&self, id: &AccountId) -> PrincipalInfo {
        let info: Result<PrincipalInfo> = query(
            &self.ic,
            self.user,
            Principal::anonymous(),
            "get_principal",
            (id,),
        );
        info.unwrap()
    }

    fn sign_agent_event(
        &self,
        n: u8,
        id: &AccountId,
        generation: u32,
        event: String,
    ) -> Result<ExecutionResult> {
        let s = self.account_id(n, id);
        let sequence = s.devices[&Hash::new([n; 32])].next_sequence;
        let mut request = AgentEventSignRequest {
            account_id: *id,
            generation,
            event,
            origin: "https://example.com".into(),
            max_cycles: 100_000_000_000,
            approval: Approval {
                device_id: Hash::new([n; 32]),
                security_epoch: s.security_epoch,
                sequence,
                request_id: execution_request_id(
                    id,
                    s.security_epoch,
                    Hash::new([n; 32]),
                    sequence,
                ),
                expires_at: time(&self.ic) + MINUTE,
                signature: Default::default(),
            },
        };
        let execution = request
            .clone()
            .into_execution(format!("{PRINCIPAL_ORIGIN}/{id}"));
        request.approval.signature = key(n)
            .sign(execution.approval_message(self.user).as_slice())
            .to_bytes()
            .into();
        update(
            &self.ic,
            self.user,
            person(n),
            "sign_agent_event",
            (request,),
        )
    }

    /// Fetch a directory path and verify the response like an ICP HTTP gateway.
    fn directory_get(&self, path: &str) -> HttpResponse<'static> {
        let request = HttpRequest::get(path).build();
        let response: HttpResponse<'static> = query(
            &self.ic,
            self.directory,
            Principal::anonymous(),
            "http_request",
            (request.clone(),),
        );
        let now = u128::from(self.ic.get_time().as_nanos_since_unix_epoch());
        let verified = ic_response_verification::verify_request_response_pair(
            request,
            response.clone(),
            self.directory.as_slice(),
            now,
            u128::from(5 * MINUTE) * 1_000_000,
            &self.ic.root_key().unwrap(),
            2,
        )
        .unwrap_or_else(|e| panic!("{path}: {e:?}"));
        assert_eq!(verified.verification_version, 2);
        assert_eq!(
            verified.response.map(|r| r.body),
            Some(response.body().to_vec())
        );
        response
    }

    fn document(&self, id: &AccountId) -> (sdk::PrincipalDocument, Vec<u8>) {
        let response = self.directory_get(&format!("/{id}"));
        assert_eq!(response.status_code().as_u16(), 200);
        let header = |name: &str| {
            response
                .headers()
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.clone())
        };
        assert_eq!(header("content-type").as_deref(), Some("application/json"));
        assert_eq!(header("access-control-allow-origin").as_deref(), Some("*"));
        let bytes = response.body().to_vec();
        let text = std::str::from_utf8(&bytes).unwrap();
        let document: sdk::PrincipalDocument =
            serde_json::from_value(sdk_id::parse_strict_json(text).unwrap()).unwrap();
        sdk::validate_principal_document(&document).unwrap();
        sdk::validate_principal_resolution(&document, &format!("{PRINCIPAL_ORIGIN}/{id}")).unwrap();
        (document, bytes)
    }
}

fn restricted() -> DelegationAuthority {
    DelegationAuthority::Restricted {
        scopes: vec!["message.draft".into()],
        audiences: vec!["https://dmsg.net".into()],
    }
}

fn grant_event(
    f: &Fixture,
    id: &AccountId,
    actor: &Hash,
    nonce: u64,
    audience: &str,
) -> sdk_id::Event<sdk::DelegationPayload> {
    let now = time(&f.ic);
    let mut payload = sdk::DelegationGrantPayload::new(
        format!("{id}.draft-{nonce}"),
        format!("{PRINCIPAL_ORIGIN}/{id}"),
        sdk_id::AgentSigner::from_seed([42; 32]).agent_id(),
        vec!["message.draft".into()],
        vec![audience.into()],
    );
    payload.expires_at = Some((now + 30 * DAY) as i64);
    sdk_id::Event::new(
        sdk::PROTOCOL,
        sdk::DELEGATION_GRANT,
        sdk_id::AgentId::from_public_key(actor),
        now as i64,
        nonce,
        sdk::DelegationPayload::Grant(payload),
    )
}

fn jcs(event: &sdk_id::Event<sdk::DelegationPayload>) -> String {
    String::from_utf8(sdk_id::canonical_event_bytes(event).unwrap()).unwrap()
}

#[test]
fn hosted_principal_publishes_certified_documents_and_signs_acceptable_grants() {
    let f = Fixture::new();
    let ready: Result<KeyState> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    ready.unwrap();
    let id = f.create(1);
    f.recoverable(1, &id);
    let principal_id = format!("{PRINCIPAL_ORIGIN}/{id}");

    // Absent principals are a certified 404, not an empty document.
    let missing = f.directory_get(&format!("/{id}"));
    assert_eq!(missing.status_code().as_u16(), 404);
    let domains = f.directory_get("/.well-known/ic-domains");
    assert_eq!(domains.body(), b"id.dmsg.test");

    f.mutate(
        1,
        &id,
        AccountCommand::EnablePrincipal {
            principal_type: PrincipalType::Person,
        },
    )
    .unwrap();
    let info = f.principal(&id);
    assert_eq!(info.principal_id, principal_id);
    assert_eq!((info.state.version, info.published_version), (1, 1));
    let (document, _) = f.document(&id);
    assert!(document.controllers.is_empty());
    assert_eq!(document.kind.as_deref(), Some("person"));
    assert_eq!(document.delegation_query_url.as_deref(), Some(QUERY_URL));

    // A key the owner did not approve is never bound, and nothing is consumed.
    let key = f.controller_key(&id, 1);
    let public_key: Hash = key.public_key.as_slice().try_into().map(Hash::new).unwrap();
    let before = f.account_id(1, &id);
    let wrong = f.register_controller(
        1,
        &id,
        AccountCommand::RegisterController {
            generation: 1,
            public_key: Hash::new([7; 32]),
            name: None,
            delegation: restricted(),
            supersedes: vec![],
        },
    );
    assert_eq!(wrong, Err(Error::IntegrityFailed));
    assert_eq!(f.account_id(1, &id).account_version, before.account_version);
    let register = AccountCommand::RegisterController {
        generation: 1,
        public_key,
        name: Some("dMsg hosted signer #1".into()),
        delegation: restricted(),
        supersedes: vec![],
    };
    // The ordinary mutation path cannot skip the derivation check.
    assert!(matches!(
        f.mutate(1, &id, register.clone()),
        Err(Error::InvalidInput(_))
    ));
    f.register_controller(1, &id, register).unwrap();
    let info = f.principal(&id);
    assert_eq!((info.state.version, info.published_version), (2, 2));
    let (document, _) = f.document(&id);
    assert_eq!(document.controllers.len(), 1);
    let actor = sdk_id::AgentId::from_public_key(&public_key);
    assert_eq!(document.controllers[0].id, actor);
    assert_eq!(document.controllers[0].source, "https://dmsg.net");

    // The certified security snapshot carries the principal's updated_at.
    let batch: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "security_snapshot_batch",
        (vec![id],),
    );
    let snapshot: SecuritySnapshot =
        decode_canonical(&certified_value(&f, batch.unwrap(), id.as_slice())).unwrap();
    assert_eq!(snapshot.schema, 3);
    assert_eq!(snapshot.principal_updated_at, Some(info.state.updated_at));
    assert_eq!(
        snapshot.principal_updated_at,
        Some(document.updated_at as u64)
    );

    // The hosted key signs an exact grant that the SDK accepts against the
    // certified document, exactly as the delegation service will. Clients use
    // created_at >= valid_from; PocketIC's clock needs a nudge past it.
    f.ic.advance_time(Duration::from_millis(10));
    let event = grant_event(&f, &id, &public_key, 10, "https://dmsg.net");
    let result = f.sign_agent_event(1, &id, 1, jcs(&event)).unwrap();
    assert_eq!(result.status(), ExecutionStatus::Completed);
    let ExecutionOutput::AgentSignature {
        event_hash,
        signature,
        key: descriptor,
    } = result.output().unwrap().clone()
    else {
        panic!("agent signature output")
    };
    assert_eq!(descriptor.purpose, KeyPurpose::AgentController);
    assert_eq!(descriptor.public_key.as_slice(), public_key.as_slice());
    assert_eq!(
        event_hash.into_array(),
        sdk_id::event_hash_bytes(&event).unwrap()
    );
    use base64::Engine;
    let envelope = sdk_id::Envelope {
        hash: sdk_id::event_hash(&event).unwrap(),
        event: event.clone(),
        signature: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.as_slice()),
    };
    sdk::validate_delegation_envelope(&envelope).unwrap();
    sdk::validate_delegation_acceptance(
        &envelope,
        &document,
        &principal_id,
        time(&f.ic) as i64,
        None,
    )
    .unwrap();
    assert_eq!(f.principal(&id).last_nonces.get(&1), Some(&10));

    // Replayed nonces, audiences outside the ceiling and foreign actors are refused.
    let replay = grant_event(&f, &id, &public_key, 10, "https://dmsg.net");
    assert_eq!(
        f.sign_agent_event(1, &id, 1, jcs(&replay)),
        Err(Error::VersionConflict)
    );
    let outside = grant_event(&f, &id, &public_key, 11, "https://tokenlist.ing");
    assert_eq!(
        f.sign_agent_event(1, &id, 1, jcs(&outside)),
        Err(Error::Forbidden)
    );
    let foreign = grant_event(&f, &id, &Hash::new([9; 32]), 12, "https://dmsg.net");
    assert!(f.sign_agent_event(1, &id, 1, jcs(&foreign)).is_err());

    // Retirement takes effect in the home immediately and is published.
    f.mutate(1, &id, AccountCommand::RetireController { generation: 1 })
        .unwrap();
    let late = grant_event(&f, &id, &public_key, 13, "https://dmsg.net");
    assert_eq!(
        f.sign_agent_event(1, &id, 1, jcs(&late)),
        Err(Error::Forbidden)
    );
    let (document, bytes) = f.document(&id);
    assert!(document.controllers.is_empty());
    assert_eq!(document.retired_controllers[0].id, actor);
    let info = f.principal(&id);
    assert_eq!((info.state.version, info.published_version), (3, 3));

    // Only the account's home publishes, and never backwards.
    let publish = |caller: Principal, state: &PrincipalState| -> Result<Publication> {
        update(&f.ic, f.directory, caller, "publish", (id, state.clone()))
    };
    assert_eq!(publish(person(1), &info.state), Err(Error::Forbidden));
    let current = publish(f.user, &info.state).unwrap();
    assert_eq!(current.version, 3);
    assert_eq!(current.document_digest, sha256(&bytes));
    let mut older = info.state.clone();
    older.version = 2;
    assert_eq!(publish(f.user, &older).unwrap(), current);
    let mut conflicting = info.state.clone();
    conflicting.principal_type = PrincipalType::Other;
    assert_eq!(
        publish(f.user, &conflicting),
        Err(Error::IdempotencyConflict)
    );
    let mut stale_time = info.state.clone();
    stale_time.version = 4;
    assert_eq!(publish(f.user, &stale_time), Err(Error::IntegrityFailed));
    let foreign_account = AccountId([1; 12]);
    let rejected: Result<Publication> = update(
        &f.ic,
        f.directory,
        f.user,
        "publish",
        (foreign_account, info.state.clone()),
    );
    assert_eq!(rejected, Err(Error::Forbidden));

    // Upgrades rebuild the certification tree from stable documents.
    f.ic.upgrade_canister(
        f.directory,
        wasm("dmsg_directory"),
        candid::encode_args((None::<DirectoryInit>,)).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(f.document(&id).1, bytes);
    assert_eq!(f.directory_get("/unknown").status_code().as_u16(), 404);
}
