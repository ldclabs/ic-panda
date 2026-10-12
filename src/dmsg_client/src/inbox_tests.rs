use super::*;
use crate::vectors::{bytes, load, text, value};

#[test]
fn letters_open_with_the_synced_inbox_key() {
    let v = load("inbox-v1.json");
    let account = text(&v["subject_id"]);
    let version = v["key_version"].as_u64().unwrap();
    assert_eq!(
        canonical(&("dmsg/inbox-key/1", account, version)),
        bytes(&v["key_object_id_input"]["hex"])
    );
    assert_eq!(key_object(account, version), text(&v["key_object_id"]));
    let key: InboxKey = decode(&canonical(&value(&v["key_payload"])), MAX_RECORD).unwrap();
    assert_eq!((key.version, key.root_generation), (1, 1));
    let seed: [u8; 32] = unb64(&key.seed).unwrap().try_into().unwrap();
    assert_eq!(hpke_public(&seed).to_vec(), bytes(&v["public_key_hex"]));
    let sealed = unb64(text(&v["envelope"])).unwrap();
    assert_eq!(sealed, bytes(&v["envelope_hex"]));
    let envelope: HpkeEnvelope = decode(&sealed, MAX_RECORD).unwrap();
    let context = letter_context(
        text(&v["sender"]),
        text(&v["recipient"]),
        text(&v["order_id"]),
        version,
    );
    assert_eq!(canonical(&context), bytes(&v["context"]["hex"]));
    let plain = hpke_open(&seed, &envelope, &context).unwrap();
    assert_eq!(*plain, bytes(&v["plaintext_hex"]));
    let body: LetterBody = decode(&plain, 16_384).unwrap();
    assert_eq!(canonical(&body), *plain);
}
