//! Directory-only Wasm regressions; no payment, COSE or user fixture is needed.
use candid::{utils::ArgumentEncoder, CandidType, Principal};
use dmsg_protocol::{agent, sha256};
use dmsg_types::{agent::*, *};
use ed25519_dalek::SigningKey;
use ic_http_certification::{HttpRequest, HttpResponse};
use pocket_ic::{PocketIc, PocketIcBuilder};
use serde::de::DeserializeOwned;
use std::path::PathBuf;

const NOW: u64 = 1_790_000_000_000;

fn wasm() -> Vec<u8> {
    let directory = std::env::var_os("DMSG_WASM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/wasm32-unknown-unknown/release")
        });
    std::fs::read(directory.join("dmsg_directory.wasm")).expect("build dmsg_directory Wasm first")
}

fn state(count: u32) -> PrincipalState {
    PrincipalState {
        principal_type: PrincipalType::Person,
        version: 1,
        updated_at: NOW + u64::from(count) * 2,
        controllers: (1..=count)
            .map(|g| HostedController {
                generation: g,
                public_key: Hash::new(
                    SigningKey::from_bytes(&[g as u8; 32])
                        .verifying_key()
                        .to_bytes(),
                ),
                name: None,
                valid_from: NOW + u64::from(g) * 2 - 1,
                delegation: DelegationAuthority::Restricted {
                    scopes: (0..8).map(|i| format!("{i}{}", "s".repeat(63))).collect(),
                    audiences: (0..4)
                        .map(|i| format!("https://{i}{}", "a".repeat(247)))
                        .collect(),
                },
                supersedes: (1..g).collect(),
                retired_at: (g < count).then_some(NOW + u64::from(g) * 2),
                invalid_from: None,
            })
            .collect(),
    }
}

struct Fixture {
    ic: PocketIc,
    directory: Principal,
    homes: [Principal; 2],
    config: DirectoryInit,
}

impl Fixture {
    fn new() -> Self {
        let ic = PocketIcBuilder::new()
            .with_nns_subnet()
            .with_application_subnet()
            .build();
        let homes = [ic.create_canister(), ic.create_canister()];
        let directory = ic.create_canister();
        ic.add_cycles(directory, 100_000_000_000_000);
        let config = DirectoryInit {
            environment: Environment::Local,
            issuer_namespace: "https://dmsg.test/u/".into(),
            user_homes: vec![homes[0]],
            principal_origin: "https://id.dmsg.test".into(),
            controller_source: "https://dmsg.test".into(),
            delegation_query_url: "https://agents.dmsg.test/query".into(),
            profile_url_prefix: "https://dmsg.test/u/".into(),
            custom_domains: vec!["id.dmsg.test".into()],
        };
        ic.install_canister(
            directory,
            wasm(),
            candid::encode_args((&config,)).unwrap(),
            None,
        );
        Self {
            ic,
            directory,
            homes,
            config,
        }
    }

    fn id(&self, home: usize, index: u32) -> AccountId {
        let mut id = [0; 12];
        id[4..9].copy_from_slice(
            &agent::account_allocator_digest(
                &self.config.environment,
                &self.config.issuer_namespace,
                self.homes[home],
            )[..5],
        );
        id[9..].copy_from_slice(&index.to_be_bytes()[1..]);
        AccountId(id)
    }

    fn query<A: ArgumentEncoder, R: CandidType + DeserializeOwned>(
        &self,
        method: &str,
        args: A,
    ) -> R {
        candid::decode_one(
            &self
                .ic
                .query_call(
                    self.directory,
                    Principal::anonymous(),
                    method,
                    candid::encode_args(args).unwrap(),
                )
                .unwrap(),
        )
        .unwrap()
    }

