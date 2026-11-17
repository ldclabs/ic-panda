use super::*;
use crate::testing::{TempDir, account_state, unlocked};
use dmsg_types::user::Capability;

fn action(client: &Client) -> Action {
    Action::DeviceCapabilities {
        device_id: client.workspace.meta.device_id,
        capabilities: vec![Capability::RootManage, Capability::FormalApprove],
    }
}

fn mutation(client: &mut Client) -> AccountMutation {
    let state = account_state(client);
    let action = action(client);
    let (command, fixed) = command(client, &action, &state, None).unwrap();
    let now = now_ms();
    client
        .prepare_unsigned_mutation(&state, command, fixed, now + 60_000, now)
        .unwrap()
        .0
}

#[test]
fn final_account_message_has_the_original_deadline_and_no_executable_journal() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let state = account_state(&client);
    let now = now_ms();
    let original = client
        .approval_create("gui", action(&client), now + 1_000)
        .unwrap();
    let (command, fixed) = command(&mut client, &original.action, &state, None).unwrap();
    let (mutation, message, purpose) = client
        .prepare_unsigned_mutation(&state, command, fixed, original.expires_at, now)
        .unwrap();
    assert_eq!(mutation.approval.expires_at, original.expires_at);
    assert_eq!(purpose, Some(HardwarePurpose::DeviceControl));
    dmsg_protocol::verify(
        &client.workspace.meta.signing_pub,
        message.as_slice(),
        mutation.approval.signature.as_slice(),
    )
    .unwrap();
    assert!(client.pending().unwrap().is_none());
    assert!(client.approved_journal(&original.id).unwrap().is_none());
    assert_eq!(
        client.approval_record(&original.id).unwrap().state,
        ApprovalState::AwaitingUser
    );
    let bytes = canonical(&mutation);
    for dimension in 0..4 {
        let mut stale = state.clone();
        match dimension {
            0 => stale.info.account_version += 1,
            1 => stale.info.security_epoch += 1,
            2 => {
                stale
                    .security
                    .devices
                    .get_mut(&mutation.approval.device_id)
                    .unwrap()
                    .next_sequence += 1
            }
            _ => {
                stale
                    .security
                    .devices
                    .get_mut(&mutation.approval.device_id)
                    .unwrap()
                    .revoked_at = Some(now)
            }
        }
        assert!(check_state(&mutation, &stale).is_err());
    }
    assert_eq!(canonical(&mutation), bytes);
    assert_eq!(
        client
            .prepare_unsigned_mutation(&state, mutation.command.clone(), None, now, now)
            .err(),
        Some(Error::Expired)
    );
}

#[test]
fn mutation_receipts_must_match_id_digest_and_version_and_uncertainty_keeps_the_original() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let mutation = mutation(&mut client);
    let receipt = OperationReceipt {
        id: mutation.approval.request_id,
        digest: digest("dmsg/account-operation/v2", &mutation),
        account_version: mutation.expected_version + 1,
    };
    assert_eq!(
        outcome(&mutation, Ok(Ok(receipt.clone()))),
        (Stage::Confirmed, Ok(()))
    );
    for dimension in 0..3 {
        let mut wrong = receipt.clone();
        match dimension {
            0 => wrong.id = Hash::new([0; 32]),
            1 => wrong.digest = Hash::new([0; 32]),
            _ => wrong.account_version += 1,
        }
        assert_eq!(
            outcome(&mutation, Ok(Ok(wrong))),
            (Stage::Unknown, Err(Error::ExecutionUnknown))
        );
    }
    for uncertain in [
        Ok(Err(Error::Expired)),
        Ok(Err(Error::ResultExpired)),
        Err(Error::Unavailable("lost reply".into())),
    ] {
        assert_eq!(
            outcome(&mutation, uncertain),
            (Stage::Unknown, Err(Error::ExecutionUnknown))
        );
    }
    assert_eq!(
        outcome(&mutation, Ok(Err(Error::Forbidden))),
        (Stage::Rejected, Err(Error::Forbidden))
    );
}

#[test]
fn a_confirmed_approval_never_overwrites_or_inherits_a_later_account_operation() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let mut request = client
        .approval_create("gui", action(&client), now_ms() + 60_000)
        .unwrap();
    request.state = ApprovalState::Executing;
    request.started = true;
    client.save_approval(&request).unwrap();
    let original = Journal {
        home: client.account().unwrap().home,
        caller: Principal::anonymous(),
        account: Some(client.account().unwrap().id),
        request: Request::Mutate(mutation(&mut client)),
        stage: Stage::Unknown,
    };
    client
        .save_approved_journal(
            &request.id,
            &ApprovedJournal {
                digest: request.digest(),
                journal: original.clone(),
                error: None,
                followup_done: false,
            },
        )
        .unwrap();
    let later = Journal {
        request: Request::Mutate(mutation(&mut client)),
        ..original.clone()
    };
    client.save_journal(&later).unwrap();
    // A reply for another mutation cannot settle this approval.
    assert_eq!(
        client
            .settle_approved_account(&request.id, &later, Stage::Confirmed, Ok(()), now_ms())
            .err(),
        Some(Error::IntegrityFailed)
    );
    client
        .settle_approved_account(&request.id, &original, Stage::Confirmed, Ok(()), now_ms())
        .unwrap()
        .unwrap();
    assert_eq!(client.pending().unwrap(), Some(later));
    assert_eq!(
        client.approval_record(&request.id).unwrap().state,
        ApprovalState::Done
    );
    // An earlier timed-out sender returning late cannot downgrade the receipt.
    client
        .settle_approved_account(
            &request.id,
            &original,
            Stage::Unknown,
            Err(Error::ExecutionUnknown),
            now_ms(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        client
            .approved_journal(&request.id)
            .unwrap()
            .unwrap()
            .journal
            .stage,
        Stage::Confirmed
    );
    assert_eq!(
        client.approval_record(&request.id).unwrap().state,
        ApprovalState::Done
    );
}

#[test]
fn a_confirmed_revocation_still_waits_for_its_root_and_does_not_inherit_another_receipt() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let action = Action::DeviceRevoke {
        device_id: Hash::new([5; 32]),
    };
    assert!(supported(&action));
    let mut request = client
        .approval_create("gui", action, now_ms() + 60_000)
        .unwrap();
    request.state = ApprovalState::Executing;
    request.started = true;
    client.save_approval(&request).unwrap();
    let state = account_state(&client);
    let (command, fixed) = command(&mut client, &request.action, &state, None).unwrap();
    let mutation = client
        .prepare_unsigned_mutation(&state, command, fixed, request.expires_at, now_ms())
        .unwrap()
        .0;
    let journal = Journal {
        home: client.account().unwrap().home,
        caller: Principal::anonymous(),
        account: Some(client.account().unwrap().id),
        request: Request::Mutate(mutation),
        stage: Stage::Unknown,
    };
    client.save_journal(&journal).unwrap();
    client
        .save_approved_journal(
            &request.id,
            &ApprovedJournal {
                digest: request.digest(),
                journal: journal.clone(),
                error: None,
                followup_done: false,
            },
        )
        .unwrap();
    client
        .settle_approved_account(&request.id, &journal, Stage::Confirmed, Ok(()), now_ms())
        .unwrap()
        .unwrap();
    assert_eq!(
        client.approval_record(&request.id).unwrap().state,
        ApprovalState::Unknown
    );
    assert!(
        !client
            .approved_journal(&request.id)
            .unwrap()
            .unwrap()
            .followup_done
    );
    assert!(client.pending().unwrap().is_none());
}
