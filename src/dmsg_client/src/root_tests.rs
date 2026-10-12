use super::*;
use crate::crypto::hpke_public;
use crate::vectors::{bytes, key, load, text, value};
use serde_json::Value as Json;

fn hash(v: &Json) -> Hash {
    Hash::new(key(v))
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
        &recovery,
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
        &recovery,
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
    assert!(bundle
        .body
        .envelopes
        .windows(2)
        .all(|w| w[0].device < w[1].device));
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
    assert!(wrap_root(
        &context(1),
        &first_root,
        &recipients,
        &recovery,
        (&Hash::new([3; 32]), &signer.1),
        None
    )
    .is_err());
}
