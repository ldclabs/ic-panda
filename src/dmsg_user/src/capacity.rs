//! Host-built capacity images: the home's own stable records and certification
//! tree, which `user_capacity_profile` loads into PocketIC.
//!
//! Every fifth account is active: three devices, two logins, a committed root,
//! a full receipt window, two month rows and one expired attestation for the
//! public cleanup. The rest are fresh accounts with one device and one login.
use crate::{commerce::Month, execution, state::*, store::*, xid};
use candid::Principal;
use dmsg_protocol::{billing::month_utc, canonical, digest, execution_receipt_key};
use dmsg_runtime::{cert_map::leaf_hash, storage::MapExt, Budget};
use dmsg_types::{billing::*, user::*, *};
use ed25519_dalek::SigningKey;
use ic_auth_types::XidGenerator;
use std::collections::BTreeMap;

/// Profile start time; image accounts were created a month earlier.
pub const AT: u64 = 1_800_000_000_000;
const NAMESPACE: &str = "https://dmsg.test/u/";
const SIZES: [u64; 2] = [10_000, 1_000_000];

/// Login `n` of image account `i`, as the profile calls it.
pub fn login(i: u64, n: u64) -> Principal {
    Principal::self_authenticating(digest("capacity login", &(i, n)).as_slice())
}

/// Signing key of device `n` of image account `i`.
pub fn device_key(i: u64, n: u64) -> SigningKey {
    SigningKey::from_bytes(&digest("capacity device", &(i, n)))
}

fn device(i: u64, n: u64, admin: bool, revoked: bool) -> (Hash, Device) {
    let id = digest("capacity device id", &(i, n));
    let device = Device {
        input: DeviceInput {
            device_id: id,
            signing_pub: device_key(i, n).verifying_key().to_bytes().into(),
            hpke_pub: digest("capacity hpke", &(i, n)),
            role: if admin {
                ControllerRole::Administrator
            } else {
                ControllerRole::Member
            },
            capabilities: if admin {
                vec![
                    Capability::RootManage,
                    Capability::FormalApprove,
                    Capability::VaultUnlock,
                    Capability::PaymentOffer,
                    Capability::ContentSign,
                ]
            } else {
                vec![Capability::VaultUnlock, Capability::ContentSign]
            },
        },
        added_at: AT - 30 * DAY,
        added_by: (n > 0).then(|| digest("capacity device id", &(i, 0u64))),
        revoked_at: revoked.then_some(AT - 10 * DAY),
        next_sequence: if admin { 20 } else { 3 },
    };
    (id, device)
}

fn account(home: Principal, cose: Principal, i: u64, id: AccountId) -> AccountState {
    let active = i.is_multiple_of(5);
    let mut s = AccountState {
        created_at_ms: AT - 30 * DAY,
        account_id: id,
        home_user: home,
        home_cose: cose,
        auth_bindings: vec![login(i, 0)],
        pending_bindings: vec![],
        account_version: 0,
        security_epoch: 0,
        devices: BTreeMap::from([device(i, 0, true, false)]),
        recovery_delay_ms: DEFAULT_RECOVERY_DELAY_MS,
        pending_recovery: None,
        completed_recovery: None,
        current_root: None,
        root_slot: None,
        next_root_generation: 1,
        vault_write_state: VaultWriteState::Uninitialized,
        sensitive_policy: SensitivePolicy::default(),
        budget: Budget::default(),
        next_execution_sequence: 1,
        operations: vec![],
        handle_authorizations: BTreeMap::new(),
        execution_expirations: BTreeMap::new(),
        principal_updated_at: None,
    };
    if !active {
        return s;
    }
    s.auth_bindings.push(login(i, 1));
    s.devices
        .extend([device(i, 1, false, false), device(i, 2, false, true)]);
    s.account_version = 40;
    s.security_epoch = 6;
    s.current_root = Some(ContentRootRef {
        generation: 2,
        suite: "dmsg-root-v2".into(),
        bundle_digest: digest("capacity bundle", &i),
        recipients_digest: digest("capacity recipients", &i),
        body_digest: digest("capacity body", &i),
    });
    s.next_root_generation = 3;
    s.vault_write_state = VaultWriteState::Ready;
    s.operations = (0..MAX_OPERATION_RECEIPTS as u64)
        .map(|n| OperationReceipt {
            id: digest("capacity operation", &(i, n)),
            digest: digest("capacity operation digest", &(i, n)),
            account_version: 24 + n,
        })
        .collect();
    s.execution_expirations
        .insert(digest("capacity execution", &i), Some(AT - DAY));
    s
}

