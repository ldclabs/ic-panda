//! Upgrade the actual pre-PrincipalDocument modules, never a synthetic new layout.
//! Run through scripts/test-dmsg-principal-upgrade.sh (schema 14 / schema 6).
use super::*;
use dmsg_types::agent::*;
use serde::Serialize;

// A subset of the old Candid variant, without the new UpdatePrincipal case.
#[derive(CandidType, Serialize, Clone)]
enum OldCommand {
    BindAuth {
        principal: Principal,
        nonce: Hash,
    },
    ReserveRoot {
        expected_generation: u64,
        op_id: OpId,
    },
    CommitRoot {
        expected_generation: u64,
        op_id: OpId,
        root: ContentRootRef,
    },
    EnablePrincipal {
        principal_type: PrincipalType,
    },
    RegisterController {
        generation: u32,
        public_key: Hash,
        name: Option<String>,
        delegation: DelegationAuthority,
        proof: Ed25519Signature,
    },
    RetireController {
        generation: u32,
    },
}

#[derive(CandidType, Clone)]
struct OldMutation {
    account_id: AccountId,
    expected_version: u64,
    command: OldCommand,
    approval: Approval,
}

#[derive(CandidType, Deserialize, Serialize, Clone)]
struct OldState {
    principal_type: PrincipalType,
    controllers: Vec<HostedController>,
    version: u64,
    updated_at: u64,
}

#[derive(CandidType, Deserialize)]
struct OldInfo {
    principal_id: String,
    state: OldState,
    published_version: u64,
}

pub(super) fn old_wasm(name: &str) -> Vec<u8> {
    let directory = PathBuf::from(
        std::env::var_os("DMSG_PRE_PRINCIPAL_WASM_DIR")
            .expect("run scripts/test-dmsg-principal-upgrade.sh"),
    );
    std::fs::read(directory.join(format!("{name}.wasm"))).unwrap()
}

fn old_mutate(
    f: &Fixture,
    n: u8,
    id: &AccountId,
    command: OldCommand,
) -> (OldMutation, OperationReceipt) {
    let s = f.account_id(n, id);
    let mut mutation = OldMutation {
        account_id: *id,
        expected_version: s.account_version,
        command,
        approval: Approval {
            device_id: Hash::new([n; 32]),
            security_epoch: s.security_epoch,
            sequence: s.devices[&Hash::new([n; 32])].next_sequence,
            request_id: digest("test-operation", &(id, s.account_version)),
            expires_at: time(&f.ic) + MINUTE,
            signature: Default::default(),
        },
    };
    mutation.approval.signature = key(n)
        .sign(
            approval_message(
                f.user,
                id,
                "dmsg/account/v2",
                &(mutation.expected_version, &mutation.command),
                &mutation.approval,
            )
            .as_slice(),
        )
        .to_bytes()
        .into();
    let receipt: Result<OperationReceipt> = update(
        &f.ic,
        f.user,
        person(n),
        "mutate_account",
        (mutation.clone(),),
    );
    (mutation, receipt.unwrap())
}

fn old_info(f: &Fixture, id: &AccountId) -> OldInfo {
    let info: Result<OldInfo> = query(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "get_principal",
        (id,),
    );
    info.unwrap()
}

fn unlock(f: &Fixture, login: u8, id: &AccountId, device: u8) -> Hash {
    let result: Result<Hash> = query(
        &f.ic,
        f.user,
        person(login),
        "unlock_secret",
        (id, Hash::new([device; 32])),
    );
    result.unwrap()
}

fn snapshot(f: &Fixture, id: &AccountId) -> Vec<u8> {
    let result: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "security_snapshot_batch",
        (vec![id],),
    );
    dmsg_client::certified::certified_path(
        &result.unwrap(),
        id.as_slice(),
        &[id.as_slice()],
        f.user,
        &f.ic.root_key().unwrap(),
        time(&f.ic),
    )
    .unwrap()
    .unwrap()
}

fn upgrade(f: &Fixture) {
    // Upgrade the directory first, then each user home; canister IDs stay fixed.
    for (id, name) in [(f.directory, "dmsg_directory"), (f.user, "dmsg_user")] {
        f.ic.upgrade_canister(id, wasm(name), candid::encode_args(()).unwrap(), None)
            .unwrap();
    }
}

