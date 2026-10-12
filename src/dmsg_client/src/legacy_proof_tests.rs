use super::*;
use crate::vectors::load;

#[test]
fn reproduces_the_shared_inheritance_digests() {
    let vectors = load("legacy-v1.json");
    let shared = &vectors["shared"];
    let proposal: SharedProposal = serde_json::from_value(shared["proposal"].clone()).unwrap();
    assert_eq!(
        proposal.digest(),
        shared["proposal_digest"].as_str().unwrap()
    );
    let manager = shared["source"]["managers"][0].as_str().unwrap();
    assert_eq!(
        hex::encode(proposal.manager_consent(manager)),
        shared["manager_consent_hex"].as_str().unwrap()
    );
    let claim: MemberClaim = serde_json::from_value(shared["claim"].clone()).unwrap();
    assert_eq!(
        hex::encode(claim.digest()),
        shared["claim_digest_hex"].as_str().unwrap()
    );
    assert_eq!(
        source_key(shared["source"]["source"].as_str().unwrap(), 1),
        shared["source_key"].as_str().unwrap()
    );
    let input = &shared["source_digest"]["input"];
    let cutover: [u8; 32] = hex::decode(input["cutover_hex"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let names: Vec<(String, Vec<u8>)> = input["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            (
                n[0].as_str().unwrap().to_string(),
                hex::decode(n[1].as_str().unwrap()).unwrap(),
            )
        })
        .collect();
    assert_eq!(
        source_digest(
            (
                input["source"].as_str().unwrap(),
                input["channel"].as_u64().unwrap() as u32,
                input["epoch"].as_str().unwrap().parse().unwrap(),
            ),
            &cutover,
            &hex::decode(input["authority_hex"].as_str().unwrap()).unwrap(),
            (
                input["identity"].as_str().unwrap(),
                input["identity_epoch"].as_str().unwrap().parse().unwrap(),
            ),
            &names,
        ),
        shared["source_digest"]["digest"].as_str().unwrap()
    );
    let source: SharedSource = serde_json::from_value(shared["source"].clone()).unwrap();
    assert!(source.controls(manager, manager));
    assert!(!source.controls(manager, "2vxsx-fae"));
}

#[test]
fn hashes_ingress_requests_independently_of_representation() {
    // ic-agent's request ID of the same call.
    let canister = Principal::from_text("nvdn4-5qaaa-aaaaj-qa4pq-cai").unwrap();
    let sender = Principal::anonymous();
    let arg = candid::encode_args((SnapshotScope::Messages(1), None::<String>)).unwrap();
    let ours = request_id(
        &canister,
        &sender,
        "legacy_snapshot",
        &sha256(&arg),
        1_760_000_000_000_000_000,
        &[7; 8],
    );
    let theirs = ic_agent::agent::EnvelopeContent::Call {
        nonce: Some(vec![7; 8]),
        ingress_expiry: 1_760_000_000_000_000_000,
        sender,
        canister_id: canister,
        method_name: "legacy_snapshot".into(),
        arg,
        sender_info: None,
    }
    .to_request_id();
    assert_eq!(ours, *theirs);
}
