use super::*;
use crate::crypto::hpke_public;
use crate::vectors::{bytes, key, load, text, value};
use serde_json::Value as Json;

fn hash(v: &Json) -> Hash {
    Hash::new(key(v))
}

#[test]
fn initial_setup_reuses_only_a_live_matching_reservation() {
    let mut job = RootJob {
        op_id: Hash::new([51; 32]),
        expected_generation: 0,
        context: None,
        candidate: None,
        bytes: None,
        plan: None,
    };
    assert!(job.reservation_current(None, 1, 100));
    let mut slot = RootReservation {
        op_id: job.op_id,
        expected_generation: 0,
        generation: 2,
        security_epoch: 1,
        expires_at: 200,
    };
    job.context = Some(RootContext {
        account: crate::testing::account().id.to_string(),
        environment: "local".into(),
        generation: slot.generation,
        op_id: hex::encode(*job.op_id),
        security_epoch: 1,
    });
    // A retry retains the prepared job before expiry, but cannot use it
    // at the deadline, after a security change, or after another reservation.
    assert!(job.reservation_current(Some(&slot), 1, 199));
    assert!(!job.reservation_current(Some(&slot), 1, 200));
    assert!(!job.reservation_current(Some(&slot), 2, 100));
    assert!(!job.reservation_current(None, 1, 100));
    slot.op_id = Hash::new([52; 32]);
    assert!(!job.reservation_current(Some(&slot), 1, 100));
    slot.op_id = job.op_id;
    slot.generation += 1;
    assert!(!job.reservation_current(Some(&slot), 1, 100));
}

// A saved job from a release that used the full 24-hour upload lifetime.
fn old_initial_job() -> RootJob {
    let bytes = ByteBuf::from(b"sealed initial root bundle".to_vec());
    RootJob {
        op_id: Hash::new([51; 32]),
        expected_generation: 0,
        context: Some(RootContext {
            account: crate::testing::account().id.to_string(),
            environment: "local".into(),
            generation: 1,
            op_id: "33".repeat(32),
            security_epoch: 0,
        }),
        candidate: Some(ByteArray::new([52; 32])),
        plan: Some(UploadPlan {
            management: None,
            upload_id: "35".repeat(32),
            object_id: "36".repeat(32),
            version_id: "37".repeat(32),
            root_generation: 1,
            epoch: 0,
            control_head: None,
            kind: "root".into(),
            chunks: vec![ChunkRef {
                digest: "38".repeat(32),
                size: 1,
            }],
            manifest_digest: hex::encode(*sha256(&bytes)),
            manifest_size: bytes.len() as u64,
            expires_at: now_ms() + dmsg_types::DAY + 5_000,
        }),
        bytes: Some(bytes),
    }
}

struct RootUploadRelay {
    client: std::sync::Mutex<Client>,
    reads: std::sync::Mutex<std::collections::VecDeque<Result<Option<Json>>>>,
    cancel: Result<Json>,
    cancellations: std::sync::atomic::AtomicUsize,
}

impl Hold for RootUploadRelay {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        step(&mut self.client.lock().unwrap())
    }

    async fn find(&self, path: &str) -> Result<Option<Json>> {
        assert!(path.ends_with(&format!("/uploads/{}", "35".repeat(32))));
        self.reads
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected read")
    }

    async fn post<P: Serialize + Sync>(
        &self,
        path: &str,
        action: &str,
        payload: &P,
        _: &Hash,
    ) -> Result<Json> {
        assert!(path.ends_with("/uploads/cancel"));
        assert_eq!(action, "dmsg/upload/cancel/v1");
        assert_eq!(
            serde_json::to_value(payload).unwrap()["upload_id"],
            "35".repeat(32)
        );
        self.cancellations
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.cancel.clone()
    }
}

fn root_relay(
    dir: &std::path::Path,
    job: &RootJob,
    reads: Vec<Result<Option<Json>>>,
    cancel: Result<Json>,
) -> RootUploadRelay {
    let mut client = crate::testing::unlocked(dir, false, None);
    client.root = None;
    client
        .workspace
        .put_private(client.keys().unwrap(), ROOT_JOB, job)
        .unwrap();
    RootUploadRelay {
        client: std::sync::Mutex::new(client),
        reads: std::sync::Mutex::new(reads.into()),
        cancel,
        cancellations: 0.into(),
    }
}