    fn update<A: ArgumentEncoder, R: CandidType + DeserializeOwned>(
        &self,
        caller: Principal,
        method: &str,
        args: A,
    ) -> R {
        candid::decode_one(
            &self
                .ic
                .update_call(
                    self.directory,
                    caller,
                    method,
                    candid::encode_args(args).unwrap(),
                )
                .unwrap(),
        )
        .unwrap()
    }

    fn publish(&self, home: usize, id: AccountId, state: &PrincipalState) -> Result<Publication> {
        self.update(self.homes[home], "publish", (id, state))
    }

    fn publication(&self, id: AccountId) -> Result<Publication> {
        self.query("get_publication", (id,))
    }

    fn verify(&self, request: HttpRequest<'static>, response: HttpResponse<'static>) -> bool {
        ic_response_verification::verify_request_response_pair(
            request,
            response,
            self.directory.as_slice(),
            u128::from(self.ic.get_time().as_nanos_since_unix_epoch()),
            300_000_000_000,
            &self.ic.root_key().unwrap(),
            2,
        )
        .is_ok()
    }

    fn get(&self, path: &str) -> HttpResponse<'static> {
        let request = HttpRequest::get(path).build();
        let response: HttpResponse<'static> = self.query("http_request", (&request,));
        assert!(
            self.verify(request, response.clone()),
            "uncertified path: {path}"
        );
        response
    }

    fn upgrade(&self, config: Option<&DirectoryInit>) -> bool {
        self.ic
            .upgrade_canister(
                self.directory,
                wasm(),
                candid::encode_args((config,)).unwrap(),
                None,
            )
            .is_ok()
    }
}