/// An attestation whose retention ended a day before `AT`.
fn expired_attestation(s: &AccountState, i: u64) -> AuthorizedExecution {
    let device_id = digest("capacity device id", &(i, 0u64));
    AuthorizedExecution {
        account_id: s.account_id,
        request_id: digest("capacity execution", &i),
        command_digest: digest("capacity command", &i),
        record: ExecutionRecord::Attestation(Attestation {
            device_id,
            security_epoch: s.security_epoch,
            approved_at: AT - 2 * DAY - MINUTE,
            expires_at: AT - 2 * DAY,
            origin: "https://dmsg.test".into(),
            to_be_signed_digest: digest("capacity tbs", &i),
            public_key_fingerprint: digest("capacity thumbprint", &i),
            signature_digest: digest("capacity signature", &i),
            artifact: SignedArtifact {
                cose_sign1: (0..15u64)
                    .flat_map(|n| *digest("capacity artifact", &(i, n)))
                    .chain([0; 420])
                    .collect::<Vec<u8>>()
                    .into(),
                cose_key: digest("capacity key", &i).to_vec().into(),
            },
        }),
    }
}

fn month(id: AccountId, month_utc: u32, valid_until_ms: u64) -> Month {
    Month {
        usage: ExecutionUsage {
            account_id: id,
            month_utc,
            month_revision: 1,
            business_revision: 1,
            lease_revision: 0,
            weight_policy_version: 1,
            allowed_units: 1_000,
            charged_units: 3,
            valid_until_ms,
        },
        weights: ExecutionWeights {
            version: 1,
            ed25519: 1,
        },
        entitlement_digest: digest("capacity entitlement", &(id, month_utc)),
    }
}

/// Writes `user-<accounts>.bin` images of growing size to DMSG_USER_IMAGE_DIR
/// for the canister DMSG_USER_IMAGE_CANISTER, whose ID the profile prints.
#[test]
#[ignore = "host-built stable images for user_capacity_profile"]
fn capacity_image() {
    let dir = std::path::PathBuf::from(std::env::var_os("DMSG_USER_IMAGE_DIR").expect("image dir"));
    let home = Principal::from_text(
        std::env::var("DMSG_USER_IMAGE_CANISTER").expect("canister id of the profile"),
    )
    .expect("canister id");
    let service = |n: u8| Principal::from_slice(&[n, 1]);
    let init = UserInit {
        environment: Environment::Local,
        issuer_namespace: NAMESPACE.into(),
        home_cose: service(2),
        handle_canister: service(3),
        payment_canister: service(4),
        commerce_canister: service(5),
        membership_canister: service(6),
        max_accounts: MAX_HOME_ACCOUNTS,
        daily_new_accounts: 100_000,
        principal_origin: "https://id.dmsg.test".into(),
        directory_canister: service(7),
        governance: service(8),
        admission_key: None,
    };
    let namespace_digest = xid::namespace_digest(&init.environment, NAMESPACE, home);
    let mut allocator = XidGenerator::new(namespace_digest[..5].try_into().expect("fingerprint"));
    let current = month_utc(AT).expect("month");
    let previous = month_utc(AT - 31 * DAY).expect("month");
    let mut accounts = 0;
    for target in SIZES {
        let started = std::time::Instant::now();
        while accounts < target {
            let i = accounts;
            // A million accounts a second keep each Xid counter in range.
            let (id, next) = allocator
                .allocate((AT - 30 * DAY) / SECOND + i / 1_000_000)
                .expect("account id");
            allocator = next;
            let s = account(home, init.home_cose, i, id);
            ACCOUNTS.with_borrow_mut(|t| t.put(id.as_slice(), &s));
            for login in &s.auth_bindings {
                AUTH.with_borrow_mut(|t| t.put(login.as_slice(), &id));
            }
            let snapshot = canonical(&s.snapshot(NAMESPACE));
            CERT.with_borrow_mut(|c| c.set(id.to_vec(), leaf_hash(&snapshot)));
            if i.is_multiple_of(5) {
                let e = expired_attestation(&s, i);
                let receipt = execution::receipt(&e, NAMESPACE).expect("receipt");
                EXECUTIONS.with_borrow_mut(|t| {
                    t.put(&[id.as_slice(), e.request_id.as_slice()].concat(), &e)
                });
                CERT.with_borrow_mut(|c| {
                    c.set(
                        execution_receipt_key(&id, e.request_id),
                        leaf_hash(&canonical(&receipt)),
                    )
                });
                crate::commerce::save(&month(id, previous, AT - 31 * DAY + 50 * MINUTE));
                crate::commerce::save(&month(id, current, AT + 50 * MINUTE));
            }
            accounts += 1;
        }
        save_config(&Config {
            schema: STABLE_SCHEMA,
            init: init.clone(),
            allocator: allocator.clone(),
            allocator_namespace_digest: namespace_digest,
            day: AT / DAY,
            created_today: 0,
            master_secret: Some(digest("capacity master", &home)),
            execution_months: Vec::new(),
        });
        let bytes = RAW.with(|m| {
            let image = m.borrow();
            std::fs::write(dir.join(format!("user-{accounts}.bin")), &*image).expect("image");
            image.len()
        });
        println!(
            "user_image accounts={accounts} stable_bytes={bytes} host_build_s={}",
            started.elapsed().as_secs()
        );
    }
}