fn upload_status(job: &RootJob, status: &str) -> Result<Option<Json>> {
    Ok(Some(serde_json::json!({"status":status, "plan":job.plan})))
}

#[tokio::test]
async fn initial_root_retry_renews_only_the_uncommitted_upload_and_persists_it() {
    for staged in [false, true] {
        let dir = crate::testing::TempDir::new();
        let mut job = old_initial_job();
        let before = serde_json::to_value(&job).unwrap();
        let reads = if staged {
            vec![
                upload_status(&job, "staging"),
                upload_status(&job, "aborted"),
            ]
        } else {
            vec![Ok(None), Ok(None)]
        };
        let relay = root_relay(
            &dir.0,
            &job,
            reads,
            Ok(serde_json::json!({"status":"aborted"})),
        );
        let started = now_ms();
        job.renew_upload(&relay).await.unwrap();
        let plan = job.plan.as_ref().unwrap();
        assert_ne!(plan.upload_id, before["plan"]["upload_id"]);
        assert!(plan.expires_at >= crate::upload::expires_at(started));
        assert!(plan.expires_at <= crate::upload::expires_at(now_ms()));
        let mut after = serde_json::to_value(&job).unwrap();
        after["plan"]["upload_id"] = before["plan"]["upload_id"].clone();
        after["plan"]["expires_at"] = before["plan"]["expires_at"].clone();
        // Neither a second root key nor a second IC reservation is generated.
        assert_eq!(after, before);
        let client = relay.client.lock().unwrap();
        let saved: RootJob = client
            .workspace
            .private(client.keys().unwrap(), ROOT_JOB)
            .unwrap()
            .unwrap();
        assert_eq!(canonical(&saved), canonical(&job));
        assert!(client.root.is_none());
        assert_eq!(
            relay
                .cancellations
                .load(std::sync::atomic::Ordering::SeqCst),
            usize::from(staged)
        );
    }
}

#[tokio::test]
async fn initial_root_retry_keeps_a_committed_upload_even_if_its_plan_is_old() {
    for race in [false, true] {
        let dir = crate::testing::TempDir::new();
        let mut job = old_initial_job();
        job.plan.as_mut().unwrap().expires_at = 1;
        let before = canonical(&job);
        let reads = vec![
            upload_status(&job, if race { "staging" } else { "committed" }),
            upload_status(&job, "committed"),
        ];
        let relay = root_relay(&dir.0, &job, reads, Err(Error::VersionConflict));
        job.renew_upload(&relay).await.unwrap();
        assert_eq!(canonical(&job), before);
        assert_eq!(
            relay
                .cancellations
                .load(std::sync::atomic::Ordering::SeqCst),
            usize::from(race)
        );
    }
}

#[tokio::test]
async fn initial_root_retry_preserves_the_job_when_reconciliation_is_unsettled() {
    for case in 0..4 {
        let dir = crate::testing::TempDir::new();
        let mut job = old_initial_job();
        let before = canonical(&job);
        let (reads, cancel, error) = match case {
            0 => (
                vec![Err(Error::Unavailable("offline".into()))],
                Ok(Json::Null),
                Error::Unavailable("offline".into()),
            ),
            1 => (
                vec![upload_status(&job, "staging")],
                Err(Error::ExecutionUnknown),
                Error::ExecutionUnknown,
            ),
            2 => (
                vec![Ok(None), Err(Error::Unavailable("offline".into()))],
                Ok(Json::Null),
                Error::Unavailable("offline".into()),
            ),
            _ => (
                vec![
                    upload_status(&job, "staging"),
                    upload_status(&job, "staging"),
                ],
                Ok(Json::Null),
                Error::ExecutionUnknown,
            ),
        };
        let relay = root_relay(&dir.0, &job, reads, cancel);
        assert_eq!(job.renew_upload(&relay).await, Err(error));
        assert_eq!(canonical(&job), before);
        let client = relay.client.lock().unwrap();
        let saved: RootJob = client
            .workspace
            .private(client.keys().unwrap(), ROOT_JOB)
            .unwrap()
            .unwrap();
        assert_eq!(canonical(&saved), before);
    }
}

