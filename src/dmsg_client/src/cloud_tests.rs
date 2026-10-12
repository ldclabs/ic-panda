use super::*;
use crate::vectors::{bytes, load, text};
use dmsg_types::Hash;
use std::collections::BTreeMap;

#[test]
fn reproduces_commands_and_http_proofs() {
    let v = load("cloud-v1.json");
    let c = &v["context"];
    // Seed 7 is the fixture's test-only device key.
    let seed = [7u8; 32];
    let signer = Signer {
        issuer: text(&c["issuer"]),
        device_id: Hash::new(bytes(&c["deviceId"]).try_into().unwrap()),
        security_epoch: c["securityEpoch"].as_u64().unwrap(),
        seed: &seed,
        expires_at: u64::MAX,
    };
    let request_id = text(&c["requestId"]);
    let deadline = c["deadline"].as_u64().unwrap();
    let profile: BTreeMap<String, Json> = serde_json::from_value(v["profile"].clone()).unwrap();
    let command = sign_command(&signer, "dmsg/profile/v1", &profile, request_id, deadline);
    assert_eq!(command, text(&v["command"]["cose_sign1"]));

    let body = canonical(&BTreeMap::from([("cose_sign1", command.as_str())]));
    assert_eq!(body, bytes(&v["body_hex"]));
    assert_eq!(hex::encode(*sha256(&body)), text(&v["body_digest"]));
    let url = Url::parse(text(&v["url"])).unwrap();
    let pop = sign_http(&signer, &url, "POST", &body, request_id, deadline).unwrap();
    assert_eq!(pop, text(&v["http_pop"]));
}

#[test]
fn deadlines_stay_within_the_evidence() {
    let seed = [7u8; 32];
    let signer = Signer {
        issuer: "https://dmsg.test/u/040g2081040g2081040g",
        device_id: Hash::new([7; 32]),
        security_epoch: 1,
        seed: &seed,
        expires_at: 1_000_030_000,
    };
    assert_eq!(signer.deadline(1_000_000_000).unwrap(), 1_000_030_000);
    assert_eq!(signer.deadline(1_000_030_000), Err(Error::PolicyStale));
    let fresh = Signer {
        expires_at: u64::MAX,
        ..signer
    };
    assert_eq!(fresh.deadline(5).unwrap(), 45_005);
}

#[test]
fn targets_sort_queries_and_refuse_ambiguity() {
    let target = |u: &str| canonical_target(&Url::parse(u).unwrap());
    assert_eq!(
        target("https://a.test/v1/x?limit=100&after=0").unwrap(),
        "/v1/x?after=0&limit=100"
    );
    assert_eq!(target("https://a.test/v1/x").unwrap(), "/v1/x");
    assert!(target("https://a.test/v1/x?a=1&a=2").is_err());
    assert!(target("https://a.test/v1/a%2Fb").is_err());
    assert!(target("https://a.test/v1/x#f").is_err());
}

#[test]
fn relay_origins_are_https_or_local_loopback() {
    assert!(Cloud::new("https://api.dmsg.test", false).is_ok());
    assert!(Cloud::new("http://127.0.0.1:8787", true).is_ok());
    assert!(Cloud::new("http://127.0.0.1:8787", false).is_err());
    assert!(Cloud::new("http://api.dmsg.test", true).is_err());
    assert!(Cloud::new("https://api.dmsg.test/base", false).is_err());
}
