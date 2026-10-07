use super::*;
use agent_protocols::{delegation as sdk, identity as sdk_id};
use dmsg_types::agent::*;
use ic_http_certification::{HttpRequest, HttpResponse};

const QUERY_URL: &str = "https://agents.dmsg.test/v1/delegations/query";

impl Fixture {
    /// Register the self-held controller key `controller` at the next
    /// generation, proving possession inside the device-approved mutation.
    fn register_controller(
        &self,
        n: u8,
        id: &AccountId,
        generation: u32,
        controller: u8,
        supersedes: Vec<u32>,
    ) -> Result<OperationReceipt> {
        let s = self.account_id(n, id);
        let request_id = digest("test-operation", &(id, s.account_version));
        let delegation = restricted();
        let proof = key(controller)
            .sign(
                controller_pop_message(
                    self.user,
                    id,
                    generation,
                    &delegation,
                    &supersedes,
                    request_id,
                )
                .as_slice(),
            )
            .to_bytes()
            .into();
        self.mutate(
            n,
            id,
            AccountCommand::RegisterController {
                generation,
                public_key: key(controller).verifying_key().to_bytes().into(),
                name: Some(format!("dMsg signer #{generation}")),
                delegation,
                supersedes,
                proof,
            },
        )
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

/// A grant event signed locally by controller key `controller`, as the
/// extension signs it before submitting to the delegation service.
fn signed_grant(
    f: &Fixture,
    id: &AccountId,
    controller: u8,
    nonce: u64,
    audience: &str,
) -> sdk_id::Envelope<sdk::DelegationPayload> {
    let now = time(&f.ic);
    let signer = sdk_id::AgentSigner::from_seed([controller; 32]);
    let mut payload = sdk::DelegationGrantPayload::new(
        format!("{id}.draft-{nonce}"),
        format!("{PRINCIPAL_ORIGIN}/{id}"),
        sdk_id::AgentSigner::from_seed([42; 32]).agent_id(),
        vec!["message.draft".into()],
        vec![audience.into()],
    );
    payload.expires_at = Some((now + 30 * DAY) as i64);
    let event = sdk_id::Event::new(
        sdk::PROTOCOL,
        sdk::DELEGATION_GRANT,
        signer.agent_id(),
        now as i64,
        nonce,
        sdk::DelegationPayload::Grant(payload),
    );
    signer.sign_event(event).unwrap()
}

#[test]
fn self_held_principal_publishes_certified_documents_and_its_grants_are_accepted() {
    let f = Fixture::new();
    let id = f.create(1);
    let principal_id = format!("{PRINCIPAL_ORIGIN}/{id}");
    // The controller key is an ordinary Ed25519 key the client keeps in its
    // vault; the test seed stands in for it.
    let public_key: Hash = key(20).verifying_key().to_bytes().into();

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

    // A key whose possession is not proven is never bound, and nothing is consumed.
    let before = f.account_id(1, &id);
    let s = f.account_id(1, &id);
    let wrong = f.mutate(
        1,
        &id,
        AccountCommand::RegisterController {
            generation: 1,
            public_key,
            name: None,
            delegation: restricted(),
            supersedes: vec![],
            proof: key(21)
                .sign(
                    controller_pop_message(
                        f.user,
                        &id,
                        1,
                        &restricted(),
                        &[],
                        digest("test-operation", &(id, s.account_version)),
                    )
                    .as_slice(),
                )
                .to_bytes()
                .into(),
        },
    );
    assert_eq!(wrong, Err(Error::IntegrityFailed));
    assert_eq!(f.account_id(1, &id), before);
    let before_cycles = f.ic.cycle_balance(f.user);
    f.register_controller(1, &id, 1, 20, vec![]).unwrap();
    println!(
        "user_cycles method=register_controller cycles={}",
        before_cycles - f.ic.cycle_balance(f.user)
    );
    let info = f.principal(&id);
    assert_eq!((info.state.version, info.published_version), (2, 2));
    assert_eq!(info.state.controllers[0].public_key, public_key);
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
    assert_eq!(snapshot.schema, 4);
    assert_eq!(snapshot.principal_updated_at, Some(info.state.updated_at));
    assert_eq!(
        snapshot.principal_updated_at,
        Some(document.updated_at as u64)
    );

    // The client signs a grant locally; the SDK accepts it against the
    // certified document exactly as the delegation service will. Clients use
    // created_at >= valid_from; PocketIC's clock needs a nudge past it.
    f.ic.advance_time(Duration::from_millis(10));
    let envelope = signed_grant(&f, &id, 20, 10, "https://dmsg.net");
    sdk::validate_delegation_envelope(&envelope).unwrap();
    sdk::validate_delegation_acceptance(
        &envelope,
        &document,
        &principal_id,
        time(&f.ic) as i64,
        None,
    )
    .unwrap();
    // Audiences outside the published ceiling and foreign actors are refused
    // by the same acceptance rules, without any home involvement.
    let outside = signed_grant(&f, &id, 20, 11, "https://tokenlist.ing");
    assert!(sdk::validate_delegation_acceptance(
        &outside,
        &document,
        &principal_id,
        time(&f.ic) as i64,
        None,
    )
    .is_err());
    let foreign = signed_grant(&f, &id, 21, 12, "https://dmsg.net");
    assert!(sdk::validate_delegation_acceptance(
        &foreign,
        &document,
        &principal_id,
        time(&f.ic) as i64,
        None,
    )
    .is_err());

    // Rotation registers a successor that supersedes the retired generation.
    f.mutate(1, &id, AccountCommand::RetireController { generation: 1 })
        .unwrap();
    f.register_controller(1, &id, 2, 21, vec![1]).unwrap();
    let (document, bytes) = f.document(&id);
    assert_eq!(document.controllers.len(), 1);
    assert_eq!(document.retired_controllers[0].id, actor);
    let late = signed_grant(&f, &id, 20, 13, "https://dmsg.net");
    assert!(sdk::validate_delegation_acceptance(
        &late,
        &document,
        &principal_id,
        time(&f.ic) as i64,
        None,
    )
    .is_err());
    let info = f.principal(&id);
    assert_eq!((info.state.version, info.published_version), (4, 4));

    // Only the account's home publishes, and never backwards.
    let publish = |caller: Principal, state: &PrincipalState| -> Result<Publication> {
        update(&f.ic, f.directory, caller, "publish", (id, state.clone()))
    };
    assert_eq!(publish(person(1), &info.state), Err(Error::Forbidden));
    let current = publish(f.user, &info.state).unwrap();
    assert_eq!(current.version, 4);
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
    stale_time.version = 5;
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
