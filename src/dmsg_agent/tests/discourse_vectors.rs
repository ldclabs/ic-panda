//! Agent Discourse vectors: the record chain, redaction and heads the
//! connector verifies before it projects a room.

use agent_protocols::discourse::*;
use serde_json::Value;

fn vectors() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/vectors/agent-protocols/agent-discourse-1.0.vectors.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn records_chain_and_redactions_verify() {
    let vectors = vectors();
    let records: Vec<ServerRecord> = serde_json::from_value(vectors["records"].clone()).unwrap();
    for record in &records {
        verify_server_record(record).unwrap();
        validate_discourse_envelope(&record.envelope).unwrap();
    }
    verify_server_record_chain(&records).unwrap();
    let last = records.last().unwrap();
    assert_eq!(last.seq, vectors["last_seq"].as_u64().unwrap());
    assert_eq!(last.hash, vectors["last_hash"].as_str().unwrap());
    let redacted: Vec<ArchiveRecord> =
        serde_json::from_value(vectors["redacted_records"].clone()).unwrap();
    assert!(redacted.iter().any(ArchiveRecord::is_redacted));
    verify_archive_chain(&redacted).unwrap();
    // A record whose stored hash was altered fails.
    let mut forged = records[1].clone();
    forged.accepted_at += 1;
    assert!(verify_server_record(&forged).is_err());
}
