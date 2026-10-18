use super::*;
use crate::approval::Action;
use crate::codec::{canonical, decode};

#[test]
fn changes_survive_the_approval_record() {
    let login = Principal::from_slice(&[7; 29]);
    let changes = [
        AccountChange::BindLogin {
            principal: login.to_text(),
            nonce: Hash::new([1; 32]),
        },
        AccountChange::RemoveLogin {
            principal: login.to_text(),
        },
        AccountChange::Policy {
            policy: SensitivePolicy::default(),
            previous: Some(SensitivePolicy::default()),
        },
        AccountChange::RecoveryDelay { delay_ms: 2 * DAY },
        AccountChange::DisputeRecovery {
            op_id: Hash::new([2; 32]),
        },
    ];
    for change in changes {
        let action = Action::Account {
            change: change.clone(),
        };
        let back: Action = decode(&canonical(&action), 4096).unwrap();
        assert_eq!(back, action);
        change.command().unwrap();
    }
    assert_eq!(
        AccountChange::BindLogin {
            principal: login.to_text(),
            nonce: Hash::new([1; 32]),
        }
        .command()
        .unwrap(),
        AccountCommand::BindAuth {
            principal: login,
            nonce: Hash::new([1; 32]),
        }
    );
    assert!(
        AccountChange::RemoveLogin {
            principal: "not a principal".into()
        }
        .command()
        .is_err()
    );
}
