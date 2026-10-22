use super::*;
use dmsg_types::{Environment, handle::HandleInit};
use icrc_ledger_types::icrc1::account::Account;

#[test]
fn document_pending_reads_the_journal_and_excludes_other_account_operations() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let account = client.account().unwrap().clone();
    assert!(!client.document_pending().unwrap());
    let mutation = AccountMutation {
        account_id: account.id,
        expected_version: 1,
        command: AccountCommand::UpdatePrincipal {
            document: b"{}".to_vec().into(),
        },
        approval: Approval {
            device_id: client.workspace.meta.device_id,
            security_epoch: 1,
            sequence: 0,
            request_id: Hash::new([1; 32]),
            expires_at: 1,
            signature: Ed25519Signature::new([0; 64]),
        },
    };
    let mut journal = Journal {
        home: account.home,
        caller: Principal::anonymous(),
        account: Some(account.id),
        request: Request::Mutate(mutation.clone()),
        stage: Stage::Prepared,
    };
    for (stage, pending) in [
        (Stage::Prepared, true),
        (Stage::Unknown, true),
        (Stage::Confirmed, false),
        (Stage::Rejected, false),
    ] {
        journal.stage = stage;
        client.save_journal(&journal).unwrap();
        assert_eq!(client.document_pending().unwrap(), pending);
    }
    journal.stage = Stage::Unknown;
    journal.request = Request::Mutate(AccountMutation {
        command: AccountCommand::EnablePrincipal {
            principal_type: dmsg_types::agent::PrincipalType::Person,
        },
        ..mutation
    });
    client.save_journal(&journal).unwrap();
    assert!(client.pending().unwrap().is_some());
    assert!(!client.document_pending().unwrap());
    client.lock();
    assert_eq!(client.document_pending(), Err(Error::Locked));
}

/// `get_handle_config` from a registry before the revenue account (schema
/// 9), as the mainnet handle answered on 2026-10-17.
const SCHEMA_9: &str = "4449444c036c08fc80e50d7995b487de0871a9cbadc30968d397c0ea0a01f0e2cb820b7dee9e89f00b02a2d2bea80c68c0ead8890e026b03cbf2f8607f9994c6bb027fdbcefad80f7f6d680100e80300001168747470733a2f2f646d73672e6e65742f010a00000000020000a7010101904e01010a000000000130926b0101010a00000000020000a6010101010a000000000130926b0101";

#[test]
fn handle_config_reads_registries_with_or_without_other_fields() {
    let old: HandleConfig = candid::decode_one(&hex::decode(SCHEMA_9).unwrap()).unwrap();
    assert_eq!(old.ledger_fee, 10_000);
    assert_eq!(old.registration_homes, old.user_homes);

    let current = HandleInit {
        environment: Environment::Production,
        issuer_namespace: "https://dmsg.net/".into(),
        user_homes: old.user_homes.clone(),
        registration_homes: old.registration_homes.clone(),
        ledger: old.ledger,
        ledger_fee: old.ledger_fee,
        revenue: Account::from(old.ledger),
        max_pending: 1_000,
        governance: old.ledger,
    };
    let read: HandleConfig = candid::decode_one(&candid::encode_one(&current).unwrap()).unwrap();
    assert_eq!(
        (read.user_homes, read.ledger, read.ledger_fee),
        (old.user_homes, old.ledger, old.ledger_fee)
    );
}