#[test]
fn publication_authentication_versions_and_rejected_updates_are_atomic() {
    let f = Fixture::new();
    let id = f.id(0, 1);
    let initial = state(1);
    assert_eq!(f.publication(id), Err(Error::NotFound));
    let forbidden: Result<Publication> =
        f.update(Principal::anonymous(), "publish", (id, &initial));
    assert_eq!(forbidden, Err(Error::Forbidden));
    assert_eq!(f.publish(1, id, &initial), Err(Error::Forbidden));
    assert_eq!(f.publish(0, f.id(1, 1), &initial), Err(Error::Forbidden));
    let publication = f.publish(0, id, &initial).unwrap();
    let before = f.get(&format!("/{id}"));
    assert_eq!(publication.document_digest, sha256(before.body()));
    assert_eq!(f.publication(id).unwrap(), publication);
    assert_eq!(f.publish(0, id, &initial).unwrap(), publication);

    let mut old = state(28); // An obsolete invalid state must still return the current publication.
    old.version = 0;
    assert_eq!(f.publish(0, id, &old).unwrap(), publication);
    let mut changed = initial.clone();
    changed.principal_type = PrincipalType::Other;
    assert_eq!(f.publish(0, id, &changed), Err(Error::IdempotencyConflict));
    changed.version += 1;
    assert_eq!(f.publish(0, id, &changed), Err(Error::IntegrityFailed));
    changed.updated_at += 1;
    changed.controllers[0].generation = 0;
    assert!(matches!(
        f.publish(0, id, &changed),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(f.publication(id).unwrap(), publication);
    assert_eq!(f.get(&format!("/{id}")).body(), before.body());
    changed.controllers[0].generation = 1;
    let next = f.publish(0, id, &changed).unwrap();
    assert_eq!(next.version, 2);
    assert_ne!(next.document_digest, publication.document_digest);
    assert_eq!(
        next.document_digest,
        sha256(f.get(&format!("/{id}")).body())
    );
}

#[test]
fn http_routes_and_changed_documents_are_certified() {
    let f = Fixture::new();
    let id = f.id(0, 1);
    let path = format!("/{id}");
    assert_eq!(f.get(&path).status_code().as_u16(), 404);
    f.publish(0, id, &state(1)).unwrap();
    let original = f.get(&path);
    assert_eq!(original.status_code().as_u16(), 200);
    for alias in [
        format!("//{id}"),
        format!("///{id}"),
        format!("/{id}?cache=1"),
        format!("/%{:02x}{}", path.as_bytes()[1], &path[2..]),
    ] {
        assert_eq!(f.get(&alias).body(), original.body(), "{alias}");
    }
    for missing in [
        "/".into(),
        "/unknown".into(),
        format!("/{id}/"),
        format!("//{id}//"),
        format!("/{id}/child"),
    ] {
        assert_eq!(f.get(&missing).status_code().as_u16(), 404, "{missing}");
    }
    for domains in [
        "/.well-known/ic-domains",
        "/.well-known//ic-domains",
        "/.well-known/%2fic-domains",
    ] {
        assert_eq!(f.get(domains).body(), b"id.dmsg.test");
    }
    let mut changed = state(1);
    changed.version = 2;
    changed.updated_at += 1;
    changed.controllers[0].name = Some("renamed".into());
    f.publish(0, id, &changed).unwrap();
    let updated = f.get(&path);
    assert_ne!(updated.body(), original.body());
    // An old body cannot be substituted under the newly published certificate.
    let tampered = HttpResponse::builder()
        .with_status_code(updated.status_code())
        .with_headers(updated.headers().to_vec())
        .with_body(original.body().to_vec())
        .build();
    assert!(!f.verify(HttpRequest::get(&path).build(), tampered));
    assert!(f.upgrade(None));
    assert_eq!(f.get(&path).body(), updated.body());
    assert_eq!(f.get("/unknown").status_code().as_u16(), 404);
}

#[test]
fn document_budget_rejection_keeps_publication_and_allows_safety_changes() {
    let f = Fixture::new();
    let id = f.id(0, 1);
    assert_eq!(
        agent::validate_principal_state(&state(28)),
        Err(Error::QuotaExceeded)
    );
    assert_eq!(f.publish(0, id, &state(28)), Err(Error::QuotaExceeded));
    assert_eq!(f.publication(id), Err(Error::NotFound));
    let initial = state(16);
    let publication = f.publish(0, id, &initial).unwrap();
    let mut oversized = state(28);
    oversized.version = 2;
    assert_eq!(f.publish(0, id, &oversized), Err(Error::QuotaExceeded));
    assert_eq!(f.publication(id).unwrap(), publication);
    assert_eq!(
        sha256(f.get(&format!("/{id}")).body()),
        publication.document_digest
    );
    let mut retired = initial;
    retired.version = 2;
    retired.updated_at += 1;
    for c in &mut retired.controllers {
        c.retired_at = Some(retired.updated_at);
        c.invalid_from = Some(c.valid_from);
        c.name = Some("\"".repeat(64));
    }
    f.publish(0, id, &retired).unwrap();
    let document = f.get(&format!("/{id}"));
    assert!(document.body().len() <= agent::MAX_PRINCIPAL_DOCUMENT_BYTES);
    assert_eq!(
        f.publication(id).unwrap().document_digest,
        sha256(document.body())
    );
}

#[test]
fn upgrades_append_homes_replace_domains_and_preserve_permanent_configuration() {
    let f = Fixture::new();
    let first = f.id(0, 1);
    let initial = f.publish(0, first, &state(1)).unwrap();
    let mut config = f.config.clone();
    config.user_homes.push(f.homes[1]);
    config.custom_domains = vec!["new.dmsg.test".into()];
    assert!(f.upgrade(Some(&config)));
    let second = f.id(1, 1);
    let other = f.publish(1, second, &state(1)).unwrap();
    assert_eq!(f.publish(1, first, &state(1)), Err(Error::Forbidden));
    assert_eq!(f.publish(0, second, &state(1)), Err(Error::Forbidden));
    assert_eq!(f.get("/.well-known/ic-domains").body(), b"new.dmsg.test");
    for field in 0..9 {
        let mut invalid = config.clone();
        match field {
            0 => invalid.environment = Environment::Staging,
            1 => invalid.issuer_namespace = "https://other.test/".into(),
            2 => invalid.principal_origin = "https://other.test".into(),
            3 => invalid.controller_source = "https://other.test".into(),
            4 => invalid.delegation_query_url = "https://other.test/query".into(),
            5 => invalid.profile_url_prefix = "https://other.test/u/".into(),
            6 => invalid.user_homes.reverse(),
            7 => {
                invalid.user_homes.pop();
            }
            _ => invalid.profile_url_prefix = format!("https://dmsg.test/{}/", "p".repeat(2_048)),
        }
        assert!(!f.upgrade(Some(&invalid)), "field {field}");
        let actual: DirectoryInit = f.query("directory_config", ());
        assert_eq!(actual, config);
    }
    assert!(f.upgrade(None));
    for (id, publication) in [(first, initial), (second, other)] {
        assert_eq!(f.publication(id).unwrap(), publication);
        assert_eq!(
            sha256(f.get(&format!("/{id}")).body()),
            publication.document_digest
        );
    }
    assert_eq!(f.get("/.well-known/ic-domains").body(), b"new.dmsg.test");
}

/// Run explicitly with --ignored --exact --nocapture. DMSG_WASM_DIR can point
/// to a baseline build; the input states and sample size stay identical.
#[test]
#[ignore = "explicit directory cost and upgrade-memory profile"]
fn directory_cost_and_rebuild_profile() {
    let f = Fixture::new();
    for count in [0, 16] {
        let id = f.id(0, count);
        let principal = state(count);
        f.publish(0, id, &principal).unwrap();
        let request = HttpRequest::get(format!("/{id}")).build();
        let certified: HttpResponse<'static> = f.query("http_request", (&request,));
        assert!(f.verify(request.clone(), certified.clone()));
        let before = f.ic.cycle_balance(f.directory);
        let publication: Result<Publication> =
            f.update(Principal::anonymous(), "get_publication", (id,));
        println!(
            "directory_publication controllers={count} bytes={} cycles={}",
            certified.body().len(),
            before - f.ic.cycle_balance(f.directory)
        );
        assert_eq!(
            publication.unwrap().document_digest,
            sha256(certified.body())
        );
        // Replicated queries measure deterministic cycle cost without the HTTP
        // certificate (which is only present in non-replicated queries).
        let before = f.ic.cycle_balance(f.directory);
        let response: HttpResponse<'static> =
            f.update(Principal::anonymous(), "http_request", (&request,));
        println!(
            "directory_http controllers={count} bytes={} cycles={}",
            response.body().len(),
            before - f.ic.cycle_balance(f.directory)
        );
        assert_eq!(response.body(), certified.body());
        let before = f.ic.cycle_balance(f.directory);
        let mut older = principal;
        older.version = 0;
        f.publish(0, id, &older).unwrap();
        println!(
            "directory_old_publish controllers={count} cycles={}",
            before - f.ic.cycle_balance(f.directory)
        );
    }
    let principal = state(16);
    for i in 1..=128 {
        f.publish(0, f.id(0, 1_000 + i), &principal).unwrap();
    }
    let before_heap =
        f.ic.canister_status(f.directory, None)
            .unwrap()
            .memory_metrics
            .wasm_memory_size;
    let before = f.ic.cycle_balance(f.directory);
    assert!(f.upgrade(None));
    let cycles = before - f.ic.cycle_balance(f.directory);
    let after_heap =
        f.ic.canister_status(f.directory, None)
            .unwrap()
            .memory_metrics
            .wasm_memory_size;
    println!("directory_rebuild records=130 heap_before={before_heap} heap_after={after_heap} cycles={cycles}");
    for i in [1, 64, 128] {
        let id = f.id(0, 1_000 + i);
        assert_eq!(
            sha256(f.get(&format!("/{id}")).body()),
            f.publication(id).unwrap().document_digest
        );
    }
}