#[test]
#[ignore = "requires schema 14/6 Wasm; run scripts/test-dmsg-principal-upgrade.sh"]
fn schema_14_and_6_upgrade_preserves_accounts_keys_documents_and_replays() {
    let f = Fixture::with_identity_modules(
        true,
        100,
        MAX_PAYMENT_ESCROWS,
        |_| {},
        [
            old_wasm("dmsg_user"),
            old_wasm("dmsg_directory"),
            old_wasm("dmsg_handle"),
        ],
    );
    let id = f.create(1);
    let without_principal = f.create(2);
    let empty_principal = f.create(3);
    let root_op = Hash::new([80; 32]);
    old_mutate(
        &f,
        1,
        &id,
        OldCommand::ReserveRoot {
            expected_generation: 0,
            op_id: root_op,
        },
    );
    let root = f.root_ref(1, &id, 1);
    old_mutate(
        &f,
        1,
        &id,
        OldCommand::CommitRoot {
            expected_generation: 0,
            op_id: root_op,
            root: root.clone(),
        },
    );
    old_mutate(
        &f,
        1,
        &id,
        OldCommand::BindAuth {
            principal: person(9),
            nonce: Hash::new([81; 32]),
        },
    );
    let accepted: Result<()> = update(
        &f.ic,
        f.user,
        person(9),
        "accept_auth_binding",
        (id, Hash::new([81; 32])),
    );
    accepted.unwrap();
    for (n, account) in [(1, id), (3, empty_principal)] {
        old_mutate(
            &f,
            n,
            &account,
            OldCommand::EnablePrincipal {
                principal_type: PrincipalType::Person,
            },
        );
    }
    for generation in 1..=2 {
        let controller = 20 + generation as u8;
        let s = f.account_id(1, &id);
        let request = digest("test-operation", &(&id, s.account_version));
        let delegation = DelegationAuthority::Unrestricted;
        let proof = key(controller)
            .sign(controller_pop_message(f.user, &id, generation, &delegation, request).as_slice())
            .to_bytes()
            .into();
        old_mutate(
            &f,
            1,
            &id,
            OldCommand::RegisterController {
                generation,
                public_key: key(controller).verifying_key().to_bytes().into(),
                name: Some(format!("Existing {generation}")),
                delegation,
                proof,
            },
        );
    }
    let (replay, receipt) = old_mutate(&f, 1, &id, OldCommand::RetireController { generation: 1 });
    // Also retain an independently verified execution receipt and its artifact.
    let request = f.attest_request(
        1,
        &id,
        Statement {
            issuer: account_issuer(NAMESPACE, &id),
            subject: Some("before-upgrade".into()),
            issued_at: None,
            content: StatementContent::Digest {
                sha256: Hash::new([82; 32]),
                content_type: None,
                location: None,
            },
        },
    );
    let artifact: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest", (request.clone(),));
    let artifact = artifact.unwrap();
    // A committed principal whose directory call failed must remain retryable.
    let delayed = f.create(4);
    f.ic.stop_canister(f.directory, None).unwrap();
    old_mutate(
        &f,
        4,
        &delayed,
        OldCommand::EnablePrincipal {
            principal_type: PrincipalType::Person,
        },
    );
    assert_eq!(old_info(&f, &delayed).published_version, 0);
    let before_delayed = f.account_id(4, &delayed);
    f.ic.start_canister(f.directory, None).unwrap();
    let before = f.account_id(1, &id);
    let before_other = f.account_id(2, &without_principal);
    let before_empty = f.account_id(3, &empty_principal);
    let security = snapshot(&f, &id);
    let secret = unlock(&f, 1, &id, 1);
    assert_eq!(unlock(&f, 9, &id, 1), secret);
    let info = old_info(&f, &id);
    assert_eq!(info.state.version, info.published_version);
    let old_body = f.directory_get(&format!("/{id}")).body().to_vec();
    let empty_body = f
        .directory_get(&format!("/{empty_principal}"))
        .body()
        .to_vec();
    let publication: Result<Publication> = query(
        &f.ic,
        f.directory,
        Principal::anonymous(),
        "get_publication",
        (id,),
    );
    let user_config: UserInit = query(&f.ic, f.user, Principal::anonymous(), "user_config", ());
    let directory_config: DirectoryInit = query(
        &f.ic,
        f.directory,
        Principal::anonymous(),
        "directory_config",
        (),
    );

    f.ic.upgrade_canister(
        f.directory,
        wasm("dmsg_directory"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    // The old, still-running home can send its original Candid state without metadata.
    let old_replay: Result<Publication> = update(
        &f.ic,
        f.directory,
        f.user,
        "publish",
        (id, info.state.clone()),
    );
    assert_eq!(old_replay, publication);
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(f.account_id(1, &id), before);
    assert_eq!(f.account_id(9, &id), before);
    assert_eq!(f.account_id(2, &without_principal), before_other);
    assert_eq!(f.account_id(3, &empty_principal), before_empty);
    assert_eq!(f.account_id(4, &delayed), before_delayed);
    assert_eq!(f.principal(&delayed).published_version, 0);
    let resumed: Result<u64> = update(
        &f.ic,
        f.user,
        Principal::anonymous(),
        "publish_principal",
        (delayed,),
    );
    assert_eq!(resumed, Ok(1));
    assert_eq!(
        f.document(&delayed).0.id,
        format!("{PRINCIPAL_ORIGIN}/{delayed}")
    );
    assert_eq!(unlock(&f, 1, &id, 1), secret);
    assert_eq!(snapshot(&f, &id), security);
    assert_eq!(
        query::<_, UserInit>(&f.ic, f.user, Principal::anonymous(), "user_config", ()),
        user_config
    );
    assert_eq!(
        query::<_, DirectoryInit>(
            &f.ic,
            f.directory,
            Principal::anonymous(),
            "directory_config",
            ()
        ),
        directory_config
    );
    assert_eq!(f.document(&id).1, old_body);
    assert_eq!(f.directory_get("/favicon.ico").status_code().as_u16(), 200);
    assert_eq!(f.document(&empty_principal).1, empty_body);
    let current = f.principal(&id);
    assert_eq!(current.principal_id, info.principal_id);
    assert_eq!(current.state.controllers, info.state.controllers);
    assert_eq!(current.state.metadata.as_ref(), b"{}");
    assert_eq!(
        (
            current.state.version,
            current.state.updated_at,
            current.published_version
        ),
        (
            info.state.version,
            info.state.updated_at,
            info.published_version
        )
    );
    let replayed: Result<OperationReceipt> =
        update(&f.ic, f.user, person(1), "mutate_account", (replay,));
    assert_eq!(replayed, Ok(receipt));
    let saved_artifact: Result<SignedArtifact> = query(
        &f.ic,
        f.user,
        person(1),
        "get_attestation",
        (id, request.approval.request_id),
    );
    assert_eq!(saved_artifact, Ok(artifact));
    let saved_receipt: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_receipt",
        (id, request.approval.request_id),
    );
    let receipt_key = execution_receipt_key(&id, request.approval.request_id);
    assert!(
        dmsg_client::certified::certified_path(
            &saved_receipt.unwrap(),
            &receipt_key,
            &[&receipt_key],
            f.user,
            &f.ic.root_key().unwrap(),
            time(&f.ic)
        )
        .unwrap()
        .is_some()
    );

    // A home may retry a publication whose reply was lost before upgrading.
    let republished: Result<Publication> = update(
        &f.ic,
        f.directory,
        f.user,
        "publish",
        (id, current.state.clone()),
    );
    assert_eq!(republished, publication);
    let mut conflicting = current.state.clone();
    conflicting.metadata = br#"{"name":"Cannot overwrite the same version"}"#.to_vec().into();
    let rejected: Result<Publication> =
        update(&f.ic, f.directory, f.user, "publish", (id, conflicting));
    assert_eq!(rejected, Err(Error::IdempotencyConflict));
    assert_eq!(f.document(&id).1, old_body);
    let missing: Result<CertifiedBatch> = query(
        &f.ic,
        f.directory,
        Principal::anonymous(),
        "principal_document",
        (without_principal,),
    );
    assert!(
        dmsg_client::certified::certified_path(
            &missing.unwrap(),
            without_principal.as_slice(),
            &[b"documents", without_principal.as_slice()],
            f.directory,
            &f.ic.root_key().unwrap(),
            time(&f.ic)
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(f.directory_get("/missing").status_code().as_u16(), 404);
    assert_eq!(
        f.directory_get("/.well-known/ic-domains").body(),
        b"id.dmsg.test"
    );

    // Existing accounts can now publish display fields; controller authority stays intact.
    let mut document = f.document(&id).0;
    document.name = Some("After upgrade".into());
    f.mutate(
        1,
        &id,
        AccountCommand::UpdatePrincipal {
            document: serde_json::to_vec(&document).unwrap().into(),
        },
    )
    .unwrap();
    assert_eq!(f.principal(&id).state.controllers, info.state.controllers);
    assert_eq!(f.account_id(1, &id).current_root, Some(root));
    assert_eq!(f.account_id(1, &id).security_epoch, before.security_epoch);
    assert_eq!(f.document(&id).0.name.as_deref(), Some("After upgrade"));
    let updated_body = f.document(&id).1;
    let updated_account = f.account_id(1, &id);
    upgrade(&f); // schema 15/7 must not repeat the migration or replace metadata.
    assert_eq!(f.document(&id).1, updated_body);
    assert_eq!(f.document(&empty_principal).1, empty_body);
    assert_eq!(f.account_id(1, &id), updated_account);
    assert_eq!(unlock(&f, 1, &id, 1), secret);
    assert_eq!(f.create(1), id); // permanent auth routing remains bound to the same ID.
    let next = f.create(5);
    assert!(![id, without_principal, empty_principal, delayed].contains(&next));
    let stats: UserStats = query(&f.ic, f.user, Principal::anonymous(), "user_stats", ());
    assert_eq!((stats.accounts, stats.created_today), (5, 5));
}