#[test]
fn opens_exported_bundles() {
    let v = load("root-v2.json");
    let account = text(&v["account"]);
    let devices = v["devices"].as_array().unwrap();
    let generations = v["generations"].as_array().unwrap();
    let mut chain: Vec<Vec<u8>> = vec![];
    for case in generations {
        let bundle_bytes = bytes(&case["bundle_hex"]);
        let bundle = parse_bundle(&bundle_bytes).unwrap();
        assert_eq!(canonical(&bundle.body), canonical(&value(&case["body"])));
        assert_eq!(
            hex::encode(*sha256(&bundle_bytes)),
            text(&case["bundle_sha256"])
        );
        let digests = bundle_digests(&bundle).unwrap();
        assert_eq!(digests.recipients, hash(&case["recipients_digest"]));
        assert_eq!(digests.body, hash(&case["body_digest"]));
        assert_eq!(digests.bundle, hash(&case["bundle_digest"]));
        // A pending v2 job from before upgrade still commits its original suite.
        let reference = root_reference(&bundle).unwrap();
        assert_eq!(reference.suite, "dmsg-root-v2");
        assert_eq!(reference.recovery_enabled, None);
        assert_eq!(reference.bundle_digest, digests.bundle);
        assert_eq!(
            commitment(&key(&case["root_hex"])),
            text(&case["commitment"])
        );
        chain.insert(0, bundle_bytes);
        let generation = case["generation"].as_u64().unwrap();
        for device in devices {
            let (root, history) = open_root(
                &chain,
                &digests.bundle,
                (account, "local", generation),
                &hash(&device["device_id"]),
                &key(&device["hpke_seed_hex"]),
            )
            .unwrap();
            assert_eq!(*root, key(&case["root_hex"]));
            for earlier in &generations[..generations.iter().position(|g| g == case).unwrap()] {
                let g = earlier["generation"].as_u64().unwrap();
                assert_eq!(*history[&g], key(&earlier["root_hex"]));
            }
        }
    }
    // A bundle opens only for its recipients, generation and committed digest.
    let latest = generations.last().unwrap();
    let digest = hash(&latest["bundle_digest"]);
    let device = &devices[0];
    let open = |chain: &[Vec<u8>], digest: &Hash, generation: u64, id: Hash| {
        open_root(
            chain,
            digest,
            (account, "local", generation),
            &id,
            &key(&device["hpke_seed_hex"]),
        )
        .err()
    };
    assert_eq!(
        open(&chain, &digest, 2, Hash::new([1; 32])),
        Some(Error::DeviceNotApproved)
    );
    let id = hash(&device["device_id"]);
    assert_eq!(open(&chain, &digest, 1, id), Some(Error::IntegrityFailed));
    assert_eq!(
        open(&chain, &Hash::new([2; 32]), 2, id),
        Some(Error::IntegrityFailed)
    );
    assert_eq!(
        open(&chain[..1], &digest, 2, id),
        Some(Error::RecoveryIncomplete)
    );
    let mut tampered = chain.clone();
    let last = tampered[1].len() - 1;
    tampered[1][last] ^= 1;
    assert_eq!(
        open(&tampered, &digest, 2, id),
        Some(Error::IntegrityFailed)
    );
}

#[test]
fn wraps_bundles_that_open_with_their_chain() {
    let v = load("root-v2.json");
    let account = text(&v["account"]);
    let recovery: RecoveryKey = serde_json::from_value(v["recovery_key"].clone()).unwrap();
    let signer = (
        hash(&v["signer"]["device_id"]),
        key(&v["signer"]["seed_hex"]),
    );
    let seeds: Vec<(Hash, Key)> = v["devices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| (hash(&d["device_id"]), key(&d["hpke_seed_hex"])))
        .collect();
    let recipients: Vec<Recipient> = seeds
        .iter()
        .map(|(device, seed)| Recipient {
            device: *device,
            hpke_pub: Hash::new(hpke_public(seed)),
        })
        .collect();
    let context = |generation| RootContext {
        account: account.into(),
        environment: "local".into(),
        generation,
        op_id: "51".repeat(32),
        security_epoch: 3,
    };
    let first_root = [41u8; 32];
    let first = wrap_root(
        &context(1),
        &first_root,
        &recipients,
        Some(&recovery),
        (&signer.0, &signer.1),
        None,
    )
    .unwrap();
    let first_digest = bundle_digests(&parse_bundle(&first).unwrap()).unwrap();
    let sha = hex::encode(*sha256(&first));
    let upload = "61".repeat(32);
    let second_root = [42u8; 32];
    let second = wrap_root(
        &context(2),
        &second_root,
        &recipients,
        Some(&recovery),
        (&signer.0, &signer.1),
        Some(PreviousLink {
            digest: &sha,
            upload_id: &upload,
            generation: 1,
            root: &first_root,
        }),
    )
    .unwrap();
    let bundle = parse_bundle(&second).unwrap();
    assert!(
        bundle
            .body
            .envelopes
            .windows(2)
            .all(|w| w[0].device < w[1].device)
    );
    let digests = bundle_digests(&bundle).unwrap();
    for (device, seed) in &seeds {
        let (root, history) = open_root(
            &[second.clone(), first.clone()],
            &digests.bundle,
            (account, "local", 2),
            device,
            seed,
        )
        .unwrap();
        assert_eq!(*root, second_root);
        assert_eq!(*history[&1], first_root);
        let (root, _) = open_root(
            std::slice::from_ref(&first),
            &first_digest.bundle,
            (account, "local", 1),
            device,
            seed,
        )
        .unwrap();
        assert_eq!(*root, first_root);
    }
    // The signer must be a recipient.
    assert!(
        wrap_root(
            &context(1),
            &first_root,
            &recipients,
            Some(&recovery),
            (&Hash::new([3; 32]), &signer.1),
            None
        )
        .is_err()
    );
}

/// A vetKD derivation the PocketIC control-plane test produced for account
/// `c2a41dj7n47abbo00000`, generation 1, to the transport seed `[91; 32]`.
const RECOVERY_ACCOUNT: &str = "c2a41dj7n47abbo00000";
const RECOVERY_PUBLIC: &str = "a17778d816ab4617e53bcdfa47e806945ee427e61c00c64523f1e529d8214b0337f2ac37c53d6bda54df34047b2ffe8e12e75302bc3e704484ebccf3540afec3f42ef3772c8f1c6e6cf82df566d59f4f2232ea4ad35c4a6762417c51b6969ab9";
const RECOVERY_ENCRYPTED: &str = "adac5deb3bd35cf015cab21c69f56a475ebbba453ef2b0b3d824274dc6f27567f6df4f8491c400c8ebc560494db808178d0c99a522acac1afb312264b38f040952f7af852e2f538c33b506a4412fd0fc2fe8cdb4ad0fd548d0e38549193a3cb518a7b7fa83a3fa8979763f94aebf658b34fe4533ede0b09be50b1c7637ba6e867c9bb3591b57275915e86520af21a5e9af5405036e7924ee991eb6721dbe931b8ede3b278f27bfba1c100efe32e234262c97cf1570aa19631c04ffcdeb9e2f94";

#[test]
fn a_derived_key_opens_the_recovery_envelope_and_its_chain() {
    let public = hex::decode(RECOVERY_PUBLIC).unwrap();
    let recovery = RecoveryKey {
        home_cose: "aaaaa-aa".into(),
        key_name: "dfx_test_key".into(),
        public_key: b64(&public),
    };
    let signer = (Hash::new([1; 32]), [2u8; 32]);
    let recipients = [Recipient {
        device: signer.0,
        hpke_pub: Hash::new(hpke_public(&[3; 32])),
    }];
    let context = |generation| RootContext {
        account: RECOVERY_ACCOUNT.into(),
        environment: "local".into(),
        generation,
        op_id: "51".repeat(32),
        security_epoch: 1,
    };
    let earlier = wrap_root(
        &context(1),
        &[40; 32],
        &recipients,
        Some(&recovery),
        (&signer.0, &signer.1),
        None,
    )
    .unwrap();
    let digest = bundle_digests(&parse_bundle(&earlier).unwrap()).unwrap();
    let transport = ic_vetkeys::TransportSecretKey::from_seed(vec![91; 32]).unwrap();
    let vetkey =
        ic_vetkeys::EncryptedVetKey::deserialize(&hex::decode(RECOVERY_ENCRYPTED).unwrap())
            .unwrap()
            .decrypt_and_verify(
                &transport,
                &DerivedPublicKey::deserialize(&public).unwrap(),
                &recovery_identity(&RECOVERY_ACCOUNT.parse().unwrap(), 1),
            )
            .unwrap();
    let (root, history) = open_recovered(
        std::slice::from_ref(&earlier),
        &digest.bundle,
        (RECOVERY_ACCOUNT, "local", 1),
        &vetkey,
    )
    .unwrap();
    assert_eq!(*root, [40; 32]);
    assert!(history.is_empty());
    // The key of another generation's identity does not verify.
    assert!(
        ic_vetkeys::EncryptedVetKey::deserialize(&hex::decode(RECOVERY_ENCRYPTED).unwrap())
            .unwrap()
            .decrypt_and_verify(
                &transport,
                &DerivedPublicKey::deserialize(&public).unwrap(),
                &recovery_identity(&RECOVERY_ACCOUNT.parse().unwrap(), 2),
            )
            .is_err()
    );
    // Nor does the envelope open for another committed digest.
    assert_eq!(
        open_recovered(
            std::slice::from_ref(&earlier),
            &Hash::new([9; 32]),
            (RECOVERY_ACCOUNT, "local", 1),
            &vetkey,
        )
        .err(),
        Some(Error::IntegrityFailed)
    );
}

#[test]
fn a_root_without_recovery_opens_only_for_its_device() {
    let v = load("root-v2.json");
    let signer = (
        hash(&v["signer"]["device_id"]),
        key(&v["signer"]["seed_hex"]),
    );
    let seed = key(&v["devices"][0]["hpke_seed_hex"]);
    let device = hash(&v["devices"][0]["device_id"]);
    let mut recipients = vec![Recipient {
        device,
        hpke_pub: Hash::new(hpke_public(&seed)),
    }];
    if signer.0 != device {
        recipients.push(Recipient {
            device: signer.0,
            hpke_pub: Hash::new(hpke_public(&[99; 32])),
        });
    }
    let context = RootContext {
        account: text(&v["account"]).into(),
        environment: "local".into(),
        generation: 1,
        op_id: "75".repeat(32),
        security_epoch: 0,
    };
    let bytes = wrap_root(
        &context,
        &[76; 32],
        &recipients,
        None,
        (&signer.0, &signer.1),
        None,
    )
    .unwrap();
    let bundle = parse_bundle(&bytes).unwrap();
    assert!(bundle.body.recovery.is_none());
    assert!(bundle.body.recovery_key.is_none());
    assert!(
        !bytes
            .windows(b"recovery".len())
            .any(|part| part == b"recovery")
    );
    let digest = bundle_digests(&bundle).unwrap();
    let (key, _) = open_root(
        std::slice::from_ref(&bytes),
        &digest.bundle,
        (&context.account, "local", 1),
        &device,
        &seed,
    )
    .unwrap();
    assert_eq!(*key, [76; 32]);
    assert!(
        open_root(
            &[bytes],
            &digest.bundle,
            (&context.account, "local", 1),
            &device,
            &[77; 32]
        )
        .is_err()
    );
    // The generation and recovery recipient are part of the chain commitment.
    let ids = recipients.iter().map(|r| r.device).collect::<Vec<_>>();
    assert_ne!(digest.recipients, root_recipients_digest_v2(&ids, 1, true));
    let mut malformed = bundle.body;
    malformed.format = "dmsg-root-bundle/2".into();
    assert_eq!(check_body(&malformed), Err(Error::IntegrityFailed));
}
